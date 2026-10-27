//! 编译阶段把执行框架和 ABI 编译为静态目标库；链接器不再调用 Cargo 或 rustc。
use super::{Archive, Result};
use std::{path::Path, process::Command};
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
) -> Result<Archive> {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let unit = format!("blink_object_{}", id.replace('-', "_"));
    let build = sdk.join("target/blc-object").join(id);
    std::fs::create_dir_all(build.join("src"))?;
    let result = (|| {
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
        let output = Command::new("cargo")
            .args(["rustc", "--offline", "--release", "--manifest-path"])
            .arg(build.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .args(["--", "--print", "native-static-libs"])
            .env("CARGO_TERM_COLOR", "never")
            .output()?;
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
            let output = Command::new("cargo")
                .args(["clean", "--offline", "--release", "--manifest-path"])
                .arg(build.join("Cargo.toml"))
                .arg("--target-dir")
                .arg(&target)
                .args(["--package", &unit])
                .env("CARGO_TERM_COLOR", "never")
                .output()?;
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
