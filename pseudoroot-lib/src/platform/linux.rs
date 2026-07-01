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
//! `stat`/`fstat`/`lstat`/`fstatat`/`statx`/`chmod`/`fchmod`/`fchmodat` always
//! go straight to a raw syscall below (dlsym(RTLD_NEXT) can resolve back into
//! our own hooks for these), so they need no dlsym lookup. The rest resolve
//! the real libc symbol lazily via [`real_fn!`] on first use after the
//! library has finished bootstrapping, falling back to a raw syscall before
//! that (or if the symbol turns out to be missing).

use crate::ownership;
use std::os::raw::c_char;
use std::sync::OnceLock;

/// Helper function to look up a function using dlsym(RTLD_NEXT)
unsafe fn get_next_function<T>(symbol: &[u8]) -> T {
    let ptr = unsafe { libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr() as *const c_char) };
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

/// Define a `real_*` wrapper that resolves `$symbol` via `dlsym(RTLD_NEXT)`
/// the first time it's called after library init has finished, caching the
/// result in a function-local static, and calls `$fallback` before that (or
/// if the library never finishes initializing, which shouldn't happen here).
macro_rules! real_fn {
    ($name:ident($($arg:ident: $argty:ty),* $(,)?) -> $ret:ty, $symbol:literal, $fallback:expr) => {
        pub unsafe fn $name($($arg: $argty),*) -> $ret {
            type RealFn = unsafe extern "C" fn($($argty),*) -> $ret;
            static REAL: OnceLock<RealFn> = OnceLock::new();

            if ownership::library_init_done() {
                let func = *REAL.get_or_init(|| unsafe { get_next_function::<RealFn>($symbol) });
                return unsafe { func($($arg),*) };
            }
            $fallback
        }
    };
}

// Re-exported for use in the main lib.rs
pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    // Always syscall: dlsym(RTLD_NEXT) can resolve back into our own hooks.
    unsafe { libc::syscall(libc::SYS_newfstatat, libc::AT_FDCWD, path, buf, 0) as i32 }
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    unsafe { libc::syscall(libc::SYS_fstat, fd, buf) as i32 }
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    unsafe {
        libc::syscall(
            libc::SYS_newfstatat,
            libc::AT_FDCWD,
            path,
            buf,
            libc::AT_SYMLINK_NOFOLLOW,
        ) as i32
    }
}

pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    unsafe { libc::syscall(libc::SYS_newfstatat, dirfd, pathname, buf, flags) as i32 }
}

pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    // Always use the syscall directly: calling libc::statx or dlsym(statx) would
    // recurse through our hook.
    unsafe { libc::syscall(libc::SYS_statx, dirfd, pathname, flags, mask, buf) as i32 }
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    unsafe { libc::syscall(libc::SYS_fchmodat, libc::AT_FDCWD, path, mode, 0) as i32 }
}

pub unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    unsafe { libc::syscall(libc::SYS_fchmod, fd, mode) as i32 }
}

pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    unsafe { libc::syscall(libc::SYS_fchmodat, dirfd, path, mode, flags) as i32 }
}

real_fn!(real_unlink(path: *const c_char) -> i32, b"unlink\0",
    unsafe { libc::unlink(path) });

real_fn!(real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32, b"unlinkat\0",
    unsafe { libc::unlinkat(dirfd, path, flags) });

real_fn!(real_rmdir(path: *const c_char) -> i32, b"rmdir\0",
    unsafe { libc::rmdir(path) });

real_fn!(real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32, b"rename\0",
    unsafe { libc::rename(oldpath, newpath) });

real_fn!(real_renameat(olddirfd: i32, oldpath: *const c_char, newdirfd: i32, newpath: *const c_char) -> i32,
    b"renameat\0",
    unsafe { libc::renameat(olddirfd, oldpath, newdirfd, newpath) });

real_fn!(real_renameat2(olddirfd: i32, oldpath: *const c_char, newdirfd: i32, newpath: *const c_char, flags: u32) -> i32,
    b"renameat2\0",
    unsafe { libc::renameat2(olddirfd, oldpath, newdirfd, newpath, flags) });

// SYS_mknod may not exist on all architectures (e.g. aarch64); fall back to
// mknodat with AT_FDCWD instead of libc::mknod.
real_fn!(real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32, b"mknod\0",
    unsafe { libc::syscall(libc::SYS_mknodat, libc::AT_FDCWD, pathname, mode, dev) as i32 });

real_fn!(real_mknodat(dirfd: i32, pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32,
    b"mknodat\0",
    unsafe { libc::syscall(libc::SYS_mknodat, dirfd, pathname, mode, dev) as i32 });

real_fn!(real_getxattr(path: *const c_char, name: *const c_char, value: *mut std::ffi::c_void, size: libc::size_t) -> i32,
    b"getxattr\0",
    unsafe { libc::syscall(libc::SYS_getxattr, path, name, value, size) as i32 });

real_fn!(real_lgetxattr(path: *const c_char, name: *const c_char, value: *mut std::ffi::c_void, size: libc::size_t) -> i32,
    b"lgetxattr\0",
    unsafe { libc::syscall(libc::SYS_lgetxattr, path, name, value, size) as i32 });

real_fn!(real_fgetxattr(fd: i32, name: *const c_char, value: *mut std::ffi::c_void, size: libc::size_t) -> i32,
    b"fgetxattr\0",
    unsafe { libc::syscall(libc::SYS_fgetxattr, fd, name, value, size) as i32 });

real_fn!(real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32, b"listxattr\0",
    unsafe { libc::syscall(libc::SYS_listxattr, path, list, size) as i32 });

real_fn!(real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32, b"llistxattr\0",
    unsafe { libc::syscall(libc::SYS_llistxattr, path, list, size) as i32 });

real_fn!(real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32, b"flistxattr\0",
    unsafe { libc::syscall(libc::SYS_flistxattr, fd, list, size) as i32 });
