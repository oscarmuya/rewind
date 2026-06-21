use anyhow::{Context, Result};
use rusqlite::{Connection, params_from_iter};

use crate::db::row_to_shortcut;
use crate::shortcut::Shortcut;

/// Builder for filtering shortcut queries.
#[derive(Debug, Default)]
pub struct ShortcutFilter {
    pub project_dir: Option<String>,
    pub git_repo: Option<String>,
    pub alias: Option<String>,
    /// If true, only return global shortcuts (is_global = true).
    pub only_global: bool,
    /// If true, include global shortcuts alongside project-scoped ones.
    pub include_global: bool,
    pub limit: Option<usize>,
}

impl ShortcutFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn project_dir(mut self, project_dir: impl Into<String>) -> Self {
        self.project_dir = Some(project_dir.into());
        self
    }

    pub fn git_repo(mut self, repo: impl Into<String>) -> Self {
        self.git_repo = Some(repo.into());
        self
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    pub fn only_global(mut self) -> Self {
        self.only_global = true;
        self
    }

    pub fn include_global(mut self) -> Self {
        self.include_global = true;
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

/// Runs a structured query with the given filter, returning matching shortcuts
/// ordered alphabetically by alias.
pub fn fetch(conn: &Connection, filter: &ShortcutFilter) -> Result<Vec<Shortcut>> {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if filter.only_global {
        conditions.push("is_global = 1".to_string());
    } else if let Some(project_dir) = &filter.project_dir {
        if filter.include_global {
            conditions.push("(project_dir = ? OR is_global = 1)".to_string());
        } else {
            conditions.push("project_dir = ?".to_string());
        }
        binds.push(Box::new(project_dir.clone()));
    }

    if let Some(repo) = &filter.git_repo {
        conditions.push("git_repo = ?".to_string());
        binds.push(Box::new(repo.clone()));
    }

    if let Some(alias) = &filter.alias {
        conditions.push("alias = ?".to_string());
        binds.push(Box::new(alias.clone()));
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
        "SELECT id, alias, command, project_dir, git_repo, is_global, created_at
         FROM shortcuts
         {where_clause}
         ORDER BY alias ASC
         {limit_clause}"
    );

    let mut stmt = conn
        .prepare(&sql)
        .context("prepare shortcut fetch failed")?;

    let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();

    let rows = stmt
        .query_map(params_from_iter(refs), row_to_shortcut)
        .context("query shortcut fetch failed")?;

    rows.map(|r| r.context("row mapping failed"))
        .collect::<Result<Vec<_>>>()
}

/// Fetches all shortcuts whose alias or command contains `term` (case-insensitive substring),
/// scoped to the given project plus globals.
pub fn search_raw(
    conn: &Connection,
    term: &str,
    project_dir: &str,
    limit: usize,
) -> Result<Vec<Shortcut>> {
    let pattern = format!("%{term}%");
    let sql = "
        SELECT id, alias, command, project_dir, git_repo, is_global, created_at
        FROM shortcuts
        WHERE (project_dir = ?1 OR is_global = 1)
          AND (alias LIKE ?2 OR command LIKE ?2)
        ORDER BY alias ASC
        LIMIT ?3
    ";

    let mut stmt = conn
        .prepare(sql)
        .context("prepare shortcut search failed")?;
    let rows = stmt
        .query_map(
            rusqlite::params![project_dir, pattern, limit as i64],
            row_to_shortcut,
        )
        .context("query shortcut search failed")?;

    rows.map(|r| r.context("row mapping failed"))
        .collect::<Result<Vec<_>>>()
}

/// Resolves a single shortcut by alias, preferring a project-scoped match over a global one.
pub fn resolve(
    conn: &Connection,
    alias: &str,
    project_dir: &str,
    include_globals: bool,
) -> Result<Option<Shortcut>> {
    let sql = "
        SELECT id, alias, command, project_dir, git_repo, is_global, created_at
        FROM shortcuts
        WHERE alias = ?1
          AND (
              project_dir = ?2
              OR (?3 AND is_global = 1)
          )
        ORDER BY
          CASE
              WHEN project_dir = ?2 THEN 0
              WHEN is_global = 1 THEN 1
              ELSE 2
          END
        LIMIT 1
    ";

    let mut stmt = conn
        .prepare(sql)
        .context("prepare shortcut resolve failed")?;

    let mut rows = stmt
        .query_map(
            rusqlite::params![alias, project_dir, include_globals],
            row_to_shortcut,
        )
        .context("query shortcut resolve failed")?;

    rows.next().transpose().context("row mapping failed")
}

/// Deletes a shortcut by id. Returns true if a row was actually deleted.
pub fn delete(conn: &Connection, id: i64) -> Result<bool> {
    let affected = conn
        .execute("DELETE FROM shortcuts WHERE id = ?1", rusqlite::params![id])
        .context("delete shortcut failed")?;
    Ok(affected > 0)
}

/// Updates the command for a shortcut by id. Returns true if a row was updated.
pub fn update_command(conn: &Connection, id: i64, command: &str) -> Result<bool> {
    let affected = conn
        .execute(
            "UPDATE shortcuts SET command = ?1 WHERE id = ?2",
            rusqlite::params![command, id],
        )
        .context("update shortcut failed")?;
    Ok(affected > 0)
}

/// Returns all shortcuts for the given project, including globals.
pub fn for_project(conn: &Connection, project_dir: &str) -> Result<Vec<Shortcut>> {
    fetch(
        conn,
        &ShortcutFilter::new()
            .project_dir(project_dir)
            .include_global(),
    )
}

/// Returns all global shortcuts.
pub fn globals(conn: &Connection) -> Result<Vec<Shortcut>> {
    fetch(conn, &ShortcutFilter::new().only_global())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, shortcut::Shortcut};

    #[test]
    fn updates_shortcut_command() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE shortcuts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                alias TEXT NOT NULL,
                command TEXT NOT NULL,
                project_dir TEXT NOT NULL,
                git_repo TEXT,
                is_global BOOLEAN DEFAULT FALSE,
                created_at DATETIME NOT NULL,
                UNIQUE(alias, project_dir)
            );",
        )
        .unwrap();
        let shortcut = db::insert_shortcut(
            &conn,
            &Shortcut::new("test", "cargo test", "/project", None, false),
        )
        .unwrap();

        assert!(update_command(&conn, shortcut.id, "cargo test --workspace").unwrap());
        assert_eq!(
            db::get_shortcut(&conn, shortcut.id)
                .unwrap()
                .unwrap()
                .command,
            "cargo test --workspace"
        );
    }
}
