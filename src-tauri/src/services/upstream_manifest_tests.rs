use super::*;

const REMOTE: &str = "https://github.com/AgriciDaniel/claude-seo";

#[test]
fn source_is_preserved_once_without_changing_identity() {
    let content = "---\nname: seo-audit\nmetadata:\n  version: 2.4.2\n---\n# Audit\n";
    let saved = with_source(content, REMOTE);
    assert_eq!(with_source(&saved, REMOTE), saved);
    assert!(matches_identity(&saved, "seo-audit"));
    assert!(saved.contains("  version: 2.4.2"));
    assert_eq!(saved.matches("skillsync-source:").count(), 1);
}

#[test]
fn tag_resolution_never_falls_back_to_a_branch() {
    let urls = candidates(REMOTE, "2.4.2", "seo-audit").unwrap();
    assert_eq!(urls.len(), 10);
    assert!(urls
        .iter()
        .all(|url| url.contains("/2.4.2/") || url.contains("/v2.4.2/")));
    assert!(urls.contains(&"https://raw.githubusercontent.com/AgriciDaniel/claude-seo/v2.4.2/skills/seo-audit/SKILL.md".to_string()));
}

#[test]
fn prefixed_tag_also_resolves_unprefixed_upstream() {
    let urls = candidates(REMOTE, "v2.4.2", "seo-audit").unwrap();
    assert!(urls[0].contains("/v2.4.2/"));
    assert!(urls[5].contains("/2.4.2/"));
}

#[test]
fn branch_is_not_rewritten_as_a_tag_or_default_branch() {
    let urls = candidates(REMOTE, "feature/fix", "seo-audit").unwrap();
    assert_eq!(urls.len(), 5);
    assert!(urls.iter().all(|url| url.contains("/feature%2Ffix/")));
}

#[test]
fn unsafe_or_decorated_names_are_not_paths() {
    for name in [
        "seo-audit (claude-seo)",
        "../seo-audit",
        "..",
        "",
        "seo?audit",
        "seo/audit",
    ] {
        assert!(candidates(REMOTE, "v2.4.2", name).is_none(), "{name}");
    }
}

#[test]
fn only_the_expected_manifest_identity_is_accepted() {
    assert!(!matches_identity("---\nname: seo-audit\n", "seo-audit"));
    assert!(!matches_identity(
        "---\nname: seo-audit\nname: other\n---\n",
        "seo-audit"
    ));
    assert!(matches_identity(
        "---\nname: seo-audit\n---\n# Audit",
        "seo-audit"
    ));
    assert!(matches_identity(
        "---\r\nname: 'seo-audit'\r\n---\r\n",
        "seo-audit"
    ));
    assert!(!matches_identity(
        "---\nname: seo\n---\n# Router",
        "seo-audit"
    ));
    assert!(!matches_identity(
        "# Documentation\nname: seo-audit",
        "seo-audit"
    ));
    assert!(!matches_identity(
        "---\ndescription: hi\n---\nname: seo-audit",
        "seo-audit"
    ));
    assert!(!matches_identity(
        "<html>name: seo-audit</html>",
        "seo-audit"
    ));
}

#[tokio::test]
#[ignore = "explicit upstream network verification; ordinary CI stays deterministic"]
async fn fetches_real_seo_audit_release_manifest() {
    let manifest = fetch(REMOTE, "2.4.2", "seo-audit").await.unwrap();
    assert!(manifest.contains("version: \"2.4.2\""));
    assert!(matches_identity(&manifest, "seo-audit"));
}

#[tokio::test]
#[ignore = "explicit upstream source-mismatch verification"]
async fn does_not_install_a_python_package_release_as_a_skill_manifest() {
    assert!(
        fetch("https://github.com/PrefectHQ/fastmcp", "4.0.11", "fastmcp")
            .await
            .is_none()
    );
}
