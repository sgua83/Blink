//! Windows encoding and caller-owned buffers for the shared B path algorithms.
use std::{ffi::OsString, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}};

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
unsafe extern "C" {
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
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let input = UnitSpan::of(&units);
    let tail = [suffix];
    let plan = unsafe { extension_prepare(input, 1) };
    let length = plan.length as usize;
    let mut output = Vec::<u16>::with_capacity(length);
    unsafe {
        extension_encode(input, UnitSpan::of(&tail), plan, output.as_mut_ptr());
        output.set_len(length);
    }
    PathBuf::from(OsString::from_wide(&output))
}
