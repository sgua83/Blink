//! 编译阶段把执行框架和 ABI 编译为静态目标库；链接器不再调用 Cargo 或 rustc。
use super::{Archive, Result};
use std::{ffi::OsString, path::Path, sync::atomic::{AtomicU64, Ordering}};
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
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let unit = format!("blink_object_{}", id.replace('-', "_"));
    let workspaces = sdk.join("target/blc-object");
    std::fs::create_dir_all(&workspaces)?;
    let build = workspaces.join(id);
    std::fs::create_dir(&build)?;
    let result = (|| {
        std::fs::create_dir(build.join("src"))?;
        let sdk_target = sdk.join("target");
        if !dependencies.is_empty() {
            let mut arguments: Vec<OsString> = [
                "build".into(), "--offline".into(), "--release".into(), "--jobs".into(), "1".into(),
                "--manifest-path".into(), sdk.join("Cargo.toml").into_os_string(),
                "--target-dir".into(), sdk_target.as_os_str().into(),
                "--lib".into(), "--color".into(), "never".into(),
            ].into();
            for name in dependencies {
                arguments.extend([OsString::from("--package"), (*name).into()]);
            }
            let output = worker.cargo_output(&arguments, &build, sdk)?;
            if !output.status.success() {
                return Err(format!(
                    "SDK目标库编译失败：\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ).into());
            }
        }
        let manifest = format!(
            "[package]\nname={unit:?}\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname={unit:?}\ncrate-type=[\"staticlib\"]\n[profile.release]\nlto=false\ncodegen-units=1\npanic=\"abort\"\n",
        );
        std::fs::write(build.join("Cargo.toml"), manifest)?;
        std::fs::write(build.join("src/lib.rs"), source)?;
        let target = sdk.join("target/dll");
        let mut arguments: Vec<OsString> = [
            "rustc".into(), "--offline".into(), "--release".into(), "--jobs".into(), "1".into(),
            "--manifest-path".into(), build.join("Cargo.toml").into_os_string(),
            "--target-dir".into(), target.as_os_str().into(), "--color".into(), "never".into(),
            "--".into(), "--print".into(), "native-static-libs".into(),
        ].into();
        for name in dependencies {
            arguments.extend([
                OsString::from("--extern"),
                format!("{name}={}", sdk_target.join(format!("release/lib{name}.rlib")).display()).into(),
            ]);
        }
        if !dependencies.is_empty() {
            arguments.extend([
                OsString::from("-L"),
                format!("dependency={}", sdk_target.join("release/deps").display()).into(),
            ]);
        }
        let output = worker.cargo_output(&arguments, &build, sdk)?;
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
                bytes: std::fs::read(target.join(format!("release/{unit}.lib")))?,
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
            let output = worker.cargo_output(&arguments, &build, sdk)?;
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
    use crate::build::hash;
    use std::{collections::BTreeSet, fs, io::ErrorKind};
    fn optional(bytes: &mut Vec<u8>, path: &Path) -> std::io::Result<bool> {
        hash::field(bytes, path.as_os_str().as_encoded_bytes());
        match fs::read(path) {
            Ok(value) => { hash::field(bytes, b"present"); hash::field(bytes, &value); Ok(true) }
            Err(error) if error.kind() == ErrorKind::NotFound => { hash::field(bytes, b"absent"); Ok(false) }
            Err(error) => Err(error),
        }
    }
    fn configuration(bytes: &mut Vec<u8>, directory: &Path) -> std::io::Result<()> {
        if !optional(bytes, &directory.join("config"))? {
            optional(bytes, &directory.join("config.toml"))?;
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
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let mut bytes = Vec::from(b"BlinkNativeInputs1".as_slice());
    hash::field(&mut bytes, sdk.as_os_str().as_encoded_bytes());
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
        hash::field(&mut bytes, name.as_encoded_bytes());
        hash::field(&mut bytes, value.as_encoded_bytes());
    }
    let mut configured = BTreeSet::new();
    for directory in sdk.ancestors() {
        let cargo = directory.join(".cargo");
        configuration(&mut bytes, &cargo)?;
        configured.insert(cargo);
        optional(&mut bytes, &directory.join("rust-toolchain"))?;
        optional(&mut bytes, &directory.join("rust-toolchain.toml"))?;
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from);
    let cargo = std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".cargo")));
    if let Some(cargo) = cargo {
        let cargo = sdk.join(cargo);
        if !configured.contains(&cargo) { configuration(&mut bytes, &cargo)?; }
    }
    let rustup = std::env::var_os("RUSTUP_HOME").map(std::path::PathBuf::from)
        .or_else(|| home.map(|home| home.join(".rustup")));
    if let Some(rustup) = rustup { optional(&mut bytes, &sdk.join(rustup).join("settings.toml"))?; }
    if runtime {
        let mut files = vec![sdk.join("Cargo.toml")];
        for name in ["blanguage", "blcompute"] {
            let package = sdk.join("crates").join(name);
            files.push(package.join("Cargo.toml"));
            optional(&mut bytes, &package.join("build.rs"))?;
            sources(&package.join("src"), &mut files)?;
        }
        files.sort();
        for path in files {
            hash::field(&mut bytes, path.as_os_str().as_encoded_bytes());
            hash::field(&mut bytes, &fs::read(path)?);
        }
    }
    Ok(hash::digest(&bytes))
}
