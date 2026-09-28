use crate::errors::SkillSyncError;
use crate::services::managed_manifest::ManagedManifest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Metadata returned by the Codex plugin owner. Codex keeps plugin files in a
/// managed marketplace/cache and does not expose a Git worktree contract for
/// those files. SkillSync must therefore update them only through `codex`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPluginInstallation {
    pub plugin_id: String,
    pub name: String,
    pub marketplace: String,
    pub install_path: PathBuf,
    pub version: Option<String>,
}

pub struct CodexPluginService;

impl CodexPluginService {
    pub fn plugin_version(path: &Path) -> Option<String> {
        [
            ".claude-plugin/plugin.json",
            ".codex-plugin/plugin.json",
            ".cursor-plugin/plugin.json",
            "plugin.json",
        ]
        .iter()
        .find_map(|relative| {
            let content = fs::read_to_string(path.join(relative)).ok()?;
            let manifest = serde_json::from_str::<serde_json::Value>(&content).ok()?;
            manifest
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(|version| version.trim_start_matches(['v', 'V']).to_string())
        })
    }

    pub fn active_installations() -> Vec<CodexPluginInstallation> {
        Self::list_installations()
    }

    pub fn installation_for_path_from(
        path: &Path,
        installations: &[CodexPluginInstallation],
    ) -> Option<CodexPluginInstallation> {
        let path = fs::canonicalize(path).ok()?;
        installations
            .iter()
            .find(|installation| {
                fs::canonicalize(&installation.install_path)
                    .ok()
                    .is_some_and(|root| path == root || path.starts_with(root))
            })
            .cloned()
    }

    /// Returns the active Codex installation that owns `path`, if the path is
    /// present in the authoritative `codex plugin list --available --json`
    /// output. Stale versioned cache directories intentionally return `None`.
    pub fn installation_for_path(path: &Path) -> Option<CodexPluginInstallation> {
        Self::installation_for_path_from(path, &Self::list_installations())
    }

    pub fn is_legacy_cache_path(path: &Path) -> bool {
        dirs::home_dir()
            .map(|home| path.starts_with(home.join(".codex/plugins/cache")))
            .unwrap_or(false)
    }

    /// Historical Codex cache paths are immutable snapshots, not active
    /// plugin installations. They are excluded from discovery unless the
    /// current Codex registry explicitly points at the same path.
    pub fn is_inactive_cache_path(path: &Path) -> bool {
        let Some(home) = dirs::home_dir() else {
            return false;
        };
        let cache_root = home.join(".codex/plugins/cache");
        let Ok(canonical) = fs::canonicalize(path) else {
            return path.starts_with(&cache_root);
        };
        if !canonical.starts_with(&cache_root) {
            return false;
        }
        Self::list_installations()
            .iter()
            .all(|installation| !canonical.starts_with(&installation.install_path))
    }

    /// The Codex marketplace checkout contains every catalog entry, while
    /// `plugin list` marks only the installed entries. Do not present
    /// uninstalled catalog entries as managed resources.
    pub fn is_uninstalled_marketplace_entry(
        path: &Path,
        installations: &[CodexPluginInstallation],
    ) -> bool {
        let Some(home) = dirs::home_dir() else {
            return false;
        };
        let marketplace_plugins = home.join(".codex/.tmp/plugins/plugins");
        path.parent() == Some(marketplace_plugins.as_path())
            && Self::installation_for_path_from(path, installations).is_none()
    }

    /// Update one active plugin through Codex's own marketplace manager. No
    /// shell is involved and no package-provided scripts are executed.
    pub fn update(path: &Path) -> Result<Vec<PathBuf>, SkillSyncError> {
        let installation = Self::installation_for_path(path).ok_or_else(|| {
            SkillSyncError::UnsupportedUpdateMethod(format!(
                "{} jest nieaktywnym lub historycznym cache Codex; odśwież rejestr `codex plugin list` przed aktualizacją",
                path.display()
            ))
        })?;
        let cli = Self::resolve_cli().ok_or_else(|| {
            SkillSyncError::UnsupportedUpdateMethod(
                "Nie znaleziono Codex CLI. Ustaw SKILLSYNC_CODEX_CLI albo zainstaluj Codex CLI i spróbuj ponownie."
                    .to_string(),
            )
        })?;

        let mut upgrade = Command::new(&cli);
        if let Some(path) = Self::runtime_path(&cli) {
            upgrade.env("PATH", path);
        }
        let output = upgrade
            .args([
                "plugin",
                "marketplace",
                "upgrade",
                &installation.marketplace,
                "--json",
            ])
            .output()
            .map_err(|error| {
                SkillSyncError::UnsupportedUpdateMethod(format!(
                    "Nie można uruchomić Codex CLI ({}): {error}",
                    cli.display()
                ))
            })?;
        if !output.status.success() {
            let detail = command_detail(&output.stdout, &output.stderr);
            return Err(SkillSyncError::FileSystem(format!(
                "Codex nie odświeżył marketplace {}: {detail}",
                installation.marketplace
            )));
        }

        let mut install = Command::new(&cli);
        if let Some(path) = Self::runtime_path(&cli) {
            install.env("PATH", path);
        }
        let output = install
            .args([
                "plugin",
                "add",
                &installation.plugin_id,
                "--marketplace",
                &installation.marketplace,
                "--json",
            ])
            .output()
            .map_err(|error| {
                SkillSyncError::UnsupportedUpdateMethod(format!(
                    "Nie można uruchomić Codex CLI ({}): {error}",
                    cli.display()
                ))
            })?;
        if !output.status.success() {
            let detail = command_detail(&output.stdout, &output.stderr);
            return Err(SkillSyncError::FileSystem(format!(
                "Codex nie zaktualizował pluginu {}: {detail}",
                installation.plugin_id
            )));
        }

        let refreshed = Self::installation_for_path(path).ok_or_else(|| {
            SkillSyncError::IntegrityCheckFailed(format!(
                "Codex nie pozostawił aktywnej instalacji pluginu {} po aktualizacji",
                installation.plugin_id
            ))
        })?;
        ManagedManifest::validate_plugin(&refreshed.install_path).map_err(|reason| {
            SkillSyncError::IntegrityCheckFailed(format!(
                "Manifest pluginu {} jest niepoprawny po aktualizacji: {reason}",
                installation.plugin_id
            ))
        })?;
        Ok(vec![refreshed.install_path])
    }

    fn list_installations() -> Vec<CodexPluginInstallation> {
        let Some(cli) = Self::resolve_cli() else {
            return Vec::new();
        };
        let mut command = Command::new(&cli);
        if let Some(path) = Self::runtime_path(&cli) {
            command.env("PATH", path);
        }
        let Ok(output) = command
            .args(["plugin", "list", "--available", "--json"])
            .output()
        else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
            return Vec::new();
        };
        value
            .get("installed")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let source = entry.get("source")?;
                let install_path = source.get("path")?.as_str()?;
                Some(CodexPluginInstallation {
                    plugin_id: entry.get("name")?.as_str()?.to_string(),
                    name: entry.get("name")?.as_str()?.to_string(),
                    marketplace: entry.get("marketplaceName")?.as_str()?.to_string(),
                    install_path: PathBuf::from(install_path),
                    version: entry
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect()
    }

    fn resolve_cli() -> Option<PathBuf> {
        if let Some(configured) = std::env::var_os("SKILLSYNC_CODEX_CLI") {
            let configured = PathBuf::from(configured);
            if configured.is_file() {
                return Some(configured);
            }
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(if cfg!(windows) { "codex.exe" } else { "codex" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let home = dirs::home_dir()?;
        [
            home.join(".local/bin/codex"),
            home.join(".npm-global/bin/codex"),
            home.join(".codex/bin/codex"),
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())
    }

    fn runtime_path(cli: &Path) -> Option<std::ffi::OsString> {
        let mut directories: Vec<PathBuf> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        if let Some(parent) = cli.parent() {
            if !directories.iter().any(|directory| directory == parent) {
                directories.insert(0, parent.to_path_buf());
            }
        }
        std::env::join_paths(directories).ok()
    }
}

fn command_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(stdout).trim().to_string()
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_local_installations_from_codex_registry_shape() {
        let value = serde_json::json!({
            "installed": [
                {
                    "name": "superpowers",
                    "marketplaceName": "openai-curated",
                    "version": "1dc19589",
                    "source": {"source": "local", "path": "/tmp/plugins/superpowers"}
                },
                {
                    "name": "remote-only",
                    "marketplaceName": "remote",
                    "version": "1.0.0",
                    "source": {"source": "remote", "id": "connector"}
                }
            ]
        });
        let entries = value
            .get("installed")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        let parsed: Vec<_> = entries
            .iter()
            .filter_map(|entry| {
                let source = entry.get("source")?;
                let install_path = source.get("path")?.as_str()?;
                Some(CodexPluginInstallation {
                    plugin_id: entry.get("name")?.as_str()?.to_string(),
                    name: entry.get("name")?.as_str()?.to_string(),
                    marketplace: entry.get("marketplaceName")?.as_str()?.to_string(),
                    install_path: PathBuf::from(install_path),
                    version: entry.get("version")?.as_str().map(str::to_string),
                })
            })
            .collect();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].plugin_id, "superpowers");
    }

    #[test]
    fn command_detail_prefers_stderr() {
        assert_eq!(command_detail(b"stdout", b"stderr"), "stderr");
        assert_eq!(command_detail(b"stdout", b"  "), "stdout");
    }

    #[test]
    fn reads_the_manifest_version_instead_of_codex_cache_hash() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-codex-plugin-version-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(root.join(".codex-plugin")).unwrap();
        fs::write(
            root.join(".codex-plugin/plugin.json"),
            r#"{"name":"superpowers","version":"v6.3.0"}"#,
        )
        .unwrap();
        assert_eq!(
            CodexPluginService::plugin_version(&root).as_deref(),
            Some("6.3.0")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn builds_a_runtime_path_for_a_cli_binary() {
        assert!(CodexPluginService::runtime_path(Path::new("/tmp/codex")).is_some());
    }

    #[test]
    fn recognizes_a_nonexistent_legacy_codex_cache_path_as_inactive() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let path = home.join(".codex/plugins/cache/openai-curated/superpowers/old/skills");
        assert!(CodexPluginService::is_inactive_cache_path(&path));
    }

    #[test]
    fn does_not_classify_non_codex_paths_as_uninstalled_marketplace_entries() {
        assert!(!CodexPluginService::is_uninstalled_marketplace_entry(
            Path::new("/tmp/plugins/superpowers"),
            &[],
        ));
    }
}
