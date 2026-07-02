use std::env;
use std::path::PathBuf;

fn main() {
    // Build scripts run on the host, so `cfg!(target_os)` would describe the
    // host rather than what we're compiling for. Consult the target instead so
    // the linker version script is applied when cross-compiling to Linux (and
    // skipped on macOS, which uses `__DATA,__interpose` and has no `ld` `-Wl,
    // --version-script`).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let version_script = manifest_dir.join("pseudoroot.lds");
        println!("cargo:rerun-if-changed={}", version_script.display());
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}",
            version_script.display()
        );
    }
}
