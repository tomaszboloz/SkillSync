use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The installable artifact managed by SkillSync. Each type has its own
/// manifest validator; a package is never reclassified solely by its path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ManagedItemType {
    #[default]
    Skill,
    Mcp,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentScope {
    #[default]
    Global,
    Codex,
    Claude,
    Cursor,
    Antigravity,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    #[default]
    UpToDate,
    UpdateAvailable,
    ModifiedLocally,
    Corrupted,
    Checking,
    Updating,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    #[serde(default)]
    pub item_type: ManagedItemType,
    pub id: String,
    pub name: String,
    /// Presentation only; never use this label to resolve upstream files.
    #[serde(default)]
    pub display_name: Option<String>,
    pub description: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub author: String,
    pub path: PathBuf,
    pub is_git_repo: bool,
    pub remote_url: Option<String>,
    pub branch_or_tag: Option<String>,
    /// Branch reported by the local checkout before a user override is applied.
    #[serde(default)]
    pub detected_branch: Option<String>,
    /// Explicit tracking branch stored in settings; it wins over auto-detection.
    #[serde(default)]
    pub branch_override: Option<String>,
    pub agent_scope: AgentScope,
    pub status: SkillStatus,
    pub update_available: bool,
    pub changelog: Option<String>,
    pub dependencies: Vec<String>,
    pub permissions: Vec<String>,
    pub last_checked: DateTime<Utc>,
    pub compatibility: Option<String>,
    pub update_compatibility: Option<String>,
    #[serde(default)]
    pub installed_locations: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub skill_id: String,
    pub skill_name: String,
    pub stage: UpdateStage,
    pub percentage: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Validating,
    BackingUp,
    Fetching,
    CheckingOut,
    Verifying,
    RollingBack,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSnapshot {
    pub snapshot_id: String,
    pub skill_id: String,
    pub created_at: DateTime<Utc>,
    pub backup_file_path: PathBuf,
    pub original_version: String,
}
