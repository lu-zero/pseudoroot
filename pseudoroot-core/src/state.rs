//! Fake root state management
//!
//! This module provides the core state structures for tracking fake ownership
//! and permissions in the pseudoroot system.

use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

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
    /// Map from file path to its fake ownership
    pub ownership_map: DashMap<String, FileOwnership>,
    /// The current fake UID to report (atomic for lock-free reads)
    pub current_uid: AtomicU32,
    /// The current fake GID to report (atomic for lock-free reads)
    pub current_gid: AtomicU32,
}

impl Default for FakeRootState {
    fn default() -> Self {
        Self {
            ownership_map: DashMap::new(),
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

    /// Set the ownership of a file or directory (lock-free concurrent insert)
    #[inline]
    pub fn set_ownership(&mut self, path: String, ownership: FileOwnership) {
        self.ownership_map.insert(path, ownership);
    }

    /// Get the ownership of a file or directory (lock-free concurrent read)
    #[inline]
    #[must_use]
    pub fn get_ownership(&self, path: &str) -> Option<FileOwnership> {
        self.ownership_map.get(path).map(|entry| *entry.value())
    }

    /// Remove the ownership entry for a file or directory
    #[inline]
    pub fn remove_ownership(&mut self, path: &str) -> Option<FileOwnership> {
        self.ownership_map.remove(path).map(|(_, v)| v)
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
///
/// # Panics
/// Panics if the global state has not been initialized
pub fn global_state_read() -> RwLockReadGuard<'static, FakeRootState> {
    let lock = GLOBAL_STATE
        .get()
        .expect("Global state not initialized. Call init_global_state() first.");
    lock.read()
        .expect("Failed to acquire read lock on global state")
}

/// Get a write lock on the global fake root state
///
/// # Panics
/// Panics if the global state has not been initialized
pub fn global_state_write() -> RwLockWriteGuard<'static, FakeRootState> {
    let lock = GLOBAL_STATE
        .get()
        .expect("Global state not initialized. Call init_global_state() first.");
    lock.write()
        .expect("Failed to acquire write lock on global state")
}
