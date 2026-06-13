use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single recorded command entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Auto-assigned by DB on insert; 0 means not yet persisted.
    pub id: i64,

    /// The raw command string as typed.
    pub command: String,

    /// Absolute path of the working directory at the time of execution.
    pub cwd: String,

    /// Git repo root if cwd is inside a git repository.
    pub git_repo: Option<String>,

    /// Active git branch at time of execution.
    pub git_branch: Option<String>,

    /// Exit code. None means the command is still running (wrapper mode).
    pub exit_code: Option<i32>,

    /// Wall-clock duration in milliseconds. None until command completes.
    pub duration_ms: Option<i64>,

    /// UTC timestamp when the command started.
    pub started_at: DateTime<Utc>,
}

impl Entry {
    pub fn new(
        command: impl Into<String>,
        cwd: impl Into<String>,
        git_repo: Option<String>,
        git_branch: Option<String>,
    ) -> Self {
        Self {
            id: 0,
            command: command.into(),
            cwd: cwd.into(),
            git_repo,
            git_branch,
            exit_code: None,
            duration_ms: None,
            started_at: Utc::now(),
        }
    }

    /// Returns true if this entry succeeded (exit code 0).
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Returns true if this entry is complete (has exit code).
    pub fn is_complete(&self) -> bool {
        self.exit_code.is_some()
    }
}

/// Compact form sent over the Unix socket from shell hooks.
/// Keeps the IPC payload small and avoids sending fields the shell cannot know.
#[derive(Debug, Serialize, Deserialize)]
pub struct HookPayload {
    pub command: String,
    pub cwd: String,
    pub exit_code: i32,
    pub duration_ms: i64,
}
