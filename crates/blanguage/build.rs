#[path = "src/build/hash.rs"]
mod hash;
use std::{env, fs, path::Path, process::Command};

fn sources(path: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Software version is display metadata, separate from compilation identity.
    println!("cargo:rerun-if-changed=../../VERSION");
    let version = fs::read_to_string("../../VERSION")?;
    println!("cargo:rustc-env=BLINK_VERSION={}", version.trim());
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files)?;
    files.extend([
        "build.rs".into(),
        "Cargo.toml".into(),
        "../../Cargo.toml".into(),
    ]);
    files.sort();
    let mut identity = Vec::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        hash::field(
            &mut identity,
            path.to_str().ok_or("编译器源码路径不是UTF-8")?.as_bytes(),
        );
        hash::field(&mut identity, &fs::read(path)?);
    }
    println!("cargo:rerun-if-changed=src");
    // Cargo owns these inputs; the content identity uses their effective values.
    for name in [
        "TARGET",
        "HOST",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
    ] {
        hash::field(&mut identity, name.as_bytes());
        match env::var_os(name) {
            Some(value) => {
                hash::field(&mut identity, b"present");
                hash::field(
                    &mut identity,
                    value.to_str().ok_or("构建配置不是UTF-8")?.as_bytes(),
                );
            }
            None => hash::field(&mut identity, b"absent"),
        }
    }
    let rustc = Command::new(env::var_os("RUSTC").ok_or("缺少RUSTC")?)
        .arg("-vV")
        .output()?;
    if !rustc.status.success() {
        return Err("无法取得编译器生产工具链身份".into());
    }
    hash::field(&mut identity, &rustc.stdout);
    println!(
        "cargo:rustc-env=BLINK_COMPILER_ID={}",
        hash::hex(&hash::digest(&identity))
    );
    Ok(())
}
