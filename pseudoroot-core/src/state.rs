//! Fake root state management
//!
//! This module provides the core state structures for tracking fake ownership
//! and permissions in the pseudoroot system.

use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Device + inode identity for a filesystem object.
pub type InodeKey = (u64, u64);

/// Per-inode fake metadata (ownership, mode, extended attributes).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FakeInode {
    /// The fake user ID
    pub uid: u32,
    /// The fake group ID
    pub gid: u32,
    /// Full mode to report (type + permission bits). Set by `chmod`/`mknod`.
    pub mode: Option<u32>,
    /// Device id to report for device nodes. Set by `mknod`.
    pub rdev: Option<u64>,
    /// Faked extended attributes (e.g. `security.capability`).
    pub xattrs: HashMap<String, Vec<u8>>,
}

impl FakeInode {
    /// Create a new inode entry with the given UID and GID.
    #[must_use]
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            mode: None,
            rdev: None,
            xattrs: HashMap::new(),
        }
    }
}

/// The global fake root state
pub struct FakeRootState {
    /// Map from `(dev, ino)` to fake inode metadata
    pub inode_map: DashMap<InodeKey, FakeInode>,
    /// The current fake UID to report (atomic for lock-free reads)
    pub current_uid: AtomicU32,
    /// The current fake GID to report (atomic for lock-free reads)
    pub current_gid: AtomicU32,
}

impl Default for FakeRootState {
    fn default() -> Self {
        Self {
            inode_map: DashMap::new(),
            current_uid: AtomicU32::new(0),
            current_gid: AtomicU32::new(0),
        }
    }
}

impl FakeRootState {
    /// Create a new FakeRootState
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the current fake UID and GID
    #[inline]
    pub fn set_current(&self, uid: u32, gid: u32) {
        self.current_uid.store(uid, Ordering::Relaxed);
        self.current_gid.store(gid, Ordering::Relaxed);
    }

    /// Get the current fake UID (lock-free atomic read)
    #[inline]
    #[must_use]
    pub fn current_uid(&self) -> u32 {
        self.current_uid.load(Ordering::Relaxed)
    }

    /// Get the current fake GID (lock-free atomic read)
    #[inline]
    #[must_use]
    pub fn current_gid(&self) -> u32 {
        self.current_gid.load(Ordering::Relaxed)
    }

    /// Record fake metadata for an inode (lock-free concurrent insert)
    #[inline]
    pub fn set_inode(&self, key: InodeKey, inode: FakeInode) {
        self.inode_map.insert(key, inode);
    }

    /// Look up fake metadata for an inode (lock-free concurrent read)
    #[inline]
    #[must_use]
    pub fn get_inode(&self, key: InodeKey) -> Option<FakeInode> {
        self.inode_map.get(&key).map(|entry| entry.value().clone())
    }

    /// Drop the fake metadata entry for an inode
    #[inline]
    pub fn remove_inode(&self, key: InodeKey) -> Option<FakeInode> {
        self.inode_map.remove(&key).map(|(_, v)| v)
    }

    /// Sentinel for leaving one id unchanged in [`Self::upsert_chown`].
    pub const ID_UNCHANGED: u32 = u32::MAX;

    /// Merge a chown into the inode map in one lock-free update.
    #[inline]
    pub fn upsert_chown(
        &self,
        key: InodeKey,
        uid: u32,
        gid: u32,
        default_uid: u32,
        default_gid: u32,
    ) {
        self.inode_map
            .entry(key)
            .and_modify(|inode| {
                if uid != Self::ID_UNCHANGED {
                    inode.uid = uid;
                }
                if gid != Self::ID_UNCHANGED {
                    inode.gid = gid;
                }
            })
            .or_insert_with(|| {
                let mut inode = FakeInode::new(default_uid, default_gid);
                if uid != Self::ID_UNCHANGED {
                    inode.uid = uid;
                }
                if gid != Self::ID_UNCHANGED {
                    inode.gid = gid;
                }
                inode
            });
    }
}

// Global state access
// This uses a RwLock for thread-safe access to the global state
static GLOBAL_STATE: std::sync::OnceLock<RwLock<FakeRootState>> = std::sync::OnceLock::new();

/// Initialize the global fake root state
pub fn init_global_state() -> RwLockWriteGuard<'static, FakeRootState> {
    let lock = GLOBAL_STATE.get_or_init(|| RwLock::new(FakeRootState::new()));
    lock.write()
        .expect("Failed to acquire write lock on global state")
}

/// Get a read lock on the global fake root state
pub fn global_state_read() -> RwLockReadGuard<'static, FakeRootState> {
    let lock = GLOBAL_STATE.get_or_init(|| RwLock::new(FakeRootState::new()));
    lock.read()
        .expect("Failed to acquire read lock on global state")
}

/// Get a write lock on the global fake root state
pub fn global_state_write() -> RwLockWriteGuard<'static, FakeRootState> {
    let lock = GLOBAL_STATE.get_or_init(|| RwLock::new(FakeRootState::new()));
    lock.write()
        .expect("Failed to acquire write lock on global state")
}
