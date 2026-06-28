//! IPC protocol for communication between pseudoroot-lib and pseudoroot-daemon
//!
//! This module defines the protocol for communication between the interposed
//! library and a potential daemon process. Currently a stub for future implementation.

/// Message types for IPC communication
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Register a file with fake ownership
    RegisterOwnership,
    /// Unregister a file
    UnregisterOwnership,
    /// Query ownership for a file
    QueryOwnership,
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
}

/// A message in the IPC protocol
#[derive(Debug, Clone)]
pub struct ProtocolMessage {
    /// The type of message
    pub message_type: MessageType,
    /// The payload data
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
    pub fn ping(request_id: u64) -> Self {
        Self::new(MessageType::Ping, Vec::new(), request_id)
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
}

/// Future: Implement actual IPC using Unix domain sockets or shared memory
/// For now, this is a placeholder for the protocol design.
#[allow(dead_code)]
pub struct IpcChannel;

#[allow(dead_code)]
impl IpcChannel {
    /// Create a new IPC channel (stub)
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Send a message (stub)
    #[allow(dead_code)]
    pub fn send(&self, _message: ProtocolMessage) -> Result<(), ()> {
        Ok(())
    }

    /// Receive a message (stub)
    #[allow(dead_code)]
    pub fn recv(&self) -> Result<ProtocolMessage, ()> {
        Err(())
    }
}
