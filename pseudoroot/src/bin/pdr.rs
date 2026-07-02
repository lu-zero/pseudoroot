//! CLI for running commands with fake root privileges.
//!
//! Installed as `pseudoroot` (main) and `pdr` (short). Uses the same
//! [`pseudoroot::FakerootCommandExt`] API as programmatic consumers.

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use pseudoroot::FakerootCommandExt;
use pseudoroot_core::protocol::{
    next_request_id, IpcChannel, MessageType, ProtocolMessage, DEFAULT_SOCKET_PATH,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

const SUBCOMMANDS: &[&str] = &[
    "run",
    "start",
    "stop",
    "status",
    "print-library-path",
    "help",
];

/// How this binary was invoked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CliName {
    /// `pdr` — fakeroot-style: `pdr [opts] [--] <cmd> [args…]`
    Short,
    /// `pseudoroot` — explicit subcommands: `pseudoroot run …`
    Long,
}

impl CliName {
    fn detect() -> Self {
        let stem = env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_owned()));
        if stem.is_some_and(|s| s == "pdr") {
            Self::Short
        } else {
            Self::Long
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Short => "pdr",
            Self::Long => "pseudoroot",
        }
    }

    fn is_short(self) -> bool {
        matches!(self, Self::Short)
    }
}

/// Fake-identity flags shared by `pdr` (top-level) and `pseudoroot run`.
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

    /// Command to run (`pdr` only — when no subcommand is given)
    #[arg(allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a command with fake root privileges (pseudoroot)
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
    let cli = CliName::detect();
    let argv = adjust_args(cli, env::args_os().collect());
    let mut cmd = Cli::command().bin_name(cli.as_str());
    if cli.is_short() {
        cmd = cmd.after_help(
            "Examples:\n  \
             pdr id\n  \
             pdr --uid 1000 -- id -u\n  \
             pdr --daemon -- make install\n  \
             pdr start\n  \
             pdr print-library-path",
        );
    }
    let matches = cmd.get_matches_from(argv);
    let parsed = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    match parsed.action {
        Some(Commands::Run { command, run }) => {
            run_command(cli, &command, run.daemon, run.socket_path, run.uid, run.gid)
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

        None if cli.is_short() => run_command(
            cli,
            &parsed.command,
            parsed.run.daemon,
            parsed.run.socket_path,
            parsed.run.uid,
            parsed.run.gid,
        ),

        None => {
            eprintln!("Error: No subcommand specified.");
            eprintln!("Usage: {} run <command> [args...]", cli.as_str());
            eprintln!("Try '{} --help' for more information.", cli.as_str());
            process::exit(1);
        }
    }
}

/// For `pdr`, insert the implicit `run` subcommand when the first arg is a
/// bare command name (not a flag or known subcommand).
fn adjust_args(cli: CliName, mut argv: Vec<OsString>) -> Vec<OsString> {
    if !cli.is_short() || argv.len() < 2 {
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
    cli: CliName,
    cmd_args: &[String],
    daemon: bool,
    socket_path: Option<String>,
    uid: u32,
    gid: u32,
) -> ! {
    if cmd_args.is_empty() {
        eprintln!("Error: No command specified.");
        eprintln!("Usage: {} [OPTIONS] [--] <command> [args...]", cli.as_str());
        eprintln!("Try '{} --help' for more information.", cli.as_str());
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
            eprintln!("Error: Could not find pseudoroot library.");
            eprintln!("Build it first with: cargo build -p pseudoroot-lib --release");
            process::exit(1);
        }
    }
}

/// Start the pseudoroot daemon (`pdrd` / `pseudoroot-daemon`).
fn start_daemon(
    socket_path: Option<String>,
    uid: u32,
    gid: u32,
    verbose: bool,
    cleanup: bool,
) -> ! {
    let daemon_bin = match find_daemon_path() {
        Some(path) => path,
        None => {
            eprintln!("Error: Could not find pdrd/pseudoroot-daemon binary.");
            eprintln!("Build it first with: cargo build -p pseudoroot-daemon");
            process::exit(1);
        }
    };

    let socket = socket_path.unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string());
    let mut command = Command::new(daemon_bin);
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

    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let status = match command.status() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error: Failed to start daemon: {}", e);
            process::exit(1);
        }
    };

    process::exit(status.code().unwrap_or(1));
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

fn find_daemon_path() -> Option<PathBuf> {
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
