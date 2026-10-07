use super::*;

#[test]
fn reads_marketplace_pin_not_a_repository_tag() {
    let root = std::env::temp_dir().join(format!(
        "skillsync-pins-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(root.join("marketplace/.claude-plugin")).unwrap();
    fs::write(
        root.join("known_marketplaces.json"),
        serde_json::json!({"official":{"installLocation":root.join("marketplace")}}).to_string(),
    )
    .unwrap();
    let pin = "172920731eedf414e9b22ae60017d9a5b6c9f81f";
    let manifest = serde_json::json!({"plugins":[{"name":"figma","source":{"source":"url","url":"https://github.com/figma/mcp-server-guide.git","sha":pin}}]});
    let path = root.join("marketplace/.claude-plugin/marketplace.json");
    fs::write(&path, manifest.to_string()).unwrap();
    assert_eq!(
        pinned_source(&root, "figma@official"),
        Some((
            "https://github.com/figma/mcp-server-guide".into(),
            pin.into()
        ))
    );
    assert!(pinned_source(&root, "other@official").is_none());
    assert!(pinned_source(&root, "figma@unknown").is_none());
    let mut invalid = manifest.clone();
    invalid["plugins"][0]["source"]["sha"] = "main".into();
    fs::write(&path, invalid.to_string()).unwrap();
    assert!(pinned_source(&root, "figma@official").is_none());
    fs::remove_dir_all(root).unwrap();
}
