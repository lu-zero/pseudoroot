//! Client-side access to an inherited session shared-memory map.

use crate::shm_map::{ShmInodeMap, SHM_FD_ENV};
use crate::state::{FakeInode, InodeKey};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

static SHM_MAP: OnceLock<Arc<ShmInodeMap>> = OnceLock::new();
static SHM_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether a shared-memory session map is active.
#[must_use]
pub fn shm_mode_active() -> bool {
    SHM_ACTIVE.load(Ordering::Acquire)
}

/// Map the inherited session table. Returns true when active.
pub fn init_shm_from_env() -> bool {
    if SHM_ACTIVE.load(Ordering::Acquire) {
        return true;
    }
    let Ok(fd_var) = env::var(SHM_FD_ENV) else {
        return false;
    };
    let Ok(fd) = fd_var.parse::<i32>() else {
        return false;
    };
    match ShmInodeMap::from_fd(fd) {
        Ok(map) => {
            let _ = SHM_MAP.set(map);
            SHM_ACTIVE.store(true, Ordering::Release);
            true
        }
        Err(err) => {
            eprintln!("pseudoroot: failed to map {SHM_FD_ENV}: {err}");
            false
        }
    }
}

fn map() -> Option<&'static Arc<ShmInodeMap>> {
    if shm_mode_active() {
        SHM_MAP.get()
    } else {
        None
    }
}

/// Read current fake uid/gid from shared memory.
pub fn shm_get_current_uid_gid() -> Option<(u32, u32)> {
    let map = map()?;
    Some((map.current_uid(), map.current_gid()))
}

/// Write current fake uid/gid to shared memory.
pub fn shm_set_current_uid_gid(uid: u32, gid: u32) {
    if let Some(map) = map() {
        map.set_current(uid, gid);
    }
}

/// Look up inode metadata from shared memory.
pub fn shm_get_inode(key: InodeKey) -> Option<FakeInode> {
    map()?.get_inode(key)
}

/// Merge a chown into the shared inode table.
pub fn shm_upsert_chown(key: InodeKey, uid: u32, gid: u32, default_uid: u32, default_gid: u32) {
    if let Some(map) = map() {
        map.upsert_chown(key, uid, gid, default_uid, default_gid);
    }
}

/// Insert or replace full inode metadata in shared memory.
pub fn shm_upsert_inode(key: InodeKey, inode: &FakeInode) {
    if let Some(map) = map() {
        map.upsert_inode(key, inode);
    }
}
