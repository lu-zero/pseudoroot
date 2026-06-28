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

use clap::Parser;
use std::env;
use std::ffi::OsString;
use std::process;

/// Run a command with fake root privileges
#[derive(Parser, Debug)]
#[command(name = "pseudoroot")]
#[command(author = "Luca Barbato <lu_zero@gentoo.org>")]
#[command(version = "0.1.0")]
#[command(about = "Run commands with fake root privileges", long_about = None)]
struct Args {
    /// The command to run with fake root privileges
    command: Vec<String>,
    
    /// Print the library path and exit
    #[arg(long)]
    print_library_path: bool,
}

fn main() {
    let args = Args::parse();

    // Handle --print-library-path
    if args.print_library_path {
        let lib_path = find_library_path();
        match lib_path {
            Some(path) => {
                println!("{}", path.display());
                process::exit(0);
            }
            None => {
                eprintln!("Error: Could not find pseudoroot library.");
                eprintln!("The library needs to be built first with: cargo cbuild -p pseudoroot-lib --release");
                process::exit(1);
            }
        }
    }

    // Check if we have a command to run
    if args.command.is_empty() {
        eprintln!("Error: No command specified.");
        eprintln!("Usage: pseudoroot <command> [args...]");
        eprintln!("Try 'pseudoroot --help' for more information.");
        process::exit(1);
    }

    // Find the library path
    let lib_path = match find_library_path() {
        Some(path) => path,
        None => {
            eprintln!("Error: Could not find pseudoroot library.");
            eprintln!("The library needs to be built first with: cargo cbuild -p pseudoroot-lib --release");
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

    // Check if the environment variable already contains other libraries
    let env_var_value = if let Some(existing) = env::var_os(env_var_name) {
        // Append our library to the existing value
        // On Linux, LD_PRELOAD uses colon-separated list
        // On macOS, DYLD_INSERT_LIBRARIES also uses colon-separated list
        let mut value = OsString::from(existing);
        value.push(":");
        value.push(lib_path);
        value
    } else {
        // Just set our library
        OsString::from(lib_path)
    };

    // Execute the command with the modified environment
    let mut command = process::Command::new(&args.command[0]);
    command.args(&args.command[1..]);
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

/// Find the path to the pseudoroot library
///
/// This tries several locations:
/// 1. The build directory (target/debug or target/release)
/// 2. Standard library paths
fn find_library_path() -> Option<std::path::PathBuf> {
    // Try to find the library in the build directory
    let candidates = [
        // Built with cargo-c in release mode
        "target/cbuild/release/libpseudoroot_lib.so",
        "target/cbuild/debug/libpseudoroot_lib.so",
        // Built with cargo in release mode
        "target/release/libpseudoroot_lib.so",
        "target/debug/libpseudoroot_lib.so",
        // macOS
        "target/cbuild/release/libpseudoroot_lib.dylib",
        "target/cbuild/debug/libpseudoroot_lib.dylib",
        "target/release/libpseudoroot_lib.dylib",
        "target/debug/libpseudoroot_lib.dylib",
    ];

    for candidate in candidates {
        let path = std::path::PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}
