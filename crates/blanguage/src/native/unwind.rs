//! Windows x64序言记录及B展开信息编码的借用交接。

#[repr(u16)]
#[derive(Clone, Copy)]
enum Kind { Push, Allocate, SaveGpr, SaveXmm }

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Operation {
    offset: u8,
    reg: u8,
    kind: Kind,
    amount: u32,
}

impl Operation {
    pub(crate) fn push(offset: u8, reg: u8) -> Self {
        Self { offset, reg, kind: Kind::Push, amount: 0 }
    }
    pub(crate) fn allocate(offset: u8, frame: u32) -> Self {
        Self { offset, reg: 0, kind: Kind::Allocate, amount: frame }
    }
    pub(super) fn save(offset: u8, reg: u8, amount: u32, xmm: bool) -> Self {
        let kind = if xmm { Kind::SaveXmm } else { Kind::SaveGpr };
        Self { offset, reg, kind, amount }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.unwind::unwind_size"]
    fn unwind_size(operations: *const Operation, count: u64) -> u64;
    #[link_name = "bootstrap.unwind::unwind_write"]
    fn unwind_write(operations: *const Operation, count: u64, prolog: u8, output: *mut u8);
}

pub(crate) fn encode(prolog: u8, operations: &[Operation]) -> Vec<u8> {
    // Callers record operations as the prologue instructions are emitted.
    // Both B calls borrow the same rows. The writer initializes the complete record.
    let count = operations.len() as u64;
    let length = unsafe { unwind_size(operations.as_ptr(), count) } as usize;
    let mut bytes = Vec::with_capacity(length);
    unsafe {
        unwind_write(operations.as_ptr(), count, prolog, bytes.as_mut_ptr());
        bytes.set_len(length);
    }
    bytes
}
