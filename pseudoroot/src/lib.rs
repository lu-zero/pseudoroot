//! pseudoroot library — API-compatible with [`fakeroost`](https://github.com/koca-build/fakeroost).
//!
//! Swap backends by changing the import:
//!
//! ```ignore
//! use fakeroost::FakerootCommandExt;   // ptrace + seccomp
//! // use pseudoroot::FakerootCommandExt; // LD_PRELOAD
//!
//! fn main() {
//!     pseudoroot::init(); // required: handles session re-exec (no-op otherwise)
//!     std::process::Command::new("id").fakeroot().status().unwrap();
//! }
//! ```
//!
//! Pseudoroot-specific options (`PSEUDOROOT_UID`, `PSEUDOROOT_GID`,
//! `PSEUDOROOT_DAEMON_SOCKET`, `PSEUDOROOT_LIB`) are passed via [`Command::env`]
//! before calling [`.fakeroot()`](FakerootCommandExt::fakeroot).

use pseudoroot_core::daemon_client::DAEMON_SOCKET_ENV;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

mod sealed {
    pub trait Sealed {}
    impl Sealed for std::process::Command {}
}

/// Environment variable marking a process re-executed to supervise a fakeroot session.
/// Set by [`FakerootCommandExt::fakeroot`], consumed by [`init`].
const SUPERVISE_VAR: &str = "__PSEUDOROOT_SUPERVISE";

/// Opt out of session supervision and use per-process state only.
pub const STANDALONE_ENV: &str = "PSEUDOROOT_STANDALONE";

/// Environment variable override for the interposed library path (tests/installs).
pub const LIB_PATH_ENV: &str = "PSEUDOROOT_LIB";

/// Adds `fakeroot`-like execution to [`std::process::Command`].
///
/// By default each `.fakeroot()` invocation re-executes the current program in a
/// short-lived session that auto-starts `pdrd`, runs the target under `LD_PRELOAD`,
/// and tears the daemon down on exit — so separate `exec`s (`install`, `tar`, …)
/// share one inode map without manual `--daemon`.
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
        if session_supervision_enabled(self) {
            return build_supervise_command(self);
        }

        let lib_path = library_path().unwrap_or_else(|| {
            panic!(
                "pseudoroot: could not find libpseudoroot_lib.so — build with \
                 `cargo build -p pseudoroot-lib` or set {LIB_PATH_ENV}"
            );
        });

        let mut cmd = clone_command(self);
        apply_preload(&mut cmd, &lib_path);
        cmd
    }
}

/// Become the session supervisor when this process was launched as one.
///
/// Call once at the start of `main`, identical to fakeroost. On a normal launch
/// this returns immediately. On a supervise re-exec it runs the requested command
/// under a private `pdrd` and exits with that command's status, never returning.
///
/// A `#[ctor]` hook also calls this before `main` so test binaries and other
/// consumers work without an explicit call site.
pub fn init() {
    if env::var_os(SUPERVISE_VAR).is_none() {
        return;
    }

    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let code = match run_session(&args) {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("pseudoroot: {err}");
            1
        }
    };
    std::process::exit(code);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[ctor::ctor]
fn supervise_ctor() {
    init();
}

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

/// Locate the `pdrd` / `pseudoroot-daemon` binary.
#[must_use]
pub fn daemon_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("PSEUDOROOT_DAEMON_BIN") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(exe_path) = env::current_exe() {
        let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("/"));
        for name in ["pdrd", "pseudoroot-daemon"] {
            candidates.push(exe_dir.join(name));
            candidates.push(exe_dir.join("..").join(name));
        }
    }

    for profile in ["debug", "release"] {
        for name in ["pdrd", "pseudoroot-daemon"] {
            candidates.push(PathBuf::from("target").join(profile).join(name));
        }
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn session_supervision_enabled(cmd: &Command) -> bool {
    if env::var_os(SUPERVISE_VAR).is_some() {
        return false;
    }
    if command_has_env(cmd, DAEMON_SOCKET_ENV) || env::var(DAEMON_SOCKET_ENV).is_ok() {
        return false;
    }
    if command_has_env(cmd, STANDALONE_ENV) || env::var(STANDALONE_ENV).is_ok() {
        return false;
    }
    true
}

fn build_supervise_command(base: &Command) -> Command {
    let exe = supervisor_exe().unwrap_or_else(|err| {
        panic!("pseudoroot: could not resolve supervisor executable: {err}");
    });

    let mut cmd = Command::new(exe);
    cmd.env(SUPERVISE_VAR, "1");
    cmd.arg(base.get_program());
    cmd.args(base.get_args());
    for (key, val) in base.get_envs() {
        match val {
            Some(val) => cmd.env(key, val),
            None => cmd.env_remove(key),
        };
    }
    if let Some(dir) = base.get_current_dir() {
        cmd.current_dir(dir);
    }
    cmd
}

fn run_session(target_args: &[OsString]) -> Result<ExitStatus, String> {
    let program = target_args
        .first()
        .ok_or_else(|| "no program given to pseudoroot session".to_string())?;

    let lib_path = library_path().ok_or_else(|| {
        format!(
            "could not find libpseudoroot_lib.so — build with \
             `cargo build -p pseudoroot-lib` or set {LIB_PATH_ENV}"
        )
    })?;
    let daemon_bin = daemon_path()
        .ok_or("could not find pdrd — build with `cargo build -p pseudoroot-daemon`")?;

    let uid = env_u32("PSEUDOROOT_UID", 0);
    let gid = env_u32("PSEUDOROOT_GID", 0);

    let socket_dir =
        TempDir::new().map_err(|err| format!("failed to create session dir: {err}"))?;
    let socket_path = socket_dir.path().join("pseudoroot.sock");

    let mut daemon = spawn_session_daemon(&daemon_bin, &socket_path, uid, gid)?;
    wait_for_socket(&socket_path)?;

    let mut cmd = Command::new(program);
    cmd.args(&target_args[1..]);
    for (key, value) in env::vars_os() {
        if key != SUPERVISE_VAR {
            cmd.env(key, value);
        }
    }
    cmd.env(DAEMON_SOCKET_ENV, &socket_path);
    apply_preload(&mut cmd, &lib_path);

    let status = cmd
        .status()
        .map_err(|err| format!("failed to execute supervised command: {err}"))?;

    stop_session_daemon(&mut daemon, &socket_path);
    Ok(status)
}

fn spawn_session_daemon(
    daemon_bin: &Path,
    socket_path: &Path,
    uid: u32,
    gid: u32,
) -> Result<Child, String> {
    Command::new(daemon_bin)
        .arg("--socket-path")
        .arg(socket_path)
        .arg("--uid")
        .arg(uid.to_string())
        .arg("--gid")
        .arg(gid.to_string())
        .arg("--cleanup")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("failed to start pdrd: {err}"))
}

fn wait_for_socket(socket_path: &Path) -> Result<(), String> {
    for _ in 0..100 {
        if socket_path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(format!(
        "pdrd did not create socket at {}",
        socket_path.display()
    ))
}

fn stop_session_daemon(daemon: &mut Child, socket_path: &Path) {
    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = std::fs::remove_file(socket_path);
}

fn supervisor_exe() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(PathBuf::from("/proc/self/exe"))
    }
    #[cfg(target_os = "macos")]
    {
        env::current_exe().map_err(|err| err.to_string())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err("unsupported platform (Linux and macOS only)".into())
    }
}

fn clone_command(base: &Command) -> Command {
    let mut cmd = Command::new(base.get_program());
    cmd.args(base.get_args());
    for (key, val) in base.get_envs() {
        match val {
            Some(val) => cmd.env(key, val),
            None => cmd.env_remove(key),
        };
    }
    if let Some(dir) = base.get_current_dir() {
        cmd.current_dir(dir);
    }
    cmd
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

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_supervision_skips_when_daemon_socket_set() {
        let mut cmd = Command::new("true");
        cmd.env(DAEMON_SOCKET_ENV, "/tmp/pseudoroot.sock");
        assert!(!session_supervision_enabled(&cmd));
    }

    #[test]
    fn session_supervision_skips_when_standalone_set() {
        let mut cmd = Command::new("true");
        cmd.env(STANDALONE_ENV, "1");
        assert!(!session_supervision_enabled(&cmd));
    }
}
