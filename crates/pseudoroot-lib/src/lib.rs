//! pseudoroot-lib - Library interposition for fake root functionality
//!
//! This shared library intercepts system calls to provide fake root functionality,
//! similar to the classic `fakeroot` tool. It uses library interposition via
//! `LD_PRELOAD` on Linux or `DYLD_INSERT_LIBRARIES` on macOS.
//!
//! # How it works
//!
//! The library maintains a global fake state that tracks:
//! - The current fake UID and GID
//! - A mapping from real to fake UID/GID
//! - File ownership information
//!
//! When intercepted functions are called, they return values from this fake state
//! instead of the real system state.
//!
//! # Configuration
//!
//! The library reads environment variables on initialization:
//! - `PSEUDOROOT_UID`: The fake UID to use (default: 0 = root)
//! - `PSEUDOROOT_GID`: The fake GID to use (default: 0 = root)
//!
//! # Safety
//!
//! This library uses unsafe code to intercept system calls. It must be used with
//! caution as incorrect interposition can cause system instability.

#![allow(clippy::missing_safety_doc)]

mod platform;

use pseudoroot_core::state::{
    global_state_read, global_state_write, init_global_state, FileOwnership,
};
use std::env;
use std::ffi::CStr;
use std::os::raw::c_char;

/// Initialize the pseudoroot library
///
/// This function is called automatically when the library is loaded,
/// thanks to the `ctor` crate.
#[ctor::ctor]
unsafe fn init() {
    // Initialize the global state and set initial UID/GID from environment
    let mut state = init_global_state();
    
    // Read configuration from environment variables
    let uid = env::var("PSEUDOROOT_UID").ok().and_then(|u| u.parse::<u32>().ok()).unwrap_or(0);
    let gid = env::var("PSEUDOROOT_GID").ok().and_then(|g| g.parse::<u32>().ok()).unwrap_or(0);
    
    state.set_current(uid, gid);
    // The guard is dropped here, but the global state is initialized and configured
}

// Re-export the platform module for conditional compilation
pub use platform::*;

/// Get the current fake UID
///
/// This wraps the real getuid() system call to return the fake UID.
#[unsafe(no_mangle)]
pub extern "C" fn getuid() -> u32 {
    // Get the current fake UID from the global state
    let state = global_state_read();
    state.current_uid()
}

/// Get the current effective UID
///
/// This wraps the real geteuid() system call to return the fake effective UID.
#[unsafe(no_mangle)]
pub extern "C" fn geteuid() -> u32 {
    // For simplicity, we treat uid and euid the same in fake mode
    getuid()
}

/// Get the current GID
///
/// This wraps the real getgid() system call to return the fake GID.
#[unsafe(no_mangle)]
pub extern "C" fn getgid() -> u32 {
    // Get the current fake GID from the global state
    let state = global_state_read();
    state.current_gid()
}

/// Get the current effective GID
///
/// This wraps the real getegid() system call to return the fake effective GID.
#[unsafe(no_mangle)]
pub extern "C" fn getegid() -> u32 {
    // For simplicity, we treat gid and egid the same in fake mode
    getgid()
}

/// Set file ownership
///
/// This intercepts chown() to record ownership changes in our fake state.
#[unsafe(no_mangle)]
pub extern "C" fn chown(
    path: *const c_char,
    uid: u32,
    gid: u32,
) -> i32 {
    // Try to record the ownership change in our fake state
    let mut state = global_state_write();
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        state.set_ownership(path_str, FileOwnership::new(uid, gid));
    }
    
    // Also call the real chown to actually change the file system
    // This allows the fake state to match the real state
    unsafe { platform::real_chown(path, uid, gid) }
}

/// Get file status
///
/// This wraps stat() to return fake ownership information.
#[unsafe(no_mangle)]
pub extern "C" fn stat(
    path: *const c_char,
    buf: *mut libc::stat,
) -> i32 {
    // First, call the real stat to get actual file info
    let result = unsafe { platform::real_stat(path, buf) };
    
    if result == 0 && !buf.is_null() {
        // Successfully got file info, now modify ownership fields
        unsafe { modify_stat_ownership(path, buf) };
    }
    
    result
}

/// Get file status for a file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn fstat(
    fd: i32,
    buf: *mut libc::stat,
) -> i32 {
    // First, call the real fstat to get actual file info
    let result = unsafe { platform::real_fstat(fd, buf) };
    
    if result == 0 && !buf.is_null() {
        // Successfully got file info, but we can't map fd to path easily
        // For now, just apply the global fake UID/GID
        unsafe { modify_stat_ownership_by_uid_gid(buf) };
    }
    
    result
}

/// Get file status for a path with symbolic link following
#[unsafe(no_mangle)]
pub extern "C" fn lstat(
    path: *const c_char,
    buf: *mut libc::stat,
) -> i32 {
    // First, call the real lstat to get actual file info
    let result = unsafe { platform::real_lstat(path, buf) };
    
    if result == 0 && !buf.is_null() {
        // Successfully got file info, now modify ownership fields
        unsafe { modify_stat_ownership(path, buf) };
    }
    
    result
}

/// Change file mode
#[unsafe(no_mangle)]
pub extern "C" fn chmod(
    path: *const c_char,
    mode: libc::mode_t,
) -> i32 {
    // Just pass through to the real chmod for now
    // In a full implementation, we might want to track file modes too
    unsafe { platform::real_chmod(path, mode) }
}

/// Change file ownership by path (no symlink following)
#[unsafe(no_mangle)]
pub extern "C" fn lchown(
    path: *const c_char,
    uid: u32,
    gid: u32,
) -> i32 {
    // Try to record the ownership change in our fake state
    let mut state = global_state_write();
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        state.set_ownership(path_str, FileOwnership::new(uid, gid));
    }
    
    // Also call the real lchown to actually change the file system
    unsafe { platform::real_lchown(path, uid, gid) }
}

/// Change file ownership by file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn fchown(
    fd: i32,
    uid: u32,
    gid: u32,
) -> i32 {
    // Can't easily map fd to path, so just pass through to real fchown
    unsafe { platform::real_fchown(fd, uid, gid) }
}

/// Modify stat buffer ownership fields based on path-specific ownership
///
/// # Safety
/// The caller must ensure that buf is a valid pointer to a libc::stat struct.
unsafe fn modify_stat_ownership(path: *const c_char, buf: *mut libc::stat) {
    if buf.is_null() {
        return;
    }
    
    // Try to get path-specific ownership from our fake state
    let state = global_state_read();
    if let Some(path_str) = cstr_to_string(path) {
        if let Some(ownership) = state.get_ownership(&path_str) {
            unsafe {
                (*buf).st_uid = ownership.uid as libc::uid_t;
                (*buf).st_gid = ownership.gid as libc::gid_t;
            }
            return;
        }
    }
    
    // No path-specific ownership, apply global fake UID/GID
    unsafe { modify_stat_ownership_by_uid_gid(buf) };
}

/// Modify stat buffer ownership fields based on global fake UID/GID
///
/// # Safety
/// The caller must ensure that buf is a valid pointer to a libc::stat struct.
unsafe fn modify_stat_ownership_by_uid_gid(buf: *mut libc::stat) {
    if buf.is_null() {
        return;
    }
    
    let state = global_state_read();
    unsafe {
        (*buf).st_uid = state.current_uid() as libc::uid_t;
        (*buf).st_gid = state.current_gid() as libc::gid_t;
    }
}

/// Helper function to convert C string to Rust string
#[inline]
#[must_use]
pub unsafe fn cstr_to_string(cstr: *const c_char) -> Option<String> {
    if cstr.is_null() {
        None
    } else {
        Some(
            CStr::from_ptr(cstr)
                .to_string_lossy()
                .into_owned(),
        )
    }
}
