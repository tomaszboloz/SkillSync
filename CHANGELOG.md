# Changelog

All notable changes follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/).

## [1.4.4] - 2026-09-28

### Fixed

- A skill installed in Codex, Claude Code, Gemini, Cursor or another monitored
  agent directory is now represented by one logical card.
- All canonical installations are retained on that card and updated as one
  transaction; source-less mirrored copies attach only when exactly one
  matching upstream exists.
- Distinct configured symlink aliases remain visible as separate paths while
  the transaction still mutates one canonical physical checkout.
- Same-named skills from different upstream repositories remain isolated and
  receive an explicit repository suffix in the UI.
- Same-named raw GitHub manifests without a local Git checkout receive the
  same source suffix, and discovery order is deterministic across scans.
- CI now rejects Rust functions over 150 lines, preventing the update engine
  from regressing into a monolithic operation.
- Added regression coverage for cross-agent copies and ambiguous upstreams.

## [1.4.3] - 2026-09-28

### Fixed

- Identical local manifests without a resolvable upstream URL are now grouped
  into one logical resource across Claude, Codex, Cursor, Gemini and Agents
  paths. Their physical locations remain visible and update-safe.
- Different manifest contents, item types, or repositories remain independent;
  deduplication cannot merge unrelated packages merely because their names
  match.

### Verification

- The verified local account is now **534 logical resources** instead of 679:
  477 Skills, 57 Plugins and 0 MCP, backed by 734 physical installations.

## [1.4.2] - 2026-09-28

### Fixed

- Exclude inactive and active Codex cache snapshots from the generic Skill
  detector. Codex-owned plugins are represented once by the authoritative
  plugin registry and cannot be routed to a Git update accidentally.
- Read the Codex plugin registry once per relevant scan, avoiding repeated CLI
  calls and mixed registry states.
- Migrate persisted monitored paths by removing obsolete
  `~/.codex/plugins/cache/**` Skill entries and canonical-path duplicates.

### Verification

- Resource accounting reports logical resources separately from physical
  installations, so shared locations do not inflate the total.

## [1.4.1] - 2026-09-28

### Added

- Add an owner-safe Codex plugin adapter: active marketplace plugins are
  refreshed only through `codex plugin marketplace upgrade` and `codex plugin
  add`, then checked for a valid manifest.

### Fixed

- Ignore immutable, historical Codex cache directories such as
  `~/.codex/plugins/cache/openai-curated/*`; they are not Git repositories and
  cannot be updated safely by SkillSync.
- Ignore uninstalled entries in the Codex marketplace catalog while retaining
  active entries from the authoritative Codex registry.
- Deduplicate local resources by canonical path and keep same-named resources
  from different sources independent.

### Security

- Codex plugin updates no longer copy cache files or execute package-provided
  scripts; the owning CLI remains the only update authority.

## [1.4.0] - 2026-09-28

### Added

- Discover skills nested in Claude marketplace repositories, including the
  `.claude/skills` layout used by Lex-Machina.
- Update registered Claude Code marketplaces through the official
  `claude plugin marketplace update` adapter, with registry and manifest
  verification.
- Add a 70-item, evidence-backed quality GAP register in
  `docs/GAP_REGISTER.md`.

### Fixed

- Keep same-named skills and plugins from different GitHub repositories as
  independent resources instead of merging their locations into one unsafe
  transaction. This prevents `seo-audit` installations from rolling back due
  to unrelated release lines.
- Resolve one release tag per upstream remote before a multi-location update,
  normalize `v` prefixes during final integrity checks, and roll back on any
  upstream manifest fetch or write failure.
- Preserve unique snapshot IDs at nanosecond precision and harden external URL
  opening on Windows and other platforms.

### Security

- Marketplace and plugin updates are delegated only to the Claude Code CLI
  after registry ownership is proven; arbitrary scripts and non-registered
  directories remain unsupported.


## [1.3.1] - 2026-09-23

### Fixed

- Make location removal independent: a stale, damaged or inaccessible installation no longer blocks removal of other selected copies. Report partial success with the affected paths, and keep errors visible in the skill details dialog.
- Classify update tracking by the presence of a usable remote source, not by whether the local checkout currently has a branch. Detached HEADs and tag-based installations with a remote are no longer incorrectly listed as untracked.
- Stop managed-item discovery below valid MCP and plugin manifests so bundled examples cannot become extra cards.

### Improved

- Reuse identical upstream checks during one scan when several skills point to the same source, version and ref, reducing repeated Git/GitHub requests.

## [1.3.0] - 2026-09-21

### Added

- Add a **Remove** action for each declared installation location and a **Remove everywhere** action for the resource as a whole. The application confirms the action, validates every selected path, creates a recovery snapshot for real directories and never follows a directory symlink while removing it.

### Fixed

- Refresh the **Bez gałęzi** counter and filter immediately after a manual branch is saved. Detached HEAD hashes and Git tags are no longer incorrectly treated as tracking branches.
- Stop discovery below a directory that is already a valid skill manifest. Examples, templates and nested `skills/` folders can no longer produce surplus skill cards; a manifest-free collection still exposes each of its independent child skills.
- Report each physical installation once instead of adding an extra canonical-path entry for every symlinked location.

## [1.2.9] - 2026-09-20

### Fixed

- Resolve the Git checkout conflict created when a previous SkillSync update generated a local `SKILL.md` adapter for a portable package and a later upstream ref introduces its own tracked `SKILL.md`.
- Remove only an untracked adapter that contains both the SkillSync provenance marker and its fixed adapter text, and only after the atomic backup has completed. User-authored and upstream-tracked manifests are never removed.

## [1.2.8] - 2026-09-20

### Added

- Add a separate **GitHub repository to track** setting for skills that are portable directories or symlinks and therefore have no local Git remote. It accepts canonical HTTPS and SSH GitHub repository roots and safely persists the choice per resource.

### Fixed

- Refresh the open detail view after saving a tracking repository or branch. The **Check GitHub** action is now visible immediately and a verification runs automatically.
- Never send a local non-Git skill through Git branch commands when it has a manual branch. It now uses its configured GitHub repository for safe release/tag verification instead of failing with a local Git error.
- Reject GitHub tree, file, query and fragment URLs in the repository field, preventing a malformed tracking source from being saved.

## [1.2.7] - 2026-09-20

### Fixed

- Treat an exact local SemVer Git tag as the installed version for `SKILL.md`, `skill.json` and explicit skill `package.json` manifests. A repository updated to a tag can no longer revert to an older, embedded manifest version on the next scan.
- Select the highest SemVer tag when several tags point at the same checked-out commit.
- Keep branch and non-Git installations manifest-based, so commit-tracked packages continue to use their correct update policy.

## [1.2.6] - 2026-09-18

### Fixed

- Preserve symlinks inside safety snapshots instead of dereferencing them. A dangling package-manager link can no longer abort an otherwise valid update with `No such file or directory`.
- Preflight every detected installation location before any snapshot or checkout. A missing, broken or non-directory location now aborts the whole transaction with its exact path, so no copy is left on a different version.
- Treat nested resources in one Git worktree as one update operation: one dirty-state check, one snapshot and one checkout. This prevents competing checkouts from invalidating a sibling resource during a bulk update.
- Verify every declared location after the update and roll back all affected roots when manifests or detected versions diverge.
- Give every safety snapshot a unique filename, so simultaneous locations of one package cannot overwrite one another before a rollback.

### Security

- Safety archives no longer follow links outside the managed resource tree.

## [1.2.0] - 2026-09-18

### Added

- Track the detected Git branch for Git-backed Skills, MCP integrations and plugins, with a per-package manual branch override that always takes precedence.
- Add the **Bez gałęzi** filter to identify Git packages that require a tracking branch before they can be monitored against upstream commits.
- Add a persistent system menu-bar / system-tray icon setting. The icon restores the window on click and exposes explicit Show and Quit actions.

### Changed

- Split local package discovery from upstream verification. The scan returns local results immediately, then a single background worker checks one upstream source at a time with a 350 ms spacing and streams each result back to its card.
- Compare tracked branch commits with `origin` without modifying the local worktree. A branch or commit mismatch is presented as an actionable update; updating safely fetches and checks out the selected branch.
- Display non-SemVer references such as `main` without a misleading `v` prefix.

## [1.1.2] - 2026-09-17

### Fixed

- Make bulk discovery use a bounded GitHub verification queue instead of opening an unbounded burst of upstream requests.
- Retry a transiently unavailable GitHub source once and give the card an actionable error state rather than incorrectly reporting that it is up to date.
- Use one release-application path for scanning and the per-item **Check GitHub** action, so both surface the same current upstream version and update decision.

## [1.1.1] - 2026-09-17

### Fixed

- Check for a new SkillSync release automatically on application launch when the setting is enabled.
- Show the installed and available application version in a persistent bottom footer, with a manual refresh action.
- Download, verify with the embedded public key, install and restart signed SkillSync updates directly from the persistent footer.
- Reserve bottom content space so the fixed footer cannot cover the final cards.

### Security

- Publish a signed Tauri updater package and `latest.json` manifest for macOS and Windows; the private signing key is kept only in GitHub Actions secrets.

## [1.1.0] - 2026-09-16

### Added

- Unified discovery, filtering and sorting for Skills, MCP integrations and agent plugins.
- Dedicated Composer update adapter for installed Laravel Boost, including Composer validation, project tests and lockfile verification.
- Registry-aware Claude Code Marketplace plugin updates through the official `claude plugin update` command.

### Fixed

- Ignore obsolete Claude Code cache copies and use the active `installed_plugins.json` entry, preventing stale plugins such as `n8n-mcp-skills` from failing as non-Git directories.
- Make rollback exact by removing files created during a failed update before restoring the snapshot.
- Do not display a hard-coded application version in the navigation header.

### Security

- Keep plugin registry ownership with Claude Code rather than mutating Marketplace cache directories directly.
- Require an explicit supported manifest and preflight before every managed update.

## [1.0.0] - 2026-09-16

### Added

- Desktop management of valid AI-agent skills for Claude Code, Codex, Cursor, Gemini and custom monitored paths.
- Safe snapshots and rollback before skill updates.
- Native release builds for macOS Apple Silicon, macOS Intel and Windows, with SHA-256 checksums.
- Polish and English README guides, screenshots, direct-answer FAQs and release documentation.

### Fixed

- Reject ordinary `package.json` directories such as `docs`, `gallery` and workspace packages during skill discovery and update validation.
- Offer a confirmation before a forced update of a Git worktree containing modified tracked files.
- Treat a missing local version as unknown rather than `v1.0.0`.
- Apply SemVer prerelease precedence, so stable `1.9.6` does not report `1.9.6-codex.5` as an available update.

### Security

- Do not overwrite unmarked package manifests during updates.
- Validate the skill manifest after every update and restore the snapshot when validation fails.

[1.0.0]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.0.0
[1.2.0]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.2.0
[1.1.0]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.1.0
[1.1.1]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.1.1
[1.1.2]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.1.2
[1.2.6]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.2.6
[1.2.7]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.2.7
[1.2.8]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.2.8
[1.2.9]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.2.9
[1.3.0]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.3.0
[1.4.0]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.4.0
[1.4.1]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.4.1
[1.4.2]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.4.2
[1.4.3]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.4.3
[1.4.4]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.4.4
[1.3.1]: https://github.com/tomaszboloz/SkillSync/releases/tag/v1.3.1
