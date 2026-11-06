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
pub(super) fn link(archives: &[Archive], objects: &[&crate::object::Object], path: &Path, import_library: bool) -> Result<()> {
    let binding = super::resources::resources(&archives.iter().collect::<Vec<_>>(), 0, 0, &[], &std::collections::BTreeMap::new(), None)?;
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
    for (index, object) in objects.iter().enumerate() {
        let path = path.parent().unwrap().join(format!(".blink-link-{id}-code-{index}.obj"));
        object.write(&path)?;
        temporary.push(path);
    }
    // 原COFF导入身份绑定到所属PE的IAT；模块与函数名称都保留实际字节。
    let mut imports = std::collections::BTreeMap::<&str, std::collections::BTreeSet<&str>>::new();
    for object in objects {
        for relocation in object.sections.iter().flat_map(|section| &section.relocations) {
            if let crate::object::Target::Import(import) = &relocation.target {
                imports.entry(&import.dll).or_default().insert(&import.name);
            }
        }
    }
    if !imports.is_empty() {
        use crate::object::{Kind, Object, Relocation, Section, Symbol, Target};
        let mut sections = [".idata$2", ".idata$3", ".idata$4", ".idata$5", ".idata$6", ".rdata"]
            .into_iter().enumerate().map(|(index, name)| Section {
                name: name.into(), bytes: Vec::new(),
                flags: (if index == 5 { 0x40000040 } else { 0xc0000040 }) | match index { 0 | 1 => 0x00300000, 2 | 3 | 5 => 0x00400000, _ => 0x00200000 },
                relocations: Vec::new(), associated: Vec::new(), layout: None,
            }).collect::<Vec<_>>();
        sections[1].bytes.resize(20, 0);
        let mut symbols = Vec::new();
        for (dll, names) in imports {
            if dll == crate::application::RUNTIME_LIBRARY {
                for name in names {
                    let value = u32::try_from(sections[5].bytes.len())?;
                    sections[5].bytes.extend([0; 8]);
                    symbols.push(Symbol {
                        name: super::resources::import_symbol(&dll, &name),
                        section: 5, value, contract: Vec::new(), function: false,
                    });
                    sections[5].relocations.push(Relocation {
                        offset: value as usize, kind: 1, target: Target::Symbol(name.into()),
                    });
                }
                continue;
            }
            let descriptor = sections[0].bytes.len();
            sections[0].bytes.resize(descriptor + 20, 0);
            let dll_name = sections[4].bytes.len();
            sections[4].bytes.extend(dll.as_bytes());
            sections[4].bytes.push(0);
            for (field, section, addend) in [
                (0, 2, sections[2].bytes.len()),
                (12, 4, dll_name),
                (16, 3, sections[3].bytes.len()),
            ] {
                sections[0].bytes[descriptor + field..descriptor + field + 4]
                    .copy_from_slice(&u32::try_from(addend)?.to_le_bytes());
                sections[0].relocations.push(Relocation {
                    offset: descriptor + field, kind: 3, target: Target::Section(section, 0),
                });
            }
            for name in names {
                if sections[4].bytes.len() % 2 != 0 { sections[4].bytes.push(0); }
                let hint = sections[4].bytes.len();
                sections[4].bytes.extend([0, 0]);
                sections[4].bytes.extend(name.as_bytes());
                sections[4].bytes.push(0);
                symbols.push(Symbol {
                    name: super::resources::import_symbol(&dll, &name),
                    section: 3, value: u32::try_from(sections[3].bytes.len())?,
                    contract: Vec::new(), function: false,
                });
                for section in [2, 3] {
                    let at = sections[section].bytes.len();
                    sections[section].bytes.extend(u64::try_from(hint)?.to_le_bytes());
                    sections[section].relocations.push(Relocation {
                        offset: at, kind: 3, target: Target::Section(4, 0),
                    });
                }
            }
            sections[2].bytes.extend([0; 8]);
            sections[3].bytes.extend([0; 8]);
        }
        let imports = Object {
            name: "@blink.imports@".into(),
            kind: Kind::Module { interface: String::new(), requirements: std::collections::BTreeMap::new(), stack_bytes: 0 },
            sections, symbols, imports: std::collections::BTreeMap::new(),
            archive: None, stack: Vec::new(),
        };
        let path = path.parent().unwrap().join(format!(".blink-link-{id}-imports.obj"));
        imports.write(&path)?;
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
                "/NOLOGO",
                "/MACHINE:X64",
                "/OPT:REF",
                "/OPT:ICF",
                "/NOEXP",
            ])
            .arg(format!("/OUT:{}", path.display()))
            .args(&temporary);
        if let Some(crate::object::Object { kind: crate::object::Kind::Application { stack_bytes }, .. }) = objects.first().copied() {
            command.arg("/SUBSYSTEM:CONSOLE").arg(format!("/STACK:{}", super::pe::stack_reserve(*stack_bytes)));
        } else {
            command.arg("/DLL");
        }
        if import_library {
            let mut import_path = path.to_path_buf();
            import_path.set_extension("lib");
            command.arg(format!("/IMPLIB:{}", import_path.display()));
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
