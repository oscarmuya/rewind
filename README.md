<p align="center">
    <a href="https://oscardev.site">
        <img src="https://github.com/user-attachments/assets/eca3754c-4766-4253-bbb4-bc334653a941" width="300"></a><!-- </a> being on the same line as the <img> tag is intentional! -->
    <br>

<a href="https://github.com/oscarmuya/rewind/actions/workflows/ci.yml">
<img src="https://img.shields.io/github/actions/workflow/status/oscarmuya/rewind/ci.yml?branch=main&style=flat&labelColor=1C2C2E&color=BEC5C9&logo=GitHub%20Actions&logoColor=BEC5C9&label=ci"></a>
      <a href="https://github.com/oscarmuya/rewind/actions/workflows/release.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/oscarmuya/rewind/release.yml?style=flat&labelColor=1C2C2E&color=BEC5C9&logo=GitHub%20Actions&logoColor=BEC5C9&label=release"></a>
<a href="https://github.com/oscarmuya/rewind/releases/latest">
    <img src="https://img.shields.io/github/v/release/oscarmuya/rewind?sort=semver&style=flat&labelColor=1C2C2E&color=BEC5C9&logo=GitHub&logoColor=BEC5C9&label=latest"></a>
<br>
</p>

`rewind` is a per-project command history tool for your shell. It records the
commands you run, where you ran them, their Git repository and branch context,
exit status, duration, and timestamp, then lets you search that history from the
terminal.

<!-- SCREENSHOT: hero screenshot of the TUI picker in action -->
<img width="1298" height="670" alt="image" src="https://github.com/user-attachments/assets/a4c1f675-7aba-4544-bed8-850189ee3cd1" />


## Install

Run the installer:

```sh
curl -fsSL https://raw.githubusercontent.com/oscarmuya/rewind/main/install.sh | sh
```

Then enable shell recording:

```sh
rw init --install
```

Restart your shell, or source your shell startup file, and you are ready to go.

## Quick Start

Once installed, your commands are recorded automatically. Open the picker to
replay any recent command:

```sh
rw
```

Search your history:

```sh
rw search cargo
```

<!-- SCREENSHOT: search mode with results -->
<img width="1303" height="672" alt="image" src="https://github.com/user-attachments/assets/cb681e57-746f-45aa-a386-ed32dbff28d9" />


Replay the most recent command directly:

```sh
rw -1
```

Save a command you run often as a shortcut:

```sh
rw shortcut add test cargo test
rw test
```

## Replaying Recent Commands

Run `rw` to open the recent-command picker:

```sh
rw
```

Replay the Nth most recent command directly (after applying any filters):

```sh
rw -1
rw -2 --repo --ok
rw -3 --plain
```

The command is printed before it runs. With `--plain`, it is only printed.

Print recent history without opening the TUI:

```sh
rw --plain
rw --plain --limit 20
```

Filter recent history by Git context or exit status:

```sh
rw --repo       # limit to the current Git repository
rw --cwd        # limit to the current directory
rw --branch     # limit to the current Git branch
rw --ok         # only successful commands
rw --fail       # only failed commands
rw --deleted    # show soft-deleted commands
```

Filters can be combined:

```sh
rw --repo --branch --fail --limit 10
```

**TUI controls:**

- `Up`/`Down` or `k`/`j` to move between commands
- `/` to enter search mode and type to filter
- in search mode, `Up`/`Down` to navigate, `Esc` to clear and return to the list
- `Enter` or click to open a command in the replay editor
- edit the command, then `Enter` to run it
- `Alt+Enter` in the editor to insert a newline for multiline commands
- `Esc` in the editor to cancel and return to the list
- `Esc` or `Ctrl-C` in the list to exit without running anything
- `dd` to soft-delete a command; `x` to show deleted commands, `dd` to restore one

## Searching History

Open the TUI directly in search mode:

```sh
rw search
```

Start with an initial query:

```sh
rw search cargo
```

Print plain matches to stdout instead of opening the TUI:

```sh
rw search cargo --plain
rw search cargo --plain --limit 20
```

<!-- SCREENSHOT: plain search output in terminal -->
<img width="1333" height="167" alt="image" src="https://github.com/user-attachments/assets/30f1ce91-5a39-4e14-97ac-47ae20f8dd6f" />


## Shortcuts

Save frequently used commands as short aliases scoped to the current project:

```sh
rw shortcut add test cargo test
rw shortcut add lint cargo clippy --workspace --all-targets
```

Run a saved shortcut by passing its alias to `rw`:

```sh
rw test
rw lint
```

Rewind prints the expanded command to stderr before running it.

Create a shortcut available from any project with `--global`:

```sh
rw shortcut add --global gs git status --short
rw gs
```

Open the shortcut manager TUI for the current project:

```sh
rw shortcut
```

Use `Enter` to edit, `dd` to delete, and `q` or `Esc` to close.

<!-- SCREENSHOT: shortcut manager TUI -->
<img width="1303" height="674" alt="image" src="https://github.com/user-attachments/assets/9fa67d21-3a76-4eaa-ac62-7fb6fae014a8" />


Other shortcut commands:

```sh
rw shortcut list                              # list shortcuts for this project
rw shortcut list --global                     # list only global shortcuts
rw shortcut edit test cargo test --workspace  # edit a shortcut
rw shortcut edit --global gs git status --branch
rw shortcut remove test                       # remove a shortcut
rw shortcut remove gs --global
```

Project shortcuts take precedence over global ones with the same alias.
Reserved subcommand names (`search`, `status`, `shortcut`, etc.) cannot be used
as shortcut aliases.

## Manual Recording

Record a single command without shell hooks:

```sh
rw run cargo test
rw run git status --short
```

`rw run` executes the command, records it, and exits with the same exit code. It
does not run through a shell, so use `sh -c` for pipes, redirects, variables,
or glob expansion:

```sh
rw run sh -c 'rg TODO | wc -l'
```

## Checking Status

Run `rw status` to verify that automatic recording is set up and healthy:

```sh
rw status
```

It checks:

- shell integration installation for your detected shell
- whether the integration is active in the current session
- required tools (`python3` and `socat`)
- whether `rw-daemon` is accepting connections
- whether the SQLite history database opens and passes an integrity check

You can also check a specific shell:

```sh
rw status bash
rw status zsh
rw status fish
```

If the hook is installed but the runtime check warns it is not visible, restart
your shell or source your startup file. If the daemon check fails, opening a new
hooked shell usually starts `rw-daemon`; you can also run `rw-daemon` directly
to debug.

## What Gets Recorded

Each history entry stores:

- command line
- current working directory
- Git repository root (when available)
- Git branch (when available)
- exit code
- duration in milliseconds
- start timestamp

Rewind stores metadata about commands, not command output. Command lines can
still contain sensitive values, so avoid putting secrets directly in commands if
you do not want them saved in local history.

## Data Storage

On Linux, history is stored under:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/rewind/
```

The main files are:

- `history.db`: SQLite database containing command history and shortcuts
- `rewind.sock`: Unix socket used by `rw-daemon`

The database uses SQLite WAL mode. Removing `history.db` deletes all recorded
history.

## Shell Integration Details

The shell integration starts `rw-daemon` when needed, sends command metadata
after each command finishes, and skips `rw` and `rw-daemon` commands to avoid
recording rewind's own activity.

Install for your current shell:

```sh
rw init --install
```

Or choose a shell explicitly:

```sh
rw init bash --install
rw init zsh --install
rw init fish --install
```

The installer writes a managed block to:

- bash: `~/.bashrc`
- zsh: `~/.zshrc`
- fish: `$XDG_CONFIG_HOME/fish/config.fish` or `~/.config/fish/config.fish`

To inspect the snippet without installing:

```sh
rw init zsh
```

To remove the managed block:

```sh
rw init --uninstall
```

## Requirements

- Unix-like system with Unix domain sockets
- `curl` and `tar` for the release installer
- `python3` and `socat` for shell integrations
- one of `bash`, `zsh`, or `fish` for automatic recording

## Current Limitations

- Shell integrations require `python3` and `socat`.
- The daemon communicates over a Unix domain socket, so Windows is not currently
  supported.

---

## Installing From Source

Requires a Rust toolchain with Cargo.

Clone the repository, then install both binaries:

```sh
cargo install --path rewind-cli
cargo install --path rewind-daemon
```

Make sure Cargo's binary directory is on your `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

You can also run the tools from the repository without installing:

```sh
cargo run --bin rw -- --help
cargo run --bin rw-daemon
```

## CLI Reference

Short flags: `-l` (limit), `-p` (plain), `-c` (cwd), `-r` (repo), `-b`
(branch), `-o` (ok), `-f` (fail), `-d` (deleted). Init supports `-i` (install)
and `-u` (uninstall). Shortcut commands support `-g` (global).

## Development

This is a Cargo workspace with three crates:

- `rewind-core`: shared database, query, entry, Git, and socket logic
- `rewind-cli`: `rw` CLI and TUI
- `rewind-daemon`: background Unix socket daemon

Useful commands:

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run --bin rw -- --help
```

Shell integration smoke-test helpers are in `tests/shell/`:

```sh
tests/shell/bash.sh
tests/shell/zsh.sh
tests/shell/fish.sh
```
