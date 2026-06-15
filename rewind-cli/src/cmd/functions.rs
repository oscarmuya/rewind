use anyhow::{Context, Result};
use rewind_core::{
    db,
    entry::{Entry, HookPayload},
    functions::{find_project_root, resolve_git},
    socket::socket_path,
};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::ExitCode,
};

pub fn exit_code_to_process_code(code: i32) -> ExitCode {
    match u8::try_from(code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    }
}

pub fn send_to_daemon(command: &str, cwd: &str, exit_code: i32, duration_ms: i64) -> Result<()> {
    let payload = HookPayload {
        command: command.to_owned(),
        cwd: cwd.to_owned(),
        exit_code,
        duration_ms,
    };

    let json = serde_json::to_string(&payload).context("could not serialize hook payload")?;
    let sock_path = socket_path().context("could not resolve daemon socket path")?;

    let mut stream = UnixStream::connect(&sock_path).with_context(|| {
        format!(
            "could not connect to daemon socket: {}",
            sock_path.display()
        )
    })?;

    stream
        .write_all(json.as_bytes())
        .context("could not write payload to daemon")?;

    stream
        .write_all(b"\n")
        .context("could not write payload newline to daemon")?;

    Ok(())
}

pub fn persist_direct(command: &str, cwd: &str, exit_code: i32, duration_ms: i64) -> Result<()> {
    let (git_repo, git_branch) = resolve_git(cwd);

    // We persist the project root
    let project_root = find_project_root(Path::new(&cwd));
    let project_root_str = project_root.to_string_lossy().into_owned();

    let entry = Entry {
        id: 0,
        command: command.to_owned(),
        cwd: project_root_str,
        git_repo,
        git_branch,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        started_at: chrono::Utc::now(),
    };

    let conn = db::open().context("could not open database")?;
    db::insert(&conn, &entry).context("could not insert command history entry")?;

    Ok(())
}

/// Returns the current working directory as a [`PathBuf`].
///
/// Prefers `$PWD` from the environment over [`std::env::current_dir`] so that
/// logical symlink paths are preserved. The shell sets `$PWD` to the logical
/// path (e.g. `/home/oscar/projects/rewind`), while `current_dir` resolves
/// symlinks to the physical path (e.g. `/data/projects/rewind`), which would
/// cause cwd filter mismatches against commands recorded by the shell hook.
pub fn get_cwd() -> PathBuf {
    std::env::var("PWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default())
}
