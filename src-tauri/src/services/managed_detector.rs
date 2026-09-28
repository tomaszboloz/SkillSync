use crate::models::config::{MonitoredPath, MonitoredPathType};
use crate::models::skill::{AgentScope, ManagedItemType, SkillMetadata, SkillStatus};
use crate::services::claude_plugin::ClaudePluginService;
use crate::services::git::GitService;
use crate::services::github::GitHubService;
use crate::services::managed_manifest::{ManagedManifest, ManagedManifestKind};
use crate::services::mcp::McpService;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const UNKNOWN_MANAGED_VERSION: &str = "unknown";

pub struct ManagedItemDetector;

impl ManagedItemDetector {
    pub fn scan_paths(paths: &[MonitoredPath]) -> Vec<SkillMetadata> {
        let mut items = HashMap::<String, SkillMetadata>::new();

        for monitored in paths
            .iter()
            .filter(|path| path.enabled && path.path.exists())
        {
            let item_type = Self::item_type_for(&monitored.item_type);
            if item_type == ManagedItemType::Skill {
                continue;
            }
            let mut entries = WalkDir::new(&monitored.path)
                .follow_links(true)
                .max_depth(4)
                .into_iter()
                .filter_entry(|entry| !Self::is_ignored_directory(entry.file_name()));
            while let Some(entry) = entries.next() {
                let Ok(entry) = entry else {
                    continue;
                };
                if !entry.file_type().is_dir() {
                    continue;
                }
                let path = entry.path();
                // Claude Code retains older cache versions after an update.
                // Only its installed_plugins.json registry may select the
                // active cache entry; displaying stale versions leads to a
                // guaranteed failed update against an unmanaged directory.
                if item_type == ManagedItemType::Plugin
                    && ClaudePluginService::is_claude_cache_path(path)
                    && ClaudePluginService::installation_for_path(path).is_none()
                {
                    continue;
                }
                let Ok(manifest) = ManagedManifest::validate(&item_type, path) else {
                    continue;
                };
                let candidate = Self::metadata_for(path, monitored, item_type.clone(), manifest);
                // Two marketplaces may publish an item with the same display
                // name. Their locations must not be merged into one update
                // transaction because each registry owns a different source.
                let key = Self::discovery_key(&candidate);
                if let Some(existing) = items.get_mut(&key) {
                    Self::add_location(existing, path);
                    if existing.remote_url.is_none() {
                        existing.remote_url = candidate.remote_url;
                    }
                } else {
                    items.insert(key, candidate);
                }
                // Like SKILL.md, a valid MCP/plugin manifest denotes one
                // installable root. Do not descend into its bundled skills,
                // fixtures, examples, or nested plugin definitions and turn
                // them into extra top-level cards.
                entries.skip_current_dir();
            }
        }

        let mut values: Vec<_> = items.into_values().collect();
        Self::disambiguate_duplicate_ids(&mut values);
        values.sort_by_key(|item| item.name.to_lowercase());
        values
    }

    fn discovery_key(item: &SkillMetadata) -> String {
        let source = item
            .remote_url
            .as_deref()
            .and_then(GitHubService::normalize_github_repository_url)
            .unwrap_or_else(|| {
                item.remote_url
                    .as_deref()
                    .unwrap_or("local")
                    .trim()
                    .trim_end_matches(".git")
                    .to_lowercase()
            });
        format!("{}|{}", item.id, source.to_lowercase())
    }

    fn disambiguate_duplicate_ids(items: &mut [SkillMetadata]) {
        let mut groups = HashMap::<String, Vec<usize>>::new();
        for (index, item) in items.iter().enumerate() {
            groups.entry(item.id.clone()).or_default().push(index);
        }
        for indexes in groups.values_mut().filter(|indexes| indexes.len() > 1) {
            indexes.sort_by_key(|index| {
                items[*index]
                    .remote_url
                    .as_deref()
                    .map(|url| {
                        GitHubService::normalize_github_repository_url(url)
                            .unwrap_or_else(|| url.trim().trim_end_matches(".git").to_string())
                            .to_lowercase()
                    })
                    .unwrap_or_default()
            });
            let base_id = items[indexes[0]].id.clone();
            let mut used = std::collections::HashSet::from([base_id.clone()]);
            for index in indexes.iter().skip(1) {
                let source = items[*index]
                    .remote_url
                    .as_deref()
                    .and_then(GitHubService::parse_github_owner_repo)
                    .map(|(owner, repo)| format!("{owner}-{repo}"))
                    .unwrap_or_else(|| format!("local-{}", index));
                let slug = source
                    .chars()
                    .map(|character| {
                        if character.is_ascii_alphanumeric() {
                            character.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>()
                    .split('-')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                let mut candidate = format!("{base_id}-{slug}");
                let mut suffix = 2;
                while !used.insert(candidate.clone()) {
                    candidate = format!("{base_id}-{slug}-{suffix}");
                    suffix += 1;
                }
                items[*index].id = candidate;
            }
        }
    }

    fn metadata_for(
        path: &Path,
        monitored: &MonitoredPath,
        item_type: ManagedItemType,
        manifest: ManagedManifestKind,
    ) -> SkillMetadata {
        let folder_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "managed-item".to_string());
        let (name, version, description, author) = match manifest {
            ManagedManifestKind::McpConfig => Self::mcp_config_details(path, &folder_name),
            ManagedManifestKind::LaravelBoost => Self::composer_details(path, &folder_name),
            ManagedManifestKind::Plugin => Self::plugin_details(path, &folder_name),
            ManagedManifestKind::Skill(_) => unreachable!("skills are handled by SkillDetector"),
        };
        let is_git_repo = GitService::is_git_repository(path);
        let version = if version == UNKNOWN_MANAGED_VERSION && is_git_repo {
            GitService::get_head_tag(path).unwrap_or(version)
        } else {
            version
        };
        let item_label = match item_type {
            ManagedItemType::Mcp => "MCP",
            ManagedItemType::Plugin => "Plugin",
            ManagedItemType::Skill => "Skill",
        };

        SkillMetadata {
            id: format!(
                "{}-{}",
                item_label.to_lowercase(),
                name.replace(' ', "-").to_lowercase()
            ),
            item_type,
            name,
            description,
            current_version: version.trim_start_matches(['v', 'V']).to_string(),
            latest_version: None,
            author,
            path: path.to_path_buf(),
            is_git_repo,
            remote_url: Self::resolve_remote_url(path),
            branch_or_tag: GitService::get_current_ref_name(path),
            detected_branch: GitService::get_current_branch_name(path),
            branch_override: None,
            agent_scope: Self::scope_for(&monitored.scope),
            status: SkillStatus::UpToDate,
            update_available: false,
            changelog: None,
            dependencies: vec![],
            permissions: vec![],
            last_checked: chrono::Utc::now(),
            compatibility: Some(format!("{} • manifest zweryfikowany", item_label)),
            update_compatibility: None,
            installed_locations: vec![path.to_path_buf()],
        }
    }

    fn mcp_config_details(path: &Path, fallback: &str) -> (String, String, String, String) {
        for filename in ["mcp.json", ".mcp.json"] {
            let Ok(content) = fs::read_to_string(path.join(filename)) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
                continue;
            };
            if let Some((name, _)) = value
                .get("mcpServers")
                .and_then(serde_json::Value::as_object)
                .and_then(|servers| servers.iter().next())
            {
                return (
                    name.to_string(),
                    UNKNOWN_MANAGED_VERSION.to_string(),
                    format!("MCP server configured in {filename}"),
                    "Local configuration".to_string(),
                );
            }
        }
        (
            fallback.to_string(),
            UNKNOWN_MANAGED_VERSION.to_string(),
            "MCP server configuration".to_string(),
            "Local configuration".to_string(),
        )
    }

    fn composer_details(path: &Path, fallback: &str) -> (String, String, String, String) {
        let value = Self::read_json(path.join("composer.json"));
        if McpService::is_laravel_boost_project(path) {
            return (
                "Laravel Boost".to_string(),
                McpService::laravel_boost_version(path)
                    .unwrap_or_else(|| UNKNOWN_MANAGED_VERSION.to_string()),
                "Laravel MCP integration managed by Composer".to_string(),
                "Laravel".to_string(),
            );
        }
        let name = value
            .as_ref()
            .and_then(|value| value.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback)
            .to_string();
        let version = value
            .as_ref()
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(UNKNOWN_MANAGED_VERSION)
            .to_string();
        let description = value
            .as_ref()
            .and_then(|value| value.get("description"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Laravel MCP integration")
            .to_string();
        (name, version, description, "Laravel community".to_string())
    }

    fn plugin_details(path: &Path, fallback: &str) -> (String, String, String, String) {
        for relative in [
            ".claude-plugin/plugin.json",
            ".codex-plugin/plugin.json",
            ".cursor-plugin/plugin.json",
            "plugin.json",
        ] {
            let Some(value) = Self::read_json(path.join(relative)) else {
                continue;
            };
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(fallback)
                .to_string();
            let version = value
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(UNKNOWN_MANAGED_VERSION)
                .to_string();
            let description = value
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Agent plugin")
                .to_string();
            let author = value
                .get("author")
                .and_then(|author| author.get("name").or(Some(author)))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Plugin author")
                .to_string();
            return (name, version, description, author);
        }
        unreachable!("validated plugin must have a plugin manifest")
    }

    fn read_json(path: PathBuf) -> Option<serde_json::Value> {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    }

    fn resolve_remote_url(path: &Path) -> Option<String> {
        if McpService::is_laravel_boost_project(path) {
            return Some("https://github.com/laravel/boost".to_string());
        }
        GitService::get_remote_url(path).or_else(|| {
            path.parent()
                .and_then(GitService::get_remote_url)
                .or_else(|| {
                    for candidate in [
                        "composer.json",
                        "package.json",
                        "plugin.json",
                        "marketplace.json",
                        ".claude-plugin/plugin.json",
                        ".codex-plugin/plugin.json",
                        ".cursor-plugin/plugin.json",
                    ] {
                        let Some(document) = Self::read_json(path.join(candidate)) else {
                            continue;
                        };
                        for key in ["repository", "homepage"] {
                            if let Some(url) = Self::github_url_from_value(document.get(key)) {
                                return Some(url);
                            }
                        }
                        if let Some(url) = document
                            .get("support")
                            .and_then(|support| support.get("source"))
                            .and_then(serde_json::Value::as_str)
                        {
                            if let Some(url) = GitHubService::normalize_github_repository_url(url) {
                                return Some(url);
                            }
                        }
                    }

                    // Claude Code marketplace checkouts are deliberately not
                    // Git repositories. Their canonical source is recorded in
                    // known_marketplaces.json next to the marketplace folder.
                    let marketplace = path.ancestors().find(|ancestor| {
                        ancestor
                            .file_name()
                            .is_some_and(|name| name == "marketplaces")
                            && ancestor
                                .parent()
                                .and_then(Path::parent)
                                .and_then(Path::file_name)
                                .is_some_and(|name| name == ".claude")
                    });
                    if let Some(marketplaces) = marketplace {
                        let registry =
                            Self::read_json(marketplaces.parent()?.join("known_marketplaces.json"));
                        if let Some(repository) = registry
                            .as_ref()
                            .and_then(serde_json::Value::as_object)
                            .and_then(|entries| {
                                entries.values().find_map(|entry| {
                                    let install = entry
                                        .get("installLocation")
                                        .and_then(serde_json::Value::as_str)?;
                                    let canonical = fs::canonicalize(install).ok()?;
                                    let current = fs::canonicalize(path).ok()?;
                                    (canonical == current).then(|| {
                                        entry.get("source").and_then(|source| source.get("repo"))
                                    })
                                })
                            })
                            .flatten()
                            .and_then(|value| Self::github_url_from_value(Some(value)))
                        {
                            return Some(repository);
                        }
                    }
                    None
                })
        })
    }

    fn github_url_from_value(value: Option<&serde_json::Value>) -> Option<String> {
        let raw = match value? {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Object(object) => object
                .get("url")
                .or_else(|| object.get("directory"))
                .or_else(|| object.get("repo"))
                .and_then(serde_json::Value::as_str)?
                .to_string(),
            _ => return None,
        };
        GitHubService::normalize_github_repository_url(&raw)
    }

    fn item_type_for(path_type: &MonitoredPathType) -> ManagedItemType {
        match path_type {
            MonitoredPathType::Skill => ManagedItemType::Skill,
            MonitoredPathType::Mcp => ManagedItemType::Mcp,
            MonitoredPathType::Plugin => ManagedItemType::Plugin,
        }
    }

    fn scope_for(scope: &str) -> AgentScope {
        match scope.to_lowercase().as_str() {
            "codex" => AgentScope::Codex,
            "claude" => AgentScope::Claude,
            "cursor" => AgentScope::Cursor,
            "antigravity" => AgentScope::Antigravity,
            "global" | "agents" => AgentScope::Global,
            other => AgentScope::Custom(other.to_string()),
        }
    }

    fn add_location(item: &mut SkillMetadata, path: &Path) {
        let path = path.to_path_buf();
        if !item.installed_locations.contains(&path) {
            item.installed_locations.push(path);
        }
    }

    fn is_ignored_directory(name: &std::ffi::OsStr) -> bool {
        matches!(
            name.to_string_lossy().as_ref(),
            ".git" | "node_modules" | "target" | "dist" | "vendor"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config::{MonitoredPath, MonitoredPathType};

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "skillsync-managed-detector-{name}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn monitored(path: PathBuf, item_type: MonitoredPathType) -> MonitoredPath {
        MonitoredPath {
            id: "test".to_string(),
            path,
            scope: "claude".to_string(),
            item_type,
            custom_label: None,
            enabled: true,
        }
    }

    #[test]
    fn finds_a_superpowers_style_plugin_without_treating_its_skills_as_plugins() {
        let path = root("plugin");
        fs::create_dir_all(path.join("superpowers/.claude-plugin")).unwrap();
        fs::create_dir_all(path.join("superpowers/skills/tdd")).unwrap();
        fs::write(
            path.join("superpowers/.claude-plugin/plugin.json"),
            r#"{"name":"superpowers","version":"6.3.0","description":"TDD"}"#,
        )
        .unwrap();
        fs::write(path.join("superpowers/skills/tdd/SKILL.md"), "# skill").unwrap();

        let items =
            ManagedItemDetector::scan_paths(&[monitored(path.clone(), MonitoredPathType::Plugin)]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, ManagedItemType::Plugin);
        assert_eq!(items[0].name, "superpowers");
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn a_valid_plugin_root_is_a_discovery_boundary_for_nested_plugin_manifests() {
        let path = root("nested-plugin");
        fs::create_dir_all(path.join("outer/.claude-plugin")).unwrap();
        fs::create_dir_all(path.join("outer/examples/embedded/.claude-plugin")).unwrap();
        fs::write(
            path.join("outer/.claude-plugin/plugin.json"),
            r#"{"name":"outer","version":"1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            path.join("outer/examples/embedded/.claude-plugin/plugin.json"),
            r#"{"name":"embedded","version":"0.1.0"}"#,
        )
        .unwrap();

        let items =
            ManagedItemDetector::scan_paths(&[monitored(path.clone(), MonitoredPathType::Plugin)]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "outer");
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn finds_laravel_boost_as_an_mcp_integration() {
        let path = root("boost");
        fs::create_dir_all(path.join("boost")).unwrap();
        fs::write(
            path.join("boost/composer.json"),
            r#"{"name":"laravel/boost","description":"Laravel MCP","require":{"laravel/mcp":"^1.0"},"support":{"source":"https://github.com/laravel/boost"}}"#,
        )
        .unwrap();

        let items =
            ManagedItemDetector::scan_paths(&[monitored(path.clone(), MonitoredPathType::Mcp)]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, ManagedItemType::Mcp);
        assert_eq!(
            items[0].remote_url.as_deref(),
            Some("https://github.com/laravel/boost")
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn uses_the_locked_boost_version_and_official_upstream_for_a_composer_project() {
        let path = root("boost-project");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("composer.json"),
            r#"{"name":"acme/app","require":{"laravel/boost":"^1.0"}}"#,
        )
        .unwrap();
        fs::write(
            path.join("composer.lock"),
            r#"{"packages":[{"name":"laravel/boost","version":"v1.4.2"}]}"#,
        )
        .unwrap();

        let items =
            ManagedItemDetector::scan_paths(&[monitored(path.clone(), MonitoredPathType::Mcp)]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Laravel Boost");
        assert_eq!(items[0].current_version, "1.4.2");
        assert_eq!(
            items[0].remote_url.as_deref(),
            Some("https://github.com/laravel/boost")
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn resolves_the_github_source_for_a_registered_marketplace_without_git() {
        let path = root("marketplace-source");
        let plugins = path.join(".claude/plugins");
        let marketplace = plugins.join("marketplaces/n8n-mcp-skills");
        fs::create_dir_all(&marketplace).unwrap();
        fs::write(
            marketplace.join("plugin.json"),
            r#"{"name":"n8n-mcp-skills","version":"1.27.3","repository":"https://github.com/czlonkowski/n8n-skills"}"#,
        )
        .unwrap();
        fs::write(
            plugins.join("known_marketplaces.json"),
            serde_json::json!({
                "n8n-mcp-skills": {
                    "source": {"source": "github", "repo": "czlonkowski/n8n-skills"},
                    "installLocation": marketplace,
                }
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(
            ManagedItemDetector::resolve_remote_url(&marketplace).as_deref(),
            Some("https://github.com/czlonkowski/n8n-skills")
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn keeps_same_named_plugins_from_different_sources_independent() {
        let path = root("same-plugin-name");
        for (folder, remote) in [
            ("first", "https://github.com/example/first-plugin"),
            ("second", "https://github.com/example/second-plugin"),
        ] {
            let plugin = path.join(folder);
            fs::create_dir_all(plugin.join(".claude-plugin")).unwrap();
            fs::write(
                plugin.join(".claude-plugin/plugin.json"),
                format!(r#"{{"name":"shared","version":"1.0.0","repository":"{remote}"}}"#),
            )
            .unwrap();
        }

        let items =
            ManagedItemDetector::scan_paths(&[monitored(path.clone(), MonitoredPathType::Plugin)]);
        assert_eq!(items.len(), 2);
        assert_ne!(items[0].id, items[1].id);
        let _ = fs::remove_dir_all(path);
    }
}
