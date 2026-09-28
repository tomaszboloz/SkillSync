use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubReleaseInfo {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub published_at: Option<String>,
}

pub struct GitHubService;

impl GitHubService {
    /// Converts an explicit GitHub repository address to the one canonical
    /// format used by the scanner and updater. Only a repository root is
    /// accepted: branches, file URLs and other hosts belong in their own
    /// fields and must not silently become a tracking source.
    pub fn normalize_github_repository_url(value: &str) -> Option<String> {
        let value = value.trim();
        if value.is_empty() || value.contains(['?', '#']) {
            return None;
        }

        let path = if let Some(path) = value.strip_prefix("https://github.com/") {
            path
        } else if let Some(path) = value.strip_prefix("http://github.com/") {
            path
        } else {
            value.strip_prefix("git@github.com:")?
        };

        let path = path.trim_end_matches('/').trim_end_matches(".git");
        let mut parts = path.split('/');
        let owner = parts.next()?;
        let repo = parts.next()?;
        if parts.next().is_some()
            || owner.is_empty()
            || repo.is_empty()
            || !owner.chars().chain(repo.chars()).all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return None;
        }

        Some(format!("https://github.com/{owner}/{repo}"))
    }

    pub fn parse_github_owner_repo(remote_url: &str) -> Option<(String, String)> {
        let normalized = Self::normalize_github_repository_url(remote_url)?;
        let path = normalized.strip_prefix("https://github.com/")?;
        let (owner, repo) = path.split_once('/')?;
        Some((owner.to_string(), repo.to_string()))
    }

    pub async fn check_latest_version(
        remote_url: &str,
    ) -> Result<Option<GitHubReleaseInfo>, String> {
        let mut last_error = None;

        // A scan is deliberately throttled by the caller, but an individual
        // connection can still fail transiently (DNS, captive Wi-Fi, GitHub
        // edge timeout). Retry the whole fallback chain once so bulk scanning
        // and the detail action have identical, dependable semantics.
        for attempt in 0..2 {
            match Self::check_latest_version_once(remote_url).await {
                Ok(release) => return Ok(release),
                Err(error) => {
                    last_error = Some(error);
                    if attempt == 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "Nieznany błąd połączenia z GitHub".to_string()))
    }

    async fn check_latest_version_once(
        remote_url: &str,
    ) -> Result<Option<GitHubReleaseInfo>, String> {
        let (owner, repo) = match Self::parse_github_owner_repo(remote_url) {
            Some(res) => res,
            None => return Ok(None),
        };
        let mut had_usable_response = false;

        // 1. Zero-rate-limit method: Check web redirect on https://github.com/{owner}/{repo}/releases/latest
        let no_redirect_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .map_err(|e| e.to_string())?;

        let web_url = format!("https://github.com/{}/{}/releases/latest", owner, repo);
        if let Ok(resp) = no_redirect_client
            .get(&web_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
            )
            .send()
            .await
        {
            let status = resp.status();
            had_usable_response |= status.is_success()
                || status.is_redirection()
                || status == reqwest::StatusCode::NOT_FOUND;
            if status.is_redirection() {
                if let Some(loc) = resp.headers().get("location").and_then(|l| l.to_str().ok()) {
                    if let Some(tag_part) = loc.split("/tag/").nth(1) {
                        let tag = tag_part.trim_matches('/').to_string();
                        if !tag.is_empty() {
                            return Ok(Some(GitHubReleaseInfo {
                                tag_name: tag.clone(),
                                name: Some(format!("Release {}", tag)),
                                body: None,
                                published_at: None,
                            }));
                        }
                    }
                }
            }
        }

        // 2. Zero-rate-limit method: Check Atom feed on https://github.com/{owner}/{repo}/releases.atom
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .map_err(|e| e.to_string())?;

        let atom_url = format!("https://github.com/{}/{}/releases.atom", owner, repo);
        if let Ok(resp) = client
            .get(&atom_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            had_usable_response |=
                resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND;
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Some(tag) = Self::extract_tag_from_atom(&text) {
                        return Ok(Some(GitHubReleaseInfo {
                            tag_name: tag.clone(),
                            name: Some(format!("Release {}", tag)),
                            body: None,
                            published_at: None,
                        }));
                    }
                }
            }
        }

        // 3. Zero-rate-limit method: Check tags.atom on https://github.com/{owner}/{repo}/tags.atom
        let tags_atom_url = format!("https://github.com/{}/{}/tags.atom", owner, repo);
        if let Ok(resp) = client
            .get(&tags_atom_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            had_usable_response |=
                resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND;
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Some(tag) = Self::extract_tag_from_atom(&text) {
                        return Ok(Some(GitHubReleaseInfo {
                            tag_name: tag.clone(),
                            name: Some(format!("Tag {}", tag)),
                            body: None,
                            published_at: None,
                        }));
                    }
                }
            }
        }

        let (api_usable, api_release) =
            Self::check_latest_version_api(&client, &owner, &repo).await;
        had_usable_response |= api_usable;
        if let Some(release) = api_release {
            return Ok(Some(release));
        }

        if had_usable_response {
            Ok(None)
        } else {
            Err(format!(
                "GitHub nie odpowiedział poprawnie dla {}/{}; spróbuj ponownie.",
                owner, repo
            ))
        }
    }

    async fn check_latest_version_api(
        client: &reqwest::Client,
        owner: &str,
        repo: &str,
    ) -> (bool, Option<GitHubReleaseInfo>) {
        let release_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
        let mut request = client
            .get(release_url)
            .header("User-Agent", "SkillSync-App")
            .header("Accept", "application/vnd.github.v3+json");
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(_) => return (false, None),
        };
        let usable =
            response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND;
        if !response.status().is_success() {
            return (usable, None);
        }
        let release = match response.json::<serde_json::Value>().await {
            Ok(release) => release,
            Err(_) => return (usable, None),
        };
        let Some(tag) = release.get("tag_name").and_then(|value| value.as_str()) else {
            return (usable, None);
        };
        let tag = tag.trim().to_string();
        let release = (!tag.is_empty()).then(|| GitHubReleaseInfo {
            tag_name: tag,
            name: release
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            body: release
                .get("body")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            published_at: release
                .get("published_at")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
        (usable, release)
    }

    pub fn extract_tag_from_atom(atom_text: &str) -> Option<String> {
        if let Some(pos) = atom_text.find("/releases/tag/") {
            let rest = &atom_text[pos + "/releases/tag/".len()..];
            let end = rest
                .find('"')
                .or_else(|| rest.find('<'))
                .or_else(|| rest.find(' '))
                .unwrap_or(rest.len());
            let tag = rest[..end].trim().to_string();
            if !tag.is_empty() {
                return Some(tag);
            }
        }
        None
    }

    pub fn is_newer_version(latest_tag: &str, current_version: &str) -> bool {
        let clean_latest = latest_tag.trim_start_matches(['v', 'V']);
        let clean_current = current_version.trim_start_matches(['v', 'V']);

        if let (Ok(latest_sem), Ok(current_sem)) = (
            semver::Version::parse(clean_latest),
            semver::Version::parse(clean_current),
        ) {
            latest_sem > current_sem
        } else {
            // A non-SemVer local value (for example "unknown" or a branch
            // name) cannot safely prove that an upstream release is newer.
            // Never turn a string mismatch into an update offer.
            false
        }
    }

    pub fn get_semver_compatibility(latest_tag: &str, current_version: &str) -> String {
        let clean_latest = latest_tag.trim_start_matches(['v', 'V']);
        let clean_current = current_version.trim_start_matches(['v', 'V']);

        if let (Ok(latest_sem), Ok(current_sem)) = (
            semver::Version::parse(clean_latest),
            semver::Version::parse(clean_current),
        ) {
            if latest_sem.major == current_sem.major {
                if latest_sem.minor > current_sem.minor {
                    format!("SemVer Minor (v{}.x) — 100% Wstecznie kompatybilna aktualizacja (brak zmian łamiących)", latest_sem.major)
                } else if latest_sem.patch > current_sem.patch {
                    format!(
                        "SemVer Patch (v{}.{}.x) — W pełni kompatybilna łatka błędów i usprawnień",
                        latest_sem.major, latest_sem.minor
                    )
                } else {
                    "Wersja bieżąca jest w pełni kompatybilna i aktualna".to_string()
                }
            } else if latest_sem.major > current_sem.major {
                format!("SemVer Major (v{} ➔ v{}) — Uwaga: Nowe wydanie główne, możliwe zmiany łamiące kompatybilność (Breaking Change)", current_sem.major, latest_sem.major)
            } else {
                "Wersja stabilna, kompatybilna ze środowiskiem agenta".to_string()
            }
        } else {
            "Nie można bezpiecznie porównać wersji lokalnej z wydaniem upstream".to_string()
        }
    }

    pub async fn fetch_raw_skill_md(
        remote_url: &str,
        tag_or_ref: &str,
        skill_name: &str,
    ) -> Option<String> {
        let (owner, repo) = Self::parse_github_owner_repo(remote_url)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .ok()?;

        let clean_tag = tag_or_ref.trim();
        let tag_v = if clean_tag.starts_with('v') || clean_tag.starts_with('V') {
            clean_tag.to_string()
        } else {
            format!("v{}", clean_tag)
        };

        // Possible candidate URLs to check
        let refs = vec![clean_tag, &tag_v, "main", "master"];
        for r in refs {
            let candidates = vec![
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/skills/{}/SKILL.md",
                    owner, repo, r, skill_name
                ),
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
                    owner, repo, r, skill_name
                ),
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/SKILL.md",
                    owner, repo, r
                ),
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/skills/{}/skill.md",
                    owner, repo, r, skill_name
                ),
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/skill.md",
                    owner, repo, r
                ),
            ];

            for url in candidates {
                if let Ok(resp) = client
                    .get(&url)
                    .header("User-Agent", "SkillSync-Desktop")
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(text) = resp.text().await {
                            if !text.trim().is_empty()
                                && (text.contains("name:")
                                    || text.contains("# ")
                                    || text.contains("description:"))
                            {
                                return Some(text);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_claude_seo() {
        let res =
            GitHubService::check_latest_version("https://github.com/AgriciDaniel/claude-seo").await;
        println!("test_check_claude_seo result: {:?}", res);
    }

    #[test]
    fn stable_release_is_not_older_than_its_prerelease() {
        assert!(!GitHubService::is_newer_version("v1.9.6-codex.5", "1.9.6"));
        assert!(GitHubService::is_newer_version("v1.9.6", "1.9.6-codex.5"));
    }

    #[test]
    fn unparseable_versions_never_create_a_false_update() {
        assert!(!GitHubService::is_newer_version("v1.9.6", "unknown"));
        assert!(!GitHubService::is_newer_version("release-next", "main"));
    }

    #[test]
    fn normalizes_only_github_repository_roots() {
        assert_eq!(
            GitHubService::normalize_github_repository_url(
                " https://github.com/PrefectHQ/fastmcp.git ",
            ),
            Some("https://github.com/PrefectHQ/fastmcp".to_string())
        );
        assert_eq!(
            GitHubService::normalize_github_repository_url("git@github.com:PrefectHQ/fastmcp.git"),
            Some("https://github.com/PrefectHQ/fastmcp".to_string())
        );
        assert!(GitHubService::normalize_github_repository_url(
            "https://github.com/PrefectHQ/fastmcp/tree/main"
        )
        .is_none());
        assert!(GitHubService::normalize_github_repository_url(
            "https://example.com/PrefectHQ/fastmcp"
        )
        .is_none());
        assert!(GitHubService::normalize_github_repository_url("main").is_none());
    }
}
