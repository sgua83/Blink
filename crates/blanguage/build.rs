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
fn bootstrap_modules() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::PathBuf};
    let seed = PathBuf::from(env::var_os("ProgramFiles").ok_or("缺少ProgramFiles")?).join("Blink");
    // Keep the installed input until all bootstrap compilation and archiving ends.
    let _seed_use = OpenOptions::new().read(true).share_mode(1).open(seed.join("VERSION"))?;
    for name in ["VERSION", "bin/bc.exe", "bin/blink.exe"] {
        println!("cargo:rerun-if-changed={}", seed.join(name).display());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let modules = ["lexer", "parser", "archive", "coff", "metadata", "bindings", "coff_read"];
    let objects: Vec<_> = modules.iter().map(|name| out.join(format!("blink_compiler_{name}.obj"))).collect();
    for (name, object) in modules.iter().zip(&objects) {
        let compiled = Command::new(seed.join("bin/bc.exe"))
            .arg("-c").arg(format!("src/bootstrap/{name}.b"))
            .args(["-I", "src", "--module", "-o"]).arg(object).status()?;
        if !compiled.success() {
            return Err(format!("稳定Blink种子编译{name}模块失败").into());
        }
    }
    let archive = out.join("blink_compiler_bootstrap.lib");
    let archived = Command::new(seed.join("bin/blink.exe"))
        .arg("--archive").args(&objects).arg("-o").arg(&archive).status()?;
    if !archived.success() {
        return Err("稳定Blink种子归档B引导模块失败".into());
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=blink_compiler_bootstrap");
    let mut compiled_objects = Vec::new();
    for object in objects { hash::field(&mut compiled_objects, &fs::read(object)?); }
    Ok(compiled_objects)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_COMPILER").is_none() {
        println!("cargo:rerun-if-changed=build.rs");
        return Ok(());
    }
    let bootstrap_objects = bootstrap_modules()?;
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files)?;
    files.extend([
        "build.rs".into(),
        "Cargo.toml".into(),
        "../../Cargo.toml".into(),
    ]);
    files.sort();
    let mut identity = Vec::new();
    hash::field(&mut identity, &bootstrap_objects);
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
        "CARGO_FEATURE_INSTALLED",
        "CARGO_FEATURE_COMPILER",
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
