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
fn bootstrap_lexer() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::PathBuf};
    let seed = PathBuf::from(env::var_os("ProgramFiles").ok_or("缺少ProgramFiles")?).join("Blink");
    // Keep the installed input in use until both native tools have finished.
    let _seed_use = OpenOptions::new().read(true).share_mode(1).open(seed.join("VERSION"))?;
    for name in ["VERSION", "bin/bc.exe", "bin/blink.exe"] {
        println!("cargo:rerun-if-changed={}", seed.join(name).display());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let object = out.join("blink_compiler_lexer.obj");
    let archive = out.join("blink_compiler_lexer.lib");
    let compiled = Command::new(seed.join("bin/bc.exe"))
        .args(["-c", "src/bootstrap/lexer.b", "-I", "src", "--module", "-o"])
        .arg(&object).status()?;
    if !compiled.success() {
        return Err("稳定Blink种子编译词法模块失败".into());
    }
    let archived = Command::new(seed.join("bin/blink.exe"))
        .arg("--archive").arg(&object).arg("-o").arg(&archive).status()?;
    if !archived.success() {
        return Err("稳定Blink种子归档词法模块失败".into());
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=blink_compiler_lexer");
    Ok(fs::read(object)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexer_object = bootstrap_lexer()?;
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files)?;
    files.extend([
        "build.rs".into(),
        "Cargo.toml".into(),
        "../../Cargo.toml".into(),
    ]);
    files.sort();
    let mut identity = Vec::new();
    hash::field(&mut identity, &lexer_object);
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
