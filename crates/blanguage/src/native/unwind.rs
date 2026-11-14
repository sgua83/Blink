//! Windows x64序言记录及B展开信息编码的借用交接。

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Operation {
    offset: u8,
    reg: u8,
    kind: u16,
    amount: u32,
}

impl Operation {
    pub(crate) fn push(offset: u8, reg: u8) -> Self {
        Self { offset, reg, kind: 0, amount: 0 }
    }
    pub(crate) fn allocate(offset: u8, frame: u32) -> Self {
        Self { offset, reg: 0, kind: 1, amount: frame }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.unwind::unwind_write"]
    fn unwind_write(operations: *const Operation, count: u64, prolog: u8, output: *mut u8) -> u64;
}

pub(crate) fn encode(prolog: u8, operations: &[Operation]) -> Vec<u8> {
    // Four header bytes and up to three two-byte slots per operation.
    let capacity = 4 + (6 * operations.len()).next_multiple_of(4);
    let mut bytes = Vec::with_capacity(capacity);
    unsafe {
        let length = unwind_write(operations.as_ptr(), operations.len() as u64, prolog, bytes.as_mut_ptr());
        bytes.set_len(length as usize);
    }
    bytes
}
