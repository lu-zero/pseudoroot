//! Core types and state management for pseudoroot
//!
//! This crate provides the shared data structures and state management
//! for the pseudoroot library interposition system.

pub mod state;
pub mod protocol;

pub use state::{FakeRootState, FileOwnership, UidGidMap};

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
        let mut state = FakeRootState::new();
        state.set_current(1234, 5678);
        assert_eq!(state.current_uid(), 1234);
        assert_eq!(state.current_gid(), 5678);
    }

    #[test]
    fn test_fake_root_state_ownership() {
        let mut state = FakeRootState::new();
        let ownership = FileOwnership::new(3000, 4000);
        state.set_ownership("/tmp/test".to_string(), ownership);
        assert_eq!(state.get_ownership("/tmp/test"), Some(ownership));
    }

    #[test]
    fn test_fake_root_state_remove_ownership() {
        let mut state = FakeRootState::new();
        let ownership = FileOwnership::new(3000, 4000);
        state.set_ownership("/tmp/test".to_string(), ownership);
        assert_eq!(state.get_ownership("/tmp/test"), Some(ownership));
        
        let removed = state.remove_ownership("/tmp/test");
        assert_eq!(removed, Some(ownership));
        assert_eq!(state.get_ownership("/tmp/test"), None);
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
