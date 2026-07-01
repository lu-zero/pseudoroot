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

/// Represents the ownership of a file or directory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileOwnership {
    /// The fake user ID
    pub uid: u32,
    /// The fake group ID
    pub gid: u32,
}

impl FileOwnership {
    /// Create a new FileOwnership with the given UID and GID
    #[must_use]
    pub const fn new(uid: u32, gid: u32) -> Self {
        Self { uid, gid }
    }
}

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

    /// Extract the ownership subset.
    #[must_use]
    pub const fn ownership(&self) -> FileOwnership {
        FileOwnership::new(self.uid, self.gid)
    }
}

impl From<FileOwnership> for FakeInode {
    fn from(ownership: FileOwnership) -> Self {
        Self::new(ownership.uid, ownership.gid)
    }
}

/// A mapping from real UID/GID to fake UID/GID
#[derive(Debug, Clone)]
pub struct UidGidMap {
    /// Map from real UID to fake UID
    pub uid_map: HashMap<u32, u32>,
    /// Map from real GID to fake GID
    pub gid_map: HashMap<u32, u32>,
    /// Reverse map from fake UID to real UID
    pub uid_reverse: HashMap<u32, u32>,
    /// Reverse map from fake GID to real GID
    pub gid_reverse: HashMap<u32, u32>,
}

impl Default for UidGidMap {
    fn default() -> Self {
        let mut map = Self {
            uid_map: HashMap::new(),
            gid_map: HashMap::new(),
            uid_reverse: HashMap::new(),
            gid_reverse: HashMap::new(),
        };
        // Always map root to root
        map.uid_map.insert(0, 0);
        map.gid_map.insert(0, 0);
        map.uid_reverse.insert(0, 0);
        map.gid_reverse.insert(0, 0);
        map
    }
}

impl UidGidMap {
    /// Add a UID mapping from real to fake
    #[inline]
    pub fn add_uid(&mut self, real: u32, fake: u32) {
        self.uid_map.insert(real, fake);
        self.uid_reverse.insert(fake, real);
    }

    /// Add a GID mapping from real to fake
    #[inline]
    pub fn add_gid(&mut self, real: u32, fake: u32) {
        self.gid_map.insert(real, fake);
        self.gid_reverse.insert(fake, real);
    }

    /// Get the fake UID for a real UID
    #[inline]
    #[must_use]
    pub fn get_uid(&self, real: u32) -> Option<u32> {
        self.uid_map.get(&real).copied()
    }

    /// Get the real UID for a fake UID
    #[inline]
    #[must_use]
    pub fn get_real_uid(&self, fake: u32) -> Option<u32> {
        self.uid_reverse.get(&fake).copied()
    }

    /// Get the fake GID for a real GID
    #[inline]
    #[must_use]
    pub fn get_gid(&self, real: u32) -> Option<u32> {
        self.gid_map.get(&real).copied()
    }

    /// Get the real GID for a fake GID
    #[inline]
    #[must_use]
    pub fn get_real_gid(&self, fake: u32) -> Option<u32> {
        self.gid_reverse.get(&fake).copied()
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
    pub fn set_current(&mut self, uid: u32, gid: u32) {
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
    pub fn set_inode(&mut self, key: InodeKey, inode: FakeInode) {
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
    pub fn remove_inode(&mut self, key: InodeKey) -> Option<FakeInode> {
        self.inode_map.remove(&key).map(|(_, v)| v)
    }

    /// Record fake ownership for an inode (lock-free concurrent insert)
    #[inline]
    pub fn set_inode_ownership(&mut self, key: InodeKey, ownership: FileOwnership) {
        self.inode_map
            .entry(key)
            .and_modify(|inode| {
                inode.uid = ownership.uid;
                inode.gid = ownership.gid;
            })
            .or_insert_with(|| ownership.into());
    }

    /// Look up fake ownership for an inode (lock-free concurrent read)
    #[inline]
    #[must_use]
    pub fn get_inode_ownership(&self, key: InodeKey) -> Option<FileOwnership> {
        self.get_inode(key).map(|inode| inode.ownership())
    }

    /// Drop the fake ownership entry for an inode
    #[inline]
    pub fn remove_inode_ownership(&mut self, key: InodeKey) -> Option<FileOwnership> {
        self.remove_inode(key).map(|inode| inode.ownership())
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
