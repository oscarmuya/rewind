use crate::cmd::init::Shell;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use rewind_core::{db, socket::socket_path};
use std::{
    env, fs,
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::ExitCode,
};

const START_MARKER: &str = "# >>> rewind init >>>";
const END_MARKER: &str = "# <<< rewind init <<<";
const HOOK_ACTIVE_ENV: &str = "REWIND_SHELL_HOOK";
const HOOK_SHELL_ENV: &str = "REWIND_SHELL_HOOK_SHELL";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Shell to check. If omitted, rw detects it from $SHELL.
    #[arg(value_enum)]
    pub shell: Option<Shell>,
}

pub fn execute(args: Args) -> Result<ExitCode> {
    let shell_result = match args.shell {
        Some(shell) => Ok(shell),
        Option::None => Shell::detect(),
    };

    let mut rows = Vec::new();
    let mut healthy = true;

    match shell_result {
        Ok(shell) => {
            let hook = check_hook(shell);
            healthy &= hook.healthy;
            rows.extend(hook.rows);
        }
        Err(error) => {
            healthy = false;
            rows.push(Row::fail("shell", error.to_string()));
        }
    }

    let deps = check_hook_dependencies();
    healthy &= deps.healthy;
    rows.extend(deps.rows);

    let daemon = check_daemon();
    healthy &= daemon.healthy;
    rows.extend(daemon.rows);

    let database = check_database();
    healthy &= database.healthy;
    rows.extend(database.rows);

    println!("rewind status");
    for row in rows {
        println!("{} {:<18} {}", row.icon(), row.name, row.detail);
    }

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn check_hook(shell: Shell) -> Check {
    let mut check = Check::default();

    match shell.config_path() {
        Ok(path) => match fs::read_to_string(&path) {
            Ok(contents) if has_managed_block(&contents) => {
                check.pass(
                    "shell hook",
                    format!("{shell} integration installed in {}", path.display()),
                );
            }
            Ok(_) => {
                check.fail(
                    "shell hook",
                    format!(
                        "{shell} integration not found in {}; run `rw init {shell} --install`",
                        path.display()
                    ),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                check.fail(
                    "shell hook",
                    format!(
                        "{} does not exist; run `rw init {shell} --install`",
                        path.display()
                    ),
                );
            }
            Err(error) => {
                check.fail(
                    "shell hook",
                    format!("could not read {}: {error}", path.display()),
                );
            }
        },
        Err(error) => check.fail("shell hook", error.to_string()),
    }

    match env::var(HOOK_ACTIVE_ENV) {
        Ok(value) if value == "1" => {
            let active_shell = env::var(HOOK_SHELL_ENV).unwrap_or_else(|_| "unknown".to_owned());
            if active_shell == shell.to_string() {
                check.pass(
                    "hook runtime",
                    format!("active in current {active_shell} session"),
                );
            } else {
                check.warn(
                    "hook runtime",
                    format!("active for {active_shell}; checking configured {shell}"),
                );
            }
        }
        _ => {
            check.warn(
                "hook runtime",
                "not visible in this process; restart or source your shell config after installing",
            );
        }
    }

    check
}

fn check_hook_dependencies() -> Check {
    let mut check = Check::default();

    for command in ["python3", "socat"] {
        match find_in_path(command) {
            Some(path) => check.pass(command, path.display().to_string()),
            Option::None => check.fail(
                command,
                format!("not found on PATH; shell recording requires `{command}`"),
            ),
        }
    }

    check
}

fn check_daemon() -> Check {
    let mut check = Check::default();

    match socket_path() {
        Ok(path) => match fs::metadata(&path) {
            Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(&path) {
                Ok(_) => check.pass(
                    "daemon",
                    format!("accepting connections at {}", path.display()),
                ),
                Err(error) => check.fail(
                    "daemon",
                    format!(
                        "socket exists but connection failed at {}: {error}",
                        path.display()
                    ),
                ),
            },
            Ok(_) => check.fail(
                "daemon",
                format!("{} exists but is not a socket", path.display()),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                check.fail(
                    "daemon",
                    format!(
                        "socket not found at {}; start a new shell or run `rw-daemon`",
                        path.display()
                    ),
                );
            }
            Err(error) => check.fail(
                "daemon",
                format!("could not inspect {}: {error}", path.display()),
            ),
        },
        Err(error) => check.fail("daemon", error.to_string()),
    }

    check
}

fn check_database() -> Check {
    let mut check = Check::default();
    let path = match db::data_path() {
        Ok(path) => path.join("history.db"),
        Err(error) => {
            check.fail("database", error.to_string());
            return check;
        }
    };

    match db::open() {
        Ok(conn) => {
            let count = conn
                .query_row("SELECT COUNT(*) FROM entries", [], |row| {
                    row.get::<_, i64>(0)
                })
                .context("could not query entries count");
            let integrity = conn
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .context("could not run integrity_check");

            match (count, integrity) {
                (Ok(count), Ok(result)) if result == "ok" => {
                    check.pass(
                        "database",
                        format!("{} is functional ({count} entries)", path.display()),
                    );
                }
                (Ok(_), Ok(result)) => {
                    check.fail(
                        "database",
                        format!("{} integrity_check returned {result}", path.display()),
                    );
                }
                (Err(error), _) | (_, Err(error)) => {
                    check.fail(
                        "database",
                        format!("{} opened but check failed: {error:#}", path.display()),
                    );
                }
            }
        }
        Err(error) => check.fail(
            "database",
            format!("could not open {}: {error:#}", path.display()),
        ),
    }

    check
}

fn has_managed_block(contents: &str) -> bool {
    let mut inside_block = false;

    for line in contents.lines() {
        match line.trim() {
            START_MARKER => inside_block = true,
            END_MARKER if inside_block => return true,
            _ => {}
        }
    }

    false
}

fn find_in_path(command: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;

    env::split_paths(&paths)
        .map(|dir| dir.join(command))
        .find(|path| is_executable_file(path))
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

struct Check {
    healthy: bool,
    rows: Vec<Row>,
}

impl Default for Check {
    fn default() -> Self {
        Self {
            healthy: true,
            rows: Vec::new(),
        }
    }
}

impl Check {
    fn pass(&mut self, name: &'static str, detail: String) {
        self.rows.push(Row {
            status: Status::Pass,
            name,
            detail,
        });
    }

    fn warn(&mut self, name: &'static str, detail: impl Into<String>) {
        self.rows.push(Row {
            status: Status::Warn,
            name,
            detail: detail.into(),
        });
    }

    fn fail(&mut self, name: &'static str, detail: String) {
        self.healthy = false;
        self.rows.push(Row {
            status: Status::Fail,
            name,
            detail,
        });
    }
}

struct Row {
    status: Status,
    name: &'static str,
    detail: String,
}

impl Row {
    fn fail(name: &'static str, detail: String) -> Self {
        Self {
            status: Status::Fail,
            name,
            detail,
        }
    }

    fn icon(&self) -> &'static str {
        match self.status {
            Status::Pass => "[ok]",
            Status::Warn => "[warn]",
            Status::Fail => "[fail]",
        }
    }
}

enum Status {
    Pass,
    Warn,
    Fail,
}
