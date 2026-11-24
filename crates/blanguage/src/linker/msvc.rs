//! Rust 执行框架的静态目标库使用既有 Windows 本机链接工具；不执行源码编译。
use super::Result;
use crate::{file_bytes::{self, FileBytes, host_path}, object::{Archive, encode_native, encode_native_with_imports}, source_path::join};
use std::{
    ffi::OsStr,
    io,
    os::windows::{
        ffi::OsStrExt,
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
struct HostProcessOutput {
    stdout: FileBytes,
    stderr: FileBytes,
    exit_code: u32,
    status: u32,
}

#[repr(C)]
#[derive(Default)]
struct MsvcText {
    start: u64,
    units: u64,
}

#[repr(C)]
struct MsvcGroup {
    spans: *const MsvcText,
    count: u64,
}

impl From<&[MsvcText]> for MsvcGroup {
    fn from(spans: &[MsvcText]) -> Self {
        Self { spans: spans.as_ptr(), count: spans.len() as u64 }
    }
}

#[repr(C)]
struct MsvcName<'a> {
    bytes: *const u8,
    length: usize,
    sdk_relative: u64,
    borrowed: std::marker::PhantomData<&'a str>,
}

impl<'a> MsvcName<'a> {
    fn new(text: &'a str) -> Self {
        Self { bytes: text.as_ptr(), length: text.len(), sdk_relative: 0, borrowed: std::marker::PhantomData }
    }

    fn text(&self) -> &'a str {
        // B only orders and compacts the original borrowed UTF-8 spans.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.length)) }
    }
}

#[repr(C)]
struct MsvcNamePlan {
    libraries: usize,
    exports: usize,
    needs_sdk: u64,
}

struct MsvcNames<'a> {
    libraries: Vec<MsvcName<'a>>,
    exports: Vec<MsvcName<'a>>,
    needs_sdk: bool,
}

#[repr(C)]
struct MsvcArguments {
    text: *const u16,
    output: MsvcText,
    inputs: MsvcGroup,
    import_library: MsvcText,
    search_paths: MsvcGroup,
    libraries: MsvcGroup,
    exports: MsvcGroup,
    stack_reserve: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.bindings::select_msvc_names"]
    fn select_msvc_names(libraries: *mut MsvcName<'_>, library_count: usize,
        exports: *mut MsvcName<'_>, export_count: usize,
        prefix: *const u8, prefix_length: usize, nodes: *mut [u64; 9]) -> MsvcNamePlan;
    #[link_name = "bootstrap.cli::write_msvc_arguments"]
    fn write_msvc_arguments(input: MsvcArguments, output: *mut u16) -> u64;
    #[link_name = "bootstrap.host_sdk::run_host_process"]
    fn run_host_process(
        executable: *const u16,
        units: usize,
        arguments: *const u16,
        argument_units: usize,
    ) -> HostProcessOutput;
    #[link_name = "bootstrap.host_sdk::remove_host_input"]
    fn remove_host_input(path: *const u16, units: usize) -> u32;
    #[link_name = "bootstrap.host_sdk::discover_msvc_tools"]
    fn discover_msvc_tools(program_files: ToolPath) -> MsvcTools;
    #[link_name = "bootstrap.host_sdk::decode_host_text"]
    fn decode_host_text(input: *const u8, bytes: i32, kind: HostText) -> HostWide;
    #[link_name = "bootstrap.host_sdk::release_host_wide"]
    fn release_host_wide(data: *const u16) -> i32;
}

fn host_text<T>(bytes: &[u8], kind: HostText, copy: impl FnOnce(&[u16]) -> T) -> Result<T> {
    if bytes.is_empty() {
        return Ok(copy(&[]));
    }
    let decoded = unsafe { decode_host_text(bytes.as_ptr(), i32::try_from(bytes.len())?, kind) };
    if decoded.status != 0 {
        return Err(io::Error::from_raw_os_error(decoded.status).into());
    }
    Ok(decoded.consume(copy)?)
}

fn append_wide(units: &mut Vec<u16>, argument: &OsStr) -> io::Result<MsvcText> {
    if argument.as_encoded_bytes().contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nul byte found in provided data"));
    }
    let start = units.len();
    units.extend(argument.encode_wide());
    Ok(MsvcText { start: start as u64, units: (units.len() - start) as u64 })
}

impl HostProcessOutput {
    fn checked_status(&self) -> io::Result<ExitStatus> {
        if self.status != 0 {
            return Err(io::Error::from_raw_os_error(self.status as i32));
        }
        self.stdout.check()?;
        self.stderr.check()?;
        Ok(ExitStatus::from_raw(self.exit_code))
    }
}

fn run_host_units(executable: &[u16], units: &[u16]) -> Result<(ExitStatus, FileBytes, FileBytes)> {
    let result = unsafe {
        run_host_process(executable.as_ptr(), executable.len(), units.as_ptr(), units.len())
    };
    let status = result.checked_status()?;
    Ok((status, result.stdout, result.stderr))
}

#[repr(C)]
struct ToolPath { data: *const u16, length: usize }

impl ToolPath {
    fn units(&self) -> &[u16] {
        // Discovery owns initialized, NUL-terminated paths through the link call.
        unsafe { std::slice::from_raw_parts(self.data, self.length) }
    }
}

#[allow(dead_code)]
#[repr(u32)]
enum MsvcDiscovery {
    Found, System, Locator, MissingTools, FileRead, VersionText, SdkRoot, MissingSdk,
    InvalidPath, TextLength,
}

#[repr(C)]
struct MsvcTools {
    paths: [ToolPath; 4],
    output: HostProcessOutput,
    failure: MsvcDiscovery,
    status: u32,
}

impl Drop for MsvcTools {
    fn drop(&mut self) {
        for path in &self.paths {
            if !path.data.is_null() {
                unsafe { release_host_wide(path.data); }
            }
        }
    }
}

fn invalid_tool_name() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "file name contained an unexpected NUL byte")
}

fn tools() -> Result<MsvcTools> {
    let program_files = std::env::var_os("ProgramFiles(x86)").ok_or("缺少 Windows ProgramFiles(x86)")?;
    let units = host_path(Path::new(&program_files))?;
    let tools = unsafe {
        discover_msvc_tools(ToolPath { data: units.as_ptr(), length: units.len() - 1 })
    };
    match tools.failure {
        MsvcDiscovery::Found => Ok(tools),
        MsvcDiscovery::System => Err(io::Error::from_raw_os_error(tools.status as i32).into()),
        MsvcDiscovery::Locator => {
            let status = tools.output.checked_status()?;
            Err(format!("Visual Studio 工具定位失败（{}）\n{}{}", status,
                String::from_utf8_lossy(&tools.output.stdout),
                String::from_utf8_lossy(&tools.output.stderr)).into())
        }
        MsvcDiscovery::MissingTools => Err("未安装 Visual Studio C++ 本机工具".into()),
        MsvcDiscovery::FileRead => Err(tools.output.stdout.read_error().into()),
        MsvcDiscovery::VersionText => {
            const ERROR_NO_UNICODE_TRANSLATION: u32 = 1113;
            let error = if tools.status == ERROR_NO_UNICODE_TRANSLATION {
                io::Error::new(io::ErrorKind::InvalidData, "stream did not contain valid UTF-8")
            } else {
                io::Error::from_raw_os_error(tools.status as i32)
            };
            Err(error.into())
        }
        MsvcDiscovery::SdkRoot => Err(format!("无法读取 Windows SDK KitsRoot10：{}",
            io::Error::from_raw_os_error(tools.status as i32)).into()),
        MsvcDiscovery::MissingSdk => Err("Windows SDK 没有已安装的本机库".into()),
        MsvcDiscovery::InvalidPath => Err(invalid_tool_name().into()),
        MsvcDiscovery::TextLength => Err("out of range integral type conversion attempted".into()),
    }
}

impl<'a> MsvcNames<'a> {
    fn from_archives(archives: &[&'a Archive]) -> Self {
        let mut libraries: Vec<_> = archives.iter().flat_map(|archive| &archive.libraries)
            .map(|name| MsvcName::new(name)).collect();
        let mut exports: Vec<_> = archives.iter().flat_map(|archive| &archive.exports)
            .map(|name| MsvcName::new(name)).collect();
        let mut nodes = Vec::with_capacity(libraries.len().max(exports.len()));
        let prefix = crate::object::SDK_RUNTIME.as_bytes();
        let plan = unsafe {
            select_msvc_names(
                libraries.as_mut_ptr(), libraries.len(), exports.as_mut_ptr(), exports.len(),
                prefix.as_ptr(), prefix.len(), nodes.as_mut_ptr(),
            )
        };
        libraries.truncate(plan.libraries);
        exports.truncate(plan.exports);
        Self { libraries, exports, needs_sdk: plan.needs_sdk != 0 }
    }
}

fn link_process(
    tools: &MsvcTools, output: &Path, inputs: &[PathBuf],
    names: &MsvcNames<'_>, sdk: Option<&Path>, stack_reserve: u64, import_library: bool,
) -> Result<(ExitStatus, FileBytes, FileBytes)> {
    let executable = tools.paths[0].units();
    if executable.contains(&0) {
        return Err(invalid_tool_name().into());
    }
    let mut text = Vec::new();
    let output_path = append_wide(&mut text, output.as_os_str())?;
    let inputs: Vec<_> = inputs.iter().map(|path| append_wide(&mut text, path.as_os_str()))
        .collect::<io::Result<_>>()?;
    let import_path = if import_library {
        let path = crate::source_path::with_extension_units(output, &[b'l' as u16, b'i' as u16, b'b' as u16]);
        append_wide(&mut text, path.as_os_str())?
    } else {
        MsvcText::default()
    };
    let search_paths = tools.paths[1..].iter().map(|path| {
        let argument = path.units();
        if argument.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "nul byte found in provided data"));
        }
        let start = text.len() as u64;
        text.extend_from_slice(argument);
        Ok(MsvcText { start, units: argument.len() as u64 })
    }).collect::<io::Result<Vec<_>>>()?;
    let libraries: Vec<_> = names.libraries.iter().map(|name| {
        if let Some(sdk) = sdk.filter(|_| name.sdk_relative != 0) {
            append_wide(&mut text, sdk.join(name.text()).as_os_str())
        } else {
            append_wide(&mut text, OsStr::new(name.text()))
        }
    }).collect::<io::Result<_>>()?;
    let exports: Vec<_> = names.exports.iter()
        .map(|name| append_wide(&mut text, OsStr::new(name.text())))
        .collect::<io::Result<_>>()?;
    let request = MsvcArguments {
        text: text.as_ptr(), output: output_path, inputs: inputs.as_slice().into(),
        import_library: import_path, search_paths: search_paths.as_slice().into(),
        libraries: libraries.as_slice().into(), exports: exports.as_slice().into(), stack_reserve,
    };
    // B writes the fixed options, one terminator per argument and the path/export prefixes.
    let capacity = 128 + text.len() + inputs.len() + search_paths.len() * 10
        + libraries.len() + exports.len() * 9;
    let mut arguments = Vec::with_capacity(capacity);
    unsafe {
        let used = write_msvc_arguments(request, arguments.as_mut_ptr());
        arguments.set_len(used as usize);
    }
    run_host_units(executable, &arguments)
}

fn write_input(temporary: &mut Vec<PathBuf>, path: PathBuf, bytes: &[u8]) -> io::Result<()> {
    let written = file_bytes::write_new(&path, bytes)?;
    if written.created() {
        temporary.push(path);
    }
    written.into_result()
}

pub(super) fn link(
    objects: &[super::Selected], path: &Path, stack_reserve: u64, import_library: bool,
) -> Result<()> {
    let archives = objects.iter().filter_map(|object| object.archive.as_ref()).collect::<Vec<_>>();
    let binding = super::resources::resources(&archives, 0, 0, std::iter::empty(), std::iter::empty(), None)?;
    let tools = tools()?;
    let names = MsvcNames::from_archives(&archives);
    let sdk = if names.needs_sdk {
        Some(crate::object::InstalledSdk::open()?)
    } else {
        None
    };
    let id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let directory = std::env::temp_dir();
    let mut temporary = Vec::new();
    let mut result = (|| {
        for (index, archive) in archives.iter().enumerate() {
            let path = join(&directory, format!(".blink-link-{id}-{index}.lib"));
            write_input(&mut temporary, path, &archive.bytes)?;
        }
        let mut imports = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            let path = join(&directory, format!(".blink-link-{id}-code-{index}.obj"));
            let code = encode_native_with_imports(&object.name, &object.sections, &object.definitions, &mut imports)?;
            write_input(&mut temporary, path, &code)?;
        }
        // The host archive and the MSVC inputs share one import-slot producer.
        let mut symbols = Vec::new();
        let members = super::imports::import_members(imports, 0, &mut symbols)?;
        if !members.is_empty() {
            let path = join(&directory, format!(".blink-link-{id}-imports.lib"));
            let bytes = crate::object::library_bytes(&members, &symbols)?;
            write_input(&mut temporary, path, &bytes)?;
        }
        if let Some(binding) = binding {
            let path = join(&directory, format!(".blink-link-{id}-resources.obj"));
            write_input(&mut temporary, path, &encode_native(super::resources::Generated::NAME, &binding.sections, &binding.definitions)?)?;
        }
        let (status, stdout, stderr) = link_process(
            &tools, path, &temporary, &names,
            sdk.as_ref().map(|sdk| sdk.root.as_path()), stack_reserve, import_library)?;
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
