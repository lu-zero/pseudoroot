//! Linux-specific implementation for library interposition
//!
//! This module provides Linux-specific implementations using `dlsym(RTLD_NEXT)`
//! to call the real system functions.
//!
//! Only wraps the real functions actually consulted by `ownership.rs`/`lib.rs`
//! (credential and chown syscalls are fully faked and never call through, so
//! they have no `real_*` counterpart here — see `macos.rs` for the same
//! reasoning on that platform).
//!
//! Note: stat/fstat/lstat/fstatat/statx/chmod/fchmod/fchmodat always go
//! straight to a raw syscall (dlsym(RTLD_NEXT) can resolve back into our own
//! hooks for these), so they need no `REAL_*` static or dlsym lookup — unlike
//! the rest of this file's functions, which fall back to a dlsym'd function
//! pointer.

use crate::ownership;
use std::os::raw::c_char;
use std::sync::{Once, OnceLock};

// Type aliases for function pointers
type UnlinkFn = unsafe extern "C" fn(*const c_char) -> i32;
type UnlinkatFn = unsafe extern "C" fn(i32, *const c_char, i32) -> i32;
type RmdirFn = unsafe extern "C" fn(*const c_char) -> i32;
type RenameFn = unsafe extern "C" fn(*const c_char, *const c_char) -> i32;
type RenameatFn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char) -> i32;
type Renameat2Fn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char, u32) -> i32;
type MknodFn = unsafe extern "C" fn(*const c_char, libc::mode_t, libc::dev_t) -> i32;
type MknodatFn = unsafe extern "C" fn(i32, *const c_char, libc::mode_t, libc::dev_t) -> i32;

// xattr function type aliases
type GetxattrFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type LgetxattrFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type FgetxattrFn =
    unsafe extern "C" fn(i32, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type ListxattrFn = unsafe extern "C" fn(*const c_char, *mut c_char, libc::size_t) -> i32;
type LlistxattrFn = unsafe extern "C" fn(*const c_char, *mut c_char, libc::size_t) -> i32;
type FlistxattrFn = unsafe extern "C" fn(i32, *mut c_char, libc::size_t) -> i32;

// Use OnceLock for thread-safe lazy initialization
static REAL_UNLINK: OnceLock<UnlinkFn> = OnceLock::new();
static REAL_UNLINKAT: OnceLock<UnlinkatFn> = OnceLock::new();
static REAL_RMDIR: OnceLock<RmdirFn> = OnceLock::new();
static REAL_RENAME: OnceLock<RenameFn> = OnceLock::new();
static REAL_RENAMEAT: OnceLock<RenameatFn> = OnceLock::new();
static REAL_RENAMEAT2: OnceLock<Renameat2Fn> = OnceLock::new();
static REAL_MKNOD: OnceLock<MknodFn> = OnceLock::new();
static REAL_MKNODAT: OnceLock<MknodatFn> = OnceLock::new();
static REAL_GETXATTR: OnceLock<GetxattrFn> = OnceLock::new();
static REAL_LGETXATTR: OnceLock<LgetxattrFn> = OnceLock::new();
static REAL_FGETXATTR: OnceLock<FgetxattrFn> = OnceLock::new();
static REAL_LISTXATTR: OnceLock<ListxattrFn> = OnceLock::new();
static REAL_LLISTXATTR: OnceLock<LlistxattrFn> = OnceLock::new();
static REAL_FLISTXATTR: OnceLock<FlistxattrFn> = OnceLock::new();

static REAL_FUNCS_INIT: Once = Once::new();

fn ensure_real_funcs() {
    if !ownership::library_init_done() {
        return;
    }
    REAL_FUNCS_INIT.call_once(|| unsafe {
        init_real_funcs();
    });
}

/// Initialize the function pointers by looking up the real functions
unsafe fn init_real_funcs() {
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
    REAL_RENAMEAT2
        .set(get_next_function::<Renameat2Fn>(b"renameat2\0"))
        .ok();
    REAL_MKNOD
        .set(get_next_function::<MknodFn>(b"mknod\0"))
        .ok();
    REAL_MKNODAT
        .set(get_next_function::<MknodatFn>(b"mknodat\0"))
        .ok();
    REAL_GETXATTR
        .set(get_next_function::<GetxattrFn>(b"getxattr\0"))
        .ok();
    REAL_LGETXATTR
        .set(get_next_function::<LgetxattrFn>(b"lgetxattr\0"))
        .ok();
    REAL_FGETXATTR
        .set(get_next_function::<FgetxattrFn>(b"fgetxattr\0"))
        .ok();
    REAL_LISTXATTR
        .set(get_next_function::<ListxattrFn>(b"listxattr\0"))
        .ok();
    REAL_LLISTXATTR
        .set(get_next_function::<LlistxattrFn>(b"llistxattr\0"))
        .ok();
    REAL_FLISTXATTR
        .set(get_next_function::<FlistxattrFn>(b"flistxattr\0"))
        .ok();
}

/// Helper function to look up a function using dlsym(RTLD_NEXT)
unsafe fn get_next_function<T>(symbol: &[u8]) -> T {
    let handle = libc::RTLD_NEXT;
    let ptr = libc::dlsym(handle, symbol.as_ptr() as *const c_char);
    if ptr.is_null() {
        panic!(
            "Failed to find symbol {} with RTLD_NEXT",
            String::from_utf8_lossy(symbol)
        );
    }
    // SAFETY: We're casting a function pointer from c_void to the specific function type
    // This is valid because we know the symbol exists and has the correct signature
    unsafe { std::mem::transmute_copy(&ptr) }
}

/// Linux platform helper implementation
pub struct LinuxHelper;

impl LinuxHelper {
    unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
        // Always syscall: dlsym(RTLD_NEXT) can resolve back into our own hooks.
        libc::syscall(libc::SYS_newfstatat, libc::AT_FDCWD, path, buf, 0) as i32
    }

    unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
        libc::syscall(libc::SYS_fstat, fd, buf) as i32
    }

    unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
        libc::syscall(
            libc::SYS_newfstatat,
            libc::AT_FDCWD,
            path,
            buf,
            libc::AT_SYMLINK_NOFOLLOW,
        ) as i32
    }

    unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
        libc::syscall(libc::SYS_fchmodat, libc::AT_FDCWD, path, mode, 0) as i32
    }

    unsafe fn real_fstatat(
        dirfd: i32,
        pathname: *const c_char,
        buf: *mut libc::stat,
        flags: i32,
    ) -> i32 {
        libc::syscall(libc::SYS_newfstatat, dirfd, pathname, buf, flags) as i32
    }

    unsafe fn real_statx(
        dirfd: i32,
        pathname: *const c_char,
        flags: i32,
        mask: u32,
        buf: *mut std::ffi::c_void,
    ) -> i32 {
        // Always use the syscall directly: calling libc::statx or dlsym(statx) would
        // recurse through our hook.
        libc::syscall(libc::SYS_statx, dirfd, pathname, flags, mask, buf) as i32
    }

    unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
        libc::syscall(libc::SYS_fchmod, fd, mode) as i32
    }

    unsafe fn real_fchmodat(
        dirfd: i32,
        path: *const c_char,
        mode: libc::mode_t,
        flags: i32,
    ) -> i32 {
        libc::syscall(libc::SYS_fchmodat, dirfd, path, mode, flags) as i32
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
            libc::renameat2(olddirfd, oldpath, newdirfd, newpath, flags)
        }
    }

    unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
        if let Some(func) = REAL_MKNOD.get() {
            func(pathname, mode, dev)
        } else {
            // SYS_mknod may not exist on all architectures (e.g., aarch64)
            // Use mknodat with AT_FDCWD instead
            libc::syscall(libc::SYS_mknodat, libc::AT_FDCWD, pathname, mode, dev) as i32
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
            libc::syscall(libc::SYS_mknodat, dirfd, pathname, mode, dev) as i32
        }
    }

    unsafe fn real_getxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        if let Some(func) = REAL_GETXATTR.get() {
            func(path, name, value, size)
        } else {
            libc::syscall(libc::SYS_getxattr, path, name, value, size) as i32
        }
    }

    unsafe fn real_lgetxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        if let Some(func) = REAL_LGETXATTR.get() {
            func(path, name, value, size)
        } else {
            libc::syscall(libc::SYS_lgetxattr, path, name, value, size) as i32
        }
    }

    unsafe fn real_fgetxattr(
        fd: i32,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
    ) -> i32 {
        if let Some(func) = REAL_FGETXATTR.get() {
            func(fd, name, value, size)
        } else {
            libc::syscall(libc::SYS_fgetxattr, fd, name, value, size) as i32
        }
    }

    unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_LISTXATTR.get() {
            func(path, list, size)
        } else {
            libc::syscall(libc::SYS_listxattr, path, list, size) as i32
        }
    }

    unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_LLISTXATTR.get() {
            func(path, list, size)
        } else {
            libc::syscall(libc::SYS_llistxattr, path, list, size) as i32
        }
    }

    unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_FLISTXATTR.get() {
            func(fd, list, size)
        } else {
            libc::syscall(libc::SYS_flistxattr, fd, list, size) as i32
        }
    }
}

// Re-export the functions for use in the main lib.rs
pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_stat(path, buf) }
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_fstat(fd, buf) }
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_lstat(path, buf) }
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_chmod(path, mode) }
}

pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_fstatat(dirfd, pathname, buf, flags) }
}

pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_statx(dirfd, pathname, flags, mask, buf) }
}

pub unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_fchmod(fd, mode) }
}

pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_fchmodat(dirfd, path, mode, flags) }
}

pub unsafe fn real_unlink(path: *const c_char) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_unlink(path) }
}

pub unsafe fn real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_unlinkat(dirfd, path, flags) }
}

pub unsafe fn real_rmdir(path: *const c_char) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_rmdir(path) }
}

pub unsafe fn real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_rename(oldpath, newpath) }
}

pub unsafe fn real_renameat(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_renameat(olddirfd, oldpath, newdirfd, newpath) }
}

pub unsafe fn real_renameat2(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
    flags: u32,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags) }
}

pub unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_mknod(pathname, mode, dev) }
}

pub unsafe fn real_mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_mknodat(dirfd, pathname, mode, dev) }
}

pub unsafe fn real_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_getxattr(path, name, value, size) }
}

pub unsafe fn real_lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_lgetxattr(path, name, value, size) }
}

pub unsafe fn real_fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_fgetxattr(fd, name, value, size) }
}

pub unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_listxattr(path, list, size) }
}

pub unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_llistxattr(path, list, size) }
}

pub unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    unsafe { LinuxHelper::real_flistxattr(fd, list, size) }
}
