//! Rust 执行框架的静态目标库使用既有 Windows 本机链接工具；不执行源码编译。
use super::{Result, Selected, resources::Generated};
use crate::{
    file_bytes::{FileBytes, HostWriteResult, host_path},
    object::{Archive, ImportKey, LinkSectionBytes, encode_native, encode_native_with_imports},
};
use std::{
    ffi::{OsStr, c_void},
    io,
    os::windows::{
        ffi::OsStrExt,
        process::ExitStatusExt,
    },
    path::Path,
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
    #[link_name = "bootstrap.host_sdk::link_msvc_inputs"]
    fn link_msvc_inputs(
        request: MsvcLinkRequest,
        input: unsafe extern "C" fn(*mut c_void, MsvcInputKind, usize) -> MsvcLinkInput,
        arguments: unsafe extern "C" fn(*mut c_void, *const ToolPath, usize) -> ToolPath,
    ) -> MsvcLinkResult;
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

#[repr(C)]
#[derive(Clone, Copy)]
struct ToolPath { data: *const u16, length: usize }

impl ToolPath {
    fn of(units: &[u16]) -> Self {
        Self { data: units.as_ptr(), length: units.len() }
    }

    fn units(&self) -> &[u16] {
        // The B owner keeps each initialized UTF-16 path through this borrow.
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
    fn from_archives(archives: impl Iterator<Item = &'a Archive>) -> (Self, usize) {
        let mut libraries = Vec::new();
        let mut exports = Vec::new();
        let mut archive_count = 0;
        for archive in archives {
            archive_count += 1;
            libraries.extend(archive.libraries.iter().map(|name| MsvcName::new(name)));
            exports.extend(archive.exports.iter().map(|name| MsvcName::new(name)));
        }
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
        (Self { libraries, exports, needs_sdk: plan.needs_sdk != 0 }, archive_count)
    }
}

fn link_arguments(
    tools: &MsvcTools, output: &Path, inputs: &[ToolPath],
    names: &MsvcNames<'_>, sdk: Option<&Path>, stack_reserve: u64, import_library: bool,
) -> Result<Vec<u16>> {
    if tools.paths[0].units().contains(&0) {
        return Err(invalid_tool_name().into());
    }
    let mut text = Vec::new();
    let output_path = append_wide(&mut text, output.as_os_str())?;
    let inputs: Vec<_> = inputs.iter().map(|path| {
        let start = text.len() as u64;
        text.extend_from_slice(path.units());
        MsvcText { start, units: path.length as u64 }
    }).collect();
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
    Ok(arguments)
}

#[repr(u32)]
#[allow(dead_code)]
enum MsvcInputKind { Archive, Code, Imports, Resources }

#[repr(C)]
struct MsvcLinkInput {
    filename: ToolPath,
    data: *const u8,
    bytes: usize,
    failed: u32,
}

#[repr(C)]
struct MsvcLinkRequest {
    directory: ToolPath,
    executable: ToolPath,
    archives: usize,
    objects: usize,
    paths: *mut ToolPath,
    context: *mut c_void,
}

#[repr(C)]
struct MsvcLinkResult {
    written: HostWriteResult,
    process: HostProcessOutput,
    cleanup: u32,
}

struct LinkHost<'a, 'code> {
    archive_objects: std::slice::Iter<'a, Selected<LinkSectionBytes<'code>>>,
    objects: &'a [Selected<LinkSectionBytes<'code>>],
    binding: Option<Generated>,
    tools: &'a MsvcTools,
    path: &'a Path,
    names: &'a MsvcNames<'a>,
    sdk: Option<&'a Path>,
    stack_reserve: u64,
    import_library: bool,
    id: String,
    filename: Vec<u16>,
    imports: Vec<ImportKey<'a>>,
    bytes: Vec<u8>,
    arguments: Vec<u16>,
    error: Result<()>,
}

impl LinkHost<'_, '_> {
    fn input(&mut self, kind: MsvcInputKind, index: usize) -> Result<Option<MsvcLinkInput>> {
        self.bytes = Vec::new();
        let suffix = match kind {
            MsvcInputKind::Archive => format!("{index}.lib"),
            MsvcInputKind::Code => {
                let object = &self.objects[index];
                self.bytes = encode_native_with_imports(
                    &object.name, &object.sections, &object.definitions, &mut self.imports,
                )?;
                format!("code-{index}.obj")
            }
            MsvcInputKind::Imports => {
                if self.imports.is_empty() {
                    return Ok(None);
                }
                // The host archive and the MSVC inputs share one import-slot producer.
                self.bytes = super::imports::archive_with_imports(&[], std::mem::take(&mut self.imports))?;
                "imports.lib".into()
            }
            MsvcInputKind::Resources => {
                let Some(binding) = self.binding.take() else { return Ok(None) };
                self.bytes = encode_native(Generated::NAME, &binding.sections, &binding.definitions)?;
                "resources.obj".into()
            }
        };
        self.filename.clear();
        self.filename.extend(format!(".blink-link-{}-{suffix}", self.id).encode_utf16());
        let bytes = match kind {
            // B requests each archive once in selection order before the code inputs.
            MsvcInputKind::Archive => self.archive_objects
                .find_map(|object| object.archive.as_ref()).unwrap().bytes.as_ref(),
            _ => self.bytes.as_slice(),
        };
        Ok(Some(MsvcLinkInput {
            filename: ToolPath::of(&self.filename),
            data: bytes.as_ptr(),
            bytes: bytes.len(),
            failed: 0,
        }))
    }
}

unsafe extern "C" fn prepare_link_input(
    context: *mut c_void, kind: MsvcInputKind, index: usize,
) -> MsvcLinkInput {
    // The synchronous writer consumes these bytes before advancing to another input.
    let host = unsafe { &mut *context.cast::<LinkHost<'_, '_>>() };
    let failed = match host.input(kind, index) {
        Ok(Some(input)) => return input,
        Ok(None) => 0,
        Err(error) => {
            host.error = Err(error);
            1
        }
    };
    MsvcLinkInput { filename: ToolPath::of(&[]), data: std::ptr::null(), bytes: 0, failed }
}

unsafe extern "C" fn prepare_link_arguments(
    context: *mut c_void, paths: *const ToolPath, count: usize,
) -> ToolPath {
    // B has finished all writes and owns this initialized prefix until the process ends.
    let host = unsafe { &mut *context.cast::<LinkHost<'_, '_>>() };
    host.bytes = Vec::new();
    let paths = unsafe { std::slice::from_raw_parts(paths, count) };
    match link_arguments(host.tools, host.path, paths, host.names, host.sdk, host.stack_reserve, host.import_library) {
        Ok(arguments) => {
            host.arguments = arguments;
            ToolPath::of(&host.arguments)
        }
        Err(error) => {
            host.error = Err(error);
            ToolPath { data: std::ptr::null(), length: 0 }
        }
    }
}

pub(super) fn link(
    objects: &[Selected<LinkSectionBytes<'_>>], path: &Path, stack_reserve: u64, import_library: bool,
) -> Result<()> {
    let archives = objects.iter().filter_map(|object| object.archive.as_ref());
    let binding = super::resources::resources(archives.clone(), 0, 0, std::iter::empty(), std::iter::empty(), None)?;
    let tools = tools()?;
    let (names, archive_count) = MsvcNames::from_archives(archives);
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
    let directory: Vec<_> = std::env::temp_dir().as_os_str().encode_wide().collect();
    let mut paths = Vec::<ToolPath>::with_capacity(archive_count + objects.len() + 2);
    let mut host = LinkHost {
        archive_objects: objects.iter(), objects, binding, tools: &tools, path, names: &names,
        sdk: sdk.as_ref().map(|sdk| sdk.root.as_path()), stack_reserve, import_library,
        id, filename: Vec::new(), imports: Vec::new(), bytes: Vec::new(),
        arguments: Vec::new(), error: Ok(()),
    };
    let request = MsvcLinkRequest {
        directory: ToolPath::of(&directory), executable: tools.paths[0],
        archives: archive_count, objects: objects.len(), paths: paths.as_mut_ptr(),
        context: (&mut host as *mut LinkHost<'_, '_>).cast(),
    };
    // B initializes and uses the path prefix in the reserved storage. It releases every
    // owned path before returning. Rust retains the original bytes and process output owners.
    let result = unsafe { link_msvc_inputs(request, prepare_link_input, prepare_link_arguments) };
    host.error?;
    result.written.into_result()?;
    let status = result.process.checked_status()?;
    if status.success() {
        if result.cleanup != 0 {
            return Err(io::Error::from_raw_os_error(result.cleanup as i32).into());
        }
        return Ok(());
    }
    let stdout = &result.process.stdout[..];
    let stderr = &result.process.stderr[..];
    let joined;
    let diagnostics = match (stdout, stderr) {
        ([], bytes) | (bytes, []) => bytes,
        _ => {
            // The decoder consumes one contiguous byte range in stdout, stderr order.
            joined = [stdout, stderr].concat();
            &joined
        }
    };
    let details = match host_text(diagnostics, HostText::LinkOutput, String::from_utf16)
        .and_then(|text| Ok(text?)) {
        Ok(text) => text,
        Err(error) => format!("链接工具诊断解码失败：{error}"),
    };
    Err(format!("本机链接工具失败（{}）：{path:?}\n{details}", status).into())
}
