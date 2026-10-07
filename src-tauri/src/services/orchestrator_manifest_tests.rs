use super::*;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
#[ignore = "explicit upstream end-to-end verification; touches temporary fixtures only"]
async fn updates_named_source_across_locations_and_survives_rescan() {
    let root = Fixture(std::env::temp_dir().join(format!(
        "skillsync-source-update-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    )));
    let locations = vec![
        root.0.join("claude/seo-audit"),
        root.0.join("codex/seo-audit"),
    ];
    for path in &locations {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("SKILL.md"), "---\nname: seo-audit\nversion: 2.4.0\nrepository: https://github.com/AgriciDaniel/claude-seo\n---\n# Audit\n").unwrap();
    }
    let mut skill = SkillDetector::scan_directories(&locations).remove(0);
    skill.display_name = Some("seo-audit (claude-seo)".into());
    skill.installed_locations = locations.clone();
    skill.latest_version = Some("2.4.2".into());
    let updated = UpdateOrchestrator::update_skill_atomic(&skill, Some("2.4.2".into()), false)
        .await
        .unwrap();
    assert_eq!(updated.current_version, "2.4.2");
    assert!(!updated.update_available);
    for path in &locations {
        assert_eq!(
            UpdateOrchestrator::version_from_manifest(&ManagedItemType::Skill, path).as_deref(),
            Some("2.4.2")
        );
    }
    let rescanned = SkillDetector::scan_directories(&locations);
    assert!(!rescanned.is_empty());
    assert!(rescanned
        .iter()
        .all(|item| item.current_version == "2.4.2" && item.name == "seo-audit"));
    assert!(rescanned.iter().all(
        |item| item.remote_url.as_deref() == Some("https://github.com/AgriciDaniel/claude-seo")
    ));
    assert_eq!(
        fs::read_to_string(locations[0].join("SKILL.md")).unwrap(),
        fs::read_to_string(locations[1].join("SKILL.md")).unwrap()
    );
}
