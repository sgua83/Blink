//! Windows encoding and caller-owned buffers for the shared B path algorithms.
use std::{ffi::{OsStr, OsString}, io, mem::MaybeUninit, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}};

#[derive(Clone, Copy)]
#[repr(C)]
struct UnitSpan { data: *const u16, length: u64 }
impl UnitSpan {
    fn of(units: &[u16]) -> Self { Self { data: units.as_ptr(), length: units.len() as u64 } }
}
#[repr(C)]
struct JoinComponent { from_base: bool, start: u64, length: u64, kind: u64 }
#[repr(C)]
struct JoinPlan { length: u64, components: u64, base_length: u64, separator: u64 }
#[repr(C)]
struct ExtensionPlan { length: u64, copy: u64, append: u64 }
#[repr(C)]
struct NativePath {
    name: *const u16,
    allocation: *mut u16,
    units: usize,
    status: u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.path_core::absolute_path"]
    fn absolute_path(path: *const u16, units: usize) -> NativePath;
    #[link_name = "blink.windows.path_core::canonicalize_path"]
    fn canonicalize_path(path: *const u16, units: usize, scratch: *mut u16, capacity: u32) -> NativePath;
    #[link_name = "blink.windows.path_core::release_native_path"]
    fn release_native_path(path: NativePath);
    #[link_name = "blink.windows.path_core::file_stem"]
    fn file_stem_units(path: UnitSpan) -> UnitSpan;
    #[link_name = "blink.windows.path_core::module_tail"]
    fn module_tail(name: *mut u16, length: u64, suffix: u16);
    #[link_name = "blink.windows.path_core::join_scratch"]
    fn join_scratch(base: UnitSpan, path: UnitSpan) -> u64;
    #[link_name = "blink.windows.path_core::join_prepare"]
    fn join_prepare(base: UnitSpan, path: UnitSpan, scratch: *mut JoinComponent) -> JoinPlan;
    #[link_name = "blink.windows.path_core::join_encode"]
    fn join_encode(base: UnitSpan, path: UnitSpan, scratch: *mut JoinComponent, plan: JoinPlan, output: *mut u16);
    #[link_name = "blink.windows.path_core::extension_prepare"]
    fn extension_prepare(path: UnitSpan, length: u64) -> ExtensionPlan;
    #[link_name = "blink.windows.path_core::extension_encode"]
    fn extension_encode(path: UnitSpan, extension: UnitSpan, plan: ExtensionPlan, output: *mut u16);
}

pub(crate) fn module(root: &Path, name: &str, suffix: u16) -> PathBuf {
    let mut tail: Vec<u16> = name.encode_utf16().collect();
    let name_length = tail.len();
    tail.resize(name_length + 2, 0);
    // The validated module name and both path inputs remain owned here until
    // synchronous encoding ends. Scratch holds the original C-layout components.
    unsafe { module_tail(tail.as_mut_ptr(), name_length as u64, suffix); }
    join_encoded(root, &tail)
}

pub(crate) fn join(base: &Path, tail: impl AsRef<Path>) -> PathBuf {
    let tail: Vec<u16> = tail.as_ref().as_os_str().encode_wide().collect();
    join_encoded(base, &tail)
}

fn join_encoded(base: &Path, tail: &[u16]) -> PathBuf {
    let units: Vec<u16> = base.as_os_str().encode_wide().collect();
    let left = UnitSpan::of(&units);
    let right = UnitSpan::of(tail);
    let mut scratch = Vec::<JoinComponent>::with_capacity(unsafe { join_scratch(left, right) } as usize);
    let plan = unsafe { join_prepare(left, right, scratch.as_mut_ptr()) };
    let length = plan.length as usize;
    let mut output = Vec::<u16>::with_capacity(length);
    unsafe {
        join_encode(left, right, scratch.as_mut_ptr(), plan, output.as_mut_ptr());
        output.set_len(length);
    }
    PathBuf::from(OsString::from_wide(&output))
}

pub(crate) fn with_extension(path: &Path, suffix: u16) -> PathBuf {
    with_extension_units(path, &[suffix])
}

pub(crate) fn with_extension_units(path: &Path, tail: &[u16]) -> PathBuf {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let input = UnitSpan::of(&units);
    let plan = unsafe { extension_prepare(input, tail.len() as u64) };
    let length = plan.length as usize;
    let mut output = Vec::<u16>::with_capacity(length);
    unsafe {
        extension_encode(input, UnitSpan::of(tail), plan, output.as_mut_ptr());
        output.set_len(length);
    }
    PathBuf::from(OsString::from_wide(&output))
}

fn path_units(path: &OsStr) -> io::Result<Vec<u16>> {
    if path.as_encoded_bytes().contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "strings passed to WinAPI cannot contain NULs"));
    }
    Ok(path.encode_wide().chain([0]).collect())
}

fn native_path_result(resolved: NativePath) -> io::Result<PathBuf> {
    if resolved.status != 0 {
        return Err(io::Error::from_raw_os_error(resolved.status as i32));
    }
    // The native name remains valid in the caller's storage or on the Windows heap.
    let name = OsString::from_wide(unsafe { std::slice::from_raw_parts(resolved.name, resolved.units) });
    unsafe { release_native_path(resolved); }
    Ok(PathBuf::from(name))
}

pub(crate) fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot make an empty path absolute"));
    }
    let units = path_units(path.as_os_str())?;
    native_path_result(unsafe { absolute_path(units.as_ptr(), units.len() - 1) })
}

pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    let units = path_units(path.as_os_str())?;
    let mut scratch = [MaybeUninit::<u16>::uninit(); 512];
    let resolved = unsafe {
        canonicalize_path(units.as_ptr(), units.len() - 1, scratch.as_mut_ptr().cast(), scratch.len() as u32)
    };
    native_path_result(resolved)
}

pub(crate) fn file_stem(path: &Path) -> Option<String> {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let stem = unsafe { file_stem_units(UnitSpan::of(&units)) };
    if stem.length == 0 {
        return None;
    }
    let units = unsafe { std::slice::from_raw_parts(stem.data, stem.length as usize) };
    String::from_utf16(units).ok()
}
