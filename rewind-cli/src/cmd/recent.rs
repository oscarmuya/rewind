use anyhow::{Result, bail};
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
    process::ExitCode,
};

use crate::cmd::functions::rerun_entry;
use crate::tui::FilterContext;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Number of entries to show.
    #[arg(short, long, default_value = "500")]
    pub limit: usize,

    /// Filter by current working directory
    #[arg(short, long)]
    pub cwd: bool,

    /// Filter by git repository (uses current repo if inside one).
    #[arg(short, long)]
    pub repo: bool,

    /// Filter by current git branch.
    #[arg(short, long)]
    pub branch: bool,

    /// Only show successful commands (exit code 0).
    #[arg(short, long)]
    pub ok: bool,

    /// Only show failed commands (non-zero exit).
    #[arg(short, long)]
    pub fail: bool,

    /// Only show soft-deleted commands.
    #[arg(short, long)]
    pub deleted: bool,

    /// Print matches to stdout instead of opening the TUI.
    #[arg(short, long)]
    pub plain: bool,

    /// Replay the Nth most recent matching command.
    #[arg(long, hide = true)]
    pub index: Option<usize>,
}

pub fn execute(args: self::Args) -> Result<ExitCode> {
    let cwd = get_cwd();
    let cwd_str = cwd.to_string_lossy().into_owned();

    let project_root = find_project_root(Path::new(&cwd));
    let project_root_str = project_root.to_string_lossy().into_owned();

    let (git_repo, git_branch) = resolve_git(&cwd_str);
    let context = FilterContext::new(&cwd_str, git_repo, git_branch);

    let mut filter = Filter::new()
        .limit(args.index.unwrap_or(args.limit))
        .project_cwd(&project_root_str);

    if args.cwd {
        filter = filter.cwd(&cwd_str);
    }

    if args.repo
        && let Some(repo) = &context.git_repo
    {
        filter = filter.git_repo(repo);
    }

    if args.branch
        && let Some(branch) = &context.git_branch
    {
        filter = filter.git_branch(branch);
    }

    if args.ok {
        filter = filter.only_success();
    }

    if args.fail {
        filter = filter.only_failure();
    }

    if args.deleted {
        filter = filter.only_deleted();
    }

    let conn = db::open()?;

    if let Some(index) = args.index {
        if index == 0 {
            bail!("recent command index must be 1 or greater");
        }

        let entries = query::fetch(&conn, &filter)?;
        let Some(entry) = entries.get(index - 1) else {
            bail!("recent command -{index} was not found");
        };

        if args.plain {
            println!("{}", entry.command);
            io::stdout().flush()?;
            return Ok(ExitCode::SUCCESS);
        }

        eprintln!("{}", entry.command);
        return rerun_entry(entry);
    }

    if !args.plain {
        if let Some(entry) = crate::tui::run_recent(&conn, context, filter, None)? {
            return rerun_entry(&entry);
        }

        return Ok(ExitCode::SUCCESS);
    }

    let entries = query::fetch(&conn, &filter)?;
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
