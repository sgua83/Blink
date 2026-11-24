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
fn runtime_artifact(stdout: &str) -> Result<PathBuf> {
    let mut archive = None;
    for line in stdout.lines().filter(|line| line.starts_with('{')) {
        let message: serde_json::Value = serde_json::from_str(line)?;
        if message["reason"] != "compiler-artifact" || message["target"]["name"] != "blcompute" {
            continue;
        }
        let filenames = message["filenames"].as_array().ok_or("Cargo 未返回执行库的产物文件名")?;
        let libraries = filenames.iter().filter_map(serde_json::Value::as_str)
            .map(Path::new).filter(|path| path.extension().is_some_and(|ext| ext == "lib"));
        for library in libraries {
            if archive.replace(library.to_path_buf()).is_some() {
                return Err("Cargo 返回了多个执行库静态归档".into());
            }
        }
    }
    archive.ok_or_else(|| "Cargo 未返回执行库的静态归档".into())
}

fn development_runtime(
    sdk: &Path,
    cache: &Path,
    work: &Path,
    worker: &crate::compiler::Worker,
) -> Result<Vec<String>> {
    let cache_directory = cache.to_str().ok_or("运行库缓存路径不能编码为 UTF-8")?;
    // Cargo shares its intermediates, while this call owns the reported final archive until publication.
    let mut arguments: Vec<OsString> = [
        "rustc".into(), "--offline".into(), "--locked".into(), "--release".into(),
        "--manifest-path".into(), sdk.join("Cargo.toml").into_os_string(),
        "--target-dir".into(), work.as_os_str().into(), "--package".into(), "blcompute".into(),
        "--config".into(), format!("build.build-dir={}", serde_json::to_string(cache_directory)?).into(),
        "--lib".into(), "--no-default-features".into(), "--config".into(), "profile.release.lto=false".into(),
        "--color".into(), "never".into(), "--message-format=json-render-diagnostics".into(),
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        let messages = stdout.split_inclusive('\n').filter(|line| !line.starts_with('{')).collect::<String>();
        return Err(format!("执行库编译失败：\n{messages}{stderr}").into());
    }
    let dependencies = stderr.lines()
        .find_map(|line| line.trim().strip_prefix("note: native-static-libs: "))
        .ok_or("Rust 未返回静态目标库的本机库依赖")?;

    let archive = runtime_artifact(&String::from_utf8(output.stdout)?)?;
    let product = crate::file_bytes::FileBytes::read_path(&archive)?;
    let hash = crate::build::hash::hex(&crate::build::hash::digest(&product));
    let shared = cache.join(format!("blink-runtime-{hash}.lib"));
    if !shared.try_exists()? {
        std::fs::create_dir_all(cache)?;
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

pub(crate) fn runtime_libraries(worker: &crate::compiler::Worker) -> Result<Vec<String>> {
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

pub(crate) fn runtime_fingerprint(libraries: &[String]) -> [u8; 32] {
    let mut digest = crate::build::hash::Digest::new();
    digest.update(b"BlinkNativeLibraries1");
    for library in libraries {
        digest.field(library.as_bytes());
    }
    digest.finish()
}

pub(crate) fn compile(
    native: Option<Vec<u8>>,
    exports: Vec<String>,
    runtime: bool,
    worker: &crate::compiler::Worker,
    libraries: Option<Vec<String>>,
) -> Result<Archive> {
    let libraries = if runtime { libraries.map_or_else(|| runtime_libraries(worker), Ok)? } else { Vec::new() };
    let mut symbols = Vec::new();
    let mut members = Vec::new();
    if let Some(bytes) = native {
        super::coff::Reader::new(bytes.as_slice())?.for_each_definition(|symbol| {
            symbols.push((0, symbol.to_owned()));
            Ok(())
        })?;
        members.push(super::static_library::Member::new("entry.obj".into(), bytes));
    }
    let bytes = super::static_library::encode(
        members.iter().map(|member| member.view()), symbols.iter().map(|(index, name)| (*index, name.as_str())),
    )?;
    Ok(Archive {
        bytes: super::ObjectBytes::generated(bytes),
        libraries,
        exports,
        resources: None,
    })
}
