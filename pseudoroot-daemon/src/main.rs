//! pseudoroot-daemon - Daemon process for persistent fake root state
//!
//! This daemon maintains a shared state of fake ownership information across
//! multiple processes, similar to the classic `faked` process from fakeroot.
//!
//! The daemon:
//! - Listens on a Unix domain socket for IPC messages
//! - Maintains a shared state of fake ownership information
//! - Handles requests for ownership lookups and modifications
//! - Manages fake UID/GID state
//!
//! Usage:
//! ```bash
//! pseudoroot-daemon [OPTIONS]
//! ```
//!
//! Options:
//! - `-s, --socket-path PATH` - Path to the Unix domain socket (default: /tmp/pseudoroot.sock)
//! - `-v, --verbose` - Enable verbose logging
//! - `--uid UID` - Initial fake UID (default: 0)
//! - `--gid GID` - Initial fake GID (default: 0)

use clap::{CommandFactory, FromArgMatches, Parser};
use pseudoroot_core::protocol::{
    InodeKeyPayload, InodeStatePayload, InodeStateResult, IpcPayload, MessageType, ProtocolMessage,
    UidGidPayload, DEFAULT_SOCKET_PATH,
};
use pseudoroot_core::state::FakeInode;
use pseudoroot_core::state::FakeRootState;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

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

/// Shared state for the daemon
struct DaemonState {
    fake_state: Arc<RwLock<FakeRootState>>,
    verbose: bool,
}

impl DaemonState {
    fn new(uid: u32, gid: u32) -> Self {
        let mut state = FakeRootState::new();
        state.set_current(uid, gid);
        Self {
            fake_state: Arc::new(RwLock::new(state)),
            verbose: false,
        }
    }

    fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

/// Handle a single client connection
fn handle_client(mut stream: UnixStream, state: Arc<RwLock<FakeRootState>>, verbose: bool) {
    if verbose {
        eprintln!("Daemon: New client connection");
    }

    loop {
        // Read the message length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(e) => {
                if verbose {
                    eprintln!("Daemon: Client disconnected: {}", e);
                }
                break;
            }
        }

        let len = u32::from_be_bytes(len_buf) as usize;

        // Read the message
        let mut buf = vec![0u8; len];
        if let Err(e) = stream.read_exact(&mut buf) {
            if verbose {
                eprintln!("Daemon: Failed to read message: {}", e);
            }
            break;
        }

        // Deserialize the message
        let message = match ProtocolMessage::from_bytes(&buf) {
            Some(msg) => msg,
            None => {
                if verbose {
                    eprintln!("Daemon: Invalid message format");
                }
                break;
            }
        };

        if verbose {
            eprintln!(
                "Daemon: Received message: {:?} (request_id: {})",
                message.message_type, message.request_id
            );
        }

        // Handle the message
        let response = match message.message_type {
            MessageType::Ping => {
                if verbose {
                    eprintln!("Daemon: Handling ping");
                }
                ProtocolMessage::response(message.request_id, b"pong".to_vec())
            }
            MessageType::GetCurrentUidGid => {
                let state = state.read().unwrap();
                let payload = UidGidPayload {
                    uid: state.current_uid(),
                    gid: state.current_gid(),
                };
                ProtocolMessage::response(message.request_id, payload.to_payload())
            }
            MessageType::SetCurrentUidGid => {
                if let Some(payload) = UidGidPayload::from_payload(&message.payload) {
                    let mut state = state.write().unwrap();
                    state.set_current(payload.uid, payload.gid);
                    ProtocolMessage::response(message.request_id, vec![])
                } else {
                    ProtocolMessage::error(message.request_id, "Invalid UidGidPayload")
                }
            }
            MessageType::RegisterOwnership => {
                if let Some(payload) = InodeStatePayload::from_payload(&message.payload) {
                    let mut state = state.write().unwrap();
                    state.set_inode(
                        (payload.dev, payload.ino),
                        FakeInode {
                            uid: payload.uid,
                            gid: payload.gid,
                            mode: payload.mode,
                            rdev: payload.rdev,
                            xattrs: payload.xattrs,
                        },
                    );
                    ProtocolMessage::response(message.request_id, vec![])
                } else {
                    ProtocolMessage::error(message.request_id, "Invalid InodeStatePayload")
                }
            }
            MessageType::GetOwnership => {
                if let Some(payload) = InodeKeyPayload::from_payload(&message.payload) {
                    let state = state.read().unwrap();
                    let result = if let Some(inode) = state.get_inode((payload.dev, payload.ino)) {
                        InodeStateResult {
                            found: true,
                            uid: inode.uid,
                            gid: inode.gid,
                            mode: inode.mode,
                            rdev: inode.rdev,
                            xattrs: inode.xattrs,
                        }
                    } else {
                        InodeStateResult {
                            found: false,
                            uid: 0,
                            gid: 0,
                            mode: None,
                            rdev: None,
                            xattrs: std::collections::HashMap::new(),
                        }
                    };
                    ProtocolMessage::response(message.request_id, result.to_payload())
                } else {
                    ProtocolMessage::error(message.request_id, "Invalid InodeKeyPayload")
                }
            }
            MessageType::RemoveOwnership => {
                if let Some(payload) = InodeKeyPayload::from_payload(&message.payload) {
                    let mut state = state.write().unwrap();
                    state.remove_inode((payload.dev, payload.ino));
                    ProtocolMessage::response(message.request_id, vec![])
                } else {
                    ProtocolMessage::error(message.request_id, "Invalid InodeKeyPayload")
                }
            }
            MessageType::Init => {
                // Initialize with provided UID/GID
                if let Some(payload) = UidGidPayload::from_payload(&message.payload) {
                    let mut state = state.write().unwrap();
                    state.set_current(payload.uid, payload.gid);
                    ProtocolMessage::response(message.request_id, vec![])
                } else {
                    ProtocolMessage::error(message.request_id, "Invalid UidGidPayload")
                }
            }
            MessageType::Response | MessageType::Error => {
                // Daemon shouldn't receive responses or errors
                ProtocolMessage::error(message.request_id, "Unexpected message type")
            }
        };

        // Send the response
        if let Err(e) = send_message(&mut stream, &response) {
            if verbose {
                eprintln!("Daemon: Failed to send response: {}", e);
            }
            break;
        }
    }

    if verbose {
        eprintln!("Daemon: Client disconnected");
    }
}

/// Send a message to a stream
fn send_message(stream: &mut UnixStream, message: &ProtocolMessage) -> Result<(), std::io::Error> {
    let bytes = message.to_bytes();
    // First send the length (4 bytes, big-endian)
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    // Then send the message
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

/// Clean up the socket file
fn cleanup_socket(socket_path: &PathBuf) {
    let _ = fs::remove_file(socket_path);
}

fn main() {
    let name = program_name();
    let args = Args::command().bin_name(name).get_matches_from(env::args());
    let args = Args::from_arg_matches(&args).unwrap_or_else(|e| e.exit());

    // Clean up any existing socket
    cleanup_socket(&args.socket_path);

    // Create the shared state
    let state = DaemonState::new(args.uid, args.gid).with_verbose(args.verbose);

    // Create the socket listener
    let listener = match UnixListener::bind(&args.socket_path) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!(
                "Error: Failed to bind to socket {}: {}",
                args.socket_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    // Set permissions on the socket
    if let Err(e) = fs::set_permissions(&args.socket_path, fs::Permissions::from_mode(0o666)) {
        eprintln!("Warning: Failed to set socket permissions: {}", e);
    }

    println!("{}: Listening on {}", name, args.socket_path.display());
    println!("{}: Initial UID={}, GID={}", name, args.uid, args.gid);
    println!("Press Ctrl+C to stop");

    let socket_path_clone = args.socket_path.clone();
    let cleanup_on_exit = args.cleanup;

    // Handle Ctrl+C for graceful shutdown
    ctrlc::set_handler(move || {
        println!("\n{name}: Shutting down...");
        if cleanup_on_exit {
            cleanup_socket(&socket_path_clone);
        }
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    // Main server loop
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state_clone = state.fake_state.clone();
                thread::spawn(move || {
                    handle_client(stream, state_clone, args.verbose);
                });
            }
            Err(e) => {
                eprintln!("Error: Failed to accept connection: {}", e);
                // Sleep briefly to avoid tight loop on error
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // Clean up on exit
    if args.cleanup {
        cleanup_socket(&args.socket_path);
    }
}
