use crate::models::skill::{AgentScope, SkillMetadata, SkillStatus};
use crate::services::codex_plugin::CodexPluginService;
use crate::services::git::GitService;
use crate::services::github::GitHubService;
use crate::services::manifest::SkillManifest;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct SkillDetector;

/// Value used only when no local manifest nor Git tag provides a version.
/// It must never be treated as a real release or as evidence that a skill is current.
pub const UNKNOWN_SKILL_VERSION: &str = "unknown";

impl SkillDetector {
    pub fn scan_directories(paths: &[PathBuf]) -> Vec<SkillMetadata> {
        let mut skills_map: std::collections::HashMap<String, SkillMetadata> =
            std::collections::HashMap::new();
        // Capture Codex ownership once for the whole scan. Legacy cache
        // directories are not Git skills; active caches are represented by
        // the plugin detector and must not be scanned a second time here.
        let codex_root = dirs::home_dir().map(|home| home.join(".codex"));
        let codex_installations = if codex_root
            .as_ref()
            .is_some_and(|root| paths.iter().any(|path| path.starts_with(root)))
        {
            CodexPluginService::active_installations()
        } else {
            Vec::new()
        };

        for base_path in paths {
            if !base_path.exists() {
                continue;
            }
            // Codex keeps immutable historical snapshots below
            // `~/.codex/plugins/cache`. The active plugin owner is the Codex
            // registry/CLI; a cache snapshot without an active registry entry
            // must not become a second install or an update target.
            if CodexPluginService::is_inactive_cache_path_from(base_path, &codex_installations)
                || CodexPluginService::is_active_plugin_cache_path_from(
                    base_path,
                    &codex_installations,
                )
            {
                continue;
            }

            // Marketplace repositories (for example Lex-Machina) commonly
            // keep their installable skills below `<repo>/.claude/skills`.
            // The old depth of three stopped at the collection directory and
            // silently hid every real SKILL.md one level below it. Keep the
            // traversal bounded, but allow the supported agent layouts and
            // still prune each manifest subtree as soon as it is found.
            let mut entries = WalkDir::new(base_path)
                .follow_links(true)
                .max_depth(8)
                .into_iter();
            while let Some(entry) = entries.next() {
                let Ok(entry) = entry else {
                    continue;
                };
                if Self::is_ignored_directory(entry.file_name()) {
                    if entry.file_type().is_dir() {
                        entries.skip_current_dir();
                    }
                    continue;
                }
                let p = entry.path();
                if entry.file_type().is_dir() {
                    if CodexPluginService::is_inactive_cache_path_from(p, &codex_installations)
                        || CodexPluginService::is_active_plugin_cache_path_from(
                            p,
                            &codex_installations,
                        )
                    {
                        entries.skip_current_dir();
                        continue;
                    }
                    if let Some(candidate) = Self::inspect_candidate_directory(p, base_path) {
                        // A matching display name does not prove two installs
                        // are the same product: e.g. `seo-audit` from
                        // marketingskills and from claude-seo have unrelated
                        // release lines. Merge locations only when their
                        // upstream identity also matches.
                        let key = Self::discovery_key(&candidate);

                        if let Some(existing) = skills_map.get_mut(&key) {
                            if !Self::contains_same_path(&existing.installed_locations, p) {
                                existing.installed_locations.push(p.to_path_buf());
                            }
                            if existing.agent_scope != candidate.agent_scope
                                && existing.agent_scope != AgentScope::Global
                            {
                                existing.agent_scope = AgentScope::Global;
                            }
                            if existing.remote_url.is_none() && candidate.remote_url.is_some() {
                                existing.remote_url = candidate.remote_url;
                            }
                            if existing.compatibility.is_none() && candidate.compatibility.is_some()
                            {
                                existing.compatibility = candidate.compatibility;
                            }
                        } else {
                            skills_map.insert(key, candidate);
                        }

                        // A manifest marks the root of one installable skill.
                        // Its own examples, templates or nested `skills/`
                        // directory must never become extra cards. Collections
                        // without a root manifest continue to expose each
                        // independently declared child skill.
                        entries.skip_current_dir();
                    }
                }
            }
        }

        let mut skills: Vec<SkillMetadata> = skills_map.into_values().collect();
        Self::disambiguate_duplicate_ids(&mut skills);
        skills.sort_by_key(|skill| skill.name.to_lowercase());
        skills
    }

    fn discovery_key(skill: &SkillMetadata) -> String {
        let source = skill
            .remote_url
            .as_deref()
            .map(|remote| {
                GitHubService::normalize_github_repository_url(remote)
                    .unwrap_or_else(|| remote.trim().trim_end_matches(".git").to_string())
                    .to_lowercase()
            })
            .unwrap_or_else(|| {
                let canonical =
                    fs::canonicalize(&skill.path).unwrap_or_else(|_| skill.path.clone());
                format!("local:{}", canonical.to_string_lossy().to_lowercase())
            });
        format!("{}|{}", skill.id, source)
    }

    fn contains_same_path(paths: &[PathBuf], candidate: &Path) -> bool {
        let candidate = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
        paths
            .iter()
            .any(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()) == candidate)
    }

    /// Preserve legacy IDs while a name has one upstream. If same-named
    /// skills come from different repositories, use a stable repository slug
    /// for the additional IDs so branch/repository overrides and updates can
    /// never cross between unrelated packages.
    fn disambiguate_duplicate_ids(skills: &mut [SkillMetadata]) {
        let mut groups = std::collections::HashMap::<String, Vec<usize>>::new();
        for (index, skill) in skills.iter().enumerate() {
            groups.entry(skill.id.clone()).or_default().push(index);
        }
        for indexes in groups.values_mut().filter(|indexes| indexes.len() > 1) {
            indexes.sort_by_key(|index| {
                skills[*index]
                    .remote_url
                    .as_deref()
                    .map(|url| {
                        GitHubService::normalize_github_repository_url(url)
                            .unwrap_or_else(|| url.trim().trim_end_matches(".git").to_string())
                            .to_lowercase()
                    })
                    .unwrap_or_default()
            });
            let base_id = skills[indexes[0]].id.clone();
            let mut used = std::collections::HashSet::from([base_id.clone()]);
            for index in indexes.iter().skip(1) {
                let source = skills[*index]
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
                skills[*index].id = candidate;
            }
        }
    }

    fn is_ignored_directory(name: &std::ffi::OsStr) -> bool {
        let s = name.to_string_lossy();
        s == ".git" || s == "node_modules" || s == "target" || s == "dist"
    }

    fn inspect_candidate_directory(dir: &Path, base_monitored: &Path) -> Option<SkillMetadata> {
        let skill_json = dir.join("skill.json");
        let skill_md = dir.join("SKILL.md");
        let package_json = dir.join("package.json");

        let folder_name = dir.file_name()?.to_string_lossy().to_string();

        // 1. Check skill.json
        if skill_json.exists() {
            if let Ok(content) = fs::read_to_string(&skill_json) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    let name = v
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(&folder_name)
                        .to_string();
                    let manifest_version = v
                        .get("version")
                        .and_then(|ver| ver.as_str())
                        .map(str::to_string);
                    let desc = v
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    let author = v
                        .get("author")
                        .and_then(|a| a.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let scope =
                        Self::infer_scope(v.get("scope").and_then(|s| s.as_str()), base_monitored);

                    let is_git = GitService::is_git_repository(dir);
                    let version = Self::resolved_local_version(dir, manifest_version);
                    let fm_dummy = FrontmatterMeta::default();
                    let remote_url = Self::resolve_remote_url(dir, &fm_dummy);
                    let branch_or_tag = GitService::get_current_ref_name(dir);
                    let compatibility = Self::infer_compatibility(dir, &scope, &fm_dummy);

                    return Some(SkillMetadata {
                        item_type: crate::models::skill::ManagedItemType::Skill,
                        id: format!("skill-{}", name.replace(' ', "-").to_lowercase()),
                        name,
                        description: desc,
                        current_version: version,
                        latest_version: None,
                        author,
                        path: dir.to_path_buf(),
                        is_git_repo: is_git,
                        remote_url,
                        branch_or_tag,
                        detected_branch: GitService::get_current_branch_name(dir),
                        branch_override: None,
                        agent_scope: scope,
                        status: SkillStatus::UpToDate,
                        update_available: false,
                        changelog: None,
                        dependencies: vec![],
                        permissions: vec![],
                        last_checked: chrono::Utc::now(),
                        compatibility: Some(compatibility),
                        update_compatibility: None,
                        installed_locations: vec![dir.to_path_buf()],
                    });
                }
            }
        }

        // 2. Check SKILL.md with YAML frontmatter
        if skill_md.exists() {
            let fm = if let Ok(content) = fs::read_to_string(&skill_md) {
                Self::parse_yaml_frontmatter(&content)
            } else {
                FrontmatterMeta::default()
            };

            let name = fm.name.clone().unwrap_or_else(|| folder_name.clone());
            let desc = fm
                .description
                .clone()
                .unwrap_or_else(|| "Skill with Markdown documentation".to_string());
            let author = fm.author.clone().unwrap_or_else(|| "Community".to_string());
            let scope = Self::infer_scope(fm.scope.as_deref(), base_monitored);

            let is_git = GitService::is_git_repository(dir);
            let remote_url = Self::resolve_remote_url(dir, &fm);
            let branch_or_tag = GitService::get_current_ref_name(dir);
            let compatibility = Self::infer_compatibility(dir, &scope, &fm);

            // A checked-out SemVer tag is the source of truth for every
            // Git-managed update. A nested manifest can legitimately retain
            // its older per-skill version while the repository is at a newer
            // release tag; treating that metadata as newer caused a false
            // update to reappear after the next scan.
            let version = Self::resolved_local_version(dir, fm.version.clone());

            return Some(SkillMetadata {
                item_type: crate::models::skill::ManagedItemType::Skill,
                id: format!("skill-{}", name.replace(' ', "-").to_lowercase()),
                name,
                description: desc,
                current_version: version,
                latest_version: None,
                author,
                path: dir.to_path_buf(),
                is_git_repo: is_git,
                remote_url,
                branch_or_tag,
                detected_branch: GitService::get_current_branch_name(dir),
                branch_override: None,
                agent_scope: scope,
                status: SkillStatus::UpToDate,
                update_available: false,
                changelog: None,
                dependencies: vec![],
                permissions: vec![],
                last_checked: chrono::Utc::now(),
                compatibility: Some(compatibility),
                update_compatibility: None,
                installed_locations: vec![dir.to_path_buf()],
            });
        }

        // 3. Check package.json
        if package_json.exists() {
            if let Ok(content) = fs::read_to_string(&package_json) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    if SkillManifest::is_explicit_package_skill(dir) {
                        let name = v
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or(&folder_name)
                            .to_string();
                        let manifest_version = v
                            .get("version")
                            .and_then(|ver| ver.as_str())
                            .map(|version| version.trim_start_matches(['v', 'V']).to_string());
                        let desc = v
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("Node-based AI skill")
                            .to_string();
                        let author = v
                            .get("author")
                            .and_then(|a| a.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let scope = Self::infer_scope(None, base_monitored);

                        let is_git = GitService::is_git_repository(dir);
                        let version = Self::resolved_local_version(dir, manifest_version);
                        let fm_dummy = FrontmatterMeta::default();
                        let remote_url = Self::resolve_remote_url(dir, &fm_dummy);
                        let branch_or_tag = GitService::get_current_ref_name(dir);
                        let compatibility = Self::infer_compatibility(dir, &scope, &fm_dummy);

                        return Some(SkillMetadata {
                            item_type: crate::models::skill::ManagedItemType::Skill,
                            id: format!("skill-{}", name.replace(' ', "-").to_lowercase()),
                            name,
                            description: desc,
                            current_version: version,
                            latest_version: None,
                            author,
                            path: dir.to_path_buf(),
                            is_git_repo: is_git,
                            remote_url,
                            branch_or_tag,
                            detected_branch: GitService::get_current_branch_name(dir),
                            branch_override: None,
                            agent_scope: scope,
                            status: SkillStatus::UpToDate,
                            update_available: false,
                            changelog: None,
                            dependencies: vec![],
                            permissions: vec![],
                            last_checked: chrono::Utc::now(),
                            compatibility: Some(compatibility),
                            update_compatibility: None,
                            installed_locations: vec![dir.to_path_buf()],
                        });
                    }
                }
            }
        }

        None
    }

    fn infer_scope(manifest_scope: Option<&str>, base_path: &Path) -> AgentScope {
        if let Some(s) = manifest_scope {
            match s.to_lowercase().as_str() {
                "codex" => return AgentScope::Codex,
                "claude" => return AgentScope::Claude,
                "cursor" => return AgentScope::Cursor,
                "antigravity" => return AgentScope::Antigravity,
                "global" => return AgentScope::Global,
                other => return AgentScope::Custom(other.to_string()),
            }
        }

        let p_str = base_path.to_string_lossy().to_lowercase();
        if p_str.contains(".codex") {
            AgentScope::Codex
        } else if p_str.contains(".claude") {
            AgentScope::Claude
        } else if p_str.contains(".cursor") {
            AgentScope::Cursor
        } else if p_str.contains("antigravity") {
            AgentScope::Antigravity
        } else {
            AgentScope::Global
        }
    }

    fn infer_compatibility(dir: &Path, scope: &AgentScope, fm: &FrontmatterMeta) -> String {
        if let Some(ref comp) = fm.compatibility {
            return comp.clone();
        }

        let mut parts = Vec::new();
        match scope {
            AgentScope::Codex => parts.push("OpenAI Codex"),
            AgentScope::Claude => parts.push("Claude Code (v1.0+)"),
            AgentScope::Cursor => parts.push("Cursor IDE"),
            AgentScope::Antigravity => parts.push("Google Antigravity"),
            AgentScope::Global => parts.push("Wszystkie agenty (Uniwersalny)"),
            AgentScope::Custom(s) => parts.push(s.as_str()),
        }

        if dir.join("pyproject.toml").exists()
            || dir.join("requirements.txt").exists()
            || dir.join("scripts").exists()
        {
            parts.push("Python >= 3.10");
        } else if dir.join("package.json").exists() {
            parts.push("Node.js >= 18");
        }

        parts.join(" • ")
    }

    fn resolve_remote_url(dir: &Path, fm: &FrontmatterMeta) -> Option<String> {
        // 1. Git repository (including skills nested below its root). Git2's
        // discovery walks all ancestors, which covers layouts such as
        // `i-have-adhd/skills/i-have-adhd/SKILL.md`.
        if let Some(url) = GitService::get_remote_url(dir) {
            return Some(url);
        }

        // 3. Frontmatter explicit repository
        if let Some(ref repo) = fm.repository {
            if repo.starts_with("http") || repo.starts_with("git@") {
                return Some(repo.clone());
            }
            if repo.contains('/') && !repo.contains(' ') {
                return Some(format!(
                    "https://github.com/{}",
                    repo.trim_start_matches('/')
                ));
            }
        }

        // 4. Check LICENSE.txt, LICENSE, LICENSE.md
        for lic_name in &["LICENSE.txt", "LICENSE", "LICENSE.md"] {
            let lic_path = dir.join(lic_name);
            if lic_path.exists() {
                if let Ok(content) = fs::read_to_string(&lic_path) {
                    if let Some(url) = Self::extract_github_url(&content) {
                        return Some(url);
                    }
                }
            }
        }

        // 5. Check pyproject.toml in dir or dir.parent()
        for p in &[
            dir.join("pyproject.toml"),
            dir.parent()
                .map(|p| p.join("pyproject.toml"))
                .unwrap_or_default(),
        ] {
            if p.exists() {
                if let Ok(content) = fs::read_to_string(p) {
                    if let Some(url) = Self::extract_github_url(&content) {
                        return Some(url);
                    }
                }
            }
        }

        // 6. Check package.json in dir or dir.parent()
        for p in &[
            dir.join("package.json"),
            dir.parent()
                .map(|p| p.join("package.json"))
                .unwrap_or_default(),
        ] {
            if p.exists() {
                if let Ok(content) = fs::read_to_string(p) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(repo) = v.get("repository") {
                            if let Some(s) = repo.as_str() {
                                if s.starts_with("http") {
                                    return Some(s.to_string());
                                } else if s.contains('/') {
                                    return Some(format!("https://github.com/{}", s));
                                }
                            } else if let Some(url) = repo.get("url").and_then(|u| u.as_str()) {
                                return Some(url.to_string());
                            }
                        }
                    }
                    if let Some(url) = Self::extract_github_url(&content) {
                        return Some(url);
                    }
                }
            }
        }

        // 7. Check SKILL.md or README.md
        for doc_name in &["SKILL.md", "README.md"] {
            let doc_path = dir.join(doc_name);
            if doc_path.exists() {
                if let Ok(content) = fs::read_to_string(&doc_path) {
                    if let Some(url) = Self::extract_github_url(&content) {
                        return Some(url);
                    }
                }
            }
        }

        None
    }

    fn extract_github_url(content: &str) -> Option<String> {
        let marker = "https://github.com/";
        let mut start_idx = 0;
        while let Some(pos) = content[start_idx..].find(marker) {
            let full_pos = start_idx + pos;
            let after = &content[full_pos..];
            let url_candidate: String = after
                .chars()
                .take_while(|c| {
                    c.is_alphanumeric()
                        || *c == ':'
                        || *c == '/'
                        || *c == '-'
                        || *c == '_'
                        || *c == '.'
                })
                .collect();

            let trimmed = url_candidate.trim_end_matches('.').trim_end_matches('/');
            let parts: Vec<&str> = trimmed.split('/').collect();
            // Expected: ["https:", "", "github.com", "owner", "repo"]
            if parts.len() >= 5 {
                let owner = parts[3];
                let repo = parts[4].trim_end_matches(".git");
                if !owner.is_empty()
                    && !repo.is_empty()
                    && owner != "user-attachments"
                    && owner != "OWNER"
                    && !repo.contains('#')
                    && !repo.contains('?')
                {
                    return Some(format!("https://github.com/{}/{}", owner, repo));
                }
            }
            start_idx = full_pos + marker.len();
        }
        None
    }

    fn parse_yaml_frontmatter(content: &str) -> FrontmatterMeta {
        let mut meta = FrontmatterMeta::default();
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            return meta;
        }

        let rest = &trimmed[3..];
        if let Some(end_pos) = rest.find("---") {
            let yaml_block = &rest[..end_pos];
            for line in yaml_block.lines() {
                let line = line.trim();
                if let Some((key, val)) = line.split_once(':') {
                    let key = key.trim().to_lowercase();
                    let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !val.is_empty() {
                        match key.as_str() {
                            "name" => meta.name = Some(val),
                            "version" => meta.version = Some(val),
                            "description" => meta.description = Some(val),
                            "author" => meta.author = Some(val),
                            "scope" => meta.scope = Some(val),
                            "repository" | "repo" | "url" | "homepage" | "github" | "source" => {
                                meta.repository = Some(val)
                            }
                            "compatibility" | "targets" | "target" | "requires" => {
                                meta.compatibility = Some(val)
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        meta
    }

    /// Keep scanning consistent with the updater: a local exact SemVer tag is
    /// the installed release. Branch and non-Git installations still report
    /// the version declared in their manifest.
    fn resolved_local_version(dir: &Path, manifest_version: Option<String>) -> String {
        GitService::get_head_tag(dir)
            .or(manifest_version)
            .unwrap_or_else(|| UNKNOWN_SKILL_VERSION.to_string())
    }
}

#[derive(Default)]
struct FrontmatterMeta {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<String>,
    scope: Option<String>,
    repository: Option<String>,
    compatibility: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "skillsync-detector-{name}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn write_file(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("fixture file has a parent")).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn detects_claude_skill_scope_from_a_fixture() {
        let root = fixture_root("claude-scope");
        let skills_dir = root.join(".claude/skills");
        let skill_dir = skills_dir.join("example-skill");
        write_file(
            &skill_dir.join("SKILL.md"),
            "---\nname: example-skill\nversion: 1.0.0\n---\n",
        );

        let skills = SkillDetector::scan_directories(&[skills_dir]);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].agent_scope, AgentScope::Claude);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_is_safe_for_empty_and_missing_monitored_paths() {
        let missing = fixture_root("missing").join("does-not-exist");

        assert!(SkillDetector::scan_directories(&[]).is_empty());
        assert!(SkillDetector::scan_directories(&[missing]).is_empty());
    }

    #[test]
    fn test_detects_codex_skill_scope() {
        let root = std::env::temp_dir().join(format!(
            "skillsync-codex-detection-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let skills_dir = root.join(".codex/skills");
        let skill_dir = skills_dir.join("example-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: example-skill\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let skills = SkillDetector::scan_directories(&[skills_dir]);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].agent_scope, AgentScope::Codex);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_update_skill_md_version() {
        let temp_dir = std::env::temp_dir().join(format!(
            "skillsync-test-skill-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let skill_md = temp_dir.join("SKILL.md");

        let sample_content = r#"---
name: test-skill
description: "A test skill"
metadata:
  author: TestAuthor
  version: "1.0.0"
---

# Test Content
"#;
        fs::write(&skill_md, sample_content).unwrap();

        // Update version to 2.3.1
        crate::services::orchestrator::UpdateOrchestrator::update_skill_md_version(
            &temp_dir, "2.3.1",
        )
        .unwrap();

        let updated_content = fs::read_to_string(&skill_md).unwrap();
        println!("Updated content:\n{}", updated_content);
        assert!(updated_content.contains("version: \"2.3.1\""));

        // Verify parsing back with SkillDetector
        let meta = SkillDetector::parse_yaml_frontmatter(&updated_content);
        assert_eq!(meta.version.as_deref(), Some("2.3.1"));

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn detects_skill_markdown_manifest() {
        let root = fixture_root("markdown");
        let skill_dir = root.join("skills/markdown-skill");
        write_file(
            &skill_dir.join("SKILL.md"),
            "---\nname: markdown-skill\nversion: 1.2.3\n---\n# Skill\n",
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "markdown-skill");
        assert_eq!(skills[0].current_version, "1.2.3");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exact_git_tag_overrides_stale_versions_in_every_supported_manifest() {
        let root = fixture_root("tag-version-source");
        let skills_root = root.join("skills");
        write_file(
            &skills_root.join("markdown/SKILL.md"),
            "---\nname: tag-markdown\nversion: 1.0.0\n---\n# Skill\n",
        );
        write_file(
            &skills_root.join("json/skill.json"),
            r#"{"name":"tag-json","version":"1.0.0"}"#,
        );
        write_file(
            &skills_root.join("package/package.json"),
            r#"{"name":"tag-package","version":"1.0.0","skill":true}"#,
        );

        let repo = git2::Repository::init(&root).unwrap();
        let signature = git2::Signature::now("SkillSync test", "tests@example.invalid").unwrap();
        let mut index = repo.index().unwrap();
        for relative in [
            "skills/markdown/SKILL.md",
            "skills/json/skill.json",
            "skills/package/package.json",
        ] {
            index.add_path(Path::new(relative)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let commit = repo
            .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap();
        let object = repo.find_object(commit, None).unwrap();
        repo.tag_lightweight("v2.11.1", &object, false).unwrap();

        let skills = SkillDetector::scan_directories(&[skills_root]);
        assert_eq!(skills.len(), 3);
        assert!(skills.iter().all(|skill| skill.current_version == "2.11.1"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_remote_for_a_skill_nested_inside_its_git_repository() {
        let root = fixture_root("nested-remote");
        let skills_root = root.join("skills");
        let skill_dir = skills_root.join("i-have-adhd");
        fs::create_dir_all(&skill_dir).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        repo.remote("origin", "https://github.com/ayghri/i-have-adhd")
            .unwrap();
        write_file(
            &skill_dir.join("SKILL.md"),
            "---\nname: i-have-adhd\n---\n# Skill\n",
        );

        let skills = SkillDetector::scan_directories(&[skills_root]);
        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].remote_url.as_deref(),
            Some("https://github.com/ayghri/i-have-adhd")
        );
        assert!(skills[0].is_git_repo);
        assert_eq!(
            GitService::repository_root(&skill_dir),
            Some(fs::canonicalize(&root).unwrap())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_skills_nested_in_a_claude_marketplace_repository() {
        let root = fixture_root("claude-marketplace");
        let marketplace = root.join("Lex-Machina/.claude/skills/prawny-router-v3");
        write_file(
            &marketplace.join("SKILL.md"),
            "---\nname: prawny-router-v3\ndescription: Legal router\n---\n# Router\n",
        );
        let repo = git2::Repository::init(root.join("Lex-Machina")).unwrap();
        repo.remote(
            "origin",
            "https://github.com/michaleiatrak-star/Lex-Machina",
        )
        .unwrap();

        let skills = SkillDetector::scan_directories(std::slice::from_ref(&root));
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "prawny-router-v3");
        assert_eq!(
            skills[0].remote_url.as_deref(),
            Some("https://github.com/michaleiatrak-star/Lex-Machina")
        );
        assert_eq!(
            fs::canonicalize(&skills[0].path).unwrap(),
            fs::canonicalize(marketplace).unwrap()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_same_named_skills_from_different_repositories_independent() {
        let root = fixture_root("same-name-different-sources");
        let first = root.join("first/seo-audit");
        let second = root.join("second/seo-audit");
        for (path, remote) in [
            (&first, "https://github.com/coreyhaines31/marketingskills"),
            (&second, "https://github.com/AgriciDaniel/claude-seo"),
        ] {
            write_file(
                &path.join("SKILL.md"),
                "---\nname: seo-audit\nversion: 1.0.0\n---\n# SEO\n",
            );
            let repo = git2::Repository::init(path).unwrap();
            repo.remote("origin", remote).unwrap();
        }

        let skills = SkillDetector::scan_directories(std::slice::from_ref(&root));
        assert_eq!(skills.len(), 2);
        assert_ne!(skills[0].id, skills[1].id);
        assert!(skills.iter().all(|skill| skill.name == "seo-audit"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_skill_json_manifest() {
        let root = fixture_root("json");
        let skill_dir = root.join("skills/json-skill");
        write_file(
            &skill_dir.join("skill.json"),
            r#"{"name":"json-skill","version":"2.0.0","description":"Fixture"}"#,
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "json-skill");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn marks_a_manifest_without_version_as_unknown() {
        let root = fixture_root("unknown-version");
        let skill_dir = root.join("skills/unknown-skill");
        write_file(
            &skill_dir.join("skill.json"),
            r#"{"name":"unknown-skill","description":"Fixture"}"#,
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].current_version, UNKNOWN_SKILL_VERSION);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_explicit_skill_package_manifest() {
        let root = fixture_root("package-skill");
        let skill_dir = root.join("skills/package-skill");
        write_file(
            &skill_dir.join("package.json"),
            r#"{"name":"package-skill","version":"v3.1.4","skill":{"entry":"index.js"}}"#,
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "package-skill");
        assert_eq!(skills[0].current_version, "3.1.4");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_explicit_ai_skill_package_manifest() {
        let root = fixture_root("ai-package-skill");
        let skill_dir = root.join("skills/ai-package-skill");
        write_file(
            &skill_dir.join("package.json"),
            r#"{"name":"ai-package-skill","version":"1.0.0","ai-skill":true}"#,
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "ai-package-skill");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_regular_packages_below_a_monitored_skills_directory() {
        let root = fixture_root("false-positives");
        let skills_root = root.join(".agents/skills");
        for relative in [
            "hyperframes/packages/aws-lambda",
            "agent-browser/docs",
            "ui-ux-pro-max-skill/gallery",
        ] {
            write_file(
                &skills_root.join(relative).join("package.json"),
                r#"{"name":"ordinary-project-file","version":"1.0.0"}"#,
            );
        }

        let skills = SkillDetector::scan_directories(&[skills_root]);
        assert!(
            skills.is_empty(),
            "ordinary nested package.json files are not skills"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn does_not_double_count_nested_skill_manifests_inside_an_installed_skill() {
        let root = fixture_root("nested-skill-boundary");
        let skill_root = root.join("skills/firebase");
        write_file(
            &skill_root.join("SKILL.md"),
            "---\nname: firebase\n---\n# Firebase\n",
        );
        write_file(
            &skill_root.join("skills/xcode-project-setup/SKILL.md"),
            "---\nname: xcode-project-setup\n---\n# Example\n",
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "firebase");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_children_of_a_collection_without_its_own_manifest() {
        let root = fixture_root("skill-collection");
        let collection = root.join("skills/firebase");
        write_file(
            &collection.join("xcode-project-setup/SKILL.md"),
            "---\nname: xcode-project-setup\n---\n# Xcode\n",
        );
        write_file(
            &collection.join("firestore/SKILL.md"),
            "---\nname: firestore\n---\n# Firestore\n",
        );

        let skills = SkillDetector::scan_directories(&[root.join("skills")]);
        assert_eq!(skills.len(), 2);
        assert!(skills
            .iter()
            .any(|skill| skill.name == "xcode-project-setup"));
        assert!(skills.iter().any(|skill| skill.name == "firestore"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_malformed_or_disabled_package_manifests() {
        let root = fixture_root("invalid-package");
        let skills_root = root.join("skills");
        write_file(&skills_root.join("malformed/package.json"), "{not-json");
        write_file(
            &skills_root.join("disabled/package.json"),
            r#"{"name":"disabled","skill":false}"#,
        );

        let skills = SkillDetector::scan_directories(&[skills_root]);
        assert!(skills.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_same_named_local_skills_separate_when_they_have_no_upstream() {
        let root = fixture_root("local-identity");
        for folder in ["first", "second"] {
            write_file(
                &root.join(folder).join("SKILL.md"),
                "---\nname: shared-local\n---\n# Skill\n",
            );
        }

        let skills = SkillDetector::scan_directories(std::slice::from_ref(&root));
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().all(|skill| skill.remote_url.is_none()));

        let _ = fs::remove_dir_all(root);
    }
}
