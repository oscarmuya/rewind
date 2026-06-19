use anyhow::{Context, Result};
use core::convert::Into;
use rewind_core::{
    db,
    entry::{Entry, HookPayload},
    functions::{find_project_root, resolve_git},
    socket::socket_path,
};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::Path,
    process::{Command, ExitCode},
    time::Instant,
};

pub fn exit_code_to_process_code(code: i32) -> ExitCode {
    match u8::try_from(code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    }
}

/// Wrap in single quotes, escaping any single quotes within
pub fn parse_cmd_args(cmd: &[String]) -> String {
    cmd.iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
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
        cwd: cwd.into(),
        project_cwd: project_root_str,
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

pub fn rerun_entry(entry: &Entry) -> Result<ExitCode> {
    let start = Instant::now();
    let exit_code = run_command(&entry.command, &entry.cwd)?;
    let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

    // Preferred path: daemon owns DB writes when it is running.
    // Fallback path: write directly when the daemon is unavailable.
    if send_to_daemon(&entry.command, &entry.cwd, exit_code, duration_ms).is_err() {
        persist_direct(&entry.command, &entry.cwd, exit_code, duration_ms)
            .context("could not persist rerun history")?;
    }

    Ok(exit_code_to_process_code(exit_code))
}

pub fn run_command(command: &str, cwd: &str) -> Result<i32> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let shell_name = std::env::var("REWIND_SHELL_HOOK_SHELL").unwrap_or_else(|_| {
        Path::new(&shell)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("sh")
            .to_string()
    });

    // We Use -i (interactive) to source ~/.zshrc / ~/.bashrc so aliases work.
    // zsh also needs -s to suppress the prompt being printed.
    let args: &[&str] = match shell_name.as_str() {
        "zsh" => &["-isc"],
        "bash" => &["-ic"],
        "fish" => &["-c"],
        _ => &["-ic"],
    };

    let status = Command::new(&shell)
        .args(args)
        .arg(command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("could not run `{}` with `{shell}`", command))?;

    let exit_code = status.code().unwrap_or(1);

    Ok(exit_code)
}
