use anyhow::Result;
use clap::Args as ClapArgs;
use rewind_core::{db, query};

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

pub fn execute(args: self::Args) -> Result<()> {
    let conn = db::open()?;

    match (args.term, args.plain) {
        // Plain text search to stdout.
        (Some(term), true) => {
            let entries = query::search_raw(&conn, &term, args.limit)?;
            for e in &entries {
                println!("{}", e.command);
            }
        }

        // Interactive TUI -- term is the pre-populated query or empty.
        (term, false) => {
            let initial = term.unwrap_or_default();
            crate::tui::run(&conn, &initial)?;
        }

        // --plain with no term: just dump recent history.
        (Option::None, true) => {
            let entries = query::recent(&conn, args.limit)?;
            for e in &entries {
                println!("{}", e.command);
            }
        }
    }

    Ok(())
}
