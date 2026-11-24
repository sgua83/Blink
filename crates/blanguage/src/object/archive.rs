//! 编译阶段把执行框架和 ABI 编译为静态目标库；链接器不再调用 Cargo 或 rustc。
use super::{Archive, Result};
use std::{ffi::OsString, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

pub(crate) struct InstalledSdk {
    pub(crate) root: PathBuf,
    _version: std::fs::File,
}

impl InstalledSdk {
    pub(crate) fn open() -> std::io::Result<Self> {
        let program_files = std::env::var_os("ProgramFiles").ok_or_else(||
            std::io::Error::new(std::io::ErrorKind::NotFound, "缺少Windows ProgramFiles安装根"))?;
        let root = crate::source_path::absolute(&PathBuf::from(program_files).join("Blink"))?;
        let version = crate::file_bytes::open_shared_read(&root.join("VERSION"))?;
        Ok(Self { root, _version: version })
    }
}

pub(crate) const SDK_RUNTIME: &str = "crates/blcompute/native/";

fn sdk_root(_worker: &crate::compiler::Worker) -> std::io::Result<PathBuf> {
    #[cfg(feature = "installed")]
    { _worker.compiler().with_sdk_input(Path::to_path_buf) }
    #[cfg(not(feature = "installed"))]
    { Ok(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().into()) }
}
fn finish<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}\n编译暂存清理失败：{cleanup}").into()),
    }
}
fn development_runtime(
    sdk: &Path,
    target: &Path,
    work: &Path,
    worker: &crate::compiler::Worker,
) -> Result<Vec<String>> {
    let mut arguments: Vec<OsString> = [
        "rustc".into(), "--offline".into(), "--locked".into(), "--release".into(),
        "--manifest-path".into(), sdk.join("Cargo.toml").into_os_string(),
        "--target-dir".into(), target.as_os_str().into(), "--package".into(), "blcompute".into(),
        "--no-default-features".into(), "--config".into(), "profile.release.lto=false".into(),
        "--color".into(), "never".into(),
    ].into();
    if let Some(directory) = option_env!("BLINK_BUILT_SHARED_OBJECTS") {
        let directory = directory.replace('\\', "\\\\");
        arguments.extend([
            "--config".into(),
            format!(r#"env.BLINK_DEVELOPMENT_SHARED_OBJECTS.value="{directory}""#).into(),
            "--config".into(),
            "env.BLINK_DEVELOPMENT_SHARED_OBJECTS.force=true".into(),
        ]);
    }
    arguments.extend(["--", "--print", "native-static-libs"].map(OsString::from));
    let output = worker.cargo_output(arguments, work, sdk)?;
    let stderr = String::from_utf8(output.stderr)?;
    if !output.status.success() {
        return Err(format!("执行库编译失败：\n{}{}", String::from_utf8_lossy(&output.stdout), stderr).into());
    }
    let dependencies = stderr.lines()
        .find_map(|line| line.trim().strip_prefix("note: native-static-libs: "))
        .ok_or("Rust 未返回静态目标库的本机库依赖")?;

    let product = crate::file_bytes::FileBytes::read_path(&target.join("release/blcompute.lib"))?;
    let hash = crate::build::hash::hex(&crate::build::hash::digest(&product));
    let shared = target.join(format!("blink-runtime-{hash}.lib"));
    if !shared.try_exists()? {
        if let Err(error) = crate::file_bytes::publish(&shared, &work.join("runtime.lib"), [&product])
            && !shared.try_exists()?
        {
            return Err(error.into());
        }
    }
    let mut libraries = vec![shared.into_os_string().into_string()
        .map_err(|_| "运行库路径不能编码为 UTF-8")?];
    libraries.extend(dependencies.split_whitespace().map(str::to_owned));
    Ok(libraries)
}

fn runtime_libraries(worker: &crate::compiler::Worker) -> Result<Vec<String>> {
    let sdk = sdk_root(worker)?;
    if cfg!(feature = "installed") {
        return Ok(std::fs::read_to_string(sdk.join(SDK_RUNTIME).join("native-static-libs.txt"))?
            .split_whitespace().map(str::to_owned).collect());
    }
    let sdk_target = sdk.join("target");
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let id = format!("{}-{created}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let workspaces = sdk_target.join("blc-object");
    std::fs::create_dir_all(&workspaces)?;
    let build = workspaces.join(id);
    std::fs::create_dir(&build)?;
    let result = development_runtime(&sdk, &sdk_target.join("dll"), &build, worker);
    finish(result, std::fs::remove_dir_all(build).map_err(Into::into))
}

pub(crate) fn compile(
    native: Option<Vec<u8>>,
    exports: Vec<String>,
    runtime: bool,
    worker: &crate::compiler::Worker,
) -> Result<Archive> {
    let libraries = if runtime { runtime_libraries(worker)? } else { Vec::new() };
    let mut symbols = Vec::new();
    let mut members = Vec::new();
    if let Some(bytes) = native {
        super::coff::Reader::new(bytes.as_slice())?.for_each_definition(|symbol| {
            symbols.push((0, symbol.to_owned()));
            Ok(())
        })?;
        members.push(super::static_library::Member::new("entry.obj".into(), bytes));
    }
    let bytes = super::static_library::encode(&members, &symbols)?;
    Ok(Archive {
        bytes: super::ObjectBytes::generated(bytes),
        libraries,
        exports,
        resources: None,
    })
}

// 构建输入来自原原生生产者；不是资源ABI或另一份运行时状态。
pub(crate) fn inputs(runtime: bool, worker: &crate::compiler::Worker) -> std::io::Result<[u8; 32]> {
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
    fn sources(path: &Path, compiler_sources: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path == compiler_sources { continue; }
            if path.is_dir() {
                sources(&path, compiler_sources, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut digest = Digest::new();
    digest.update(b"BlinkNativeInputs2");
    if !runtime { return Ok(digest.finish()); }
    let sdk = sdk_root(worker)?;
    if cfg!(feature = "installed") {
        let link_inputs = sdk.join(SDK_RUNTIME).join("native-static-libs.txt");
        digest.field(b"installed-native-library");
        digest.field(&FileBytes::read_path(&link_inputs)?);
        return Ok(digest.finish());
    }
    digest.field(sdk.as_os_str().as_encoded_bytes());
    let variables = std::env::vars_os().filter(|(name, _)| {
        let name = name.to_string_lossy().to_ascii_uppercase();
        // compile() fixes jobs, profile and target-dir with explicit Cargo arguments.
        !matches!(name.as_str(), "CARGO_BUILD_JOBS" | "CARGO_BUILD_PROFILE"
            | "CARGO_TARGET_DIR" | "CARGO_BUILD_TARGET_DIR")
            && (matches!(name.as_str(), "PATH" | "RUSTC" | "RUSTFLAGS" | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER" | "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN"
                | "CARGO_HOME" | "CARGO_INCREMENTAL" | "CARGO_ENCODED_RUSTFLAGS"
                | "LIB" | "LIBPATH" | "INCLUDE" | "SYSTEMROOT")
                || matches!(name.as_str(), "BLINK_BOOTSTRAP_SDK" | "PROGRAMFILES")
                || name.starts_with("CARGO_PROFILE_") || name.starts_with("CARGO_TARGET_")
                || name.starts_with("CARGO_BUILD_"))
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
    optional(&mut digest, &sdk.join("Cargo.lock"))?;
    let mut files = vec![sdk.join("Cargo.toml")];
    let compiler_sources = sdk.join("crates/blanguage/src/bootstrap");
    optional(&mut digest, &sdk.join("crates/blanguage/build-sdk.rs"))?;
    if let Some(source) = option_env!("BLINK_RUNTIME_COFF_SOURCE") {
        let source = sdk.join(source);
        for extension in ["b", "h"] {
            optional(&mut digest, &source.with_extension(extension))?;
        }
    } else {
        optional(&mut digest, &sdk.join("crates/blanguage/native/blink_value_format.lib"))?;
    }
    for name in ["blanguage", "blcompute"] {
        let package = sdk.join("crates").join(name);
        files.push(package.join("Cargo.toml"));
        optional(&mut digest, &package.join("build.rs"))?;
        sources(&package.join("src"), &compiler_sources, &mut files)?;
    }
    files.sort();
    for path in files {
        digest.field(path.as_os_str().as_encoded_bytes());
        digest.field(&FileBytes::read_path(&path)?);
    }
    Ok(digest.finish())
}
