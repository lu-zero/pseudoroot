//! Builds `pseudoroot-lib`'s cdylib into an isolated target-dir and exposes
//! its path via `PSEUDOROOT_LIB_EMBED_PATH` so `src/lib.rs` can
//! `include_bytes!` it — `pdr` embeds the interposed library instead of
//! needing to find it on disk at runtime.
//!
//! Cargo has no stable "give me a sibling crate's build artifact" primitive
//! (the real mechanism, `-Zbindeps`/RFC 3028 artifact-dependencies, is
//! nightly-only), so this shells out to a nested `cargo build` — the only
//! option compatible with `cargo install --path pseudoroot`, since `cargo
//! install` runs a crate's own `build.rs` automatically but has no hook for
//! an external pre-build step.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("pseudoroot/build.rs: {err}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), String> {
    let target = require_env("TARGET")?; // triple, always set by Cargo
    let profile = require_env("PROFILE")?; // "debug" | "release"
    let manifest_dir = PathBuf::from(require_env("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir
        .parent()
        .ok_or("pseudoroot should live directly under the workspace root")?;
    let out_dir = PathBuf::from(require_env("OUT_DIR")?);
    let embed_target_dir = out_dir.join("embed-target");
    let cargo = require_env("CARGO")?;

    let mut cmd = Command::new(&cargo);
    cmd.args([
        "build",
        "-p",
        "pseudoroot-lib",
        "--target",
        &target,
        "--target-dir",
    ])
    .arg(&embed_target_dir)
    .arg("--offline") // never touch the network independently of the outer build
    .current_dir(workspace_root);
    if profile == "release" {
        cmd.arg("--release");
    }
    let status = cmd
        .status()
        .map_err(|err| format!("failed to spawn `{cargo} build -p pseudoroot-lib`: {err}"))?;
    if !status.success() {
        return Err(format!(
            "nested `cargo build -p pseudoroot-lib` failed ({status})"
        ));
    }

    let lib_name = if target.contains("apple") {
        "libpseudoroot_lib.dylib"
    } else {
        "libpseudoroot_lib.so"
    };
    let lib_path = embed_target_dir.join(&target).join(&profile).join(lib_name);
    if !lib_path.exists() {
        return Err(format!(
            "expected {} to exist after nested build",
            lib_path.display()
        ));
    }
    println!(
        "cargo:rustc-env=PSEUDOROOT_LIB_EMBED_PATH={}",
        lib_path.display()
    );

    // Without this, Cargo's default rerun heuristic only tracks files inside
    // THIS package — editing pseudoroot-lib or pseudoroot-core would not
    // rerun this script, silently embedding stale bytes.
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("pseudoroot-lib/src").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("pseudoroot-lib/Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("pseudoroot-core/src").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.lock").display()
    );
    Ok(())
}

fn require_env(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("{key} not set (expected to run under `cargo build`)"))
}
