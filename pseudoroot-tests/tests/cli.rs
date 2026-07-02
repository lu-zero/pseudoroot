//! End-to-end tests driving pseudoroot against system tools (bash/coreutils/tar).
//!
//! A shared `pdrd` backs these tests so inode state survives across separate
//! `exec` calls within each shell script (`touch`, `chown`, `stat`, …).
//!
//! Linux-only, for two independent reasons:
//! - macOS SIP strips `DYLD_INSERT_LIBRARIES` from Apple-signed binaries
//!   (`/bin/sh` and every coreutil it spawns), so nothing here would actually
//!   be interposed — the assertions would test the real filesystem, not the
//!   fake overlay. Credential/stat interposition on macOS is covered instead
//!   by the `api` and `interposition` suites via freshly built binaries.
//! - the scripts use GNU coreutils syntax (`stat -c`, `install -o/-g`,
//!   `tar --numeric-owner`) that BSD userland on macOS doesn't accept.
#![cfg(target_os = "linux")]

use pseudoroot_tests::run_pseudoroot_sh;
use std::path::Path;
use tempfile::TempDir;

fn pseudoroot_sh(dir: &Path, script: &str) -> String {
    let out = run_pseudoroot_sh(dir, script);
    assert!(
        out.status.success(),
        "pseudoroot failed: status={:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn fakes_root_identity() {
    let dir = TempDir::new().unwrap();
    assert_eq!(pseudoroot_sh(dir.path(), "id -u").trim(), "0");
    assert_eq!(pseudoroot_sh(dir.path(), "id -g").trim(), "0");
}

#[test]
fn created_files_appear_root_owned() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(dir.path(), "touch f; mkdir d; stat -c '%n %u:%g' f d");
    assert!(out.contains("f 0:0"), "got: {out}");
    assert!(out.contains("d 0:0"), "got: {out}");
}

#[test]
fn chown_to_arbitrary_uid_reads_back_via_stat() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(dir.path(), "touch g; chown 200:200 g; stat -c '%u:%g' g");
    assert_eq!(out.trim(), "200:200");
}

#[test]
fn partial_chown_keeps_fake_current_id() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(dir.path(), "touch g; chown :7 g; stat -c '%u:%g' g");
    assert_eq!(out.trim(), "0:7");
}

#[test]
fn install_and_tar_share_map_in_one_session() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(
        dir.path(),
        "touch src; install -m 644 -o 200 -g 200 src lib-installed; \
         install -m 755 -o 0 -g 0 /bin/true bin-installed; \
         tar --numeric-owner -cf out.tar lib-installed bin-installed; \
         tar --numeric-owner -tvf out.tar",
    );
    assert!(
        out.contains("0/0"),
        "root-owned bin missing from tar: {out}"
    );
    assert!(
        out.contains("200/200"),
        "installer-owned lib missing from tar: {out}"
    );
}

#[test]
fn tar_records_mixed_fake_ownership() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(
        dir.path(),
        "touch r u; chown 0:0 r; chown 200:200 u; \
         tar --numeric-owner -cf out.tar r u; tar --numeric-owner -tvf out.tar",
    );
    assert!(out.contains("0/0"), "got: {out}");
    assert!(out.contains("200/200"), "got: {out}");
}

#[cfg(target_os = "linux")]
#[test]
fn mknod_reports_device_node() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(dir.path(), "mknod cdev c 1 3 && stat -c '%n %F %t,%T' cdev");
    assert!(out.contains("character special file"), "got: {out}");
    assert!(out.contains("1,3"), "got: {out}");
}

#[test]
fn inode_reuse_does_not_inherit_stale_owner() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(
        dir.path(),
        "touch a; chown 200:200 a; rm a; touch b; stat -c '%u:%g' b",
    );
    assert_eq!(out.trim(), "0:0", "reused inode wrongly kept fake owner");
}

#[test]
fn real_filesystem_is_untouched() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    pseudoroot_sh(dir.path(), "touch realf; chown 200:200 realf");
    let real_uid = std::fs::metadata(dir.path()).unwrap().uid();
    let file_uid = std::fs::metadata(dir.path().join("realf")).unwrap().uid();
    assert_eq!(
        file_uid, real_uid,
        "pseudoroot must not really chown on disk"
    );
}

#[test]
fn bad_path_syscall_does_not_abort_tree() {
    let dir = TempDir::new().unwrap();
    let out = pseudoroot_sh(
        dir.path(),
        "mkfifo conftest.fifo/ 2>/dev/null; \
         chown 1:1 conftest.missing 2>/dev/null; \
         chmod 0644 conftest.missing 2>/dev/null; \
         echo survived",
    );
    assert_eq!(out.trim(), "survived");
}
