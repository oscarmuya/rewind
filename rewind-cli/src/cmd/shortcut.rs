use std::{process::ExitCode, time::Instant};

use anyhow::{Context, Result, bail};
use clap::{Args as ClapArgs, Subcommand};

use rewind_core::{
    db,
    functions::{find_project_root, get_cwd},
    query_shortcuts,
    shortcut::Shortcut,
};

use crate::{
    RESERVED,
    cmd::functions::{
        exit_code_to_process_code, parse_cmd_args, persist_direct, run_command, send_to_daemon,
    },
};

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: ShortcutCommand,
}

#[derive(Subcommand, Debug)]
pub enum ShortcutCommand {
    /// Save a new shortcut scoped to the current project (or globally with --global).
    Add {
        /// The alias to invoke the shortcut with.
        alias: String,
        /// The command the alias expands to.
        #[arg(num_args(1..), trailing_var_arg = true)]
        command: Vec<String>,
        /// Save this shortcut globally instead of scoping it to the current project.
        #[arg(long)]
        global: bool,
    },
    /// Remove a shortcut by alias from the current project (or globally with --global).
    Remove {
        /// The alias to remove.
        alias: String,
        /// Remove from global shortcuts instead of the current project.
        #[arg(long)]
        global: bool,
    },
    /// List shortcuts for the current project, including globals.
    List {
        /// List only global shortcuts.
        #[arg(long)]
        global: bool,
    },
}

pub fn execute(args: self::Args) -> Result<ExitCode> {
    let conn = db::open()?;
    let cwd = get_cwd();
    let project_root = find_project_root(&cwd);
    let project_root_str = project_root.to_string_lossy().into_owned();

    match args.command {
        ShortcutCommand::Add {
            alias,
            command,
            global,
        } => {
            if RESERVED.contains(&alias.as_str()) {
                bail!(
                    "'{alias}' is a reserved subcommand name and cannot be used as a shortcut alias"
                );
            }

            let project_dir = if global {
                String::from("__global__")
            } else {
                project_root_str.clone()
            };

            if command.is_empty() || command.iter().all(|arg| arg.trim().is_empty()) {
                bail!("A command is required. Usage: rw shortcut add <alias> <command>");
            }
            let command = parse_cmd_args(&command);

            let shortcut = Shortcut::new(&alias, &command, &project_dir, None, global);

            db::insert_shortcut(&conn, &shortcut)?;

            let scope = if global {
                "globally"
            } else {
                "for this project"
            };
            println!("Saved shortcut '{alias}' {scope}.");
        }

        ShortcutCommand::Remove { alias, global } => {
            let project_dir = if global {
                String::from("__global__")
            } else {
                project_root_str.clone()
            };

            let existing = query_shortcuts::resolve(&conn, &alias, &project_dir, global)?;

            match existing {
                Some(s) => {
                    query_shortcuts::delete(&conn, s.id)?;
                    println!("Removed shortcut '{alias}'.");
                }
                None => {
                    bail!(
                        "No shortcut found for alias '{alias}'. \
                 use `--global` if it is a global shortcut"
                    );
                }
            }
        }

        ShortcutCommand::List { global } => {
            let shortcuts = if global {
                query_shortcuts::globals(&conn)?
            } else {
                query_shortcuts::for_project(&conn, &project_root_str)?
            };

            if shortcuts.is_empty() {
                println!("No shortcuts found.");
            } else {
                let alias_width = shortcuts.iter().map(|s| s.alias.len()).max().unwrap_or(5);
                let command_width = shortcuts.iter().map(|s| s.command.len()).max().unwrap_or(7);

                for s in &shortcuts {
                    let scope = if s.global() { "[global]" } else { "[project]" };
                    println!(
                        "{:<6}  {:<alias_w$}  {:<cmd_w$}",
                        scope,
                        s.alias,
                        s.command,
                        alias_w = alias_width,
                        cmd_w = command_width,
                    );
                }
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Attempts to resolve `alias` as a shortcut in the current project or globally,
/// and reruns its command. Returns Ok(None) if no shortcut matched.
pub fn try_invoke_alias(args: &[String]) -> Result<Option<ExitCode>> {
    let alias = match args.first() {
        Some(a) => a.as_str(),
        None => return Ok(None),
    };

    // Extra arguments are appended to the saved command verbatim, e.g.
    // `rw test entry::tests` with shortcut `test="cargo test --workspace --locked"`
    // expands to `cargo test --workspace --locked entry::tests`.
    // TODO: In future we may support named placeholders or template substitution.
    let extra_args = &args[1..];

    let conn = db::open()?;
    let cwd = get_cwd();
    let project_root = find_project_root(&cwd);
    let project_root_str = project_root.to_string_lossy().into_owned();

    match query_shortcuts::resolve(&conn, alias, &project_root_str, true)? {
        Some(shortcut) => {
            let cwd_str = cwd.to_string_lossy().into_owned();

            // Build the full command by appending any extra args to the saved command.
            let command = if extra_args.is_empty() {
                shortcut.command.clone()
            } else {
                format!("{} {}", shortcut.command, parse_cmd_args(extra_args))
            };

            // Execute the command first; persistence should not affect command execution.
            let start = Instant::now();
            let exit_code = run_command(&command, &cwd_str)?;
            let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

            // Preferred path: daemon owns DB writes when it is running.
            // Fallback path: write directly when the daemon is unavailable.
            if send_to_daemon(&command, &cwd_str, exit_code, duration_ms).is_err() {
                persist_direct(&command, &cwd_str, exit_code, duration_ms)
                    .context("could not persist command history")?;
            }

            Ok(Some(exit_code_to_process_code(exit_code)))
        }
        None => Ok(None),
    }
}
