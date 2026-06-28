//! Direct interposition tests
//!
//! These tests verify that the library interposition actually modifies
//! system call return values by running a test program through pseudoroot.

use pseudoroot_tests::{find_pseudoroot_bin, create_test_file, cleanup_test_file};
use std::process::Command;
use std::str;

/// Helper to compile and run a C program through pseudoroot
fn run_c_program_through_pseudoroot(
    pseudoroot_bin: &std::path::PathBuf,
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
    
    // Run through pseudoroot
    let output = Command::new(pseudoroot_bin)
        .arg("--uid")
        .arg(uid.to_string())
        .arg("--gid")
        .arg(gid.to_string())
        .arg(c_executable)
        .output()
        .ok()?;
    
    Some(output)
}

/// Test that getuid returns the fake UID by using a small C program
#[test]
fn test_getuid_interposition_with_c() {
    let pseudoroot_bin = find_pseudoroot_bin();
    
    let c_program = r##"#include <stdio.h>
#include <unistd.h>
int main() {
    printf("%05u %05u\n", getuid(), getgid());
    return 0;
}
"##;
    
    let _ = std::fs::write("/tmp/test_getuid_c.c", c_program);
    
    let output = run_c_program_through_pseudoroot(
        &pseudoroot_bin,
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
    
    assert!(output.status.success(), "Test program should run successfully");
    
    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let trimmed = stdout.trim();
    assert_eq!(trimmed, "12345 67890", "Expected fake UID 12345 and GID 67890, got {}", trimmed);
    
    let _ = std::fs::remove_file("/tmp/test_getuid_c");
    let _ = std::fs::remove_file("/tmp/test_getuid_c.c");
}

/// Test stat interposition
#[test]
fn test_stat_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_interpose_test";
    create_test_file(test_file);
    
    let pseudoroot_bin = find_pseudoroot_bin();
    
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
    
    let output = run_c_program_through_pseudoroot(
        &pseudoroot_bin,
        "/tmp/test_stat_c.c",
        "/tmp/test_stat_c",
        55555,
        77777,
    );
    
    let output = match output {
        Some(o) => o,
        None => {
            cleanup_test_file(test_file);
            let _ = std::fs::remove_file("/tmp/test_stat_c.c");
            return;
        }
    };
    
    assert!(output.status.success(), "Test program should run successfully");
    
    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let trimmed = stdout.trim();
    assert_eq!(trimmed, "55555 77777", "Expected fake UID 55555 and GID 77777, got {}", trimmed);
    
    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_stat_c");
    let _ = std::fs::remove_file("/tmp/test_stat_c.c");
}

/// Test chown interposition
#[test]
fn test_chown_interposition_with_c() {
    let test_file = "/tmp/pseudoroot_chown_test";
    create_test_file(test_file);
    
    let pseudoroot_bin = find_pseudoroot_bin();
    
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
    
    let output = run_c_program_through_pseudoroot(
        &pseudoroot_bin,
        "/tmp/test_chown_c.c",
        "/tmp/test_chown_c",
        0,
        0,
    );
    
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
        assert_eq!(trimmed, "99999 88888", "Expected fake UID 99999 and GID 88888, got {}", trimmed);
    }
    
    cleanup_test_file(test_file);
    let _ = std::fs::remove_file("/tmp/test_chown_c");
    let _ = std::fs::remove_file("/tmp/test_chown_c.c");
}
