use crate::ir::{CollectiveOperator, TileOp, Ty};

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Form { Unknown, TileReduce, TileScan, Reduce, SegmentedReduce, Scan, SegmentedScan }

#[repr(C)]
pub(super) struct Builtin {
    pub(super) form: Form,
    pub(super) operation: CollectiveOperator,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::source_collective"]
    fn source_collective(bytes: *const u8, length: usize) -> Builtin;
    #[link_name = "bootstrap.operator_types::collective_accepts"]
    fn collective_accepts(operation: CollectiveOperator, numeric: u32, integer: u32) -> u32;
}

pub(super) fn from_source(name: &str) -> Builtin {
    unsafe { source_collective(name.as_ptr(), name.len()) }
}

impl Builtin {
    pub(super) fn is_tile(&self) -> bool {
        matches!(self.form, Form::TileReduce | Form::TileScan)
    }

    pub(super) fn is_stage(&self) -> bool {
        matches!(self.form, Form::Reduce | Form::SegmentedReduce | Form::Scan | Form::SegmentedScan)
    }

    pub(super) fn tile(self) -> Option<TileOp> {
        use CollectiveOperator::*;
        use Form::*;
        Some(match (self.form, self.operation) {
            (TileReduce, Sum) => TileOp::ReduceSum,
            (TileReduce, Min) => TileOp::ReduceMin,
            (TileReduce, Max) => TileOp::ReduceMax,
            (TileReduce, BitAnd) => TileOp::ReduceBitAnd,
            (TileReduce, BitOr) => TileOp::ReduceBitOr,
            (TileReduce, BitXor) => TileOp::ReduceBitXor,
            (TileScan, Sum) => TileOp::ScanExclusiveSum,
            (TileScan, Min) => TileOp::ScanExclusiveMin,
            (TileScan, Max) => TileOp::ScanExclusiveMax,
            (TileScan, BitAnd) => TileOp::ScanExclusiveBitAnd,
            (TileScan, BitOr) => TileOp::ScanExclusiveBitOr,
            (TileScan, BitXor) => TileOp::ScanExclusiveBitXor,
            _ => return None,
        })
    }
}

pub(super) fn accepts(operation: CollectiveOperator, ty: &Ty) -> bool {
    let numeric = ty.is_numeric();
    let integer = numeric && ty.components[0].0.is_integer();
    unsafe { collective_accepts(operation, u32::from(numeric), u32::from(integer)) != 0 }
}
