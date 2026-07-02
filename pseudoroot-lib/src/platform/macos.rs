//! macOS-specific wrappers for calling the real libc functions, plus the
//! Darwin xattr interposition hooks and the dyld `__interpose` wiring table
//! (see the `interpose` module below).
//!
//! dyld never applies `__DATA,__interpose` entries to the interposing image
//! itself, so unlike Linux — where the preloaded library must go through
//! `dlsym(RTLD_NEXT)` to avoid binding to its own exported symbols — these
//! `real_*` wrappers can call libc directly.
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

// Darwin xattr interposition hooks. Unlike Linux, macOS has no `l`-prefixed
// variants: the plain functions carry an extra `position` (resource-fork
// offset, always 0 for the attributes we fake) and an `options` bitmask
// whose `XATTR_NOFOLLOW` bit selects the no-symlink-follow behaviour. The
// get/list calls return `ssize_t`, so the i32 the fake_* helpers produce is
// widened on the way out.
mod macos_xattr {
    use crate::ownership::{
        fake_getxattr_fd, fake_getxattr_path, fake_listxattr_fd, fake_listxattr_path,
        fake_removexattr_fd, fake_removexattr_path, fake_setxattr_fd, fake_setxattr_path,
    };
    use std::os::raw::c_char;

    #[inline]
    fn nofollow(options: i32) -> bool {
        options & libc::XATTR_NOFOLLOW != 0
    }

    pub extern "C" fn setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const std::ffi::c_void,
        size: libc::size_t,
        _position: u32,
        options: i32,
    ) -> i32 {
        fake_setxattr_path(path, name, value, size, nofollow(options))
    }

    pub extern "C" fn fsetxattr(
        fd: i32,
        name: *const c_char,
        value: *const std::ffi::c_void,
        size: libc::size_t,
        _position: u32,
        _options: i32,
    ) -> i32 {
        fake_setxattr_fd(fd, name, value, size)
    }

    pub extern "C" fn getxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
        _position: u32,
        options: i32,
    ) -> isize {
        fake_getxattr_path(path, name, value, size, nofollow(options)) as isize
    }

    pub extern "C" fn fgetxattr(
        fd: i32,
        name: *const c_char,
        value: *mut std::ffi::c_void,
        size: libc::size_t,
        _position: u32,
        _options: i32,
    ) -> isize {
        fake_getxattr_fd(fd, name, value, size) as isize
    }

    pub extern "C" fn listxattr(
        path: *const c_char,
        list: *mut c_char,
        size: libc::size_t,
        options: i32,
    ) -> isize {
        fake_listxattr_path(path, list, size, nofollow(options)) as isize
    }

    pub extern "C" fn flistxattr(
        fd: i32,
        list: *mut c_char,
        size: libc::size_t,
        _options: i32,
    ) -> isize {
        fake_listxattr_fd(fd, list, size) as isize
    }

    pub extern "C" fn removexattr(path: *const c_char, name: *const c_char, options: i32) -> i32 {
        fake_removexattr_path(path, name, nofollow(options))
    }

    pub extern "C" fn fremovexattr(fd: i32, name: *const c_char, _options: i32) -> i32 {
        fake_removexattr_fd(fd, name)
    }
}

/// dyld interposition table for macOS.
///
/// On Darwin, `DYLD_INSERT_LIBRARIES` alone does not rebind anything: the
/// two-level namespace binds every image's libc calls straight to libSystem,
/// so exporting a symbol named `stat` (the `LD_PRELOAD` trick) has no effect.
/// Interposition instead happens through `__DATA,__interpose`, a section of
/// `(replacement, replacee)` pointer pairs that dyld processes at load time.
///
/// Two consequences shape `lib.rs` and the `real_*` wrappers above:
/// - the crate must *not* export the libc names (`no_mangle` is Linux-only),
///   otherwise the replacee relocation would bind to our own definition;
/// - dyld never applies interposition to the interposing image itself, so
///   `real_*` wrappers can call libc directly without `dlsym(RTLD_NEXT)`.
///
/// Taking the *address* of the `libc` crate's declaration (rather than naming
/// the symbol) also picks up `$INODE64`-suffixed variants on x86_64 for free.
///
/// `getresuid`/`getresgid`, `setresuid`/`setresgid`, and `setfsuid`/`setfsgid`
/// have no Darwin counterpart in libSystem, so they get no entry here.
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
        GETUID: libc::getuid => crate::getuid;
        GETEUID: libc::geteuid => crate::geteuid;
        GETGID: libc::getgid => crate::getgid;
        GETEGID: libc::getegid => crate::getegid;
        SETUID: libc::setuid => crate::setuid;
        SETGID: libc::setgid => crate::setgid;
        SETREUID: libc::setreuid => crate::setreuid;
        SETREGID: libc::setregid => crate::setregid;
        // Darwin's setgroups takes `c_int`; ours takes `size_t` but ignores
        // the argument, so the width mismatch is inconsequential.
        SETGROUPS: libc::setgroups => crate::setgroups;
        CHOWN: libc::chown => crate::chown;
        LCHOWN: libc::lchown => crate::lchown;
        FCHOWN: libc::fchown => crate::fchown;
        FCHOWNAT: libc::fchownat => crate::fchownat;
        STAT: libc::stat => crate::stat;
        FSTAT: libc::fstat => crate::fstat;
        LSTAT: libc::lstat => crate::lstat;
        FSTATAT: libc::fstatat => crate::fstatat;
        CHMOD: libc::chmod => crate::chmod;
        FCHMOD: libc::fchmod => crate::fchmod;
        FCHMODAT: libc::fchmodat => crate::fchmodat;
        UNLINK: libc::unlink => crate::unlink;
        UNLINKAT: libc::unlinkat => crate::unlinkat;
        RMDIR: libc::rmdir => crate::rmdir;
        RENAME: libc::rename => crate::rename;
        RENAMEAT: libc::renameat => crate::renameat;
        MKNOD: libc::mknod => crate::mknod;
        MKNODAT: libc::mknodat => crate::mknodat;
        // xattr uses the Darwin-signature hooks from `macos_xattr` (see there).
        SETXATTR: libc::setxattr => super::macos_xattr::setxattr;
        FSETXATTR: libc::fsetxattr => super::macos_xattr::fsetxattr;
        GETXATTR: libc::getxattr => super::macos_xattr::getxattr;
        FGETXATTR: libc::fgetxattr => super::macos_xattr::fgetxattr;
        LISTXATTR: libc::listxattr => super::macos_xattr::listxattr;
        FLISTXATTR: libc::flistxattr => super::macos_xattr::flistxattr;
        REMOVEXATTR: libc::removexattr => super::macos_xattr::removexattr;
        FREMOVEXATTR: libc::fremovexattr => super::macos_xattr::fremovexattr;
    }
}
