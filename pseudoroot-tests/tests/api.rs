//! API-first tests — same call pattern as fakeroost (`init` + `.fakeroot()`).

use pseudoroot::FakerootCommandExt;
use pseudoroot_tests::find_pseudoroot_lib;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// Path to the freshly built `print-ids` helper (see its module docs): unlike
/// `/usr/bin/id`, this unsigned binary keeps `DYLD_INSERT_LIBRARIES` under
/// macOS SIP, so the faked credentials actually reach it.
const PRINT_IDS: &str = env!("CARGO_BIN_EXE_print-ids");

fn with_library<F: FnOnce()>(f: F) {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    // A panic in an earlier test poisons the mutex; the guard only serializes
    // env mutation, so recovering the inner `()` is safe and keeps one failure
    // from cascading into spurious failures across the rest of the suite.
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let lib = find_pseudoroot_lib();
    // SAFETY: test mutex serializes env mutation across parallel test runs.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
    }
    f();
}

#[test]
fn init_returns_without_supervise_marker() {
    pseudoroot::init();
    pseudoroot::init();
}

#[test]
fn fakeroot_extension_runs_command() {
    with_library(|| {
        pseudoroot::init();
        let out = Command::new(PRINT_IDS)
            .fakeroot()
            .output()
            .expect("print-ids should run");
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Default fake identity is root (uid 0, gid 0).
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0 0");
    });
}

#[test]
fn fakeroot_extension_respects_uid_env() {
    with_library(|| {
        pseudoroot::init();
        let out = Command::new(PRINT_IDS)
            .env("PSEUDOROOT_UID", "4242")
            .env("PSEUDOROOT_GID", "4242")
            .fakeroot()
            .output()
            .expect("print-ids should run");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4242 4242");
    });
}

#[test]
fn library_path_env_override() {
    let lib = find_pseudoroot_lib();
    with_library(|| {
        assert_eq!(pseudoroot::library_path().as_deref(), Some(lib.as_path()));
    });
}
