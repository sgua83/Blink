use blanguage::ir::{CollectiveOperator, Scalar, Ty};
use std::{ffi::c_void, mem::MaybeUninit};

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct SummaryRows {
    data: *mut c_void,
    count: usize,
    stride: usize,
}

#[repr(C)]
struct SummaryColumn {
    offset: u64,
    width: u32,
    signed: u32,
    floating: u32,
    operation: CollectiveOperator,
}

impl SummaryColumn {
    fn new(scalar: Scalar, offset: u32, operation: CollectiveOperator) -> Self {
        Self {
            offset: u64::from(offset),
            width: scalar.bit_width(),
            signed: u32::from(scalar.is_signed()),
            floating: u32::from(scalar.is_float()),
            operation,
        }
    }
}

unsafe extern "C" {
    #[link_name = "blink.windows.cpu_collective::reduce_column"]
    fn reduce_column(rows: SummaryRows, column: SummaryColumn, output: *mut u8);
    #[link_name = "blink.windows.cpu_collective::scan_column"]
    fn scan_column(rows: SummaryRows, column: SummaryColumn, head: u64, segmented: u32);
    #[link_name = "blink.windows.cpu_collective::segment_counts"]
    fn segment_counts(rows: SummaryRows, metadata: u64, count: u32, output: *mut u8);
    #[link_name = "blink.windows.cpu_collective::compact_prefix"]
    pub(super) fn compact_prefix(rows: SummaryRows, output: *mut u8);
}

impl SummaryRows {
    pub(super) fn new(rows: &mut [MaybeUninit<u32>], count: usize, words: usize) -> Self {
        Self { data: rows.as_mut_ptr().cast(), count, stride: words * size_of::<u32>() }
    }

    // The caller holds the summary allocation and supplies completed, matching native rows.
    pub(super) unsafe fn reduce(self, ty: &Ty, operation: CollectiveOperator, output: *mut u8) {
        for &(scalar, offset) in &ty.components {
            unsafe { reduce_column(self, SummaryColumn::new(scalar, offset, operation), output) };
        }
    }

    pub(super) unsafe fn scan(self, ty: &Ty, operation: CollectiveOperator, segmented: bool) {
        let head = summary_value_bytes(ty);
        for &(scalar, offset) in &ty.components {
            unsafe { scan_column(self, SummaryColumn::new(scalar, offset, operation), head, u32::from(segmented)) };
        }
    }

    pub(super) unsafe fn segmented_reduce(
        self, ty: &Ty, operation: CollectiveOperator, count: u32, output: *mut u8,
    ) {
        unsafe {
            self.scan(ty, operation, true);
            segment_counts(self, summary_value_bytes(ty), count, output);
        }
    }
}

fn summary_value_bytes(ty: &Ty) -> u64 {
    u64::from(ty.size.div_ceil(4)) * 4
}
