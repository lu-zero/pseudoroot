//! CLI for running commands with fake root privileges.
//!
//! Uses the same [`pseudoroot::FakerootCommandExt`] API as programmatic
//! consumers. `pdr start` runs the session daemon in-process (via
//! [`pseudoroot_core::daemon_server::run_blocking`]) — no separate `pdrd`
//! binary is required for `pdr`'s own functionality.

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use pseudoroot::FakerootCommandExt;
use pseudoroot_core::protocol::{
    next_request_id, IpcChannel, MessageType, ProtocolMessage, DEFAULT_SOCKET_PATH,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};

const SUBCOMMANDS: &[&str] = &[
    "run",
    "start",
    "stop",
    "status",
    "print-library-path",
    "help",
];

/// Fake-identity flags shared by the top-level form and `pdr run`.
#[derive(Args, Debug)]
struct RunArgs {
    /// Fake UID to use when running a command (default: 0 = root)
    #[arg(long, default_value = "0")]
    uid: u32,

    /// Fake GID to use when running a command (default: 0 = root)
    #[arg(long, default_value = "0")]
    gid: u32,

    /// Attach to an existing pdrd instead of starting a per-invocation session
    #[arg(long)]
    daemon: bool,

    /// Daemon socket path (default: /tmp/pseudoroot.sock)
    #[arg(long)]
    socket_path: Option<String>,
}

/// Run commands with fake root privileges.
#[derive(Parser, Debug)]
#[command(author = "Luca Barbato <lu_zero@gentoo.org>")]
#[command(version = "0.1.0")]
#[command(about = "Run commands with fake root privileges", long_about = None)]
#[command(subcommand_negates_reqs = true)]
struct Cli {
    #[command(flatten)]
    run: RunArgs,

    #[command(subcommand)]
    action: Option<Commands>,

    /// Command to run (when no subcommand is given)
    #[arg(allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a command with fake root privileges
    Run {
        /// The command to run with fake root privileges
        #[arg(allow_hyphen_values = true)]
        command: Vec<String>,

        #[command(flatten)]
        run: RunArgs,
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
    pseudoroot::init();
    let argv = adjust_args(env::args_os().collect());
    let cmd = Cli::command().bin_name("pdr").after_help(
        "Examples:\n  \
         pdr id\n  \
         pdr --uid 1000 -- id -u\n  \
         pdr --daemon -- make install\n  \
         pdr start\n  \
         pdr print-library-path",
    );
    let matches = cmd.get_matches_from(argv);
    let parsed = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    match parsed.action {
        Some(Commands::Run { command, run }) => {
            run_command(&command, run.daemon, run.socket_path, run.uid, run.gid)
        }

        Some(Commands::Start {
            socket_path,
            uid,
            gid,
            verbose,
            cleanup,
        }) => start_daemon(socket_path, uid, gid, verbose, cleanup),

        Some(Commands::Stop { socket_path }) => stop_daemon(socket_path),

        Some(Commands::Status { socket_path }) => check_daemon_status(socket_path),

        Some(Commands::PrintLibraryPath) => print_library_path(),

        None => run_command(
            &parsed.command,
            parsed.run.daemon,
            parsed.run.socket_path,
            parsed.run.uid,
            parsed.run.gid,
        ),
    }
}

/// Insert the implicit `run` subcommand when the first arg is a bare command
/// name (not a flag or known subcommand).
fn adjust_args(mut argv: Vec<OsString>) -> Vec<OsString> {
    if argv.len() < 2 {
        return argv;
    }

    let first = argv[1].to_string_lossy();
    if first == "run" || SUBCOMMANDS.contains(&first.as_ref()) || first.starts_with('-') {
        return argv;
    }

    argv.insert(1, OsString::from("run"));
    argv
}

fn run_command(
    cmd_args: &[String],
    daemon: bool,
    socket_path: Option<String>,
    uid: u32,
    gid: u32,
) -> ! {
    if cmd_args.is_empty() {
        eprintln!("Error: No command specified.");
        eprintln!("Usage: pdr [OPTIONS] [--] <command> [args...]");
        eprintln!("Try 'pdr --help' for more information.");
        process::exit(1);
    }

    let mut base = Command::new(&cmd_args[0]);
    base.args(&cmd_args[1..]);
    base.env("PSEUDOROOT_UID", uid.to_string());
    base.env("PSEUDOROOT_GID", gid.to_string());
    if daemon {
        let socket = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
        base.env("PSEUDOROOT_DAEMON_SOCKET", socket);
    }

    let mut command = base.fakeroot();
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let status = match command.status() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error: Failed to execute command: {}", e);
            process::exit(1);
        }
    };

    process::exit(status.code().unwrap_or(1));
}

fn print_library_path() -> ! {
    match pseudoroot::library_path() {
        Some(path) => {
            println!("{}", path.display());
            process::exit(0);
        }
        None => {
            eprintln!("Error: Could not extract the pseudoroot library.");
            eprintln!("Set PSEUDOROOT_LIB to override the library path.");
            process::exit(1);
        }
    }
}

/// Start the pseudoroot session daemon in-process (no separate `pdrd` needed).
fn start_daemon(
    socket_path: Option<String>,
    uid: u32,
    gid: u32,
    verbose: bool,
    cleanup: bool,
) -> ! {
    let socket_path = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
    println!("pdr: Listening on {socket_path}");
    println!("pdr: Initial UID={uid}, GID={gid}");
    println!("Press Ctrl+C to stop");

    let sp = socket_path.clone();
    if let Err(err) = ctrlc::set_handler(move || {
        println!("\npdr: Shutting down...");
        if cleanup {
            let _ = std::fs::remove_file(&sp);
        }
        process::exit(0);
    }) {
        eprintln!("Error: Failed to set Ctrl+C handler: {err}");
        process::exit(1);
    }

    match pseudoroot_core::daemon_server::run_blocking(&socket_path, uid, gid, verbose, cleanup) {
        Ok(()) => process::exit(0),
        Err(err) => {
            eprintln!("Error: {err}");
            process::exit(1);
        }
    }
}

fn stop_daemon(socket_path: Option<String>) -> ! {
    let socket_path = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
    let mut channel = IpcChannel::new(socket_path.clone());

    if channel.connect().is_err() {
        // Nothing is listening; at most a stale socket file is left behind.
        match fs::remove_file(&socket_path) {
            Ok(_) => println!("No daemon running; removed stale socket file: {socket_path}"),
            Err(_) => println!("Daemon is not running"),
        }
        process::exit(0);
    }

    let request = ProtocolMessage::new(MessageType::Shutdown, vec![], next_request_id());
    match channel.request(request) {
        Ok(_) => {
            println!("Daemon stopped: {socket_path}");
            process::exit(0);
        }
        Err(e) => {
            eprintln!("Error: Failed to stop daemon: {e}");
            process::exit(1);
        }
    }
}

fn check_daemon_status(socket_path: Option<String>) -> ! {
    let socket_path = PathBuf::from(socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string()));

    match UnixStream::connect(&socket_path) {
        Ok(_) => {
            println!("Daemon is running on {}", socket_path.display());
            process::exit(0);
        }
        Err(_) => {
            println!("Daemon is not running");
            process::exit(1);
        }
    }
}
