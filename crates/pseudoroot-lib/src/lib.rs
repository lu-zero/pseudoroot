//! pseudoroot-lib - Library interposition for fake root functionality
//!
//! This shared library intercepts system calls to provide fake root functionality,
//! similar to the classic `fakeroot` tool. It uses library interposition via
//! `LD_PRELOAD` on Linux or `DYLD_INSERT_LIBRARIES` on macOS.
//!
//! # Safety
//!
//! This library uses unsafe code to intercept system calls. It must be used with
//! caution as incorrect interposition can cause system instability.

#![allow(clippy::missing_safety_doc)]

mod platform;

use pseudoroot_core::state::init_global_state;
use std::ffi::CStr;
use std::os::raw::c_char;

/// Initialize the pseudoroot library
///
/// This function is called automatically when the library is loaded,
/// thanks to the `ctor` crate.
#[ctor::ctor]
unsafe fn init() {
    // Initialize the global state
    let _guard = init_global_state();
    // The guard is dropped here, but the global state is initialized
}

// Re-export the platform module for conditional compilation
pub use platform::*;

/// Get the current fake UID
///
/// This wraps the real getuid() system call to return the fake UID.
#[unsafe(no_mangle)]
pub extern "C" fn getuid() -> u32 {
    // For now, always return 0 (root) as a simple implementation
    // In a full implementation, this would check the global state
    // and return the appropriate fake UID based on the current process
    0
}

/// Get the current effective UID
///
/// This wraps the real geteuid() system call to return the fake effective UID.
#[unsafe(no_mangle)]
pub extern "C" fn geteuid() -> u32 {
    // For now, always return 0 (root) as a simple implementation
    0
}

/// Get the current GID
///
/// This wraps the real getgid() system call to return the fake GID.
#[unsafe(no_mangle)]
pub extern "C" fn getgid() -> u32 {
    // For now, always return 0 (root group) as a simple implementation
    0
}

/// Get the current effective GID
///
/// This wraps the real getegid() system call to return the fake effective GID.
#[unsafe(no_mangle)]
pub extern "C" fn getegid() -> u32 {
    // For now, always return 0 (root group) as a simple implementation
    0
}

/// Set file ownership (stub implementation)
///
/// This is a placeholder for the chown interposition.
#[unsafe(no_mangle)]
pub extern "C" fn chown(
    _path: *const c_char,
    _uid: u32,
    _gid: u32,
) -> i32 {
    // In a real implementation, we would:
    // 1. Record the ownership change in our fake state
    // 2. Potentially call the real chown() if we want to affect the actual filesystem
    // For now, just return 0 (success)
    0
}

/// Get file status (stub implementation)
///
/// This wraps stat() to return fake ownership information.
#[unsafe(no_mangle)]
pub extern "C" fn stat(
    path: *const c_char,
    buf: *mut libc::stat,
) -> i32 {
    // In a real implementation, we would:
    // 1. Call the real stat() to get actual file info
    // 2. Modify the ownership fields to reflect our fake state
    // For now, just delegate to the real stat()
    unsafe { platform::real_stat(path, buf) }
}

/// Get file status for a file descriptor (stub implementation)
#[unsafe(no_mangle)]
pub extern "C" fn fstat(
    fd: i32,
    buf: *mut libc::stat,
) -> i32 {
    // In a real implementation, we would modify the ownership fields
    unsafe { platform::real_fstat(fd, buf) }
}

/// Get file status for a path with symbolic link following (stub implementation)
#[unsafe(no_mangle)]
pub extern "C" fn lstat(
    path: *const c_char,
    buf: *mut libc::stat,
) -> i32 {
    // In a real implementation, we would modify the ownership fields
    unsafe { platform::real_lstat(path, buf) }
}

/// Change file mode (stub implementation)
#[unsafe(no_mangle)]
pub extern "C" fn chmod(
    _path: *const c_char,
    _mode: libc::mode_t,
) -> i32 {
    // For now, just return 0 (success)
    0
}

/// Change file ownership by path (stub implementation)
#[unsafe(no_mangle)]
pub extern "C" fn lchown(
    _path: *const c_char,
    _uid: u32,
    _gid: u32,
) -> i32 {
    // For now, just return 0 (success)
    0
}

/// Change file ownership by file descriptor (stub implementation)
#[unsafe(no_mangle)]
pub extern "C" fn fchown(
    _fd: i32,
    _uid: u32,
    _gid: u32,
) -> i32 {
    // For now, just return 0 (success)
    0
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
