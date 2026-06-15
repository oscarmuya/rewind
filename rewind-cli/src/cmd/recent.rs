use anyhow::{Context, Result};
use chrono::{Datelike, Local};
use clap::Args as ClapArgs;
use rewind_core::{
    db,
    entry::Entry,
    functions::{find_project_root, get_cwd, resolve_git},
    query::{self, Filter},
};
use std::{
    io::{self, Write},
    path::Path,
    process::{Command, ExitCode},
    time::Instant,
};

use crate::cmd::functions::{exit_code_to_process_code, persist_direct, send_to_daemon};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Number of entries to show.
    #[arg(short, long, default_value = "500")]
    pub limit: usize,

    /// Filter by git repository (uses current repo if inside one).
    #[arg(long)]
    pub repo: bool,

    /// Filter by current git branch.
    #[arg(long)]
    pub branch: bool,

    /// Only show successful commands (exit code 0).
    #[arg(long)]
    pub ok: bool,

    /// Only show failed commands (non-zero exit).
    #[arg(long)]
    pub fail: bool,

    /// Print matches to stdout instead of opening the TUI.
    #[arg(long)]
    pub plain: bool,
}

pub fn execute(args: self::Args) -> Result<ExitCode> {
    let cwd = get_cwd();
    let cwd_str = cwd.to_string_lossy().into_owned();

    let project_root = find_project_root(Path::new(&cwd));
    let project_root_str = project_root.to_string_lossy().into_owned();

    let (git_repo, git_branch) = resolve_git(&cwd_str);
    let mut filter = Filter::new()
        .limit(args.limit)
        .project_cwd(&project_root_str);

    if args.repo
        && let Some(repo) = git_repo
    {
        filter = filter.git_repo(repo);
    }

    if args.branch
        && let Some(branch) = git_branch
    {
        filter = filter.git_branch(branch);
    }

    if args.ok {
        filter = filter.only_success();
    }

    if args.fail {
        filter = filter.only_failure();
    }

    let conn = db::open()?;
    let entries = query::fetch(&conn, &filter)?;

    if !args.plain {
        if let Some(entry) = crate::tui::run_recent(entries)? {
            return rerun_entry(&entry);
        }

        return Ok(ExitCode::SUCCESS);
    }

    print_entries(&entries)?;

    Ok(ExitCode::SUCCESS)
}

pub fn print_entries(entries: &[Entry]) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut last_heading = String::new();

    for e in entries {
        let heading = date_heading(e);
        if heading != last_heading {
            if !last_heading.is_empty() {
                writeln!(out)?;
            }
            writeln!(out, "{heading}")?;
            last_heading = heading;
        }

        let status = match e.exit_code {
            Some(0) => "✓".to_string(),
            Some(c) => format!("✗{c}"),
            Option::None => "?".to_string(),
        };
        let branch_tag = e
            .git_branch
            .as_deref()
            .map(|b| format!(" [{b}]"))
            .unwrap_or_default();
        let duration = e.duration_ms.map(|d| format!(" {d}ms")).unwrap_or_default();
        let time = e.started_at.with_timezone(&Local).format("%H:%M");

        writeln!(
            out,
            "  {time}  {status}{branch_tag}{duration}  {}",
            e.command
        )?;
    }

    Ok(())
}

fn rerun_entry(entry: &Entry) -> Result<ExitCode> {
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

    let start = Instant::now();
    let status = Command::new(&shell)
        .args(args)
        .arg(&entry.command)
        .current_dir(&entry.cwd)
        .status()
        .with_context(|| format!("could not rerun `{}` with `{shell}`", entry.command))?;

    let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);
    let exit_code = status.code().unwrap_or(1);

    // Preferred path: daemon owns DB writes when it is running.
    // Fallback path: write directly when the daemon is unavailable.
    if send_to_daemon(&entry.command, &entry.cwd, exit_code, duration_ms).is_err() {
        persist_direct(&entry.command, &entry.cwd, exit_code, duration_ms)
            .context("could not persist rerun history")?;
    }

    Ok(exit_code_to_process_code(exit_code))
}

fn date_heading(entry: &Entry) -> String {
    let local = entry.started_at.with_timezone(&Local);
    let date = local.date_naive();
    let today = Local::now().date_naive();

    if date == today {
        "Today".to_string()
    } else if date == today.pred_opt().unwrap_or(today) {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        local.format("%A, %b %-d").to_string()
    } else {
        local.format("%A, %b %-d, %Y").to_string()
    }
}
