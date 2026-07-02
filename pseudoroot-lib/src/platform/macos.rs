//! macOS-specific wrappers for calling the real libc functions.
//!
//! dyld never applies `__DATA,__interpose` entries to the interposing image
//! itself (see the `interpose` module in `lib.rs`), so unlike Linux — where
//! the preloaded library must go through `dlsym(RTLD_NEXT)` to avoid binding
//! to its own exported symbols — these wrappers can call libc directly.
//!
//! Only wraps the real functions actually consulted by `ownership.rs`,
//! `inode.rs`, and `lib.rs` on this platform (credential and chown syscalls
//! are fully faked and never call through, so they have no `real_*`
//! counterpart here — see `linux.rs` for the same reasoning there).
//!
//! Darwin has no `l`-prefixed xattr variants (`lgetxattr`, `llistxattr`, …):
//! the plain functions take an `options` flag (`XATTR_NOFOLLOW`) instead,
//! plus a resource-fork `position` argument we always pass as 0.

use std::os::raw::c_char;

pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    unsafe { libc::stat(path, buf) }
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    unsafe { libc::fstat(fd, buf) }
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    unsafe { libc::lstat(path, buf) }
}

pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    unsafe { libc::fstatat(dirfd, pathname, buf, flags) }
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    unsafe { libc::chmod(path, mode) }
}

pub unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    unsafe { libc::fchmod(fd, mode) }
}

pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    unsafe { libc::fchmodat(dirfd, path, mode, flags) }
}

pub unsafe fn real_unlink(path: *const c_char) -> i32 {
    unsafe { libc::unlink(path) }
}

pub unsafe fn real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    unsafe { libc::unlinkat(dirfd, path, flags) }
}

pub unsafe fn real_rmdir(path: *const c_char) -> i32 {
    unsafe { libc::rmdir(path) }
}

pub unsafe fn real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    unsafe { libc::rename(oldpath, newpath) }
}

pub unsafe fn real_renameat(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) -> i32 {
    unsafe { libc::renameat(olddirfd, oldpath, newdirfd, newpath) }
}

pub unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    unsafe { libc::mknod(pathname, mode, dev) }
}

pub unsafe fn real_mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    unsafe { libc::mknodat(dirfd, pathname, mode, dev) }
}

unsafe fn getxattr_impl(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
    options: i32,
) -> i32 {
    let ret = unsafe { libc::getxattr(path, name, value, size, 0, options) };
    ret.try_into().unwrap_or(-1)
}

pub unsafe fn real_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    unsafe { getxattr_impl(path, name, value, size, 0) }
}

pub unsafe fn real_lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    unsafe { getxattr_impl(path, name, value, size, libc::XATTR_NOFOLLOW) }
}

pub unsafe fn real_fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    let ret = unsafe { libc::fgetxattr(fd, name, value, size, 0, 0) };
    ret.try_into().unwrap_or(-1)
}

unsafe fn listxattr_impl(
    path: *const c_char,
    list: *mut c_char,
    size: libc::size_t,
    options: i32,
) -> i32 {
    let ret = unsafe { libc::listxattr(path, list, size, options) };
    ret.try_into().unwrap_or(-1)
}

pub unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { listxattr_impl(path, list, size, 0) }
}

pub unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { listxattr_impl(path, list, size, libc::XATTR_NOFOLLOW) }
}

pub unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    let ret = unsafe { libc::flistxattr(fd, list, size, 0) };
    ret.try_into().unwrap_or(-1)
}
