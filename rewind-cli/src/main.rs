use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod cmd;
mod tui;

pub const RESERVED: &[&str] = &[
    "run", "search", "status", "shortcut", "recent", "daemon", "init", "help",
];

#[derive(Debug, Parser)]
#[command(
    name = "rw",
    bin_name = "rw",
    version,
    about = "rewind — per-project command history",
    long_about = None,
    propagate_version = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    recent: cmd::recent::Args,
}

impl Cli {
    fn execute(self) -> Result<ExitCode> {
        match self.command {
            Some(command) => command.execute(),
            Option::None => cmd::recent::execute(self.recent),
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum Commands {
    /// Run a command and record it with full context.
    Run(cmd::run::Args),

    /// Search history interactively or print matches to stdout.
    Search(cmd::search::Args),

    /// Show recent history or replay from the TUI.
    Recent(cmd::recent::Args),

    /// Print the shell integration snippet for the given shell.
    Init(cmd::init::Args),

    /// Check shell hook, daemon, and database health.
    Status(cmd::status::Args),

    /// Start the background daemon.
    Daemon,

    /// Manage command shortcuts.
    Shortcut(cmd::shortcut::Args),

    /// Catch-all for shortcut alias invocation.
    #[command(external_subcommand)]
    Alias(Vec<String>),
}

impl Commands {
    fn execute(self) -> Result<ExitCode> {
        match self {
            Self::Run(args) => cmd::run::execute(args),

            Self::Search(args) => cmd::search::execute(args),

            Self::Recent(args) => cmd::recent::execute(args),

            Self::Init(args) => {
                cmd::init::execute(args)?;
                Ok(ExitCode::SUCCESS)
            }

            Self::Status(args) => cmd::status::execute(args),

            Self::Daemon => {
                eprintln!("Use `rw-daemon` directly or let your shell init manage it.");
                Ok(ExitCode::SUCCESS)
            }

            Self::Shortcut(args) => cmd::shortcut::execute(args),
            Self::Alias(args) => match cmd::shortcut::try_invoke_alias(&args)? {
                Some(code) => Ok(code),
                None => {
                    let alias = args.first().map(String::as_str).unwrap_or("");
                    eprintln!(
                        "unknown command or shortcut `{alias}` -- \
                 use `rw shortcut list` to see available shortcuts"
                    );
                    Ok(ExitCode::FAILURE)
                }
            },
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
