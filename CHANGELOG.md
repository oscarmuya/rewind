# Changelog

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
