//! pseudoroot - CLI for running commands with fake root privileges
//!
//! This binary provides a command-line interface for running commands with
//! fake root privileges using the pseudoroot library interposition system.
//!
//! # Usage
//!
//! ```bash
//! pseudoroot <command> [args...]
//! ```
//!
//! This sets the appropriate environment variable (LD_PRELOAD on Linux,
//! DYLD_INSERT_LIBRARIES on macOS) and executes the given command.

use clap::{Parser, Subcommand};
use pseudoroot_core::protocol::DEFAULT_SOCKET_PATH;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process;

/// pseudoroot - Run commands with fake root privileges
#[derive(Parser, Debug)]
#[command(name = "pseudoroot")]
#[command(author = "Luca Barbato <lu_zero@gentoo.org>")]
#[command(version = "0.1.0")]
#[command(about = "Run commands with fake root privileges", long_about = None)]
#[command(subcommand_negates_reqs = true)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a command with fake root privileges
    Run {
        /// The command to run with fake root privileges
        #[arg(allow_hyphen_values = true)]
        command: Vec<String>,

        /// Use daemon mode for persistent state across multiple processes
        #[arg(long)]
        daemon: bool,

        /// Daemon socket path (default: /tmp/pseudoroot.sock)
        #[arg(long)]
        socket_path: Option<String>,

        /// Fake UID to use (default: 0 = root)
        #[arg(long, default_value = "0")]
        uid: u32,

        /// Fake GID to use (default: 0 = root)
        #[arg(long, default_value = "0")]
        gid: u32,
    },

    /// Start the pseudoroot daemon for persistent state
    Start {
        /// Daemon socket path (default: /tmp/pseudoroot.sock)
        #[arg(short, long)]
        socket_path: Option<String>,

        /// Initial fake UID (default: 0)
        #[arg(long, default_value = "0")]
        uid: u32,

        /// Initial fake GID (default: 0)
        #[arg(long, default_value = "0")]
        gid: u32,

        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,

        /// Clean up socket file on exit
        #[arg(long)]
        cleanup: bool,
    },

    /// Stop the pseudoroot daemon
    Stop {
        /// Daemon socket path (default: /tmp/pseudoroot.sock)
        #[arg(short, long)]
        socket_path: Option<String>,
    },

    /// Check if the pseudoroot daemon is running
    Status {
        /// Daemon socket path (default: /tmp/pseudoroot.sock)
        #[arg(short, long)]
        socket_path: Option<String>,
    },

    /// Print the library path and exit
    PrintLibraryPath,
}

fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            command: cmd_args,
            daemon,
            socket_path,
            uid,
            gid,
        } => {
            // Handle running a command
            if cmd_args.is_empty() {
                eprintln!("Error: No command specified.");
                eprintln!("Usage: pseudoroot run <command> [args...]");
                eprintln!("Try 'pseudoroot --help' for more information.");
                process::exit(1);
            }

            // Find the library path
            let lib_path = match find_library_path() {
                Some(path) => path,
                None => {
                    eprintln!("Error: Could not find pseudoroot library.");
                    eprintln!(
                        "The library needs to be built first with: cargo build -p pseudoroot-lib"
                    );
                    process::exit(1);
                }
            };

            // Set the appropriate environment variable based on the platform
            let env_var_name = if cfg!(target_os = "linux") {
                "LD_PRELOAD"
            } else if cfg!(target_os = "macos") {
                "DYLD_INSERT_LIBRARIES"
            } else {
                eprintln!("Error: Unsupported platform.");
                eprintln!("pseudoroot currently supports Linux and macOS only.");
                process::exit(1);
            };

            // Set the fake UID and GID environment variables for the library to read
            let mut command = process::Command::new(&cmd_args[0]);
            command.args(&cmd_args[1..]);
            command.env("PSEUDOROOT_UID", uid.to_string());
            command.env("PSEUDOROOT_GID", gid.to_string());

            // If daemon mode is enabled, set the daemon socket path
            if daemon {
                let socket = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
                command.env("PSEUDOROOT_DAEMON_SOCKET", socket);
            }

            // Check if the preload environment variable already contains other libraries
            let env_var_value = if let Some(existing) = env::var_os(env_var_name) {
                // Append our library to the existing value
                // On Linux, LD_PRELOAD uses colon-separated list
                // On macOS, DYLD_INSERT_LIBRARIES also uses colon-separated list
                let mut value = existing;
                value.push(":");
                value.push(lib_path);
                value
            } else {
                // Just set our library
                OsString::from(lib_path)
            };

            command.env(env_var_name, env_var_value);

            // Inherit stdin, stdout, stderr from the parent process
            command.stdin(process::Stdio::inherit());
            command.stdout(process::Stdio::inherit());
            command.stderr(process::Stdio::inherit());

            // Execute the command
            let status = match command.status() {
                Ok(status) => status,
                Err(e) => {
                    eprintln!("Error: Failed to execute command: {}", e);
                    process::exit(1);
                }
            };

            // Exit with the command's exit status
            let exit_code = status.code().unwrap_or(1);
            process::exit(exit_code);
        }

        Commands::Start {
            socket_path,
            uid,
            gid,
            verbose,
            cleanup,
        } => {
            start_daemon(socket_path, uid, gid, verbose, cleanup);
        }

        Commands::Stop { socket_path } => {
            stop_daemon(socket_path);
        }

        Commands::Status { socket_path } => {
            check_daemon_status(socket_path);
        }

        Commands::PrintLibraryPath => {
            let lib_path = find_library_path();
            match lib_path {
                Some(path) => {
                    println!("{}", path.display());
                    process::exit(0);
                }
                None => {
                    eprintln!("Error: Could not find pseudoroot library.");
                    eprintln!("The library needs to be built first with: cargo build -p pseudoroot-lib --release");
                    process::exit(1);
                }
            }
        }
    }
}

/// Start the pseudoroot daemon
fn start_daemon(socket_path: Option<String>, uid: u32, gid: u32, verbose: bool, cleanup: bool) {
    let daemon_bin = match find_daemon_path() {
        Some(path) => path,
        None => {
            eprintln!("Error: Could not find pseudoroot-daemon binary.");
            eprintln!("Build it first with: cargo build -p pseudoroot-daemon");
            process::exit(1);
        }
    };

    let socket = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
    let mut command = process::Command::new(daemon_bin);
    command
        .arg("--socket-path")
        .arg(socket)
        .arg("--uid")
        .arg(uid.to_string())
        .arg("--gid")
        .arg(gid.to_string());

    if verbose {
        command.arg("--verbose");
    }
    if cleanup {
        command.arg("--cleanup");
    }

    command.stdin(process::Stdio::inherit());
    command.stdout(process::Stdio::inherit());
    command.stderr(process::Stdio::inherit());

    let status = match command.status() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error: Failed to start daemon: {}", e);
            process::exit(1);
        }
    };

    process::exit(status.code().unwrap_or(1));
}

/// Stop the pseudoroot daemon by sending a shutdown signal
fn stop_daemon(socket_path: Option<String>) {
    let socket_path = PathBuf::from(socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string()));

    // Try to connect to the daemon and send a shutdown message
    // For now, we just try to remove the socket file
    match fs::remove_file(&socket_path) {
        Ok(_) => {
            println!("Daemon socket removed: {}", socket_path.display());
        }
        Err(e) => {
            eprintln!("Error: Failed to stop daemon: {}", e);
            process::exit(1);
        }
    }
}

/// Check if the daemon is running
fn check_daemon_status(socket_path: Option<String>) {
    let socket_path = PathBuf::from(socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string()));

    // Try to connect to the daemon socket
    match UnixStream::connect(&socket_path) {
        Ok(_) => {
            println!("Daemon is running on {}", socket_path.display());
        }
        Err(_) => {
            println!("Daemon is not running");
            process::exit(1);
        }
    }
}

/// Find the path to the pseudoroot-daemon binary
fn find_daemon_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        let exe_dir = exe_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/"));
        candidates.push(exe_dir.join("pseudoroot-daemon"));
        candidates.push(exe_dir.join("../pseudoroot-daemon"));
    }

    candidates.extend(
        [
            "target/debug/pseudoroot-daemon",
            "target/release/pseudoroot-daemon",
        ]
        .iter()
        .map(PathBuf::from),
    );

    candidates.into_iter().find(|candidate| candidate.exists())
}

/// Find the path to the pseudoroot library
///
/// This tries several locations:
/// 1. The build directory (target/debug or target/release)
/// 2. Standard library paths
fn find_library_path() -> Option<std::path::PathBuf> {
    // Try to find the library in the build directory
    // First, try relative to the current executable's location
    let mut candidates = Vec::new();

    if let Ok(exe_path) = std::env::current_exe() {
        let exe_dir = exe_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/"));
        candidates.push(exe_dir.join("../libpseudoroot_lib.so"));
        candidates.push(exe_dir.join("../libpseudoroot_lib.dylib"));
        candidates.push(exe_dir.join("libpseudoroot_lib.so"));
        candidates.push(exe_dir.join("libpseudoroot_lib.dylib"));
    }

    // Also try standard cargo build locations from the current working directory
    candidates.extend(
        [
            // Built with cargo-c
            "target/cbuild/release/libpseudoroot_lib.so",
            "target/cbuild/debug/libpseudoroot_lib.so",
            // Built with cargo - debug
            "target/debug/libpseudoroot_lib.so",
            "target/debug/libpseudoroot_lib.dylib",
            // Built with cargo - release
            "target/release/libpseudoroot_lib.so",
            "target/release/libpseudoroot_lib.dylib",
            // Also try with hyphen (older cargo versions)
            "target/debug/libpseudoroot-lib.so",
            "target/release/libpseudoroot-lib.so",
        ]
        .iter()
        .map(std::path::PathBuf::from),
    );

    candidates.into_iter().find(|candidate| candidate.exists())
}
