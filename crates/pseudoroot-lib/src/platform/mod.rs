//! Platform-specific implementations for library interposition
//!
//! This module provides platform-specific implementations for Linux and macOS.

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
pub use macos::*;

// Common types and traits for platform-specific code
pub trait PlatformHelper {
    /// Get the real stat function
    unsafe fn real_stat(path: *const std::os::raw::c_char, buf: *mut libc::stat) -> i32;

    /// Get the real fstat function
    unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32;

    /// Get the real lstat function
    unsafe fn real_lstat(path: *const std::os::raw::c_char, buf: *mut libc::stat) -> i32;

    /// Get the real getuid function
    unsafe fn real_getuid() -> u32;

    /// Get the real geteuid function
    unsafe fn real_geteuid() -> u32;

    /// Get the real getgid function
    unsafe fn real_getgid() -> u32;

    /// Get the real getegid function
    unsafe fn real_getegid() -> u32;
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;
