use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A saved command shortcut scoped to a project or global.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcut {
    /// Auto-assigned by DB on insert; 0 means not yet persisted.
    pub id: i64,
    /// The alias used to invoke this shortcut.
    pub alias: String,
    /// The full command string the alias expands to.
    pub command: String,
    /// Absolute path to the project root this shortcut belongs to.
    pub project_dir: String,
    /// Git repo root, if the project is inside a git repository.
    pub git_repo: Option<String>,
    /// Whether this shortcut is available across all projects.
    pub is_global: bool,
    /// UTC timestamp when the shortcut was created.
    pub created_at: DateTime<Utc>,
}

impl Shortcut {
    pub fn new(
        alias: impl Into<String>,
        command: impl Into<String>,
        project_dir: impl Into<String>,
        git_repo: Option<String>,
        is_global: bool,
    ) -> Self {
        Self {
            id: 0,
            alias: alias.into(),
            command: command.into(),
            project_dir: project_dir.into(),
            git_repo,
            is_global,
            created_at: Utc::now(),
        }
    }

    /// Returns true if this shortcut applies to all projects.
    pub fn global(&self) -> bool {
        self.is_global
    }
}
