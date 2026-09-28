import {
  AppConfig,
  AppUpdateProgress,
  AppUpdateInfo,
  BackupSnapshot,
  SkillMetadata,
} from "../types/skillsync";

const APP_RELEASES_URL = "https://github.com/tomaszboloz/SkillSync/releases";
const isDevelopmentBuild = import.meta.env.DEV;

type AppUpdateProgressHandler = (progress: AppUpdateProgress) => void;

// Check if running inside Tauri window
export const isTauriEnvironment = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

// Initial Mock Data for testing & browser preview
const MOCK_SKILLS: SkillMetadata[] = [
  {
    id: "skill-claude-seo",
    name: "claude-seo",
    description:
      "Comprehensive SEO analysis skill for Claude Code with parallel subagent delegation, technical auditing, and schema validation.",
    currentVersion: "2.0.0",
    latestVersion: "2.3.1",
    author: "AgriciDaniel",
    path: "~/.claude/skills/seo-audit",
    isGitRepo: false,
    remoteUrl: "https://github.com/AgriciDaniel/claude-seo",
    branchOrTag: null,
    agentScope: "claude",
    status: "update_available",
    updateAvailable: true,
    changelog:
      "## v2.3.1\n- **Feature:** Advanced multi-page citability scoring\n- **Fix:** Schema.org breadcrumb validation for e-commerce sites\n- **Perf:** 40% faster subagent batch crawler execution",
    dependencies: ["python>=3.10", "uv>=0.4.0"],
    permissions: ["network:outbound", "filesystem:read"],
    lastChecked: new Date().toISOString(),
    compatibility: "Claude Code (v1.0+) • Python >= 3.10",
    updateCompatibility:
      "SemVer Minor (v2.x) — 100% Wstecznie kompatybilna aktualizacja (brak zmian łamiących)",
  },
  {
    id: "skill-web-search-pro",
    name: "web-search-pro",
    description:
      "Autonomous multi-source search and content synthesis engine for AI agents with duckduckgo and google scraping.",
    currentVersion: "1.4.2",
    latestVersion: "1.5.0",
    author: "SkillSync Core Team",
    path: "~/.claude/skills/web-search-pro",
    isGitRepo: true,
    remoteUrl: "https://github.com/skillsync/web-search-pro.git",
    branchOrTag: "v1.4.2",
    agentScope: "claude",
    status: "update_available",
    updateAvailable: true,
    changelog:
      "## v1.5.0 (2026-09-15)\n- **Feature:** Added structured markdown output filtering\n- **Fix:** Fixed rate-limiting retry mechanism\n- **Perf:** 30% faster HTML parsing with AST-based cleanup",
    dependencies: ["curl-impersonate>=0.6.1"],
    permissions: ["network:outbound", "filesystem:temp"],
    lastChecked: new Date().toISOString(),
    compatibility: "Claude Code (v1.0+)",
    updateCompatibility:
      "SemVer Minor (v1.x) — Kompatybilna aktualizacja funkcji",
  },
  {
    id: "skill-code-analyzer",
    name: "code-analyzer",
    description:
      "AST-aware architectural review, dependency vulnerability audit, and complexity metrics computation.",
    currentVersion: "2.1.0",
    latestVersion: "2.1.0",
    author: "DevOps Lead",
    path: "~/.config/skills/code-analyzer",
    isGitRepo: true,
    remoteUrl: "https://github.com/skillsync/code-analyzer.git",
    branchOrTag: "v2.1.0",
    agentScope: "global",
    status: "up_to_date",
    updateAvailable: false,
    changelog:
      "## v2.1.0\n- Initial stable release with tree-sitter integration.",
    dependencies: ["tree-sitter"],
    permissions: ["filesystem:read"],
    lastChecked: new Date().toISOString(),
  },
  {
    id: "skill-cursor-rule-engine",
    name: "cursor-rule-engine",
    description:
      "Enforces high-context semantic rules, auto-imports standards, and style tokens across Cursor IDE workflows.",
    currentVersion: "0.9.4",
    latestVersion: "1.0.0",
    author: "Frontend Guild",
    path: "~/.cursor/extensions/cursor-rule-engine",
    isGitRepo: true,
    remoteUrl: "https://github.com/skillsync/cursor-rule-engine.git",
    branchOrTag: "v0.9.4",
    agentScope: "cursor",
    status: "update_available",
    updateAvailable: true,
    changelog:
      "## v1.0.0\n- **Major:** Full support for Cursor v0.40+ rule specifications\n- **Breaking:** Removed legacy .cursorrules v1 format",
    dependencies: [],
    permissions: ["filesystem:read", "filesystem:write"],
    lastChecked: new Date().toISOString(),
  },
  {
    id: "skill-antigravity-orchestrator",
    name: "antigravity-orchestrator",
    description:
      "Autonomous sidecar manager for Google Antigravity custom plugins and long-running subagent swarms.",
    currentVersion: "3.0.1",
    latestVersion: "3.0.1",
    author: "DeepMind Devs",
    path: "~/Library/Application Support/Antigravity/skills/orchestrator",
    isGitRepo: true,
    remoteUrl: "https://github.com/google/antigravity-skills.git",
    branchOrTag: "v3.0.1",
    agentScope: "antigravity",
    status: "up_to_date",
    updateAvailable: false,
    changelog:
      "## v3.0.1\n- Patch release fixing subagent communication channel teardown.",
    dependencies: ["grpc"],
    permissions: ["network:all", "process:spawn"],
    lastChecked: new Date().toISOString(),
  },
  {
    id: "skill-local-custom-prompt",
    name: "brand-voice-synthesizer",
    description:
      "Translates technical documentation into client-ready executive briefs adhering to company tone guidelines.",
    currentVersion: "1.0.0",
    latestVersion: null,
    author: "Local Workspace",
    path: "~/.config/skills/brand-voice",
    isGitRepo: false,
    remoteUrl: null,
    branchOrTag: null,
    agentScope: "global",
    status: "modified_locally",
    updateAvailable: false,
    changelog: null,
    dependencies: [],
    permissions: [],
    lastChecked: new Date().toISOString(),
  },
];

const DEFAULT_CONFIG: AppConfig = {
  general: {
    language: "en",
    launchAtLogin: false,
    minimizeToTray: true,
    showTrayIcon: true,
    checkAppUpdates: true,
  },
  paths: {
    monitored: [
      { id: "p1", path: "~/.config/skills", scope: "global", enabled: true },
      { id: "p2", path: "~/.claude/skills", scope: "claude", enabled: true },
      {
        id: "p3",
        path: "~/.cursor/extensions",
        scope: "cursor",
        enabled: true,
      },
    ],
    defaultInstallDirectory: "~/.config/skills",
  },
  updates: {
    autoCheckFrequency: "every_6_hours",
    autoInstall: "ask",
    concurrencyLimit: 4,
    backupRetentionDays: 14,
    allowPrerelease: false,
    branchOverrides: {},
    repositoryOverrides: {},
  },
  notifications: {
    enabled: true,
    onUpdateFound: true,
    onUpdateSuccess: true,
    onUpdateFailure: true,
    sound: true,
  },
  appearance: {
    theme: "dark",
    accentColor: "violet",
    reducedMotion: false,
    compactView: false,
  },
  advanced: {
    logLevel: "info",
    gitTimeoutSeconds: 30,
    customGitBinary: null,
    cacheTtlMinutes: 5,
  },
};

export const api = {
  async scanSkills(forceRefresh: boolean = false): Promise<SkillMetadata[]> {
    if (isTauriEnvironment()) {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        return await invoke<SkillMetadata[]>("scan_skills", { forceRefresh });
      } catch (err) {
        console.error("Tauri IPC scan_skills failed, fallback to mock:", err);
      }
    }
    // Simulate slight network/disk delay
    await new Promise((r) => setTimeout(r, 250));
    // Used only by the browser preview to capture documentation without
    // exposing a real user's paths, installed skills, or release history.
    if (
      typeof window !== "undefined" &&
      new URLSearchParams(window.location.search).get("preview") === "empty"
    ) {
      return [];
    }
    return [...MOCK_SKILLS];
  },

  async updateSingleSkill(
    skillId: string,
    targetVersion?: string,
    force: boolean = false,
  ): Promise<SkillMetadata> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<SkillMetadata>("update_single_skill", {
        skillId,
        targetVersion,
        force,
      });
    }
    // Simulate update progression
    await new Promise((r) => setTimeout(r, 600));
    const skill = MOCK_SKILLS.find((s) => s.id === skillId);
    if (!skill) throw new Error(`Skill with ID ${skillId} not found`);

    const updated = {
      ...skill,
      currentVersion: skill.latestVersion || skill.currentVersion,
      updateAvailable: false,
      status: "up_to_date" as const,
      lastChecked: new Date().toISOString(),
    };
    return updated;
  },

  async batchUpdateSkills(
    skillIds: string[],
  ): Promise<{ succeeded: number; failed: number }> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<{ succeeded: number; failed: number }>(
        "batch_update_skills",
        { skillIds },
      );
    }
    await new Promise((r) => setTimeout(r, 1200));
    return { succeeded: skillIds.length, failed: 0 };
  },

  async rollbackSkill(skillId: string, snapshotId?: string): Promise<boolean> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<boolean>("rollback_skill", { skillId, snapshotId });
    }
    await new Promise((r) => setTimeout(r, 400));
    return true;
  },

  async getConfig(): Promise<AppConfig> {
    if (isTauriEnvironment()) {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        return await invoke<AppConfig>("get_config");
      } catch (err) {
        console.warn("Failed to load Tauri config, using default:", err);
      }
    }
    return DEFAULT_CONFIG;
  },

  async saveConfig(config: AppConfig): Promise<void> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<void>("save_config", { config });
    }
    console.log("Mock saved config:", config);
  },

  async openInEditor(path: string): Promise<void> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<void>("open_in_editor", { path });
    }
    console.log("Mock openInEditor:", path);
  },

  async getBackups(skillId: string): Promise<BackupSnapshot[]> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<BackupSnapshot[]>("get_backups_list", { skillId });
    }
    return [
      {
        snapshotId: "snap-001",
        skillId,
        createdAt: new Date(Date.now() - 3600000).toISOString(),
        backupFilePath: `~/.skillsync/backups/${skillId}_pre_update.tar.gz`,
        originalVersion: "1.4.2",
      },
    ];
  },

  async checkGitHubUpdate(skillId: string): Promise<SkillMetadata> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<SkillMetadata>("check_github_update", { skillId });
    }
    await new Promise((r) => setTimeout(r, 450));
    const skill = MOCK_SKILLS.find((s) => s.id === skillId);
    if (!skill) throw new Error(`Skill ${skillId} not found`);

    if (skill.remoteUrl) {
      const updated = {
        ...skill,
        latestVersion: skill.latestVersion || "1.6.0",
        updateAvailable: true,
        status: "update_available" as const,
        changelog: `## Latest GitHub Release\n- Synchronized from ${skill.remoteUrl}\n- Verified upstream changes.`,
        lastChecked: new Date().toISOString(),
      };
      return updated;
    }
    return skill;
  },

  async setBranchOverride(
    skillId: string,
    branch: string | null,
  ): Promise<void> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_branch_override", { skillId, branch });
    }
  },

  async setRepositoryOverride(
    skillId: string,
    repository: string | null,
  ): Promise<void> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_repository_override", { skillId, repository });
    }
  },

  async removeSkill(skillId: string, locations: string[]): Promise<string[]> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<string[]>("remove_skill", { skillId, locations });
    }
    return locations;
  },

  async checkAppUpdate(): Promise<AppUpdateInfo> {
    if (isTauriEnvironment()) {
      const { getVersion } = await import("@tauri-apps/api/app");
      const currentVersion = await getVersion();

      // A local `tauri dev` build cannot be updated from a public release and
      // a release manifest may not yet exist while a release is being built.
      // Treat this as a successful development check rather than showing a
      // production-network error in the footer or Settings.
      if (isDevelopmentBuild) {
        return {
          currentVersion,
          latestVersion: null,
          updateAvailable: false,
          releaseUrl: APP_RELEASES_URL,
        };
      }

      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();

      if (update) {
        return {
          currentVersion: update.currentVersion,
          latestVersion: update.version,
          updateAvailable: true,
          releaseUrl: APP_RELEASES_URL,
        };
      }

      return {
        currentVersion,
        latestVersion: null,
        updateAvailable: false,
        releaseUrl: APP_RELEASES_URL,
      };
    }

    return {
      currentVersion: "Browser preview",
      latestVersion: null,
      updateAvailable: false,
      releaseUrl: APP_RELEASES_URL,
    };
  },

  async getAppVersion(): Promise<string> {
    if (isTauriEnvironment()) {
      const { getVersion } = await import("@tauri-apps/api/app");
      return await getVersion();
    }

    return "Browser preview";
  },

  async installAppUpdate(onProgress: AppUpdateProgressHandler): Promise<void> {
    if (!isTauriEnvironment()) {
      throw new Error(
        "Automatic updates are available only in the installed SkillSync desktop app.",
      );
    }

    const [{ check }, { relaunch }] = await Promise.all([
      import("@tauri-apps/plugin-updater"),
      import("@tauri-apps/plugin-process"),
    ]);
    const update = await check();
    if (!update) {
      throw new Error("No newer SkillSync update is available.");
    }

    let downloadedBytes = 0;
    await update.downloadAndInstall(
      (event) => {
        if (event.event === "Started") {
          onProgress({
            phase: "downloading",
            downloadedBytes: 0,
            contentLength: event.data.contentLength ?? null,
          });
          return;
        }

        if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          onProgress({
            phase: "downloading",
            downloadedBytes,
            contentLength: null,
          });
          return;
        }

        onProgress({
          phase: "installing",
          downloadedBytes,
          contentLength: null,
        });
      },
      { restartAfterInstall: true },
    );

    onProgress({
      phase: "restarting",
      downloadedBytes,
      contentLength: null,
    });
    await relaunch();
  },

  async checkoutCustomVersion(
    skillId: string,
    targetRef: string,
  ): Promise<SkillMetadata> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<SkillMetadata>("checkout_custom_version", {
        skillId,
        targetRef,
      });
    }
    await new Promise((r) => setTimeout(r, 600));
    const skill = MOCK_SKILLS.find((s) => s.id === skillId);
    if (!skill) throw new Error(`Skill ${skillId} not found`);

    const updated = {
      ...skill,
      currentVersion: targetRef,
      updateAvailable: false,
      status: "up_to_date" as const,
      lastChecked: new Date().toISOString(),
    };
    return updated;
  },

  async openUrl(url: string): Promise<void> {
    if (isTauriEnvironment()) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_url", { url });
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  },
};
