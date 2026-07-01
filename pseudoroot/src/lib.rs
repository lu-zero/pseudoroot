//! pseudoroot library — API-compatible with [`fakeroost`](https://github.com/koca-build/fakeroost).
//!
//! Swap backends by changing the import:
//!
//! ```ignore
//! use fakeroost::FakerootCommandExt;   // ptrace + seccomp
//! // use pseudoroot::FakerootCommandExt; // LD_PRELOAD
//!
//! fn main() {
//!     pseudoroot::init(); // no-op here; required for fakeroost
//!     std::process::Command::new("id").fakeroot().status().unwrap();
//! }
//! ```
//!
//! Pseudoroot-specific options (`PSEUDOROOT_UID`, `PSEUDOROOT_GID`,
//! `PSEUDOROOT_DAEMON_SOCKET`, `PSEUDOROOT_LIB`) are passed via [`Command::env`]
//! before calling [`.fakeroot()`](FakerootCommandExt::fakeroot).

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

mod sealed {
    pub trait Sealed {}
    impl Sealed for std::process::Command {}
}

/// Environment variable override for the interposed library path (tests/installs).
pub const LIB_PATH_ENV: &str = "PSEUDOROOT_LIB";

/// Adds `fakeroot`-like execution to [`std::process::Command`].
///
/// API-identical to fakeroost's trait: the returned command runs the same program
/// with library interposition enabled via `LD_PRELOAD` / `DYLD_INSERT_LIBRARIES`.
pub trait FakerootCommandExt: sealed::Sealed {
    /// Rewrite this command so that running it executes the same program under
    /// fakeroot, returning it as a plain [`std::process::Command`].
    ///
    /// Configure stdio (`.stdout`, pipes, …) on the **returned** command, not before:
    /// `Command` exposes no way to read back its stdio, so any redirection set prior
    /// to this call cannot be carried over.
    fn fakeroot(&self) -> Command;
}

impl FakerootCommandExt for Command {
    fn fakeroot(&self) -> Command {
        let lib_path = library_path().unwrap_or_else(|| {
            panic!(
                "pseudoroot: could not find libpseudoroot_lib.so — build with \
                 `cargo build -p pseudoroot-lib` or set {LIB_PATH_ENV}"
            );
        });

        let mut cmd = Command::new(self.get_program());
        cmd.args(self.get_args());
        for (key, val) in self.get_envs() {
            match val {
                Some(val) => cmd.env(key, val),
                None => cmd.env_remove(key),
            };
        }
        if let Some(dir) = self.get_current_dir() {
            cmd.current_dir(dir);
        }

        apply_preload(&mut cmd, &lib_path);
        cmd
    }
}

/// Compatibility hook — call once at the start of `main`, identical to fakeroost.
///
/// Always a no-op for pseudoroot (LD_PRELOAD does not need a supervisor re-exec).
/// Provided so consumers can swap `use fakeroost` ↔ `use pseudoroot` without
/// changing call sites.
pub fn init() {}

/// Locate the interposed shared library.
#[must_use]
pub fn library_path() -> Option<PathBuf> {
    if let Ok(path) = env::var(LIB_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("/"));
        candidates.push(exe_dir.join("../libpseudoroot_lib.so"));
        candidates.push(exe_dir.join("../libpseudoroot_lib.dylib"));
        candidates.push(exe_dir.join("libpseudoroot_lib.so"));
        candidates.push(exe_dir.join("libpseudoroot_lib.dylib"));
    }

    candidates.extend(
        [
            "target/cbuild/release/libpseudoroot_lib.so",
            "target/cbuild/debug/libpseudoroot_lib.so",
            "target/debug/libpseudoroot_lib.so",
            "target/debug/libpseudoroot_lib.dylib",
            "target/release/libpseudoroot_lib.so",
            "target/release/libpseudoroot_lib.dylib",
            "target/debug/libpseudoroot-lib.so",
            "target/release/libpseudoroot-lib.so",
        ]
        .iter()
        .map(PathBuf::from),
    );

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn apply_preload(cmd: &mut Command, lib_path: &Path) {
    let env_var_name = if cfg!(target_os = "linux") {
        "LD_PRELOAD"
    } else if cfg!(target_os = "macos") {
        "DYLD_INSERT_LIBRARIES"
    } else {
        panic!("pseudoroot: unsupported platform (Linux and macOS only)");
    };

    ensure_default_env(cmd, "PSEUDOROOT_UID", "0");
    ensure_default_env(cmd, "PSEUDOROOT_GID", "0");

    let env_var_value = if let Some(existing) = env_in_command(cmd, env_var_name) {
        let mut value = existing;
        value.push(":");
        value.push(lib_path.as_os_str());
        value
    } else if let Some(existing) = env::var_os(env_var_name) {
        let mut value = existing;
        value.push(":");
        value.push(lib_path.as_os_str());
        value
    } else {
        OsString::from(lib_path)
    };

    cmd.env(env_var_name, env_var_value);
}

fn ensure_default_env(cmd: &mut Command, key: &str, default: &str) {
    if !command_has_env(cmd, key) {
        cmd.env(key, default);
    }
}

fn command_has_env(cmd: &Command, key: &str) -> bool {
    cmd.get_envs().any(|(k, v)| k == key && v.is_some())
}

fn env_in_command(cmd: &Command, key: &str) -> Option<OsString> {
    cmd.get_envs()
        .find_map(|(k, v)| (k == key).then(|| v.map(|s| s.to_os_string())).flatten())
}
