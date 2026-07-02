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
//! - Inode-keyed file ownership information
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

// On macOS the `*at`/xattr/mknod interposition surface is still Linux-gated
// (see todo/macos.md item 1), which leaves their ownership/inode helpers
// compiled but unreferenced there.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod inode;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod ownership;
mod platform;

use ownership::{
    current_fake_gid, current_fake_uid, maybe_remove_inode_path, modify_stat_buf,
    prepare_rename_overwrite, record_chmod_fd, record_chmod_path, record_chown_fd,
    record_chown_path, set_current_ids, set_fsuid, setfsgid as set_fake_fsgid,
};
#[cfg(target_os = "linux")]
use ownership::{
    fake_getxattr_fd, fake_getxattr_path, fake_listxattr_fd, fake_listxattr_path, fake_mknod_path,
    fake_mknodat, fake_removexattr_fd, fake_removexattr_path, fake_setxattr_fd, fake_setxattr_path,
    maybe_remove_inode_at, record_chmod_at, record_chown_at,
};
use std::ffi::CStr;
use std::os::raw::c_char;

/// Initialize the pseudoroot library
///
/// This function is called automatically when the library is loaded,
/// thanks to the `ctor` crate.
#[ctor::ctor]
unsafe fn init() {
    let uid = std::env::var("PSEUDOROOT_UID")
        .ok()
        .and_then(|u| u.parse::<u32>().ok())
        .unwrap_or(0);
    let gid = std::env::var("PSEUDOROOT_GID")
        .ok()
        .and_then(|g| g.parse::<u32>().ok())
        .unwrap_or(0);
    ownership::store_bootstrap_ids(uid, gid);
}

pub use platform::*;

/// Get the current fake UID
///
/// This wraps the real getuid() system call to return the fake UID.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn getuid() -> u32 {
    current_fake_uid()
}

/// Get the current effective UID
///
/// This wraps the real geteuid() system call to return the fake effective UID.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn geteuid() -> u32 {
    current_fake_uid()
}

/// Get the current GID
///
/// This wraps the real getgid() system call to return the fake GID.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn getgid() -> u32 {
    current_fake_gid()
}

/// Get the current effective GID
///
/// This wraps the real getegid() system call to return the fake effective GID.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn getegid() -> u32 {
    current_fake_gid()
}

/// Get real, effective, and saved user IDs
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
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
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
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
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setuid(uid: u32) -> i32 {
    set_current_ids(uid, getgid())
}

/// Set real group ID - always succeeds in fake mode
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setgid(gid: u32) -> i32 {
    set_current_ids(getuid(), gid)
}

/// Set real and effective user IDs
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setreuid(_ruid: u32, euid: u32) -> i32 {
    set_current_ids(euid, getgid())
}

/// Set real and effective group IDs
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setregid(_rgid: u32, egid: u32) -> i32 {
    set_current_ids(getuid(), egid)
}

/// Set real, effective, and saved user IDs
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setresuid(_ruid: u32, euid: u32, _suid: u32) -> i32 {
    set_current_ids(euid, getgid())
}

/// Set real, effective, and saved group IDs
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setresgid(_rgid: u32, egid: u32, _sgid: u32) -> i32 {
    set_current_ids(getuid(), egid)
}

/// Set filesystem user ID
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setfsuid(uid: u32) -> i32 {
    set_fsuid(uid) as i32
}

/// Set filesystem group ID
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setfsgid(gid: u32) -> i32 {
    set_fake_fsgid(gid) as i32
}

/// Set file ownership
///
/// This intercepts chown() to record ownership changes in our fake state.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn chown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    record_chown_path(path, false, uid, gid)
}

/// Get file status
///
/// This wraps stat() to return fake ownership information.
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    let result = unsafe { platform::real_stat(path, buf) };

    if result == 0 && !buf.is_null() {
        unsafe { modify_stat_buf(buf) };
    }

    result
}

/// Get file status for a file descriptor
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    let result = unsafe { platform::real_fstat(fd, buf) };

    if result == 0 && !buf.is_null() {
        unsafe { modify_stat_buf(buf) };
    }

    result
}

/// Get file status for a path with symbolic link following
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    let result = unsafe { platform::real_lstat(path, buf) };

    if result == 0 && !buf.is_null() {
        unsafe { modify_stat_buf(buf) };
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
    let result = unsafe { platform::real_fstatat(dirfd, pathname, buf, flags) };

    if result == 0 && !buf.is_null() {
        unsafe { modify_stat_buf(buf) };
    }

    result
}

/// Extended stat (Linux-specific)
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    let result = unsafe { platform::real_statx(dirfd, pathname, flags, mask, buf) };

    if result == 0 && !buf.is_null() {
        unsafe { ownership::modify_statx_buf(buf) };
    }

    result
}

/// Change file mode
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    record_chmod_path(path, mode)
}

/// Change file mode by file descriptor
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    record_chmod_fd(fd, mode)
}

/// Change file mode relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fchmodat(dirfd: i32, path: *const c_char, mode: libc::mode_t, flags: i32) -> i32 {
    record_chmod_at(dirfd, path, mode, flags)
}

/// Change file ownership by path (no symlink following)
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    record_chown_path(path, true, uid, gid)
}

/// Change file ownership by file descriptor
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    record_chown_fd(fd, uid, gid)
}

/// Change file ownership relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fchownat(dirfd: i32, path: *const c_char, uid: u32, gid: u32, flags: i32) -> i32 {
    record_chown_at(dirfd, path, flags, uid, gid)
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
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn unlink(path: *const c_char) -> i32 {
    maybe_remove_inode_path(path);
    unsafe { platform::real_unlink(path) }
}

/// Remove directory entry relative to directory file descriptor
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    maybe_remove_inode_at(dirfd, path, flags);
    unsafe { platform::real_unlinkat(dirfd, path, flags) }
}

/// Remove directory
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn rmdir(path: *const c_char) -> i32 {
    maybe_remove_inode_path(path);
    unsafe { platform::real_rmdir(path) }
}

/// Rename a file
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    prepare_rename_overwrite(libc::AT_FDCWD, oldpath, libc::AT_FDCWD, newpath);
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
    prepare_rename_overwrite(olddirfd, oldpath, newdirfd, newpath);
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
    prepare_rename_overwrite(olddirfd, oldpath, newdirfd, newpath);
    unsafe { platform::real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags) }
}

/// Create a special file (FIFO, character device, block device)
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    fake_mknod_path(pathname, mode, dev)
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
    fake_mknodat(dirfd, pathname, mode, dev)
}

/// Set supplementary group IDs - always succeeds in fake mode
#[cfg_attr(target_os = "linux", unsafe(no_mangle))]
pub extern "C" fn setgroups(_size: libc::size_t, _list: *const libc::gid_t) -> i32 {
    0
}

/// Set capabilities - always succeeds in fake mode
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn capset(_hdrp: *const std::ffi::c_void, _data: *const std::ffi::c_void) -> i32 {
    0
}

// xattr functions — fake security.capability and other xattrs in the inode table
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    _flags: i32,
) -> i32 {
    fake_setxattr_path(path, name, value, size, false)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lsetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    _flags: i32,
) -> i32 {
    fake_setxattr_path(path, name, value, size, true)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fsetxattr(
    fd: i32,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    _flags: i32,
) -> i32 {
    fake_setxattr_fd(fd, name, value, size)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    fake_getxattr_path(path, name, value, size, false)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    fake_getxattr_path(path, name, value, size, true)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    fake_getxattr_fd(fd, name, value, size)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    fake_listxattr_path(path, list, size, false)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    fake_listxattr_path(path, list, size, true)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    fake_listxattr_fd(fd, list, size)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn removexattr(path: *const c_char, name: *const c_char) -> i32 {
    fake_removexattr_path(path, name, false)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn lremovexattr(path: *const c_char, name: *const c_char) -> i32 {
    fake_removexattr_path(path, name, true)
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn fremovexattr(fd: i32, name: *const c_char) -> i32 {
    fake_removexattr_fd(fd, name)
}

/// dyld interposition table for macOS.
///
/// On Darwin, `DYLD_INSERT_LIBRARIES` alone does not rebind anything: the
/// two-level namespace binds every image's libc calls straight to libSystem,
/// so exporting a symbol named `stat` (the `LD_PRELOAD` trick) has no effect.
/// Interposition instead happens through `__DATA,__interpose`, a section of
/// `(replacement, replacee)` pointer pairs that dyld processes at load time.
///
/// Two consequences shape the code above and in `platform::macos`:
/// - this image must *not* export the libc names (`no_mangle` is Linux-only),
///   otherwise the replacee relocation would bind to our own definition;
/// - dyld never applies interposition to the interposing image itself, so
///   `real_*` wrappers can call libc directly without `dlsym(RTLD_NEXT)`.
///
/// Taking the *address* of the `libc` crate's declaration (rather than naming
/// the symbol) also picks up `$INODE64`-suffixed variants on x86_64 for free.
///
/// `getresuid`/`getresgid`, `setresuid`/`setresgid`, and `setfsuid`/`setfsgid`
/// have no Darwin counterpart in libSystem, so they get no entry here.
#[cfg(target_os = "macos")]
mod interpose {
    /// One `(replacement, replacee)` pair in the `__interpose` section.
    #[repr(C)]
    struct InterposeEntry {
        replacement: *const (),
        replacee: *const (),
    }

    // SAFETY: the entries are immutable function addresses only read by dyld.
    unsafe impl Sync for InterposeEntry {}

    macro_rules! interpose {
        ($($entry:ident: $replacee:path => $replacement:path;)+) => {
            $(
                #[used]
                #[unsafe(link_section = "__DATA,__interpose")]
                static $entry: InterposeEntry = InterposeEntry {
                    replacement: $replacement as *const (),
                    replacee: $replacee as *const (),
                };
            )+
        };
    }

    interpose! {
        GETUID: libc::getuid => super::getuid;
        GETEUID: libc::geteuid => super::geteuid;
        GETGID: libc::getgid => super::getgid;
        GETEGID: libc::getegid => super::getegid;
        SETUID: libc::setuid => super::setuid;
        SETGID: libc::setgid => super::setgid;
        SETREUID: libc::setreuid => super::setreuid;
        SETREGID: libc::setregid => super::setregid;
        // Darwin's setgroups takes `c_int`; ours takes `size_t` but ignores
        // the argument, so the width mismatch is inconsequential.
        SETGROUPS: libc::setgroups => super::setgroups;
        CHOWN: libc::chown => super::chown;
        LCHOWN: libc::lchown => super::lchown;
        FCHOWN: libc::fchown => super::fchown;
        STAT: libc::stat => super::stat;
        FSTAT: libc::fstat => super::fstat;
        LSTAT: libc::lstat => super::lstat;
        CHMOD: libc::chmod => super::chmod;
        FCHMOD: libc::fchmod => super::fchmod;
        UNLINK: libc::unlink => super::unlink;
        RMDIR: libc::rmdir => super::rmdir;
        RENAME: libc::rename => super::rename;
    }
}
