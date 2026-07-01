//! Core types and state management for pseudoroot
//!
//! This crate provides the shared data structures and state management
//! for the pseudoroot library interposition system.

pub mod daemon_client;
pub mod daemon_server;
pub mod protocol;
pub mod shm_client;
pub mod shm_map;
pub mod state;

pub use state::{FakeInode, FakeRootState, FileOwnership, InodeKey, UidGidMap};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_ownership_default() {
        let ownership = FileOwnership::default();
        assert_eq!(ownership.uid, 0);
        assert_eq!(ownership.gid, 0);
    }

    #[test]
    fn test_file_ownership_new() {
        let ownership = FileOwnership::new(1000, 2000);
        assert_eq!(ownership.uid, 1000);
        assert_eq!(ownership.gid, 2000);
    }

    #[test]
    fn test_uid_gid_map_default() {
        let map = UidGidMap::default();
        // Root should always map to root
        assert_eq!(map.get_uid(0), Some(0));
        assert_eq!(map.get_gid(0), Some(0));
        assert_eq!(map.get_real_uid(0), Some(0));
        assert_eq!(map.get_real_gid(0), Some(0));
    }

    #[test]
    fn test_uid_gid_map_add_uid() {
        let mut map = UidGidMap::default();
        map.add_uid(1000, 1001);
        assert_eq!(map.get_uid(1000), Some(1001));
        assert_eq!(map.get_real_uid(1001), Some(1000));
    }

    #[test]
    fn test_uid_gid_map_add_gid() {
        let mut map = UidGidMap::default();
        map.add_gid(2000, 2001);
        assert_eq!(map.get_gid(2000), Some(2001));
        assert_eq!(map.get_real_gid(2001), Some(2000));
    }

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
    fn test_fake_root_state_inode_ownership() {
        let state = FakeRootState::new();
        let ownership = FileOwnership::new(3000, 4000);
        let key = (1u64, 42u64);
        state.set_inode_ownership(key, ownership);
        assert_eq!(state.get_inode_ownership(key), Some(ownership));
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
        let ownership = state.get_inode_ownership((1, 2)).unwrap();
        assert_eq!(ownership.uid, 100);
        assert_eq!(ownership.gid, 200);
        state.upsert_chown((1, 2), FakeRootState::ID_UNCHANGED, 7, 0, 0);
        let ownership = state.get_inode_ownership((1, 2)).unwrap();
        assert_eq!(ownership.uid, 100);
        assert_eq!(ownership.gid, 7);
    }

    fn test_fake_root_state_remove_inode_ownership() {
        let mut state = FakeRootState::new();
        let ownership = FileOwnership::new(3000, 4000);
        let key = (1u64, 42u64);
        state.set_inode_ownership(key, ownership);
        assert_eq!(state.get_inode_ownership(key), Some(ownership));

        let removed = state.remove_inode_ownership(key);
        assert_eq!(removed, Some(ownership));
        assert_eq!(state.get_inode_ownership(key), None);
    }

    #[test]
    fn test_uid_gid_map_multiple_mappings() {
        let mut map = UidGidMap::default();
        map.add_uid(1000, 1001);
        map.add_uid(1002, 1003);
        map.add_gid(2000, 2001);

        assert_eq!(map.get_uid(1000), Some(1001));
        assert_eq!(map.get_uid(1002), Some(1003));
        assert_eq!(map.get_uid(9999), None);

        assert_eq!(map.get_gid(2000), Some(2001));
        assert_eq!(map.get_gid(9999), None);
    }
}
