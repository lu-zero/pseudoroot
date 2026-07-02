//! Shared ownership tracking helpers used by the interposed hooks.

use crate::inode::{
    ID_UNCHANGED, fstat_fd, key_from_stat, lstat_path, resolve_path_at, resolve_stat_flags,
    stat_at, stat_path, stat_path_buf,
};
use pseudoroot_core::daemon_client::{
    daemon_get_current_uid_gid, daemon_get_inode, daemon_init, daemon_mode_active,
    daemon_mode_enabled, daemon_remove_inode, daemon_set_current_uid_gid, daemon_set_inode,
    daemon_upsert_chown, init_daemon_connection,
};
use pseudoroot_core::shm_client::{
    init_shm_from_env, shm_get_current_uid_gid, shm_get_inode, shm_mode_active, shm_remove_inode,
    shm_set_current_uid_gid, shm_upsert_chown, shm_upsert_inode,
};
use pseudoroot_core::state::{
    FakeInode, InodeKey, global_state_read, global_state_write, init_global_state,
};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const ALLPERMS: u32 = 0o7777;
const MAX_XATTR: usize = 64 * 1024;

static LIBRARY_INIT: Once = Once::new();
static LIBRARY_INIT_DONE: AtomicBool = AtomicBool::new(false);
static LIBRARY_INITIALIZING: AtomicBool = AtomicBool::new(false);
static CTOR_DONE: AtomicBool = AtomicBool::new(false);
static BOOT_UID: AtomicU32 = AtomicU32::new(0);
static BOOT_GID: AtomicU32 = AtomicU32::new(0);

pub(crate) static FS_UID: AtomicU32 = AtomicU32::new(u32::MAX);
pub(crate) static FS_GID: AtomicU32 = AtomicU32::new(u32::MAX);

/// Whether the full library state has been initialized.
///
/// Only the Linux backend consults this (its `real_fn!` wrappers defer
/// `dlsym(RTLD_NEXT)` until bootstrap is done); macOS calls libc directly.
#[cfg(target_os = "linux")]
#[inline]
#[must_use]
pub(crate) fn library_init_done() -> bool {
    LIBRARY_INIT_DONE.load(Ordering::Acquire)
}

/// Record bootstrap UID/GID from the library constructor (lock-free).
pub fn store_bootstrap_ids(uid: u32, gid: u32) {
    BOOT_UID.store(uid, Ordering::Relaxed);
    BOOT_GID.store(gid, Ordering::Relaxed);
    CTOR_DONE.store(true, Ordering::Release);
}

fn ensure_library_init() {
    if LIBRARY_INIT_DONE.load(Ordering::Acquire) {
        ensure_session_backing_init();
        return;
    }
    if !CTOR_DONE.load(Ordering::Acquire) {
        // Called before our own constructor has run. On macOS dyld applies
        // `__interpose` before running any initializers, so libSystem's own
        // bootstrap (e.g. `__malloc_init` stat'ing feature-flag files) enters
        // our hooks while this image's thread-local support is still missing —
        // initializing global state here (DashMap → RandomState → TLS) aborts
        // in `_tlv_bootstrap_error`. Stay on the lock-free bootstrap path;
        // callers all fall back to `BOOT_UID`/`BOOT_GID` until init completes.
        return;
    }
    if LIBRARY_INITIALIZING.load(Ordering::Acquire) {
        // Reentrant (e.g. allocator/stat during global state setup). The outer
        // init call will finish; avoid deadlocking on LIBRARY_INIT's Once.
        return;
    }
    LIBRARY_INIT.call_once(|| {
        LIBRARY_INITIALIZING.store(true, Ordering::Release);
        finish_library_init();
        LIBRARY_INITIALIZING.store(false, Ordering::Release);
    });
    ensure_session_backing_init();
}

fn ensure_session_backing_init() {
    static SESSION_INIT: Once = Once::new();
    SESSION_INIT.call_once(|| {
        let _ = init_shm_from_env();
        if shm_mode_active() || !daemon_mode_enabled() {
            return;
        }
        let uid = BOOT_UID.load(Ordering::Relaxed);
        let gid = BOOT_GID.load(Ordering::Relaxed);
        let _ = init_daemon_connection();
        let _ = daemon_init(uid, gid);
    });
}

/// Complete library initialization on first use (safe outside the dynamic linker ctor).
pub fn finish_library_init() {
    let uid = BOOT_UID.load(Ordering::Relaxed);
    let gid = BOOT_GID.load(Ordering::Relaxed);

    {
        let state = init_global_state();
        state.set_current(uid, gid);
    }

    LIBRARY_INIT_DONE.store(true, Ordering::Release);
}

#[must_use]
pub(crate) fn current_fake_uid() -> u32 {
    if !LIBRARY_INIT_DONE.load(Ordering::Acquire) {
        return BOOT_UID.load(Ordering::Relaxed);
    }
    ensure_session_backing_init();
    if shm_mode_active()
        && let Some((uid, _)) = shm_get_current_uid_gid()
    {
        return uid;
    }
    if daemon_mode_active()
        && let Some((uid, _)) = daemon_get_current_uid_gid()
    {
        return uid;
    }
    global_state_read().current_uid()
}

#[must_use]
pub(crate) fn current_fake_gid() -> u32 {
    if !LIBRARY_INIT_DONE.load(Ordering::Acquire) {
        return BOOT_GID.load(Ordering::Relaxed);
    }
    ensure_session_backing_init();
    if shm_mode_active()
        && let Some((_, gid)) = shm_get_current_uid_gid()
    {
        return gid;
    }
    if daemon_mode_active()
        && let Some((_, gid)) = daemon_get_current_uid_gid()
    {
        return gid;
    }
    global_state_read().current_gid()
}

pub(crate) fn set_current_ids(uid: u32, gid: u32) -> i32 {
    ensure_library_init();
    BOOT_UID.store(uid, Ordering::Relaxed);
    BOOT_GID.store(gid, Ordering::Relaxed);
    {
        let state = global_state_write();
        state.set_current(uid, gid);
    }
    if shm_mode_active() {
        shm_set_current_uid_gid(uid, gid);
    } else if daemon_mode_enabled() {
        let _ = daemon_set_current_uid_gid(uid, gid);
    }
    0
}

fn get_inode(key: InodeKey) -> Option<FakeInode> {
    if shm_mode_active() {
        return shm_get_inode(key);
    }
    if daemon_mode_active()
        && let Some(inode) = daemon_get_inode(key)
    {
        return Some(inode);
    }
    let state = global_state_read();
    state.get_inode(key)
}

fn set_inode(key: InodeKey, inode: FakeInode) {
    if shm_mode_active() {
        shm_upsert_inode(key, &inode);
        return;
    }
    {
        let state = global_state_write();
        state.set_inode(key, inode.clone());
    }
    if daemon_mode_enabled() {
        let _ = daemon_set_inode(key, &inode);
    }
}

fn update_inode<F>(key: InodeKey, f: F)
where
    F: FnOnce(&mut FakeInode),
{
    let mut inode =
        get_inode(key).unwrap_or_else(|| FakeInode::new(current_fake_uid(), current_fake_gid()));
    f(&mut inode);
    set_inode(key, inode);
}

fn remove_inode(key: InodeKey) {
    if shm_mode_active() {
        shm_remove_inode(key);
        return;
    }
    {
        let state = global_state_write();
        state.remove_inode(key);
    }
    if daemon_mode_enabled() {
        let _ = daemon_remove_inode(key);
    }
}

/// Record a chown against `key` in one map update (avoids get+set double lookup).
fn record_chown_key(key: InodeKey, uid: u32, gid: u32) {
    let fake_uid = current_fake_uid();
    let fake_gid = current_fake_gid();
    if shm_mode_active() {
        shm_upsert_chown(key, uid, gid, fake_uid, fake_gid);
        return;
    }
    if daemon_mode_active() {
        let _ = daemon_upsert_chown(key, uid, gid, fake_uid, fake_gid);
        return;
    }

    let inode = {
        let state = global_state_write();
        let entry = state.inode_map.entry(key);
        let inode_ref = entry
            .and_modify(|inode| {
                if uid != ID_UNCHANGED {
                    inode.uid = uid;
                }
                if gid != ID_UNCHANGED {
                    inode.gid = gid;
                }
            })
            .or_insert_with(|| {
                let mut inode = FakeInode::new(fake_uid, fake_gid);
                if uid != ID_UNCHANGED {
                    inode.uid = uid;
                }
                if gid != ID_UNCHANGED {
                    inode.gid = gid;
                }
                inode
            });
        inode_ref.clone()
    };
    if daemon_mode_enabled() {
        let _ = daemon_set_inode(key, &inode);
    }
}

/// How to resolve a `libc::stat` for a syscall being intercepted, and (for
/// path-based sources) whether to follow a trailing symlink.
#[derive(Clone, Copy)]
enum StatSource {
    Path {
        path: *const c_char,
        nofollow: bool,
    },
    Fd(i32),
    At {
        dirfd: i32,
        path: *const c_char,
        at_flags: i32,
    },
}

impl StatSource {
    fn stat(&self) -> Result<libc::stat, i32> {
        match *self {
            Self::Path {
                path,
                nofollow: true,
            } => lstat_path(path),
            Self::Path {
                path,
                nofollow: false,
            } => stat_path(path),
            Self::Fd(fd) => fstat_fd(fd),
            Self::At {
                dirfd,
                path,
                at_flags,
            } => {
                let flags = resolve_stat_flags(dirfd, path, at_flags);
                stat_at(dirfd, path, flags)
            }
        }
    }
}

/// Compose a full mode value: keep the real type bits, override the permission bits.
#[inline]
#[must_use]
fn compose_mode(real_mode: u32, requested_perms: u32) -> u32 {
    (real_mode & !ALLPERMS) | (requested_perms & ALLPERMS)
}

#[inline]
#[must_use]
fn zero_on_err(result: i32) -> i32 {
    if result < 0 { 0 } else { result }
}

// libc::mode_t is already u32 on Linux (the casts below are no-ops there) but
// u16 on Darwin, where the widening cast is load-bearing.
#[allow(clippy::unnecessary_cast)]
fn record_chmod_for_key(key: InodeKey, real_mode: libc::mode_t, req_mode: libc::mode_t) {
    let mode = compose_mode(real_mode as u32, req_mode as u32);
    update_inode(key, |inode| {
        inode.mode = Some(mode);
    });
}

fn record_chown(source: StatSource, uid: u32, gid: u32) -> i32 {
    ensure_library_init();
    match source.stat() {
        Ok(st) => {
            record_chown_key(key_from_stat(&st), uid, gid);
            0
        }
        Err(errno) => errno,
    }
}

pub(crate) fn record_chown_at(
    dirfd: i32,
    path: *const c_char,
    at_flags: i32,
    uid: u32,
    gid: u32,
) -> i32 {
    record_chown(
        StatSource::At {
            dirfd,
            path,
            at_flags,
        },
        uid,
        gid,
    )
}

pub(crate) fn record_chown_path(path: *const c_char, nofollow: bool, uid: u32, gid: u32) -> i32 {
    record_chown(StatSource::Path { path, nofollow }, uid, gid)
}

pub(crate) fn record_chown_fd(fd: i32, uid: u32, gid: u32) -> i32 {
    record_chown(StatSource::Fd(fd), uid, gid)
}

fn record_chmod(source: StatSource, mode: libc::mode_t) -> i32 {
    ensure_library_init();
    match source.stat() {
        Ok(st) => {
            let key = key_from_stat(&st);
            record_chmod_for_key(key, st.st_mode, mode);
            let real = match source {
                StatSource::Path { path, .. } => unsafe { crate::platform::real_chmod(path, mode) },
                StatSource::Fd(fd) => unsafe { crate::platform::real_fchmod(fd, mode) },
                StatSource::At {
                    dirfd,
                    path,
                    at_flags,
                } => unsafe { crate::platform::real_fchmodat(dirfd, path, mode, at_flags) },
            };
            zero_on_err(real)
        }
        Err(errno) => errno,
    }
}

pub(crate) fn record_chmod_path(path: *const c_char, mode: libc::mode_t) -> i32 {
    record_chmod(
        StatSource::Path {
            path,
            nofollow: false,
        },
        mode,
    )
}

pub(crate) fn record_chmod_fd(fd: i32, mode: libc::mode_t) -> i32 {
    record_chmod(StatSource::Fd(fd), mode)
}

pub(crate) fn record_chmod_at(
    dirfd: i32,
    path: *const c_char,
    mode: libc::mode_t,
    at_flags: i32,
) -> i32 {
    record_chmod(
        StatSource::At {
            dirfd,
            path,
            at_flags,
        },
        mode,
    )
}

fn maybe_remove_inode(key: InodeKey, nlink: libc::nlink_t) {
    if nlink <= 1 {
        remove_inode(key);
    }
}

pub(crate) fn prepare_rename_overwrite(
    olddirfd: i32,
    oldpath: *const c_char,
    newdirfd: i32,
    newpath: *const c_char,
) {
    ensure_library_init();
    if let Ok(new_st) = stat_at(newdirfd, newpath, 0) {
        let new_key = key_from_stat(&new_st);
        if let Ok(old_st) = stat_at(olddirfd, oldpath, 0) {
            let old_key = key_from_stat(&old_st);
            if new_key != old_key {
                maybe_remove_inode(new_key, new_st.st_nlink);
            }
        } else {
            maybe_remove_inode(new_key, new_st.st_nlink);
        }
    }
}

pub(crate) fn maybe_remove_inode_at(dirfd: i32, path: *const c_char, at_flags: i32) {
    ensure_library_init();
    let stat_flags = resolve_stat_flags(dirfd, path, at_flags);
    if let Ok(st) = stat_at(dirfd, path, stat_flags) {
        maybe_remove_inode(key_from_stat(&st), st.st_nlink);
    }
}

pub(crate) fn maybe_remove_inode_path(path: *const c_char) {
    ensure_library_init();
    if let Ok(st) = stat_path(path) {
        maybe_remove_inode(key_from_stat(&st), st.st_nlink);
    }
}

fn read_fs_uid() -> u32 {
    let stored = FS_UID.load(Ordering::Relaxed);
    if stored == u32::MAX {
        current_fake_uid()
    } else {
        stored
    }
}

fn read_fs_gid() -> u32 {
    let stored = FS_GID.load(Ordering::Relaxed);
    if stored == u32::MAX {
        current_fake_gid()
    } else {
        stored
    }
}

/// Set the filesystem UID and return the previous value.
#[must_use]
pub(crate) fn set_fsuid(uid: u32) -> u32 {
    let previous = read_fs_uid();
    FS_UID.store(uid, Ordering::Relaxed);
    previous
}

/// Set the filesystem GID and return the previous value.
#[must_use]
pub(crate) fn setfsgid(gid: u32) -> u32 {
    let previous = read_fs_gid();
    FS_GID.store(gid, Ordering::Relaxed);
    previous
}

// libc::mode_t/dev_t are already u32/u64 on Linux (no-op casts there) but
// u16/i32 on Darwin, where the widening casts are load-bearing.
#[allow(clippy::unnecessary_cast)]
fn finish_fake_mknod(resolved: &Path, mode: libc::mode_t, dev: libc::dev_t) -> i32 {
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o644)
        .open(resolved)
        .is_err()
    {
        return -1;
    }
    match stat_path_buf(resolved) {
        Ok(st) => {
            let key = key_from_stat(&st);
            let mut inode = FakeInode::new(current_fake_uid(), current_fake_gid());
            inode.mode = Some(mode as u32);
            let kind = mode & libc::S_IFMT;
            if kind == libc::S_IFCHR || kind == libc::S_IFBLK {
                inode.rdev = Some(dev as u64);
            }
            set_inode(key, inode);
            0
        }
        Err(errno) => errno,
    }
}

/// Create a fake special file by dropping a regular placeholder and recording metadata.
pub(crate) fn fake_mknod_path(
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    ensure_library_init();
    let Some(resolved) = resolve_path_at(libc::AT_FDCWD, pathname) else {
        return unsafe { crate::platform::real_mknod(pathname, mode, dev) };
    };
    let result = finish_fake_mknod(&resolved, mode, dev);
    if result < 0 {
        return unsafe { crate::platform::real_mknod(pathname, mode, dev) };
    }
    result
}

/// Create a fake special file relative to a directory file descriptor.
pub(crate) fn fake_mknodat(
    dirfd: i32,
    pathname: *const c_char,
    mode: libc::mode_t,
    dev: libc::dev_t,
) -> i32 {
    ensure_library_init();
    let Some(resolved) = resolve_path_at(dirfd, pathname) else {
        return unsafe { crate::platform::real_mknodat(dirfd, pathname, mode, dev) };
    };
    let result = finish_fake_mknod(&resolved, mode, dev);
    if result < 0 {
        return unsafe { crate::platform::real_mknodat(dirfd, pathname, mode, dev) };
    }
    result
}

fn patch_inode_fields(st: &mut libc::stat, inode: &FakeInode) {
    st.st_uid = inode.uid as libc::uid_t;
    st.st_gid = inode.gid as libc::gid_t;
    if let Some(mode) = inode.mode {
        st.st_mode = mode as libc::mode_t;
    }
    if let Some(rdev) = inode.rdev {
        st.st_rdev = rdev as libc::dev_t;
    }
}

fn apply_default_ownership(st: &mut libc::stat) {
    let state = global_state_read();
    st.st_uid = state.current_uid() as libc::uid_t;
    st.st_gid = state.current_gid() as libc::gid_t;
}

/// Modify stat buffer ownership fields using inode identity.
///
/// # Safety
/// The caller must ensure that buf is a valid pointer to a libc::stat struct.
pub(crate) unsafe fn modify_stat_buf(buf: *mut libc::stat) {
    if buf.is_null() {
        return;
    }

    if LIBRARY_INITIALIZING.load(Ordering::Acquire) {
        return;
    }

    ensure_library_init();

    if !LIBRARY_INIT_DONE.load(Ordering::Acquire) {
        unsafe {
            (*buf).st_uid = BOOT_UID.load(Ordering::Relaxed) as libc::uid_t;
            (*buf).st_gid = BOOT_GID.load(Ordering::Relaxed) as libc::gid_t;
        }
        return;
    }

    let st = unsafe { &mut *buf };
    let key = key_from_stat(st);
    if let Some(inode) = get_inode(key) {
        patch_inode_fields(st, &inode);
        return;
    }

    apply_default_ownership(st);
}

/// Modify statx buffer ownership fields using inode identity.
///
/// # Safety
/// The caller must ensure that buf is a valid pointer to a libc::statx struct.
#[cfg(target_os = "linux")]
pub(crate) unsafe fn modify_statx_buf(buf: *mut std::ffi::c_void) {
    if buf.is_null() {
        return;
    }

    if LIBRARY_INITIALIZING.load(Ordering::Acquire) {
        return;
    }

    ensure_library_init();

    if !LIBRARY_INIT_DONE.load(Ordering::Acquire) {
        let stx = unsafe { &mut *(buf.cast::<libc::statx>()) };
        stx.stx_uid = BOOT_UID.load(Ordering::Relaxed);
        stx.stx_gid = BOOT_GID.load(Ordering::Relaxed);
        stx.stx_mask |= libc::STATX_UID | libc::STATX_GID;
        return;
    }

    let stx = unsafe { &mut *(buf.cast::<libc::statx>()) };
    let dev = libc::makedev(stx.stx_dev_major, stx.stx_dev_minor) as u64;
    let key = (dev, stx.stx_ino);
    if let Some(inode) = get_inode(key) {
        stx.stx_uid = inode.uid;
        stx.stx_gid = inode.gid;
        stx.stx_mask |= libc::STATX_UID | libc::STATX_GID;
        if let Some(mode) = inode.mode {
            stx.stx_mode = mode as u16;
            stx.stx_mask |= libc::STATX_MODE | libc::STATX_TYPE;
        }
        if let Some(rdev) = inode.rdev {
            stx.stx_rdev_major = libc::major(rdev as libc::dev_t);
            stx.stx_rdev_minor = libc::minor(rdev as libc::dev_t);
        }
        return;
    }

    let state = global_state_read();
    stx.stx_uid = state.current_uid();
    stx.stx_gid = state.current_gid();
    stx.stx_mask |= libc::STATX_UID | libc::STATX_GID;
}

fn read_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn read_xattr_value(value: *const std::ffi::c_void, size: libc::size_t) -> Vec<u8> {
    let size = size.min(MAX_XATTR);
    if size == 0 || value.is_null() {
        return Vec::new();
    }
    let mut buf = vec![0u8; size];
    unsafe {
        std::ptr::copy_nonoverlapping(value.cast::<u8>(), buf.as_mut_ptr(), size);
    }
    buf
}

fn write_xattr_value(value: &[u8], buf: *mut std::ffi::c_void, size: libc::size_t) -> i32 {
    if size == 0 {
        return value.len() as i32;
    }
    if value.len() > size {
        return -libc::ERANGE;
    }
    if !buf.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(value.as_ptr(), buf.cast::<u8>(), value.len());
        }
    }
    value.len() as i32
}

fn merge_xattr_lists(real_list: &[u8], extra: &[String]) -> Vec<u8> {
    let mut names: Vec<Vec<u8>> = Vec::new();
    for n in real_list.split(|&c| c == 0) {
        if !n.is_empty() {
            names.push(n.to_vec());
        }
    }
    for name in extra {
        let bytes = name.as_bytes().to_vec();
        if !names.contains(&bytes) {
            names.push(bytes);
        }
    }
    let mut blob = Vec::new();
    for n in &names {
        blob.extend_from_slice(n);
        blob.push(0);
    }
    blob
}

fn write_xattr_list(blob: &[u8], list: *mut c_char, size: libc::size_t) -> i32 {
    if size == 0 {
        return blob.len() as i32;
    }
    if blob.len() > size {
        return -libc::ERANGE;
    }
    if !list.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(blob.as_ptr(), list.cast::<u8>(), blob.len());
        }
    }
    blob.len() as i32
}

fn record_xattr_for_key(key: InodeKey, name: String, value: Vec<u8>) {
    update_inode(key, |inode| {
        inode.xattrs.insert(name, value);
    });
}

fn remove_xattr_for_key(key: InodeKey, name: &str) {
    update_inode(key, |inode| {
        inode.xattrs.remove(name);
    });
}

fn fake_setxattr(
    source: StatSource,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_library_init();
    let Some(name) = read_cstr(name) else {
        return -libc::EINVAL;
    };
    match source.stat() {
        Ok(st) => {
            let key = key_from_stat(&st);
            let value = read_xattr_value(value, size);
            record_xattr_for_key(key, name, value);
            0
        }
        Err(errno) => errno,
    }
}

pub(crate) fn fake_setxattr_path(
    path: *const c_char,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
    nofollow: bool,
) -> i32 {
    fake_setxattr(StatSource::Path { path, nofollow }, name, value, size)
}

pub(crate) fn fake_setxattr_fd(
    fd: i32,
    name: *const c_char,
    value: *const std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    fake_setxattr(StatSource::Fd(fd), name, value, size)
}

/// Dispatch to the real `getxattr` family matching `source`.
///
/// # Panics
/// Panics if `source` is a [`StatSource::At`] — xattr syscalls have no
/// `*at`-family variant, so callers in this module never construct one here.
fn real_getxattr_for(
    source: StatSource,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    match source {
        StatSource::Path {
            path,
            nofollow: true,
        } => unsafe { crate::platform::real_lgetxattr(path, name, value, size) },
        StatSource::Path {
            path,
            nofollow: false,
        } => unsafe { crate::platform::real_getxattr(path, name, value, size) },
        StatSource::Fd(fd) => unsafe { crate::platform::real_fgetxattr(fd, name, value, size) },
        StatSource::At { .. } => unreachable!("xattr sources are never StatSource::At"),
    }
}

fn fake_getxattr(
    source: StatSource,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    ensure_library_init();
    let Some(name_key) = read_cstr(name) else {
        return -libc::EINVAL;
    };
    if let Ok(st) = source.stat() {
        let key = key_from_stat(&st);
        if let Some(inode) = get_inode(key)
            && let Some(stored) = inode.xattrs.get(&name_key)
        {
            return write_xattr_value(stored, value, size);
        }
    }
    real_getxattr_for(source, name, value, size)
}

pub(crate) fn fake_getxattr_path(
    path: *const c_char,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
    nofollow: bool,
) -> i32 {
    fake_getxattr(StatSource::Path { path, nofollow }, name, value, size)
}

pub(crate) fn fake_getxattr_fd(
    fd: i32,
    name: *const c_char,
    value: *mut std::ffi::c_void,
    size: libc::size_t,
) -> i32 {
    fake_getxattr(StatSource::Fd(fd), name, value, size)
}

/// Dispatch to the real `listxattr` family matching `source`.
///
/// # Panics
/// Panics if `source` is a [`StatSource::At`] — xattr syscalls have no
/// `*at`-family variant, so callers in this module never construct one here.
fn real_listxattr_for(source: StatSource, list: *mut c_char, size: libc::size_t) -> i32 {
    match source {
        StatSource::Path {
            path,
            nofollow: true,
        } => unsafe { crate::platform::real_llistxattr(path, list, size) },
        StatSource::Path {
            path,
            nofollow: false,
        } => unsafe { crate::platform::real_listxattr(path, list, size) },
        StatSource::Fd(fd) => unsafe { crate::platform::real_flistxattr(fd, list, size) },
        StatSource::At { .. } => unreachable!("xattr sources are never StatSource::At"),
    }
}

fn fake_listxattr(source: StatSource, list: *mut c_char, size: libc::size_t) -> i32 {
    ensure_library_init();
    let extra = if let Ok(st) = source.stat() {
        get_inode(key_from_stat(&st))
            .map(|inode| inode.xattrs.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let real_ret = real_listxattr_for(source, list, size);
    if extra.is_empty() {
        return real_ret;
    }

    let real_list = if real_ret > 0 && !list.is_null() {
        let mut buf = vec![0u8; real_ret as usize];
        unsafe {
            std::ptr::copy_nonoverlapping(list.cast::<u8>(), buf.as_mut_ptr(), real_ret as usize);
        }
        buf
    } else {
        Vec::new()
    };
    let merged = merge_xattr_lists(&real_list, &extra);
    write_xattr_list(&merged, list, size)
}

pub(crate) fn fake_listxattr_path(
    path: *const c_char,
    list: *mut c_char,
    size: libc::size_t,
    nofollow: bool,
) -> i32 {
    fake_listxattr(StatSource::Path { path, nofollow }, list, size)
}

pub(crate) fn fake_listxattr_fd(fd: i32, list: *mut c_char, size: libc::size_t) -> i32 {
    fake_listxattr(StatSource::Fd(fd), list, size)
}

fn fake_removexattr(source: StatSource, name: *const c_char) -> i32 {
    ensure_library_init();
    let Some(name) = read_cstr(name) else {
        return -libc::EINVAL;
    };
    match source.stat() {
        Ok(st) => {
            remove_xattr_for_key(key_from_stat(&st), &name);
            0
        }
        Err(errno) => errno,
    }
}

pub(crate) fn fake_removexattr_path(
    path: *const c_char,
    name: *const c_char,
    nofollow: bool,
) -> i32 {
    fake_removexattr(StatSource::Path { path, nofollow }, name)
}

pub(crate) fn fake_removexattr_fd(fd: i32, name: *const c_char) -> i32 {
    fake_removexattr(StatSource::Fd(fd), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unnecessary_cast)]
    fn compose_mode_keeps_type_overrides_perms() {
        // `mode_t` is u16 on macOS, so widen explicitly (no-op on Linux).
        let real = libc::S_IFREG as u32 | 0o644;
        assert_eq!(compose_mode(real, 0o4755), libc::S_IFREG as u32 | 0o4755);
    }

    #[test]
    fn merge_xattr_lists_dedupes() {
        let real = b"user.foo\0system.posix_acl_access\0";
        let extra = vec!["security.capability".to_string()];
        let merged = merge_xattr_lists(real, &extra);
        assert!(
            merged
                .windows(20)
                .any(|w| w.starts_with(b"security.capability"))
        );
        assert!(merged.windows(8).any(|w| w.starts_with(b"user.foo")));
    }
}
