//! Linux-specific implementation for library interposition
//!
//! This module provides Linux-specific implementations using `dlsym(RTLD_NEXT)`
//! to call the real system functions.

use super::PlatformHelper;
use std::os::raw::c_char;
use std::sync::OnceLock;

// Type aliases for function pointers
type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> i32;
type FstatFn = unsafe extern "C" fn(i32, *mut libc::stat) -> i32;
type LstatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> i32;
type GetuidFn = unsafe extern "C" fn() -> u32;
type GeteuidFn = unsafe extern "C" fn() -> u32;
type GetgidFn = unsafe extern "C" fn() -> u32;
type GetegidFn = unsafe extern "C" fn() -> u32;

// Use OnceLock for thread-safe lazy initialization
static REAL_STAT: OnceLock<StatFn> = OnceLock::new();
static REAL_FSTAT: OnceLock<FstatFn> = OnceLock::new();
static REAL_LSTAT: OnceLock<LstatFn> = OnceLock::new();
static REAL_GETUID: OnceLock<GetuidFn> = OnceLock::new();
static REAL_GETEUID: OnceLock<GeteuidFn> = OnceLock::new();
static REAL_GETGID: OnceLock<GetgidFn> = OnceLock::new();
static REAL_GETEGID: OnceLock<GetegidFn> = OnceLock::new();

/// Initialize the function pointers by looking up the real functions
#[ctor::ctor]
fn init() {
    unsafe {
        REAL_STAT.set(get_next_function::<StatFn>(b"stat\0")).ok();
        REAL_FSTAT.set(get_next_function::<FstatFn>(b"fstat\0")).ok();
        REAL_LSTAT.set(get_next_function::<LstatFn>(b"lstat\0")).ok();
        REAL_GETUID.set(get_next_function::<GetuidFn>(b"getuid\0")).ok();
        REAL_GETEUID.set(get_next_function::<GeteuidFn>(b"geteuid\0")).ok();
        REAL_GETGID.set(get_next_function::<GetgidFn>(b"getgid\0")).ok();
        REAL_GETEGID.set(get_next_function::<GetegidFn>(b"getegid\0")).ok();
    }
}

/// Helper function to look up a function using dlsym(RTLD_NEXT)
unsafe fn get_next_function<T>(symbol: &[u8]) -> T {
    let handle = libc::RTLD_NEXT;
    let ptr = libc::dlsym(handle, symbol.as_ptr() as *const c_char);
    if ptr.is_null() {
        panic!("Failed to find symbol {} with RTLD_NEXT", String::from_utf8_lossy(symbol));
    }
    // SAFETY: We're casting a function pointer from c_void to the specific function type
    // This is valid because we know the symbol exists and has the correct signature
    unsafe { std::mem::transmute_copy(&ptr) }
}

/// Linux platform helper implementation
pub struct LinuxHelper;

impl PlatformHelper for LinuxHelper {
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

    unsafe fn real_getuid() -> u32 {
        if let Some(func) = REAL_GETUID.get() {
            func()
        } else {
            libc::getuid()
        }
    }

    unsafe fn real_geteuid() -> u32 {
        if let Some(func) = REAL_GETEUID.get() {
            func()
        } else {
            libc::geteuid()
        }
    }

    unsafe fn real_getgid() -> u32 {
        if let Some(func) = REAL_GETGID.get() {
            func()
        } else {
            libc::getgid()
        }
    }

    unsafe fn real_getegid() -> u32 {
        if let Some(func) = REAL_GETEGID.get() {
            func()
        } else {
            libc::getegid()
        }
    }
}

// Re-export the functions for use in the main lib.rs
pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    LinuxHelper::real_stat(path, buf)
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    LinuxHelper::real_fstat(fd, buf)
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    LinuxHelper::real_lstat(path, buf)
}

pub unsafe fn real_getuid() -> u32 {
    LinuxHelper::real_getuid()
}

pub unsafe fn real_geteuid() -> u32 {
    LinuxHelper::real_geteuid()
}

pub unsafe fn real_getgid() -> u32 {
    LinuxHelper::real_getgid()
}

pub unsafe fn real_getegid() -> u32 {
    LinuxHelper::real_getegid()
}
