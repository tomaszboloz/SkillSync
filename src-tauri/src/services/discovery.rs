use crate::models::skill::ManagedItemType;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Build an identity for a manifest with no resolvable upstream URL. Copies
/// of one local skill are commonly installed in several agent directories;
/// absolute paths must not inflate the logical resource counter.
///
/// Manifest bytes are part of the identity, so unrelated local resources with
/// the same display name remain independent unless their manifests match.
pub fn local_manifest_identity(path: &Path, item_type: &ManagedItemType) -> String {
    let files: &[&str] = match item_type {
        ManagedItemType::Skill => &["SKILL.md", "skill.json", "package.json"],
        ManagedItemType::Plugin => &[
            ".claude-plugin/plugin.json",
            ".codex-plugin/plugin.json",
            ".cursor-plugin/plugin.json",
            "plugin.json",
        ],
        ManagedItemType::Mcp => &["mcp.json", ".mcp.json", "composer.json", "composer.lock"],
    };

    let mut hasher = DefaultHasher::new();
    for relative in files {
        relative.hash(&mut hasher);
        match fs::read(path.join(relative)) {
            Ok(contents) => contents.hash(&mut hasher),
            Err(_) => 0u8.hash(&mut hasher),
        }
    }
    format!("manifest:{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_local_manifests_share_an_identity() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-discovery-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let manifest = "---\nname: shared\n---\n# Shared\n";
        fs::write(first.join("SKILL.md"), manifest).unwrap();
        fs::write(second.join("SKILL.md"), manifest).unwrap();

        assert_eq!(
            local_manifest_identity(&first, &ManagedItemType::Skill),
            local_manifest_identity(&second, &ManagedItemType::Skill)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn different_local_manifests_remain_independent() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-discovery-different-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("SKILL.md"), "---\nname: same\n---\n# One\n").unwrap();
        fs::write(second.join("SKILL.md"), "---\nname: same\n---\n# Two\n").unwrap();

        assert_ne!(
            local_manifest_identity(&first, &ManagedItemType::Skill),
            local_manifest_identity(&second, &ManagedItemType::Skill)
        );
        let _ = fs::remove_dir_all(root);
    }
}
