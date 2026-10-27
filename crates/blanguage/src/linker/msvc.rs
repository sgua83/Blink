//! Rust 执行框架的静态目标库使用既有 Windows 本机链接工具；不执行源码编译。
use super::Result;
use crate::object::Archive;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(windows)]
fn text(bytes: &[u8]) -> Result<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MultiByteToWideChar(
            page: u32,
            flags: u32,
            input: *const u8,
            len: i32,
            output: *mut u16,
            capacity: i32,
        ) -> i32;
    }
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let len = i32::try_from(bytes.len())?;
    let size = unsafe { MultiByteToWideChar(0, 8, bytes.as_ptr(), len, std::ptr::null_mut(), 0) };
    if size == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut wide = vec![0; size as usize];
    if unsafe { MultiByteToWideChar(0, 8, bytes.as_ptr(), len, wide.as_mut_ptr(), size) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(String::from_utf16(&wide)?)
}
#[cfg(not(windows))]
fn text(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)?.into())
}
fn output(command: &mut Command) -> Result<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "本机链接工具失败：{}{}",
            text(&output.stdout)?,
            text(&output.stderr)?
        )
        .into());
    }
    Ok(text(&output.stdout)?.trim().to_owned())
}
fn tools() -> Result<(PathBuf, Vec<PathBuf>)> {
    let installer = PathBuf::from(
        std::env::var_os("ProgramFiles(x86)").ok_or("缺少 Windows ProgramFiles(x86)")?,
    )
    .join("Microsoft Visual Studio/Installer/vswhere.exe");
    let installation = output(Command::new(installer).args([
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ]))?;
    if installation.is_empty() {
        return Err("未安装 Visual Studio C++ 本机工具".into());
    }
    let vs = Path::new(&installation);
    let version = std::fs::read_to_string(
        vs.join("VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt"),
    )?;
    let vc = vs.join("VC/Tools/MSVC").join(version.trim());
    let registry = output(Command::new("reg.exe").args([
        "query",
        r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
        "/v",
        "KitsRoot10",
        "/reg:64",
    ]))?;
    let sdk = registry
        .lines()
        .find_map(|line| line.split_once("REG_SZ").map(|(_, v)| v.trim()))
        .ok_or("未安装 Windows SDK KitsRoot10")?;
    let lib = Path::new(sdk).join("Lib");
    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&lib)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Ok(parts) = name
            .split('.')
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()
        {
            versions.push((parts, entry.path()));
        }
    }
    let sdk = versions
        .into_iter()
        .max_by_key(|(v, _)| v.clone())
        .ok_or("Windows SDK 没有已安装的本机库")?
        .1;
    Ok((
        vc.join("bin/Hostx64/x64/link.exe"),
        vec![vc.join("lib/x64"), sdk.join("um/x64"), sdk.join("ucrt/x64")],
    ))
}
pub(super) fn library(archives: &[Archive], path: &Path, import_library: bool) -> Result<()> {
    let binding = super::resources::resources(archives)?;
    let (linker, libraries) = tools()?;
    let id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let mut temporary = Vec::new();
    for (index, archive) in archives.iter().enumerate() {
        let path = path
            .parent()
            .unwrap()
            .join(format!(".blink-link-{id}-{index}.lib"));
        std::fs::write(&path, &archive.bytes)?;
        temporary.push(path);
    }
    if let Some(binding) = binding {
        let path = path
            .parent()
            .unwrap()
            .join(format!(".blink-link-{id}-resources.obj"));
        binding.write(&path)?;
        temporary.push(path);
    }
    let result = (|| {
        let mut command = Command::new(linker);
        command
            .args([
                "/DLL",
                "/NOLOGO",
                "/MACHINE:X64",
                "/OPT:REF",
                "/OPT:ICF",
                "/NOEXP",
            ])
            .arg(format!("/OUT:{}", path.display()))
            .args(&temporary);
        if import_library {
            command.arg(format!("/IMPLIB:{}", path.with_extension("lib").display()));
        } else {
            command.arg("/NOIMPLIB");
        }
        for lib in libraries {
            command.arg(format!("/LIBPATH:{}", lib.display()));
        }
        let libraries = archives
            .iter()
            .flat_map(|archive| &archive.libraries)
            .collect::<std::collections::BTreeSet<_>>();
        let exports = archives
            .iter()
            .flat_map(|archive| &archive.exports)
            .collect::<std::collections::BTreeSet<_>>();
        for name in libraries {
            command.arg(name);
        }
        for name in exports {
            command.arg(format!("/EXPORT:{name}"));
        }
        output(&mut command)?;
        Ok(())
    })();
    for path in temporary {
        std::fs::remove_file(path)?;
    }
    result
}
