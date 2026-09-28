use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MonitoredPathType {
    #[default]
    Skill,
    Mcp,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub paths: PathsConfig,
    pub updates: UpdatesConfig,
    pub notifications: NotificationsConfig,
    pub appearance: AppearanceConfig,
    pub advanced: AdvancedConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self {
            general: GeneralConfig {
                language: "pl".into(),
                launch_at_login: false,
                minimize_to_tray: true,
                show_tray_icon: true,
                check_app_updates: true,
            },
            paths: PathsConfig {
                monitored: PathsConfig::discover_default_paths(),
                default_install_directory: home.join(".agents/skills"),
            },
            updates: UpdatesConfig {
                auto_check_frequency: "every_6_hours".into(),
                auto_install: "ask".into(),
                concurrency_limit: 4,
                backup_retention_days: 14,
                allow_prerelease: false,
                branch_overrides: std::collections::HashMap::new(),
                repository_overrides: std::collections::HashMap::new(),
            },
            notifications: NotificationsConfig {
                enabled: true,
                on_update_found: true,
                on_update_success: true,
                on_update_failure: true,
                sound: true,
            },
            appearance: AppearanceConfig {
                theme: "dark".into(),
                accent_color: "violet".into(),
                reduced_motion: false,
                compact_view: false,
            },
            advanced: AdvancedConfig {
                log_level: "info".into(),
                git_timeout_seconds: 30,
                custom_git_binary: None,
                cache_ttl_minutes: 5,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoredPath {
    pub id: String,
    pub path: PathBuf,
    pub scope: String,
    #[serde(default)]
    pub item_type: MonitoredPathType,
    pub custom_label: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralConfig {
    pub language: String,
    pub launch_at_login: bool,
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
    pub check_app_updates: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathsConfig {
    pub monitored: Vec<MonitoredPath>,
    pub default_install_directory: PathBuf,
}

impl PathsConfig {
    pub fn discover_default_paths() -> Vec<MonitoredPath> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self::discover_default_paths_from(&home)
    }

    fn discover_default_paths_from(home: &std::path::Path) -> Vec<MonitoredPath> {
        let mut list = Vec::new();
        let mut idx = 1;

        let standard_candidates = vec![
            (
                home.join(".agents/skills"),
                "agents",
                "AI Agents Skills (~/.agents/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".codex/skills"),
                "codex",
                "OpenAI Codex Skills (~/.codex/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".claude/skills"),
                "claude",
                "Claude Desktop / Code (~/.claude/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".gemini/config/skills"),
                "antigravity",
                "Gemini CLI / Antigravity (~/.gemini/config/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".gemini/antigravity/builtin/skills"),
                "antigravity",
                "Gemini Built-in (~/.gemini/antigravity/builtin/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".cursor/skills"),
                "cursor",
                "Cursor AI Skills (~/.cursor/skills)",
                MonitoredPathType::Skill,
            ),
            (
                home.join(".config/skills"),
                "global",
                "Global CLI Skills (~/.config/skills)",
                MonitoredPathType::Skill,
            ),
        ];

        for (p, scope, label, item_type) in standard_candidates {
            if p.exists() {
                list.push(MonitoredPath {
                    id: format!("p{}", idx),
                    path: p,
                    scope: scope.to_string(),
                    item_type,
                    custom_label: Some(label.to_string()),
                    enabled: true,
                });
                idx += 1;
            }
        }

        // Check plugin directories containing skills
        let plugin_roots = vec![
            (home.join(".gemini/config/plugins"), "antigravity"),
            (home.join(".claude/plugins"), "claude"),
            (home.join(".codex/plugins"), "codex"),
            // Current Codex marketplace installations live in the managed
            // snapshot root below `.tmp`; the legacy `plugins/cache` tree is
            // handled as immutable history by the Codex adapter.
            (home.join(".codex/.tmp/plugins"), "codex"),
            (
                home.join(".cache/codex-runtimes/codex-primary-runtime/plugins"),
                "codex",
            ),
        ];

        for (root, scope) in plugin_roots {
            if !root.exists() {
                continue;
            }

            if list.iter().any(|known: &MonitoredPath| known.path == root) {
                continue;
            }
            list.push(MonitoredPath {
                id: format!("p{}", idx),
                path: root,
                scope: scope.to_string(),
                item_type: MonitoredPathType::Plugin,
                custom_label: Some("Agent plugins".to_string()),
                enabled: true,
            });
            idx += 1;
        }

        for (path, scope, label) in [
            (home.join(".mcp"), "global", "MCP Servers (~/.mcp)"),
            (
                home.join(".config/mcp"),
                "global",
                "MCP Servers (~/.config/mcp)",
            ),
            (
                home.join(".claude/mcp"),
                "claude",
                "Claude Code MCP Servers",
            ),
            (home.join(".codex/mcp"), "codex", "OpenAI Codex MCP Servers"),
        ] {
            if path.exists() && !list.iter().any(|known: &MonitoredPath| known.path == path) {
                list.push(MonitoredPath {
                    id: format!("p{}", idx),
                    path,
                    scope: scope.to_string(),
                    item_type: MonitoredPathType::Mcp,
                    custom_label: Some(label.to_string()),
                    enabled: true,
                });
                idx += 1;
            }
        }

        if list.is_empty() {
            list.push(MonitoredPath {
                id: "p1".into(),
                path: home.join(".claude/skills"),
                scope: "claude".into(),
                item_type: MonitoredPathType::Skill,
                custom_label: None,
                enabled: true,
            });
        }

        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_codex_skills_and_the_claude_plugin_root() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-paths-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let codex = root.join(".codex/skills");
        let plugin_root = root.join(".claude/plugins");
        let nested_plugin = plugin_root.join("cache/example/1.0.0/skills");
        fs::create_dir_all(&codex).unwrap();
        fs::create_dir_all(&nested_plugin).unwrap();

        let paths = PathsConfig::discover_default_paths_from(&root);

        assert!(paths
            .iter()
            .any(|path| path.path == codex && path.scope == "codex"));
        assert!(paths.iter().any(|path| path.path == plugin_root
            && path.scope == "claude"
            && path.item_type == MonitoredPathType::Plugin));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn loads_existing_settings_without_a_tray_or_tracking_override_field() {
        let mut legacy = serde_json::to_value(AppConfig::default()).unwrap();
        legacy["general"]
            .as_object_mut()
            .unwrap()
            .remove("showTrayIcon");
        legacy["updates"]
            .as_object_mut()
            .unwrap()
            .remove("branchOverrides");
        legacy["updates"]
            .as_object_mut()
            .unwrap()
            .remove("repositoryOverrides");

        let loaded: AppConfig = serde_json::from_value(legacy).unwrap();
        assert!(loaded.general.show_tray_icon);
        assert!(loaded.updates.branch_overrides.is_empty());
        assert!(loaded.updates.repository_overrides.is_empty());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesConfig {
    pub auto_check_frequency: String,
    pub auto_install: String,
    pub concurrency_limit: usize,
    pub backup_retention_days: u32,
    pub allow_prerelease: bool,
    /// Explicit branches selected by the user. They intentionally take
    /// precedence over the branch detected from the local Git worktree.
    #[serde(default)]
    pub branch_overrides: std::collections::HashMap<String, String>,
    /// Explicit GitHub repositories for portable/local resources without a
    /// discoverable Git remote. Stored separately from branches so a URL can
    /// never be mistaken for a branch name.
    #[serde(default)]
    pub repository_overrides: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsConfig {
    pub enabled: bool,
    pub on_update_found: bool,
    pub on_update_success: bool,
    pub on_update_failure: bool,
    pub sound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceConfig {
    pub theme: String,
    pub accent_color: String,
    pub reduced_motion: bool,
    pub compact_view: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedConfig {
    pub log_level: String,
    pub git_timeout_seconds: u64,
    pub custom_git_binary: Option<PathBuf>,
    pub cache_ttl_minutes: u64,
}
