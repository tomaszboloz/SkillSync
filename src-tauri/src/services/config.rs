use crate::errors::SkillSyncError;
use crate::models::config::{AppConfig, MonitoredPathType};
use crate::services::codex_plugin::CodexPluginService;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

pub struct ConfigService;

impl ConfigService {
    pub fn get_config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        let app_dir = base.join("SkillSync");
        let _ = fs::create_dir_all(&app_dir);
        app_dir.join("config.json")
    }

    pub fn load_config() -> AppConfig {
        let path = Self::get_config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&content) {
                    let discovered = crate::models::config::PathsConfig::discover_default_paths();
                    let mut changed = false;
                    for disc in discovered {
                        if !cfg.paths.monitored.iter().any(|m| m.path == disc.path) {
                            cfg.paths.monitored.push(disc);
                            changed = true;
                        }
                    }
                    changed |= Self::sanitize_monitored_paths(&mut cfg);
                    if changed {
                        let _ = Self::save_config(&cfg);
                    }
                    return cfg;
                }
            }
        }
        let default_config = AppConfig::default();
        let _ = Self::save_config(&default_config);
        default_config
    }

    pub fn save_config(config: &AppConfig) -> Result<(), SkillSyncError> {
        let mut config = config.clone();
        Self::sanitize_monitored_paths(&mut config);
        let path = Self::get_config_path();
        let tmp_path = path.with_extension("json.tmp");

        let json_str = serde_json::to_string_pretty(&config)
            .map_err(|e| SkillSyncError::Config(e.to_string()))?;

        let mut file = File::create(&tmp_path)?;
        file.write_all(json_str.as_bytes())?;
        file.sync_all()?;

        // Atomic replace
        fs::rename(tmp_path, path)?;
        Ok(())
    }

    /// Remove paths that can never be safely updated by SkillSync and collapse
    /// aliases before they reach discovery. Codex's historical versioned cache
    /// is owned by Codex (not Git) and is represented by the plugin registry;
    /// keeping every hash directory in `monitored` made the UI show stale
    /// resources and route updates through the generic Skill adapter.
    fn sanitize_monitored_paths(config: &mut AppConfig) -> bool {
        let before = config.paths.monitored.len();
        config.paths.monitored.retain(|monitored| {
            !(monitored.item_type == MonitoredPathType::Skill
                && CodexPluginService::is_legacy_cache_path(&monitored.path))
        });

        let mut seen = HashSet::new();
        config.paths.monitored.retain(|monitored| {
            let canonical =
                fs::canonicalize(&monitored.path).unwrap_or_else(|_| monitored.path.clone());
            let key = format!(
                "{}|{}",
                canonical.to_string_lossy().to_lowercase(),
                monitored.item_type.item_type_key()
            );
            seen.insert(key)
        });

        before != config.paths.monitored.len()
    }
}

trait MonitoredPathTypeKey {
    fn item_type_key(&self) -> &'static str;
}

impl MonitoredPathTypeKey for MonitoredPathType {
    fn item_type_key(&self) -> &'static str {
        match self {
            MonitoredPathType::Skill => "skill",
            MonitoredPathType::Mcp => "mcp",
            MonitoredPathType::Plugin => "plugin",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config::{MonitoredPath, PathsConfig};

    #[test]
    fn monitored_path_type_key_is_stable_for_deduplication() {
        assert_eq!(MonitoredPathType::Skill.item_type_key(), "skill");
        assert_eq!(MonitoredPathType::Mcp.item_type_key(), "mcp");
        assert_eq!(MonitoredPathType::Plugin.item_type_key(), "plugin");
    }

    #[test]
    fn sanitization_does_not_drop_non_codex_paths() {
        let mut config = AppConfig {
            paths: PathsConfig {
                monitored: vec![MonitoredPath {
                    id: "fixture".to_string(),
                    path: std::path::Path::new("/tmp/skillsync-fixture-skills").to_path_buf(),
                    scope: "global".to_string(),
                    item_type: MonitoredPathType::Skill,
                    custom_label: None,
                    enabled: true,
                }],
                default_install_directory: std::env::temp_dir(),
            },
            ..AppConfig::default()
        };
        assert!(!ConfigService::sanitize_monitored_paths(&mut config));
        assert_eq!(config.paths.monitored.len(), 1);
    }
}
