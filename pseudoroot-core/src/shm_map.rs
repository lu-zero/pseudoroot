//! Experimental shared-memory inode map for session mode (Linux and macOS).
//!
//! An anonymous shared-memory table is created by the session supervisor and
//! inherited by preloaded children via `PSEUDOROOT_SHM_FD`, avoiding per-stat
//! Unix socket RPC. The backing fd is a `memfd` on Linux and an immediately
//! `shm_unlink`ed POSIX shared-memory object on macOS; everything after fd
//! creation (`ftruncate`/`mmap`/fd inheritance across `exec`) is plain POSIX
//! and shared between the two.
//!
//! Layout: `Header | Slot[slot_count] | XattrPage[slot_count]`. Each slot has a
//! dedicated `XATTR_PAGE_SIZE`-byte page (indexed the same way as its `Slot`)
//! holding a bincode-serialized `xattrs` map; entries that don't fit are dropped
//! (with a warning) rather than corrupting the table.

use crate::state::{FakeInode, InodeKey};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Environment variable holding the inherited memfd file descriptor.
pub const SHM_FD_ENV: &str = "PSEUDOROOT_SHM_FD";

/// Environment variable holding the shared map byte length (avoids `fstat` on inherited memfds).
pub const SHM_LEN_ENV: &str = "PSEUDOROOT_SHM_LEN";

const SHM_MAGIC: u32 = 0x5044_5253; // "PDRS"
const SHM_VERSION: u32 = 3;
const ID_UNCHANGED: u32 = u32::MAX;

/// Per-slot inline storage for a bincode-serialized xattr map.
const XATTR_PAGE_SIZE: usize = 4096;

/// Slot has never been used.
const SLOT_EMPTY: u32 = 0;
/// Slot holds a live entry.
const SLOT_LIVE: u32 = 1;
/// Slot held an entry that was removed; skip over it when probing but treat it
/// as free for insertion. Needed because linear probing takes an empty slot as
/// its search terminator — clearing a removed slot straight to `SLOT_EMPTY`
/// would hide any later entry that hashed into the same bucket.
const SLOT_TOMBSTONE: u32 = 2;

/// Outcome of a linear probe through the slot table.
enum Probe {
    /// A live entry matching the key, at this slot index.
    Found(u32),
    /// No entry for the key; insert at this slot index.
    Vacant(u32),
    /// No entry for the key, and no empty or reusable slot within the probe
    /// bound — the table is effectively full for this bucket.
    Full,
}

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
    _fd: std::os::fd::OwnedFd,
}

/// Create an anonymous shared-memory fd (CLOEXEC set) of no particular size.
///
/// Linux uses `memfd_create`; macOS has no memfd, so it opens an exclusive
/// POSIX shared-memory object and `shm_unlink`s the name straight away — the
/// returned fd (and any `mmap`/`dup` of it, including in `exec`ed children)
/// keeps the object alive, so children inherit it by descriptor, not by name.
#[cfg(target_os = "linux")]
fn create_anon_shm_fd() -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    let fd = unsafe { libc::memfd_create(c"pseudoroot".as_ptr().cast(), 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

#[cfg(target_os = "macos")]
fn create_anon_shm_fd() -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    use std::sync::atomic::AtomicU32;

    // Darwin caps POSIX shm names at PSHMNAMLEN (31) bytes, so keep it short
    // and unique per (pid, call) to avoid colliding with a concurrent session.
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("/psr.{}.{seq}\0", std::process::id());
    let fd = unsafe {
        libc::shm_open(
            name.as_ptr().cast(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600 as libc::c_int,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // Drop the name now; the fd we return still refers to the live object.
    unsafe { libc::shm_unlink(name.as_ptr().cast()) };
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

impl ShmInodeMap {
    /// Create a new shared map and return it together with its fd (CLOEXEC set).
    ///
    /// # Errors
    /// Returns an error when the shared-memory fd, `ftruncate`, or `mmap` fails.
    pub fn create(slot_count: u32, uid: u32, gid: u32) -> io::Result<Arc<Self>> {
        use std::os::fd::AsRawFd;
        use std::ptr;

        let slot_count = slot_count.next_power_of_two().max(1024);
        let header_size = std::mem::size_of::<Header>();
        debug_assert_eq!(header_size % std::mem::align_of::<Slot>(), 0);
        let slots_size = std::mem::size_of::<Slot>()
            .checked_mul(slot_count as usize)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "slot table too large"))?;
        let xattr_region_size = XATTR_PAGE_SIZE
            .checked_mul(slot_count as usize)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "xattr region too large"))?;
        let len = header_size
            .checked_add(slots_size)
            .and_then(|v| v.checked_add(xattr_region_size))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "shm map too large"))?;

        let owned = create_anon_shm_fd()?;
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

    /// Map an inherited shared-memory fd passed through the environment.
    ///
    /// # Errors
    /// Returns an error when the descriptor is invalid or the map is corrupt.
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

    /// File descriptor children inherit (supervisor clears CLOEXEC before spawn).
    pub fn inherited_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self._fd.as_raw_fd()
    }

    /// Mapped byte length (pass to children via [`SHM_LEN_ENV`]).
    #[must_use]
    pub fn map_len(&self) -> usize {
        self.len
    }

    /// Read the current fake uid from the map header.
    #[inline]
    #[must_use]
    pub fn current_uid(&self) -> u32 {
        self.header().current_uid.load(Ordering::Relaxed)
    }

    /// Read the current fake gid from the map header.
    #[inline]
    #[must_use]
    pub fn current_gid(&self) -> u32 {
        self.header().current_gid.load(Ordering::Relaxed)
    }

    /// Store the current fake uid/gid in the map header.
    #[inline]
    pub fn set_current(&self, uid: u32, gid: u32) {
        let header = self.header();
        header.current_uid.store(uid, Ordering::Relaxed);
        header.current_gid.store(gid, Ordering::Relaxed);
    }

    /// Look up fake metadata for an inode key.
    #[must_use]
    pub fn get_inode(&self, key: InodeKey) -> Option<FakeInode> {
        let (dev, ino) = key;
        let Probe::Found(idx) = self.probe(dev, ino) else {
            return None;
        };
        let slot = self.slot(idx);
        let mut inode = FakeInode::new(slot.uid, slot.gid);
        if slot.mode != 0 {
            inode.mode = Some(slot.mode);
        }
        if slot.rdev != 0 {
            inode.rdev = Some(slot.rdev);
        }
        inode.xattrs = self.read_xattrs(idx);
        Some(inode)
    }

    /// Merge a chown into the table, creating an entry when absent.
    ///
    /// `uid`/`gid` of `u32::MAX` leave that field unchanged on an existing
    /// entry. `default_uid`/`default_gid` seed a new slot.
    pub fn upsert_chown(
        &self,
        key: InodeKey,
        uid: u32,
        gid: u32,
        default_uid: u32,
        default_gid: u32,
    ) {
        let (dev, ino) = key;
        match self.probe(dev, ino) {
            Probe::Found(idx) => {
                let slot = self.slot(idx);
                if uid != ID_UNCHANGED {
                    slot.uid = uid;
                }
                if gid != ID_UNCHANGED {
                    slot.gid = gid;
                }
            }
            Probe::Vacant(idx) => {
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
                // Clear any xattrs left behind by whatever previously
                // occupied this slot before publishing it as live.
                self.write_xattrs(idx, &HashMap::new());
                let target = self.slot(idx);
                target.dev = dev;
                target.ino = ino;
                target.uid = new_uid;
                target.gid = new_gid;
                target.mode = 0;
                target.rdev = 0;
                target.occupied.store(SLOT_LIVE, Ordering::Release);
            }
            Probe::Full => {}
        }
    }

    /// Insert or replace full inode metadata for `key`.
    pub fn upsert_inode(&self, key: InodeKey, inode: &FakeInode) {
        let (dev, ino) = key;
        match self.probe(dev, ino) {
            Probe::Found(idx) => {
                let slot = self.slot(idx);
                slot.uid = inode.uid;
                slot.gid = inode.gid;
                slot.mode = inode.mode.unwrap_or(0);
                slot.rdev = inode.rdev.unwrap_or(0);
                self.write_xattrs(idx, &inode.xattrs);
            }
            Probe::Vacant(idx) => {
                self.write_xattrs(idx, &inode.xattrs);
                let target = self.slot(idx);
                target.dev = dev;
                target.ino = ino;
                target.uid = inode.uid;
                target.gid = inode.gid;
                target.mode = inode.mode.unwrap_or(0);
                target.rdev = inode.rdev.unwrap_or(0);
                target.occupied.store(SLOT_LIVE, Ordering::Release);
            }
            Probe::Full => {}
        }
    }

    /// Tombstone the entry for `key`, if present. Returns whether one was found.
    ///
    /// This is a linear-probed table, so a removed slot can't simply go back to
    /// `SLOT_EMPTY`: that would stop the probe early and hide any later entry
    /// that hashed into the same bucket. [`Self::upsert_chown`] and
    /// [`Self::upsert_inode`] treat tombstones as reusable on insert.
    pub fn remove_inode(&self, key: InodeKey) -> bool {
        let (dev, ino) = key;
        let Probe::Found(idx) = self.probe(dev, ino) else {
            return false;
        };
        self.write_xattrs(idx, &HashMap::new());
        self.slot(idx)
            .occupied
            .store(SLOT_TOMBSTONE, Ordering::Release);
        true
    }

    /// Walk the linear probe sequence for `(dev, ino)`, stopping at a live
    /// match, the first empty slot (remembering the earliest tombstone seen
    /// along the way as the preferred insertion point), or after
    /// `slot_count` steps if the table has no room left for this key.
    #[inline]
    fn probe(&self, dev: u64, ino: u64) -> Probe {
        let mut idx = self.hash(dev, ino);
        let mut reuse: Option<u32> = None;
        for _ in 0..self.slot_count {
            let slot = self.slot(idx);
            match slot.occupied.load(Ordering::Acquire) {
                SLOT_EMPTY => return Probe::Vacant(reuse.unwrap_or(idx)),
                SLOT_LIVE if slot.dev == dev && slot.ino == ino => return Probe::Found(idx),
                SLOT_TOMBSTONE if reuse.is_none() => reuse = Some(idx),
                _ => {}
            }
            idx = (idx + 1) % self.slot_count;
        }
        Probe::Full
    }

    /// Offset of the xattr page region, right after the slot table.
    fn xattr_region_offset(&self) -> usize {
        std::mem::size_of::<Header>() + std::mem::size_of::<Slot>() * self.slot_count as usize
    }

    #[allow(clippy::mut_from_ref)] // mmap-backed interior mutability
    fn xattr_page(&self, index: u32) -> &mut [u8] {
        let offset = self.xattr_region_offset() + XATTR_PAGE_SIZE * index as usize;
        unsafe { std::slice::from_raw_parts_mut(self.base.add(offset), XATTR_PAGE_SIZE) }
    }

    /// Serialize `xattrs` into slot `index`'s page. Silently drops (with a
    /// warning) anything that doesn't fit in [`XATTR_PAGE_SIZE`] bytes.
    fn write_xattrs(&self, index: u32, xattrs: &HashMap<String, Vec<u8>>) {
        let page = self.xattr_page(index);
        if xattrs.is_empty() {
            page[..4].copy_from_slice(&0u32.to_le_bytes());
            return;
        }
        let fits = bincode::serialize(xattrs)
            .ok()
            .filter(|bytes| bytes.len() <= XATTR_PAGE_SIZE - 4);
        match fits {
            Some(bytes) => {
                page[..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
                page[4..4 + bytes.len()].copy_from_slice(&bytes);
            }
            None => {
                eprintln!(
                    "pseudoroot: xattrs exceed the {XATTR_PAGE_SIZE}B session limit; not faked"
                );
                page[..4].copy_from_slice(&0u32.to_le_bytes());
            }
        }
    }

    fn read_xattrs(&self, index: u32) -> HashMap<String, Vec<u8>> {
        let page = self.xattr_page(index);
        let len = u32::from_le_bytes(page[..4].try_into().unwrap()) as usize;
        if len == 0 || len > XATTR_PAGE_SIZE - 4 {
            return HashMap::new();
        }
        bincode::deserialize(&page[4..4 + len]).unwrap_or_default()
    }

    fn init_header(&self, uid: u32, gid: u32) {
        let header = self.header_mut();
        header.magic = SHM_MAGIC;
        header.version = SHM_VERSION;
        header.slot_count = self.slot_count;
        header.current_uid.store(uid, Ordering::Relaxed);
        header.current_gid.store(gid, Ordering::Relaxed);
        for i in 0..self.slot_count {
            self.slot(i).occupied.store(SLOT_EMPTY, Ordering::Relaxed);
        }
        // Xattr pages start zeroed (fresh memfd pages are zero-filled), so
        // their length prefixes already read as "no xattrs" — no init needed.
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

    #[test]
    fn shm_remove_then_relookup_returns_none() {
        let map = ShmInodeMap::create(256, 0, 0).unwrap();
        map.upsert_chown((9, 42), 1000, 2000, 0, 0);
        assert!(map.remove_inode((9, 42)));
        assert!(map.get_inode((9, 42)).is_none());
        // Removing again (already a tombstone) reports no entry present.
        assert!(!map.remove_inode((9, 42)));
    }

    #[test]
    fn shm_remove_does_not_break_probing_for_colliding_key() {
        // Force a collision: create a table small enough that two distinct
        // keys land in the same bucket, then make sure removing the first
        // entry doesn't hide the second behind the new tombstone.
        let map = ShmInodeMap::create(1, 0, 0).unwrap(); // rounds up to 1024 slots
        let (dev_a, ino_a) = (1u64, 1u64);
        let (dev_b, ino_b) = (dev_a, ino_a + 1024); // same bucket, mod slot_count
        map.upsert_chown((dev_a, ino_a), 10, 20, 0, 0);
        map.upsert_chown((dev_b, ino_b), 30, 40, 0, 0);

        assert!(map.remove_inode((dev_a, ino_a)));
        assert!(map.get_inode((dev_a, ino_a)).is_none());

        let inode_b = map.get_inode((dev_b, ino_b)).unwrap();
        assert_eq!(inode_b.uid, 30);
        assert_eq!(inode_b.gid, 40);
    }

    #[test]
    fn shm_reinsert_into_tombstone_does_not_leak_stale_data() {
        let map = ShmInodeMap::create(1, 0, 0).unwrap(); // rounds up to 1024 slots
        let (dev, ino_a) = (1u64, 1u64);
        let ino_b = ino_a + 1024; // same bucket, mod slot_count

        let mut inode_a = FakeInode::new(10, 20);
        inode_a
            .xattrs
            .insert("security.capability".to_string(), vec![1, 2, 3]);
        map.upsert_inode((dev, ino_a), &inode_a);
        assert!(map.remove_inode((dev, ino_a)));

        // A fresh key that probes through the tombstone must not inherit its
        // uid/gid/mode/xattrs when it lands on the recycled slot.
        map.upsert_chown((dev, ino_b), 99, 99, 0, 0);
        let inode_b = map.get_inode((dev, ino_b)).unwrap();
        assert_eq!(inode_b.uid, 99);
        assert_eq!(inode_b.gid, 99);
        assert!(inode_b.xattrs.is_empty());
    }

    #[test]
    fn shm_xattr_roundtrip_through_upsert_inode() {
        let map = ShmInodeMap::create(256, 0, 0).unwrap();
        let mut inode = FakeInode::new(1000, 2000);
        inode
            .xattrs
            .insert("security.capability".to_string(), vec![0x01, 0x00, 0x02]);
        inode
            .xattrs
            .insert("user.pax.flags".to_string(), b"m".to_vec());
        map.upsert_inode((9, 43), &inode);

        let read_back = map.get_inode((9, 43)).unwrap();
        assert_eq!(
            read_back.xattrs.get("security.capability"),
            Some(&vec![0x01, 0x00, 0x02])
        );
        assert_eq!(read_back.xattrs.get("user.pax.flags"), Some(&b"m".to_vec()));

        // upsert_chown (uid/gid-only merge) must not disturb existing xattrs.
        map.upsert_chown((9, 43), 1500, ID_UNCHANGED, 0, 0);
        let read_back = map.get_inode((9, 43)).unwrap();
        assert_eq!(read_back.uid, 1500);
        assert_eq!(
            read_back.xattrs.get("security.capability"),
            Some(&vec![0x01, 0x00, 0x02])
        );
    }

    #[test]
    fn shm_oversized_xattr_blob_is_dropped_not_corrupting() {
        let map = ShmInodeMap::create(256, 0, 0).unwrap();
        let mut inode = FakeInode::new(0, 0);
        inode
            .xattrs
            .insert("user.big".to_string(), vec![0u8; XATTR_PAGE_SIZE * 2]);
        map.upsert_inode((9, 44), &inode);

        let read_back = map.get_inode((9, 44)).unwrap();
        assert_eq!(read_back.uid, 0);
        assert!(read_back.xattrs.is_empty());
    }
}
