use crate::entry::Entry;
use crate::shortcut::Shortcut;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dirs::data_dir;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

/// Returns the path to the rewind data directory, creating it if needed.
pub fn data_path() -> Result<PathBuf> {
    let dir = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".dev-data")
    } else {
        data_dir()
            .context("could not resolve XDG data directory")?
            .join("rewind")
    };

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

/// The current schema version.
const SCHEMA_VERSION: i64 = 3;

/// Applies all schema migrations in order, using schema_version to track
/// which migrations have already been applied. Each migration is additive
/// and never destructive so existing data is always preserved.
fn migrate(conn: &Connection) -> Result<()> {
    // Bootstrap the version table and read the current version in one batch.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
         INSERT INTO schema_version (version) SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM schema_version);",
    )
    .context("could not bootstrap schema_version")?;

    let version: i64 = conn
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .context("could not read schema version")?;

    // Migration 1: initial schema.
    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
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
            CREATE INDEX IF NOT EXISTS idx_entries_started_at ON entries(started_at);",
        )
        .context("migration 1 failed")?;
    }

    // Migration 2: add project_cwd column to scope commands to the git root.
    if version < 2 {
        conn.execute_batch(
            "ALTER TABLE entries ADD COLUMN project_cwd TEXT;
             CREATE INDEX IF NOT EXISTS idx_entries_project_cwd ON entries(project_cwd);
             UPDATE entries SET project_cwd = cwd WHERE project_cwd IS NULL;",
        )
        .context("migration 2 failed")?;
    }

    // Migration 3: add shortcuts table for saved command aliases.
    if version < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS shortcuts (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                alias       TEXT    NOT NULL,
                command     TEXT    NOT NULL,
                project_dir TEXT    NOT NULL,
                git_repo    TEXT,
                is_global   BOOLEAN DEFAULT FALSE,
                created_at  DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                UNIQUE(alias, project_dir)
            );
            CREATE INDEX IF NOT EXISTS idx_shortcuts_alias       ON shortcuts(alias);
            CREATE INDEX IF NOT EXISTS idx_shortcuts_project_dir ON shortcuts(project_dir);
            CREATE INDEX IF NOT EXISTS idx_shortcuts_git_repo    ON shortcuts(git_repo);
            CREATE INDEX IF NOT EXISTS idx_shortcuts_is_global   ON shortcuts(is_global);",
        )
        .context("migration 3 failed")?;
    }

    // Stamp the version after all migrations succeed.
    if version < SCHEMA_VERSION {
        conn.execute(
            "UPDATE schema_version SET version = ?1",
            params![SCHEMA_VERSION],
        )
        .context("could not update schema version")?;
    }

    Ok(())
}

/// Inserts a new entry and returns it with its assigned id.
pub fn insert(conn: &Connection, entry: &Entry) -> Result<Entry> {
    conn.execute(
        "INSERT INTO entries (command, cwd, project_cwd, git_repo, git_branch, exit_code, duration_ms, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            entry.command,
            entry.cwd,
            entry.project_cwd,
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
        "SELECT id, command, cwd, project_cwd, git_repo, git_branch, exit_code, duration_ms, started_at
         FROM entries WHERE id = ?1",
        params![id],
        row_to_entry,
    )
    .optional()
    .context("get entry failed")
}

/// Maps a rusqlite Row to an Entry.
pub fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let started_at_str: String = row.get(8)?;
    let started_at = started_at_str
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now());

    Ok(Entry {
        id: row.get(0)?,
        command: row.get(1)?,
        cwd: row.get(2)?,
        project_cwd: row.get(3)?,
        git_repo: row.get(4)?,
        git_branch: row.get(5)?,
        exit_code: row.get(6)?,
        duration_ms: row.get(7)?,
        started_at,
    })
}

/// Inserts a new shortcut and returns it with its assigned id.
pub fn insert_shortcut(conn: &Connection, shortcut: &Shortcut) -> Result<Shortcut> {
    conn.execute(
        "INSERT INTO shortcuts (alias, command, project_dir, git_repo, is_global, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            shortcut.alias,
            shortcut.command,
            shortcut.project_dir,
            shortcut.git_repo,
            shortcut.is_global,
            shortcut.created_at.to_rfc3339(),
        ],
    )
    .context("insert shortcut failed")?;

    let id = conn.last_insert_rowid();
    Ok(Shortcut {
        id,
        ..shortcut.clone()
    })
}

/// Fetches a single shortcut by id.
pub fn get_shortcut(conn: &Connection, id: i64) -> Result<Option<Shortcut>> {
    conn.query_row(
        "SELECT id, alias, command, project_dir, git_repo, is_global, created_at
         FROM shortcuts WHERE id = ?1",
        params![id],
        row_to_shortcut,
    )
    .optional()
    .context("get shortcut failed")
}

/// Maps a rusqlite Row to a Shortcut.
pub fn row_to_shortcut(row: &rusqlite::Row) -> rusqlite::Result<Shortcut> {
    let created_at_str: String = row.get(6)?;
    let created_at = created_at_str
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now());

    Ok(Shortcut {
        id: row.get(0)?,
        alias: row.get(1)?,
        command: row.get(2)?,
        project_dir: row.get(3)?,
        git_repo: row.get(4)?,
        is_global: row.get(5)?,
        created_at,
    })
}
