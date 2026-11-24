//! Lower checked atomics through the SPIR-V emitter's address and value storage.
use super::*;
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(u32)]
enum AtomicOperation {
    Load, Store, Add, Sub, Exchange, CompareExchange, Min, Max, And, Or, Xor,
}

#[repr(C)]
struct AtomicSource {
    operation: AtomicOperation,
    scalar: ScalarCode,
    unit: ScalarCode,
    result_type: u32,
    pointer: u32,
    workgroup: u32,
    order: u32,
    failure: u32,
    value: Reg,
    compare: Reg,
}

#[repr(C)]
struct AtomicWords {
    words: usize,
    result: u32,
}

#[repr(C)]
struct AtomicHost {
    constant: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    id: unsafe extern "C" fn(*mut c_void) -> u32,
    bits: unsafe extern "C" fn(*mut c_void, Reg) -> u32,
    body: unsafe extern "C" fn(*mut c_void) -> *mut u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_source_atomic"]
    fn emit_source_atomic(context: *mut c_void, source: AtomicSource, host: AtomicHost) -> AtomicWords;
}

unsafe extern "C" fn constant(context: *mut c_void, bits: u32) -> u32 {
    unsafe { &mut *context.cast::<Emitter<'_>>() }.constant(Scalar::U32, bits)
}

unsafe extern "C" fn id(context: *mut c_void) -> u32 {
    unsafe { &mut *context.cast::<Emitter<'_>>() }.id()
}

unsafe extern "C" fn bits(context: *mut c_void, value: Reg) -> u32 {
    unsafe { &mut *context.cast::<Emitter<'_>>() }.raw_bits(value)
}

unsafe extern "C" fn body(context: *mut c_void) -> *mut u32 {
    let body = &mut unsafe { &mut *context.cast::<Emitter<'_>>() }.body;
    body.reserve(9);
    unsafe { body.as_mut_ptr().add(body.len()) }
}

const HOST: AtomicHost = AtomicHost { constant, id, bits, body };

impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) -> Result<(), &'static str> {
        let unit = Self::raw_scalar(atomic.scalar);
        let (pointer, workgroup) = match atomic.target {
            IndexedSource::Data { binding, index } => {
                let offset = atomic.offset.map_or(0, |reg| self.value(reg));
                let pointer = self.page_address(
                    unit, PageSource::Dataset(self.buffers[binding]), self.value(index), atomic.word, offset,
                );
                (pointer, 0)
            }
            IndexedSource::Tile { tile, index } => (
                self.tile_word_address(
                    self.stage.shared_word(tile, atomic.word),
                    self.value(index),
                    atomic.offset.map(|reg| self.value(reg)),
                ).0,
                1,
            ),
            IndexedSource::View { .. } | IndexedSource::Pointer { .. } => return Err("GPU原子不能使用CPU原生地址".into()),
        };
        let (operation, value, compare, failure) = match atomic.op {
            AtomicOp::Load => (AtomicOperation::Load, 0, 0, 0),
            AtomicOp::Store(value) => (AtomicOperation::Store, value, 0, 0),
            AtomicOp::Add(value) => (AtomicOperation::Add, value, 0, 0),
            AtomicOp::Sub(value) => (AtomicOperation::Sub, value, 0, 0),
            AtomicOp::Exchange(value) => (AtomicOperation::Exchange, value, 0, 0),
            AtomicOp::CompareExchange { expected, desired, failure } => (
                AtomicOperation::CompareExchange, desired, expected, failure as u32,
            ),
            AtomicOp::Min(value) => (AtomicOperation::Min, value, 0, 0),
            AtomicOp::Max(value) => (AtomicOperation::Max, value, 0, 0),
            AtomicOp::And(value) => (AtomicOperation::And, value, 0, 0),
            AtomicOp::Or(value) => (AtomicOperation::Or, value, 0, 0),
            AtomicOp::Xor(value) => (AtomicOperation::Xor, value, 0, 0),
        };
        let source = AtomicSource {
            operation, scalar: atomic.scalar.into(), unit: unit.into(), result_type: self.types[&unit],
            pointer, workgroup, order: atomic.order as u32, failure, value, compare,
        };
        // B asks for the final body pointer after raw operand materialization is finished.
        let emitted = unsafe { emit_source_atomic(std::ptr::from_mut(&mut *self).cast(), source, HOST) };
        unsafe { self.body.set_len(self.body.len() + emitted.words) };
        if let Some(dst) = atomic.result {
            self.from_words(dst, &[emitted.result], unit);
        }
        Ok(())
    }
}
