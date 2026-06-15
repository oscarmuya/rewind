use std::{path::Path, process::ExitCode};

use anyhow::Result;
use clap::Args as ClapArgs;
use rewind_core::{
    db,
    functions::{find_project_root, get_cwd},
    fuzzy, query,
};

use crate::cmd::functions::rerun_entry;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Search term. Omit to open the interactive TUI.
    pub term: Option<String>,

    /// Print matches to stdout instead of opening the TUI.
    #[arg(long)]
    pub plain: bool,

    /// Maximum results (plain mode only).
    #[arg(short, long, default_value = "50")]
    pub limit: usize,
}

pub fn execute(args: self::Args) -> Result<ExitCode> {
    let conn = db::open()?;

    let cwd = get_cwd();
    let project_root = find_project_root(Path::new(&cwd));
    let project_root_str = project_root.to_string_lossy().into_owned();

    match (args.term, args.plain) {
        // Plain text search to stdout.
        (Some(term), true) => {
            // We fetch all the recent and perform fuzzy search on the results
            let entries = query::recent(&conn, &project_root_str, args.limit * 10)?;
            let filtered = fuzzy::search_fuzzy(&entries, &term, args.limit);
            for e in &filtered {
                println!("{}", e.command);
            }
        }

        // Interactive TUI -- term is the pre-populated query or empty.
        (term, false) => {
            let initial = term.unwrap_or_default();

            if let Some(entry) = crate::tui::run(&conn, &project_root_str, &initial)? {
                return rerun_entry(&entry);
            }
        }

        // --plain with no term: just dump recent history.
        (Option::None, true) => {
            let entries = query::recent(&conn, &project_root_str, args.limit)?;
            for e in &entries {
                println!("{}", e.command);
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}
