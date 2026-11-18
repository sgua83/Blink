//! Physical typed construction for the fixed B execution-plan format.
use crate::codec::{
    ReadStatus,
    contracts::{self, Native, Read, Slot, Write},
};
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::codec::contracts::projected_record;

macro_rules! projected_enum {
    ($ty:ty,$empty:ident,$read:literal,{$($variant:ident=>($field:ident,$write:literal)),+$(,)?}) => {
        const _: () = {
            #[repr(C)] struct Reader { $($field: unsafe extern "C" fn(Slot),)+ }
            $(unsafe extern "C" fn $field(output: Slot) { unsafe { *output.cast::<$ty>() = <$ty>::$variant; } })+
            unsafe extern "C" {
                #[link_name = $read] fn read(input: contracts::Bytes, position: *mut usize, output: Slot, host: Reader) -> ReadStatus;
            }
            impl Native for $ty {
                fn empty() -> Self { Self::$empty }
                unsafe fn write(&self, output: Slot) {
                    match self { $(Self::$variant => {
                        unsafe extern "C" {
                            #[link_name = $write] fn write(output: Slot, append: unsafe extern "C" fn(Slot,*const u8,usize));
                        }
                        unsafe { write(output, contracts::append); }
                    },)+ }
                }
                unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
                    let (bytes,position) = unsafe { contracts::input(context) };
                    unsafe { read(bytes,position,contracts::slot(output),Reader { $($field,)+ }) }
                }
            }
        };
        contracts::codec!($ty);
    };
}

projected_record!(Reduction,"bootstrap.contract_codec::resource_write_reduction","bootstrap.contract_codec::resource_read_reduction", {
    source: usize,
    source_word: u32,
    target: usize,
    target_word: u32,
});

projected_record!(ReductionSpec,"bootstrap.contract_codec::resource_write_reduction_spec","bootstrap.contract_codec::resource_read_reduction_spec", {
    fields: Reduction,
    ty: Ty,
    operator: CollectiveOperator,
});

projected_record!(SegmentedReduction,"bootstrap.contract_codec::resource_write_segmented_reduction","bootstrap.contract_codec::resource_read_segmented_reduction", {
    values: ReductionSpec,
    head_word: u32,
    count: PrefixCount,
});

projected_record!(Scan,"bootstrap.contract_codec::resource_write_scan","bootstrap.contract_codec::resource_read_scan", {
    values: Reduction,
    ty: Ty,
    operator: CollectiveOperator,
    head_word: Option<u32>,
});

projected_record!(Compaction,"bootstrap.contract_codec::resource_write_compaction","bootstrap.contract_codec::resource_read_compaction", {
    source: usize,
    flags: usize,
    flag_word: u32,
    target: usize,
    count: usize,
    count_word: u32,
});

projected_record!(Sorting,"bootstrap.contract_codec::resource_write_sorting","bootstrap.contract_codec::resource_read_sorting", {
    source: usize,
    keys: Vec<(u32, Scalar, SortOrder)>,
    target: usize,
});

projected_record!(ReductionLayout,"bootstrap.contract_codec::resource_write_reduction_layout","bootstrap.contract_codec::resource_read_reduction_layout", {
    fields: Reduction,
    ty: Ty,
});

projected_record!(CustomScan<ReductionLayout>,"bootstrap.contract_codec::resource_write_custom_scan","bootstrap.contract_codec::resource_read_custom_scan", {
    values: ReductionLayout,
    head_word: Option<u32>,
});

projected_record!(CustomSegmentedReduction<ReductionLayout>,"bootstrap.contract_codec::resource_write_custom_segmented_reduction","bootstrap.contract_codec::resource_read_custom_segmented_reduction", {
    values: ReductionLayout,
    head_word: u32,
    count: PrefixCount,
});

projected_record!(Selection,"bootstrap.contract_codec::resource_write_selection","bootstrap.contract_codec::resource_read_selection", {
    binding: usize,
    word: u32,
});

projected_record!(ExecutionStage,"bootstrap.contract_codec::resource_write_execution_stage","bootstrap.contract_codec::resource_read_execution_stage", {
    domain: usize,
    prefix: Option<PrefixCount>,
    selection: Option<Selection>,
    operation: Operation<ReductionLayout>,
    reads: Vec<usize>,
    writes: Vec<usize>,
    write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    prefix_guarantees: Vec<PrefixRelation>,
    element_guarantees: Vec<ElementFact>,
    inputs: BTreeSet<(usize, u32)>,
    tile_scope: bool,
    width: u32,
    shared_bytes: u64,
});

projected_record!(ExecutionPlan,"bootstrap.contract_codec::resource_write_execution_plan","bootstrap.contract_codec::resource_read_execution_plan", {
    interface: ProgramLayout,
    stages: Vec<ExecutionStage>,
    indirect_buffer: usize,
    auxiliary_buffers: usize,
    native_frame: Option<u32>,
});

projected_enum!(SortOrder,Ascending,"bootstrap.contract_codec::resource_read_sort_order",{
    Ascending=>(ascending,"bootstrap.contract_codec::resource_write_sort_order_ascending"),
    Descending=>(descending,"bootstrap.contract_codec::resource_write_sort_order_descending"),
});

projected_enum!(CollectiveOperator,Sum,"bootstrap.contract_codec::resource_read_collective_operator",{
    Sum=>(sum,"bootstrap.contract_codec::resource_write_collective_operator_sum"),
    Min=>(min,"bootstrap.contract_codec::resource_write_collective_operator_min"),
    Max=>(max,"bootstrap.contract_codec::resource_write_collective_operator_max"),
    BitAnd=>(bit_and,"bootstrap.contract_codec::resource_write_collective_operator_bit_and"),
    BitOr=>(bit_or,"bootstrap.contract_codec::resource_write_collective_operator_bit_or"),
    BitXor=>(bit_xor,"bootstrap.contract_codec::resource_write_collective_operator_bit_xor"),
});

contracts::codec!(Operation<ReductionLayout>);
const _: () = {
    type PlanOperation = Operation<ReductionLayout>;
    #[repr(C)]
    struct Reader {
        map: Read,
        reduce: Read,
        custom_reduce: Read,
        custom_scan: Read,
        custom_segmented_reduce: Read,
        segmented_reduce: Read,
        scan_exclusive: Read,
        compact: Read,
        stable_sort: Read,
    }
    unsafe extern "C" fn map(_context: Slot, output: Slot) -> ReadStatus {
        unsafe {
            *output.cast::<PlanOperation>() = Operation::Map;
        }
        ReadStatus::Success
    }
    macro_rules! construct {
        ($name:ident,$variant:ident,$payload:ty) => {
            unsafe extern "C" fn $name(context: Slot, output: Slot) -> ReadStatus {
                let mut value = <$payload as Native>::empty();
                let status = unsafe { <$payload as Native>::read(context, &mut value) };
                if status == ReadStatus::Success {
                    unsafe {
                        *output.cast::<PlanOperation>() = Operation::$variant(value);
                    }
                }
                status
            }
        };
    }
    construct!(reduce, Reduce, ReductionSpec);
    construct!(custom_reduce, CustomReduce, ReductionLayout);
    construct!(custom_scan, CustomScan, CustomScan<ReductionLayout>);
    construct!(
        custom_segmented_reduce,
        CustomSegmentedReduce,
        CustomSegmentedReduction<ReductionLayout>
    );
    construct!(segmented_reduce, SegmentedReduce, SegmentedReduction);
    construct!(scan_exclusive, ScanExclusive, Scan);
    construct!(compact, Compact, Compaction);
    construct!(stable_sort, StableSort, Sorting);
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::resource_read_operation"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for PlanOperation {
        fn empty() -> Self {
            Self::Map
        }
        unsafe fn write(&self, output: Slot) {
            match self {
                Self::Map => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_map"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(output, contracts::append);
                    }
                }
                Self::Reduce(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_reduce"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<ReductionSpec>,
                            contracts::append,
                        );
                    }
                }
                Self::CustomReduce(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_custom_reduce"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<ReductionLayout>,
                            contracts::append,
                        );
                    }
                }
                Self::CustomScan(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_custom_scan"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<CustomScan<ReductionLayout>>,
                            contracts::append,
                        );
                    }
                }
                Self::CustomSegmentedReduce(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_custom_segmented_reduce"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<CustomSegmentedReduction<ReductionLayout>>,
                            contracts::append,
                        );
                    }
                }
                Self::SegmentedReduce(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_segmented_reduce"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<SegmentedReduction>,
                            contracts::append,
                        );
                    }
                }
                Self::ScanExclusive(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_scan_exclusive"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<Scan>,
                            contracts::append,
                        );
                    }
                }
                Self::Compact(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_compact"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<Compaction>,
                            contracts::append,
                        );
                    }
                }
                Self::StableSort(value) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::resource_write_operation_stable_sort"]
                        fn write(
                            source: contracts::Address,
                            output: Slot,
                            payload: Write,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            contracts::address(value),
                            output,
                            contracts::write_value::<Sorting>,
                            contracts::append,
                        );
                    }
                }
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(output),
                    Reader {
                        map,
                        reduce,
                        custom_reduce,
                        custom_scan,
                        custom_segmented_reduce,
                        segmented_reduce,
                        scan_exclusive,
                        compact,
                        stable_sort,
                    },
                )
            }
        }
    }
};
