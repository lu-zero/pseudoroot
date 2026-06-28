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

    /// Get the real fchmod function
    unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32;

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

    /// Get the real unlink function
    unsafe fn real_unlink(path: *const std::os::raw::c_char) -> i32;

    /// Get the real unlinkat function
    #[cfg(target_os = "linux")]
    unsafe fn real_unlinkat(
        dirfd: i32,
        path: *const std::os::raw::c_char,
        flags: i32,
    ) -> i32;

    /// Get the real rmdir function
    unsafe fn real_rmdir(path: *const std::os::raw::c_char) -> i32;

    /// Get the real rename function
    unsafe fn real_rename(oldpath: *const std::os::raw::c_char, newpath: *const std::os::raw::c_char) -> i32;

    /// Get the real renameat function
    #[cfg(target_os = "linux")]
    unsafe fn real_renameat(
        olddirfd: i32,
        oldpath: *const std::os::raw::c_char,
        newdirfd: i32,
        newpath: *const std::os::raw::c_char,
    ) -> i32;

    /// Get the real renameat2 function
    #[cfg(target_os = "linux")]
    unsafe fn real_renameat2(
        olddirfd: i32,
        oldpath: *const std::os::raw::c_char,
        newdirfd: i32,
        newpath: *const std::os::raw::c_char,
        flags: u32,
    ) -> i32;

    /// Get the real mknod function
    #[cfg(target_os = "linux")]
    unsafe fn real_mknod(
        pathname: *const std::os::raw::c_char,
        mode: libc::mode_t,
        dev: libc::dev_t,
    ) -> i32;

    /// Get the real mknodat function
    #[cfg(target_os = "linux")]
    unsafe fn real_mknodat(
        dirfd: i32,
        pathname: *const std::os::raw::c_char,
        mode: libc::mode_t,
        dev: libc::dev_t,
    ) -> i32;

    /// Get the real setgroups function
    unsafe fn real_setgroups(size: i32, list: *const libc::gid_t) -> i32;

    /// Get the real capset function
    #[cfg(target_os = "linux")]
    unsafe fn real_capset(hdrp: *const std::ffi::c_void, data: *const std::ffi::c_void) -> i32;

    // xattr functions
    /// Get the real setxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_setxattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
        value: *const std::os::raw::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32;

    /// Get the real lsetxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_lsetxattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
        value: *const std::os::raw::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32;

    /// Get the real fsetxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_fsetxattr(
        fd: i32,
        name: *const std::os::raw::c_char,
        value: *const std::os::raw::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32;

    /// Get the real getxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_getxattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
        value: *mut std::os::raw::c_void,
        size: libc::size_t,
    ) -> i32;

    /// Get the real lgetxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_lgetxattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
        value: *mut std::os::raw::c_void,
        size: libc::size_t,
    ) -> i32;

    /// Get the real fgetxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_fgetxattr(
        fd: i32,
        name: *const std::os::raw::c_char,
        value: *mut std::os::raw::c_void,
        size: libc::size_t,
    ) -> i32;

    /// Get the real listxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_listxattr(
        path: *const std::os::raw::c_char,
        list: *mut std::os::raw::c_char,
        size: libc::size_t,
    ) -> i32;

    /// Get the real llistxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_llistxattr(
        path: *const std::os::raw::c_char,
        list: *mut std::os::raw::c_char,
        size: libc::size_t,
    ) -> i32;

    /// Get the real flistxattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_flistxattr(
        fd: i32,
        list: *mut std::os::raw::c_char,
        size: libc::size_t,
    ) -> i32;

    /// Get the real removexattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_removexattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
    ) -> i32;

    /// Get the real lremovexattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_lremovexattr(
        path: *const std::os::raw::c_char,
        name: *const std::os::raw::c_char,
    ) -> i32;

    /// Get the real fremovexattr function
    #[cfg(target_os = "linux")]
    unsafe fn real_fremovexattr(fd: i32, name: *const std::os::raw::c_char) -> i32;
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;
