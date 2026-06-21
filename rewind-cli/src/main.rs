use anyhow::Result;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
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
    match Cli::parse_from(normalize_recent_selector(std::env::args_os())).execute() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn normalize_recent_selector(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut args: Vec<_> = args.into_iter().collect();
    let mut index = 1;
    let mut explicit_recent = false;

    while let Some(arg) = args.get(index).and_then(|arg| arg.to_str()) {
        if arg == "recent" && !explicit_recent {
            explicit_recent = true;
            index += 1;
            continue;
        }

        if let Some(value) = arg
            .strip_prefix('-')
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .map(str::to_owned)
        {
            let mut replacement = vec![OsString::from("recent")];
            replacement.extend(
                args[1..index]
                    .iter()
                    .filter(|arg| arg.as_os_str() != "recent")
                    .cloned(),
            );
            replacement.extend([OsString::from("--index"), value.into()]);
            args.splice(1..=index, replacement);
            break;
        }

        match arg {
            "-l" | "--limit" => index += 2,
            "--cwd" | "--repo" | "--branch" | "--ok" | "--fail" | "--deleted" | "--plain" => {
                index += 1
            }
            _ if arg.starts_with("--limit=") => index += 1,
            _ if let Some(consumed) = short_recent_option_span(arg) => index += consumed,
            _ => break,
        }
    }

    args
}

fn short_recent_option_span(arg: &str) -> Option<usize> {
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
    use super::{Cli, Commands, normalize_recent_selector};
    use clap::Parser;
    use std::ffi::OsString;

    #[test]
    fn normalizes_recent_selector_with_filters() {
        let args = ["rw", "--repo", "-2", "--plain", "--ok"].map(OsString::from);
        let normalized = normalize_recent_selector(args);

        assert_eq!(
            normalized,
            ["rw", "recent", "--repo", "--index", "2", "--plain", "--ok"].map(OsString::from)
        );

        let cli = Cli::try_parse_from(normalized).unwrap();
        let Some(Commands::Recent(args)) = cli.command else {
            panic!("expected recent command");
        };
        assert_eq!(args.index, Some(2));
        assert!(args.repo && args.plain && args.ok);
    }

    #[test]
    fn normalizes_selector_for_recent_subcommand() {
        let args = ["rw", "recent", "--limit", "20", "-2"].map(OsString::from);

        assert_eq!(
            normalize_recent_selector(args),
            ["rw", "recent", "--limit", "20", "--index", "2"].map(OsString::from)
        );
    }

    #[test]
    fn normalizes_recent_selector_after_grouped_short_flags() {
        let args = ["rw", "recent", "-of", "-3"].map(OsString::from);

        assert_eq!(
            normalize_recent_selector(args),
            ["rw", "recent", "-of", "--index", "3"].map(OsString::from)
        );
    }

    #[test]
    fn leaves_command_arguments_unchanged() {
        let args = ["rw", "run", "echo", "-2"].map(OsString::from);

        assert_eq!(normalize_recent_selector(args.clone()), args);
    }
}
