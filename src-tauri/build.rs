use std::path::Path;

fn main() {
    tauri_build::build();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-test.manifest");
        println!("cargo::rerun-if-changed={}", manifest.display());
        println!("cargo::rerun-if-env-changed=LEGIO_WINDOWS_TESTS");
        if std::env::var_os("LEGIO_WINDOWS_TESTS").is_some() {
            println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
            println!(
                "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
                manifest.display()
            );
        }
    }
}
