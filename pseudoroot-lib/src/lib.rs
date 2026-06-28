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
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod platform;

use pseudoroot_core::daemon_client::{
    daemon_get_current_uid_gid, daemon_get_ownership, daemon_init, daemon_mode_enabled,
    daemon_remove_ownership, daemon_set_current_uid_gid, daemon_set_ownership,
    init_daemon_connection,
};
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
    let uid = env::var("PSEUDOROOT_UID")
        .ok()
        .and_then(|u| u.parse::<u32>().ok())
        .unwrap_or(0);
    let gid = env::var("PSEUDOROOT_GID")
        .ok()
        .and_then(|g| g.parse::<u32>().ok())
        .unwrap_or(0);

    state.set_current(uid, gid);
    // The guard is dropped here, but the global state is initialized and configured

    // Initialize daemon connection if daemon mode is enabled
    if daemon_mode_enabled() {
        let _ = init_daemon_connection();
        // Initialize daemon with our UID/GID
        let _ = daemon_init(uid, gid);
    }
}

pub use platform::*;

fn set_current_ids(uid: u32, gid: u32) -> i32 {
    if daemon_mode_enabled() && daemon_set_current_uid_gid(uid, gid) {
        return 0;
    }
    let mut state = global_state_write();
    state.set_current(uid, gid);
    0
}

fn record_ownership(path: String, ownership: FileOwnership) {
    if daemon_mode_enabled() {
        let _ = daemon_set_ownership(path, ownership);
        return;
    }
    let mut state = global_state_write();
    state.set_ownership(path, ownership);
}

fn remove_tracked_ownership(path: &str) {
    if daemon_mode_enabled() {
        let _ = daemon_remove_ownership(path);
        return;
    }
    let mut state = global_state_write();
    state.remove_ownership(path);
}

fn rename_tracked_ownership(old_path: String, new_path: String) {
    if daemon_mode_enabled() {
        if let Some(ownership) = daemon_get_ownership(&old_path) {
            let _ = daemon_set_ownership(new_path, ownership);
            let _ = daemon_remove_ownership(&old_path);
        }
        return;
    }
    let mut state = global_state_write();
    if let Some(ownership) = state.remove_ownership(&old_path) {
        state.set_ownership(new_path, ownership);
    }
}

/// Get the current fake UID
///
/// This wraps the real getuid() system call to return the fake UID.
#[unsafe(no_mangle)]
pub extern "C" fn getuid() -> u32 {
    // Check if daemon mode is enabled
    if daemon_mode_enabled() {
        if let Some((uid, _)) = daemon_get_current_uid_gid() {
            return uid;
        }
    }
    // Fall back to global state
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
    // Check if daemon mode is enabled
    if daemon_mode_enabled() {
        if let Some((_, gid)) = daemon_get_current_uid_gid() {
            return gid;
        }
    }
    // Fall back to global state
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

/// Get real, effective, and saved user IDs
#[unsafe(no_mangle)]
pub extern "C" fn getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32 {
    let current_uid = getuid();

    if !ruid.is_null() {
        unsafe {
            *ruid = current_uid;
        }
    }
    if !euid.is_null() {
        unsafe {
            *euid = current_uid;
        }
    }
    if !suid.is_null() {
        unsafe {
            *suid = current_uid;
        }
    }

    0
}

/// Get real, effective, and saved group IDs
#[unsafe(no_mangle)]
pub extern "C" fn getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32 {
    let current_gid = getgid();

    if !rgid.is_null() {
        unsafe {
            *rgid = current_gid;
        }
    }
    if !egid.is_null() {
        unsafe {
            *egid = current_gid;
        }
    }
    if !sgid.is_null() {
        unsafe {
            *sgid = current_gid;
        }
    }

    0
}

/// Set real user ID - always succeeds in fake mode
#[unsafe(no_mangle)]
pub extern "C" fn setuid(uid: u32) -> i32 {
    set_current_ids(uid, getgid())
}

/// Set real group ID - always succeeds in fake mode
#[unsafe(no_mangle)]
pub extern "C" fn setgid(gid: u32) -> i32 {
    set_current_ids(getuid(), gid)
}

/// Set real and effective user IDs
#[unsafe(no_mangle)]
pub extern "C" fn setreuid(_ruid: u32, euid: u32) -> i32 {
    set_current_ids(euid, getgid())
}

/// Set real and effective group IDs
#[unsafe(no_mangle)]
pub extern "C" fn setregid(_rgid: u32, egid: u32) -> i32 {
    set_current_ids(getuid(), egid)
}

/// Set real, effective, and saved user IDs
#[unsafe(no_mangle)]
pub extern "C" fn setresuid(_ruid: u32, euid: u32, _suid: u32) -> i32 {
    set_current_ids(euid, getgid())
}

/// Set real, effective, and saved group IDs
#[unsafe(no_mangle)]
pub extern "C" fn setresgid(_rgid: u32, egid: u32, _sgid: u32) -> i32 {
    set_current_ids(getuid(), egid)
}

/// Set filesystem user ID
#[unsafe(no_mangle)]
pub extern "C" fn setfsuid(uid: u32) -> i32 {
    set_current_ids(uid, getgid())
}

/// Set filesystem group ID
#[unsafe(no_mangle)]
pub extern "C" fn setfsgid(gid: u32) -> i32 {
    set_current_ids(getuid(), gid)
}

/// Set file ownership
///
/// This intercepts chown() to record ownership changes in our fake state.
#[unsafe(no_mangle)]
pub extern "C" fn chown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        record_ownership(path_str, FileOwnership::new(uid, gid));
    }

    unsafe { platform::real_chown(path, uid, gid) }
}

/// Get file status
///
/// This wraps stat() to return fake ownership information.
#[unsafe(no_mangle)]
pub extern "C" fn stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
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
pub extern "C" fn fstat(fd: i32, buf: *mut libc::stat) -> i32 {
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
pub extern "C" fn lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    // First, call the real lstat to get actual file info
    let result = unsafe { platform::real_lstat(path, buf) };

    if result == 0 && !buf.is_null() {
        // Successfully got file info, now modify ownership fields
        unsafe { modify_stat_ownership(path, buf) };
    }

    result
}

/// Get file status relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    // First, call the real fstatat to get actual file info
    let result = unsafe { platform::real_fstatat(dirfd, pathname, buf, flags) };

    if result == 0 && !buf.is_null() {
        // Successfully got file info, now modify ownership fields
        // For now, we use the pathname (won't work correctly for relative paths)
        unsafe { modify_stat_ownership(pathname, buf) };
    }

    result
}

/// Extended stat (Linux-specific)
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn statx(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut std::ffi::c_void,
    mask: u32,
    flags: i32,
) -> i32 {
    // For now, just pass through to the real statx
    // In a full implementation, we would modify the ownership fields in the statx buffer
    unsafe { platform::real_statx(dirfd, pathname, buf, mask, flags) }
}

/// Change file mode
#[unsafe(no_mangle)]
pub extern "C" fn chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    // Just pass through to the real chmod for now
    // In a full implementation, we might want to track file modes too
    unsafe { platform::real_chmod(path, mode) }
}

/// Change file mode by file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    // Just pass through to the real fchmod for now
    // In a full implementation, we might want to track file modes too
    unsafe { platform::real_fchmod(fd, mode) }
}

/// Change file mode relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fchmodat(dirfd: i32, path: *const c_char, mode: libc::mode_t, flags: i32) -> i32 {
    // Just pass through to the real fchmodat for now
    unsafe { platform::real_fchmodat(dirfd, path, mode, flags) }
}

/// Change file ownership by path (no symlink following)
#[unsafe(no_mangle)]
pub extern "C" fn lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        record_ownership(path_str, FileOwnership::new(uid, gid));
    }

    unsafe { platform::real_lchown(path, uid, gid) }
}

/// Change file ownership by file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    // Can't easily map fd to path, so just pass through to real fchown
    unsafe { platform::real_fchown(fd, uid, gid) }
}

/// Change file ownership relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fchownat(dirfd: i32, path: *const c_char, uid: u32, gid: u32, flags: i32) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        record_ownership(path_str, FileOwnership::new(uid, gid));
    }

    unsafe { platform::real_fchownat(dirfd, path, uid, gid, flags) }
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
    if let Some(path_str) = cstr_to_string(path) {
        if daemon_mode_enabled() {
            if let Some(ownership) = daemon_get_ownership(&path_str) {
                unsafe {
                    (*buf).st_uid = ownership.uid as libc::uid_t;
                    (*buf).st_gid = ownership.gid as libc::gid_t;
                }
                return;
            }
        } else {
            let state = global_state_read();
            if let Some(ownership) = state.get_ownership(&path_str) {
                unsafe {
                    (*buf).st_uid = ownership.uid as libc::uid_t;
                    (*buf).st_gid = ownership.gid as libc::gid_t;
                }
                return;
            }
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
        Some(CStr::from_ptr(cstr).to_string_lossy().into_owned())
    }
}

/// Remove directory entry (delete file)
#[unsafe(no_mangle)]
pub extern "C" fn unlink(path: *const c_char) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        remove_tracked_ownership(&path_str);
    }

    unsafe { platform::real_unlink(path) }
}

/// Remove directory entry relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        remove_tracked_ownership(&path_str);
    }

    unsafe { platform::real_unlinkat(dirfd, path, flags) }
}

/// Remove directory
#[unsafe(no_mangle)]
pub extern "C" fn rmdir(path: *const c_char) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(path) } {
        remove_tracked_ownership(&path_str);
    }

    unsafe { platform::real_rmdir(path) }
}

/// Rename a file
#[unsafe(no_mangle)]
pub extern "C" fn rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    if let (Some(old_str), Some(new_str)) = (unsafe { cstr_to_string(oldpath) }, unsafe {
        cstr_to_string(newpath)
    }) {
        rename_tracked_ownership(old_str, new_str);
    }

    unsafe { platform::real_rename(oldpath, newpath) }
}

/// Rename a file relative to directory file descriptors
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn renameat(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) -> i32 {
    if olddirfd == libc::AT_FDCWD && newdirfd == libc::AT_FDCWD {
        if let (Some(old_str), Some(new_str)) = (unsafe { cstr_to_string(oldpath) }, unsafe {
            cstr_to_string(newpath)
        }) {
            rename_tracked_ownership(old_str, new_str);
        }
    }

    unsafe { platform::real_renameat(olddirfd, oldpath, newdirfd, newpath) }
}

/// Rename a file relative to directory file descriptors with flags
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn renameat2(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
    flags: u32,
) -> i32 {
    if olddirfd == libc::AT_FDCWD && newdirfd == libc::AT_FDCWD {
        if let (Some(old_str), Some(new_str)) = (unsafe { cstr_to_string(oldpath) }, unsafe {
            cstr_to_string(newpath)
        }) {
            rename_tracked_ownership(old_str, new_str);
        }
    }

    unsafe { platform::real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags) }
}

/// Create a special file (FIFO, character device, block device)
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(pathname) } {
        record_ownership(path_str, FileOwnership::new(getuid(), getgid()));
    }

    unsafe { platform::real_mknod(pathname, mode, dev) }
}

/// Create a special file relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    if let Some(path_str) = unsafe { cstr_to_string(pathname) } {
        record_ownership(path_str, FileOwnership::new(getuid(), getgid()));
    }

    unsafe { platform::real_mknodat(dirfd, pathname, mode, dev) }
}

/// Set supplementary group IDs - always succeeds in fake mode
#[unsafe(no_mangle)]
pub extern "C" fn setgroups(_size: i32, _list: *const libc::gid_t) -> i32 {
    // In fake mode, we just succeed (don't actually change groups)
    0
}

/// Set capabilities - always succeeds in fake mode
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn capset(_hdrp: *const std::ffi::c_void, _data: *const std::ffi::c_void) -> i32 {
    // In fake mode, we just succeed (don't actually change capabilities)
    0
}

// xattr functions - for now, just pass through to real functions
// In a full implementation, we might want to fake xattr ownership
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    unsafe { platform::real_setxattr(path, name, value, size, flags) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lsetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    unsafe { platform::real_lsetxattr(path, name, value, size, flags) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fsetxattr(
    fd: i32,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    unsafe { platform::real_fsetxattr(fd, name, value, size, flags) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    unsafe { platform::real_getxattr(path, name, value, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    unsafe { platform::real_lgetxattr(path, name, value, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    unsafe { platform::real_fgetxattr(fd, name, value, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { platform::real_listxattr(path, list, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { platform::real_llistxattr(path, list, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { platform::real_flistxattr(fd, list, size) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn removexattr(path: *const c_char, name: *const c_char) -> i32 {
    unsafe { platform::real_removexattr(path, name) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lremovexattr(path: *const c_char, name: *const c_char) -> i32 {
    unsafe { platform::real_lremovexattr(path, name) }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fremovexattr(fd: i32, name: *const c_char) -> i32 {
    unsafe { platform::real_fremovexattr(fd, name) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cstr_to_string_null() {
        unsafe {
            let result = cstr_to_string(std::ptr::null());
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_cstr_to_string_valid() {
        use std::ffi::CString;
        unsafe {
            let c_str = CString::new("test").unwrap();
            let result = cstr_to_string(c_str.as_ptr());
            assert_eq!(result, Some("test".to_string()));
        }
    }

    #[test]
    fn test_cstr_to_string_empty() {
        use std::ffi::CString;
        unsafe {
            let c_str = CString::new("").unwrap();
            let result = cstr_to_string(c_str.as_ptr());
            assert_eq!(result, Some("".to_string()));
        }
    }
}
