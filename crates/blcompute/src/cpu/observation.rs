use crate::Result;
use std::ffi::c_void;

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}

impl FileTime {
    fn ticks(&self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryPerformanceCounter(ticks: *mut i64) -> i32;
    fn QueryPerformanceFrequency(ticks_per_second: *mut i64) -> i32;
    fn GetCurrentThread() -> *mut c_void;
    fn GetCurrentThreadId() -> u32;
    fn GetThreadTimes(
        thread: *mut c_void,
        created: *mut FileTime,
        exited: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GetLastError() -> u32;
    fn SetLastError(error: u32);
}

#[derive(Clone, Copy, Debug)]
pub struct CpuThreadTime {
    pub kernel_100ns: u64,
    pub user_100ns: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct CpuTimePoint {
    pub qpc_ticks: i64,
    /// Cumulative worker CPU time or the original Windows query error.
    pub thread_time: std::result::Result<CpuThreadTime, u32>,
}

impl CpuThreadTime {
    fn read() -> std::result::Result<Self, u32> {
        let mut created = FileTime::default();
        let mut exited = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        unsafe {
            let status = GetThreadTimes(
                GetCurrentThread(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            );
            if status == 0 {
                Err(GetLastError())
            } else {
                Ok(Self { kernel_100ns: kernel.ticks(), user_100ns: user.ticks() })
            }
        }
    }
}

fn qpc_ticks() -> i64 {
    let mut ticks = 0;
    // A valid output on supported Windows always supplies QPC.
    unsafe { QueryPerformanceCounter(&mut ticks) };
    ticks
}

#[derive(Clone, Copy, Debug)]
pub struct CpuRangeTiming {
    pub begin: u32,
    pub end: u32,
    pub thread_id: u32,
    pub failed: bool,
    pub started: CpuTimePoint,
    pub finished: CpuTimePoint,
}

pub struct CpuRangeCall {
    /// The stage index in the original execution plan.
    pub stage: usize,
    /// The actual loaded entry at the time of the call.
    pub entry: usize,
    pub ranges: Vec<CpuRangeTiming>,
}

pub struct CpuRangeTimings {
    pub ticks_per_second: i64,
    pub calls: Vec<CpuRangeCall>,
}

impl CpuRangeTimings {
    pub fn new() -> Self {
        let mut ticks_per_second = 0;
        unsafe {
            let previous = GetLastError();
            QueryPerformanceFrequency(&mut ticks_per_second);
            SetLastError(previous);
        }
        Self { ticks_per_second, calls: Vec::new() }
    }
}

/// A successful invocation writes all requested slots before returning.
pub(super) unsafe fn capture<T>(
    timings: &mut Option<&mut CpuRangeTimings>,
    stage: usize,
    entry: usize,
    count: usize,
    run: impl FnOnce(Option<*mut CpuRangeTiming>) -> Result<T>,
) -> Result<T> {
    let Some(timings) = timings.as_deref_mut().filter(|_| count != 0) else {
        return run(None);
    };
    timings.calls.try_reserve(1)?;
    let mut ranges = Vec::new();
    ranges.try_reserve_exact(count)?;
    let result = run(Some(ranges.as_mut_ptr()))?;
    unsafe { ranges.set_len(count) };
    timings.calls.push(CpuRangeCall { stage, entry, ranges });
    Ok(result)
}

/// An observed invocation owns its destination until the original caller joins.
pub(super) unsafe fn measure(
    timing: Option<*mut CpuRangeTiming>,
    begin: u32,
    end: u32,
    run: impl FnOnce() -> *mut c_void,
) -> *mut c_void {
    let Some(timing) = timing else {
        return run();
    };
    let previous = unsafe { GetLastError() };
    let thread_id = unsafe { GetCurrentThreadId() };
    let thread_time = CpuThreadTime::read();
    let started = CpuTimePoint { qpc_ticks: qpc_ticks(), thread_time };
    unsafe { SetLastError(previous) };

    let result = run();

    let previous = unsafe { GetLastError() };
    let finished = CpuTimePoint { qpc_ticks: qpc_ticks(), thread_time: CpuThreadTime::read() };
    unsafe {
        SetLastError(previous);
        timing.write(CpuRangeTiming { begin, end, thread_id, failed: !result.is_null(), started, finished });
    }
    result
}
