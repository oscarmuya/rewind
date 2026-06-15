use anyhow::{Context, Result};
use rusqlite::{Connection, params_from_iter};

use crate::db::row_to_entry;
use crate::entry::Entry;

/// Builder for filtering history queries.
#[derive(Debug, Default)]
pub struct Filter {
    pub cwd: Option<String>,
    pub project_cwd: Option<String>,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    /// If true, only return entries where exit_code = 0.
    pub only_success: bool,
    /// If true, only return entries where exit_code != 0.
    pub only_failure: bool,
    pub limit: Option<usize>,
}

impl Filter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn project_cwd(mut self, project_cwd: impl Into<String>) -> Self {
        self.project_cwd = Some(project_cwd.into());
        self
    }

    pub fn git_repo(mut self, repo: impl Into<String>) -> Self {
        self.git_repo = Some(repo.into());
        self
    }

    pub fn git_branch(mut self, branch: impl Into<String>) -> Self {
        self.git_branch = Some(branch.into());
        self
    }

    pub fn only_success(mut self) -> Self {
        self.only_success = true;
        self
    }

    pub fn only_failure(mut self) -> Self {
        self.only_failure = true;
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

/// Runs a structured query with the given filter, returning matching entries
/// ordered newest-first.
pub fn fetch(conn: &Connection, filter: &Filter) -> Result<Vec<Entry>> {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(cwd) = &filter.cwd {
        conditions.push("cwd = ?".to_string());
        binds.push(Box::new(cwd.clone()));
    }

    if let Some(project_cwd) = &filter.project_cwd {
        conditions.push("project_cwd = ?".to_string());
        binds.push(Box::new(project_cwd.clone()));
    }

    if let Some(repo) = &filter.git_repo {
        conditions.push("git_repo = ?".to_string());
        binds.push(Box::new(repo.clone()));
    }

    if let Some(branch) = &filter.git_branch {
        conditions.push("git_branch = ?".to_string());
        binds.push(Box::new(branch.clone()));
    }

    if filter.only_success {
        conditions.push("exit_code = 0".to_string());
    }

    if filter.only_failure {
        conditions.push("(exit_code IS NOT NULL AND exit_code != 0)".to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let limit_clause = filter
        .limit
        .map(|n| format!("LIMIT {n}"))
        .unwrap_or_default();

    let sql = format!(
        "SELECT id, command, cwd, project_cwd, git_repo, git_branch, exit_code, duration_ms, started_at
         FROM entries
         {where_clause}
         ORDER BY started_at DESC
         {limit_clause}"
    );

    let mut stmt = conn.prepare(&sql).context("prepare fetch failed")?;

    let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();

    let rows = stmt
        .query_map(params_from_iter(refs), row_to_entry)
        .context("query fetch failed")?;

    rows.map(|r| r.context("row mapping failed"))
        .collect::<Result<Vec<_>>>()
}

/// Fetches all entries whose command contains `term` (case-insensitive substring).
/// This is the pre-filter step before nucleo fuzzy ranking in the TUI.
pub fn search_raw(conn: &Connection, term: &str, limit: usize) -> Result<Vec<Entry>> {
    let pattern = format!("%{term}%");
    let sql = "
        SELECT id, command, cwd, project_cwd, git_repo, git_branch, exit_code, duration_ms, started_at
        FROM entries
        WHERE command LIKE ?1
        ORDER BY started_at DESC
        LIMIT ?2
    ";

    let mut stmt = conn.prepare(sql).context("prepare search failed")?;
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit as i64], row_to_entry)
        .context("query search failed")?;

    rows.map(|r| r.context("row mapping failed"))
        .collect::<Result<Vec<_>>>()
}

/// Returns the N most recently used unique commands in the given directory,
/// useful for the TUI default view when no search term is typed.
pub fn recent(conn: &Connection, limit: usize) -> Result<Vec<Entry>> {
    fetch(conn, &Filter::new().limit(limit))
}
