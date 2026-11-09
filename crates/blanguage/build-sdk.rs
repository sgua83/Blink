#[path = "src/build/compiler_identity.rs"]
mod compiler_identity;
use std::{env, fs::{self, OpenOptions}, os::windows::fs::OpenOptionsExt, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_COMPILER").is_none() {
        println!("cargo:rerun-if-changed=build-sdk.rs");
        return Ok(());
    }
    let _sdk_use = OpenOptions::new().read(true).share_mode(1).open("../../VERSION")?;
    let input = "native/blink_compiler_bootstrap.lib";
    println!("cargo:rerun-if-changed={input}");
    let bootstrap = fs::read(input)?;
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    fs::write(out.join("blink_compiler_bootstrap.lib"), &bootstrap)?;
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=blink_compiler_bootstrap");
    compiler_identity::emit(&bootstrap)
}
