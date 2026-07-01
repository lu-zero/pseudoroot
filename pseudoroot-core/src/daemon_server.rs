//! In-process and standalone fake-root daemon server.

use crate::protocol::{
    ChownPayload, InodeKeyPayload, InodeStatePayload, InodeStateResult, IpcPayload, MessageType,
    ProtocolMessage, UidGidPayload,
};
use crate::state::{FakeInode, FakeRootState};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Concurrent daemon state shared across client handler threads.
pub type SharedDaemonState = Arc<FakeRootState>;

/// Background daemon for a single fakeroot session (no separate `pdrd` binary).
pub struct SessionDaemon {
    socket_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    state: SharedDaemonState,
}

impl SessionDaemon {
    /// Bind `socket_path` and serve IPC on a background thread.
    ///
    /// # Errors
    /// Returns an I/O error if the socket cannot be created or bound.
    pub fn start(socket_path: impl AsRef<Path>, uid: u32, gid: u32) -> io::Result<Self> {
        let socket_path = socket_path.as_ref().to_path_buf();
        remove_socket(&socket_path);

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        if let Err(err) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o666)) {
            eprintln!("pseudoroot: warning: socket permissions: {err}");
        }

        let state = new_shared_state(uid, gid);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_accept = Arc::clone(&shutdown);
        let accept_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            if let Err(err) = accept_loop(listener, accept_state, shutdown_accept, false) {
                eprintln!("pseudoroot: session daemon stopped: {err}");
            }
        });

        Ok(Self {
            socket_path,
            shutdown,
            handle: Some(handle),
            state,
        })
    }

    /// Socket path clients should pass via `PSEUDOROOT_DAEMON_SOCKET`.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Shared inode table backing this session (for SHM experiments).
    #[must_use]
    pub fn state(&self) -> &SharedDaemonState {
        &self.state
    }
}

impl Drop for SessionDaemon {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        remove_socket(&self.socket_path);
    }
}

/// Run the daemon on the current thread until the listener is closed.
///
/// # Errors
/// Returns an I/O error if binding or accepting connections fails irrecoverably.
pub fn run_blocking(
    socket_path: impl AsRef<Path>,
    uid: u32,
    gid: u32,
    verbose: bool,
    cleanup: bool,
) -> io::Result<()> {
    let socket_path = socket_path.as_ref().to_path_buf();
    remove_socket(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    if let Err(err) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o666)) {
        eprintln!("Warning: Failed to set socket permissions: {err}");
    }

    listener.set_nonblocking(true)?;
    let state = new_shared_state(uid, gid);
    let shutdown = Arc::new(AtomicBool::new(false));
    let result = accept_loop(listener, state, Arc::clone(&shutdown), verbose);

    // A graceful (message-triggered) shutdown always cleans up its own
    // socket; `--cleanup` covers exits (e.g. an accept() error) that
    // otherwise wouldn't.
    if cleanup || result.is_ok() {
        remove_socket(&socket_path);
    }
    result
}

/// Create concurrent shared state for a daemon session.
#[must_use]
pub fn new_shared_state(uid: u32, gid: u32) -> SharedDaemonState {
    let state = FakeRootState::new();
    state.set_current(uid, gid);
    Arc::new(state)
}

fn accept_loop(
    listener: UnixListener,
    state: SharedDaemonState,
    shutdown: Arc<AtomicBool>,
    verbose: bool,
) -> io::Result<()> {
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let state_clone = Arc::clone(&state);
                let shutdown_clone = Arc::clone(&shutdown);
                thread::spawn(move || handle_client(stream, state_clone, shutdown_clone, verbose));
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn handle_client(
    mut stream: UnixStream,
    state: SharedDaemonState,
    shutdown: Arc<AtomicBool>,
    verbose: bool,
) {
    if verbose {
        eprintln!("Daemon: New client connection");
    }

    loop {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(err) => {
                if verbose {
                    eprintln!("Daemon: Client disconnected: {err}");
                }
                break;
            }
        }

        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        if let Err(err) = stream.read_exact(&mut buf) {
            if verbose {
                eprintln!("Daemon: Failed to read message: {err}");
            }
            break;
        }

        let Some(message) = ProtocolMessage::from_bytes(&buf) else {
            if verbose {
                eprintln!("Daemon: Invalid message format");
            }
            break;
        };

        if verbose {
            eprintln!(
                "Daemon: Received message: {:?} (request_id: {})",
                message.message_type, message.request_id
            );
        }

        let response = dispatch_message(&message, &state, &shutdown);
        if let Err(err) = send_message(&mut stream, &response) {
            if verbose {
                eprintln!("Daemon: Failed to send response: {err}");
            }
            break;
        }
    }

    if verbose {
        eprintln!("Daemon: Client disconnected");
    }
}

fn dispatch_message(
    message: &ProtocolMessage,
    state: &SharedDaemonState,
    shutdown: &Arc<AtomicBool>,
) -> ProtocolMessage {
    match message.message_type {
        MessageType::Ping => ProtocolMessage::response(message.request_id, b"pong".to_vec()),
        MessageType::GetCurrentUidGid => {
            let payload = UidGidPayload {
                uid: state.current_uid(),
                gid: state.current_gid(),
            };
            ProtocolMessage::response(message.request_id, payload.to_payload())
        }
        MessageType::SetCurrentUidGid => {
            if let Some(payload) = UidGidPayload::from_payload(&message.payload) {
                state.set_current(payload.uid, payload.gid);
                ProtocolMessage::response(message.request_id, vec![])
            } else {
                ProtocolMessage::error(message.request_id, "Invalid UidGidPayload")
            }
        }
        MessageType::RegisterOwnership => {
            if let Some(payload) = InodeStatePayload::from_payload(&message.payload) {
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
        MessageType::UpsertChown => {
            if let Some(payload) = ChownPayload::from_payload(&message.payload) {
                state.upsert_chown(
                    (payload.dev, payload.ino),
                    payload.uid,
                    payload.gid,
                    payload.default_uid,
                    payload.default_gid,
                );
                ProtocolMessage::response(message.request_id, vec![])
            } else {
                ProtocolMessage::error(message.request_id, "Invalid ChownPayload")
            }
        }
        MessageType::GetOwnership => {
            if let Some(payload) = InodeKeyPayload::from_payload(&message.payload) {
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
                state.remove_inode((payload.dev, payload.ino));
                ProtocolMessage::response(message.request_id, vec![])
            } else {
                ProtocolMessage::error(message.request_id, "Invalid InodeKeyPayload")
            }
        }
        MessageType::Init => {
            if let Some(payload) = UidGidPayload::from_payload(&message.payload) {
                state.set_current(payload.uid, payload.gid);
                ProtocolMessage::response(message.request_id, vec![])
            } else {
                ProtocolMessage::error(message.request_id, "Invalid UidGidPayload")
            }
        }
        MessageType::Shutdown => {
            shutdown.store(true, Ordering::Release);
            ProtocolMessage::response(message.request_id, vec![])
        }
        MessageType::Response | MessageType::Error => {
            ProtocolMessage::error(message.request_id, "Unexpected message type")
        }
    }
}

fn send_message(stream: &mut UnixStream, message: &ProtocolMessage) -> io::Result<()> {
    let bytes = message.to_bytes();
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(&bytes)?;
    stream.flush()
}

fn remove_socket(socket_path: &Path) {
    let _ = fs::remove_file(socket_path);
}
