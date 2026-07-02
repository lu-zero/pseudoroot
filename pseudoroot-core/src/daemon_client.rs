//! Daemon client for connecting to pseudoroot-daemon
//!
//! This module provides functionality for the interposed library to connect
//! to a running daemon for persistent state management.

use crate::protocol::{
    ChownPayload, InodeKeyPayload, InodeStatePayload, InodeStateResult, IpcPayload, MessageType,
    ProtocolMessage, UidGidPayload,
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

/// Send an RPC to the daemon and return its response, or `None` if there's
/// no live connection or the round-trip fails.
fn daemon_request(message_type: MessageType, payload: Vec<u8>) -> Option<ProtocolMessage> {
    let channel = get_daemon_channel()?;
    let mut channel_guard = channel.lock().ok()?;
    let message = ProtocolMessage::new(message_type, payload, crate::protocol::next_request_id());
    channel_guard.request(message).ok()
}

/// An RPC that reports success as "not an error response".
fn daemon_request_ok(message_type: MessageType, payload: Vec<u8>) -> bool {
    daemon_request(message_type, payload).is_some_and(|r| r.message_type != MessageType::Error)
}

/// Get current UID/GID from daemon
pub fn daemon_get_current_uid_gid() -> Option<(u32, u32)> {
    let response = daemon_request(MessageType::GetCurrentUidGid, vec![])?;
    if response.message_type != MessageType::Response {
        return None;
    }
    UidGidPayload::from_payload(&response.payload).map(|p| (p.uid, p.gid))
}

/// Set current UID/GID in daemon
pub fn daemon_set_current_uid_gid(uid: u32, gid: u32) -> bool {
    let payload = UidGidPayload { uid, gid };
    daemon_request_ok(MessageType::SetCurrentUidGid, payload.to_payload())
}

/// Get inode state from daemon
pub fn daemon_get_inode(key: InodeKey) -> Option<FakeInode> {
    let payload = InodeKeyPayload {
        dev: key.0,
        ino: key.1,
    };
    let response = daemon_request(MessageType::GetOwnership, payload.to_payload())?;
    if response.message_type != MessageType::Response {
        return None;
    }
    let r = InodeStateResult::from_payload(&response.payload)?;
    r.found.then_some(FakeInode {
        uid: r.uid,
        gid: r.gid,
        mode: r.mode,
        rdev: r.rdev,
        xattrs: r.xattrs,
    })
}

/// Merge a chown into daemon state in one RPC.
pub fn daemon_upsert_chown(
    key: InodeKey,
    uid: u32,
    gid: u32,
    default_uid: u32,
    default_gid: u32,
) -> bool {
    let payload = ChownPayload {
        dev: key.0,
        ino: key.1,
        uid,
        gid,
        default_uid,
        default_gid,
    };
    daemon_request_ok(MessageType::UpsertChown, payload.to_payload())
}

/// Set inode state in daemon
pub fn daemon_set_inode(key: InodeKey, inode: &FakeInode) -> bool {
    let payload = InodeStatePayload {
        dev: key.0,
        ino: key.1,
        uid: inode.uid,
        gid: inode.gid,
        mode: inode.mode,
        rdev: inode.rdev,
        xattrs: inode.xattrs.clone(),
    };
    daemon_request_ok(MessageType::RegisterOwnership, payload.to_payload())
}

/// Remove inode state from daemon
pub fn daemon_remove_inode(key: InodeKey) -> bool {
    let payload = InodeKeyPayload {
        dev: key.0,
        ino: key.1,
    };
    daemon_request_ok(MessageType::RemoveOwnership, payload.to_payload())
}

/// Initialize daemon with UID/GID
pub fn daemon_init(uid: u32, gid: u32) -> bool {
    let payload = UidGidPayload { uid, gid };
    daemon_request_ok(MessageType::Init, payload.to_payload())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    /// `DAEMON_SOCKET_ENV` is process-wide state; serialize the tests that
    /// touch it so they don't race each other under the default parallel
    /// test runner (each test still restores whatever value it found).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_daemon_mode_enabled_no_env() {
        let _guard = lock_env();
        let old_var = env::var(DAEMON_SOCKET_ENV);
        env::remove_var(DAEMON_SOCKET_ENV);
        assert!(!daemon_mode_enabled());
        if let Ok(var) = old_var {
            env::set_var(DAEMON_SOCKET_ENV, var);
        }
    }

    #[test]
    fn test_daemon_mode_enabled_with_env() {
        let _guard = lock_env();
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
        let _guard = lock_env();
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
