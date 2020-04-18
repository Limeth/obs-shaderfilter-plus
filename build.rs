use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=legacy_stdio_definitions");
    }
}
