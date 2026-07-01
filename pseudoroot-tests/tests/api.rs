//! API-first tests — same call pattern as fakeroost (`init` + `.fakeroot()`).

use pseudoroot::FakerootCommandExt;
use pseudoroot_tests::{find_pdrd_bin, find_pseudoroot_lib};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn with_library<F: FnOnce()>(f: F) {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let lib = find_pseudoroot_lib();
    let daemon = find_pdrd_bin();
    // SAFETY: test mutex serializes env mutation across parallel test runs.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
        std::env::set_var("PSEUDOROOT_DAEMON_BIN", &daemon);
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
        let out = Command::new("id")
            .args(["-u"])
            .fakeroot()
            .output()
            .expect("id should run");
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    });
}

#[test]
fn fakeroot_extension_respects_uid_env() {
    with_library(|| {
        pseudoroot::init();
        let out = Command::new("id")
            .args(["-u"])
            .env("PSEUDOROOT_UID", "4242")
            .env("PSEUDOROOT_GID", "4242")
            .fakeroot()
            .output()
            .expect("id should run");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4242");
    });
}

#[test]
fn library_path_env_override() {
    let lib = find_pseudoroot_lib();
    with_library(|| {
        assert_eq!(pseudoroot::library_path().as_deref(), Some(lib.as_path()));
    });
}
