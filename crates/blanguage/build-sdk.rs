#[path = "src/build/compiler_identity.rs"]
mod compiler_identity;
use std::{
    env,
    fs::{File, OpenOptions},
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

fn capture(
    name: &str, out: &Path, identity: Option<&mut compiler_identity::Identity>,
) -> io::Result<()> {
    let input = format!("native/{name}.lib");
    println!("cargo:rerun-if-changed={input}");
    let mut library = File::open(input)?;
    let mut output = File::create(out.join(format!("{name}.lib")))?;
    if let Some(identity) = identity {
        identity.library(&mut library, output)?;
    } else {
        io::copy(&mut library, &mut output)?;
    }
    println!("cargo:rustc-link-lib=static={name}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build-sdk.rs");
    let _sdk_use = OpenOptions::new().read(true).share_mode(1).open("../../VERSION")?;
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let mut identity = env::var_os("CARGO_FEATURE_COMPILER")
        .map(|_| compiler_identity::Identity::new());
    capture("blink_value_format", &out, identity.as_mut())?;
    println!("cargo:rustc-link-search=native={}", out.display());
    if let Some(mut identity) = identity {
        capture("blink_compiler_bootstrap", &out, Some(&mut identity))?;
        compiler_identity::emit(identity)?;
    }
    Ok(())
}
