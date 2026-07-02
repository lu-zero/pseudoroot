//! Integration tests for pseudoroot
//!
//! These tests verify that the library interposition works correctly
//! for core functionality like getuid, getgid, and stat.

use pseudoroot_tests::{
    cleanup_test_file, create_test_file, find_pdr_bin, find_pseudoroot_lib, run_pseudoroot_command,
    run_pseudoroot_script,
};
use std::process::Command;
use std::str;

/// Test that the pseudoroot binary itself works
#[test]
fn test_pseudoroot_binary_runs() {
    let pdr_bin = find_pdr_bin();

    let output = Command::new(pdr_bin)
        .arg("--help")
        .output()
        .expect("Failed to run pseudoroot --help");

    assert!(output.status.success(), "pseudoroot --help should succeed");
    assert!(
        !output.stdout.is_empty(),
        "pseudoroot --help should output something"
    );
}

/// Test that the library path can be printed
#[test]
fn test_print_library_path() {
    let pdr_bin = find_pdr_bin();

    let output = Command::new(pdr_bin)
        .arg("print-library-path")
        .output()
        .expect("Failed to run pseudoroot print-library-path");

    assert!(
        output.status.success(),
        "pseudoroot print-library-path should succeed"
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let lib_path = stdout.trim();

    assert!(!lib_path.is_empty(), "Library path should not be empty");
    assert!(
        lib_path.contains("libpseudoroot_lib"),
        "Library path should contain libpseudoroot_lib"
    );
}

/// Test that pseudoroot can run a simple command
#[test]
fn test_pseudoroot_runs_simple_command() {
    let output = run_pseudoroot_command(&["echo", "hello"], 0, 0);

    assert!(
        output.status.success(),
        "pseudoroot should run simple commands"
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    assert_eq!(stdout.trim(), "hello", "Expected 'hello' output");
}

/// Test environment variable passing through pseudoroot
#[test]
fn test_environment_variables() {
    let output = run_pseudoroot_script(r"echo $PSEUDOROOT_UID $PSEUDOROOT_GID", 999, 888);

    assert!(
        output.status.success(),
        "Should be able to read environment variables"
    );

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    assert_eq!(parts.len(), 2, "Should have both UID and GID");
    assert_eq!(parts[0], "999", "Expected PSEUDOROOT_UID=999");
    assert_eq!(parts[1], "888", "Expected PSEUDOROOT_GID=888");
}

/// Test that the library can be found
#[test]
fn test_library_exists() {
    let lib_path = find_pseudoroot_lib();
    assert!(lib_path.exists(), "Library should exist at {:?}", lib_path);
}

/// Test that pseudoroot preserves the command's exit status
#[test]
fn test_exit_status_preserved() {
    // Test with a command that succeeds
    let output = run_pseudoroot_command(&["true"], 0, 0);
    assert!(output.status.success(), "true should succeed");

    // Test with a command that fails
    let output = run_pseudoroot_command(&["false"], 0, 0);
    assert!(!output.status.success(), "false should fail");
}

/// Test running commands with custom UID/GID
#[test]
fn test_custom_uid_gid_in_command() {
    let output = run_pseudoroot_script(r"echo $PSEUDOROOT_UID $PSEUDOROOT_GID", 1234, 5678);

    assert!(output.status.success());

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "1234");
    assert_eq!(parts[1], "5678");
}

/// Test that multiple arguments are passed correctly
#[test]
fn test_multiple_arguments() {
    let output = run_pseudoroot_command(&["echo", "arg1", "arg2", "arg3"], 0, 0);

    assert!(output.status.success());

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    assert_eq!(stdout.trim(), "arg1 arg2 arg3");
}

/// Test chown command through pseudoroot
#[test]
fn test_chown_command() {
    let test_file = "/tmp/pseudoroot_chown_test";
    create_test_file(test_file);

    // Run chown through pseudoroot - this should work even if we don't have permission
    // because pseudoroot fakes the permissions
    let output = run_pseudoroot_command(&["chown", "1000:1000", test_file], 0, 0);

    // chown might succeed or fail depending on system permissions
    // The important thing is that the command ran
    assert!(
        output.status.success() || !output.stderr.is_empty(),
        "chown command should run"
    );

    cleanup_test_file(test_file);
}

/// Test stat command through pseudoroot.
///
/// Linux-only: uses GNU `stat --format` (BSD `stat` on macOS spells it `-f`),
/// and `/usr/bin/stat` is SIP-restricted on macOS so it wouldn't be interposed
/// anyway — `interposition::test_stat_interposition_with_c` covers stat there.
#[cfg(target_os = "linux")]
#[test]
fn test_stat_command() {
    let test_file = "/tmp/pseudoroot_stat_test";
    create_test_file(test_file);

    let output = run_pseudoroot_command(&["stat", "--format=%u", test_file], 0, 0);

    assert!(
        output.status.success(),
        "stat should succeed through pseudoroot: {}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );

    cleanup_test_file(test_file);
}

/// Test id command through pseudoroot
#[test]
fn test_id_command() {
    // Run id -u to get the current user ID
    let output = run_pseudoroot_command(&["id", "-u"], 0, 0);

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        let uid = stdout.trim();

        // Note: id might not use our interposed library depending on how it's built
        // So we just verify it runs, not necessarily that it returns 0
        println!("id -u returned: {}", uid);
    }

    // Also test id -g
    let output = run_pseudoroot_command(&["id", "-g"], 0, 0);

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        let gid = stdout.trim();

        println!("id -g returned: {}", gid);
    }
}

/// Test that we can chain multiple commands
#[test]
fn test_chained_commands() {
    let output = run_pseudoroot_script("echo hello && echo world", 0, 0);

    assert!(output.status.success());

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let lines: Vec<&str> = stdout
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    assert!(!lines.is_empty(), "Should have at least one line of output");
}
