//! Daemon client for connecting to pseudoroot-daemon
//!
//! This module provides functionality for the interposed library to connect
//! to a running daemon for persistent state management.

use crate::protocol::{
    IpcPayload, MessageType, OwnershipResult, PathPayload, ProtocolMessage, UidGidPayload,
};
use crate::state::FileOwnership;
use std::env;
use std::sync::OnceLock;

/// Environment variable to specify daemon socket path
pub const DAEMON_SOCKET_ENV: &str = "PSEUDOROOT_DAEMON_SOCKET";

/// Global daemon channel (lazy initialization)
static DAEMON_CHANNEL: OnceLock<std::sync::Mutex<crate::protocol::IpcChannel>> = OnceLock::new();

/// Check if daemon mode is enabled
#[must_use]
pub fn daemon_mode_enabled() -> bool {
    env::var(DAEMON_SOCKET_ENV).is_ok()
}

/// Get the daemon socket path from environment or default
#[must_use]
pub fn get_daemon_socket_path() -> Option<String> {
    env::var(DAEMON_SOCKET_ENV).ok()
}

/// Initialize connection to daemon
pub fn init_daemon_connection() -> Option<&'static std::sync::Mutex<crate::protocol::IpcChannel>> {
    if !daemon_mode_enabled() {
        return None;
    }

    let socket_path = get_daemon_socket_path()?;
    let channel = DAEMON_CHANNEL.get_or_init(|| {
        let mut channel = crate::protocol::IpcChannel::new(socket_path);
        // Try to connect
        let _ = channel.connect();
        std::sync::Mutex::new(channel)
    });

    Some(channel)
}

/// Get a reference to the daemon channel if available
#[must_use]
pub fn get_daemon_channel() -> Option<&'static std::sync::Mutex<crate::protocol::IpcChannel>> {
    DAEMON_CHANNEL.get()
}

/// Get current UID/GID from daemon
pub fn daemon_get_current_uid_gid() -> Option<(u32, u32)> {
    let channel = get_daemon_channel()?;
    let mut channel_guard = channel.lock().unwrap();
    let message = ProtocolMessage::new(
        MessageType::GetCurrentUidGid,
        vec![],
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => {
            if response.message_type == MessageType::Response {
                UidGidPayload::from_payload(&response.payload).map(|p| (p.uid, p.gid))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Set current UID/GID in daemon
pub fn daemon_set_current_uid_gid(uid: u32, gid: u32) -> bool {
    let channel = get_daemon_channel().unwrap();
    let mut channel_guard = channel.lock().unwrap();
    let payload = UidGidPayload { uid, gid };
    let message = ProtocolMessage::new(
        MessageType::SetCurrentUidGid,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => response.message_type != MessageType::Error,
        Err(_) => false,
    }
}

/// Get ownership for a path from daemon
pub fn daemon_get_ownership(path: &str) -> Option<FileOwnership> {
    let channel = get_daemon_channel()?;
    let mut channel_guard = channel.lock().unwrap();
    let payload = PathPayload {
        path: path.to_string(),
    };
    let message = ProtocolMessage::new(
        MessageType::GetOwnership,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => {
            if response.message_type == MessageType::Response {
                OwnershipResult::from_payload(&response.payload)
                    .map(|r| FileOwnership::new(r.uid, r.gid))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Set ownership for a path in daemon
pub fn daemon_set_ownership(path: String, ownership: FileOwnership) -> bool {
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = channel.lock().unwrap();
    use crate::protocol::OwnershipPayload;
    let payload = OwnershipPayload {
        path,
        uid: ownership.uid,
        gid: ownership.gid,
    };
    let message = ProtocolMessage::new(
        MessageType::RegisterOwnership,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => response.message_type != MessageType::Error,
        Err(_) => false,
    }
}

/// Remove ownership for a path from daemon
pub fn daemon_remove_ownership(path: &str) -> bool {
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = channel.lock().unwrap();
    let payload = PathPayload {
        path: path.to_string(),
    };
    let message = ProtocolMessage::new(
        MessageType::RemoveOwnership,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => response.message_type != MessageType::Error,
        Err(_) => false,
    }
}

/// Initialize daemon with UID/GID
pub fn daemon_init(uid: u32, gid: u32) -> bool {
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = channel.lock().unwrap();
    let payload = UidGidPayload { uid, gid };
    let message = ProtocolMessage::new(
        MessageType::Init,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => response.message_type != MessageType::Error,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_daemon_mode_enabled_no_env() {
        // Save current env
        let old_var = env::var(DAEMON_SOCKET_ENV);

        // Clear the env var
        env::remove_var(DAEMON_SOCKET_ENV);

        assert!(!daemon_mode_enabled());

        // Restore
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        }
    }

    #[test]
    fn test_daemon_mode_enabled_with_env() {
        // Save current env
        let old_var = env::var(DAEMON_SOCKET_ENV);

        // Set the env var
        env::set_var(DAEMON_SOCKET_ENV, "/tmp/test.sock");

        assert!(daemon_mode_enabled());

        // Restore
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        } else {
            env::remove_var(DAEMON_SOCKET_ENV);
        }
    }

    #[test]
    fn test_get_daemon_socket_path() {
        // Save current env
        let old_var = env::var(DAEMON_SOCKET_ENV);

        // Clear the env var
        env::remove_var(DAEMON_SOCKET_ENV);

        assert_eq!(get_daemon_socket_path(), None);

        // Set the env var
        env::set_var(DAEMON_SOCKET_ENV, "/tmp/custom.sock");

        assert_eq!(
            get_daemon_socket_path(),
            Some("/tmp/custom.sock".to_string())
        );

        // Restore
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        } else {
            env::remove_var(DAEMON_SOCKET_ENV);
        }
    }
}
