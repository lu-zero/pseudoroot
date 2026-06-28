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

    /// Get the real chown function
    unsafe fn real_chown(path: *const std::os::raw::c_char, uid: u32, gid: u32) -> i32;

    /// Get the real chmod function
    unsafe fn real_chmod(path: *const std::os::raw::c_char, mode: libc::mode_t) -> i32;

    /// Get the real lchown function
    unsafe fn real_lchown(path: *const std::os::raw::c_char, uid: u32, gid: u32) -> i32;

    /// Get the real fchown function
    unsafe fn real_fchown(fd: i32, uid: u32, gid: u32) -> i32;

    /// Get the real fstatat function
    #[cfg(target_os = "linux")]
    unsafe fn real_fstatat(
        dirfd: i32,
        pathname: *const std::os::raw::c_char,
        buf: *mut libc::stat,
        flags: i32,
    ) -> i32;

    /// Get the real statx function (Linux-specific)
    #[cfg(target_os = "linux")]
    unsafe fn real_statx(
        dirfd: i32,
        pathname: *const std::os::raw::c_char,
        buf: *mut std::ffi::c_void,
        mask: u32,
        flags: i32,
    ) -> i32;

    /// Get the real fchownat function
    #[cfg(target_os = "linux")]
    unsafe fn real_fchownat(
        dirfd: i32,
        path: *const std::os::raw::c_char,
        uid: u32,
        gid: u32,
        flags: i32,
    ) -> i32;

    /// Get the real fchmodat function
    #[cfg(target_os = "linux")]
    unsafe fn real_fchmodat(
        dirfd: i32,
        path: *const std::os::raw::c_char,
        mode: libc::mode_t,
        flags: i32,
    ) -> i32;

    /// Get the real getresuid function
    unsafe fn real_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32;

    /// Get the real getresgid function
    unsafe fn real_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32;

    /// Get the real setuid function
    unsafe fn real_setuid(uid: u32) -> i32;

    /// Get the real setgid function
    unsafe fn real_setgid(gid: u32) -> i32;

    /// Get the real setreuid function
    unsafe fn real_setreuid(ruid: u32, euid: u32) -> i32;

    /// Get the real setregid function
    unsafe fn real_setregid(rgid: u32, egid: u32) -> i32;

    /// Get the real setresuid function
    unsafe fn real_setresuid(ruid: u32, euid: u32, suid: u32) -> i32;

    /// Get the real setresgid function
    unsafe fn real_setresgid(rgid: u32, egid: u32, sgid: u32) -> i32;

    /// Get the real setfsuid function
    unsafe fn real_setfsuid(uid: u32) -> i32;

    /// Get the real setfsgid function
    unsafe fn real_setfsgid(gid: u32) -> i32;
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;
