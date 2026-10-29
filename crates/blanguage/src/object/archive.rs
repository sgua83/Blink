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
        let dependencies = dependencies
            .iter()
            .map(|name| {
                format!(
                    "{name}={{path={:?}}}\n",
                    sdk.join("crates").join(name).to_string_lossy()
                )
            })
            .collect::<String>();
        let manifest = format!(
            "[package]\nname={unit:?}\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname={unit:?}\ncrate-type=[\"staticlib\"]\n[dependencies]\n{dependencies}[profile.release]\nlto=false\ncodegen-units=1\npanic=\"abort\"\n",
        );
        std::fs::write(build.join("Cargo.toml"), manifest)?;
        std::fs::write(build.join("src/lib.rs"), source)?;
        let target = sdk.join("target/dll");
        let arguments: Vec<OsString> = [
            "rustc".into(), "--offline".into(), "--release".into(), "--jobs".into(), "1".into(),
            "--manifest-path".into(), build.join("Cargo.toml").into_os_string(),
            "--target-dir".into(), target.as_os_str().into(), "--color".into(), "never".into(),
            "--".into(), "--print".into(), "native-static-libs".into(),
        ].into();
        let output = worker.cargo_output(&arguments, &build)?;
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
            let output = worker.cargo_output(&arguments, &build)?;
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
