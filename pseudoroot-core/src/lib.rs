//! Shared state, IPC, and session backing for pseudoroot.
//!
//! Both the CLI/API crate (`pseudoroot`) and the interposed cdylib
//! (`pseudoroot-lib`) depend on this crate. It owns the inode-keyed fake
//! metadata model and the two multi-process backends:
//!
//! - [`shm_map`] / [`shm_client`] — lock-free shared-memory hash table inherited
//!   across `exec` within a session (default).
//! - [`daemon_server`] / [`daemon_client`] — Unix-socket RPC for external or
//!   in-process daemons (`pdrd`, `pdr start`, or `PSEUDOROOT_SESSION_SHM=0`).
//!
//! Per-process fallback lives in [`state`] (`global_state_read` /
//! `global_state_write`), used when neither SHM nor daemon mode is active.
//!
//! See [the architecture guide](https://github.com/lu-zero/pseudoroot/blob/master/docs/architecture.md)
//! for end-to-end flows and backend selection.

#![deny(missing_docs)]

pub mod daemon_client;
pub mod daemon_server;
pub mod protocol;
pub mod shm_client;
pub mod shm_map;
pub mod state;

pub use state::{FakeInode, FakeRootState, InodeKey};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_root_state_new() {
        let state = FakeRootState::new();
        assert_eq!(state.current_uid(), 0);
        assert_eq!(state.current_gid(), 0);
    }

    #[test]
    fn test_fake_root_state_set_current() {
        let state = FakeRootState::new();
        state.set_current(1234, 5678);
        assert_eq!(state.current_uid(), 1234);
        assert_eq!(state.current_gid(), 5678);
    }

    #[test]
    fn test_fake_root_state_inode_mode() {
        let state = FakeRootState::new();
        let key = (1u64, 42u64);
        let mut inode = FakeInode::new(0, 0);
        inode.mode = Some(0o4755);
        state.set_inode(key, inode.clone());
        assert_eq!(state.get_inode(key), Some(inode));
    }

    #[test]
    fn test_fake_root_state_upsert_chown() {
        let state = FakeRootState::new();
        state.upsert_chown((1, 2), 100, 200, 0, 0);
        let inode = state.get_inode((1, 2)).unwrap();
        assert_eq!(inode.uid, 100);
        assert_eq!(inode.gid, 200);
        state.upsert_chown((1, 2), FakeRootState::ID_UNCHANGED, 7, 0, 0);
        let inode = state.get_inode((1, 2)).unwrap();
        assert_eq!(inode.uid, 100);
        assert_eq!(inode.gid, 7);
    }

    #[test]
    fn test_fake_root_state_remove_inode() {
        let state = FakeRootState::new();
        let key = (1u64, 42u64);
        let inode = FakeInode::new(3000, 4000);
        state.set_inode(key, inode.clone());
        assert_eq!(state.get_inode(key), Some(inode.clone()));

        let removed = state.remove_inode(key);
        assert_eq!(removed, Some(inode));
        assert_eq!(state.get_inode(key), None);
    }
}
