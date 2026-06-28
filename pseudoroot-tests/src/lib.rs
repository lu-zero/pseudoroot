//! Test utilities for pseudoroot integration tests.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Workspace root (`pseudoroot-tests` lives one level below it).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("pseudoroot-tests should live directly under the workspace root")
        .to_path_buf()
}

fn target_artifact(name: &str) -> Option<PathBuf> {
    let root = workspace_root();
    for profile in ["debug", "release"] {
        let path = root.join("target").join(profile).join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Find the path to the pseudoroot binary.
pub fn find_pseudoroot_bin() -> PathBuf {
    target_artifact("pseudoroot").unwrap_or_else(|| {
        panic!(
            "Could not find pseudoroot binary under {}/target/{{debug,release}}/pseudoroot. \
             Run `cargo build -p pseudoroot` first.",
            workspace_root().display()
        );
    })
}

/// Find the path to the interposed shared library.
pub fn find_pseudoroot_lib() -> PathBuf {
    let lib_name = if cfg!(target_os = "macos") {
        "libpseudoroot_lib.dylib"
    } else {
        "libpseudoroot_lib.so"
    };

    target_artifact(lib_name).unwrap_or_else(|| {
        panic!(
            "Could not find {} under {}/target/{{debug,release}}/. \
             Run `cargo build -p pseudoroot-lib` first.",
            lib_name,
            workspace_root().display()
        );
    })
}

/// Run a command through pseudoroot with the given UID and GID.
pub fn run_pseudoroot_command(command: &[&str], uid: u32, gid: u32) -> Output {
    let mut cmd = Command::new(find_pseudoroot_bin());
    cmd.arg("run")
        .arg("--uid")
        .arg(uid.to_string())
        .arg("--gid")
        .arg(gid.to_string())
        .args(command);

    cmd.output().expect("Failed to run pseudoroot command")
}

/// Create a temporary file for testing.
pub fn create_test_file(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref().to_path_buf();
    std::fs::write(&path, "test content").expect("Failed to create test file");
    path
}

/// Clean up a test file.
pub fn cleanup_test_file(path: impl AsRef<Path>) {
    let _ = std::fs::remove_file(path);
}
