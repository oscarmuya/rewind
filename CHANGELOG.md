# Changelog

## v0.7.0 - 2026-06-22

### Added

- Added interactive shortcut management, including a dedicated shortcut TUI.
- Added soft deletion and restoration for command history entries.
- Added direct replay of recent commands by index, such as `rw -1`.
- Added short option aliases for history filters and shortcut commands.
- Added structured GitHub issue templates for bugs, features, and documentation.

### Changed

- Collapsed repeated commands in the history TUI.
- Consolidated recent history and search into one interactive interface.
- Made recent history the default `rw` experience and removed the `recent` subcommand.
- Improved installer output and progress reporting.

## v0.6.0 - 2026-06-21

### Added

- Added adaptive light and dark TUI themes.
- Added command shortcuts and alias invocation.
- Added interactive history filters and an in-place search mode to recent history.

### Changed

- Reused the command argument parser across CLI entry points.

## v0.5.0 - 2026-06-18

### Added

- Added a replay editor so commands can be changed before execution.
- Added shared history TUI chrome and documented its editor controls.

### Changed

- Cached command display metadata to improve TUI performance.

## v0.4.0 - 2026-06-16

### Added

- Added fuzzy history search scoped to the current project root.
- Added the rewind logo and expanded command-editing documentation.

### Fixed

- Improved fuzzy-search rerun behavior.

## v0.3.0 - 2026-06-15

### Added

- Added automatic binary updates when running the installer again.
- Added working-directory filtering for recent commands.
- Added shell-context command execution support.

### Changed

- Persisted and scoped history at Git project roots.
- Improved recent-command reruns.

### Fixed

- Prevented already-parsed command arguments from being tokenized again.

## v0.2.0 - 2026-06-15

### Added

- Added `rw status` to check shell integration installation, hook runtime visibility, required shell tools, daemon connectivity, and database health.
- Added Linux aarch64 release artifacts for Ubuntu, Debian, and Fedora builds.

### Fixed

- Improved the installer to select architecture-specific Linux assets, fall back to the Debian build for unknown glibc Linux distributions, and verify archives more reliably.
- Fixed installer status output so fallback messages do not get mixed into detected asset names.
- Improved shell hook startup so stale sockets no longer prevent daemon startup, existing daemons are left running, and background recording is quieter in interactive shells.

## v0.1.0 - 2026-06-14

### Added

- Added the `rw` CLI for replaying recent commands, searching history, manually recording commands, and installing shell integrations.
- Added the `rw-daemon` background service for fast command history recording through Unix sockets.
- Added SQLite-backed command history with Git repository, branch, exit status, duration, working directory, and timestamp metadata.
- Added shell integrations for bash, zsh, and fish.
- Added interactive TUI views for recent commands and command search.
- Added release packaging for Linux and macOS artifacts.
- Added a release installer script for installing `rw` and `rw-daemon` from GitHub release artifacts.

### Fixed

- Fixed release workflow configuration for macOS runner selection and artifact publishing.

### Changed

- Cleaned up formatting issues across command and database modules.
