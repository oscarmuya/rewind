use anyhow::{Context, Result, bail};
use clap::{Args as ClapArgs, ValueEnum};
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

const START_MARKER: &str = "# >>> rewind init >>>";
const END_MARKER: &str = "# <<< rewind init <<<";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Shell to configure. If omitted, rw detects it from $SHELL.
    #[arg(value_enum)]
    pub shell: Option<Shell>,

    /// Install the integration into the shell startup file.
    #[arg(long, conflicts_with = "uninstall")]
    pub install: bool,

    /// Remove the integration from the shell startup file.
    #[arg(long, conflicts_with = "install")]
    pub uninstall: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    pub(crate) fn detect() -> Result<Self> {
        let shell = env::var("SHELL").context("could not detect shell: $SHELL is not set")?;
        let name = Path::new(&shell)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        match name {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            other => bail!("unsupported shell `{other}`. Choose bash, zsh, or fish."),
        }
    }

    fn snippet(self) -> &'static str {
        match self {
            Self::Bash => include_str!("../../shell/bash.bash"),
            Self::Zsh => include_str!("../../shell/zsh.zsh"),
            Self::Fish => include_str!("../../shell/fish.fish"),
        }
    }

    pub(crate) fn config_path(self) -> Result<PathBuf> {
        match self {
            Self::Bash => Ok(home_dir()?.join(".bashrc")),
            Self::Zsh => Ok(home_dir()?.join(".zshrc")),
            Self::Fish => {
                if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
                    Ok(PathBuf::from(xdg_config_home).join("fish/config.fish"))
                } else {
                    Ok(home_dir()?.join(".config/fish/config.fish"))
                }
            }
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bash => f.write_str("bash"),
            Self::Zsh => f.write_str("zsh"),
            Self::Fish => f.write_str("fish"),
        }
    }
}

pub fn execute(args: Args) -> Result<()> {
    let shell = match args.shell {
        Some(shell) => shell,
        Option::None => Shell::detect()?,
    };

    match (args.install, args.uninstall) {
        (true, false) => install(shell),
        (false, true) => uninstall(shell),
        (false, false) => {
            print!("{}", shell.snippet());
            Ok(())
        }
        (true, true) => unreachable!("clap prevents --install and --uninstall together"),
    }
}

fn install(shell: Shell) -> Result<()> {
    let path = shell.config_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create config directory: {}", parent.display()))?;
    }

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let cleaned = remove_managed_block(&existing);
    let updated = append_block(&cleaned, &managed_block(shell));

    fs::write(&path, updated)
        .with_context(|| format!("could not write shell config: {}", path.display()))?;

    eprintln!("installed rewind {shell} integration in {}", path.display());
    eprintln!("restart your shell or source the config file to activate it");

    Ok(())
}

fn uninstall(shell: Shell) -> Result<()> {
    let path = shell.config_path()?;

    if !path.exists() {
        eprintln!("no {shell} config found at {}", path.display());
        return Ok(());
    }

    let existing = fs::read_to_string(&path)
        .with_context(|| format!("could not read shell config: {}", path.display()))?;

    let updated = remove_managed_block(&existing);

    fs::write(&path, updated)
        .with_context(|| format!("could not write shell config: {}", path.display()))?;

    eprintln!("removed rewind {shell} integration from {}", path.display());

    Ok(())
}

fn managed_block(shell: Shell) -> String {
    format!(
        "{START_MARKER}\n# Managed by `rw init --install`; remove with `rw init --uninstall`.\n{}\n{END_MARKER}\n",
        shell.snippet().trim()
    )
}

fn append_block(existing: &str, block: &str) -> String {
    let existing = existing.trim_end();

    if existing.is_empty() {
        return block.to_owned();
    }

    format!("{existing}\n\n{block}")
}

fn remove_managed_block(input: &str) -> String {
    let mut output = String::new();
    let mut inside_block = false;

    for line in input.lines() {
        match line.trim() {
            START_MARKER => {
                inside_block = true;
            }
            END_MARKER if inside_block => {
                inside_block = false;
            }
            _ if !inside_block => {
                output.push_str(line);
                output.push('\n');
            }
            _ => {}
        }
    }

    output.trim_end().to_owned() + "\n"
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("could not find home directory: $HOME is not set")
}
