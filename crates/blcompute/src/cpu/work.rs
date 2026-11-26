//! The native work queue borrows these exact bindings until Windows reports completion.
use super::{CpuRangeTiming, RangeEntry, RangeWork};
use std::{ffi::c_void, io, mem, ptr, sync::atomic::{AtomicUsize, Ordering}};

type WorkEntry = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void);
type ObservedEntry = unsafe extern "C" fn(
    *const c_void, *const super::BufferAddress, *mut u32, u32, u32, *mut u32, usize,
) -> *mut c_void;

#[repr(C)]
struct CreatedWork {
    handle: *mut c_void,
    error: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct RangePlan {
    count: usize,
    grain: usize,
    pub(super) ranges: usize,
    submissions: usize,
}

#[repr(C)]
struct RangeInput {
    plan: RangePlan,
    entry: RangeEntry,
    buffers: *const super::BufferAddress,
    parameter_slot: usize,
    uniforms: *const u32,
    frame_words: usize,
    partials: *mut u32,
    result_words: usize,
    errors: *mut *mut c_void,
    parameters: *const u32,
    parameter_words: usize,
    context: *const c_void,
    observed_entry: Option<ObservedEntry>,
    cursor: *const AtomicUsize,
    memory_failures: *const AtomicUsize,
    add: unsafe extern "C" fn(*const AtomicUsize, usize) -> usize,
}

#[repr(C)]
pub(super) struct RangeCompletion {
    pub(super) error: u32,
    pub(super) joined: u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.work::open_work"]
    fn open_work(entry: WorkEntry, context: *mut c_void) -> CreatedWork;
    #[link_name = "blink.windows.work::submit_work"]
    fn submit_work(work: *mut c_void);
    #[link_name = "blink.windows.work::complete_work"]
    fn complete_work(work: *mut c_void);
    #[link_name = "blink.windows.work::plan_ranges"]
    fn plan_ranges(count: u32, workers: u32, width: u32) -> RangePlan;
    #[link_name = "blink.windows.work::run_ranges"]
    fn run_ranges(input: RangeInput) -> RangeCompletion;
}

pub(crate) struct WorkHandle(*mut c_void);

impl WorkHandle {
    /// The callback and context remain live until the B owner joins the native work.
    pub(crate) unsafe fn new(entry: WorkEntry, context: *mut c_void) -> io::Result<Self> {
        let created = unsafe { open_work(entry, context) };
        if created.handle.is_null() {
            Err(io::Error::from_raw_os_error(created.error as i32))
        } else {
            Ok(Self(created.handle))
        }
    }

    pub(crate) unsafe fn submit(&self) {
        unsafe { submit_work(self.0) };
    }
}

impl Drop for WorkHandle {
    fn drop(&mut self) {
        unsafe { complete_work(self.0) };
    }
}

pub(super) fn plan(count: u32, workers: usize, width: u32) -> RangePlan {
    // The OS reports a positive count of logical processors within its u32 ABI.
    unsafe { plan_ranges(count, workers as u32, width) }
}

unsafe extern "C" fn add(counter: *const AtomicUsize, amount: usize) -> usize {
    // The C interface carries a raw pointer to the single initialized atomic word.
    unsafe { &*counter }.fetch_add(amount, Ordering::Relaxed)
}

unsafe extern "C" fn observed(
    context: *const c_void,
    buffers: *const super::BufferAddress,
    uniforms: *mut u32,
    end: u32,
    begin: u32,
    output: *mut u32,
    range: usize,
) -> *mut c_void {
    let work = unsafe { &*context.cast::<RangeWork<'_>>() };
    let timing = work.timing.map(|first| unsafe { first.add(range) });
    unsafe { work.invoke(work.entry, buffers, uniforms, end, begin, output, timing) }
}

pub(super) unsafe fn run(
    work: &mut RangeWork<'_>, errors: *mut *mut c_void, timing: Option<*mut CpuRangeTiming>,
) -> RangeCompletion {
    let cursor = AtomicUsize::new(0);
    let memory_failures = AtomicUsize::new(0);
    work.timing = timing;
    let observed_entry = (work.frame.is_some() || timing.is_some()).then_some(observed as ObservedEntry);
    let completion = unsafe {
        run_ranges(RangeInput {
            plan: work.plan,
            entry: mem::transmute::<*mut c_void, RangeEntry>(work.entry),
            buffers: work.buffers,
            parameter_slot: work.range_parameter_slot,
            uniforms: work.uniforms,
            frame_words: work.frame.map_or(0, |(_, _, words, _, _)| words),
            partials: work.partials,
            result_words: work.result.words(),
            errors,
            parameters: work.range_parameters,
            parameter_words: work.range_parameter_words,
            context: ptr::from_ref(work).cast(),
            observed_entry,
            cursor: &cursor,
            memory_failures: &memory_failures,
            add,
        })
    };
    work.timing = None;
    completion
}
