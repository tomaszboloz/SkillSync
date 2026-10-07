export type AgentScope =
  "global" | "codex" | "claude" | "cursor" | "antigravity" | string;

export type SkillStatus =
  | "up_to_date"
  | "update_available"
  | "modified_locally"
  | "corrupted"
  | "checking"
  | "updating"
  | "error"
  | { error: string };

export type ManagedItemType = "skill" | "mcp" | "plugin";

export type UpdateStage =
  | "validating"
  | "backing_up"
  | "fetching"
  | "checking_out"
  | "verifying"
  | "rolling_back"
  | "completed"
  | "failed";

export interface SkillMetadata {
  displayName?: string | null;
  id: string;
  itemType?: ManagedItemType;
  name: string;
  description: string;
  currentVersion: string;
  latestVersion: string | null;
  author: string;
  path: string;
  isGitRepo: boolean;
  remoteUrl: string | null;
  branchOrTag: string | null;
  detectedBranch?: string | null;
  branchOverride?: string | null;
  agentScope: AgentScope;
  status: SkillStatus;
  updateAvailable: boolean;
  changelog: string | null;
  dependencies: string[];
  permissions: string[];
  lastChecked: string;
  compatibility?: string | null;
  updateCompatibility?: string | null;
  installedLocations?: string[];
}

export interface UpdateProgressPayload {
  skillId: string;
  skillName: string;
  stage: UpdateStage;
  percentage: number;
  message: string;
}

export interface MonitoredPath {
  id: string;
  path: string;
  scope: string;
  itemType?: ManagedItemType;
  customLabel?: string;
  enabled: boolean;
}

export interface AppConfig {
  general: {
    language: string;
    launchAtLogin: boolean;
    minimizeToTray: boolean;
    showTrayIcon: boolean;
    checkAppUpdates: boolean;
  };
  paths: {
    monitored: MonitoredPath[];
    defaultInstallDirectory: string;
  };
  updates: {
    autoCheckFrequency: "hourly" | "every_6_hours" | "daily" | "manual";
    autoInstall: "ask" | "always" | "never";
    concurrencyLimit: number;
    backupRetentionDays: number;
    allowPrerelease: boolean;
    branchOverrides: Record<string, string>;
    repositoryOverrides: Record<string, string>;
  };
  notifications: {
    enabled: boolean;
    onUpdateFound: boolean;
    onUpdateSuccess: boolean;
    onUpdateFailure: boolean;
    sound: boolean;
  };
  appearance: {
    theme: "system" | "dark" | "light";
    accentColor: string;
    reducedMotion: boolean;
    compactView: boolean;
  };
  advanced: {
    logLevel: "debug" | "info" | "warn" | "error";
    gitTimeoutSeconds: number;
    customGitBinary: string | null;
    cacheTtlMinutes: number;
  };
}

export interface BackupSnapshot {
  snapshotId: string;
  skillId: string;
  createdAt: string;
  backupFilePath: string;
  originalVersion: string;
}

export interface AppUpdateInfo {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string;
}

export type AppUpdatePhase =
  "idle" | "checking" | "downloading" | "installing" | "restarting" | "error";

export interface AppUpdateProgress {
  phase: AppUpdatePhase;
  downloadedBytes: number;
  contentLength: number | null;
}
