//! Builds the interposed cdylib from the source bundled at `interpose/` and
//! exposes its path via `PSEUDOROOT_LIB_EMBED_PATH` so `src/lib.rs` can
//! `include_bytes!` it — `pdr` embeds the interposed library instead of
//! needing to find it on disk at runtime.
//!
//! The interposition source ships *inside* this package (not as a sibling
//! Cargo package), so `cargo install pseudoroot` from a registry stays
//! self-contained: there's no stable Cargo primitive for "give me a cdylib
//! built from a sibling crate" (RFC 3028 artifact dependencies is
//! nightly-only), so this synthesizes a throwaway cdylib crate in `OUT_DIR`,
//! compiles the bundled source with it, and reads back the artifact.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
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
    let out_dir = PathBuf::from(require_env("OUT_DIR")?);
    let cargo = require_env("CARGO")?;

    let interpose = manifest_dir.join("interpose");
    let gen_dir = out_dir.join("interpose-pkg");

    // (Re)generate the throwaway package: copy the bundled source + version
    // script, and write a manifest + build script that pins `statx` to
    // `GLIBC_2.28` and re-exports the rest (`--export-dynamic`).
    if gen_dir.exists() {
        fs::remove_dir_all(&gen_dir).map_err(|e| format!("clean {gen_dir:?}: {e}"))?;
    }
    copy_dir(&interpose.join("src"), &gen_dir.join("src"))?;
    fs::copy(
        interpose.join("pseudoroot.lds"),
        gen_dir.join("pseudoroot.lds"),
    )
    .map_err(|e| format!("copy version script: {e}"))?;

    let mut manifest = MANIFEST.to_string();
    // During in-workspace development `pseudoroot-core` is not on any registry,
    // so redirect it to the workspace path. A registry install has no such
    // neighbour and resolves `pseudoroot-core` from the cache (it is already a
    // dependency of this crate).
    let ws_core = manifest_dir.parent().map(|p| p.join("pseudoroot-core"));
    if let Some(core) = ws_core
        && core.join("Cargo.toml").exists()
    {
        manifest.push_str(&format!(
            "\n[patch.crates-io]\npseudoroot-core = {{ path = {:?} }}\n",
            core.display()
        ));
    }
    fs::write(gen_dir.join("Cargo.toml"), manifest).map_err(|e| format!("write manifest: {e}"))?;
    fs::write(gen_dir.join("build.rs"), EMBED_BUILD_SCRIPT)
        .map_err(|e| format!("write build script: {e}"))?;

    let embed_target_dir = out_dir.join("embed-target");
    let mut cmd = Command::new(&cargo);
    cmd.args([
        "build",
        "--manifest-path",
        gen_dir.join("Cargo.toml").to_str().ok_or("non-utf8 path")?,
        "--target",
        &target,
        "--target-dir",
    ])
    .arg(&embed_target_dir)
    // No `--offline`: a registry install resolves `pseudoroot-core` (and
    // ctor/libc) from crates.io, which needs the index. In-workspace dev
    // builds still avoid the network for `pseudoroot-core` via the [patch] above.
    .current_dir(&out_dir);
    if profile == "release" {
        cmd.arg("--release");
    }
    let status = cmd
        .status()
        .map_err(|err| format!("failed to spawn nested build: {err}"))?;
    if !status.success() {
        return Err(format!("nested interpose build failed ({status})"));
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

    // Without these, editing the bundled source wouldn't rerun this script.
    println!("cargo:rerun-if-changed={}", interpose.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        interpose.join("pseudoroot.lds").display()
    );
    Ok(())
}

/// The throwaway cdylib package manifest. `[workspace]` keeps it an isolated
/// workspace root so it never merges with the surrounding repo workspace.
const MANIFEST: &str = r#"[package]
name = "pseudoroot-lib-embed"
version = "0.2.0"
edition = "2024"
build = "build.rs"
publish = false

[lib]
name = "pseudoroot_lib"
crate-type = ["cdylib"]
path = "src/lib.rs"

[dependencies]
ctor = "0.2"
libc = "0.2"
pseudoroot-core = "0.2.0"

[workspace]
"#;

/// Build script for the throwaway package: applies the version script on Linux
/// (same logic as `pseudoroot-lib/build.rs`, which builds the standalone lib).
const EMBED_BUILD_SCRIPT: &str = r#"fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let lds = std::path::Path::new(&dir).join("pseudoroot.lds");
        println!("cargo:rerun-if-changed={}", lds.display());
        println!("cargo:rustc-cdylib-link-arg=-Wl,--version-script={}", lds.display());
        println!("cargo:rustc-cdylib-link-arg=-Wl,--export-dynamic");
    }
}
"#;

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("readdir: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &dst.join(entry.file_name()))?;
        } else {
            fs::copy(&path, dst.join(entry.file_name()))
                .map_err(|e| format!("copy {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn require_env(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("{key} not set (expected to run under `cargo build`)"))
}
