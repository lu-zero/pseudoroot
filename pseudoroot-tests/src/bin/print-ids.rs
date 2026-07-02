//! Minimal `id`-substitute used by the interposition tests.
//!
//! On macOS, System Integrity Protection strips `DYLD_INSERT_LIBRARIES` from
//! the environment of Apple-signed binaries (`/usr/bin/id`, `/bin/sh`, …), so
//! the library never loads into them and their `getuid`/`getgid` can't be
//! faked. This freshly built, unsigned helper is not restricted, so it lets
//! the tests exercise real credential interposition on both platforms.
//!
//! Prints the current uid and gid as `"<uid> <gid>"`.

fn main() {
    // SAFETY: getuid/getgid are always safe to call and never fail.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    println!("{uid} {gid}");
}
