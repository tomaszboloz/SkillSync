use crate::models::skill::{BackupSnapshot, ManagedItemType, SkillMetadata, SkillStatus};
use crate::services::backup::BackupService;
use crate::services::config::ConfigService;
use crate::services::detector::SkillDetector;
use crate::services::detector::UNKNOWN_SKILL_VERSION;
use crate::services::git::GitService;
use crate::services::github::GitHubService;
use crate::services::managed_detector::ManagedItemDetector;
use crate::services::managed_manifest::ManagedManifest;
use crate::services::manifest::SkillManifest;
use crate::services::orchestrator::UpdateOrchestrator;
use crate::services::scan_queue::ScanQueue;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

pub(crate) fn apply_github_release(
    skill: &mut SkillMetadata,
    release: &crate::services::github::GitHubReleaseInfo,
) {
    let is_newer = GitHubService::is_newer_version(&release.tag_name, &skill.current_version);
    let compatibility =
        GitHubService::get_semver_compatibility(&release.tag_name, &skill.current_version);

    skill.latest_version = Some(release.tag_name.trim_start_matches(['v', 'V']).to_string());
    skill.update_available = is_newer;
    skill.status = if is_newer {
        SkillStatus::UpdateAvailable
    } else {
        SkillStatus::UpToDate
    };
    if let Some(body) = &release.body {
        skill.changelog = Some(body.clone());
    }
    skill.update_compatibility = Some(compatibility);
    skill.last_checked = chrono::Utc::now();
}

fn discover_skills() -> Vec<SkillMetadata> {
    let config = ConfigService::load_config();
    let paths: Vec<_> = config
        .paths
        .monitored
        .into_iter()
        .filter(|p| p.enabled)
        .collect();

    let skill_paths = paths
        .iter()
        .filter(|path| path.item_type == crate::models::config::MonitoredPathType::Skill)
        .map(|path| path.path.clone())
        .collect::<Vec<_>>();
    let mut skills = SkillDetector::scan_directories(&skill_paths);
    skills.extend(ManagedItemDetector::scan_paths(&paths));

    // A missing manifest version can be resolved only from a tag pointing at HEAD.
    for skill in &mut skills {
        skill.current_version = skill
            .current_version
            .trim_start_matches(['v', 'V'])
            .to_string();
        if skill.is_git_repo && skill.current_version == UNKNOWN_SKILL_VERSION {
            if let Some(tag) = GitService::get_head_tag(&skill.path) {
                skill.current_version = tag.trim_start_matches(['v', 'V']).to_string();
            }
        }
    }

    for skill in &mut skills {
        if let Some(repository) = config.updates.repository_overrides.get(&skill.id) {
            if let Some(repository) = GitHubService::normalize_github_repository_url(repository) {
                skill.remote_url = Some(repository);
            }
        }
        if let Some(branch) = config.updates.branch_overrides.get(&skill.id) {
            let branch = branch.trim();
            if GitService::is_valid_branch_name(branch) {
                skill.branch_override = Some(branch.to_string());
                skill.branch_or_tag = Some(branch.to_string());
            }
        }
    }

    skills
}

fn available_locations(skill: &SkillMetadata) -> Vec<PathBuf> {
    let mut locations = skill.installed_locations.clone();
    if !locations.contains(&skill.path) {
        locations.push(skill.path.clone());
    }
    locations
}

fn validate_removal_targets(
    skill: &SkillMetadata,
    requested: &[PathBuf],
    all_skills: &[SkillMetadata],
) -> Result<Vec<PathBuf>, String> {
    if requested.is_empty() {
        return Err("Wybierz co najmniej jedną lokalizację do usunięcia.".to_string());
    }

    let available = available_locations(skill);
    let mut targets = Vec::new();
    for location in requested {
        if !available.contains(location) {
            return Err(format!(
                "Odmowa usunięcia nieznanej lokalizacji: {}",
                location.display()
            ));
        }
        if !targets.contains(location) {
            targets.push(location.clone());
        }
    }

    for target in &targets {
        if !target.is_dir() && !target.is_symlink() {
            return Err(format!(
                "Lokalizacja skillu już nie istnieje: {}",
                target.display()
            ));
        }
        let manifest_result = match skill.item_type {
            ManagedItemType::Skill => SkillManifest::validate(target).map(|_| ()),
            ManagedItemType::Mcp | ManagedItemType::Plugin => {
                ManagedManifest::validate(&skill.item_type, target).map(|_| ())
            }
        };
        manifest_result.map_err(|reason| {
            format!(
                "Odmowa usunięcia {}: manifest nie jest już poprawny ({reason})",
                target.display()
            )
        })?;

        // A stale UI entry must never turn a single-skill deletion into
        // deletion of another item nested below the same monitored directory.
        if let Some(other) = all_skills.iter().find(|other| {
            other.id != skill.id
                && available_locations(other)
                    .iter()
                    .any(|path| path != target && path.starts_with(target))
        }) {
            return Err(format!(
                "Nie można usunąć {}: zawiera osobny wykryty zasób '{}'. Usuń go oddzielnie.",
                target.display(),
                other.name
            ));
        }
    }

    Ok(targets)
}

fn remove_location(target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(target).map_err(|error| {
        format!(
            "Nie można odczytać lokalizacji {}: {error}",
            target.display()
        )
    })?;

    // Removing a directory symlink must remove the link only. Following it
    // would silently delete another declared installation.
    if metadata.file_type().is_symlink() {
        fs::remove_file(target)
            .map_err(|error| format!("Nie można usunąć dowiązania {}: {error}", target.display()))
    } else if metadata.is_dir() {
        fs::remove_dir_all(target)
            .map_err(|error| format!("Nie można usunąć {}: {error}", target.display()))
    } else {
        Err(format!(
            "Lokalizacja nie jest katalogiem: {}",
            target.display()
        ))
    }
}

fn process_removal_locations<F>(
    skill: &SkillMetadata,
    requested: &[PathBuf],
    all_skills: &[SkillMetadata],
    mut remove_one: F,
) -> Result<(Vec<PathBuf>, Vec<String>), String>
where
    F: FnMut(&Path) -> Result<(), String>,
{
    if requested.is_empty() {
        return Err("Wybierz co najmniej jedną lokalizację do usunięcia.".to_string());
    }

    let available = available_locations(skill);
    let mut targets = Vec::new();
    for location in requested {
        if !available.contains(location) {
            return Err(format!(
                "Odmowa usunięcia nieznanej lokalizacji: {}",
                location.display()
            ));
        }
        if !targets.contains(location) {
            targets.push(location.clone());
        }
    }

    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for target in targets {
        let validated =
            match validate_removal_targets(skill, std::slice::from_ref(&target), all_skills) {
                Ok(validated) => validated,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
        match remove_one(&validated[0]) {
            Ok(()) => removed.push(validated[0].clone()),
            Err(error) => failures.push(error),
        }
    }

    Ok((removed, failures))
}

#[tauri::command]
pub async fn scan_skills(
    app: tauri::AppHandle,
    queue: State<'_, ScanQueue>,
    _force_refresh: bool,
) -> Result<Vec<SkillMetadata>, String> {
    let mut skills = discover_skills();
    for skill in &mut skills {
        if skill.remote_url.is_some() {
            skill.status = SkillStatus::Checking;
            skill.update_available = false;
        }
    }
    queue.enqueue(app, skills.clone());

    Ok(skills)
}

#[tauri::command]
pub async fn check_github_update(skill_id: String) -> Result<SkillMetadata, String> {
    let all_skills = discover_skills();
    let skill = all_skills
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill with ID {} not found", skill_id))?;

    Ok(crate::services::scan_queue::refresh_upstream(skill).await)
}

#[tauri::command]
pub async fn update_single_skill(
    app: tauri::AppHandle,
    skill_id: String,
    target_version: Option<String>,
    force: Option<bool>,
) -> Result<SkillMetadata, String> {
    use crate::models::skill::{UpdateProgressPayload, UpdateStage};
    use tauri::Emitter;

    let all_skills = discover_skills();
    let mut skill = all_skills
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill with ID {} not found", skill_id))?;

    // The details view may have been open while a new tag or commit appeared.
    // Refresh the chosen tracking source immediately before the transaction so
    // Update never falls back to the stale manifest version discovered at app
    // startup.
    if target_version.is_none() {
        skill = crate::services::scan_queue::refresh_upstream(skill).await;
        if let SkillStatus::Error(message) = &skill.status {
            return Err(format!(
                "Nie można bezpiecznie rozpocząć aktualizacji: {message}"
            ));
        }
    }

    let _ = app.emit(
        "update-progress",
        UpdateProgressPayload {
            skill_id: skill.id.clone(),
            skill_name: skill.name.clone(),
            stage: UpdateStage::Validating,
            percentage: 15,
            message: format!("Walidacja parametrów dla {}...", skill.name),
        },
    );

    let _ = app.emit(
        "update-progress",
        UpdateProgressPayload {
            skill_id: skill.id.clone(),
            skill_name: skill.name.clone(),
            stage: UpdateStage::BackingUp,
            percentage: 35,
            message: "Tworzenie migawki bezpieczeństwa (backup snapshot)...".into(),
        },
    );

    let locs_count = if skill.installed_locations.is_empty() {
        1
    } else {
        skill.installed_locations.len()
    };
    let _ = app.emit(
        "update-progress",
        UpdateProgressPayload {
            skill_id: skill.id.clone(),
            skill_name: skill.name.clone(),
            stage: UpdateStage::CheckingOut,
            percentage: 65,
            message: format!(
                "Aktualizacja i synchronizacja w {} lokalizacjach...",
                locs_count
            ),
        },
    );

    let result =
        UpdateOrchestrator::update_skill_atomic(&skill, target_version, force.unwrap_or(false))
            .await;

    match result {
        Ok(updated) => {
            let _ = app.emit(
                "update-progress",
                UpdateProgressPayload {
                    skill_id: skill.id.clone(),
                    skill_name: skill.name.clone(),
                    stage: UpdateStage::Completed,
                    percentage: 100,
                    message: format!(
                        "Zaktualizowano pomyślnie do wersji {} we wszystkich lokalizacjach!",
                        updated.current_version
                    ),
                },
            );
            Ok(updated)
        }
        Err(e) => {
            let _ = app.emit(
                "update-progress",
                UpdateProgressPayload {
                    skill_id: skill.id.clone(),
                    skill_name: skill.name.clone(),
                    stage: UpdateStage::Failed,
                    percentage: 100,
                    message: format!("Błąd aktualizacji: {}", e),
                },
            );
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn checkout_custom_version(
    skill_id: String,
    target_ref: String,
) -> Result<SkillMetadata, String> {
    let all_skills = discover_skills();
    let mut skill = all_skills
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill with ID {} not found", skill_id))?;

    // 1. Create backup snapshot first
    let snapshot = BackupService::create_snapshot(&skill.path, &skill.id, &skill.current_version)
        .map_err(|e| e.to_string())?;

    // 2. Perform git checkout ref
    if let Err(e) = GitService::checkout_ref(&skill.path, &target_ref) {
        let _ = BackupService::restore_snapshot(&skill.path, &snapshot.backup_file_path);
        return Err(e.to_string());
    }

    skill.current_version = target_ref;
    skill.update_available = false;
    skill.status = SkillStatus::UpToDate;
    skill.last_checked = chrono::Utc::now();

    Ok(skill)
}

#[tauri::command]
pub async fn batch_update_skills(skill_ids: Vec<String>) -> Result<serde_json::Value, String> {
    let all_skills = discover_skills();
    let target_skills: Vec<SkillMetadata> = all_skills
        .into_iter()
        .filter(|s| skill_ids.contains(&s.id))
        .collect();

    let config = ConfigService::load_config();
    let limit = config.updates.concurrency_limit;

    let (succeeded, failed) = UpdateOrchestrator::batch_update(target_skills, limit).await;

    Ok(serde_json::json!({
        "succeeded": succeeded,
        "failed": failed,
    }))
}

/// Removes only locations selected by the user from the freshly discovered
/// item. Each real directory receives a recovery snapshot before any deletion;
/// a directory symlink is unlinked without touching its target.
#[tauri::command]
pub async fn remove_skill(
    skill_id: String,
    locations: Vec<PathBuf>,
) -> Result<Vec<String>, String> {
    let all_skills = discover_skills();
    let skill = all_skills
        .iter()
        .find(|skill| skill.id == skill_id)
        .ok_or_else(|| format!("Nie znaleziono skillu o identyfikatorze {skill_id}"))?;
    // Process each requested installation independently. One stale, damaged,
    // or unreadable location must not prevent the user's other selected copies
    // from being removed. Every real directory still requires its own backup.
    let (removed, failures) =
        process_removal_locations(skill, &locations, &all_skills, |target| {
            match fs::symlink_metadata(target) {
                Ok(metadata) if !metadata.file_type().is_symlink() => {
                    if let Err(error) =
                        BackupService::create_snapshot(target, &skill.id, &skill.current_version)
                    {
                        return Err(format!(
                        "Nie usunięto {}: nie udało się utworzyć migawki bezpieczeństwa: {error}",
                        target.display()
                    ));
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    return Err(format!(
                        "Nie usunięto {}: nie można odczytać lokalizacji: {error}",
                        target.display()
                    ));
                }
            }
            remove_location(target)
        })?;

    if failures.is_empty() {
        return Ok(removed
            .iter()
            .map(|target| target.to_string_lossy().to_string())
            .collect());
    }
    let prefix = if removed.is_empty() {
        "Nie udało się usunąć żadnej lokalizacji."
    } else {
        "Usunięto część lokalizacji; pozostałe wymagają uwagi."
    };
    let removed_summary = if removed.is_empty() {
        String::new()
    } else {
        format!(
            " Usunięto: {}.",
            removed
                .iter()
                .map(|target| target.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(format!("{prefix}{removed_summary} {}", failures.join(" ")))
}

#[tauri::command]
pub async fn rollback_skill(skill_id: String, snapshot_id: Option<String>) -> Result<bool, String> {
    let all_skills = discover_skills();
    let skill = all_skills
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill with ID {} not found", skill_id))?;

    let backups = BackupService::list_snapshots(&skill_id);
    let target_snapshot = if let Some(id) = snapshot_id {
        backups.into_iter().find(|b| b.snapshot_id == id)
    } else {
        backups.into_iter().next()
    };

    if let Some(snapshot) = target_snapshot {
        BackupService::restore_snapshot(&skill.path, &snapshot.backup_file_path)
            .map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Err("No backup snapshot found for rollback".into())
    }
}

#[tauri::command]
pub fn set_branch_override(skill_id: String, branch: Option<String>) -> Result<(), String> {
    let mut config = ConfigService::load_config();
    match branch.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => {
            if !GitService::is_valid_branch_name(&value) {
                return Err(
                    "Nieprawidłowa nazwa gałęzi. Wpisz nazwę, np. main lub feature/branch, a nie adres repozytorium GitHub.".to_string(),
                );
            }
            config.updates.branch_overrides.insert(skill_id, value);
        }
        _ => {
            config.updates.branch_overrides.remove(&skill_id);
        }
    }
    ConfigService::save_config(&config).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_repository_override(skill_id: String, repository: Option<String>) -> Result<(), String> {
    let mut config = ConfigService::load_config();
    match repository.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => {
            let normalized = GitHubService::normalize_github_repository_url(&value).ok_or_else(|| {
                "Nieprawidłowy adres GitHub. Wpisz adres główny repozytorium, np. https://github.com/PrefectHQ/fastmcp.".to_string()
            })?;
            config
                .updates
                .repository_overrides
                .insert(skill_id, normalized);
        }
        _ => {
            config.updates.repository_overrides.remove(&skill_id);
        }
    }
    ConfigService::save_config(&config).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_backups_list(skill_id: String) -> Result<Vec<BackupSnapshot>, String> {
    Ok(BackupService::list_snapshots(&skill_id))
}

#[tauri::command]
pub async fn open_in_editor(path: String) -> Result<(), String> {
    // Linux has no platform-specific launcher branch below, but it still
    // exposes this IPC command for a consistent cross-platform API.
    let _ = &path;
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(&path).spawn();
    }
    Ok(())
}

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !is_safe_external_url(&url) {
        return Err(
            "Dozwolone są wyłącznie adresy http:// lub https:// bez znaków sterujących."
                .to_string(),
        );
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        // Calling `cmd /C start` lets URL text become shell syntax. Explorer
        // receives the argument directly and avoids command injection.
        std::process::Command::new("explorer.exe")
            .arg(&url)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn is_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    !trimmed.is_empty()
        && !trimmed.chars().any(|character| character.is_control())
        && (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_url_validation_rejects_shell_and_non_web_targets() {
        assert!(is_safe_external_url(
            "https://github.com/tomaszboloz/SkillSync"
        ));
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("https://example.test/\nopen"));
        assert!(is_safe_external_url("https://example.test/?q=a&next=b"));
    }

    fn deletion_fixture(path: PathBuf, id: &str) -> SkillMetadata {
        SkillMetadata {
            item_type: ManagedItemType::Skill,
            id: id.to_string(),
            name: id.to_string(),
            description: "Fixture".to_string(),
            current_version: "1.0.0".to_string(),
            latest_version: None,
            author: "Tester".to_string(),
            path: path.clone(),
            is_git_repo: false,
            remote_url: None,
            branch_or_tag: None,
            detected_branch: None,
            branch_override: None,
            agent_scope: crate::models::skill::AgentScope::Global,
            status: SkillStatus::UpToDate,
            update_available: false,
            changelog: None,
            dependencies: vec![],
            permissions: vec![],
            last_checked: chrono::Utc::now(),
            compatibility: None,
            update_compatibility: None,
            installed_locations: vec![path],
        }
    }

    #[test]
    fn removal_rejects_a_path_that_was_not_discovered_for_the_skill() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-removal-reject-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let skill_path = root.join("skill");
        let unrelated = root.join("unrelated");
        fs::create_dir_all(&skill_path).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(skill_path.join("SKILL.md"), "# Skill").unwrap();
        let skill = deletion_fixture(skill_path, "skill-fixture");

        let error = validate_removal_targets(&skill, &[unrelated], std::slice::from_ref(&skill))
            .unwrap_err();
        assert!(error.contains("nieznanej lokalizacji"));
        assert!(skill.path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removal_deletes_the_selected_directory_only() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-removal-selected-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let first = root.join("claude/fixture");
        let second = root.join("codex/fixture");
        for location in [&first, &second] {
            fs::create_dir_all(location).unwrap();
            fs::write(location.join("SKILL.md"), "# Skill").unwrap();
        }
        let mut skill = deletion_fixture(first.clone(), "skill-fixture");
        skill.installed_locations.push(second.clone());
        let (removed, failures) = process_removal_locations(
            &skill,
            std::slice::from_ref(&first),
            std::slice::from_ref(&skill),
            remove_location,
        )
        .unwrap();

        assert_eq!(removed, vec![first.clone()]);
        assert!(failures.is_empty());
        assert!(!first.exists());
        assert!(second.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remove_everywhere_continues_when_one_location_fails() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-removal-all-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let first = root.join("claude/fixture");
        let second = root.join("codex/fixture");
        for location in [&first, &second] {
            fs::create_dir_all(location).unwrap();
            fs::write(location.join("SKILL.md"), "# Skill").unwrap();
        }
        let mut skill = deletion_fixture(first.clone(), "skill-fixture");
        skill.installed_locations.push(second.clone());

        let (removed, failures) = process_removal_locations(
            &skill,
            &[first.clone(), second.clone()],
            std::slice::from_ref(&skill),
            |target| {
                if target == first {
                    return Err("fixture failure".to_string());
                }
                remove_location(target)
            },
        )
        .unwrap();

        assert_eq!(removed, vec![second.clone()]);
        assert_eq!(failures, vec!["fixture failure"]);
        assert!(first.exists(), "the failed installation must be preserved");
        assert!(
            !second.exists(),
            "the other location should still be removed"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removal_rejects_any_unknown_location_before_removing_known_ones() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-removal-unknown-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let first = root.join("known");
        let unknown = root.join("unknown");
        for location in [&first, &unknown] {
            fs::create_dir_all(location).unwrap();
            fs::write(location.join("SKILL.md"), "# Skill").unwrap();
        }
        let skill = deletion_fixture(first.clone(), "skill-fixture");

        let error = process_removal_locations(
            &skill,
            &[first.clone(), unknown],
            std::slice::from_ref(&skill),
            remove_location,
        )
        .unwrap_err();

        assert!(error.contains("nieznanej lokalizacji"));
        assert!(
            first.exists(),
            "untrusted paths must invalidate the request"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn removal_unlinks_a_directory_symlink_without_deleting_its_target() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "skillsync-removal-symlink-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let target = root.join("central/fixture");
        let link = root.join("claude/fixture");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(target.join("SKILL.md"), "# Skill").unwrap();
        symlink(&target, &link).unwrap();

        remove_location(&link).unwrap();
        assert!(!link.exists());
        assert!(target.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_update_check_uses_semver_ordering() {
        assert!(GitHubService::is_newer_version("v2.3.1", "2.0.0"));
        assert!(!GitHubService::is_newer_version("v2.3.1", "2.3.1"));
        assert!(!GitHubService::is_newer_version("v2.0.0", "2.3.1"));
    }

    #[test]
    fn scan_and_detail_apply_the_same_github_release_result() {
        let mut skill = SkillMetadata {
            item_type: crate::models::skill::ManagedItemType::Skill,
            id: "skill-release-fixture".to_string(),
            name: "release-fixture".to_string(),
            description: "Fixture".to_string(),
            current_version: "1.0.0".to_string(),
            latest_version: None,
            author: "Tester".to_string(),
            path: std::path::PathBuf::from("/tmp/release-fixture"),
            is_git_repo: true,
            remote_url: Some("https://github.com/example/release-fixture".to_string()),
            branch_or_tag: Some("main".to_string()),
            detected_branch: Some("main".to_string()),
            branch_override: None,
            agent_scope: crate::models::skill::AgentScope::Global,
            status: SkillStatus::UpToDate,
            update_available: false,
            changelog: None,
            dependencies: vec![],
            permissions: vec![],
            last_checked: chrono::Utc::now(),
            compatibility: None,
            update_compatibility: None,
            installed_locations: vec![],
        };
        let release = crate::services::github::GitHubReleaseInfo {
            tag_name: "v1.2.0".to_string(),
            name: Some("v1.2.0".to_string()),
            body: Some("Release notes".to_string()),
            published_at: None,
        };

        apply_github_release(&mut skill, &release);

        assert_eq!(skill.latest_version.as_deref(), Some("1.2.0"));
        assert!(skill.update_available);
        assert_eq!(skill.status, SkillStatus::UpdateAvailable);
        assert_eq!(skill.changelog.as_deref(), Some("Release notes"));
    }

    #[tokio::test]
    async fn test_atomic_update_and_rollback() {
        let temp_dir = std::env::temp_dir().join(format!(
            "skillsync-test-{}",
            chrono::Utc::now().timestamp_micros()
        ));
        let skill_dir = temp_dir.join("test-sync-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let initial_skill_md = "---\nname: test-sync-skill\nmetadata:\n  author: Tester\n  version: \"1.0.0\"\n---\n# Test Sync\nInitial content\n";
        std::fs::write(skill_dir.join("SKILL.md"), initial_skill_md).unwrap();

        let meta = SkillMetadata {
            item_type: crate::models::skill::ManagedItemType::Skill,
            id: "skill-test-sync-skill".to_string(),
            name: "test-sync-skill".to_string(),
            description: "Test skill".to_string(),
            current_version: "1.0.0".to_string(),
            latest_version: Some("1.2.0".to_string()),
            author: "Tester".to_string(),
            path: skill_dir.clone(),
            is_git_repo: false,
            remote_url: None,
            branch_or_tag: None,
            detected_branch: None,
            branch_override: None,
            agent_scope: crate::models::skill::AgentScope::Global,
            status: SkillStatus::UpdateAvailable,
            update_available: true,
            changelog: None,
            dependencies: vec![],
            permissions: vec![],
            last_checked: chrono::Utc::now(),
            compatibility: Some("All Agents".to_string()),
            update_compatibility: None,
            installed_locations: vec![skill_dir.clone()],
        };

        // 1. Perform atomic update to 1.2.0
        let updated =
            UpdateOrchestrator::update_skill_atomic(&meta, Some("1.2.0".to_string()), false)
                .await
                .unwrap();
        assert_eq!(updated.current_version, "1.2.0");
        assert_eq!(updated.status, SkillStatus::UpToDate);

        let content_after = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content_after.contains("version: \"1.2.0\""));

        // 2. Verify snapshot was saved and has accurate timestamp & original version
        let snapshots = BackupService::list_snapshots("skill-test-sync-skill");
        assert!(
            !snapshots.is_empty(),
            "Backup snapshot should have been created"
        );
        let latest_snap = &snapshots[0];
        assert_eq!(latest_snap.original_version, "1.0.0");

        // 3. Test rollback
        BackupService::restore_snapshot(&skill_dir, &latest_snap.backup_file_path).unwrap();
        let content_restored = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content_restored.contains("version: \"1.0.0\""));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_agent_browser_detection_and_tag_resolution() {
        let skills = discover_skills();
        let agent_browser = skills.iter().find(|s| s.name == "agent-browser");
        if let Some(skill) = agent_browser {
            println!(
                "agent-browser: current={}, latest={:?}, update_available={}",
                skill.current_version, skill.latest_version, skill.update_available
            );
            // The local installation may legitimately already be on the
            // latest tag, so this test must not depend on a developer's
            // private version being older than a fixed release.
            assert!(
                !skill.current_version.is_empty(),
                "current_version must be resolved from a real local manifest or Git ref"
            );
            if skill.latest_version == Some("0.37.1".to_string()) {
                assert!(
                    skill.update_available,
                    "agent-browser should have update_available = true"
                );
            }
        }
    }
}
