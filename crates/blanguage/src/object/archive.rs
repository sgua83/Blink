//! 编译阶段把执行框架和 ABI 编译为静态目标库；链接器不再调用 Cargo 或 rustc。
use super::{Archive, Result};
use std::{ffi::OsString, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

pub(crate) fn sdk_root() -> std::io::Result<PathBuf> {
    #[cfg(feature = "installed")]
    { Ok(PathBuf::from(std::env::var_os("ProgramFiles").ok_or_else(||
        std::io::Error::new(std::io::ErrorKind::NotFound, "缺少Windows ProgramFiles安装根"))?).join("Blink")) }
    #[cfg(not(feature = "installed"))]
    { Ok(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().into()) }
}
fn build_root(sdk: &Path) -> std::io::Result<PathBuf> {
    #[cfg(feature = "installed")]
    { let _ = sdk; Ok(std::env::current_dir()?.join("target")) }
    #[cfg(not(feature = "installed"))]
    { Ok(sdk.join("target")) }
}
fn finish<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}\n编译暂存清理失败：{cleanup}").into()),
    }
}
pub(crate) fn compile(
    source: &str,
    exports: Vec<String>,
    dependencies: &[&str],
    worker: &crate::compiler::Worker,
) -> Result<Archive> {
    let sdk = sdk_root()?;
    let sdk_target = build_root(&sdk)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let id = format!("{}-{created}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let unit = format!("blink_object_{}", id.replace('-', "_"));
    let workspaces = sdk_target.join("blc-object");
    std::fs::create_dir_all(&workspaces)?;
    let build = workspaces.join(id);
    std::fs::create_dir(&build)?;
    let result = (|| {
        std::fs::create_dir(build.join("src"))?;
        let mut manifest = format!(
            "[package]\nname={unit:?}\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname={unit:?}\ncrate-type=[\"staticlib\"]\n[profile.release]\nlto=false\ncodegen-units=1\npanic=\"abort\"\n",
        );
        manifest.push_str("[dependencies]\n");
        for name in dependencies {
            let package = sdk.join("crates").join(name);
            let path = package.to_str().ok_or("SDK依赖路径不能编码为UTF-8 TOML")?;
            manifest.push_str(&format!("{name}={{path={path:?},default-features=false"));
            #[cfg(feature = "installed")]
            if *name == "blanguage" { manifest.push_str(",features=[\"installed\"]"); }
            manifest.push_str("}\n");
        }
        std::fs::write(build.join("Cargo.toml"), manifest)?;
        std::fs::write(build.join("src/lib.rs"), source)?;
        let target = sdk_target.join("dll");
        let arguments: Vec<OsString> = [
            "rustc".into(), "--offline".into(), "--release".into(), "--jobs".into(), "1".into(),
            "--manifest-path".into(), build.join("Cargo.toml").into_os_string(),
            "--target-dir".into(), target.as_os_str().into(), "--color".into(), "never".into(),
            "--".into(), "--print".into(), "native-static-libs".into(),
        ].into();
        let output = worker.cargo_output(&arguments, &build, &sdk)?;
        let compilation = (|| {
            let stderr = String::from_utf8(output.stderr)?;
            if !output.status.success() {
                return Err(format!(
                    "执行库目标代码编译失败：\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    stderr
                )
                .into());
            }
            let libraries = stderr
                .lines()
                .find_map(|line| line.trim().strip_prefix("note: native-static-libs: "))
                .ok_or("Rust 未返回静态目标库的本机库依赖")?
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            Ok(Archive {
                bytes: super::ObjectBytes::read(&target.join(format!("release/{unit}.lib")))?,
                libraries,
                exports,
                resources: None,
            })
        })();
        let cleanup = (|| {
            let arguments: Vec<OsString> = [
                "clean".into(), "--offline".into(), "--release".into(), "--manifest-path".into(),
                build.join("Cargo.toml").into_os_string(), "--target-dir".into(), target.as_os_str().into(),
                "--package".into(), unit.clone().into(), "--color".into(), "never".into(),
            ].into();
            let output = worker.cargo_output(&arguments, &build, &sdk)?;
            if !output.status.success() {
                return Err(format!(
                    "执行库单元产物清理失败：\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            Ok(())
        })();
        finish(compilation, cleanup)
    })();
    finish(result, std::fs::remove_dir_all(build).map_err(Into::into))
}


// 构建输入来自原原生生产者；不是资源ABI或另一份运行时状态。
pub(crate) fn inputs(runtime: bool) -> std::io::Result<[u8; 32]> {
    use crate::{build::hash::Digest, file_bytes::FileBytes};
    use std::{collections::BTreeSet, fs, io::ErrorKind};
    fn optional(digest: &mut Digest, path: &Path) -> std::io::Result<bool> {
        digest.field(path.as_os_str().as_encoded_bytes());
        match FileBytes::read_path(path) {
            Ok(value) => { digest.field(b"present"); digest.field(&value); Ok(true) }
            Err(error) if error.kind() == ErrorKind::NotFound => { digest.field(b"absent"); Ok(false) }
            Err(error) => Err(error),
        }
    }
    fn configuration(digest: &mut Digest, directory: &Path) -> std::io::Result<()> {
        if !optional(digest, &directory.join("config"))? {
            optional(digest, &directory.join("config.toml"))?;
        }
        Ok(())
    }
    fn sources(path: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() { sources(&path, files)?; } else { files.push(path); }
        }
        Ok(())
    }
    let sdk = sdk_root()?;
    let mut digest = Digest::new();
    digest.update(b"BlinkNativeInputs1");
    digest.field(sdk.as_os_str().as_encoded_bytes());
    let variables = std::env::vars_os().filter(|(name, _)| {
        let name = name.to_string_lossy().to_ascii_uppercase();
        matches!(name.as_str(), "PATH" | "RUSTC" | "RUSTFLAGS" | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER" | "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN"
            | "CARGO_HOME" | "CARGO_INCREMENTAL" | "CARGO_ENCODED_RUSTFLAGS"
            | "LIB" | "LIBPATH" | "INCLUDE" | "SYSTEMROOT")
            || name.starts_with("CARGO_PROFILE_") || name.starts_with("CARGO_TARGET_")
            || (name.starts_with("CARGO_BUILD_") && name != "CARGO_BUILD_JOBS")
    }).collect::<std::collections::BTreeMap<_, _>>();
    for (name, value) in variables {
        digest.field(name.as_encoded_bytes());
        digest.field(value.as_encoded_bytes());
    }
    let mut configured = BTreeSet::new();
    for directory in sdk.ancestors() {
        let cargo = directory.join(".cargo");
        configuration(&mut digest, &cargo)?;
        configured.insert(cargo);
        optional(&mut digest, &directory.join("rust-toolchain"))?;
        optional(&mut digest, &directory.join("rust-toolchain.toml"))?;
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from);
    let cargo = std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".cargo")));
    if let Some(cargo) = cargo {
        let cargo = sdk.join(cargo);
        if !configured.contains(&cargo) { configuration(&mut digest, &cargo)?; }
    }
    let rustup = std::env::var_os("RUSTUP_HOME").map(std::path::PathBuf::from)
        .or_else(|| home.map(|home| home.join(".rustup")));
    if let Some(rustup) = rustup { optional(&mut digest, &sdk.join(rustup).join("settings.toml"))?; }
    if runtime {
        let mut files = vec![sdk.join("Cargo.toml")];
        optional(&mut digest, &sdk.join("crates/blanguage/build-sdk.rs"))?;
        for name in ["blanguage", "blcompute"] {
            let package = sdk.join("crates").join(name);
            files.push(package.join("Cargo.toml"));
            optional(&mut digest, &package.join("build.rs"))?;
            sources(&package.join("src"), &mut files)?;
        }
        files.sort();
        for path in files {
            digest.field(path.as_os_str().as_encoded_bytes());
            digest.field(&FileBytes::read_path(&path)?);
        }
    }
    Ok(digest.finish())
}
