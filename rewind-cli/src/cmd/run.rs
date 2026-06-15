use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::{
    process::{Command, ExitCode},
    time::Instant,
};

use crate::cmd::functions::{exit_code_to_process_code, get_cwd, persist_direct, send_to_daemon};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// The command and its arguments to run.
    #[arg(trailing_var_arg = true, required = true)]
    pub cmd: Vec<String>,
}

pub fn execute(args: Args) -> Result<ExitCode> {
    let cwd = get_cwd().to_string_lossy().into_owned();

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
