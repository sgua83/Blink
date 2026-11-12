//! Rust 执行框架的静态目标库使用既有 Windows 本机链接工具；不执行源码编译。
use super::Result;
use crate::object::Archive;
use std::{
    ffi::OsString,
    fs::File,
    io::{self, Write},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf, Prefix},
    process::{Command, Output},
};
fn output(command: &mut Command) -> io::Result<Output> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    command.creation_flags(DETACHED_PROCESS).output()
}
#[repr(C)]
struct HostWide {
    data: *const u16,
    first: u32,
    units: u32,
    status: i32,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum HostText {
    LinkOutput = 0,
    InstallationPath = 1,
    ToolVersion = 2,
}

impl HostWide {
    fn consume<T>(self, copy: impl FnOnce(&[u16]) -> T) -> io::Result<T> {
        // A successful decode owns this initialized span until the copy ends.
        let units = unsafe {
            std::slice::from_raw_parts(self.data.add(self.first as usize), self.units as usize)
        };
        let result = copy(units);
        if unsafe { release_host_wide(self.data) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }
}

#[repr(C)]
struct SdkVersionResult {
    units: u32,
    status: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.host_sdk::find_sdk_version"]
    fn find_sdk_version(pattern: *const u16, selected: *mut u16) -> SdkVersionResult;
    #[link_name = "bootstrap.host_sdk::read_sdk_root"]
    fn read_sdk_root() -> HostWide;
    #[link_name = "bootstrap.host_sdk::decode_host_text"]
    fn decode_host_text(input: *const u8, bytes: i32, kind: HostText) -> HostWide;
    #[link_name = "bootstrap.host_sdk::release_host_wide"]
    fn release_host_wide(data: *const u16) -> i32;
}

fn sdk_root() -> Result<PathBuf> {
    let root = unsafe { read_sdk_root() };
    if root.status != 0 {
        return Err(format!("无法读取 Windows SDK KitsRoot10：{}", std::io::Error::from_raw_os_error(root.status)).into());
    }
    Ok(root.consume(|units| PathBuf::from(OsString::from_wide(units)))?)
}

fn host_text<T>(bytes: &[u8], kind: HostText, copy: impl FnOnce(&[u16]) -> T) -> Result<T> {
    if bytes.is_empty() {
        return Ok(copy(&[]));
    }
    let decoded = unsafe { decode_host_text(bytes.as_ptr(), i32::try_from(bytes.len())?, kind) };
    const ERROR_NO_UNICODE_TRANSLATION: i32 = 1113;
    if matches!(kind, HostText::ToolVersion) && decoded.status == ERROR_NO_UNICODE_TRANSLATION {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "stream did not contain valid UTF-8").into());
    }
    if decoded.status != 0 {
        return Err(io::Error::from_raw_os_error(decoded.status).into());
    }
    Ok(decoded.consume(copy)?)
}

fn sdk_search_pattern(directory: &Path) -> io::Result<Vec<u16>> {
    let pattern = directory.join("*");
    let mut units: Vec<_> = pattern.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "file name contained an unexpected NUL byte"));
    }
    const VERBATIM: &str = r"\\?\";
    let verbatim = units.starts_with(&[92, 92, 63, 92]) || units.starts_with(&[92, 63, 63, 92]);
    if !verbatim && (units.len() >= 247 || !pattern.is_absolute()) {
        let absolute = std::path::absolute(&pattern)?;
        let (prefix, skip) = match absolute.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) => (VERBATIM, 0),
                Prefix::UNC(..) => (r"\\?\UNC\", 2),
                Prefix::DeviceNS(_) => (VERBATIM, 4),
                _ => ("", 0),
            },
            _ => ("", 0),
        };
        units.clear();
        units.extend(prefix.encode_utf16());
        units.extend(absolute.as_os_str().encode_wide().skip(skip));
    }
    units.push(0);
    Ok(units)
}

fn tools() -> Result<(PathBuf, Vec<PathBuf>)> {
    let installer = PathBuf::from(
        std::env::var_os("ProgramFiles(x86)").ok_or("缺少 Windows ProgramFiles(x86)")?,
    )
    .join("Microsoft Visual Studio/Installer/vswhere.exe");
    let found = output(Command::new(installer).args([
        "-utf8",
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ]))?;
    if !found.status.success() {
        return Err(format!(
            "Visual Studio 工具定位失败（{}）\n{}{}",
            found.status,
            String::from_utf8_lossy(&found.stdout),
            String::from_utf8_lossy(&found.stderr)
        ).into());
    }
    let installation = host_text(&found.stdout, HostText::InstallationPath, OsString::from_wide)?;
    if installation.is_empty() {
        return Err("未安装 Visual Studio C++ 本机工具".into());
    }
    let vs = Path::new(&installation);
    let vc = {
        let path = vs.join("VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt");
        let bytes = crate::file_bytes::FileBytes::read(&File::open(path)?)?;
        let version = host_text(&bytes, HostText::ToolVersion, OsString::from_wide)?;
        vs.join("VC/Tools/MSVC").join(version)
    };
    let lib = sdk_root()?.join("Lib");
    let pattern = sdk_search_pattern(&lib)?;
    let mut version = [0u16; 260];
    let selected = unsafe { find_sdk_version(pattern.as_ptr(), version.as_mut_ptr()) };
    if selected.status != 0 {
        return Err(io::Error::from_raw_os_error(selected.status as i32).into());
    }
    if selected.units == 0 {
        return Err("Windows SDK 没有已安装的本机库".into());
    }
    let sdk = lib.join(OsString::from_wide(&version[..selected.units as usize]));
    Ok((
        vc.join("bin/Hostx64/x64/link.exe"),
        vec![vc.join("lib/x64"), sdk.join("um/x64"), sdk.join("ucrt/x64")],
    ))
}
fn path_argument(prefix: &str, path: &Path) -> OsString {
    let mut argument = OsString::from(prefix);
    argument.push(path);
    argument
}

fn write_input(temporary: &mut Vec<PathBuf>, path: PathBuf, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create_new(&path)?;
    temporary.push(path);
    file.write_all(bytes)
}

pub(super) fn link(archives: &[Archive], objects: &[&crate::object::Object], path: &Path, import_library: bool) -> Result<()> {
    let binding = super::resources::resources(&archives.iter().collect::<Vec<_>>(), 0, 0, std::iter::empty(), std::iter::empty(), None)?;
    let (linker, libraries) = tools()?;
    let id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let mut temporary = Vec::new();
    let mut result = (|| {
        let directory = path.parent().unwrap();
        for (index, archive) in archives.iter().enumerate() {
            let path = directory.join(format!(".blink-link-{id}-{index}.lib"));
            write_input(&mut temporary, path, &archive.bytes)?;
        }
        for (index, object) in objects.iter().enumerate() {
            let path = directory.join(format!(".blink-link-{id}-code-{index}.obj"));
            write_input(&mut temporary, path, &object.coff_bytes()?)?;
        }
        // Same import-slot owner as the explicit native-host archive product.
        let imports = objects.iter().flat_map(|object| &object.sections)
            .flat_map(|section| &section.relocations)
            .filter_map(|relocation| match &relocation.target {
                crate::object::Target::Import(import) => Some(import), _ => None,
            }).collect::<std::collections::BTreeSet<_>>();
        if !imports.is_empty() {
            let mut members = Vec::new();
            let mut symbols = Vec::new();
            for import in imports {
                let (member, symbol) = super::imports::member(import)?;
                symbols.push((members.len(), symbol));
                members.push(member);
            }
            let path = directory.join(format!(".blink-link-{id}-imports.lib"));
            let bytes = crate::object::library_bytes(&members, &symbols)?;
            write_input(&mut temporary, path, &bytes)?;
        }
        if let Some(binding) = binding {
            let path = directory.join(format!(".blink-link-{id}-resources.obj"));
            write_input(&mut temporary, path, &binding.coff_bytes()?)?;
        }
        let mut command = Command::new(linker);
        command
            .args([
                "/NOLOGO",
                "/MACHINE:X64",
                "/OPT:REF",
                "/OPT:ICF",
                "/NOEXP",
            ])
            .arg(path_argument("/OUT:", path))
            .args(&temporary);
        if let Some(crate::object::Object { kind: crate::object::Kind::Application { stack_bytes }, .. }) = objects.first().copied() {
            command.arg("/SUBSYSTEM:CONSOLE").arg(format!("/STACK:{}", super::pe::stack_reserve(*stack_bytes)));
        } else {
            command.arg("/DLL");
        }
        if import_library {
            let mut import_path = path.to_path_buf();
            import_path.set_extension("lib");
            command.arg(path_argument("/IMPLIB:", &import_path));
        } else {
            command.arg("/NOIMPLIB");
        }
        for lib in libraries {
            command.arg(path_argument("/LIBPATH:", &lib));
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
        let linked = output(&mut command)?;
        if linked.status.success() {
            return Ok(());
        }
        let mut diagnostics = linked.stdout;
        diagnostics.extend(linked.stderr);
        let details = match host_text(&diagnostics, HostText::LinkOutput, String::from_utf16)
            .and_then(|text| Ok(text?)) {
            Ok(text) => text,
            Err(error) => format!("链接工具诊断解码失败：{error}"),
        };
        Err(format!(
            "本机链接工具失败（{}）：{path:?}\n{details}",
            linked.status
        ).into())
    })();
    for path in temporary {
        let removed = std::fs::remove_file(path).map_err(Into::into);
        result = result.and(removed);
    }
    result
}
