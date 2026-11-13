//! Rust 执行框架的静态目标库使用既有 Windows 本机链接工具；不执行源码编译。
use super::Result;
use crate::{file_bytes::{self, FileBytes, host_path}, object::Archive, source_path::join};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::ExitStatus,
};
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

#[repr(C)]
struct HostProcessOutput {
    stdout: FileBytes,
    stderr: FileBytes,
    exit_code: u32,
    status: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.host_sdk::run_host_process"]
    fn run_host_process(
        executable: *const u16,
        units: usize,
        arguments: *const u16,
        argument_units: usize,
    ) -> HostProcessOutput;
    #[link_name = "bootstrap.host_sdk::remove_host_input"]
    fn remove_host_input(path: *const u16, units: usize) -> u32;
    #[link_name = "bootstrap.host_sdk::find_sdk_version"]
    fn find_sdk_version(pattern: *const u16, units: usize, selected: *mut u16) -> SdkVersionResult;
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

fn host_process(
    program: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<(ExitStatus, FileBytes, FileBytes)> {
    let executable = host_path(program)?;
    let mut units = Vec::new();
    for argument in arguments {
        let argument = argument.as_ref();
        if argument.as_encoded_bytes().contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "nul byte found in provided data").into());
        }
        units.extend(argument.encode_wide());
        units.push(0);
    }
    let result = unsafe {
        run_host_process(executable.as_ptr(), executable.len() - 1, units.as_ptr(), units.len())
    };
    if result.status != 0 {
        return Err(io::Error::from_raw_os_error(result.status as i32).into());
    }
    Ok((ExitStatus::from_raw(result.exit_code), result.stdout.into_result()?, result.stderr.into_result()?))
}

fn tools() -> Result<(PathBuf, Vec<PathBuf>)> {
    let program_files = std::env::var_os("ProgramFiles(x86)").ok_or("缺少 Windows ProgramFiles(x86)")?;
    let installer = join(Path::new(&program_files), "Microsoft Visual Studio/Installer/vswhere.exe");
    let (status, stdout, stderr) = host_process(&installer, [
        "-utf8",
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ])?;
    if !status.success() {
        return Err(format!(
            "Visual Studio 工具定位失败（{}）\n{}{}",
            status,
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        ).into());
    }
    let installation = host_text(&stdout, HostText::InstallationPath, OsString::from_wide)?;
    if installation.is_empty() {
        return Err("未安装 Visual Studio C++ 本机工具".into());
    }
    let vs = Path::new(&installation);
    let vc = {
        let path = join(vs, "VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt");
        let bytes = FileBytes::read(&File::open(path)?)?;
        let version = host_text(&bytes, HostText::ToolVersion, OsString::from_wide)?;
        join(&join(vs, "VC/Tools/MSVC"), version)
    };
    let lib = join(&sdk_root()?, "Lib");
    let pattern = host_path(&join(&lib, "*"))?;
    let mut version = [0u16; 260];
    let selected = unsafe { find_sdk_version(pattern.as_ptr(), pattern.len() - 1, version.as_mut_ptr()) };
    if selected.status != 0 {
        return Err(io::Error::from_raw_os_error(selected.status as i32).into());
    }
    if selected.units == 0 {
        return Err("Windows SDK 没有已安装的本机库".into());
    }
    let sdk = join(&lib, OsString::from_wide(&version[..selected.units as usize]));
    Ok((
        join(&vc, "bin/Hostx64/x64/link.exe"),
        vec![join(&vc, "lib/x64"), join(&sdk, "um/x64"), join(&sdk, "ucrt/x64")],
    ))
}
fn path_argument(prefix: &str, path: &Path) -> OsString {
    let mut argument = OsString::from(prefix);
    argument.push(path);
    argument
}

fn write_input(temporary: &mut Vec<PathBuf>, path: PathBuf, bytes: &[u8]) -> io::Result<()> {
    let written = file_bytes::write_new(&path, bytes)?;
    if written.created() {
        temporary.push(path);
    }
    written.into_result()
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
            let path = join(directory, format!(".blink-link-{id}-{index}.lib"));
            write_input(&mut temporary, path, &archive.bytes)?;
        }
        for (index, object) in objects.iter().enumerate() {
            let path = join(directory, format!(".blink-link-{id}-code-{index}.obj"));
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
            let path = join(directory, format!(".blink-link-{id}-imports.lib"));
            let bytes = crate::object::library_bytes(&members, &symbols)?;
            write_input(&mut temporary, path, &bytes)?;
        }
        if let Some(binding) = binding {
            let path = join(directory, format!(".blink-link-{id}-resources.obj"));
            write_input(&mut temporary, path, &binding.coff_bytes()?)?;
        }
        let mut arguments: Vec<_> = ["/NOLOGO", "/MACHINE:X64", "/OPT:REF", "/OPT:ICF", "/NOEXP"]
            .into_iter().map(OsString::from).collect();
        arguments.push(path_argument("/OUT:", path));
        arguments.extend(temporary.iter().map(|path| path.as_os_str().to_owned()));
        if let Some(crate::object::Object { kind: crate::object::Kind::Application { stack_bytes }, .. }) = objects.first().copied() {
            arguments.push("/SUBSYSTEM:CONSOLE".into());
            arguments.push(format!("/STACK:{}", super::pe::stack_reserve(*stack_bytes)).into());
        } else {
            arguments.push("/DLL".into());
        }
        if import_library {
            let import_path = crate::source_path::with_extension_units(
                path, &[b'l' as u16, b'i' as u16, b'b' as u16],
            );
            arguments.push(path_argument("/IMPLIB:", &import_path));
        } else {
            arguments.push("/NOIMPLIB".into());
        }
        arguments.extend(libraries.iter().map(|lib| path_argument("/LIBPATH:", lib)));
        let libraries = archives
            .iter()
            .flat_map(|archive| &archive.libraries)
            .collect::<std::collections::BTreeSet<_>>();
        let exports = archives
            .iter()
            .flat_map(|archive| &archive.exports)
            .collect::<std::collections::BTreeSet<_>>();
        arguments.extend(libraries.into_iter().map(|name| OsString::from(name.as_str())));
        arguments.extend(exports.into_iter().map(|name| OsString::from(format!("/EXPORT:{name}"))));

        let (status, stdout, stderr) = host_process(&linker, arguments)?;
        if status.success() {
            return Ok(());
        }
        let mut diagnostics = Vec::with_capacity(stdout.len() + stderr.len());
        diagnostics.extend_from_slice(&stdout);
        diagnostics.extend_from_slice(&stderr);
        let details = match host_text(&diagnostics, HostText::LinkOutput, String::from_utf16)
            .and_then(|text| Ok(text?)) {
            Ok(text) => text,
            Err(error) => format!("链接工具诊断解码失败：{error}"),
        };
        Err(format!(
            "本机链接工具失败（{}）：{path:?}\n{details}",
            status
        ).into())
    })();
    for path in temporary {
        let removed = host_path(&path).and_then(|name| {
            let status = unsafe { remove_host_input(name.as_ptr(), name.len() - 1) };
            if status == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
        });
        result = result.and(removed.map_err(Into::into));
    }
    result
}
