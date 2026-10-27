//! Frame 跨 Rust 静态库的 ABI 绑定来自实际 SDK 输入及本次 Rust 工具链。
use super::Result;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(windows)]
fn digest(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::ffi::c_void;
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptOpenAlgorithmProvider(
            handle: *mut *mut c_void,
            algorithm: *const u16,
            implementation: *const u16,
            flags: u32,
        ) -> i32;
        fn BCryptHash(
            handle: *mut c_void,
            secret: *mut u8,
            secret_bytes: u32,
            input: *mut u8,
            input_bytes: u32,
            output: *mut u8,
            output_bytes: u32,
        ) -> i32;
        fn BCryptCloseAlgorithmProvider(handle: *mut c_void, flags: u32) -> i32;
    }
    let size = u32::try_from(bytes.len())?;
    let mut handle = std::ptr::null_mut();
    let algorithm = "SHA256\0".encode_utf16().collect::<Vec<_>>();
    let opened = unsafe {
        BCryptOpenAlgorithmProvider(&mut handle, algorithm.as_ptr(), std::ptr::null(), 0)
    };
    if opened < 0 {
        return Err(format!("无法打开模块 ABI 的 SHA256 提供者：{opened:#x}").into());
    }
    let mut hash = vec![0; 32];
    let status = unsafe {
        BCryptHash(
            handle,
            std::ptr::null_mut(),
            0,
            bytes.as_ptr().cast_mut(),
            size,
            hash.as_mut_ptr(),
            32,
        )
    };
    let closed = unsafe { BCryptCloseAlgorithmProvider(handle, 0) };
    if status < 0 || closed < 0 {
        return Err(format!("模块 ABI 哈希失败：hash={status:#x}, close={closed:#x}").into());
    }
    Ok(hash)
}
#[cfg(not(windows))]
fn digest(_: &[u8]) -> Result<Vec<u8>> {
    Err("当前原生模块执行框架 ABI 面向 Windows SDK".into())
}
pub(crate) fn identity(sdk: &Path) -> Result<Vec<u8>> {
    use crate::module::codec::{Encode, Writer};
    fn files(path: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                files(&path, output)?;
            } else if path.extension().is_some_and(|extension| {
                extension == "rs" || extension == "in" || extension == "toml"
            }) {
                output.push(path);
            }
        }
        Ok(())
    }
    let mut inputs = vec![sdk.join("Cargo.toml"), sdk.join("Cargo.lock")];
    files(&sdk.join("crates/blanguage/src"), &mut inputs)?;
    files(&sdk.join("crates/blcompute/src"), &mut inputs)?;
    inputs.extend([
        sdk.join("crates/blanguage/Cargo.toml"),
        sdk.join("crates/blcompute/Cargo.toml"),
    ]);
    inputs.sort();
    let mut bytes = Writer(Vec::new());
    for path in inputs {
        path.strip_prefix(sdk)?
            .to_string_lossy()
            .replace('\\', "/")
            .encode(&mut bytes);
        std::fs::read(path)?.encode(&mut bytes);
    }
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = Command::new(rustc)
        .args(["--version", "--verbose"])
        .output()?;
    if !version.status.success() {
        return Err(format!(
            "读取实际 Rust ABI 工具链失败：{}",
            String::from_utf8_lossy(&version.stderr)
        )
        .into());
    }
    version.stdout.encode(&mut bytes);
    for variable in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        std::env::var_os(variable)
            .map(|value| value.to_string_lossy().into_owned())
            .encode(&mut bytes);
    }
    digest(&bytes.0)
}
