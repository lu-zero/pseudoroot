//! macOS-specific implementation for library interposition
//!
//! This module provides macOS-specific implementations using `dlsym(RTLD_NEXT)`
//! to call the real system functions.
//!
//! Only wraps the real functions actually consulted by `ownership.rs`/`lib.rs`
//! (credential and chown syscalls are fully faked and never call through, so
//! they have no `real_*` counterpart here — see `linux.rs` for the same
//! reasoning on that platform).

use std::os::raw::c_char;
use std::sync::OnceLock;

/// Define a `real_*` wrapper around a dlsym'd function pointer stored in a
/// module-level static, falling back to `$fallback` if the ctor below hasn't
/// resolved it (e.g. dlsym failed).
macro_rules! real_fn {
    ($name:ident, $ty:ident, $static:ident($($arg:ident: $argty:ty),* $(,)?) -> $ret:ty, $fallback:expr) => {
        type $ty = unsafe extern "C" fn($($argty),*) -> $ret;
        static $static: OnceLock<$ty> = OnceLock::new();

        pub unsafe fn $name($($arg: $argty),*) -> $ret {
            if let Some(func) = $static.get() {
                unsafe { func($($arg),*) }
            } else {
                $fallback
            }
        }
    };
}

real_fn!(real_stat, StatFn, REAL_STAT(path: *const c_char, buf: *mut libc::stat) -> i32,
    unsafe { libc::stat(path, buf) });
real_fn!(real_fstat, FstatFn, REAL_FSTAT(fd: i32, buf: *mut libc::stat) -> i32,
    unsafe { libc::fstat(fd, buf) });
real_fn!(real_lstat, LstatFn, REAL_LSTAT(path: *const c_char, buf: *mut libc::stat) -> i32,
    unsafe { libc::lstat(path, buf) });
real_fn!(real_fstatat, FstatatFn, REAL_FSTATAT(dirfd: i32, pathname: *const c_char, buf: *mut libc::stat, flags: i32) -> i32,
    unsafe { libc::fstatat(dirfd, pathname, buf, flags) });
real_fn!(real_chmod, ChmodFn, REAL_CHMOD(path: *const c_char, mode: libc::mode_t) -> i32,
    unsafe { libc::chmod(path, mode) });
real_fn!(real_fchmod, FchmodFn, REAL_FCHMOD(fd: i32, mode: libc::mode_t) -> i32,
    unsafe { libc::fchmod(fd, mode) });
real_fn!(real_fchmodat, FchmodatFn, REAL_FCHMODAT(dirfd: i32, path: *const c_char, mode: libc::mode_t, flags: i32) -> i32,
    unsafe { libc::fchmodat(dirfd, path, mode, flags) });
real_fn!(real_unlink, UnlinkFn, REAL_UNLINK(path: *const c_char) -> i32,
    unsafe { libc::unlink(path) });
real_fn!(real_unlinkat, UnlinkatFn, REAL_UNLINKAT(dirfd: i32, path: *const c_char, flags: i32) -> i32,
    unsafe { libc::unlinkat(dirfd, path, flags) });
real_fn!(real_rmdir, RmdirFn, REAL_RMDIR(path: *const c_char) -> i32,
    unsafe { libc::rmdir(path) });
real_fn!(real_rename, RenameFn, REAL_RENAME(oldpath: *const c_char, newpath: *const c_char) -> i32,
    unsafe { libc::rename(oldpath, newpath) });
real_fn!(real_renameat, RenameatFn, REAL_RENAMEAT(olddirfd: i32, oldpath: *const c_char, newdirfd: i32, newpath: *const c_char) -> i32,
    unsafe { libc::renameat(olddirfd, oldpath, newdirfd, newpath) });
real_fn!(real_mknod, MknodFn, REAL_MKNOD(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32,
    unsafe { libc::mknod(pathname, mode, dev) });
real_fn!(real_mknodat, MknodatFn, REAL_MKNODAT(dirfd: i32, pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32,
    unsafe { libc::mknodat(dirfd, pathname, mode, dev) });

// statx and renameat2 aren't available on all macOS versions, so they're
// resolved "optionally" below (no panic if dlsym can't find them) and fall
// back to ENOSYS / a plain renameat respectively rather than a libc call.
type StatxFn = unsafe extern "C" fn(i32, *const c_char, i32, u32, *mut std::ffi::c_void) -> i32;
type Renameat2Fn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char, u32) -> i32;
static REAL_STATX: OnceLock<StatxFn> = OnceLock::new();
static REAL_RENAMEAT2: OnceLock<Renameat2Fn> = OnceLock::new();

pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    if let Some(func) = REAL_STATX.get() {
        unsafe { func(dirfd, pathname, flags, mask, buf) }
    } else {
        libc::ENOSYS
    }
}

pub unsafe fn real_renameat2(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
    flags: u32,
) -> i32 {
    if let Some(func) = REAL_RENAMEAT2.get() {
        unsafe { func(olddirfd, oldpath, newdirfd, newpath, flags) }
    } else {
        // No renameat2 on this system: flags (e.g. RENAME_NOREPLACE)
        // can't be honored, but a plain renameat is the closest fallback.
        unsafe { libc::renameat(olddirfd, oldpath, newdirfd, newpath) }
    }
}

// xattr functions. Darwin has no `l`-prefixed variants (`lgetxattr`,
// `llistxattr`, ...) -- the plain function takes an extra `options` flag
// (`XATTR_NOFOLLOW`) instead, plus a resource-fork `position` argument we
// always pass as 0. `real_lgetxattr`/`real_llistxattr` below reuse these same
// dlsym'd pointers with that flag set rather than looking up a symbol that
// doesn't exist on this platform.
type GetxattrFn = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *mut std::ffi::c_void,
    libc::size_t,
    u32,
    i32,
) -> isize;
type FgetxattrFn = unsafe extern "C" fn(
    i32,
    *const c_char,
    *mut std::ffi::c_void,
    libc::size_t,
    u32,
    i32,
) -> isize;
type ListxattrFn = unsafe extern "C" fn(*const c_char, *mut c_char, libc::size_t, i32) -> isize;
type FlistxattrFn = unsafe extern "C" fn(i32, *mut c_char, libc::size_t, i32) -> isize;

static REAL_GETXATTR: OnceLock<GetxattrFn> = OnceLock::new();
static REAL_FGETXATTR: OnceLock<FgetxattrFn> = OnceLock::new();
static REAL_LISTXATTR: OnceLock<ListxattrFn> = OnceLock::new();
static REAL_FLISTXATTR: OnceLock<FlistxattrFn> = OnceLock::new();

unsafe fn getxattr_impl(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
    options: i32,
) -> i32 {
    let ret = if let Some(func) = REAL_GETXATTR.get() {
        unsafe { func(path, name, value, size, 0, options) }
    } else {
        unsafe { libc::getxattr(path, name, value, size, 0, options) }
    };
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
    let ret = if let Some(func) = REAL_FGETXATTR.get() {
        unsafe { func(fd, name, value, size, 0, 0) }
    } else {
        unsafe { libc::fgetxattr(fd, name, value, size, 0, 0) }
    };
    ret.try_into().unwrap_or(-1)
}

unsafe fn listxattr_impl(
    path: *const c_char,
    list: *mut c_char,
    size: libc::size_t,
    options: i32,
) -> i32 {
    let ret = if let Some(func) = REAL_LISTXATTR.get() {
        unsafe { func(path, list, size, options) }
    } else {
        unsafe { libc::listxattr(path, list, size, options) }
    };
    ret.try_into().unwrap_or(-1)
}

pub unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { listxattr_impl(path, list, size, 0) }
}

pub unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    unsafe { listxattr_impl(path, list, size, libc::XATTR_NOFOLLOW) }
}

pub unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    let ret = if let Some(func) = REAL_FLISTXATTR.get() {
        unsafe { func(fd, list, size, 0) }
    } else {
        unsafe { libc::flistxattr(fd, list, size, 0) }
    };
    ret.try_into().unwrap_or(-1)
}

/// Initialize the function pointers by looking up the real functions
#[ctor::ctor]
fn init() {
    unsafe {
        REAL_STAT.set(get_next_function::<StatFn>(b"stat\0")).ok();
        REAL_FSTAT
            .set(get_next_function::<FstatFn>(b"fstat\0"))
            .ok();
        REAL_LSTAT
            .set(get_next_function::<LstatFn>(b"lstat\0"))
            .ok();
        REAL_FSTATAT
            .set(get_next_function::<FstatatFn>(b"fstatat\0"))
            .ok();
        set_optional_function(&REAL_STATX, b"statx\0");
        REAL_CHMOD
            .set(get_next_function::<ChmodFn>(b"chmod\0"))
            .ok();
        REAL_FCHMOD
            .set(get_next_function::<FchmodFn>(b"fchmod\0"))
            .ok();
        REAL_FCHMODAT
            .set(get_next_function::<FchmodatFn>(b"fchmodat\0"))
            .ok();
        REAL_UNLINK
            .set(get_next_function::<UnlinkFn>(b"unlink\0"))
            .ok();
        REAL_UNLINKAT
            .set(get_next_function::<UnlinkatFn>(b"unlinkat\0"))
            .ok();
        REAL_RMDIR
            .set(get_next_function::<RmdirFn>(b"rmdir\0"))
            .ok();
        REAL_RENAME
            .set(get_next_function::<RenameFn>(b"rename\0"))
            .ok();
        REAL_RENAMEAT
            .set(get_next_function::<RenameatFn>(b"renameat\0"))
            .ok();
        set_optional_function(&REAL_RENAMEAT2, b"renameat2\0");
        REAL_MKNOD
            .set(get_next_function::<MknodFn>(b"mknod\0"))
            .ok();
        REAL_MKNODAT
            .set(get_next_function::<MknodatFn>(b"mknodat\0"))
            .ok();
        REAL_GETXATTR
            .set(get_next_function::<GetxattrFn>(b"getxattr\0"))
            .ok();
        REAL_FGETXATTR
            .set(get_next_function::<FgetxattrFn>(b"fgetxattr\0"))
            .ok();
        REAL_LISTXATTR
            .set(get_next_function::<ListxattrFn>(b"listxattr\0"))
            .ok();
        REAL_FLISTXATTR
            .set(get_next_function::<FlistxattrFn>(b"flistxattr\0"))
            .ok();
    }
}

/// Helper function to look up a function using dlsym(RTLD_NEXT)
unsafe fn get_next_function<T>(symbol: &[u8]) -> T {
    unsafe { try_get_next_function::<T>(symbol) }.unwrap_or_else(|| {
        panic!(
            "Failed to find symbol {} with RTLD_NEXT",
            String::from_utf8_lossy(symbol)
        );
    })
}

/// Look up a function that may be absent on this platform (e.g. Linux-only syscalls).
unsafe fn try_get_next_function<T>(symbol: &[u8]) -> Option<T> {
    let ptr = unsafe { libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr() as *const c_char) };
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute_copy(&ptr) })
    }
}

/// Store a dlsym result only when the symbol exists.
unsafe fn set_optional_function<T>(slot: &OnceLock<T>, symbol: &[u8]) {
    if let Some(func) = unsafe { try_get_next_function::<T>(symbol) } {
        let _ = slot.set(func);
    }
}
