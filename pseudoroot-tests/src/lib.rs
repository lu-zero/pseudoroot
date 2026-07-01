//! Test utilities for pseudoroot integration tests.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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

/// Find the main CLI binary (`pseudoroot`, falling back to `pdr`).
pub fn find_pseudoroot_bin() -> PathBuf {
    for name in ["pseudoroot", "pdr"] {
        if let Some(path) = target_artifact(name) {
            return path;
        }
    }
    panic!(
        "Could not find pseudoroot/pdr binary under {}/target/{{debug,release}}/. \
         Run `cargo build -p pseudoroot` first.",
        workspace_root().display()
    );
}

/// Find the short CLI binary (`pdr`, falling back to `pseudoroot`).
pub fn find_pdr_bin() -> PathBuf {
    for name in ["pdr", "pseudoroot"] {
        if let Some(path) = target_artifact(name) {
            return path;
        }
    }
    panic!(
        "Could not find pdr/pseudoroot binary under {}/target/{{debug,release}}/. \
         Run `cargo build -p pseudoroot` first.",
        workspace_root().display()
    );
}

/// Find the daemon binary (`pdrd`, falling back to `pseudoroot-daemon`).
pub fn find_pdrd_bin() -> PathBuf {
    for name in ["pdrd", "pseudoroot-daemon"] {
        if let Some(path) = target_artifact(name) {
            return path;
        }
    }
    panic!(
        "Could not find pdrd/pseudoroot-daemon under {}/target/{{debug,release}}/. \
         Run `cargo build -p pseudoroot-daemon` first.",
        workspace_root().display()
    );
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

/// Whether `pseudoroot` needs an explicit `run` subcommand (unlike `pdr`).
fn needs_run_subcommand(bin: &Path) -> bool {
    bin.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name == "pseudoroot")
}

/// Start building a command through the CLI, inserting `run` when required.
pub fn command_for_cli_run(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    if needs_run_subcommand(bin) {
        cmd.arg("run");
    }
    cmd
}

struct TestDaemon {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    _child: Child,
}

fn shared_test_daemon() -> &'static Mutex<TestDaemon> {
    static DAEMON: OnceLock<Mutex<TestDaemon>> = OnceLock::new();
    DAEMON.get_or_init(|| {
        let dir = tempfile::TempDir::new().expect("tempdir for test daemon");
        let socket = dir.path().join("pseudoroot.sock");
        let daemon_bin = find_pdrd_bin();
        let mut child = Command::new(&daemon_bin)
            .arg("-s")
            .arg(&socket)
            .spawn()
            .expect("Failed to start pdrd for tests");

        for _ in 0..100 {
            if socket.exists() {
                return Mutex::new(TestDaemon {
                    _dir: dir,
                    socket,
                    _child: child,
                });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("pdrd did not create socket at {}", socket.display());
    })
}

/// Socket path for the shared per-test-suite daemon.
pub fn test_daemon_socket() -> PathBuf {
    shared_test_daemon().lock().unwrap().socket.clone()
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

/// Run `sh -c <script>` under pseudoroot in `dir`.
///
/// Uses a shared `pdrd` instance so inode state survives across separate
/// `exec` calls (`touch`, `chown`, `stat`, …) within the script.
pub fn run_pseudoroot_sh(dir: &Path, script: &str) -> Output {
    use pseudoroot::FakerootCommandExt;

    let lib = find_pseudoroot_lib();
    let socket = test_daemon_socket();
    // SAFETY: test processes are separate; env is per-invocation.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
    }

    pseudoroot::init();
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .env("PSEUDOROOT_UID", "0")
        .env("PSEUDOROOT_GID", "0")
        .env("PSEUDOROOT_DAEMON_SOCKET", &socket)
        .fakeroot()
        .output()
        .expect("Failed to run pseudoroot shell script")
}
