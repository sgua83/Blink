//! 编译阶段把执行框架和 ABI 编译为静态目标库；链接器不再调用 Cargo 或 rustc。
use super::{Archive, Result};
use std::{path::Path, process::Command};
pub(crate) fn compile(source: &str, prefix: &str, exports: Vec<String>) -> Result<Archive> {
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
    let build = sdk.join("target/blc-object").join(id);
    std::fs::create_dir_all(build.join("src"))?;
    let result = (|| {
        let manifest = format!(
            "[package]\nname=\"blink_object\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname={prefix:?}\ncrate-type=[\"staticlib\"]\n[dependencies]\nblanguage={{path={:?}}}\nblcompute={{path={:?}}}\n[profile.release]\nlto=\"thin\"\ncodegen-units=1\npanic=\"abort\"\n",
            sdk.join("crates/blanguage").to_string_lossy(),
            sdk.join("crates/blcompute").to_string_lossy()
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
            bytes: std::fs::read(target.join(format!("release/{prefix}.lib")))?,
            libraries,
            exports,
        })
    })();
    std::fs::remove_dir_all(build)?;
    result
}
