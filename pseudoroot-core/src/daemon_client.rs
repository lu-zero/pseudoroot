//! Daemon client for connecting to pseudoroot-daemon
//!
//! This module provides functionality for the interposed library to connect
//! to a running daemon for persistent state management.

use crate::protocol::{
    InodeKeyPayload, InodeStatePayload, InodeStateResult, IpcPayload, MessageType, ProtocolMessage,
    UidGidPayload,
};
use crate::state::{FakeInode, InodeKey};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Environment variable to specify daemon socket path
pub const DAEMON_SOCKET_ENV: &str = "PSEUDOROOT_DAEMON_SOCKET";

/// Global daemon channel (lazy initialization)
static DAEMON_CHANNEL: OnceLock<std::sync::Mutex<crate::protocol::IpcChannel>> = OnceLock::new();
static DAEMON_CONNECTED: AtomicBool = AtomicBool::new(false);

/// Check if daemon mode is requested via environment
#[must_use]
pub fn daemon_mode_enabled() -> bool {
    env::var(DAEMON_SOCKET_ENV).is_ok()
}

/// Check if daemon mode is active (env set and connection established)
#[must_use]
pub fn daemon_mode_active() -> bool {
    daemon_mode_enabled() && DAEMON_CONNECTED.load(Ordering::Relaxed)
}

/// Get the daemon socket path from environment or default
#[must_use]
pub fn get_daemon_socket_path() -> Option<String> {
    env::var(DAEMON_SOCKET_ENV).ok()
}

/// Initialize connection to daemon. Returns true if connected.
pub fn init_daemon_connection() -> bool {
    if !daemon_mode_enabled() {
        return false;
    }

    let socket_path = match get_daemon_socket_path() {
        Some(p) => p,
        None => return false,
    };

    let connected = DAEMON_CHANNEL.get_or_init(|| {
        let mut channel = crate::protocol::IpcChannel::new(socket_path);
        let ok = channel.connect().is_ok();
        DAEMON_CONNECTED.store(ok, Ordering::Relaxed);
        std::sync::Mutex::new(channel)
    });

    DAEMON_CONNECTED.load(Ordering::Relaxed) || {
        let mut channel = connected.lock().unwrap();
        let ok = channel.connect().is_ok();
        DAEMON_CONNECTED.store(ok, Ordering::Relaxed);
        ok
    }
}

/// Get a reference to the daemon channel if available and connected
#[must_use]
pub fn get_daemon_channel() -> Option<&'static std::sync::Mutex<crate::protocol::IpcChannel>> {
    if !daemon_mode_active() {
        return None;
    }
    DAEMON_CHANNEL.get()
}

/// Get current UID/GID from daemon
pub fn daemon_get_current_uid_gid() -> Option<(u32, u32)> {
    let channel = get_daemon_channel()?;
    let mut channel_guard = channel.lock().ok()?;
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
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = match channel.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
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

/// Get inode state from daemon
pub fn daemon_get_inode(key: InodeKey) -> Option<FakeInode> {
    let channel = get_daemon_channel()?;
    let mut channel_guard = channel.lock().ok()?;
    let payload = InodeKeyPayload {
        dev: key.0,
        ino: key.1,
    };
    let message = ProtocolMessage::new(
        MessageType::GetOwnership,
        payload.to_payload(),
        crate::protocol::next_request_id(),
    );

    match channel_guard.request(message) {
        Ok(response) => {
            if response.message_type == MessageType::Response {
                InodeStateResult::from_payload(&response.payload).and_then(|r| {
                    if r.found {
                        Some(FakeInode {
                            uid: r.uid,
                            gid: r.gid,
                            mode: r.mode,
                            rdev: r.rdev,
                            xattrs: r.xattrs,
                        })
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Set inode state in daemon
pub fn daemon_set_inode(key: InodeKey, inode: &FakeInode) -> bool {
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = match channel.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let payload = InodeStatePayload {
        dev: key.0,
        ino: key.1,
        uid: inode.uid,
        gid: inode.gid,
        mode: inode.mode,
        rdev: inode.rdev,
        xattrs: inode.xattrs.clone(),
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

/// Remove inode state from daemon
pub fn daemon_remove_inode(key: InodeKey) -> bool {
    let channel = match get_daemon_channel() {
        Some(ch) => ch,
        None => return false,
    };
    let mut channel_guard = match channel.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let payload = InodeKeyPayload {
        dev: key.0,
        ino: key.1,
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
    let mut channel_guard = match channel.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
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
        let old_var = env::var(DAEMON_SOCKET_ENV);
        env::remove_var(DAEMON_SOCKET_ENV);
        assert!(!daemon_mode_enabled());
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        }
    }

    #[test]
    fn test_daemon_mode_enabled_with_env() {
        let old_var = env::var(DAEMON_SOCKET_ENV);
        env::set_var(DAEMON_SOCKET_ENV, "/tmp/test.sock");
        assert!(daemon_mode_enabled());
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        } else {
            env::remove_var(DAEMON_SOCKET_ENV);
        }
    }

    #[test]
    fn test_get_daemon_socket_path() {
        let old_var = env::var(DAEMON_SOCKET_ENV);
        env::remove_var(DAEMON_SOCKET_ENV);
        assert_eq!(get_daemon_socket_path(), None);
        env::set_var(DAEMON_SOCKET_ENV, "/tmp/custom.sock");
        assert_eq!(
            get_daemon_socket_path(),
            Some("/tmp/custom.sock".to_string())
        );
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        } else {
            env::remove_var(DAEMON_SOCKET_ENV);
        }
    }
}
