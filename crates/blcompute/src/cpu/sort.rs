//! Native CPU stable-sort image bindings and range completion.
use super::*;
pub(super) struct Code {
    source: usize,
    target: usize,
    initialize: Vec<*mut c_void>,
    histogram: *mut c_void,
    scatter: *mut c_void,
    gather: *mut c_void,
}
impl Code {
    pub(super) fn entries(&self) -> impl Iterator<Item = *mut c_void> + '_ {
        self.initialize.iter().copied().chain([self.histogram, self.scatter, self.gather])
    }
    pub fn load<C>(code: &blanguage::native::SortCode<C>, load: &mut impl FnMut(&C) -> *mut c_void) -> Self {
        Self {
            source: code.source,
            target: code.target,
            initialize: code
                .initialize
                .iter()
                .map(&mut *load)
                .collect(),
            histogram: load(&code.histogram),
            scatter: load(&code.scatter),
            gather: load(&code.gather),
        }
    }
}
#[repr(C)]
struct SortPass {
    entry: *mut c_void,
    buffers: *const BufferAddress,
    plan: work::RangePlan,
    parameters: *const u32,
    parameter_slot: usize,
    parameter_words: usize,
}

#[repr(C)]
struct SortInput {
    source: BufferAddress,
    target: BufferAddress,
    count: u32,
    workers: u32,
    initializers: *const *mut c_void,
    keys: usize,
    histogram: *mut c_void,
    scatter: *mut c_void,
    gather: *mut c_void,
    context: *mut c_void,
    execute: unsafe extern "C" fn(*mut c_void, SortPass) -> u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.cpu_sort::sort_records"]
    fn sort_records(input: SortInput) -> u32;
}

struct PassContext<'a, 'timing> {
    host: &'a CpuCode,
    stage: usize,
    errors: &'a mut Vec<*mut c_void>,
    timings: &'a mut Option<&'timing mut CpuRangeTimings>,
    failure: Option<Box<dyn std::error::Error + Send + Sync>>,
}

unsafe extern "C" fn execute_pass(context: *mut c_void, pass: SortPass) -> u32 {
    let context = unsafe { &mut *context.cast::<PassContext<'_, '_>>() };
    let mut work = RangeWork {
        frame: None,
        entry: pass.entry,
        buffers: pass.buffers,
        range_parameter_slot: pass.parameter_slot,
        uniforms: std::ptr::null(),
        plan: pass.plan,
        result: RangeResult::None,
        partials: std::ptr::null_mut(),
        range_parameters: pass.parameters,
        range_parameter_words: pass.parameter_words,
        stage: context.stage,
        timing: None,
    };
    match context.host.execute(&mut work, context.errors, context.timings) {
        Ok(()) => 0,
        Err(error) => {
            context.failure = Some(error);
            1
        }
    }
}

pub(super) fn run(
    host: &CpuCode,
    code: &Code,
    pointers: &[BufferAddress],
    count: u32,
    stage: usize,
    errors: &mut Vec<*mut c_void>,
    timings: &mut Option<&mut CpuRangeTimings>,
) -> Result<()> {
    let mut context = PassContext {
        host,
        stage,
        errors,
        timings,
        failure: None,
    };
    let status = unsafe {
        sort_records(SortInput {
            source: pointers[code.source],
            target: pointers[code.target],
            count,
            workers: host.workers as u32,
            initializers: code.initialize.as_ptr(),
            keys: code.initialize.len(),
            histogram: code.histogram,
            scatter: code.scatter,
            gather: code.gather,
            context: (&mut context as *mut PassContext<'_, '_>).cast(),
            execute: execute_pass,
        })
    };
    match status {
        0 => Ok(()),
        1 => Err(std::io::Error::from_raw_os_error(8).into()),
        2 => Err(context.failure.expect("sort pass reported its range error")),
        _ => unreachable!("unexpected native sort status {status}"),
    }
}
