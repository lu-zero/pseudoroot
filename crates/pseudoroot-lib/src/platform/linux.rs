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
type ChownFn = unsafe extern "C" fn(*const c_char, u32, u32) -> i32;
type ChmodFn = unsafe extern "C" fn(*const c_char, libc::mode_t) -> i32;
type LchownFn = unsafe extern "C" fn(*const c_char, u32, u32) -> i32;
type FchownFn = unsafe extern "C" fn(i32, u32, u32) -> i32;
type FstatatFn = unsafe extern "C" fn(i32, *const c_char, *mut libc::stat, i32) -> i32;
type StatxFn = unsafe extern "C" fn(i32, *const c_char, *mut std::ffi::c_void, u32, i32) -> i32;
type FchownatFn = unsafe extern "C" fn(i32, *const c_char, u32, u32, i32) -> i32;
type FchmodatFn = unsafe extern "C" fn(i32, *const c_char, libc::mode_t, i32) -> i32;
type GetresuidFn = unsafe extern "C" fn(*mut u32, *mut u32, *mut u32) -> i32;
type GetresgidFn = unsafe extern "C" fn(*mut u32, *mut u32, *mut u32) -> i32;
type SetuidFn = unsafe extern "C" fn(u32) -> i32;
type SetgidFn = unsafe extern "C" fn(u32) -> i32;
type SetreuidFn = unsafe extern "C" fn(u32, u32) -> i32;
type SetregidFn = unsafe extern "C" fn(u32, u32) -> i32;
type SetresuidFn = unsafe extern "C" fn(u32, u32, u32) -> i32;
type SetresgidFn = unsafe extern "C" fn(u32, u32, u32) -> i32;
type SetfsuidFn = unsafe extern "C" fn(u32) -> i32;
type SetfsgidFn = unsafe extern "C" fn(u32) -> i32;

// Use OnceLock for thread-safe lazy initialization
static REAL_STAT: OnceLock<StatFn> = OnceLock::new();
static REAL_FSTAT: OnceLock<FstatFn> = OnceLock::new();
static REAL_LSTAT: OnceLock<LstatFn> = OnceLock::new();
static REAL_GETUID: OnceLock<GetuidFn> = OnceLock::new();
static REAL_GETEUID: OnceLock<GeteuidFn> = OnceLock::new();
static REAL_GETGID: OnceLock<GetgidFn> = OnceLock::new();
static REAL_GETEGID: OnceLock<GetegidFn> = OnceLock::new();
static REAL_CHOWN: OnceLock<ChownFn> = OnceLock::new();
static REAL_CHMOD: OnceLock<ChmodFn> = OnceLock::new();
static REAL_LCHOWN: OnceLock<LchownFn> = OnceLock::new();
static REAL_FCHOWN: OnceLock<FchownFn> = OnceLock::new();
static REAL_FSTATAT: OnceLock<FstatatFn> = OnceLock::new();
static REAL_STATX: OnceLock<StatxFn> = OnceLock::new();
static REAL_FCHOWNAT: OnceLock<FchownatFn> = OnceLock::new();
static REAL_FCHMODAT: OnceLock<FchmodatFn> = OnceLock::new();
static REAL_GETRESUID: OnceLock<GetresuidFn> = OnceLock::new();
static REAL_GETRESGID: OnceLock<GetresgidFn> = OnceLock::new();
static REAL_SETUID: OnceLock<SetuidFn> = OnceLock::new();
static REAL_SETGID: OnceLock<SetgidFn> = OnceLock::new();
static REAL_SETREUID: OnceLock<SetreuidFn> = OnceLock::new();
static REAL_SETREGID: OnceLock<SetregidFn> = OnceLock::new();
static REAL_SETRESUID: OnceLock<SetresuidFn> = OnceLock::new();
static REAL_SETRESGID: OnceLock<SetresgidFn> = OnceLock::new();
static REAL_SETFSUID: OnceLock<SetfsuidFn> = OnceLock::new();
static REAL_SETFSGID: OnceLock<SetfsgidFn> = OnceLock::new();

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
        REAL_CHOWN.set(get_next_function::<ChownFn>(b"chown\0")).ok();
        REAL_CHMOD.set(get_next_function::<ChmodFn>(b"chmod\0")).ok();
        REAL_LCHOWN.set(get_next_function::<LchownFn>(b"lchown\0")).ok();
        REAL_FCHOWN.set(get_next_function::<FchownFn>(b"fchown\0")).ok();
        REAL_FSTATAT.set(get_next_function::<FstatatFn>(b"fstatat\0")).ok();
        REAL_STATX.set(get_next_function::<StatxFn>(b"statx\0")).ok();
        REAL_FCHOWNAT.set(get_next_function::<FchownatFn>(b"fchownat\0")).ok();
        REAL_FCHMODAT.set(get_next_function::<FchmodatFn>(b"fchmodat\0")).ok();
        REAL_GETRESUID.set(get_next_function::<GetresuidFn>(b"getresuid\0")).ok();
        REAL_GETRESGID.set(get_next_function::<GetresgidFn>(b"getresgid\0")).ok();
        REAL_SETUID.set(get_next_function::<SetuidFn>(b"setuid\0")).ok();
        REAL_SETGID.set(get_next_function::<SetgidFn>(b"setgid\0")).ok();
        REAL_SETREUID.set(get_next_function::<SetreuidFn>(b"setreuid\0")).ok();
        REAL_SETREGID.set(get_next_function::<SetregidFn>(b"setregid\0")).ok();
        REAL_SETRESUID.set(get_next_function::<SetresuidFn>(b"setresuid\0")).ok();
        REAL_SETRESGID.set(get_next_function::<SetresgidFn>(b"setresgid\0")).ok();
        REAL_SETFSUID.set(get_next_function::<SetfsuidFn>(b"setfsuid\0")).ok();
        REAL_SETFSGID.set(get_next_function::<SetfsgidFn>(b"setfsgid\0")).ok();
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

    unsafe fn real_chown(path: *const c_char, uid: u32, gid: u32) -> i32 {
        if let Some(func) = REAL_CHOWN.get() {
            func(path, uid, gid)
        } else {
            libc::chown(path, uid, gid)
        }
    }

    unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
        if let Some(func) = REAL_CHMOD.get() {
            func(path, mode)
        } else {
            libc::chmod(path, mode)
        }
    }

    unsafe fn real_lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
        if let Some(func) = REAL_LCHOWN.get() {
            func(path, uid, gid)
        } else {
            libc::lchown(path, uid, gid)
        }
    }

    unsafe fn real_fchown(fd: i32, uid: u32, gid: u32) -> i32 {
        if let Some(func) = REAL_FCHOWN.get() {
            func(fd, uid, gid)
        } else {
            libc::fchown(fd, uid, gid)
        }
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    unsafe fn real_statx(
        dirfd: i32,
        pathname: *const c_char,
        buf: *mut std::ffi::c_void,
        mask: u32,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_STATX.get() {
            func(dirfd, pathname, buf, mask, flags)
        } else {
            // libc statx has different signature - we'll use syscall directly
            // For now, just call the real function via syscall
            libc::syscall(libc::SYS_statx, dirfd, pathname, buf, mask, flags) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_fchownat(
        dirfd: i32,
        path: *const c_char,
        uid: u32,
        gid: u32,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_FCHOWNAT.get() {
            func(dirfd, path, uid, gid, flags)
        } else {
            libc::fchownat(dirfd, path, uid, gid, flags)
        }
    }

    #[cfg(target_os = "linux")]
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

    unsafe fn real_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32 {
        if let Some(func) = REAL_GETRESUID.get() {
            func(ruid, euid, suid)
        } else {
            libc::getresuid(ruid, euid, suid)
        }
    }

    unsafe fn real_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32 {
        if let Some(func) = REAL_GETRESGID.get() {
            func(rgid, egid, sgid)
        } else {
            libc::getresgid(rgid, egid, sgid)
        }
    }

    unsafe fn real_setuid(uid: u32) -> i32 {
        if let Some(func) = REAL_SETUID.get() {
            func(uid)
        } else {
            libc::setuid(uid)
        }
    }

    unsafe fn real_setgid(gid: u32) -> i32 {
        if let Some(func) = REAL_SETGID.get() {
            func(gid)
        } else {
            libc::setgid(gid)
        }
    }

    unsafe fn real_setreuid(ruid: u32, euid: u32) -> i32 {
        if let Some(func) = REAL_SETREUID.get() {
            func(ruid, euid)
        } else {
            libc::setreuid(ruid, euid)
        }
    }

    unsafe fn real_setregid(rgid: u32, egid: u32) -> i32 {
        if let Some(func) = REAL_SETREGID.get() {
            func(rgid, egid)
        } else {
            libc::setregid(rgid, egid)
        }
    }

    unsafe fn real_setresuid(ruid: u32, euid: u32, suid: u32) -> i32 {
        if let Some(func) = REAL_SETRESUID.get() {
            func(ruid, euid, suid)
        } else {
            libc::setresuid(ruid, euid, suid)
        }
    }

    unsafe fn real_setresgid(rgid: u32, egid: u32, sgid: u32) -> i32 {
        if let Some(func) = REAL_SETRESGID.get() {
            func(rgid, egid, sgid)
        } else {
            libc::setresgid(rgid, egid, sgid)
        }
    }

    unsafe fn real_setfsuid(uid: u32) -> i32 {
        if let Some(func) = REAL_SETFSUID.get() {
            func(uid)
        } else {
            libc::setfsuid(uid)
        }
    }

    unsafe fn real_setfsgid(gid: u32) -> i32 {
        if let Some(func) = REAL_SETFSGID.get() {
            func(gid)
        } else {
            libc::setfsgid(gid)
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

pub unsafe fn real_chown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    LinuxHelper::real_chown(path, uid, gid)
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    LinuxHelper::real_chmod(path, mode)
}

pub unsafe fn real_lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    LinuxHelper::real_lchown(path, uid, gid)
}

pub unsafe fn real_fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    LinuxHelper::real_fchown(fd, uid, gid)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    LinuxHelper::real_fstatat(dirfd, pathname, buf, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut std::ffi::c_void,
    mask: u32,
    flags: i32,
) -> i32 {
    LinuxHelper::real_statx(dirfd, pathname, buf, mask, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fchownat(
    dirfd: i32,
    path: *const c_char,
    uid: u32,
    gid: u32,
    flags: i32,
) -> i32 {
    LinuxHelper::real_fchownat(dirfd, path, uid, gid, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    LinuxHelper::real_fchmodat(dirfd, path, mode, flags)
}

pub unsafe fn real_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32 {
    LinuxHelper::real_getresuid(ruid, euid, suid)
}

pub unsafe fn real_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32 {
    LinuxHelper::real_getresgid(rgid, egid, sgid)
}

pub unsafe fn real_setuid(uid: u32) -> i32 {
    LinuxHelper::real_setuid(uid)
}

pub unsafe fn real_setgid(gid: u32) -> i32 {
    LinuxHelper::real_setgid(gid)
}

pub unsafe fn real_setreuid(ruid: u32, euid: u32) -> i32 {
    LinuxHelper::real_setreuid(ruid, euid)
}

pub unsafe fn real_setregid(rgid: u32, egid: u32) -> i32 {
    LinuxHelper::real_setregid(rgid, egid)
}

pub unsafe fn real_setresuid(ruid: u32, euid: u32, suid: u32) -> i32 {
    LinuxHelper::real_setresuid(ruid, euid, suid)
}

pub unsafe fn real_setresgid(rgid: u32, egid: u32, sgid: u32) -> i32 {
    LinuxHelper::real_setresgid(rgid, egid, sgid)
}

pub unsafe fn real_setfsuid(uid: u32) -> i32 {
    LinuxHelper::real_setfsuid(uid)
}

pub unsafe fn real_setfsgid(gid: u32) -> i32 {
    LinuxHelper::real_setfsgid(gid)
}
