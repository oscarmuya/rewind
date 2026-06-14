use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use rewind_core::{
    db,
    entry::{Entry, HookPayload},
    functions::resolve_git,
    socket::socket_path,
};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    process::{Command, ExitCode},
    time::Instant,
};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// The command and its arguments to run.
    #[arg(trailing_var_arg = true, required = true)]
    pub cmd: Vec<String>,
}

pub fn execute(args: Args) -> Result<ExitCode> {
    let cwd = std::env::current_dir()
        .context("could not read cwd")?
        .to_string_lossy()
        .into_owned();

    let command_str = args.cmd.join(" ");

    // Execute the command first; persistence should not affect command execution.
    let start = Instant::now();
    let status = Command::new(&args.cmd[0])
        .args(&args.cmd[1..])
        .status()
        .with_context(|| format!("could not spawn `{}`", args.cmd[0]))?;

    let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);
    let exit_code = status.code().unwrap_or(1);

    // Preferred path: daemon owns DB writes when it is running.
    // Fallback path: write directly when the daemon is unavailable.
    if send_to_daemon(&command_str, &cwd, exit_code, duration_ms).is_err() {
        persist_direct(&command_str, &cwd, exit_code, duration_ms)
            .context("could not persist command history")?;
    }

    Ok(exit_code_to_process_code(exit_code))
}

fn send_to_daemon(command: &str, cwd: &str, exit_code: i32, duration_ms: i64) -> Result<()> {
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

fn persist_direct(command: &str, cwd: &str, exit_code: i32, duration_ms: i64) -> Result<()> {
    let (git_repo, git_branch) = resolve_git(cwd);

    let entry = Entry {
        id: 0,
        command: command.to_owned(),
        cwd: cwd.to_owned(),
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

fn exit_code_to_process_code(code: i32) -> ExitCode {
    match u8::try_from(code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    }
}
