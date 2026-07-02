use std::env;
use std::path::PathBuf;

fn main() {
    // Build scripts run on the host, so `cfg!(target_os)` would describe the
    // host rather than what we're compiling for. Consult the target instead so
    // the linker version script is applied when cross-compiling to Linux (and
    // skipped on macOS, which uses `__DATA,__interpose` and has no `ld` `-Wl,
    // --version-script`).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        // The version script lives with the bundled interposition source under
        // `pseudoroot/interpose/` (this thin shim package has no `src/` of its own).
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let version_script = manifest_dir
            .join("..")
            .join("pseudoroot")
            .join("interpose")
            .join("pseudoroot.lds");
        println!("cargo:rerun-if-changed={}", version_script.display());
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}",
            version_script.display()
        );
        // The script only pins `statx` to the `GLIBC_2.28` version node; any
        // symbol it does not list defaults to local and would be hidden. Re-export
        // every other interposed entry at the base (unversioned) version so that
        // versioned glibc lookups (e.g. `getuid@@GLIBC_2.2.6`) still resolve via
        // LD_PRELOAD. `--export-dynamic` is used instead of an anonymous
        // `{ global: *; };` catch-all node because LLD rejects an anonymous node
        // mixed with a named one ("EOF expected"), while GNU ld accepts both.
        println!("cargo:rustc-cdylib-link-arg=-Wl,--export-dynamic");
    }
}
