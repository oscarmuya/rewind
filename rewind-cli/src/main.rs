use anyhow::Result;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::process::ExitCode;

mod cmd;
mod tui;

pub const RESERVED: &[&str] = &[
    "run", "search", "status", "shortcut", "daemon", "init", "help",
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
    history: cmd::history::Args,
}

impl Cli {
    fn execute(self) -> Result<ExitCode> {
        match self.command {
            Some(command) => command.execute(),
            Option::None => cmd::history::execute(self.history),
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
    match Cli::parse_from(normalize_history_selector(std::env::args_os())).execute() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn normalize_history_selector(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut args: Vec<_> = args.into_iter().collect();
    let mut index = 1;

    while let Some(arg) = args.get(index).and_then(|arg| arg.to_str()) {
        if let Some(value) = arg
            .strip_prefix('-')
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .map(str::to_owned)
        {
            args.splice(index..=index, [OsString::from("--index"), value.into()]);
            break;
        }

        match arg {
            "-l" | "--limit" => index += 2,
            "--cwd" | "--repo" | "--branch" | "--ok" | "--fail" | "--deleted" | "--plain" => {
                index += 1
            }
            _ if arg.starts_with("--limit=") => index += 1,
            _ if let Some(consumed) = short_history_option_span(arg) => index += consumed,
            _ => break,
        }
    }

    args
}

fn short_history_option_span(arg: &str) -> Option<usize> {
    if !arg.starts_with('-') || arg.starts_with("--") {
        return None;
    }

    let mut flags = arg[1..].chars().peekable();
    flags.peek()?;
    while let Some(flag) = flags.next() {
        match flag {
            'c' | 'r' | 'b' | 'o' | 'f' | 'd' | 'p' => {}
            'l' => return Some(if flags.peek().is_some() { 1 } else { 2 }),
            _ => return None,
        }
    }

    Some(1)
}

#[cfg(test)]
mod tests {
    use super::{Cli, normalize_history_selector};
    use clap::Parser;
    use std::ffi::OsString;

    #[test]
    fn normalizes_history_selector_with_filters() {
        let args = ["rw", "--repo", "-2", "--plain", "--ok"].map(OsString::from);
        let normalized = normalize_history_selector(args);

        assert_eq!(
            normalized,
            ["rw", "--repo", "--index", "2", "--plain", "--ok"].map(OsString::from)
        );

        let cli = Cli::try_parse_from(normalized).unwrap();
        assert_eq!(cli.history.index, Some(2));
        assert!(cli.history.repo && cli.history.plain && cli.history.ok);
    }

    #[test]
    fn normalizes_history_selector_after_grouped_short_flags() {
        let args = ["rw", "-of", "-3"].map(OsString::from);

        assert_eq!(
            normalize_history_selector(args),
            ["rw", "-of", "--index", "3"].map(OsString::from)
        );
    }

    #[test]
    fn leaves_command_arguments_unchanged() {
        let args = ["rw", "run", "echo", "-2"].map(OsString::from);

        assert_eq!(normalize_history_selector(args.clone()), args);
    }
}
