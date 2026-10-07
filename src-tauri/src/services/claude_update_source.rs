//! Match the source used by `claude plugin update`, not unrelated Git tags.
use super::{claude_plugin::ClaudePluginService, git::GitService, github::GitHubService};
use crate::models::skill::{ManagedItemType, SkillMetadata, SkillStatus};
use std::{fs, path::Path};

#[cfg(test)]
#[path = "claude_update_source_tests.rs"]
mod tests;

pub(crate) fn pinned_source(root: &Path, identifier: &str) -> Option<(String, String)> {
    let (plugin, marketplace) = identifier.split_once('@')?;
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("known_marketplaces.json")).ok()?)
            .ok()?;
    let location = registry
        .get(marketplace)?
        .get("installLocation")?
        .as_str()?;
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(location).join(".claude-plugin/marketplace.json")).ok()?,
    )
    .ok()?;
    let entry = manifest
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(|name| name.as_str()) == Some(plugin))?;
    let source = entry.get("source")?;
    if source.get("source")?.as_str()? != "url" {
        return None;
    }
    let remote = GitHubService::normalize_github_repository_url(source.get("url")?.as_str()?)?;
    let sha = source.get("sha")?.as_str()?;
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((remote, sha.to_ascii_lowercase()))
}

pub(crate) async fn refresh(mut skill: SkillMetadata) -> Option<SkillMetadata> {
    if skill.item_type != ManagedItemType::Plugin {
        return None;
    }
    let installation = ClaudePluginService::installation_for_path(&skill.path)?;
    let (remote, sha) = pinned_source(&installation.plugins_root, &installation.identifier)?;
    let head = GitService::get_head_commit(&skill.path)?;
    skill.branch_or_tag = GitService::get_current_ref_name(&skill.path);
    skill.last_checked = chrono::Utc::now();
    if head.eq_ignore_ascii_case(&sha) {
        skill.latest_version = Some(skill.current_version.clone());
        skill.update_available = false;
        skill.status = SkillStatus::UpToDate;
        skill.update_compatibility = Some(
            "Instalacja odpowiada commitowi przypiętemu przez marketplace Claude Code.".into(),
        );
        return Some(skill);
    }
    let result = async {
        let (owner, repo) = GitHubService::parse_github_owner_repo(&remote)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .ok()?;
        let mut response = client
            .get(format!(
                "https://raw.githubusercontent.com/{owner}/{repo}/{sha}/.claude-plugin/plugin.json"
            ))
            .header("User-Agent", "SkillSync-Desktop")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 64 * 1024 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        if value.get("name")?.as_str()? != skill.name {
            return None;
        }
        let version = value.get("version")?.as_str()?;
        semver::Version::parse(version.trim_start_matches(['v', 'V'])).ok()?;
        Some(version.to_owned())
    }
    .await;
    if let Some(version) = result {
        skill.latest_version = Some(version);
        skill.update_available = true;
        skill.status = SkillStatus::UpdateAvailable;
        skill.update_compatibility = Some("Marketplace Claude Code wskazuje inny commit; aktualizacja użyje oficjalnego adaptera.".into());
    } else {
        skill.update_available = false;
        skill.status = SkillStatus::Error(
            "Nie można potwierdzić manifestu pluginu dla commitu marketplace Claude Code.".into(),
        );
    }
    Some(skill)
}
