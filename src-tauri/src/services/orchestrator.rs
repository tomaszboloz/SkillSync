use crate::errors::SkillSyncError;
use crate::models::skill::ManagedItemType;
use crate::models::skill::{SkillMetadata, SkillStatus};
use crate::services::backup::BackupService;
use crate::services::claude_plugin::ClaudePluginService;
use crate::services::detector::SkillDetector;
use crate::services::git::GitService;
use crate::services::github::GitHubService;
use crate::services::managed_manifest::{ManagedManifest, ManagedManifestKind};
use crate::services::manifest::SkillManifest;
use crate::services::mcp::McpService;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

pub struct UpdateOrchestrator;

// Discovery can expose multiple resources from one Git worktree. Serialising
// update transactions keeps a batch update from checking out the same
// repository while another resource is creating its safety snapshot.
static UPDATE_TRANSACTION_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpdateOperation {
    /// The directory to mutate or snapshot. For Git installations this is
    /// always the repository worktree, never a nested SKILL.md directory.
    target: PathBuf,
    /// The resource manifest that made this operation eligible for update.
    logical_target: PathBuf,
    is_git: bool,
}

impl UpdateOrchestrator {
    pub async fn update_skill_atomic(
        skill: &SkillMetadata,
        target_version: Option<String>,
        allow_dirty_worktree: bool,
    ) -> Result<SkillMetadata, SkillSyncError> {
        let _transaction_guard = UPDATE_TRANSACTION_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        // Only a user-selected branch overrides the release policy. A branch
        // merely detected from the current checkout is not a tracking choice:
        // tagged repositories must continue to update by their newest tag.
        let explicit_branch = skill
            .branch_override
            .clone()
            .filter(|branch| !branch.trim().is_empty());
        let target_tag = target_version
            .clone()
            .or_else(|| skill.latest_version.clone())
            .unwrap_or_else(|| skill.current_version.clone());
        let mut tracked_branch = explicit_branch.clone();
        let mut resolved_latest_version = skill.latest_version.clone();

        let mut locations: Vec<PathBuf> = skill.installed_locations.clone();
        if !locations.contains(&skill.path) {
            locations.push(skill.path.clone());
        }

        // Resolve *every* declared location before creating a snapshot or
        // checking anything out. Silently dropping a missing path made a
        // partial update look successful and left a stale installation behind.
        let canonical_targets = Self::resolve_locations(&locations)?;
        let operations = Self::plan_operations(&canonical_targets);

        // Resolve each remote's release once before touching any location.
        // Previously every operation called `ls-remote` independently and a
        // multi-location update could select different tags during a moving
        // upstream release. A shared plan makes the transaction deterministic
        // and lets non-Git mirrors use the same release as their Git source.
        let mut planned_tags = HashMap::<String, String>::new();
        let mut planned_default_tag = None;
        if explicit_branch.is_none() && target_version.is_none() {
            for operation in operations.iter().filter(|operation| operation.is_git) {
                let Some(remote) = GitService::get_remote_url(&operation.target) else {
                    continue;
                };
                let remote_key = Self::remote_key(&remote);
                if planned_tags.contains_key(&remote_key) {
                    continue;
                }
                if let Some(tag) = GitService::get_latest_remote_tag(&operation.target)? {
                    if planned_default_tag.is_none() {
                        planned_default_tag = Some(tag.clone());
                    }
                    planned_tags.insert(remote_key, tag);
                }
            }
        }
        let target_tag = planned_default_tag
            .as_deref()
            .map(|tag| tag.trim_start_matches(['v', 'V']).to_string())
            .unwrap_or(target_tag);

        let mut planned_branch_commits = HashMap::<String, String>::new();
        if let Some(branch) = explicit_branch.as_deref() {
            for operation in operations.iter().filter(|operation| operation.is_git) {
                let Some(remote) = GitService::get_remote_url(&operation.target) else {
                    continue;
                };
                let remote_key = Self::remote_key(&remote);
                if planned_branch_commits.contains_key(&remote_key) {
                    continue;
                }
                planned_branch_commits.insert(
                    remote_key,
                    GitService::get_remote_branch_commit(&operation.target, branch)?,
                );
            }
        }

        // Never snapshot, check out, or rewrite an arbitrary directory. The
        // detector should make this guard unreachable in normal use, but it is
        // the transaction-level safety net for stale UI state and custom IPC.
        for target in &canonical_targets {
            let manifest =
                ManagedManifest::validate(&skill.item_type, target).map_err(|reason| {
                    SkillSyncError::InvalidManifest(format!("{}: {reason}", target.display()))
                })?;
            let safe_adapter = (skill.item_type == ManagedItemType::Mcp
                && manifest == ManagedManifestKind::LaravelBoost
                && McpService::is_laravel_boost_project(target))
                || (skill.item_type == ManagedItemType::Plugin
                    && manifest == ManagedManifestKind::Plugin
                    && (ClaudePluginService::installation_for_path(target).is_some()
                        || ClaudePluginService::marketplace_for_path(target).is_some()));
            if !GitService::is_git_repository(target)
                && skill.item_type != ManagedItemType::Skill
                && !safe_adapter
            {
                return Err(SkillSyncError::UnsupportedUpdateMethod(format!(
                    "{} nie jest repozytorium Git. SkillSync monitoruje ten manifest, ale nie uruchomi automatycznie menedżera pakietów bez jawnego, bezpiecznego adaptera aktualizacji.",
                    target.display()
                )));
            }
        }

        // A repository can expose several nested skills. It is one working
        // tree, so check its dirty state once and perform at most one checkout
        // for it. This prevents concurrent/nested entries from invalidating
        // each other's path during an update.
        for operation in &operations {
            if operation.is_git && !allow_dirty_worktree {
                let target = &operation.target;
                let clean = GitService::is_worktree_clean(target)?;
                let safe_legacy_skill_metadata = skill.item_type == ManagedItemType::Skill
                    && !clean
                    && GitService::has_only_skill_version_metadata_change(target)?;
                if !clean && !safe_legacy_skill_metadata {
                    return Err(SkillSyncError::WorktreeDirty);
                }
            }
        }

        // 1. Stage: Create Atomic Snapshots for all target paths
        let mut snapshots = Vec::new();
        for operation in &operations {
            let snapshot_target = &operation.target;
            let snapshot =
                BackupService::create_snapshot(snapshot_target, &skill.id, &skill.current_version)
                    .map_err(|error| {
                        SkillSyncError::FileSystem(format!(
                            "Nie udało się utworzyć migawki bezpieczeństwa dla zasobu {} (katalog kopii: {}): {error}",
                            operation.logical_target.display(),
                            snapshot_target.display(),
                        ))
                    })?;
            snapshots.push((snapshot_target.clone(), snapshot));
        }

        // 2. Stage: Perform updates across all locations
        let mut resolved_version: Option<String> = None;
        let mut resolved_tag: Option<String> = None;
        let mut resolved_locations: Option<Vec<PathBuf>> = None;
        for operation in &operations {
            let target = &operation.target;
            let logical_target = &operation.logical_target;
            let manifest =
                ManagedManifest::validate(&skill.item_type, logical_target).map_err(|reason| {
                    SkillSyncError::InvalidManifest(format!(
                        "{}: {reason}",
                        logical_target.display()
                    ))
                })?;
            let mut verification_targets = vec![logical_target.clone()];

            // Laravel Boost installed in an application root is owned by
            // Composer, not by the application's Git remote. Run its explicit
            // adapter only when lockfile evidence proves it is installed.
            if skill.item_type == ManagedItemType::Mcp
                && manifest == ManagedManifestKind::LaravelBoost
                && McpService::is_laravel_boost_project(logical_target)
            {
                if let Err(error) = McpService::update_laravel_boost(logical_target) {
                    for (t, snap) in &snapshots {
                        let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                    }
                    return Err(error);
                }
                let version =
                    McpService::laravel_boost_version(logical_target).ok_or_else(|| {
                        SkillSyncError::IntegrityCheckFailed(format!(
                            "composer.lock nie zawiera laravel/boost po aktualizacji w {}",
                            logical_target.display()
                        ))
                    })?;
                if let Some(previous) = &resolved_version {
                    if previous != &version {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(SkillSyncError::IntegrityCheckFailed(
                            "różne lokalizacje Laravel Boost mają różne wersje po aktualizacji"
                                .to_string(),
                        ));
                    }
                } else {
                    resolved_version = Some(version);
                }
            // Claude Code cache plugins must be updated by the CLI that owns
            // their registry, never by copying cache files directly.
            } else if skill.item_type == ManagedItemType::Plugin
                && manifest == ManagedManifestKind::Plugin
                && ClaudePluginService::installation_for_path(logical_target).is_some()
            {
                let locations = match ClaudePluginService::update(logical_target) {
                    Ok(locations) => locations,
                    Err(error) => {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(error);
                    }
                };
                let version =
                    ClaudePluginService::plugin_version(&locations[0]).ok_or_else(|| {
                        SkillSyncError::IntegrityCheckFailed(format!(
                            "Claude Code nie podał wersji pluginu po aktualizacji w {}",
                            locations[0].display()
                        ))
                    })?;
                verification_targets = locations.clone();
                resolved_locations = Some(locations);
                resolved_version = Some(version);
            // Claude marketplace checkouts are registry-owned sources. The
            // marketplace command updates the catalog and its plugin manifests
            // together; treating the directory as a generic Git repository
            // would fail for legitimate installations without a local .git.
            } else if skill.item_type == ManagedItemType::Plugin
                && manifest == ManagedManifestKind::Plugin
                && ClaudePluginService::marketplace_for_path(logical_target).is_some()
            {
                let locations = match ClaudePluginService::update_marketplace(logical_target) {
                    Ok(locations) => locations,
                    Err(error) => {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(error);
                    }
                };
                let version =
                    ClaudePluginService::plugin_version(&locations[0]).ok_or_else(|| {
                        SkillSyncError::IntegrityCheckFailed(format!(
                            "Claude Code nie podał wersji marketplace po aktualizacji w {}",
                            locations[0].display()
                        ))
                    })?;
                verification_targets = locations.clone();
                resolved_locations = Some(locations);
                resolved_version = Some(version);
            // A. If Git repo, perform fetch and checkout
            } else if operation.is_git {
                // A portable package can need a tiny local SKILL.md adapter
                // for discovery. If a later upstream release introduces its
                // own tracked SKILL.md, Git correctly refuses to overwrite
                // the untracked adapter. Remove only our unmistakably
                // generated, untracked adapter after the snapshot exists;
                // user-authored and upstream-tracked manifests stay intact.
                for location in canonical_targets.iter().filter(|location| {
                    GitService::repository_root(location).as_deref() == Some(target.as_path())
                }) {
                    if let Err(error) = Self::remove_generated_skill_adapter(location) {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(error);
                    }
                }

                let safe_legacy_skill_metadata = skill.item_type == ManagedItemType::Skill
                    && !GitService::is_worktree_clean(target)?
                    && GitService::has_only_skill_version_metadata_change(target)?;
                let allow_git_checkout = allow_dirty_worktree || safe_legacy_skill_metadata;
                let checkout_result = if let Some(branch) = &explicit_branch {
                    tracked_branch = Some(branch.clone());
                    resolved_latest_version = Some(branch.clone());
                    let result =
                        GitService::fetch_and_checkout_branch(target, branch, allow_git_checkout);
                    if result.is_ok() {
                        if let Some(remote) = GitService::get_remote_url(target) {
                            if let Some(expected) =
                                planned_branch_commits.get(&Self::remote_key(&remote))
                            {
                                if GitService::get_head_commit(target).as_deref()
                                    != Some(expected.as_str())
                                {
                                    for (t, snap) in &snapshots {
                                        let _ = BackupService::restore_snapshot(
                                            t,
                                            &snap.backup_file_path,
                                        );
                                    }
                                    return Err(SkillSyncError::IntegrityCheckFailed(
                                        "gałąź upstream zmieniła się w trakcie aktualizacji; ponów próbę"
                                            .to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    result
                } else if let Some(requested_tag) = target_version.as_deref() {
                    tracked_branch = None;
                    let version = requested_tag.trim_start_matches(['v', 'V']).to_string();
                    resolved_latest_version = Some(version.clone());
                    resolved_version = Some(version);
                    GitService::fetch_and_checkout_tag(target, requested_tag, allow_git_checkout)
                } else {
                    let planned_tag = GitService::get_remote_url(target)
                        .and_then(|remote| planned_tags.get(&Self::remote_key(&remote)).cloned());
                    let remote_tag = match planned_tag {
                        Some(tag) => Some(tag),
                        None => GitService::get_latest_remote_tag(target)?,
                    };
                    match remote_tag {
                        Some(remote_tag) => {
                            tracked_branch = None;
                            let version = remote_tag.trim_start_matches(['v', 'V']).to_string();
                            if let Some(previous) = &resolved_tag {
                                if previous.trim_start_matches(['v', 'V']) != version.as_str() {
                                    for (t, snap) in &snapshots {
                                        let _ = BackupService::restore_snapshot(
                                            t,
                                            &snap.backup_file_path,
                                        );
                                    }
                                    return Err(SkillSyncError::IntegrityCheckFailed(
                                        "lokalizacje wskazują różne wydania upstream; aktualizacja została wycofana"
                                            .to_string(),
                                    ));
                                }
                            }
                            resolved_tag = Some(remote_tag.clone());
                            resolved_latest_version = Some(version.clone());
                            resolved_version = Some(version);
                            GitService::fetch_and_checkout_tag(
                                target,
                                &remote_tag,
                                allow_git_checkout,
                            )
                        }
                        None => {
                            let branch = GitService::resolve_fallback_branch(
                                target,
                                skill.detected_branch.as_deref(),
                            )?;
                            tracked_branch = Some(branch.clone());
                            resolved_latest_version = Some(branch.clone());
                            GitService::fetch_and_checkout_branch(
                                target,
                                &branch,
                                allow_git_checkout,
                            )
                        }
                    }
                };
                if let Err(e) = checkout_result {
                    for (t, snap) in &snapshots {
                        let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                    }
                    return Err(e);
                }
            } else if skill.item_type == ManagedItemType::Skill {
                if let Some(ref remote_url) = skill.remote_url {
                    // If not git repo, fetch latest upstream SKILL.md if available
                    let Some(upstream_content) =
                        GitHubService::fetch_raw_skill_md(remote_url, &target_tag, &skill.name)
                            .await
                    else {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(SkillSyncError::IntegrityCheckFailed(format!(
                            "Nie można pobrać manifestu skilla {} z {} dla wersji {}",
                            skill.name, remote_url, target_tag
                        )));
                    };
                    let skill_md_path = logical_target.join("SKILL.md");
                    if let Err(error) = fs::write(&skill_md_path, upstream_content) {
                        for (t, snap) in &snapshots {
                            let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                        }
                        return Err(SkillSyncError::FileSystem(format!(
                            "Nie można zapisać manifestu skilla po aktualizacji w {}: {error}",
                            skill_md_path.display()
                        )));
                    }
                }
            } else {
                for (t, snap) in &snapshots {
                    let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                }
                return Err(SkillSyncError::UnsupportedUpdateMethod(format!(
                    "{} nie jest repozytorium Git. SkillSync monitoruje ten manifest, ale nie uruchomi automatycznie menedżera pakietów bez jawnego, bezpiecznego adaptera aktualizacji.",
                    target.display()
                )));
            }

            // B. Update manifests on disk (SKILL.md, skill.json, package.json)
            if skill.item_type == ManagedItemType::Skill && !operation.is_git {
                if let Err(e) = Self::update_skill_md_version(logical_target, &target_tag) {
                    for (t, snap) in &snapshots {
                        let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                    }
                    return Err(e);
                }

                Self::update_skill_json_version(logical_target, &target_tag)?;
                if SkillManifest::is_explicit_package_skill(logical_target) {
                    Self::update_package_json_version(logical_target, &target_tag)?;
                }
            }

            // Some upstreams evolve a tracked SKILL.md into a portable Node
            // package. The package is still the same skill only when its
            // declared name matches the discovered skill and it exposes a
            // command entry point. Keep it discoverable with a local adapter
            // instead of rolling back an otherwise valid tagged release.
            if skill.item_type == ManagedItemType::Skill
                && GitService::is_git_repository(target)
                && !Self::verify_integrity(&skill.item_type, logical_target)
            {
                let adapter_version = resolved_latest_version.as_deref().unwrap_or(&target_tag);
                if let Err(error) = Self::materialize_package_skill_adapter(
                    logical_target,
                    &skill.name,
                    adapter_version,
                ) {
                    for (t, snap) in &snapshots {
                        let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                    }
                    return Err(error);
                }
            }

            // C. Stage: Post-Update Integrity Verification
            for verification_target in verification_targets {
                if !Self::verify_integrity(&skill.item_type, &verification_target) {
                    for (t, snap) in &snapshots {
                        let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                    }
                    return Err(SkillSyncError::IntegrityCheckFailed(format!(
                        "Manifest zasobu jest uszkodzony lub nieobecny po aktualizacji w {:?}",
                        verification_target
                    )));
                }
            }

            // A branch update is commit-based, so the release tag cannot tell
            // us the new package version. Re-read the manifest after checkout
            // instead of leaving the pre-update version in the UI.
            if tracked_branch.is_some() {
                if let Some(version) = Self::version_from_manifest(&skill.item_type, logical_target)
                {
                    if let Some(previous) = &resolved_version {
                        if previous != &version {
                            for (t, snap) in &snapshots {
                                let _ = BackupService::restore_snapshot(t, &snap.backup_file_path);
                            }
                            return Err(SkillSyncError::IntegrityCheckFailed(
                                "różne lokalizacje mają różne wersje po aktualizacji".to_string(),
                            ));
                        }
                    } else {
                        resolved_version = Some(version);
                    }
                }
            }
        }

        // The operation can only be reported as successful once every
        // location supplied by discovery still exists and exposes a valid
        // manifest. This catches stale aliases and nested package paths after
        // a checkout before the UI persists an optimistic version.
        let mut observed_versions = Vec::new();
        for target in &canonical_targets {
            if !Self::verify_integrity(&skill.item_type, target) {
                for (snapshot_target, snapshot) in &snapshots {
                    let _ = BackupService::restore_snapshot(
                        snapshot_target,
                        &snapshot.backup_file_path,
                    );
                }
                return Err(SkillSyncError::IntegrityCheckFailed(format!(
                    "Manifest zasobu jest uszkodzony lub nieobecny po aktualizacji w {:?}",
                    target
                )));
            }
            if let Some(version) = Self::version_from_manifest(&skill.item_type, target) {
                if !observed_versions.contains(&version) {
                    observed_versions.push(version);
                }
            }
        }
        if observed_versions.len() > 1 {
            for (snapshot_target, snapshot) in &snapshots {
                let _ =
                    BackupService::restore_snapshot(snapshot_target, &snapshot.backup_file_path);
            }
            return Err(SkillSyncError::IntegrityCheckFailed(
                "różne lokalizacje mają różne wersje po aktualizacji; przywrócono wszystkie migawki"
                    .to_string(),
            ));
        }

        // Return updated metadata
        let mut updated = skill.clone();
        let current_version = resolved_version.unwrap_or_else(|| {
            if tracked_branch.is_some() {
                skill.current_version.clone()
            } else {
                target_tag.clone()
            }
        });
        let latest_version = resolved_latest_version.unwrap_or_else(|| target_tag.clone());
        let update_still_available = tracked_branch.is_none()
            && crate::services::github::GitHubService::is_newer_version(
                &latest_version,
                &current_version,
            );
        updated.current_version = current_version.clone();
        updated.latest_version = Some(latest_version.clone());
        updated.update_available = update_still_available;
        updated.status = if update_still_available {
            SkillStatus::UpdateAvailable
        } else {
            SkillStatus::UpToDate
        };
        updated.update_compatibility = update_still_available.then(|| {
            format!(
                "Composer retained {} while upstream offers {}; check the package constraint in composer.json.",
                current_version, latest_version
            )
        });
        updated.last_checked = chrono::Utc::now();
        if let Some(branch) = tracked_branch {
            updated.branch_or_tag = Some(branch);
        }
        if let Some(locations) = resolved_locations {
            updated.path = locations[0].clone();
            updated.installed_locations = locations;
        }

        Ok(updated)
    }

    fn resolve_locations(locations: &[PathBuf]) -> Result<Vec<PathBuf>, SkillSyncError> {
        let mut resolved = Vec::new();
        for location in locations {
            let canonical = fs::canonicalize(location).map_err(|error| {
                SkillSyncError::FileSystem(format!(
                    "Lokalizacja instalacji jest niedostępna: {} ({error}). Aktualizacja została przerwana przed utworzeniem migawki, aby żadna lokalizacja nie pozostała na innej wersji.",
                    location.display()
                ))
            })?;
            if !canonical.is_dir() {
                return Err(SkillSyncError::FileSystem(format!(
                    "Lokalizacja instalacji nie jest katalogiem: {}. Aktualizacja została przerwana przed utworzeniem migawki.",
                    location.display()
                )));
            }
            if !resolved.contains(&canonical) {
                resolved.push(canonical);
            }
        }
        if resolved.is_empty() {
            return Err(SkillSyncError::FileSystem(
                "Brak lokalizacji instalacji do aktualizacji".into(),
            ));
        }
        Ok(resolved)
    }

    fn plan_operations(locations: &[PathBuf]) -> Vec<UpdateOperation> {
        let mut operations = Vec::new();
        for location in locations {
            let git_root = GitService::repository_root(location);
            let target = git_root.clone().unwrap_or_else(|| location.clone());
            if operations
                .iter()
                .any(|operation: &UpdateOperation| operation.target == target)
            {
                continue;
            }
            operations.push(UpdateOperation {
                target,
                logical_target: location.clone(),
                is_git: git_root.is_some(),
            });
        }
        operations
    }

    fn remote_key(remote: &str) -> String {
        GitHubService::normalize_github_repository_url(remote)
            .unwrap_or_else(|| remote.trim().trim_end_matches(".git").to_string())
            .to_lowercase()
    }

    fn version_from_manifest(item_type: &ManagedItemType, path: &Path) -> Option<String> {
        match item_type {
            ManagedItemType::Skill => SkillDetector::scan_directories(&[path.to_path_buf()])
                .into_iter()
                .find(|skill| skill.path == path)
                .map(|skill| {
                    skill
                        .current_version
                        .trim_start_matches(['v', 'V'])
                        .to_string()
                })
                .filter(|version| version != crate::services::detector::UNKNOWN_SKILL_VERSION),
            ManagedItemType::Plugin => {
                for relative in [
                    ".claude-plugin/plugin.json",
                    ".codex-plugin/plugin.json",
                    ".cursor-plugin/plugin.json",
                    "plugin.json",
                ] {
                    let value = fs::read_to_string(path.join(relative))
                        .ok()
                        .and_then(|content| {
                            serde_json::from_str::<serde_json::Value>(&content).ok()
                        });
                    if let Some(version) = value
                        .as_ref()
                        .and_then(|value| value.get("version"))
                        .and_then(serde_json::Value::as_str)
                    {
                        return Some(version.trim_start_matches(['v', 'V']).to_string());
                    }
                }
                None
            }
            ManagedItemType::Mcp => McpService::laravel_boost_version(path),
        }
    }

    pub fn update_skill_md_version(dir: &Path, new_version: &str) -> Result<(), SkillSyncError> {
        let skill_md_path = dir.join("SKILL.md");
        if !skill_md_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&skill_md_path)?;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut updated = false;

        if lines.first().map(|l| l.trim()) == Some("---") {
            let mut end_idx = None;
            for (i, line) in lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    end_idx = Some(i);
                    break;
                }
            }

            if let Some(end) = end_idx {
                for line in lines.iter_mut().take(end).skip(1) {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("version:") {
                        let indent_len = line.len() - trimmed.len();
                        let indent = &line[..indent_len];
                        *line = format!("{}version: \"{}\"", indent, new_version);
                        updated = true;
                        break;
                    }
                }
                if !updated {
                    // Check if metadata: section exists
                    let mut meta_idx = None;
                    for (i, line) in lines.iter().enumerate().take(end).skip(1) {
                        if line.trim() == "metadata:" {
                            meta_idx = Some(i);
                            break;
                        }
                    }
                    if let Some(mi) = meta_idx {
                        lines.insert(mi + 1, format!("  version: \"{}\"", new_version));
                    } else {
                        lines.insert(1, format!("version: \"{}\"", new_version));
                    }
                    updated = true;
                }
            }
        }

        if !updated {
            let frontmatter = format!("---\nversion: \"{}\"\n---\n\n", new_version);
            let new_content = format!("{}{}", frontmatter, content);
            fs::write(&skill_md_path, new_content)?;
        } else {
            let mut new_content = lines.join("\n");
            if content.ends_with('\n') {
                new_content.push('\n');
            }
            fs::write(&skill_md_path, new_content)?;
        }

        Ok(())
    }

    pub fn update_skill_json_version(dir: &Path, new_version: &str) -> Result<(), SkillSyncError> {
        let skill_json_path = dir.join("skill.json");
        if !skill_json_path.exists() {
            return Ok(());
        }
        if let Ok(content) = fs::read_to_string(&skill_json_path) {
            if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert(
                        "version".to_string(),
                        serde_json::Value::String(new_version.to_string()),
                    );
                    if let Ok(serialized) = serde_json::to_string_pretty(&val) {
                        let _ = fs::write(&skill_json_path, serialized);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn update_package_json_version(
        dir: &Path,
        new_version: &str,
    ) -> Result<(), SkillSyncError> {
        if !SkillManifest::is_explicit_package_skill(dir) {
            return Ok(());
        }

        let pkg_json_path = dir.join("package.json");
        if !pkg_json_path.exists() {
            return Ok(());
        }
        if let Ok(content) = fs::read_to_string(&pkg_json_path) {
            if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert(
                        "version".to_string(),
                        serde_json::Value::String(new_version.to_string()),
                    );
                    if let Ok(serialized) = serde_json::to_string_pretty(&val) {
                        let _ = fs::write(&pkg_json_path, serialized);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn batch_update(
        skills: Vec<SkillMetadata>,
        concurrency_limit: usize,
    ) -> (usize, usize) {
        let semaphore = Arc::new(Semaphore::new(concurrency_limit));
        let mut handles = Vec::new();

        for skill in skills {
            let sem = semaphore.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.unwrap();
                Self::update_skill_atomic(&skill, None, false).await
            }));
        }

        let mut succeeded = 0;
        let mut failed = 0;

        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => succeeded += 1,
                _ => failed += 1,
            }
        }

        (succeeded, failed)
    }

    fn verify_integrity(item_type: &ManagedItemType, path: &Path) -> bool {
        ManagedManifest::validate(item_type, path).is_ok()
    }

    /// Write a local adapter only for a confirmed CLI package that replaces an
    /// earlier SKILL.md in the *same* Git repository. This is intentionally
    /// narrow: malformed manifests and unrelated package.json files continue
    /// to fail the integrity gate instead of being silently reclassified.
    fn materialize_package_skill_adapter(
        dir: &Path,
        expected_name: &str,
        version: &str,
    ) -> Result<(), SkillSyncError> {
        if dir.join("SKILL.md").exists() || dir.join("skill.json").exists() {
            return Ok(());
        }

        let package_path = dir.join("package.json");
        let content = fs::read_to_string(&package_path).map_err(|error| {
            SkillSyncError::IntegrityCheckFailed(format!(
                "Brak obsługiwanego manifestu i nie można odczytać package.json w {}: {error}",
                dir.display()
            ))
        })?;
        let package: serde_json::Value = serde_json::from_str(&content).map_err(|_| {
            SkillSyncError::IntegrityCheckFailed(format!(
                "Brak obsługiwanego manifestu, a package.json nie zawiera poprawnego JSON w {}",
                dir.display()
            ))
        })?;
        let package_name = package.get("name").and_then(serde_json::Value::as_str);
        let has_command = package.get("bin").is_some();
        if package_name != Some(expected_name) || !has_command {
            return Err(SkillSyncError::IntegrityCheckFailed(format!(
                "Tag nie zawiera manifestu skilla ani potwierdzonego pakietu CLI '{}' w {}",
                expected_name,
                dir.display()
            )));
        }

        let description = package
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Portable AI agent skill package")
            .replace('\n', " ");
        let adapter = format!(
            "---\nname: {expected_name}\ndescription: {description}\nmetadata:\n  version: \"{}\"\n  generated-by: SkillSync\n---\n\n# {expected_name}\n\nThis local SkillSync adapter preserves discovery for the tagged CLI package. Run the command declared in package.json or consult README.md for usage.\n",
            version.trim_start_matches(['v', 'V'])
        );
        fs::write(dir.join("SKILL.md"), adapter)?;

        Ok(())
    }

    fn remove_generated_skill_adapter(dir: &Path) -> Result<bool, SkillSyncError> {
        let adapter_path = dir.join("SKILL.md");
        if !adapter_path.is_file() || GitService::is_file_tracked(&adapter_path) {
            return Ok(false);
        }

        let content = fs::read_to_string(&adapter_path)?;
        let is_generated_adapter = content.contains("generated-by: SkillSync")
            && content.contains("This local SkillSync adapter preserves discovery");
        if !is_generated_adapter {
            return Ok(false);
        }

        fs::remove_file(&adapter_path)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::skill::AgentScope;

    fn fixture_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "skillsync-orchestrator-{name}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn metadata_for(path: PathBuf) -> SkillMetadata {
        SkillMetadata {
            item_type: ManagedItemType::Skill,
            id: "skill-fixture".to_string(),
            name: "fixture".to_string(),
            description: "Fixture skill".to_string(),
            current_version: "1.0.0".to_string(),
            latest_version: Some("1.1.0".to_string()),
            author: "Test".to_string(),
            path: path.clone(),
            is_git_repo: false,
            remote_url: None,
            branch_or_tag: None,
            detected_branch: None,
            branch_override: None,
            agent_scope: AgentScope::Global,
            status: SkillStatus::UpdateAvailable,
            update_available: true,
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
    fn rejects_an_unavailable_location_before_any_snapshot_or_checkout() {
        let missing = fixture_dir("unavailable-location");
        let error = UpdateOrchestrator::resolve_locations(std::slice::from_ref(&missing))
            .expect_err("an unavailable location must abort the complete transaction");

        assert!(matches!(error, SkillSyncError::FileSystem(_)));
        assert!(error.to_string().contains(&missing.display().to_string()));
        assert!(error.to_string().contains("przed utworzeniem migawki"));
    }

    #[test]
    fn plans_one_git_operation_for_nested_locations_in_the_same_repository() {
        let root = fixture_dir("nested-operation");
        let first = root.join("skills/first");
        let second = root.join("skills/second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        git2::Repository::init(&root).unwrap();

        let locations = vec![
            fs::canonicalize(&first).unwrap(),
            fs::canonicalize(&second).unwrap(),
        ];
        let operations = UpdateOrchestrator::plan_operations(&locations);

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].target, fs::canonicalize(&root).unwrap());
        assert_eq!(operations[0].logical_target, locations[0]);
        assert!(operations[0].is_git);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn rejects_a_directory_without_a_skill_manifest_before_mutation() {
        let dir = fixture_dir("missing-manifest");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("package.json"), r#"{"name":"documentation"}"#).unwrap();
        let metadata = metadata_for(dir.clone());

        let error = UpdateOrchestrator::update_skill_atomic(&metadata, None, false)
            .await
            .expect_err("ordinary package directories must never be updated");
        assert!(matches!(error, SkillSyncError::InvalidManifest(_)));
        assert_eq!(
            fs::read_to_string(dir.join("package.json")).unwrap(),
            r#"{"name":"documentation"}"#
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn refuses_to_bump_a_non_git_skill_when_upstream_manifest_is_unavailable() {
        let dir = fixture_dir("missing-upstream-manifest");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: fixture\nversion: \"1.0.0\"\n---\n# Fixture\n",
        )
        .unwrap();
        let mut metadata = metadata_for(dir.clone());
        metadata.remote_url = Some("https://example.invalid/missing-skill".to_string());

        let error = UpdateOrchestrator::update_skill_atomic(&metadata, Some("2.0.0".into()), false)
            .await
            .expect_err("a failed upstream fetch must not become a local version bump");
        assert!(error.to_string().contains("Nie można pobrać manifestu"));
        assert!(fs::read_to_string(dir.join("SKILL.md"))
            .unwrap()
            .contains("version: \"1.0.0\""));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ordinary_package_version_is_never_rewritten() {
        let dir = fixture_dir("ordinary-package");
        fs::create_dir_all(&dir).unwrap();
        let original = r#"{"name":"documentation","version":"1.0.0"}"#;
        fs::write(dir.join("package.json"), original).unwrap();

        UpdateOrchestrator::update_package_json_version(&dir, "2.0.0").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("package.json")).unwrap(),
            original
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_package_skill_version_is_rewritten() {
        let dir = fixture_dir("package-skill");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name":"fixture","version":"1.0.0","skill":true}"#,
        )
        .unwrap();

        UpdateOrchestrator::update_package_json_version(&dir, "2.0.0").unwrap();
        let package: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
        assert_eq!(package["version"], "2.0.0");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_the_version_from_a_skill_manifest_after_a_branch_checkout() {
        let dir = fixture_dir("branch-version");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: wcag-accessibility-skills\nmetadata:\n  version: \"1.1.0\"\n---\n# Skill\n",
        )
        .unwrap();

        assert_eq!(
            UpdateOrchestrator::version_from_manifest(&ManagedItemType::Skill, &dir),
            Some("1.1.0".to_string())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn keeps_a_tagged_cli_skill_discoverable_when_upstream_removes_skill_md() {
        let dir = fixture_dir("portable-cli-skill");
        fs::create_dir_all(&dir).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        let signature = git2::Signature::now("SkillSync test", "tests@example.invalid").unwrap();
        let skill_md = dir.join("SKILL.md");
        fs::write(
            &skill_md,
            "---\nname: fixture\nmetadata:\n  version: \"1.0.0\"\n---\n# Fixture\n",
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("SKILL.md")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let first = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "skill manifest",
                &tree,
                &[],
            )
            .unwrap();
        let first_object = repo.find_object(first, None).unwrap();
        repo.tag_lightweight("v1.0.0", &first_object, false)
            .unwrap();

        fs::remove_file(&skill_md).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name":"fixture","version":"1.1.0","description":"Portable fixture","bin":{"fixture":"bin/fixture.js"}}"#,
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("SKILL.md")).unwrap();
        index.add_path(Path::new("package.json")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "portable package",
                &tree,
                &[&parent],
            )
            .unwrap();
        let second_object = repo.find_object(second, None).unwrap();
        repo.tag_lightweight("v1.1.0", &second_object, false)
            .unwrap();

        let initial = repo.find_commit(first).unwrap();
        repo.checkout_tree(initial.as_object(), None).unwrap();
        repo.set_head_detached(first).unwrap();

        let mut metadata = metadata_for(dir.clone());
        metadata.name = "fixture".to_string();
        metadata.current_version = "1.0.0".to_string();
        metadata.is_git_repo = true;

        let updated =
            UpdateOrchestrator::update_skill_atomic(&metadata, Some("1.1.0".to_string()), false)
                .await
                .unwrap();

        assert_eq!(updated.current_version, "1.1.0");
        assert!(ManagedManifest::validate(&ManagedItemType::Skill, &dir).is_ok());
        let adapter = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(adapter.contains("generated-by: SkillSync"));
        assert!(adapter.contains("version: \"1.1.0\""));

        // The next upstream release restores a real manifest. The adapter
        // created above is intentionally untracked, so a normal Git checkout
        // would reject overwriting it as a conflict without the recovery
        // guard in the update transaction.
        let repo = git2::Repository::open(&dir).unwrap();
        fs::write(
            &skill_md,
            "---\nname: fixture\nmetadata:\n  version: \"1.2.0\"\n---\n# Upstream skill\n",
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("SKILL.md")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let third = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "restore upstream skill manifest",
                &tree,
                &[&parent],
            )
            .unwrap();
        let third_object = repo.find_object(third, None).unwrap();
        repo.tag_lightweight("v1.2.0", &third_object, false)
            .unwrap();

        // Restore the local generated adapter to reproduce the state left by
        // the previous update, then request the newer tag.
        let second_object = repo.find_object(second, None).unwrap();
        repo.checkout_tree(&second_object, None).unwrap();
        repo.set_head_detached(second).unwrap();
        UpdateOrchestrator::materialize_package_skill_adapter(&dir, "fixture", "1.1.0").unwrap();
        assert!(!GitService::is_file_tracked(&skill_md));

        metadata.current_version = "1.1.0".to_string();
        metadata.latest_version = Some("1.2.0".to_string());
        let updated =
            UpdateOrchestrator::update_skill_atomic(&metadata, Some("1.2.0".to_string()), false)
                .await
                .unwrap();

        assert_eq!(updated.current_version, "1.2.0");
        let checked_out_manifest = fs::read_to_string(&skill_md).unwrap();
        assert!(checked_out_manifest.contains("# Upstream skill"));
        assert!(!checked_out_manifest.contains("generated-by: SkillSync"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preserves_a_user_owned_skill_manifest_during_adapter_cleanup() {
        let dir = fixture_dir("user-manifest");
        fs::create_dir_all(&dir).unwrap();
        let skill_md = dir.join("SKILL.md");
        fs::write(&skill_md, "---\nname: custom\n---\n# User content\n").unwrap();

        assert!(!UpdateOrchestrator::remove_generated_skill_adapter(&dir).unwrap());
        assert!(skill_md.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn integrity_check_rejects_malformed_skill_json() {
        let dir = fixture_dir("corrupted-json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("skill.json"), "{invalid").unwrap();

        assert!(!UpdateOrchestrator::verify_integrity(
            &ManagedItemType::Skill,
            &dir
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn updates_a_git_backed_plugin_and_keeps_its_manifest_valid() {
        let dir = fixture_dir("plugin-update");
        fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        let manifest = dir.join(".claude-plugin/plugin.json");
        fs::write(&manifest, r#"{"name":"superpowers","version":"1.0.0"}"#).unwrap();

        let signature = git2::Signature::now("SkillSync test", "tests@example.invalid").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(Path::new(".claude-plugin/plugin.json"))
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let first = repo
            .commit(Some("HEAD"), &signature, &signature, "v1", &tree, &[])
            .unwrap();
        let first_object = repo.find_object(first, None).unwrap();
        repo.tag_lightweight("v1.0.0", &first_object, false)
            .unwrap();

        fs::write(&manifest, r#"{"name":"superpowers","version":"1.1.0"}"#).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(Path::new(".claude-plugin/plugin.json"))
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "v1.1",
                &tree,
                &[&parent],
            )
            .unwrap();
        let second_object = repo.find_object(second, None).unwrap();
        repo.tag_lightweight("v1.1.0", &second_object, false)
            .unwrap();

        let mut metadata = metadata_for(dir.clone());
        metadata.id = "plugin-superpowers".to_string();
        metadata.item_type = ManagedItemType::Plugin;
        metadata.name = "superpowers".to_string();
        metadata.latest_version = Some("1.1.0".to_string());

        let updated =
            UpdateOrchestrator::update_skill_atomic(&metadata, Some("1.1.0".to_string()), false)
                .await
                .unwrap();

        assert_eq!(updated.current_version, "1.1.0");
        assert!(ManagedManifest::validate_plugin(&dir).is_ok());
        assert!(fs::read_to_string(&manifest).unwrap().contains("1.1.0"));
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn refuses_a_non_git_mcp_update_without_writing_to_the_manifest() {
        let dir = fixture_dir("mcp-no-package-manager");
        fs::create_dir_all(&dir).unwrap();
        let original = r#"{"mcpServers":{"demo":{"command":"node"}}}"#;
        fs::write(dir.join("mcp.json"), original).unwrap();
        let mut metadata = metadata_for(dir.clone());
        metadata.item_type = ManagedItemType::Mcp;

        let error = UpdateOrchestrator::update_skill_atomic(&metadata, None, false)
            .await
            .expect_err("a generic MCP config must not be changed with a guessed package manager");

        assert!(matches!(error, SkillSyncError::UnsupportedUpdateMethod(_)));
        assert_eq!(fs::read_to_string(dir.join("mcp.json")).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }
}
