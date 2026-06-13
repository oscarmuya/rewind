use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod cmd;
mod tui;

#[derive(Debug, Parser)]
#[command(
    name = "rw",
    bin_name = "rw",
    version,
    about = "rewind — per-project command history",
    long_about = None,
    propagate_version = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn execute(self) -> Result<ExitCode> {
        self.command.execute()
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum Commands {
    /// Run a command and record it with full context.
    Run(cmd::run::Args),

    /// Search history interactively or print matches to stdout.
    Search(cmd::search::Args),

    /// Print recent history for the current directory.
    Recent(cmd::recent::Args),

    /// Print the shell integration snippet for the given shell.
    Init(cmd::init::Args),

    /// Start the background daemon.
    Daemon,
}

impl Commands {
    fn execute(self) -> Result<ExitCode> {
        match self {
            Self::Run(args) => cmd::run::execute(args),

            Self::Search(args) => {
                cmd::search::execute(args)?;
                Ok(ExitCode::SUCCESS)
            }

            Self::Recent(args) => {
                cmd::recent::execute(args)?;
                Ok(ExitCode::SUCCESS)
            }

            Self::Init(args) => {
                cmd::init::execute(args)?;
                Ok(ExitCode::SUCCESS)
            }

            Self::Daemon => {
                eprintln!("Use `rw-daemon` directly or let your shell init manage it.");
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().execute() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
