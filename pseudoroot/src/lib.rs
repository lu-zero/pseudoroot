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
use pseudoroot_core::daemon_server::SessionDaemon;
use pseudoroot_core::shm_map::{SHM_FD_ENV, SHM_LEN_ENV, ShmInodeMap};

/// Disable memfd session backing and use Unix socket IPC instead.
pub const SESSION_SHM_ENV: &str = "PSEUDOROOT_SESSION_SHM";
use std::env;
use std::ffi::OsString;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

/// The interposed library, embedded at build time by `build.rs`.
static EMBEDDED_LIB: &[u8] = include_bytes!(env!("PSEUDOROOT_LIB_EMBED_PATH"));

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
/// short-lived session with an in-process IPC server, runs the target under
/// `LD_PRELOAD`, and tears down on exit — so separate `exec`s (`install`, `tar`, …)
/// share one inode map without manual `--daemon` or a separate `pdrd` install.
pub trait FakerootCommandExt: sealed::Sealed {
    /// Rewrite this command so that running it executes the same program under
    /// fakeroot, returning it as a plain [`std::process::Command`].
    ///
    /// Configure stdio (`.stdout`, pipes, …) on the **returned** command, not before:
    /// `Command` exposes no way to read back its stdio, so any redirection set prior
    /// to this call cannot be carried over.
    #[must_use = "fakeroot() builds a new Command; running the original executes unwrapped"]
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
/// under a private in-process daemon and exits with that command's status, never returning.
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

/// Locate the interposed shared library, extracting the embedded copy to a
/// cache directory on first use.
#[must_use]
pub fn library_path() -> Option<PathBuf> {
    if let Ok(path) = env::var(LIB_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    match extract_embedded_lib() {
        Ok(path) => Some(path),
        Err(err) => {
            eprintln!(
                "pseudoroot: failed to extract embedded library: {err} \
                 (set {LIB_PATH_ENV} to override)"
            );
            None
        }
    }
}

/// Root directory for cached extracted assets.
///
/// Always the Linux/XDG-style path, regardless of platform: `$XDG_CACHE_HOME`
/// if set, else `$HOME/.cache`, else `std::env::temp_dir()` as a last resort.
/// Preferring the user's cache dir over `/tmp` matters beyond tidiness: `/tmp`
/// is frequently mounted `noexec` (hardened distros, containers), which blocks
/// `mmap(PROT_EXEC)` there — exactly what `dlopen()`/`LD_PRELOAD` needs.
/// `~/.cache` is essentially never `noexec`.
fn cache_root() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".cache");
    }
    env::temp_dir()
}

/// Extract [`EMBEDDED_LIB`] to a content-hash-keyed cache path, skipping the
/// write if it's already there.
fn extract_embedded_lib() -> io::Result<PathBuf> {
    let lib_name = if cfg!(target_os = "macos") {
        "libpseudoroot_lib.dylib"
    } else {
        "libpseudoroot_lib.so"
    };

    let mut hasher = DefaultHasher::new();
    EMBEDDED_LIB.hash(&mut hasher);
    let hash = hasher.finish();

    let mut dir = cache_root().join("pseudoroot").join("embed");
    // /tmp is shared/1777; ~/.cache is already private, so only the temp_dir
    // fallback needs a uid segment to avoid cross-user permission contention.
    if dir.starts_with(env::temp_dir()) {
        dir = dir.join(format!("uid-{}", unsafe { libc::getuid() }));
    }
    let dir = dir.join(format!("{hash:016x}"));
    let path = dir.join(lib_name);

    if path.exists() {
        return Ok(path);
    }

    std::fs::create_dir_all(&dir)?;
    let tmp_path = dir.join(format!(".{lib_name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp_path, EMBEDDED_LIB)?;
    std::fs::rename(&tmp_path, &path)?; // atomic on POSIX
    Ok(path)
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
    let uid = env_u32("PSEUDOROOT_UID", 0);
    let gid = env_u32("PSEUDOROOT_GID", 0);

    let socket_dir =
        TempDir::new().map_err(|err| format!("failed to create session dir: {err}"))?;

    let use_shm = session_shm_enabled();
    let shm = if use_shm {
        Some(
            ShmInodeMap::create(1 << 16, uid, gid)
                .map_err(|err| format!("failed to create session shm map: {err}"))?,
        )
    } else {
        None
    };
    let daemon = if shm.is_none() {
        let socket_path = socket_dir.path().join("pseudoroot.sock");
        Some(
            SessionDaemon::start(&socket_path, uid, gid)
                .map_err(|err| format!("failed to start session daemon: {err}"))?,
        )
    } else {
        None
    };

    let mut cmd = Command::new(program);
    cmd.args(&target_args[1..]);
    // The rest of the environment is inherited; the marker must be *removed*,
    // not merely left out of overrides, or a pseudoroot-linked target would
    // re-enter supervision on its own argv.
    cmd.env_remove(SUPERVISE_VAR);
    if let Some(shm) = &shm {
        // fd inheritance across exec is plain POSIX: clear CLOEXEC on the shm
        // descriptor and hand the child its number plus the map length. Works
        // the same on Linux (memfd) and macOS (shm_open'd object).
        let fd = shm.inherited_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags >= 0 {
            let _ = unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) };
        }
        cmd.env(SHM_FD_ENV, fd.to_string());
        cmd.env(SHM_LEN_ENV, shm.map_len().to_string());
    } else if let Some(daemon) = &daemon {
        cmd.env(DAEMON_SOCKET_ENV, daemon.socket_path());
    }
    apply_preload(&mut cmd, &lib_path);

    let status = cmd
        .status()
        .map_err(|err| format!("failed to execute supervised command: {err}"))?;

    Ok(status)
}

#[inline]
fn session_shm_enabled() -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        match env::var(SESSION_SHM_ENV) {
            Ok(value) => value != "0" && !value.eq_ignore_ascii_case("false"),
            Err(_) => true,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
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
    // A value in the calling process's environment is inherited by the child;
    // defaulting over it would clobber it (same lookup order as the preload
    // merge above).
    if !command_has_env(cmd, key) && env::var_os(key).is_none() {
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
