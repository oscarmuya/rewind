# rewind

`rewind` is a per-project command history tool for your shell. It records the
commands you run, where you ran them, their Git repository and branch context,
exit status, duration, and timestamp, then lets you search that history from the
terminal.

The workspace builds two binaries:

- `rw`: the user-facing CLI for replaying recent commands, setup, search, and
  manual command recording.
- `rw-daemon`: the background process used by shell integrations to persist
  command history quickly.

## What It Records

Each history entry stores:

- command line
- current working directory
- Git repository root, when available
- Git branch, when available
- exit code
- duration in milliseconds
- start timestamp

`rewind` stores metadata about commands, not command output. Command lines can
still contain sensitive values, so avoid putting secrets directly in commands if
you do not want them saved in local history.

## Requirements

- Rust toolchain with Cargo
- Unix-like system with Unix domain sockets
- `python3` and `socat` for the shell integrations
- one of `bash`, `zsh`, or `fish` for automatic shell recording

## Install From Source

Clone the repository, then install both binaries:

```sh
cargo install --path rewind-cli
cargo install --path rewind-daemon
```

Make sure Cargo's binary directory is on your `PATH`, usually:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

You can also run the tools from the repository without installing:

```sh
cargo run --bin rw -- --help
cargo run --bin rw-daemon
```

## Shell Integration

The shell integration records normal interactive commands automatically. It
starts `rw-daemon` when needed, sends command metadata to it after each command
finishes, and skips `rw` and `rw-daemon` commands to avoid recording rewind's own
activity.

Install the integration for your current shell:

```sh
rw init --install
```

Or choose a shell explicitly:

```sh
rw init bash --install
rw init zsh --install
rw init fish --install
```

Restart your shell, or source your shell startup file, after installing.

To inspect the snippet without installing it:

```sh
rw init zsh
```

To remove the managed block from your shell config:

```sh
rw init --uninstall
```

The installer writes a managed block to:

- bash: `~/.bashrc`
- zsh: `~/.zshrc`
- fish: `$XDG_CONFIG_HOME/fish/config.fish` or `~/.config/fish/config.fish`

## Manual Recording

You can record a single command without installing shell hooks:

```sh
rw run cargo test
rw run git status --short
```

`rw run` executes the command, records it, and exits with the same exit code. It
does not run through a shell, so use `sh -c` when you need shell syntax such as
pipes, redirects, variables, or glob expansion:

```sh
rw run sh -c 'rg TODO | wc -l'
```

If the daemon is not running, `rw run` writes directly to the local database.

## Replay Recent Commands

Run `rw` to open the recent-command picker:

```sh
rw
```

Print recent history without opening the TUI:

```sh
rw --plain
rw --plain --limit 20
```

Filter recent history by Git context or status:

```sh
rw --repo
rw --branch
rw --ok
rw --fail
```

Filters can be combined:

```sh
rw --repo --branch --fail --limit 10
```

`rw recent` is still accepted as a compatibility alias for the same behavior.

## Search History

Open the interactive TUI:

```sh
rw search
```

Start the TUI with an initial query:

```sh
rw search cargo
```

Print plain matches to stdout:

```sh
rw search cargo --plain
rw search cargo --plain --limit 20
```

In the TUI:

- type to filter commands
- use `Up`/`Down` or `k`/`j` to move
- press `Enter` to print the selected command
- press `Esc` or `Ctrl-C` to exit without selecting

## Data Files

On Linux, history is stored under:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/rewind/
```

The main files are:

- `history.db`: SQLite database containing command history
- `rewind.sock`: Unix socket used by `rw-daemon`

The database uses SQLite WAL mode. Removing `history.db` deletes your recorded
history.

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

## Current Limitations

- Shell integrations require `python3` and `socat`.
- The daemon communicates over a Unix domain socket, so Windows is not currently
  supported.
- Search is local to the SQLite database and currently uses case-insensitive
  substring matching.
- `rw run` executes commands directly, not through a shell, unless you explicitly
  call a shell yourself.
