//! Core types and state management for pseudoroot
//!
//! This crate provides the shared data structures and state management
//! for the pseudoroot library interposition system.

pub mod state;
pub mod protocol;

pub use state::{FakeRootState, FileOwnership, UidGidMap};
