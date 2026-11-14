use std::{ffi::OsString, os::windows::ffi::OsStringExt};

#[derive(Clone, Copy)]
#[repr(C)]
struct CommandLineSpan {
    data: *const u16,
    length: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct CommandLineSizes {
    units: u32,
    arguments: u32,
}

#[repr(C)]
struct ArgumentRange {
    start: u32,
    length: u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.process_core::blink_process_command_line"]
    fn blink_process_command_line() -> CommandLineSpan;
    #[link_name = "blink.windows.process_core::blink_process_arguments_size"]
    fn blink_process_arguments_size(line: CommandLineSpan) -> CommandLineSizes;
    #[link_name = "blink.windows.process_core::blink_process_arguments_write"]
    fn blink_process_arguments_write(
        line: CommandLineSpan, sizes: CommandLineSizes,
        units: *mut u16, ranges: *mut ArgumentRange,
    );
}

/// Produce the original tool arguments after argv[0]. B owns Windows quoting
/// and UTF-16 placement; OsString keeps its original Windows encoding and life.
pub(crate) fn read() -> Vec<OsString> {
    // The OS keeps the input alive. B fills both output buffers synchronously
    // according to its plan; all initialized units/rows remain ours thereafter.
    let (units, ranges) = unsafe {
        let line = blink_process_command_line();
        let sizes = blink_process_arguments_size(line);
        let mut units = Vec::<u16>::with_capacity(sizes.units as usize);
        let mut ranges = Vec::<ArgumentRange>::with_capacity(sizes.arguments as usize);
        blink_process_arguments_write(line, sizes, units.as_mut_ptr(), ranges.as_mut_ptr());
        units.set_len(sizes.units as usize);
        ranges.set_len(sizes.arguments as usize);
        (units, ranges)
    };
    ranges.into_iter().skip(1).map(|range| {
        let start = range.start as usize;
        OsString::from_wide(&units[start..start + range.length as usize])
    }).collect()
}
