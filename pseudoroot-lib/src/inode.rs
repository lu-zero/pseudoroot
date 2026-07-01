//! Inode identity helpers for ownership tracking.

use crate::platform;
use pseudoroot_core::state::InodeKey;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

/// Sentinel meaning "leave this id unchanged" in chown(2).
pub const ID_UNCHANGED: u32 = u32::MAX;

/// `AT_EMPTY_PATH` doesn't exist on Darwin (no `/proc/self/fd` idiom to pair
/// it with); treat it as a no-op bit there instead of the real flag.
#[cfg(target_os = "linux")]
const AT_EMPTY_PATH: i32 = libc::AT_EMPTY_PATH;
#[cfg(not(target_os = "linux"))]
const AT_EMPTY_PATH: i32 = 0;

/// Build an inode key from a `stat` buffer.
///
/// `st_dev` is already `u64` on Linux (the cast is a no-op there) but `i32`
/// on Darwin, so the cast is load-bearing on macOS even though clippy can
/// only see the Linux build.
#[inline]
#[must_use]
#[allow(clippy::unnecessary_cast)]
pub fn key_from_stat(st: &libc::stat) -> InodeKey {
    (st.st_dev as u64, st.st_ino)
}

/// `fstatat` wrapper returning the stat buffer or errno.
pub fn stat_at(dirfd: i32, path: *const c_char, flags: i32) -> Result<libc::stat, i32> {
    let mut st = unsafe { std::mem::zeroed() };
    let pathname = if path.is_null() { c"".as_ptr() } else { path };
    let rc = unsafe { platform::real_fstatat(dirfd, pathname, &mut st, flags) };
    if rc == 0 {
        Ok(st)
    } else {
        Err(rc)
    }
}

/// Stat a path relative to `AT_FDCWD`.
pub fn stat_path(path: *const c_char) -> Result<libc::stat, i32> {
    stat_at(libc::AT_FDCWD, path, 0)
}

/// Lstat a path relative to `AT_FDCWD`.
pub fn lstat_path(path: *const c_char) -> Result<libc::stat, i32> {
    stat_at(libc::AT_FDCWD, path, libc::AT_SYMLINK_NOFOLLOW)
}

/// `fstat` wrapper returning the stat buffer or errno.
pub fn fstat_fd(fd: i32) -> Result<libc::stat, i32> {
    let mut st = unsafe { std::mem::zeroed() };
    let rc = unsafe { platform::real_fstat(fd, &mut st) };
    if rc == 0 {
        Ok(st)
    } else {
        Err(rc)
    }
}

/// Chown flags for `*at` syscalls mapped to `fstatat` flags.
#[inline]
#[must_use]
pub fn chown_stat_flags(at_flags: i32) -> i32 {
    if at_flags & libc::AT_SYMLINK_NOFOLLOW != 0 {
        libc::AT_SYMLINK_NOFOLLOW
    } else {
        0
    }
}

/// Whether `AT_EMPTY_PATH` is set in `*at` syscall flags.
#[inline]
#[must_use]
pub fn at_empty_path(at_flags: i32) -> bool {
    at_flags & AT_EMPTY_PATH != 0
}

/// Resolve a `(dirfd, path)` pair to an absolute filesystem path.
#[must_use]
pub fn resolve_path_at(dirfd: i32, path: *const c_char) -> Option<PathBuf> {
    if path.is_null() {
        return None;
    }
    let c_path = unsafe { CStr::from_ptr(path) };
    if c_path.to_bytes().is_empty() {
        return None;
    }
    let path_str = c_path.to_str().ok()?;
    if dirfd == libc::AT_FDCWD || path_str.starts_with('/') {
        return Some(PathBuf::from(path_str));
    }
    #[cfg(target_os = "linux")]
    {
        Some(PathBuf::from(format!("/proc/self/fd/{dirfd}/{path_str}")))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Stat a filesystem path represented as a [`PathBuf`].
pub fn stat_path_buf(path: &std::path::Path) -> Result<libc::stat, i32> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| -libc::EINVAL)?;
    stat_path(c_path.as_ptr())
}

/// Resolve the `fstatat` flags for a `(dirfd, path, at_flags)` triple.
#[inline]
#[must_use]
pub fn resolve_stat_flags(dirfd: i32, path: *const c_char, at_flags: i32) -> i32 {
    let mut flags = chown_stat_flags(at_flags);
    if at_empty_path(at_flags) {
        flags |= AT_EMPTY_PATH;
        return flags;
    }
    if path.is_null() {
        return flags;
    }
    let empty = unsafe { CStr::from_ptr(path) }.to_bytes().is_empty();
    if empty && dirfd != libc::AT_FDCWD {
        flags |= AT_EMPTY_PATH;
    }
    flags
}
