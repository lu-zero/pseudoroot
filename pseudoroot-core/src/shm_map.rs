//! Experimental shared-memory inode map for session mode (Linux).
//!
//! A memfd-backed table is created by the session supervisor and inherited by
//! preloaded children via `PSEUDOROOT_SHM_FD`, avoiding per-stat Unix socket RPC.

use crate::state::{FakeInode, InodeKey};
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Environment variable holding the inherited memfd file descriptor.
pub const SHM_FD_ENV: &str = "PSEUDOROOT_SHM_FD";

/// Environment variable holding the shared map byte length (avoids `fstat` on inherited memfds).
pub const SHM_LEN_ENV: &str = "PSEUDOROOT_SHM_LEN";

const SHM_MAGIC: u32 = 0x5044_5253; // "PDRS"
const SHM_VERSION: u32 = 2;
const ID_UNCHANGED: u32 = u32::MAX;

#[repr(C, align(64))]
struct Header {
    magic: u32,
    version: u32,
    slot_count: u32,
    current_uid: AtomicU32,
    current_gid: AtomicU32,
}

#[repr(C, align(8))]
struct Slot {
    occupied: AtomicU32,
    dev: u64,
    ino: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    rdev: u64,
}

/// mmap-backed inode table shared across a fakeroot session.
pub struct ShmInodeMap {
    base: *mut u8,
    len: usize,
    slot_count: u32,
    #[cfg(target_os = "linux")]
    _fd: std::os::fd::OwnedFd,
}

impl ShmInodeMap {
    /// Create a new shared map and return it together with the memfd (CLOEXEC set).
    ///
    /// # Errors
    /// Returns an error when memfd/mmap setup fails.
    #[cfg(target_os = "linux")]
    pub fn create(slot_count: u32, uid: u32, gid: u32) -> io::Result<Arc<Self>> {
        use std::os::fd::{AsRawFd, FromRawFd, RawFd};
        use std::ptr;

        let slot_count = slot_count.next_power_of_two().max(1024);
        let header_size = std::mem::size_of::<Header>();
        debug_assert_eq!(header_size % std::mem::align_of::<Slot>(), 0);
        let slots_size = std::mem::size_of::<Slot>()
            .checked_mul(slot_count as usize)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "slot table too large"))?;
        let len = header_size + slots_size;

        let fd = unsafe { libc::memfd_create(c"pseudoroot".as_ptr().cast(), 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as RawFd) };
        if unsafe { libc::ftruncate(owned.as_raw_fd(), len as libc::off_t) } < 0 {
            return Err(io::Error::last_os_error());
        }

        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                owned.as_raw_fd(),
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let map = Arc::new(Self {
            base: base.cast(),
            len,
            slot_count,
            _fd: owned,
        });
        map.init_header(uid, gid);
        Ok(map)
    }

    /// Map an inherited memfd passed through the environment.
    ///
    /// # Errors
    /// Returns an error when the descriptor is invalid or the map is corrupt.
    #[cfg(target_os = "linux")]
    pub fn from_fd(fd: std::os::fd::RawFd) -> io::Result<Arc<Self>> {
        use std::env;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
        use std::ptr;

        // Duplicate the inherited descriptor so we own a private table entry.
        // Taking ownership of the inherited fd trips Rust IO-safety checks and
        // can close a descriptor the session supervisor still relies on.
        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let owned = unsafe { OwnedFd::from_raw_fd(dup_fd as RawFd) };
        let len = match env::var(SHM_LEN_ENV)
            .ok()
            .and_then(|value| value.parse().ok())
        {
            Some(len) if len >= std::mem::size_of::<Header>() => len,
            _ => {
                let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
                if unsafe { libc::fstat(owned.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
                    return Err(io::Error::last_os_error());
                }
                (unsafe { stat.assume_init().st_size }) as usize
            }
        };
        if len < std::mem::size_of::<Header>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shm map too small",
            ));
        }

        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                owned.as_raw_fd(),
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let header = unsafe { &*(base.cast::<Header>()) };
        if header.magic != SHM_MAGIC || header.version != SHM_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shm map header mismatch",
            ));
        }

        Ok(Arc::new(Self {
            base: base.cast(),
            len,
            slot_count: header.slot_count,
            _fd: owned,
        }))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn create(_slot_count: u32, _uid: u32, _gid: u32) -> io::Result<Arc<Self>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "shared-memory sessions require Linux",
        ))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn from_fd(_fd: std::os::fd::RawFd) -> io::Result<Arc<Self>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "shared-memory sessions require Linux",
        ))
    }

    /// File descriptor children inherit (supervisor clears CLOEXEC before spawn).
    #[cfg(target_os = "linux")]
    pub fn inherited_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self._fd.as_raw_fd()
    }

    /// Mapped byte length (pass to children via [`SHM_LEN_ENV`]).
    #[must_use]
    pub fn map_len(&self) -> usize {
        self.len
    }

    #[inline]
    #[must_use]
    pub fn current_uid(&self) -> u32 {
        self.header().current_uid.load(Ordering::Relaxed)
    }

    #[inline]
    #[must_use]
    pub fn current_gid(&self) -> u32 {
        self.header().current_gid.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_current(&self, uid: u32, gid: u32) {
        let header = self.header();
        header.current_uid.store(uid, Ordering::Relaxed);
        header.current_gid.store(gid, Ordering::Relaxed);
    }

    #[must_use]
    pub fn get_inode(&self, key: InodeKey) -> Option<FakeInode> {
        let (dev, ino) = key;
        let mut idx = self.hash(dev, ino);
        for _ in 0..self.slot_count {
            let slot = self.slot(idx);
            if slot.occupied.load(Ordering::Acquire) == 0 {
                return None;
            }
            if slot.dev == dev && slot.ino == ino {
                let mut inode = FakeInode::new(slot.uid, slot.gid);
                if slot.mode != 0 {
                    inode.mode = Some(slot.mode);
                }
                if slot.rdev != 0 {
                    inode.rdev = Some(slot.rdev);
                }
                return Some(inode);
            }
            idx = (idx + 1) % self.slot_count;
        }
        None
    }

    pub fn upsert_chown(
        &self,
        key: InodeKey,
        uid: u32,
        gid: u32,
        default_uid: u32,
        default_gid: u32,
    ) {
        let (dev, ino) = key;
        let mut idx = self.hash(dev, ino);
        for _ in 0..self.slot_count {
            let slot = self.slot(idx);
            match slot.occupied.load(Ordering::Acquire) {
                0 => {
                    let new_uid = if uid == ID_UNCHANGED {
                        default_uid
                    } else {
                        uid
                    };
                    let new_gid = if gid == ID_UNCHANGED {
                        default_gid
                    } else {
                        gid
                    };
                    slot.dev = dev;
                    slot.ino = ino;
                    slot.uid = new_uid;
                    slot.gid = new_gid;
                    slot.mode = 0;
                    slot.rdev = 0;
                    slot.occupied.store(1, Ordering::Release);
                    return;
                }
                _ if slot.dev == dev && slot.ino == ino => {
                    if uid != ID_UNCHANGED {
                        slot.uid = uid;
                    }
                    if gid != ID_UNCHANGED {
                        slot.gid = gid;
                    }
                    return;
                }
                _ => {
                    idx = (idx + 1) % self.slot_count;
                }
            }
        }
    }

    /// Insert or replace full inode metadata for `key`.
    pub fn upsert_inode(&self, key: InodeKey, inode: &FakeInode) {
        let (dev, ino) = key;
        let mut idx = self.hash(dev, ino);
        for _ in 0..self.slot_count {
            let slot = self.slot(idx);
            match slot.occupied.load(Ordering::Acquire) {
                0 => {
                    slot.dev = dev;
                    slot.ino = ino;
                    slot.uid = inode.uid;
                    slot.gid = inode.gid;
                    slot.mode = inode.mode.unwrap_or(0);
                    slot.rdev = inode.rdev.unwrap_or(0);
                    slot.occupied.store(1, Ordering::Release);
                    return;
                }
                _ if slot.dev == dev && slot.ino == ino => {
                    slot.uid = inode.uid;
                    slot.gid = inode.gid;
                    slot.mode = inode.mode.unwrap_or(0);
                    slot.rdev = inode.rdev.unwrap_or(0);
                    return;
                }
                _ => {
                    idx = (idx + 1) % self.slot_count;
                }
            }
        }
    }

    fn init_header(&self, uid: u32, gid: u32) {
        let header = self.header_mut();
        header.magic = SHM_MAGIC;
        header.version = SHM_VERSION;
        header.slot_count = self.slot_count;
        header.current_uid.store(uid, Ordering::Relaxed);
        header.current_gid.store(gid, Ordering::Relaxed);
        for i in 0..self.slot_count {
            self.slot(i).occupied.store(0, Ordering::Relaxed);
        }
    }

    fn header(&self) -> &Header {
        unsafe { &*(self.base.cast::<Header>()) }
    }

    #[allow(clippy::mut_from_ref)] // mmap-backed interior mutability
    fn header_mut(&self) -> &mut Header {
        unsafe { &mut *(self.base.cast::<Header>()) }
    }

    #[allow(clippy::mut_from_ref)] // mmap-backed interior mutability
    fn slot(&self, index: u32) -> &mut Slot {
        let offset = std::mem::size_of::<Header>() + std::mem::size_of::<Slot>() * index as usize;
        unsafe { &mut *(self.base.add(offset).cast::<Slot>()) }
    }

    fn hash(&self, dev: u64, ino: u64) -> u32 {
        let mixed = dev ^ ino.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (mixed % self.slot_count as u64) as u32
    }
}

unsafe impl Send for ShmInodeMap {}
unsafe impl Sync for ShmInodeMap {}

impl Drop for ShmInodeMap {
    fn drop(&mut self) {
        if !self.base.is_null() {
            unsafe {
                libc::munmap(self.base.cast(), self.len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn shm_spawn_preloaded_child_exits() {
        use std::process::Command;

        let map = ShmInodeMap::create(256, 0, 0).unwrap();
        let fd = map.inherited_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags >= 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            }
        }

        let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/debug/libpseudoroot_lib.so");
        if !lib.exists() {
            return; // built without pseudoroot-lib in debug
        }

        for program in ["/bin/true", "sh"] {
            let mut cmd = Command::new(program);
            if program == "sh" {
                cmd.arg("-c").arg("echo ok");
            }
            cmd.env(SHM_FD_ENV, fd.to_string());
            cmd.env(SHM_LEN_ENV, map.map_len().to_string());
            cmd.env("LD_PRELOAD", &lib);
            cmd.env("PSEUDOROOT_UID", "0");
            cmd.env("PSEUDOROOT_GID", "0");
            let status = cmd
                .status()
                .unwrap_or_else(|err| panic!("{program} should start: {err}"));
            assert!(status.success(), "{program} should exit 0");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn shm_roundtrip_chown_and_lookup() {
        let map = ShmInodeMap::create(256, 0, 0).unwrap();
        map.upsert_chown((9, 42), 1000, 2000, 0, 0);
        let inode = map.get_inode((9, 42)).unwrap();
        assert_eq!(inode.uid, 1000);
        assert_eq!(inode.gid, 2000);
        map.upsert_chown((9, 42), ID_UNCHANGED, 7, 0, 0);
        let inode = map.get_inode((9, 42)).unwrap();
        assert_eq!(inode.uid, 1000);
        assert_eq!(inode.gid, 7);
    }
}
