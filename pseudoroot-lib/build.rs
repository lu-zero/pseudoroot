use std::env;
use std::path::PathBuf;

fn main() {
    #[cfg(target_os = "linux")]
    {
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
