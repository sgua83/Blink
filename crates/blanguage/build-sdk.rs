#[path = "src/build/compiler_identity.rs"]
mod compiler_identity;
use std::{env, fs::{self, OpenOptions}, os::windows::fs::OpenOptionsExt, path::{Path, PathBuf}};

fn capture(name: &str, out: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let input = format!("native/{name}.lib");
    println!("cargo:rerun-if-changed={input}");
    let library = fs::read(input)?;
    fs::write(out.join(format!("{name}.lib")), &library)?;
    println!("cargo:rustc-link-lib=static={name}");
    Ok(library)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build-sdk.rs");
    let _sdk_use = OpenOptions::new().read(true).share_mode(1).open("../../VERSION")?;
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let value_format = capture("blink_value_format", &out)?;
    println!("cargo:rustc-link-search=native={}", out.display());
    if env::var_os("CARGO_FEATURE_COMPILER").is_some() {
        let bootstrap = capture("blink_compiler_bootstrap", &out)?;
        compiler_identity::emit(&[&value_format, &bootstrap])?;
    }
    Ok(())
}
