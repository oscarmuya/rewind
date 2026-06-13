use anyhow::Result;
use std::path::PathBuf;

use crate::db::data_path;

/// Returns the path to the Unix domain socket the daemon listens on.
pub fn socket_path() -> Result<PathBuf> {
    Ok(data_path()?.join("rewind.sock"))
}

/// Every message sent over the socket is a newline-delimited JSON string
/// followed by '\n'. The daemon reads until '\n' and deserializes.
/// This module documents the protocol; actual serialization is in entry.rs
/// (HookPayload) and rewind-daemon.
pub const PROTOCOL_VERSION: u8 = 1;
