use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dirs::data_dir;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

use crate::entry::Entry;

/// Returns the path to the rewind data directory, creating it if needed.
pub fn data_path() -> Result<PathBuf> {
    let dir = data_dir()
        .context("could not resolve XDG data directory")?
        .join("rewind");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("could not create data dir: {}", dir.display()))?;
    Ok(dir)
}

/// Opens (or creates) the SQLite database and runs migrations.
pub fn open() -> Result<Connection> {
    let path = data_path()?.join("history.db");
    let conn = Connection::open(&path)
        .with_context(|| format!("could not open db at {}", path.display()))?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .context("could not set WAL mode")?;

    migrate(&conn)?;
    Ok(conn)
}

/// Applies all schema migrations in order.
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS entries (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            command     TEXT    NOT NULL,
            cwd         TEXT    NOT NULL,
            git_repo    TEXT,
            git_branch  TEXT,
            exit_code   INTEGER,
            duration_ms INTEGER,
            started_at  TEXT    NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_entries_cwd        ON entries(cwd);
        CREATE INDEX IF NOT EXISTS idx_entries_git_repo   ON entries(git_repo);
        CREATE INDEX IF NOT EXISTS idx_entries_git_branch ON entries(git_branch);
        CREATE INDEX IF NOT EXISTS idx_entries_started_at ON entries(started_at);
        ",
    )
    .context("migration failed")?;

    Ok(())
}

/// Inserts a new entry and returns it with its assigned id.
pub fn insert(conn: &Connection, entry: &Entry) -> Result<Entry> {
    conn.execute(
        "INSERT INTO entries (command, cwd, git_repo, git_branch, exit_code, duration_ms, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.command,
            entry.cwd,
            entry.git_repo,
            entry.git_branch,
            entry.exit_code,
            entry.duration_ms,
            entry.started_at.to_rfc3339(),
        ],
    )
    .context("insert entry failed")?;

    let id = conn.last_insert_rowid();
    Ok(Entry {
        id,
        ..entry.clone()
    })
}

/// Updates exit_code and duration_ms for an existing entry (wrapper mode).
pub fn complete(conn: &Connection, id: i64, exit_code: i32, duration_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE entries SET exit_code = ?1, duration_ms = ?2 WHERE id = ?3",
        params![exit_code, duration_ms, id],
    )
    .context("complete entry failed")?;
    Ok(())
}

/// Fetches a single entry by id.
pub fn get(conn: &Connection, id: i64) -> Result<Option<Entry>> {
    conn.query_row(
        "SELECT id, command, cwd, git_repo, git_branch, exit_code, duration_ms, started_at
         FROM entries WHERE id = ?1",
        params![id],
        row_to_entry,
    )
    .optional()
    .context("get entry failed")
}

/// Maps a rusqlite Row to an Entry.
pub fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let started_at_str: String = row.get(7)?;
    let started_at = started_at_str
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now());

    Ok(Entry {
        id: row.get(0)?,
        command: row.get(1)?,
        cwd: row.get(2)?,
        git_repo: row.get(3)?,
        git_branch: row.get(4)?,
        exit_code: row.get(5)?,
        duration_ms: row.get(6)?,
        started_at,
    })
}

