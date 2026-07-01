//! Linux-specific implementation for library interposition
//!
//! This module provides Linux-specific implementations using `dlsym(RTLD_NEXT)`
//! to call the real system functions.

use crate::ownership;
use std::os::raw::c_char;
use std::sync::{Once, OnceLock};

// Type aliases for function pointers.
//
// Note: stat/fstat/lstat/chown/chmod/lchown/fchown/fstatat/fchownat/fchmod/
// fchmodat always go straight to a raw syscall below (dlsym(RTLD_NEXT) can
// resolve back into our own hooks for these), so they have no REAL_* static
// or dlsym lookup — unlike the rest of this file's functions, which fall
// back to a dlsym'd function pointer.
type GetuidFn = unsafe extern "C" fn() -> u32;
type GeteuidFn = unsafe extern "C" fn() -> u32;
type GetgidFn = unsafe extern "C" fn() -> u32;
type GetegidFn = unsafe extern "C" fn() -> u32;
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
type UnlinkFn = unsafe extern "C" fn(*const c_char) -> i32;
type UnlinkatFn = unsafe extern "C" fn(i32, *const c_char, i32) -> i32;
type RmdirFn = unsafe extern "C" fn(*const c_char) -> i32;
type RenameFn = unsafe extern "C" fn(*const c_char, *const c_char) -> i32;
type RenameatFn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char) -> i32;
type Renameat2Fn = unsafe extern "C" fn(i32, *const c_char, i32, *const c_char, u32) -> i32;
type MknodFn = unsafe extern "C" fn(*const c_char, libc::mode_t, libc::dev_t) -> i32;
type MknodatFn = unsafe extern "C" fn(i32, *const c_char, libc::mode_t, libc::dev_t) -> i32;
type SetgroupsFn = unsafe extern "C" fn(libc::size_t, *const libc::gid_t) -> i32;
type CapsetFn = unsafe extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> i32;

// xattr function type aliases
type SetxattrFn = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const std::ffi::c_void,
    libc::size_t,
    i32,
) -> i32;
type LsetxattrFn = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const std::ffi::c_void,
    libc::size_t,
    i32,
) -> i32;
type FsetxattrFn =
    unsafe extern "C" fn(i32, *const c_char, *const std::ffi::c_void, libc::size_t, i32) -> i32;
type GetxattrFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type LgetxattrFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type FgetxattrFn =
    unsafe extern "C" fn(i32, *const c_char, *mut std::ffi::c_void, libc::size_t) -> i32;
type ListxattrFn = unsafe extern "C" fn(*const c_char, *mut c_char, libc::size_t) -> i32;
type LlistxattrFn = unsafe extern "C" fn(*const c_char, *mut c_char, libc::size_t) -> i32;
type FlistxattrFn = unsafe extern "C" fn(i32, *mut c_char, libc::size_t) -> i32;
type RemovexattrFn = unsafe extern "C" fn(*const c_char, *const c_char) -> i32;
type LremovexattrFn = unsafe extern "C" fn(*const c_char, *const c_char) -> i32;
type FremovexattrFn = unsafe extern "C" fn(i32, *const c_char) -> i32;

// Use OnceLock for thread-safe lazy initialization
static REAL_GETUID: OnceLock<GetuidFn> = OnceLock::new();
static REAL_GETEUID: OnceLock<GeteuidFn> = OnceLock::new();
static REAL_GETGID: OnceLock<GetgidFn> = OnceLock::new();
static REAL_GETEGID: OnceLock<GetegidFn> = OnceLock::new();
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
static REAL_UNLINK: OnceLock<UnlinkFn> = OnceLock::new();
static REAL_UNLINKAT: OnceLock<UnlinkatFn> = OnceLock::new();
static REAL_RMDIR: OnceLock<RmdirFn> = OnceLock::new();
static REAL_RENAME: OnceLock<RenameFn> = OnceLock::new();
static REAL_RENAMEAT: OnceLock<RenameatFn> = OnceLock::new();
static REAL_RENAMEAT2: OnceLock<Renameat2Fn> = OnceLock::new();
static REAL_MKNOD: OnceLock<MknodFn> = OnceLock::new();
static REAL_MKNODAT: OnceLock<MknodatFn> = OnceLock::new();
static REAL_SETGROUPS: OnceLock<SetgroupsFn> = OnceLock::new();
static REAL_CAPSET: OnceLock<CapsetFn> = OnceLock::new();
// xattr statics
static REAL_SETXATTR: OnceLock<SetxattrFn> = OnceLock::new();
static REAL_LSETXATTR: OnceLock<LsetxattrFn> = OnceLock::new();
static REAL_FSETXATTR: OnceLock<FsetxattrFn> = OnceLock::new();
static REAL_GETXATTR: OnceLock<GetxattrFn> = OnceLock::new();
static REAL_LGETXATTR: OnceLock<LgetxattrFn> = OnceLock::new();
static REAL_FGETXATTR: OnceLock<FgetxattrFn> = OnceLock::new();
static REAL_LISTXATTR: OnceLock<ListxattrFn> = OnceLock::new();
static REAL_LLISTXATTR: OnceLock<LlistxattrFn> = OnceLock::new();
static REAL_FLISTXATTR: OnceLock<FlistxattrFn> = OnceLock::new();
static REAL_REMOVEXATTR: OnceLock<RemovexattrFn> = OnceLock::new();
static REAL_LREMOVEXATTR: OnceLock<LremovexattrFn> = OnceLock::new();
static REAL_FREMOVEXATTR: OnceLock<FremovexattrFn> = OnceLock::new();

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
    REAL_GETUID
        .set(get_next_function::<GetuidFn>(b"getuid\0"))
        .ok();
    REAL_GETEUID
        .set(get_next_function::<GeteuidFn>(b"geteuid\0"))
        .ok();
    REAL_GETGID
        .set(get_next_function::<GetgidFn>(b"getgid\0"))
        .ok();
    REAL_GETEGID
        .set(get_next_function::<GetegidFn>(b"getegid\0"))
        .ok();
    REAL_GETRESUID
        .set(get_next_function::<GetresuidFn>(b"getresuid\0"))
        .ok();
    REAL_GETRESGID
        .set(get_next_function::<GetresgidFn>(b"getresgid\0"))
        .ok();
    REAL_SETUID
        .set(get_next_function::<SetuidFn>(b"setuid\0"))
        .ok();
    REAL_SETGID
        .set(get_next_function::<SetgidFn>(b"setgid\0"))
        .ok();
    REAL_SETREUID
        .set(get_next_function::<SetreuidFn>(b"setreuid\0"))
        .ok();
    REAL_SETREGID
        .set(get_next_function::<SetregidFn>(b"setregid\0"))
        .ok();
    REAL_SETRESUID
        .set(get_next_function::<SetresuidFn>(b"setresuid\0"))
        .ok();
    REAL_SETRESGID
        .set(get_next_function::<SetresgidFn>(b"setresgid\0"))
        .ok();
    REAL_SETFSUID
        .set(get_next_function::<SetfsuidFn>(b"setfsuid\0"))
        .ok();
    REAL_SETFSGID
        .set(get_next_function::<SetfsgidFn>(b"setfsgid\0"))
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
    REAL_RENAMEAT2
        .set(get_next_function::<Renameat2Fn>(b"renameat2\0"))
        .ok();
    REAL_MKNOD
        .set(get_next_function::<MknodFn>(b"mknod\0"))
        .ok();
    REAL_MKNODAT
        .set(get_next_function::<MknodatFn>(b"mknodat\0"))
        .ok();
    REAL_SETGROUPS
        .set(get_next_function::<SetgroupsFn>(b"setgroups\0"))
        .ok();
    REAL_CAPSET
        .set(get_next_function::<CapsetFn>(b"capset\0"))
        .ok();
    // xattr functions
    REAL_SETXATTR
        .set(get_next_function::<SetxattrFn>(b"setxattr\0"))
        .ok();
    REAL_LSETXATTR
        .set(get_next_function::<LsetxattrFn>(b"lsetxattr\0"))
        .ok();
    REAL_FSETXATTR
        .set(get_next_function::<FsetxattrFn>(b"fsetxattr\0"))
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
    REAL_REMOVEXATTR
        .set(get_next_function::<RemovexattrFn>(b"removexattr\0"))
        .ok();
    REAL_LREMOVEXATTR
        .set(get_next_function::<LremovexattrFn>(b"lremovexattr\0"))
        .ok();
    REAL_FREMOVEXATTR
        .set(get_next_function::<FremovexattrFn>(b"fremovexattr\0"))
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
        libc::syscall(libc::SYS_fchownat, libc::AT_FDCWD, path, uid, gid, 0) as i32
    }

    unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
        libc::syscall(libc::SYS_fchmodat, libc::AT_FDCWD, path, mode, 0) as i32
    }

    unsafe fn real_lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
        libc::syscall(
            libc::SYS_fchownat,
            libc::AT_FDCWD,
            path,
            uid,
            gid,
            libc::AT_SYMLINK_NOFOLLOW,
        ) as i32
    }

    unsafe fn real_fchown(fd: i32, uid: u32, gid: u32) -> i32 {
        libc::syscall(libc::SYS_fchown, fd, uid, gid) as i32
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_fstatat(
        dirfd: i32,
        pathname: *const c_char,
        buf: *mut libc::stat,
        flags: i32,
    ) -> i32 {
        libc::syscall(libc::SYS_newfstatat, dirfd, pathname, buf, flags) as i32
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    unsafe fn real_fchownat(
        dirfd: i32,
        path: *const c_char,
        uid: u32,
        gid: u32,
        flags: i32,
    ) -> i32 {
        libc::syscall(libc::SYS_fchownat, dirfd, path, uid, gid, flags) as i32
    }

    unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
        libc::syscall(libc::SYS_fchmod, fd, mode) as i32
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_fchmodat(
        dirfd: i32,
        path: *const c_char,
        mode: libc::mode_t,
        flags: i32,
    ) -> i32 {
        libc::syscall(libc::SYS_fchmodat, dirfd, path, mode, flags) as i32
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

    unsafe fn real_unlink(path: *const c_char) -> i32 {
        if let Some(func) = REAL_UNLINK.get() {
            func(path)
        } else {
            libc::unlink(path)
        }
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
        if let Some(func) = REAL_MKNOD.get() {
            func(pathname, mode, dev)
        } else {
            // SYS_mknod may not exist on all architectures (e.g., aarch64)
            // Use mknodat with AT_FDCWD instead
            libc::syscall(libc::SYS_mknodat, libc::AT_FDCWD, pathname, mode, dev) as i32
        }
    }

    #[cfg(target_os = "linux")]
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

    unsafe fn real_setgroups(size: libc::size_t, list: *const libc::gid_t) -> i32 {
        if let Some(func) = REAL_SETGROUPS.get() {
            func(size, list)
        } else {
            libc::setgroups(size, list)
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_capset(hdrp: *const std::ffi::c_void, data: *const std::ffi::c_void) -> i32 {
        if let Some(func) = REAL_CAPSET.get() {
            func(hdrp, data)
        } else {
            libc::syscall(libc::SYS_capset, hdrp, data) as i32
        }
    }

    // xattr functions - all just pass through for now
    #[cfg(target_os = "linux")]
    unsafe fn real_setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const std::ffi::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_SETXATTR.get() {
            func(path, name, value, size, flags)
        } else {
            libc::syscall(libc::SYS_setxattr, path, name, value, size, flags) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_lsetxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const std::ffi::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_LSETXATTR.get() {
            func(path, name, value, size, flags)
        } else {
            libc::syscall(libc::SYS_lsetxattr, path, name, value, size, flags) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_fsetxattr(
        fd: i32,
        name: *const c_char,
        value: *const std::ffi::c_void,
        size: libc::size_t,
        flags: i32,
    ) -> i32 {
        if let Some(func) = REAL_FSETXATTR.get() {
            func(fd, name, value, size, flags)
        } else {
            libc::syscall(libc::SYS_fsetxattr, fd, name, value, size, flags) as i32
        }
    }

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
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

    #[cfg(target_os = "linux")]
    unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_LISTXATTR.get() {
            func(path, list, size)
        } else {
            libc::syscall(libc::SYS_listxattr, path, list, size) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_LLISTXATTR.get() {
            func(path, list, size)
        } else {
            libc::syscall(libc::SYS_llistxattr, path, list, size) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
        if let Some(func) = REAL_FLISTXATTR.get() {
            func(fd, list, size)
        } else {
            libc::syscall(libc::SYS_flistxattr, fd, list, size) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_removexattr(path: *const c_char, name: *const c_char) -> i32 {
        if let Some(func) = REAL_REMOVEXATTR.get() {
            func(path, name)
        } else {
            libc::syscall(libc::SYS_removexattr, path, name) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_lremovexattr(path: *const c_char, name: *const c_char) -> i32 {
        if let Some(func) = REAL_LREMOVEXATTR.get() {
            func(path, name)
        } else {
            libc::syscall(libc::SYS_lremovexattr, path, name) as i32
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn real_fremovexattr(fd: i32, name: *const c_char) -> i32 {
        if let Some(func) = REAL_FREMOVEXATTR.get() {
            func(fd, name)
        } else {
            libc::syscall(libc::SYS_fremovexattr, fd, name) as i32
        }
    }
}

// Re-export the functions for use in the main lib.rs
pub unsafe fn real_stat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_stat(path, buf)
}

pub unsafe fn real_fstat(fd: i32, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fstat(fd, buf)
}

pub unsafe fn real_lstat(path: *const c_char, buf: *mut libc::stat) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_lstat(path, buf)
}

pub unsafe fn real_getuid() -> u32 {
    ensure_real_funcs();
    LinuxHelper::real_getuid()
}

pub unsafe fn real_geteuid() -> u32 {
    ensure_real_funcs();
    LinuxHelper::real_geteuid()
}

pub unsafe fn real_getgid() -> u32 {
    ensure_real_funcs();
    LinuxHelper::real_getgid()
}

pub unsafe fn real_getegid() -> u32 {
    ensure_real_funcs();
    LinuxHelper::real_getegid()
}

pub unsafe fn real_chown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_chown(path, uid, gid)
}

pub unsafe fn real_chmod(path: *const c_char, mode: libc::mode_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_chmod(path, mode)
}

pub unsafe fn real_lchown(path: *const c_char, uid: u32, gid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_lchown(path, uid, gid)
}

pub unsafe fn real_fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fchown(fd, uid, gid)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fstatat(
    dirfd: i32,
    pathname: *const c_char,
    buf: *mut libc::stat,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fstatat(dirfd, pathname, buf, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_statx(
    dirfd: i32,
    pathname: *const c_char,
    flags: i32,
    mask: u32,
    buf: *mut std::ffi::c_void,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_statx(dirfd, pathname, flags, mask, buf)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fchownat(
    dirfd: i32,
    path: *const c_char,
    uid: u32,
    gid: u32,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fchownat(dirfd, path, uid, gid, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fchmod(fd: i32, mode: libc::mode_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fchmod(fd, mode)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fchmodat(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fchmodat(dirfd, path, mode, flags)
}

pub unsafe fn real_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_getresuid(ruid, euid, suid)
}

pub unsafe fn real_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_getresgid(rgid, egid, sgid)
}

pub unsafe fn real_setuid(uid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setuid(uid)
}

pub unsafe fn real_setgid(gid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setgid(gid)
}

pub unsafe fn real_setreuid(ruid: u32, euid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setreuid(ruid, euid)
}

pub unsafe fn real_setregid(rgid: u32, egid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setregid(rgid, egid)
}

pub unsafe fn real_setresuid(ruid: u32, euid: u32, suid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setresuid(ruid, euid, suid)
}

pub unsafe fn real_setresgid(rgid: u32, egid: u32, sgid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setresgid(rgid, egid, sgid)
}

pub unsafe fn real_setfsuid(uid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setfsuid(uid)
}

pub unsafe fn real_setfsgid(gid: u32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setfsgid(gid)
}

pub unsafe fn real_unlink(path: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_unlink(path)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_unlinkat(dirfd: i32, path: *const c_char, flags: i32) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_unlinkat(dirfd, path, flags)
}

pub unsafe fn real_rmdir(path: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_rmdir(path)
}

pub unsafe fn real_rename(oldpath: *const c_char, newpath: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_rename(oldpath, newpath)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_renameat(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_renameat(olddirfd, oldpath, newdirfd, newpath)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_renameat2(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
    flags: u32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_mknod(pathname: *const c_char, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_mknod(pathname, mode, dev)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_mknodat(dirfd, pathname, mode, dev)
}

pub unsafe fn real_setgroups(size: libc::size_t, list: *const libc::gid_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setgroups(size, list)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_capset(hdrp: *const std::ffi::c_void, data: *const std::ffi::c_void) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_capset(hdrp, data)
}

// xattr public re-exports
#[cfg(target_os = "linux")]
pub unsafe fn real_setxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_setxattr(path, name, value, size, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_lsetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_lsetxattr(path, name, value, size, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fsetxattr(
    fd: i32,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    flags: i32,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fsetxattr(fd, name, value, size, flags)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_getxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_getxattr(path, name, value, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_lgetxattr(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_lgetxattr(path, name, value, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fgetxattr(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fgetxattr(fd, name, value, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_listxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_listxattr(path, list, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_llistxattr(path: *const c_char, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_llistxattr(path, list, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_flistxattr(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_flistxattr(fd, list, size)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_removexattr(path: *const c_char, name: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_removexattr(path, name)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_lremovexattr(path: *const c_char, name: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_lremovexattr(path, name)
}

#[cfg(target_os = "linux")]
pub unsafe fn real_fremovexattr(fd: i32, name: *const c_char) -> i32 {
    ensure_real_funcs();
    LinuxHelper::real_fremovexattr(fd, name)
}
