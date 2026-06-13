use anyhow::{Context, Result};
use rewind_core::{
    db,
    entry::{Entry, HookPayload},
    functions::resolve_git,
    socket::socket_path,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{UnixListener, UnixStream},
};

#[tokio::main]
async fn main() -> Result<()> {
    let sock = socket_path()?;

    // Remove stale socket file from a previous run.
    if sock.exists() {
        std::fs::remove_file(&sock)
            .with_context(|| format!("could not remove stale socket: {}", sock.display()))?;
    }

    let listener = UnixListener::bind(&sock)
        .with_context(|| format!("could not bind socket: {}", sock.display()))?;

    eprintln!("[rewind-daemon] listening on {}", sock.display());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move {
                    if let Err(e) = handle(stream).await {
                        eprintln!("[rewind-daemon] handler error: {e:#}");
                    }
                });
            }
            Err(e) => eprintln!("[rewind-daemon] accept error: {e}"),
        }
    }
}

/// Handles a single client connection.
/// Each connection sends exactly one newline-delimited JSON HookPayload.
async fn handle(stream: UnixStream) -> Result<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let payload: HookPayload =
            serde_json::from_str(&line).context("could not deserialize payload")?;

        // Git context resolution is cheap and synchronous -- run it inline.
        let (git_repo, git_branch) = resolve_git(&payload.cwd);

        let entry = Entry {
            id: 0,
            command: payload.command,
            cwd: payload.cwd,
            git_repo,
            git_branch,
            exit_code: Some(payload.exit_code),
            duration_ms: Some(payload.duration_ms),
            started_at: chrono::Utc::now(),
        };

        // DB open per-write is fine here; WAL mode keeps it fast.
        // For high-throughput use we could keep a connection in state,
        // but shell hooks fire at human speed so this is sufficient.
        let conn = db::open()?;
        db::insert(&conn, &entry)?;
    }

    Ok(())
}
