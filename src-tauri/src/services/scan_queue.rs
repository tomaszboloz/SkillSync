use crate::commands::skills::apply_github_release;
use crate::models::skill::{SkillMetadata, SkillStatus};
use crate::services::git::GitService;
use crate::services::github::GitHubService;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(350);

struct ScanJob {
    generation: u64,
    app: AppHandle,
    skill: SkillMetadata,
}

/// A single background worker deliberately spaces remote checks. Discovery is
/// quick and local; upstream verification is serial so a large install never
/// creates a GitHub/Git connection burst.
pub struct ScanQueue {
    sender: UnboundedSender<ScanJob>,
    generation: Arc<AtomicU64>,
}

impl ScanQueue {
    pub fn new() -> Self {
        let (sender, mut receiver) = unbounded_channel::<ScanJob>();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&generation);

        tauri::async_runtime::spawn(async move {
            let mut active_generation = None;
            let mut generation_cache = HashMap::<String, SkillMetadata>::new();
            while let Some(job) = receiver.recv().await {
                if worker_generation.load(Ordering::Acquire) != job.generation {
                    continue;
                }
                if active_generation != Some(job.generation) {
                    active_generation = Some(job.generation);
                    generation_cache.clear();
                }

                let cache_key = upstream_cache_key(&job.skill);
                let (skill, checked_upstream) =
                    if let Some(cached) = generation_cache.get(&cache_key) {
                        (reuse_upstream_result(job.skill, cached), false)
                    } else {
                        let skill = refresh_upstream(job.skill).await;
                        generation_cache.insert(cache_key, skill.clone());
                        (skill, true)
                    };
                if worker_generation.load(Ordering::Acquire) == job.generation {
                    let _ = job.app.emit("scan-result", skill);
                }
                if checked_upstream {
                    tokio::time::sleep(REQUEST_INTERVAL).await;
                }
            }
        });

        Self { sender, generation }
    }

    pub fn enqueue(&self, app: AppHandle, skills: impl IntoIterator<Item = SkillMetadata>) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        for skill in skills {
            if skill.remote_url.is_some() {
                let _ = self.sender.send(ScanJob {
                    generation,
                    app: app.clone(),
                    skill,
                });
            }
        }
    }
}

fn upstream_cache_key(skill: &SkillMetadata) -> String {
    format!(
        "{}|{}|{}|{}|{:?}|{}|{}",
        skill.remote_url.as_deref().unwrap_or_default().trim(),
        skill.current_version,
        skill.branch_override.as_deref().unwrap_or_default().trim(),
        skill.branch_or_tag.as_deref().unwrap_or_default().trim(),
        skill.item_type,
        skill.is_git_repo,
        if skill.item_type == crate::models::skill::ManagedItemType::Plugin {
            skill.path.to_str().unwrap_or_default()
        } else if !skill.is_git_repo
            && skill.item_type == crate::models::skill::ManagedItemType::Skill
        {
            skill.name.as_str()
        } else {
            ""
        }
    )
}

fn reuse_upstream_result(mut target: SkillMetadata, cached: &SkillMetadata) -> SkillMetadata {
    target.latest_version = cached.latest_version.clone();
    target.status = cached.status.clone();
    target.update_available = cached.update_available;
    target.changelog = cached.changelog.clone();
    target.update_compatibility = cached.update_compatibility.clone();
    target.last_checked = cached.last_checked;
    target.branch_or_tag = cached.branch_or_tag.clone();
    target
}

impl Default for ScanQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) async fn refresh_upstream(mut skill: SkillMetadata) -> SkillMetadata {
    if let Some(owned) = super::claude_update_source::refresh(skill.clone()).await {
        return owned;
    }
    // A manually selected branch is an explicit opt-out from release tags.
    // Automatically detected branches must never have that effect: a
    // repository publishing SemVer tags is tracked by its newest tag.
    if skill.is_git_repo {
        if let Some(branch) = skill
            .branch_override
            .clone()
            .filter(|branch| !branch.trim().is_empty())
        {
            return refresh_branch_upstream(skill, branch).await;
        }
    }

    if skill.is_git_repo {
        let path = skill.path.clone();
        match tauri::async_runtime::spawn_blocking(move || GitService::get_latest_remote_tag(&path))
            .await
        {
            Ok(Ok(Some(tag))) => {
                let clean_tag = tag.trim_start_matches(['v', 'V']).to_string();
                skill.branch_or_tag = Some(tag.clone());
                skill.latest_version = Some(clean_tag.clone());
                skill.update_available =
                    GitHubService::is_newer_version(&clean_tag, &skill.current_version);
                skill.status = if skill.update_available {
                    SkillStatus::UpdateAvailable
                } else {
                    SkillStatus::UpToDate
                };
                skill.update_compatibility = Some(GitHubService::get_semver_compatibility(
                    &clean_tag,
                    &skill.current_version,
                ));
                skill.last_checked = chrono::Utc::now();
                return skill;
            }
            Ok(Ok(None)) => {
                let path = skill.path.clone();
                let detected_branch = skill.detected_branch.clone();
                match tauri::async_runtime::spawn_blocking(move || {
                    GitService::resolve_fallback_branch(&path, detected_branch.as_deref())
                })
                .await
                {
                    Ok(Ok(branch)) => return refresh_branch_upstream(skill, branch).await,
                    Ok(Err(error)) => {
                        skill.status = SkillStatus::Error(format!(
                            "Repozytorium nie ma tagu SemVer ani dostępnej gałęzi do śledzenia: {}",
                            error
                        ));
                    }
                    Err(error) => {
                        skill.status = SkillStatus::Error(format!(
                            "Kolejka sprawdzania gałęzi została przerwana: {}",
                            error
                        ));
                    }
                }
                skill.update_available = false;
                skill.last_checked = chrono::Utc::now();
                return skill;
            }
            Ok(Err(error)) => {
                skill.status =
                    SkillStatus::Error(format!("Nie udało się sprawdzić tagów Git: {}", error));
                skill.update_available = false;
                skill.last_checked = chrono::Utc::now();
                return skill;
            }
            Err(error) => {
                skill.status = SkillStatus::Error(format!(
                    "Kolejka sprawdzania tagów została przerwana: {}",
                    error
                ));
                skill.update_available = false;
                skill.last_checked = chrono::Utc::now();
                return skill;
            }
        }
    }

    if let Some(remote_url) = skill.remote_url.clone() {
        match GitHubService::check_latest_version(&remote_url).await {
            Ok(Some(release)) => {
                apply_github_release(&mut skill, &release);
                skill = super::update_eligibility::verify(skill).await;
            }
            Ok(None) => {
                skill.status = SkillStatus::Error(
                    "Upstream nie udostępnia wersji ani tagu do porównania.".to_string(),
                );
                skill.update_available = false;
            }
            Err(error) => {
                skill.status = SkillStatus::Error(format!(
                    "Nie udało się sprawdzić aktualizacji GitHub: {}",
                    error
                ));
                skill.update_available = false;
            }
        }
    }
    skill.last_checked = chrono::Utc::now();
    skill
}

async fn refresh_branch_upstream(mut skill: SkillMetadata, branch: String) -> SkillMetadata {
    let path = skill.path.clone();
    let branch_for_check = branch.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        let current = GitService::get_head_commit(&path)
            .ok_or_else(|| "Nie można odczytać lokalnego commitu HEAD".to_string())?;
        let remote = GitService::get_remote_branch_commit(&path, &branch_for_check)
            .map_err(|error| error.to_string())?;
        let active_branch = GitService::get_current_branch_name(&path);
        Ok::<_, String>((current, remote, active_branch))
    })
    .await
    {
        Ok(Ok((current, remote, active_branch))) => {
            let branch_mismatch = active_branch.as_deref() != Some(branch.as_str());
            let commit_mismatch = current != remote;
            skill.branch_or_tag = Some(branch.clone());
            skill.latest_version = Some(branch.clone());
            skill.update_available = branch_mismatch || commit_mismatch;
            skill.status = if skill.update_available {
                SkillStatus::UpdateAvailable
            } else {
                SkillStatus::UpToDate
            };
            skill.update_compatibility = Some(if branch_mismatch {
                format!(
                    "Niezgodna gałąź: aktywna {:?}, wymagana {}; aktualizacja wyrówna checkout.",
                    active_branch.unwrap_or_else(|| "detached HEAD".to_string()),
                    branch
                )
            } else if commit_mismatch {
                format!(
                    "Gałąź {} ma nowszy commit upstream; aktualizacja wyrówna lokalny checkout.",
                    branch
                )
            } else {
                format!("Gałąź {} jest zgodna z upstream.", branch)
            });
        }
        Ok(Err(error)) => {
            skill.status = SkillStatus::Error(format!(
                "Nie udało się sprawdzić gałęzi {}: {}",
                branch, error
            ));
            skill.update_available = false;
        }
        Err(error) => {
            skill.status = SkillStatus::Error(format!(
                "Kolejka sprawdzania gałęzi została przerwana: {}",
                error
            ));
            skill.update_available = false;
        }
    }
    skill.last_checked = chrono::Utc::now();
    skill
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::skill::{AgentScope, ManagedItemType};
    use std::path::PathBuf;

    fn fixture(id: &str, name: &str, path: &str) -> SkillMetadata {
        SkillMetadata {
            item_type: ManagedItemType::Skill,
            id: id.to_string(),
            name: name.to_string(),
            display_name: None,
            description: format!("{name} description"),
            current_version: "1.0.0".to_string(),
            latest_version: None,
            author: "Tester".to_string(),
            path: PathBuf::from(path),
            is_git_repo: true,
            remote_url: Some("https://github.com/example/repo".to_string()),
            branch_or_tag: Some("a1b2c3d".to_string()),
            detected_branch: None,
            branch_override: None,
            agent_scope: AgentScope::Global,
            status: SkillStatus::Checking,
            update_available: false,
            changelog: None,
            dependencies: vec![],
            permissions: vec![],
            last_checked: chrono::Utc::now(),
            compatibility: None,
            update_compatibility: None,
            installed_locations: vec![PathBuf::from(path)],
        }
    }

    #[test]
    fn same_source_version_and_ref_share_one_scan_check() {
        let first = fixture("skill-first", "first", "/skills/first");
        let second = fixture("skill-second", "second", "/skills/second");
        assert_eq!(upstream_cache_key(&first), upstream_cache_key(&second));

        let mut other_ref = second.clone();
        other_ref.branch_override = Some("main".to_string());
        assert_ne!(upstream_cache_key(&first), upstream_cache_key(&other_ref));
    }

    #[test]
    fn raw_manifest_eligibility_cache_is_skill_specific() {
        let mut first = fixture("a", "seo-audit", "/skills/a");
        let mut second = fixture("b", "seo-page", "/skills/b");
        first.is_git_repo = false;
        second.is_git_repo = false;
        assert_ne!(upstream_cache_key(&first), upstream_cache_key(&second));
        second.name = first.name.clone();
        assert_eq!(upstream_cache_key(&first), upstream_cache_key(&second));
    }

    #[test]
    fn plugin_owner_channels_do_not_share_a_cached_git_tag() {
        let mut first = fixture("a", "figma", "/claude/cache/figma");
        let mut second = fixture("b", "figma", "/codex/cache/figma");
        first.item_type = ManagedItemType::Plugin;
        second.item_type = ManagedItemType::Plugin;
        assert_ne!(upstream_cache_key(&first), upstream_cache_key(&second));
    }

    #[tokio::test]
    #[ignore = "explicit real-resource check; reads installations but never updates them"]
    async fn verifies_real_reported_resources_without_mutation() {
        let mut fastmcp = fixture("fastmcp", "fastmcp", "/unused");
        fastmcp.is_git_repo = false;
        fastmcp.remote_url = Some("https://github.com/PrefectHQ/fastmcp".into());
        fastmcp.latest_version = Some("4.0.11".into());
        fastmcp.update_available = true;
        let result = super::super::update_eligibility::verify(fastmcp).await;
        assert!(!result.update_available);
        assert!(matches!(result.status, SkillStatus::Error(_)));
        let mut seo = fixture("seo-audit", "seo-audit", "/unused");
        seo.is_git_repo = false;
        seo.remote_url = Some("https://github.com/AgriciDaniel/claude-seo".into());
        seo.latest_version = Some("2.4.2".into());
        seo.update_available = true;
        assert!(
            super::super::update_eligibility::verify(seo)
                .await
                .update_available
        );
        let path = dirs::home_dir()
            .unwrap()
            .join(".claude/plugins/cache/claude-plugins-official/figma/2.2.120");
        let mut figma = fixture("figma", "figma", path.to_str().unwrap());
        figma.item_type = ManagedItemType::Plugin;
        figma.current_version = "2.2.120".into();
        let result = super::super::claude_update_source::refresh(figma)
            .await
            .unwrap();
        assert!(!result.update_available);
        assert_eq!(result.latest_version.as_deref(), Some("2.2.120"));
        assert_eq!(result.status, SkillStatus::UpToDate);
    }

    #[test]
    fn reusing_a_check_keeps_the_skill_specific_metadata() {
        let first = fixture("skill-first", "first", "/skills/first");
        let mut checked = first.clone();
        checked.latest_version = Some("1.2.0".to_string());
        checked.status = SkillStatus::UpdateAvailable;
        checked.update_available = true;
        checked.branch_or_tag = Some("v1.2.0".to_string());

        let second = fixture("skill-second", "second", "/skills/second");
        let reused = reuse_upstream_result(second, &checked);

        assert_eq!(reused.id, "skill-second");
        assert_eq!(reused.name, "second");
        assert_eq!(reused.path, PathBuf::from("/skills/second"));
        assert_eq!(reused.latest_version.as_deref(), Some("1.2.0"));
        assert_eq!(reused.branch_or_tag.as_deref(), Some("v1.2.0"));
        assert!(reused.update_available);
        assert_eq!(reused.status, SkillStatus::UpdateAvailable);
    }
}
