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

// Type aliases for function pointers
type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> i32;
type FstatFn = unsafe extern "C" fn(i32, *mut libc::stat) -> i32;
type LstatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> i32;
type FstatatFn = unsafe extern "C" fn(i32, *const c_char, *mut libc::stat, i32) -> i32;
type StatxFn = unsafe extern "C" fn(i32, *const c_char, i32, u32, *mut std::ffi::c_void) -> i32;
type ChmodFn = unsafe extern "C" fn(*const c_char, libc::mode_t) -> i32;
type FchmodFn = unsafe extern "C" fn(i32, libc::mode_t) -> i32;
type FchmodatFn = unsafe extern "C" fn(i32, *const c_char, libc::mode_t, i32) -> i32;
type UnlinkFn = unsafe extern "C" fn(*const c_char) -> i32;
type UnlinkatFn = unsafe extern "C" fn(i32, *const c_char, i32) -> i32;
type RmdirFn = unsafe extern "C" fn(*const c_char) -> i32;
type RenameFn = unsafe extern "C" fn(*const c_char, *const c_char) -> i32;
type RenameatFn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char) -> i32;
type Renameat2Fn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char, u32) -> i32;
type MknodFn = unsafe extern "C" fn(*const c_char, libc::mode_t, libc::dev_t) -> i32;
type MknodatFn = unsafe extern "C" fn(i32, *const c_char, libc::mode_t, libc::dev_t) -> i32;

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

// Use OnceLock for thread-safe lazy initialization
static REAL_STAT: OnceLock<StatFn> = OnceLock::new();
static REAL_FSTAT: OnceLock<FstatFn> = OnceLock::new();
static REAL_LSTAT: OnceLock<LstatFn> = OnceLock::new();
static REAL_FSTATAT: OnceLock<FstatatFn> = OnceLock::new();
static REAL_STATX: OnceLock<StatxFn> = OnceLock::new();
static REAL_CHMOD: OnceLock<ChmodFn> = OnceLock::new();
static REAL_FCHMOD: OnceLock<FchmodFn> = OnceLock::new();
static REAL_FCHMODAT: OnceLock<FchmodatFn> = OnceLock::new();
static REAL_UNLINK: OnceLock<UnlinkFn> = OnceLock::new();
static REAL_UNLINKAT: OnceLock<UnlinkatFn> = OnceLock::new();
static REAL_RMDIR: OnceLock<RmdirFn> = OnceLock::new();
static REAL_RENAME: OnceLock<RenameFn> = OnceLock::new();
static REAL_RENAMEAT: OnceLock<RenameatFn> = OnceLock::new();
static REAL_RENAMEAT2: OnceLock<Renameat2Fn> = OnceLock::new();
static REAL_MKNOD: OnceLock<MknodFn> = OnceLock::new();
static REAL_MKNODAT: OnceLock<MknodatFn> = OnceLock::new();
static REAL_GETXATTR: OnceLock<GetxattrFn> = OnceLock::new();
static REAL_FGETXATTR: OnceLock<FgetxattrFn> = OnceLock::new();
static REAL_LISTXATTR: OnceLock<ListxattrFn> = OnceLock::new();
static REAL_FLISTXATTR: OnceLock<FlistxattrFn> = OnceLock::new();

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
    try_get_next_function::<T>(symbol).unwrap_or_else(|| {
        panic!(
            "Failed to find symbol {} with RTLD_NEXT",
            String::from_utf8_lossy(symbol)
        );
    })
}

/// Look up a function that may be absent on this platform (e.g. Linux-only syscalls).
unsafe fn try_get_next_function<T>(symbol: &[u8]) -> Option<T> {
    let ptr = libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr() as *const c_char);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute_copy(&ptr) })
    }
}

/// Store a dlsym result only when the symbol exists.
unsafe fn set_optional_function<T>(slot: &OnceLock<T>, symbol: &[u8]) {
    if let Some(func) = try_get_next_function::<T>(symbol) {
        let _ = slot.set(func);
    }
}

/// macOS platform helper implementation
pub struct MacosHelper;

impl MacosHelper {
    unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
        if let Some(func) = REAL_STAT.get() {
            func(path, buf)
        } else {
            libc::stat(path, buf)
        }
    }

    unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
        if let Some(func) = REAL_FSTAT.get() {
            func(fd, buf)
        } else {
            libc::fstat(fd, buf)
        }
    }

    unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
        if let Some(func) = REAL_LSTAT.get() {
            func(path, buf)
        } else {
            libc::lstat(path, buf)
        }
    }

    unsafe fn real_fstatat(
        dirfd: i32,
        pathname: *const c_char,
        buf: *mut libc::stat,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_FSTATAT.get() {
            func(dirfd, pathname, buf, flags)
        } else {
            libc::fstatat(dirfd, pathname, buf, flags)
        }
    }

    unsafe fn real_statx(
        dirfd: i32,
        pathname: *const c_char,
        flags: i32,
        mask: u32,
        buf: *mut std::ffi::c_void,
    ) -> i32 {
        if let Some(func) = REAL_STATX.get() {
            func(dirfd, pathname, flags, mask, buf)
        } else {
            libc::ENOSYS
        }
    }

    unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
        if let Some(func) = REAL_CHMOD.get() {
            func(path, mode)
        } else {
            libc::chmod(path, mode)
        }
    }

    unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
        if let Some(func) = REAL_FCHMOD.get() {
            func(fd, mode)
        } else {
            libc::fchmod(fd, mode)
        }
    }

    unsafe fn real_fchmodat(
        dirfd: i32,
        path: *const c_char,
        mode: libc::mode_t,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_FCHMODAT.get() {
            func(dirfd, path, mode, flags)
        } else {
            libc::fchmodat(dirfd, path, mode, flags)
        }
    }

    unsafe fn real_unlink(path: *const c_char) -> i32 {
        if let Some(func) = REAL_UNLINK.get() {
            func(path)
        } else {
            libc::unlink(path)
        }
    }

    unsafe fn real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
        if let Some(func) = REAL_UNLINKAT.get() {
            func(dirfd, path, flags)
        } else {
            libc::unlinkat(dirfd, path, flags)
        }
    }

    unsafe fn real_rmdir(path: *const c_char) -> i32 {
        if let Some(func) = REAL_RMDIR.get() {
            func(path)
        } else {
            libc::rmdir(path)
        }
    }

    unsafe fn real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
        if let Some(func) = REAL_RENAME.get() {
            func(oldpath, newpath)
        } else {
            libc::rename(oldpath, newpath)
        }
    }

    unsafe fn real_renameat(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
    ) -> i32 {
        if let Some(func) = REAL_RENAMEAT.get() {
            func(olddirfd, oldpath, newdirfd, newpath)
        } else {
            libc::renameat(olddirfd, oldpath, newdirfd, newpath)
        }
    }

    unsafe fn real_renameat2(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
        flags: u32,
    ) -> i32 {
        if let Some(func) = REAL_RENAMEAT2.get() {
            func(olddirfd, oldpath, newdirfd, newpath, flags)
        } else {
            // No renameat2 on this system: flags (e.g. RENAME_NOREPLACE)
            // can't be honored, but a plain renameat is the closest fallback.
            libc::renameat(olddirfd, oldpath, newdirfd, newpath)
        }
    }

    unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
        if let Some(func) = REAL_MKNOD.get() {
            func(pathname, mode, dev)
        } else {
            libc::mknod(pathname, mode, dev)
        }
    }

    unsafe fn real_mknodat(
        dirfd: i32,
        pathname: *const c_char,
        mode: libc::mode_t,
        dev: libc::dev_t,
    ) -> i32 {
        if let Some(func) = REAL_MKNODAT.get() {
            func(dirfd, pathname, mode, dev)
        } else {
            libc::mknodat(dirfd, pathname, mode, dev)
        }
    }

    unsafe fn getxattr_impl(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
        options: i32,
    ) -> i32 {
        let ret = if let Some(func) = REAL_GETXATTR.get() {
            func(path, name, value, size, 0, options)
        } else {
            libc::getxattr(path, name, value, size, 0, options)
        };
        ret.try_into().unwrap_or(-1)
    }

    unsafe fn real_getxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        Self::getxattr_impl(path, name, value, size, 0)
    }

    unsafe fn real_lgetxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        Self::getxattr_impl(path, name, value, size, libc::XATTR_NOFOLLOW)
    }

    unsafe fn real_fgetxattr(
        fd: i32,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        let ret = if let Some(func) = REAL_FGETXATTR.get() {
            func(fd, name, value, size, 0, 0)
        } else {
            libc::fgetxattr(fd, name, value, size, 0, 0)
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
            func(path, list, size, options)
        } else {
            libc::listxattr(path, list, size, options)
        };
        ret.try_into().unwrap_or(-1)
    }

    unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        Self::listxattr_impl(path, list, size, 0)
    }

    unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        Self::listxattr_impl(path, list, size, libc::XATTR_NOFOLLOW)
    }

    unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
        let ret = if let Some(func) = REAL_FLISTXATTR.get() {
            func(fd, list, size, 0)
        } else {
            libc::flistxattr(fd, list, size, 0)
        };
        ret.try_into().unwrap_or(-1)
    }
}

// Re-export the functions for use in the main lib.rs
pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    MacosHelper::real_stat(path, buf)
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    MacosHelper::real_fstat(fd, buf)
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    MacosHelper::real_lstat(path, buf)
}

pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    MacosHelper::real_fstatat(dirfd, pathname, buf, flags)
}

pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    MacosHelper::real_statx(dirfd, pathname, flags, mask, buf)
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    MacosHelper::real_chmod(path, mode)
}

pub unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    MacosHelper::real_fchmod(fd, mode)
}

pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    MacosHelper::real_fchmodat(dirfd, path, mode, flags)
}

pub unsafe fn real_unlink(path: *const c_char) -> i32 {
    MacosHelper::real_unlink(path)
}

pub unsafe fn real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    MacosHelper::real_unlinkat(dirfd, path, flags)
}

pub unsafe fn real_rmdir(path: *const c_char) -> i32 {
    MacosHelper::real_rmdir(path)
}

pub unsafe fn real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    MacosHelper::real_rename(oldpath, newpath)
}

pub unsafe fn real_renameat(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) -> i32 {
    MacosHelper::real_renameat(olddirfd, oldpath, newdirfd, newpath)
}

pub unsafe fn real_renameat2(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
    flags: u32,
) -> i32 {
    MacosHelper::real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags)
}

pub unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    MacosHelper::real_mknod(pathname, mode, dev)
}

pub unsafe fn real_mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    MacosHelper::real_mknodat(dirfd, pathname, mode, dev)
}

pub unsafe fn real_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    MacosHelper::real_getxattr(path, name, value, size)
}

pub unsafe fn real_lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    MacosHelper::real_lgetxattr(path, name, value, size)
}

pub unsafe fn real_fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    MacosHelper::real_fgetxattr(fd, name, value, size)
}

pub unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    MacosHelper::real_listxattr(path, list, size)
}

pub unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    MacosHelper::real_llistxattr(path, list, size)
}

pub unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    MacosHelper::real_flistxattr(fd, list, size)
}
