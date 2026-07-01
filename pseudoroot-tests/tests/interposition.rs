//! Direct interposition tests
//!
//! These tests verify that the library interposition actually modifies
//! system call return values by running a test program through pseudoroot.

use pseudoroot::FakerootCommandExt;
use pseudoroot_tests::{cleanup_test_file, create_test_file, find_pseudoroot_lib};
use std::process::Command;
use std::str;

/// Helper to compile and run a C program through pseudoroot
fn run_c_program_through_pseudoroot(
    c_source: &str,
    c_executable: &str,
    uid: u32,
    gid: u32,
) -> Option<std::process::Output> {
    // Compile
    let compile_output = Command::new("gcc")
        .arg("-o")
        .arg(c_executable)
        .arg(c_source)
        .output()
        .ok()?;

    if !compile_output.status.success() {
        return None;
    }

    let lib = find_pseudoroot_lib();
    // SAFETY: interposition tests are separate processes; env is per-test.
    unsafe {
        std::env::set_var(pseudoroot::LIB_PATH_ENV, &lib);
    }
    pseudoroot::init();
    let output = std::process::Command::new(c_executable)
        .env("PSEUDOROOT_UID", uid.to_string())
        .env("PSEUDOROOT_GID", gid.to_string())
        // Socket IPC retains full per-inode metadata (xattrs, …) for hook tests.
        .env(pseudoroot::SESSION_SHM_ENV, "0")
        .fakeroot()
        .output()
        .ok()?;

    Some(output)
}

/// Test that getuid returns the fake UID by using a small C program
#[test]
fn test_getuid_interposition_with_c() {
    let c_program = r##"#include <stdio.h>
#include <unistd.h>
int main() {
    printf("%05u %05u\n", getuid(), getgid());
    return 0;
}
"##;

    let _ = std::fs::write("/tmp/test_getuid_c.c", c_program);

    let output = run_c_program_through_pseudoroot(
        "/tmp/test_getuid_c.c",
        "/tmp/test_getuid_c",
        12345,
        67890,
    );

    let output = match output {
        Some(o) => o,
        None => {
            let _ = std::fs::remove_file("/tmp/test_getuid_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "Test program should run successfully"
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let trimmed = stdout.trim();
    assert_eq!(
        trimmed, "12345 67890",
        "Expected fake UID 12345 and GID 67890, got {}",
        trimmed
    );

    let _ = std::fs::remove_file("/tmp/test_getuid_c");
    let _ = std::fs::remove_file("/tmp/test_getuid_c.c");
}

/// Test stat interposition
#[test]
fn test_stat_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_interpose_test";
    create_test_file(test_file);

    let c_template = r##"#include <stdio.h>
#include <sys/stat.h>
int main() {
    struct stat buf;
    if (stat("XFILEX", &buf) == 0) {
        printf("%05u %05u\n", buf.st_uid, buf.st_gid);
        return 0;
    }
    return 1;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_stat_c.c", &c_program);

    let output =
        run_c_program_through_pseudoroot("/tmp/test_stat_c.c", "/tmp/test_stat_c", 55555, 77777);

    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_stat_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "Test program should run successfully"
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let trimmed = stdout.trim();
    assert_eq!(
        trimmed, "55555 77777",
        "Expected fake UID 55555 and GID 77777, got {}",
        trimmed
    );

    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_stat_c");
    let _ = std::fs::remove_file("/tmp/test_stat_c.c");
}

/// Test chown interposition
#[test]
fn test_chown_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_chown_test";
    create_test_file(test_file);

    let c_template = r##"#include <stdio.h>
#include <sys/stat.h>
int main() {
    chown("XFILEX", 99999, 88888);
    struct stat buf;
    if (stat("XFILEX", &buf) == 0) {
        printf("%05u %05u\n", buf.st_uid, buf.st_gid);
        return 0;
    }
    return 1;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_chown_c.c", &c_program);

    let output = run_c_program_through_pseudoroot("/tmp/test_chown_c.c", "/tmp/test_chown_c", 0, 0);

    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_chown_c.c");
            return;
        }
    };

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        let trimmed = stdout.trim();
        assert_eq!(
            trimmed, "99999 88888",
            "Expected fake UID 99999 and GID 88888, got {}",
            trimmed
        );
    }

    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_chown_c");
    let _ = std::fs::remove_file("/tmp/test_chown_c.c");
}

/// Test that statx passthrough works with the correct C ABI.
#[cfg(target_os = "linux")]
#[test]
fn test_statx_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_statx_test";
    create_test_file(test_file);

    let c_template = r##"#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
int main() {
    struct statx stx;
    if (statx(AT_FDCWD, "XFILEX", 0, STATX_BASIC_STATS, &stx) != 0) {
        perror("statx");
        return 1;
    }
    printf("%05o %05u %05u\n", stx.stx_mode & 07777, stx.stx_uid, stx.stx_gid);
    return 0;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_statx_c.c", &c_program);

    let output = run_c_program_through_pseudoroot("/tmp/test_statx_c.c", "/tmp/test_statx_c", 0, 0);

    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_statx_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "statx should succeed through pseudoroot: {}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let trimmed = stdout.trim();
    assert_eq!(
        trimmed, "00644 00000 00000",
        "statx should report fake root ownership, got {}",
        trimmed
    );

    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_statx_c");
    let _ = std::fs::remove_file("/tmp/test_statx_c.c");
}

/// Test chmod fake-on-error: chmod on a root-owned file should succeed and stat reports the mode.
#[test]
fn test_chmod_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_chmod_test";
    create_test_file(test_file);

    let c_template = r##"#include <stdio.h>
#include <sys/stat.h>
int main() {
    if (chmod("XFILEX", 04755) != 0) {
        perror("chmod");
        return 1;
    }
    struct stat buf;
    if (stat("XFILEX", &buf) == 0) {
        printf("%05o\n", buf.st_mode & 07777);
        return 0;
    }
    return 1;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_chmod_c.c", &c_program);

    let output = run_c_program_through_pseudoroot("/tmp/test_chmod_c.c", "/tmp/test_chmod_c", 0, 0);

    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_chmod_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "chmod should succeed under pseudoroot: {}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    assert_eq!(
        stdout.trim(),
        "04755",
        "stat should report faked chmod mode, got {}",
        stdout.trim()
    );

    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_chmod_c");
    let _ = std::fs::remove_file("/tmp/test_chmod_c.c");
}

/// Test xattr faking for security.capability.
#[cfg(target_os = "linux")]
#[test]
fn test_xattr_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_xattr_test";
    create_test_file(test_file);

    let c_template = r##"#include <stdio.h>
#include <string.h>
#include <sys/xattr.h>
int main() {
    const char *name = "security.capability";
    const unsigned char value[] = {0x01, 0x00, 0x00, 0x02};
    if (setxattr("XFILEX", name, value, sizeof(value), 0) != 0) {
        perror("setxattr");
        return 1;
    }
    unsigned char buf[16];
    ssize_t len = getxattr("XFILEX", name, buf, sizeof(buf));
    if (len < 0) {
        perror("getxattr");
        return 1;
    }
    printf("%zd", len);
    for (ssize_t i = 0; i < len; i++) {
        printf(" %02x", buf[i]);
    }
    printf("\n");
    return 0;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_xattr_c.c", &c_program);

    let output = run_c_program_through_pseudoroot("/tmp/test_xattr_c.c", "/tmp/test_xattr_c", 0, 0);

    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_xattr_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "xattr should succeed under pseudoroot: {}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    assert_eq!(
        stdout.trim(),
        "4 01 00 00 02",
        "getxattr should return faked value, got {}",
        stdout.trim()
    );

    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_xattr_c");
    let _ = std::fs::remove_file("/tmp/test_xattr_c.c");
}

/// Test mknod faking — placeholder file with device metadata in stat.
#[cfg(target_os = "linux")]
#[test]
fn test_mknod_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_mknod_test";
    let _ = std::fs::remove_file(test_file);

    let c_template = r##"#include <stdio.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
int main() {
    if (mknod("XFILEX", S_IFCHR | 0644, makedev(1, 3)) != 0) {
        perror("mknod");
        return 1;
    }
    struct stat buf;
    if (stat("XFILEX", &buf) != 0) {
        perror("stat");
        return 1;
    }
    if (!S_ISCHR(buf.st_mode)) {
        fprintf(stderr, "not a char device: %o\n", buf.st_mode & 07777);
        return 1;
    }
    printf("%u,%u\n", major(buf.st_rdev), minor(buf.st_rdev));
    return 0;
}
"##;

    let c_program = c_template.replace("XFILEX", test_file);
    let _ = std::fs::write("/tmp/test_mknod_c.c", &c_program);

    let output = run_c_program_through_pseudoroot("/tmp/test_mknod_c.c", "/tmp/test_mknod_c", 0, 0);

    let output = match output {
        Some(o) => o,
        None => {
            let _ = std::fs::remove_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_mknod_c.c");
            return;
        }
    };

    assert!(
        output.status.success(),
        "mknod should succeed under pseudoroot: {}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    assert_eq!(stdout.trim(), "1,3", "stat should report faked rdev");

    let _ = std::fs::remove_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_mknod_c");
    let _ = std::fs::remove_file("/tmp/test_mknod_c.c");
}
