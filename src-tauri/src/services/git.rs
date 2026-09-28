use crate::errors::SkillSyncError;
use git2::{build::CheckoutBuilder, Commit, Repository, Status, StatusOptions};
use std::fs;
use std::path::{Path, PathBuf};

pub struct GitService;

impl GitService {
    pub fn is_valid_branch_name(branch: &str) -> bool {
        let branch = branch.trim();
        !branch.is_empty()
            && !branch.starts_with('-')
            && !branch.ends_with('.')
            && branch != "."
            && branch != ".."
            && !branch.starts_with('/')
            && !branch.ends_with('/')
            && !branch.contains("//")
            && !branch.contains("@{")
            && !branch.contains("..")
            && !branch.chars().any(|character| {
                character.is_whitespace()
                    || character.is_control()
                    || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            })
            && !branch.contains("://")
    }

    pub fn is_git_repository(path: &Path) -> bool {
        Repository::discover(path).is_ok()
    }

    pub fn repository_root(path: &Path) -> Option<PathBuf> {
        let root = Repository::discover(path).ok()?.workdir()?.to_path_buf();
        fs::canonicalize(root).ok()
    }

    pub fn get_remote_url(path: &Path) -> Option<String> {
        // `SKILL.md` files are often nested inside a repository (for example
        // `repo/skills/name`). Discover the repository from the nested path so
        // the UI can still expose its GitHub remote and update controls.
        let repo = Repository::discover(path).ok()?;
        let remote = repo.find_remote("origin").ok()?;
        remote.url().ok().map(str::to_owned)
    }

    /// Returns true only when `path` is present in the current Git index.
    /// SkillSync uses this to distinguish a temporary local adapter from an
    /// upstream-owned manifest before a checkout is allowed to replace it.
    pub fn is_file_tracked(path: &Path) -> bool {
        let repo = match Repository::discover(path) {
            Ok(repo) => repo,
            Err(_) => return false,
        };
        let workdir = match repo.workdir() {
            Some(workdir) => workdir,
            None => return false,
        };
        let relative = match path.strip_prefix(workdir) {
            Ok(relative) => relative,
            Err(_) => return false,
        };

        repo.index()
            .ok()
            .and_then(|index| index.get_path(relative, 0).map(|_| ()))
            .is_some()
    }

    pub fn get_current_ref_name(path: &Path) -> Option<String> {
        let repo = Repository::discover(path).ok()?;
        let head = repo.head().ok()?;
        if head.is_branch() {
            head.shorthand().ok().map(str::to_owned)
        } else {
            // Detached HEAD or tag
            head.target().map(|oid| oid.to_string()[..7].to_string())
        }
    }

    pub fn get_current_branch_name(path: &Path) -> Option<String> {
        let repo = Repository::discover(path).ok()?;
        let head = repo.head().ok()?;
        head.is_branch()
            .then(|| head.shorthand().ok().map(str::to_owned))
            .flatten()
    }

    pub fn get_head_commit(path: &Path) -> Option<String> {
        let repo = Repository::discover(path).ok()?;
        let commit = repo.head().ok()?.target().map(|oid| oid.to_string());
        commit
    }

    /// Resolves the current SHA of a remote branch without changing the local
    /// checkout. `git ls-remote` honours the user's configured credentials,
    /// which is essential for private agent repositories.
    pub fn get_remote_branch_commit(path: &Path, branch: &str) -> Result<String, SkillSyncError> {
        if branch.trim().is_empty() {
            return Err(SkillSyncError::Git {
                code: -4,
                message: "Nazwa gałęzi nie może być pusta".to_string(),
            });
        }
        let output = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "ls-remote",
                "--heads",
                "origin",
                branch.trim(),
            ])
            .output()
            .map_err(|error| SkillSyncError::FileSystem(error.to_string()))?;
        if !output.status.success() {
            return Err(SkillSyncError::Git {
                code: output.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let sha = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if sha.is_empty() {
            return Err(SkillSyncError::Git {
                code: -3,
                message: format!("Gałąź '{}' nie istnieje w origin", branch.trim()),
            });
        }
        Ok(sha)
    }

    /// Return the highest semantic-version tag advertised by `origin` without
    /// modifying the working tree. The original tag spelling is retained so a
    /// repository using `v1.2.3` is checked out by that exact ref.
    pub fn get_latest_remote_tag(path: &Path) -> Result<Option<String>, SkillSyncError> {
        let output = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "ls-remote",
                "--tags",
                "--refs",
                "origin",
            ])
            .output()
            .map_err(|error| SkillSyncError::FileSystem(error.to_string()))?;
        if !output.status.success() {
            return Err(SkillSyncError::Git {
                code: output.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(Self::latest_semver_tag_from_ls_remote(
            &String::from_utf8_lossy(&output.stdout),
        ))
    }

    fn latest_semver_tag_from_ls_remote(output: &str) -> Option<String> {
        output
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .filter_map(|reference| reference.strip_prefix("refs/tags/"))
            .filter_map(|tag| {
                semver::Version::parse(tag.trim_start_matches(['v', 'V']))
                    .ok()
                    .map(|version| (version, tag.to_string()))
            })
            .max_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, tag)| tag)
    }

    /// Branch tracking is only a fallback for repositories without SemVer
    /// tags. Prefer main, then the remote default branch, then the branch that
    /// was detected locally. Each candidate is proved to exist upstream.
    pub fn resolve_fallback_branch(
        path: &Path,
        detected_branch: Option<&str>,
    ) -> Result<String, SkillSyncError> {
        let mut candidates = vec!["main".to_string()];
        if let Some(default_branch) = Self::get_remote_default_branch(path)? {
            candidates.push(default_branch);
        }
        if let Some(detected) = detected_branch.filter(|branch| Self::is_valid_branch_name(branch))
        {
            candidates.push(detected.to_string());
        }
        let mut unique_candidates = Vec::new();
        for branch in candidates {
            if !unique_candidates.contains(&branch) {
                unique_candidates.push(branch);
            }
        }

        let mut last_error = None;
        for branch in unique_candidates {
            match Self::get_remote_branch_commit(path, &branch) {
                Ok(_) => return Ok(branch),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or_else(|| SkillSyncError::Git {
            code: -3,
            message: "Origin nie udostępnia gałęzi do śledzenia".to_string(),
        }))
    }

    fn get_remote_default_branch(path: &Path) -> Result<Option<String>, SkillSyncError> {
        let output = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "ls-remote",
                "--symref",
                "origin",
                "HEAD",
            ])
            .output()
            .map_err(|error| SkillSyncError::FileSystem(error.to_string()))?;
        if !output.status.success() {
            return Err(SkillSyncError::Git {
                code: output.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| {
                line.strip_prefix("ref: refs/heads/")
                    .and_then(|value| value.split_whitespace().next())
            })
            .filter(|branch| Self::is_valid_branch_name(branch))
            .map(str::to_string))
    }

    pub fn fetch_and_checkout_branch(
        path: &Path,
        branch: &str,
        allow_dirty_worktree: bool,
    ) -> Result<(), SkillSyncError> {
        let repo = Repository::discover(path)?;
        if !allow_dirty_worktree && !Self::is_worktree_clean(path)? {
            return Err(SkillSyncError::WorktreeDirty);
        }

        let branch = branch.trim();
        let refspec = format!("refs/heads/{branch}:refs/remotes/origin/{branch}");
        let output = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "fetch",
                "origin",
                &refspec,
            ])
            .output()
            .map_err(|error| SkillSyncError::FileSystem(error.to_string()))?;
        if !output.status.success() {
            return Err(SkillSyncError::Git {
                code: output.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let remote_ref = format!("refs/remotes/origin/{branch}");
        let commit = repo
            .find_reference(&remote_ref)
            .and_then(|reference| reference.peel_to_commit())?;
        if allow_dirty_worktree {
            Self::preserve_checkout_conflicts(path, &repo, &commit)?;
        }
        let mut checkout = CheckoutBuilder::new();
        if allow_dirty_worktree {
            checkout.force();
        } else {
            checkout.safe();
        }
        repo.checkout_tree(commit.as_object(), Some(&mut checkout))?;

        let local_ref = format!("refs/heads/{branch}");
        if repo.find_reference(&local_ref).is_ok() {
            repo.reference(&local_ref, commit.id(), true, "SkillSync branch update")?;
        } else {
            repo.branch(branch, &commit, false)?;
        }
        repo.set_head(&local_ref)?;
        Ok(())
    }

    pub fn is_worktree_clean(path: &Path) -> Result<bool, SkillSyncError> {
        let repo = Repository::discover(path)?;
        let mut opts = StatusOptions::new();
        // Updates change tracked files. Untracked files are commonly created
        // by skill tooling and must not permanently block an update; a real
        // checkout conflict is still rejected by libgit2 below.
        opts.include_untracked(false)
            .include_ignored(false)
            .recurse_untracked_dirs(false);
        let statuses = repo.statuses(Some(&mut opts))?;

        // A deleted tracked file has no local content that a checkout could
        // overwrite. Recreating it from the selected release is safe, and is
        // common when an installed package is left with a removed test file.
        // Any other tracked/index change remains a hard safety stop.
        Ok(statuses
            .iter()
            .all(|entry| entry.status() == Status::WT_DELETED))
    }

    /// A failed legacy SkillSync update could add only `metadata.version` to a
    /// tracked SKILL.md before the integrity gate rolled the checkout back.
    /// That isolated bookkeeping delta is safe to replace from the snapshot;
    /// every other tracked or staged change remains protected as dirty state.
    pub fn has_only_skill_version_metadata_change(path: &Path) -> Result<bool, SkillSyncError> {
        let repo = Repository::discover(path)?;
        let mut options = StatusOptions::new();
        options
            .include_untracked(false)
            .include_ignored(false)
            .recurse_untracked_dirs(false);
        let statuses = repo.statuses(Some(&mut options))?;
        if statuses.is_empty()
            || !statuses.iter().all(|entry| {
                entry.status() == Status::WT_MODIFIED && entry.path().ok() == Some("SKILL.md")
            })
        {
            return Ok(false);
        }

        let output = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "diff",
                "--no-ext-diff",
                "--unified=0",
                "HEAD",
                "--",
                "SKILL.md",
            ])
            .output()
            .map_err(|error| SkillSyncError::FileSystem(error.to_string()))?;
        if !output.status.success() {
            return Err(SkillSyncError::Git {
                code: output.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let mut found_metadata_change = false;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
                continue;
            }
            if let Some(changed_line) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) {
                if changed_line.trim_start().starts_with("version:") {
                    found_metadata_change = true;
                    continue;
                }
                return Ok(false);
            }
        }
        Ok(found_metadata_change)
    }

    pub fn get_head_tag(path: &Path) -> Option<String> {
        let repo = Repository::discover(path).ok()?;
        let head = repo.head().ok()?;
        let head_commit = head.peel_to_commit().ok()?;
        let head_id = head_commit.id();

        repo.tag_names(None)
            .ok()?
            .iter()
            .flatten()
            .flatten()
            .filter_map(|tag_name| {
                let version =
                    semver::Version::parse(tag_name.trim_start_matches(['v', 'V'])).ok()?;
                let refname = format!("refs/tags/{tag_name}");
                let reference = repo.find_reference(&refname).ok()?;
                let commit = reference.peel_to_commit().ok()?;
                (commit.id() == head_id).then_some((version, tag_name.to_string()))
            })
            .max_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, tag_name)| tag_name.trim_start_matches(['v', 'V']).to_string())
    }

    pub fn fetch_and_checkout_tag(
        path: &Path,
        tag_name: &str,
        allow_dirty_worktree: bool,
    ) -> Result<(), SkillSyncError> {
        let repo = Repository::discover(path)?;

        // A forced checkout is destructive and is only reachable after an
        // explicit confirmation in the UI. The update transaction still
        // creates its own snapshot before this point.
        if !allow_dirty_worktree && !Self::is_worktree_clean(path)? {
            return Err(SkillSyncError::WorktreeDirty);
        }

        // 1. Fetch tags from remote origin using git CLI (uses system credentials/keys)
        let _ = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "fetch",
                "--tags",
                "origin",
            ])
            .output();

        // 2. Also attempt fetch via libgit2
        if let Ok(mut remote) = repo.find_remote("origin") {
            let refspec = "+refs/tags/*:refs/tags/*";
            let _ = remote.fetch(&[refspec], None, None);
        }

        // Candidates to resolve (supporting both v1.2.3 and 1.2.3, branch, and remote refs)
        let clean = tag_name.trim_start_matches(['v', 'V']);
        let with_v = format!("v{}", clean);
        let candidates = vec![
            format!("refs/tags/{}", tag_name),
            format!("refs/tags/{}", with_v),
            format!("refs/tags/{}", clean),
            tag_name.to_string(),
            with_v.clone(),
            clean.to_string(),
            format!("origin/{}", tag_name),
            format!("origin/{}", with_v),
            format!("origin/{}", clean),
        ];

        let mut target_commit = None;
        for cand in &candidates {
            if let Ok(reference) = repo.find_reference(cand) {
                if let Ok(commit) = reference.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
            if let Ok(reference) = repo.resolve_reference_from_short_name(cand) {
                if let Ok(commit) = reference.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
            if let Ok(obj) = repo.revparse_single(cand) {
                if let Ok(commit) = obj.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
        }

        let commit = match target_commit {
            Some(c) => c,
            None => {
                // CLI fallback checkout
                for cand in &[&with_v, &clean.to_string(), &tag_name.to_string()] {
                    let mut command = std::process::Command::new("git");
                    command.arg("-C").arg(path).arg("checkout");
                    if allow_dirty_worktree {
                        command.arg("--force");
                    }
                    let res = command.arg(cand).output();
                    if let Ok(out) = res {
                        if out.status.success() {
                            return Ok(());
                        }
                    }
                }
                return Err(SkillSyncError::Git {
                    code: -3,
                    message: format!(
                        "Nie znaleziono tagu ani referencji Git dla '{}' ani '{}'",
                        tag_name, with_v
                    ),
                });
            }
        };

        if allow_dirty_worktree {
            Self::preserve_checkout_conflicts(path, &repo, &commit)?;
        }

        // Checkout target commit
        let mut checkout = CheckoutBuilder::new();
        if allow_dirty_worktree {
            checkout.force();
        } else {
            checkout.safe();
        }
        repo.checkout_tree(commit.as_object(), Some(&mut checkout))?;
        repo.set_head_detached(commit.id())?;

        Ok(())
    }

    /// Preserve local files that Git cannot replace during a forced checkout.
    ///
    /// `git status` intentionally ignores untracked files for the normal
    /// cleanliness check because skills commonly generate local files. Git
    /// still refuses a checkout when one of those files occupies a path that
    /// the target commit needs (most notably a local directory replacing a
    /// tracked symlink, as used by gstack). Move only those collisions outside
    /// the repository; the atomic snapshot created by the orchestrator remains
    /// the rollback source for the complete pre-update tree.
    fn preserve_checkout_conflicts(
        path: &Path,
        repo: &Repository,
        commit: &Commit<'_>,
    ) -> Result<(), SkillSyncError> {
        let tree = commit.tree()?;
        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true);
        let statuses = repo.statuses(Some(&mut options))?;

        let mut conflicts: Vec<PathBuf> = statuses
            .iter()
            .filter_map(|entry| {
                let status = entry.status();
                if !(status.contains(Status::WT_NEW) || status.contains(Status::WT_TYPECHANGE)) {
                    return None;
                }
                let relative = PathBuf::from(entry.path().ok()?);
                tree.get_path(&relative).ok()?;
                Some(relative)
            })
            .collect();
        conflicts.sort_by_key(|path| path.components().count());
        conflicts.dedup();
        let all_conflicts = conflicts.clone();
        conflicts.retain(|candidate| {
            !all_conflicts
                .iter()
                .any(|other| other != candidate && candidate.starts_with(other))
        });

        if conflicts.is_empty() {
            return Ok(());
        }

        let worktree_root = repo.workdir().unwrap_or(path);
        let parent = worktree_root.parent().unwrap_or(worktree_root);
        let conflict_root = parent.join(format!(
            ".skillsync-conflicts-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&conflict_root)?;

        for relative in &conflicts {
            let source = worktree_root.join(relative);
            if !fs::symlink_metadata(&source).is_ok() {
                continue;
            }
            let destination = conflict_root.join(relative);
            if let Some(destination_parent) = destination.parent() {
                fs::create_dir_all(destination_parent)?;
            }
            fs::rename(&source, &destination).map_err(|error| {
                SkillSyncError::FileSystem(format!(
                    "Nie można zachować lokalnego konfliktu {} przed checkoutem: {}",
                    source.display(),
                    error
                ))
            })?;
        }

        let manifest = conflict_root.join("README.txt");
        fs::write(
            manifest,
            "SkillSync preserved local files before a forced Git update. Restore them manually if needed; the atomic SkillSync snapshot also contains the complete pre-update tree.\n",
        )?;
        Ok(())
    }

    pub fn get_latest_local_tag(path: &Path) -> Option<String> {
        if !path.join(".git").exists() {
            return None;
        }
        let repo = Repository::open(path).ok()?;
        let tags = repo.tag_names(None).ok()?;
        let mut tag_list: Vec<String> =
            tags.iter().flatten().flatten().map(str::to_owned).collect();

        // Sort descending by SemVer
        tag_list.sort_by(|a, b| {
            let clean_a = a.trim_start_matches(['v', 'V']);
            let clean_b = b.trim_start_matches(['v', 'V']);
            match (
                semver::Version::parse(clean_a),
                semver::Version::parse(clean_b),
            ) {
                (Ok(va), Ok(vb)) => vb.cmp(&va),
                _ => b.cmp(a),
            }
        });

        tag_list.into_iter().next()
    }

    pub fn checkout_ref(path: &Path, target_ref: &str) -> Result<(), SkillSyncError> {
        let repo = Repository::open(path)?;

        if !Self::is_worktree_clean(path)? {
            return Err(SkillSyncError::WorktreeDirty);
        }

        // Try fetch first via CLI and git2
        let _ = std::process::Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap_or("."),
                "fetch",
                "--all",
                "--tags",
            ])
            .output();

        if let Ok(mut remote) = repo.find_remote("origin") {
            let _ = remote.fetch(
                &["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"],
                None,
                None,
            );
        }

        let clean = target_ref.trim_start_matches(['v', 'V']);
        let with_v = format!("v{}", clean);
        let candidates = vec![
            target_ref.to_string(),
            format!("refs/tags/{}", target_ref),
            format!("refs/tags/{}", with_v),
            format!("refs/tags/{}", clean),
            format!("origin/{}", target_ref),
            format!("origin/{}", with_v),
            with_v.clone(),
            clean.to_string(),
        ];

        let mut target_commit = None;
        for cand in &candidates {
            if let Ok(reference) = repo.find_reference(cand) {
                if let Ok(commit) = reference.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
            if let Ok(reference) = repo.resolve_reference_from_short_name(cand) {
                if let Ok(commit) = reference.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
            if let Ok(obj) = repo.revparse_single(cand) {
                if let Ok(commit) = obj.peel_to_commit() {
                    target_commit = Some(commit);
                    break;
                }
            }
        }

        let commit = match target_commit {
            Some(c) => c,
            None => {
                // CLI checkout fallback
                for cand in &[target_ref, &with_v, clean] {
                    let res = std::process::Command::new("git")
                        .args(["-C", path.to_str().unwrap_or("."), "checkout", cand])
                        .output();
                    if let Ok(out) = res {
                        if out.status.success() {
                            return Ok(());
                        }
                    }
                }
                return Err(SkillSyncError::Git {
                    code: -3,
                    message: format!("Nie znaleziono referencji Git dla '{}'", target_ref),
                });
            }
        };

        repo.checkout_tree(commit.as_object(), None)?;
        repo.set_head_detached(commit.id())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn committed_repo(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "skillsync-git-test-{}-{}",
            name,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&path).unwrap();
        let repo = Repository::init(&path).unwrap();
        fs::write(path.join("SKILL.md"), "initial\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("SKILL.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("SkillSync test", "tests@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        path
    }

    #[test]
    fn untracked_files_do_not_block_an_update() {
        let path = committed_repo("untracked");
        fs::write(path.join("local-notes.txt"), "keep me\n").unwrap();

        assert!(GitService::is_worktree_clean(&path).unwrap());

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn rejects_a_repository_url_as_a_branch_name() {
        assert!(!GitService::is_valid_branch_name(
            "https://github.com/ayghri/i-have-adhd"
        ));
        assert!(!GitService::is_valid_branch_name("feature//branch"));
        assert!(!GitService::is_valid_branch_name("/feature"));
        assert!(!GitService::is_valid_branch_name("feature/"));
        assert!(GitService::is_valid_branch_name("main"));
        assert!(GitService::is_valid_branch_name("feature/branch"));
    }

    #[test]
    fn chooses_the_highest_semver_remote_tag_and_preserves_its_spelling() {
        let refs = concat!(
            "deadbeef\trefs/tags/v2.9.0\n",
            "deadbeef\trefs/tags/v2.11.1\n",
            "deadbeef\trefs/tags/nightly\n",
            "deadbeef\trefs/tags/1.10.0\n",
            "deadbeef\trefs/tags/v2.11.1-rc.1\n",
        );

        assert_eq!(
            GitService::latest_semver_tag_from_ls_remote(refs),
            Some("v2.11.1".to_string())
        );
    }

    #[test]
    fn ignores_non_semver_tags_when_deciding_whether_branch_tracking_is_needed() {
        assert_eq!(
            GitService::latest_semver_tag_from_ls_remote(
                "deadbeef\trefs/tags/main\ndeadbeef\trefs/tags/nightly\n"
            ),
            None
        );
    }

    #[test]
    fn tracked_file_changes_still_block_an_update() {
        let path = committed_repo("modified");
        fs::write(path.join("SKILL.md"), "locally changed\n").unwrap();

        assert!(!GitService::is_worktree_clean(&path).unwrap());

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn recognizes_only_the_legacy_skill_version_bookkeeping_change() {
        let path = committed_repo("legacy-version-metadata");
        let repo = Repository::open(&path).unwrap();
        let original =
            "---\nname: fixture\nmetadata:\n  short-description: Fixture\n---\n# Fixture\n";
        fs::write(path.join("SKILL.md"), original).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("SKILL.md")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let signature = git2::Signature::now("SkillSync test", "tests@example.invalid").unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "skill frontmatter",
            &tree,
            &[&parent],
        )
        .unwrap();

        fs::write(
            path.join("SKILL.md"),
            "---\nname: fixture\nmetadata:\n  version: \"1.1.0\"\n  short-description: Fixture\n---\n# Fixture\n",
        )
        .unwrap();
        assert!(GitService::has_only_skill_version_metadata_change(&path).unwrap());

        fs::write(
            path.join("SKILL.md"),
            "---\nname: fixture\ndescription: changed\nmetadata:\n  version: \"1.1.0\"\n  short-description: Fixture\n---\n# Fixture\n",
        )
        .unwrap();
        assert!(!GitService::has_only_skill_version_metadata_change(&path).unwrap());

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn deleted_tracked_files_do_not_block_an_update() {
        let path = committed_repo("deleted");
        fs::remove_file(path.join("SKILL.md")).unwrap();

        assert!(GitService::is_worktree_clean(&path).unwrap());

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn detects_the_checked_out_branch_and_head_commit() {
        let path = committed_repo("branch-detection");
        let expected_branch = Repository::open(&path)
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string();

        assert_eq!(
            GitService::get_current_branch_name(&path),
            Some(expected_branch)
        );
        assert!(GitService::get_head_commit(&path).is_some());

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn confirmed_force_update_restores_the_selected_tag() {
        let path = committed_repo("force-checkout");
        let repo = Repository::open(&path).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("v1.0.0", head.as_object(), false)
            .unwrap();
        fs::write(path.join("SKILL.md"), "locally changed\n").unwrap();

        assert!(matches!(
            GitService::fetch_and_checkout_tag(&path, "v1.0.0", false),
            Err(SkillSyncError::WorktreeDirty)
        ));

        GitService::fetch_and_checkout_tag(&path, "v1.0.0", true).unwrap();
        let restored = fs::read_to_string(path.join("SKILL.md")).unwrap();
        assert_eq!(restored.replace("\r\n", "\n"), "initial\n");

        let _ = fs::remove_dir_all(path);
    }
}
