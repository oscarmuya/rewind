use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::{process::ExitCode, time::Instant};

use crate::cmd::functions::{
    exit_code_to_process_code, parse_cmd_args, persist_direct, run_command, send_to_daemon,
};
use rewind_core::functions::get_cwd;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// The command and its arguments to run.
    #[arg(trailing_var_arg = true, required = true)]
    pub cmd: Vec<String>,
}

pub fn execute(args: Args) -> Result<ExitCode> {
    let cwd = get_cwd().to_string_lossy().into_owned();

    let command_str = parse_cmd_args(&args.cmd);

    // Execute the command first; persistence should not affect command execution.
    let start = Instant::now();
    let exit_code = run_command(&command_str, &cwd)?;
    let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

    // Preferred path: daemon owns DB writes when it is running.
    // Fallback path: write directly when the daemon is unavailable.
    if send_to_daemon(&command_str, &cwd, exit_code, duration_ms).is_err() {
        persist_direct(&command_str, &cwd, exit_code, duration_ms)
            .context("could not persist command history")?;
    }

    Ok(exit_code_to_process_code(exit_code))
}
