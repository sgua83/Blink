//! x64 原子作用于原数据地址；读改写指令返回旧值，不经过宿主锁或回调。
use super::{allocation::Location, operands::MoveLocation, *};
use crate::builtin_types::ScalarEncoding;

#[repr(u32)]
enum AtomicOperation {
    Load, Store, Add, Sub, Exchange, CompareExchange, And, Or, Xor, Min, Max,
}

#[repr(u32)]
enum AtomicStorage {
    Shared, Private, Split,
}

#[repr(C)]
struct AtomicSequence {
    low: MoveLocation,
    high: MoveLocation,
    first: MoveLocation,
    second: MoveLocation,
    scalar: ScalarEncoding,
    operation: AtomicOperation,
    storage: AtomicStorage,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::atomic_sequence"]
    fn atomic_sequence(input: AtomicSequence, output: *mut u8) -> u64;
}

impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let at = if let Some(offset) = atomic.offset {
            self.indexed_address(&atomic.target, offset, atomic.word);
            Memory::new(15, 0)
        } else {
            match atomic.target {
                IndexedSource::Data { binding, index } => {
                    self.address(binding, index);
                    Memory::new(15, atomic.word)
                }
                IndexedSource::Tile { tile, index } => {
                    self.tile_index_base(index, 0);
                    Memory::new(15, self.shared_stream(self.stage.shared_word(tile, atomic.word)))
                }
                IndexedSource::View { .. } | IndexedSource::Pointer { .. } => {
                    self.indexed_base(&atomic.target);
                    Memory::new(15, atomic.word)
                }
            }
        };
        let (storage, high) = match atomic.target {
            IndexedSource::Tile { tile, .. } if atomic.scalar.size() == 8 => {
                let low_stream = self.shared_stream(self.stage.shared_word(tile, atomic.word));
                let high_stream = self.shared_stream(self.stage.shared_word(tile, atomic.word + 4));
                let delta = (i64::from(high_stream) - i64::from(low_stream)) as u32;
                (AtomicStorage::Split, at.offset(delta))
            }
            IndexedSource::Tile { .. } => (AtomicStorage::Private, at),
            _ => (AtomicStorage::Shared, at),
        };
        let (operation, first, second) = match atomic.op {
            AtomicOp::Load => (AtomicOperation::Load, None, None),
            AtomicOp::Store(value) => (AtomicOperation::Store, Some(value), None),
            AtomicOp::Add(value) => (AtomicOperation::Add, Some(value), None),
            AtomicOp::Sub(value) => (AtomicOperation::Sub, Some(value), None),
            AtomicOp::Exchange(value) => (AtomicOperation::Exchange, Some(value), None),
            AtomicOp::CompareExchange { expected, desired, .. } => {
                (AtomicOperation::CompareExchange, Some(expected), Some(desired))
            }
            AtomicOp::And(value) => (AtomicOperation::And, Some(value), None),
            AtomicOp::Or(value) => (AtomicOperation::Or, Some(value), None),
            AtomicOp::Xor(value) => (AtomicOperation::Xor, Some(value), None),
            AtomicOp::Min(value) => (AtomicOperation::Min, Some(value), None),
            AtomicOp::Max(value) => (AtomicOperation::Max, Some(value), None),
        };
        let input = AtomicSequence {
            low: at.into(),
            high: high.into(),
            first: first.map_or(Location::Immediate(0), |value| self.location(value)).into(),
            second: second.map_or(Location::Immediate(0), |value| self.location(value)).into(),
            scalar: atomic.scalar.into(),
            operation,
            storage,
        };
        self.emit_bytes(128, |output| unsafe { atomic_sequence(input, output) });
        if let Some(result) = atomic.result {
            self.store(result);
        }
    }
}
