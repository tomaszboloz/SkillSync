//! Resolve a skill manifest without confusing presentation, identity and refs.
use super::manifest_provenance::with_source;
use std::time::Duration;

pub(crate) fn candidates(remote: &str, reference: &str, name: &str) -> Option<Vec<String>> {
    let (owner, repo) = super::github::GitHubService::parse_github_owner_repo(remote)?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || matches!(name, "." | "..")
    {
        return None;
    }
    let reference = reference.trim();
    if reference.is_empty()
        || reference.contains(['?', '#', '\\'])
        || reference
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return None;
    }
    let mut refs = vec![reference.to_string()];
    let bare = reference.trim_start_matches(['v', 'V']);
    if semver::Version::parse(bare).is_ok() {
        let alternate = if bare == reference {
            format!("v{bare}")
        } else {
            bare.to_string()
        };
        refs.push(alternate);
    }
    let mut urls = Vec::new();
    for reference in refs {
        for path in [
            format!("skills/{name}/SKILL.md"),
            format!("{name}/SKILL.md"),
            "SKILL.md".into(),
            format!("skills/{name}/skill.md"),
            "skill.md".into(),
        ] {
            let mut url = reqwest::Url::parse(&format!(
                "https://raw.githubusercontent.com/{owner}/{repo}/"
            ))
            .ok()?;
            // A branch such as feature/fix is one ref, not extra URL path components.
            url.path_segments_mut()
                .ok()?
                .pop_if_empty()
                .push(&reference)
                .extend(path.split('/'));
            urls.push(url.to_string());
        }
    }
    Some(urls)
}

pub(crate) fn matches_identity(content: &str, expected_name: &str) -> bool {
    let mut lines = content.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim) != Some("---") {
        return false;
    }
    let mut name_matches = false;
    let mut seen_name = false;
    for line in lines {
        if line.trim() == "---" {
            return name_matches;
        }
        if let Some(value) = line.strip_prefix("name:") {
            if seen_name {
                return false;
            }
            seen_name = true;
            name_matches = value.trim().trim_matches(['\'', '"']) == expected_name;
        }
    }
    false
}

pub(crate) async fn fetch(remote: &str, reference: &str, name: &str) -> Option<String> {
    let urls = candidates(remote, reference, name)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    // Bound the whole search, not just each individual candidate request.
    tokio::time::timeout(Duration::from_secs(25), async {
        for url in urls {
            let Ok(mut response) = client
                .get(url)
                .header("User-Agent", "SkillSync-Desktop")
                .send()
                .await
            else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let mut bytes = Vec::new();
            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                            return None;
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        bytes.clear();
                        break;
                    }
                }
            }
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };
            if matches_identity(&content, name) {
                let source = super::github::GitHubService::normalize_github_repository_url(remote)?;
                return Some(with_source(&content, &source));
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
#[path = "upstream_manifest_tests.rs"]
mod tests;
