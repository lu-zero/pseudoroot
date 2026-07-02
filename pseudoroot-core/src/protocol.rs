//! IPC protocol for communication between pseudoroot-lib and pseudoroot-daemon
//!
//! This module provides a complete IPC implementation using Unix domain sockets
//! for communication between the interposed library and the daemon process.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default socket path for daemon communication
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/pseudoroot.sock";

/// Message types for IPC communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// Register fake ownership for a path
    RegisterOwnership,
    /// Set current UID/GID
    SetCurrentUidGid,
    /// Get current UID/GID
    GetCurrentUidGid,
    /// Ping message for health check
    Ping,
    /// Response to a message
    Response,
    /// Error response
    Error,
    /// Get file ownership by path
    GetOwnership,
    /// Remove file ownership by path
    RemoveOwnership,
    /// Initialize connection with UID/GID
    Init,
    /// Merge uid/gid into an inode entry in one update
    UpsertChown,
    /// Ask a standalone daemon to shut itself down
    Shutdown,
}

/// Request ID generator
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a new request ID
#[must_use]
pub fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// A message in the IPC protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    /// The type of message
    pub message_type: MessageType,
    /// The payload data (serialized)
    pub payload: Vec<u8>,
    /// Request ID for matching responses
    pub request_id: u64,
}

impl ProtocolMessage {
    /// Create a new protocol message
    #[must_use]
    pub fn new(message_type: MessageType, payload: Vec<u8>, request_id: u64) -> Self {
        Self {
            message_type,
            payload,
            request_id,
        }
    }

    /// Create a ping message
    #[must_use]
    pub fn ping() -> Self {
        Self::new(MessageType::Ping, Vec::new(), next_request_id())
    }

    /// Create a response message
    #[must_use]
    pub fn response(request_id: u64, payload: Vec<u8>) -> Self {
        Self::new(MessageType::Response, payload, request_id)
    }

    /// Create an error message
    #[must_use]
    pub fn error(request_id: u64, error_message: &str) -> Self {
        Self::new(
            MessageType::Error,
            error_message.as_bytes().to_vec(),
            request_id,
        )
    }

    /// Serialize the message to bytes
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Failed to serialize message")
    }

    /// Deserialize a message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

/// Write a length-prefixed frame (4-byte big-endian length + payload).
pub fn write_framed(stream: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(bytes)?;
    stream.flush()
}

/// Read a length-prefixed frame (4-byte big-endian length + payload).
pub fn read_framed(stream: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// IPC Channel for communication with the daemon
pub struct IpcChannel {
    stream: Option<UnixStream>,
    socket_path: PathBuf,
}

impl IpcChannel {
    /// Create a new IPC channel connected to the daemon socket
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            stream: None,
            socket_path: socket_path.into(),
        }
    }

    /// Connect to the daemon socket
    pub fn connect(&mut self) -> Result<(), std::io::Error> {
        if self.stream.is_some() {
            return Ok(());
        }
        let stream = UnixStream::connect(&self.socket_path)?;
        stream.set_nonblocking(false)?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Check if connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Send a message
    pub fn send(&mut self, message: ProtocolMessage) -> Result<(), std::io::Error> {
        if let Some(stream) = &mut self.stream {
            write_framed(stream, &message.to_bytes())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Not connected to daemon",
            ))
        }
    }

    /// Receive a message (blocking)
    pub fn recv(&mut self) -> Result<ProtocolMessage, std::io::Error> {
        if let Some(stream) = &mut self.stream {
            let buf = read_framed(stream)?;
            ProtocolMessage::from_bytes(&buf).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid message")
            })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Not connected to daemon",
            ))
        }
    }

    /// Send a request and wait for response
    pub fn request(&mut self, message: ProtocolMessage) -> Result<ProtocolMessage, std::io::Error> {
        self.send(message)?;
        let response = self.recv()?;
        Ok(response)
    }

    /// Get the socket path
    #[must_use]
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

impl Default for IpcChannel {
    fn default() -> Self {
        Self::new(DEFAULT_SOCKET_PATH)
    }
}

/// Helper trait for types that can be sent as IPC payloads
pub trait IpcPayload: Serialize + for<'a> Deserialize<'a> {
    /// Convert to payload bytes
    fn to_payload(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Failed to serialize payload")
    }

    /// Convert from payload bytes
    fn from_payload(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

impl<T: Serialize + for<'a> Deserialize<'a>> IpcPayload for T {}

/// Inode identity payload
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InodeKeyPayload {
    pub dev: u64,
    pub ino: u64,
}

/// Inode state registration payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InodeStatePayload {
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
    pub gid: u32,
    pub mode: Option<u32>,
    pub rdev: Option<u64>,
    pub xattrs: HashMap<String, Vec<u8>>,
}

/// Inode state query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InodeStateResult {
    pub found: bool,
    pub uid: u32,
    pub gid: u32,
    pub mode: Option<u32>,
    pub rdev: Option<u64>,
    pub xattrs: HashMap<String, Vec<u8>>,
}

/// UID/GID payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UidGidPayload {
    pub uid: u32,
    pub gid: u32,
}

/// Chown merge payload (`uid`/`gid` of `u32::MAX` leave that field unchanged).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChownPayload {
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
    pub gid: u32,
    pub default_uid: u32,
    pub default_gid: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = ProtocolMessage::new(MessageType::Ping, vec![1, 2, 3], 42);
        let bytes = msg.to_bytes();
        let decoded = ProtocolMessage::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.message_type, MessageType::Ping);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
        assert_eq!(decoded.request_id, 42);
    }

    #[test]
    fn test_inode_state_payload() {
        let payload = InodeStatePayload {
            dev: 1,
            ino: 42,
            uid: 1000,
            gid: 2000,
            mode: Some(0o4755),
            rdev: Some(0x0803),
            xattrs: HashMap::from([("security.capability".to_string(), vec![1, 2, 3])]),
        };
        let bytes = payload.to_payload();
        let decoded: InodeStatePayload = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.dev, 1);
        assert_eq!(decoded.ino, 42);
        assert_eq!(decoded.uid, 1000);
        assert_eq!(decoded.gid, 2000);
        assert_eq!(decoded.mode, Some(0o4755));
        assert_eq!(decoded.rdev, Some(0x0803));
        assert_eq!(decoded.xattrs["security.capability"], vec![1, 2, 3]);
    }

    #[test]
    fn test_request_id_generation() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert_eq!(id1 + 1, id2);
    }
}
