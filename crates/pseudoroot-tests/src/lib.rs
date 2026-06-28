//! Test utilities for pseudoroot integration tests
//!
//! This crate provides helper functions for running integration tests
//! that verify the pseudoroot library interposition works correctly.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Find the path to the pseudoroot binary
pub fn find_pseudoroot_bin() -> PathBuf {
    // Try several candidate locations
    // The tests run from the workspace root's target directory context
    let candidates = [
        // From workspace root
        "../../../target/debug/pseudoroot",
        "../../../target/release/pseudoroot",
        // From crates/pseudoroot-tests
        "../../target/debug/pseudoroot",
        "../../target/release/pseudoroot",
        // Absolute fallback - try common paths
        "/home/lu_zero/Sources/pseudoroot/target/debug/pseudoroot",
        "/home/lu_zero/Sources/pseudoroot/target/release/pseudoroot",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return path.canonicalize().unwrap_or(path);
        }
    }

    panic!("Could not find pseudoroot binary. Run 'cargo build -p pseudoroot' first. Tried: {:?}", candidates);
}

/// Find the path to the pseudoroot library
pub fn find_pseudoroot_lib() -> PathBuf {
    let candidates = [
        // From workspace root
        "../../../target/cbuild/debug/libpseudoroot_lib.so",
        "../../../target/cbuild/release/libpseudoroot_lib.so",
        "../../../target/debug/libpseudoroot_lib.so",
        "../../../target/debug/libpseudoroot_lib.dylib",
        "../../../target/release/libpseudoroot_lib.so",
        "../../../target/release/libpseudoroot_lib.dylib",
        // From crates/pseudoroot-tests
        "../../target/cbuild/debug/libpseudoroot_lib.so",
        "../../target/cbuild/release/libpseudoroot_lib.so",
        "../../target/debug/libpseudoroot_lib.so",
        "../../target/debug/libpseudoroot_lib.dylib",
        "../../target/release/libpseudoroot_lib.so",
        "../../target/release/libpseudoroot_lib.dylib",
        // Absolute fallback
        "/home/lu_zero/Sources/pseudoroot/target/debug/libpseudoroot_lib.so",
        "/home/lu_zero/Sources/pseudoroot/target/release/libpseudoroot_lib.so",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return path.canonicalize().unwrap_or(path);
        }
    }

    panic!("Could not find pseudoroot library. Run 'cargo build -p pseudoroot-lib' first.");
}

/// Run a command through pseudoroot with the given UID and GID
pub fn run_pseudoroot_command(command: &[&str], uid: u32, gid: u32) -> Output {
    let pseudoroot_bin = find_pseudoroot_bin();

    let mut cmd = Command::new(pseudoroot_bin);
    cmd.arg("--uid").arg(uid.to_string());
    cmd.arg("--gid").arg(gid.to_string());
    cmd.args(command);

    cmd.output().expect("Failed to run pseudoroot command")
}

/// Create a temporary file for testing
pub fn create_test_file(path: &str) -> PathBuf {
    let pb = PathBuf::from(path);
    std::fs::write(&pb, "test content").expect("Failed to create test file");
    pb
}

/// Clean up a test file
pub fn cleanup_test_file(path: &str) {
    let _ = std::fs::remove_file(path);
}
