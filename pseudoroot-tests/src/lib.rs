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

/// Find the CLI binary (`pdr`).
pub fn find_pdr_bin() -> PathBuf {
    target_artifact("pdr").unwrap_or_else(|| {
        panic!(
            "Could not find pdr binary under {}/target/{{debug,release}}/. \
             Run `cargo build -p pseudoroot` first.",
            workspace_root().display()
        )
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

/// Run a command through the API with the given UID and GID (fakeroost-compatible).
pub fn run_pseudoroot_command(command: &[&str], uid: u32, gid: u32) -> Output {
    use pseudoroot::FakerootCommandExt;

    let lib = find_pseudoroot_lib();
    // SAFETY: integration tests run sequentially enough for env setup; parallel
    // API tests use their own mutex.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
    }

    pseudoroot::init();
    let program = command.first().expect("command must not be empty");
    Command::new(program)
        .args(&command[1..])
        .env("PSEUDOROOT_UID", uid.to_string())
        .env("PSEUDOROOT_GID", gid.to_string())
        .fakeroot()
        .output()
        .expect("Failed to run pseudoroot command")
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

/// Resolve the shell used to run `-c` scripts.
///
/// Honours `$SHELL` so macOS CI can point this at a non-SIP Homebrew bash —
/// System Integrity Protection strips `DYLD_INSERT_LIBRARIES` from the
/// Apple-signed `/bin/sh` and `/bin/bash`, making interposed runs flake.
/// Falls back to `sh` everywhere else.
fn shell() -> String {
    env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

/// Run `"$SHELL" -c <script>` under pseudoroot with the given UID and GID.
pub fn run_pseudoroot_script(script: &str, uid: u32, gid: u32) -> Output {
    let sh = shell();
    run_pseudoroot_command(&[sh.as_str(), "-c", script], uid, gid)
}

/// Run `"$SHELL" -c <script>` under pseudoroot in `dir`.
///
/// Session supervision starts a private in-process session (SHM-backed map,
/// or an in-process daemon thread if SHM is unavailable) for the script so
/// inode state survives across separate `exec` calls (`touch`, `chown`,
/// `stat`, …) without needing a separate `pdrd` process.
pub fn run_pseudoroot_sh(dir: &Path, script: &str) -> Output {
    use pseudoroot::FakerootCommandExt;

    let lib = find_pseudoroot_lib();
    // SAFETY: test processes are separate; env is per-invocation.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
    }

    pseudoroot::init();
    Command::new(shell())
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .env("PSEUDOROOT_UID", "0")
        .env("PSEUDOROOT_GID", "0")
        .fakeroot()
        .output()
        .expect("Failed to run pseudoroot shell script")
}
