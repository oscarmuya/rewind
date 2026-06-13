use anyhow::Result;
use clap::Args as ClapArgs;
use rewind_core::{
    db,
    functions::resolve_git,
    query::{self, Filter},
};
use std::io::{self, Write};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Number of entries to show.
    #[arg(short, long, default_value = "50")]
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
}

pub fn execute(args: self::Args) -> Result<()> {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let (git_repo, git_branch) = resolve_git(&cwd);

    let mut filter = Filter::new().limit(args.limit);

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

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for e in &entries {
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

        writeln!(out, "{status}{branch_tag}{duration}  {}", e.command)?;
    }

    Ok(())
}
