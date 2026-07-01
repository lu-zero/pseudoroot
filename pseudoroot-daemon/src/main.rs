//! pseudoroot-daemon - Daemon process for persistent fake root state

use clap::{CommandFactory, FromArgMatches, Parser};
use pseudoroot_core::daemon_server;
use pseudoroot_core::protocol::DEFAULT_SOCKET_PATH;
use std::env;
use std::path::PathBuf;

/// Installed as `pseudoroot-daemon` (main) and `pdrd` (short).
fn program_name() -> &'static str {
    if env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_owned()))
        .is_some_and(|s| s == "pdrd")
    {
        "pdrd"
    } else {
        "pseudoroot-daemon"
    }
}

/// Configuration for the daemon
#[derive(Parser, Debug)]
#[command(author = "Luca Barbato <lu_zero@gentoo.org>")]
#[command(version = "0.1.0")]
#[command(about = "Daemon for persistent fake root state")]
struct Args {
    /// Path to the Unix domain socket
    #[arg(short, long, default_value = DEFAULT_SOCKET_PATH)]
    socket_path: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Initial fake UID
    #[arg(long, default_value = "0")]
    uid: u32,

    /// Initial fake GID
    #[arg(long, default_value = "0")]
    gid: u32,

    /// Clean up socket file on exit
    #[arg(long)]
    cleanup: bool,
}

fn main() {
    let name = program_name();
    let args = Args::command().bin_name(name).get_matches_from(env::args());
    let args = Args::from_arg_matches(&args).unwrap_or_else(|e| e.exit());

    println!("{}: Listening on {}", name, args.socket_path.display());
    println!("{}: Initial UID={}, GID={}", name, args.uid, args.gid);
    println!("Press Ctrl+C to stop");

    let socket_path = args.socket_path.clone();
    let cleanup = args.cleanup;
    ctrlc::set_handler(move || {
        println!("\n{name}: Shutting down...");
        if cleanup {
            let _ = std::fs::remove_file(&socket_path);
        }
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    if let Err(err) = daemon_server::run_blocking(
        &args.socket_path,
        args.uid,
        args.gid,
        args.verbose,
        args.cleanup,
    ) {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
