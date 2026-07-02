//! pseudoroot-daemon - Daemon process for persistent fake root state

use clap::{CommandFactory, FromArgMatches, Parser};
use pseudoroot_core::daemon_server;
use pseudoroot_core::protocol::DEFAULT_SOCKET_PATH;
use std::env;
use std::path::PathBuf;

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
    let args = Args::command()
        .bin_name("pdrd")
        .get_matches_from(env::args());
    let args = Args::from_arg_matches(&args).unwrap_or_else(|e| e.exit());

    println!("pdrd: Listening on {}", args.socket_path.display());
    println!("pdrd: Initial UID={}, GID={}", args.uid, args.gid);
    println!("Press Ctrl+C to stop");

    let socket_path = args.socket_path.clone();
    let cleanup = args.cleanup;
    if let Err(err) = ctrlc::set_handler(move || {
        println!("\npdrd: Shutting down...");
        if cleanup {
            let _ = std::fs::remove_file(&socket_path);
        }
        std::process::exit(0);
    }) {
        eprintln!("Error: Failed to set Ctrl+C handler: {err}");
        std::process::exit(1);
    }

    match daemon_server::run_blocking(
        &args.socket_path,
        args.uid,
        args.gid,
        args.verbose,
        args.cleanup,
    ) {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}
