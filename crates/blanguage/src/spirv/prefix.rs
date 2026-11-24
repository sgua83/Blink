//! 有效前缀的 GPU 间接参数与多维工作组索引；实际数量始终驻留设备。
use super::*;
use crate::spirv::{DISPATCH_CONTINUATION_SPEC_ID, DISPATCH_RECORD_BYTES, SCAN_WIDTH, sort_histogram_count};

#[repr(u32)]
#[allow(dead_code)] // Returned by bootstrap.spirv_envelope.
enum DispatchCapacity { None, Fixed, EntryCount, ReadDomain }

#[repr(C)]
struct DispatchArgumentPlan {
    capacity: DispatchCapacity,
    collective: u32,
    sorting: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::plan_dispatch_arguments"]
    fn plan_dispatch_arguments(
        operation: EntryOperation, selected: u32, prefix: u32,
        fixed: u32, scan_count: u32, scan_width: u32,
    ) -> DispatchArgumentPlan;
}

impl Emitter<'_> {
    fn dispatch_push(&self, zero: u32) -> DispatchPush {
        DispatchPush {
            uint_type: self.types[&Scalar::U32],
            pointer_type: self.ptr_push_u32,
            block: self.push,
            zero,
        }
    }

    fn prefix_push(&mut self, word: u32) -> u32 {
        let zero = self.constant(Scalar::U32, 0);
        let word = self.constant(Scalar::U32, word);
        let push = self.dispatch_push(zero);
        let at = self.body.len();
        self.body.reserve(10);
        let emitted = unsafe {
            let emitted = read_dispatch_word(push, word, self.next, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        emitted.value
    }

    pub(super) fn linear_index(&mut self, local: u32) -> (u32, u32, [u32; 2]) {
        let scalars = self.dispatch_scalars();
        let push = self.dispatch_push(scalars.zero);
        let at = self.body.len();
        self.body.reserve(46);
        let emitted = unsafe {
            let emitted = encode_linear_index(
                push, self.u64_type, scalars.bool_type, local, scalars.one, self.next,
                self.body.as_mut_ptr().add(at),
            );
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.index = emitted.index;
        (emitted.count, emitted.active, [0; 2])
    }
    pub(super) fn dispatch_scalars(&mut self) -> DispatchScalars {
        DispatchScalars {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            zero: self.constant(Scalar::U32, 0),
            one: self.constant(Scalar::U32, 1),
        }
    }

    pub(super) fn prefix_arguments(&mut self, prefix: Option<&PrefixCount>, invocation: u32) -> bool {
        let domain = self.stage.domain;
        let operation = EntryOperation::from(&self.stage.operation);
        let capacity = self.program.buffers[domain].count;
        let fixed = capacity.unwrap_or(0);
        let scan_count = match (&operation, capacity) {
            (EntryOperation::Sort, Some(count)) => sort_histogram_count(count),
            _ => fixed,
        };
        let plan = unsafe {
            plan_dispatch_arguments(
                operation, u32::from(self.stage.selection.is_some()), u32::from(prefix.is_some()),
                u32::from(capacity.is_some()), scan_count, SCAN_WIDTH,
            )
        };
        let count_word = 2 + domain as u32;
        let zero = self.constant(Scalar::U32, 0);
        let first = self.id();
        let run = self.id();
        let end = self.id();
        self.open_index_branch(first, invocation, zero, false, run, end);
        let count = match prefix {
            Some(prefix) => {
                let source = self.address_scalar(Scalar::U32, prefix.binding, zero, prefix.word);
                self.load_aligned_word(source)
            }
            None => self.prefix_push(count_word),
        };
        let max_x = self.prefix_push(0);
        let max_y = self.prefix_push(1);
        let capacity = match plan.capacity {
            DispatchCapacity::None => 0,
            DispatchCapacity::Fixed => self.constant(Scalar::U32, fixed),
            DispatchCapacity::EntryCount => count,
            DispatchCapacity::ReadDomain => self.prefix_push(count_word),
        };
        let collective = plan.collective != 0;
        let input = self.dispatch_grid_input(
            count, [max_x, max_y], capacity, collective, plan.sorting != 0, 0,
        );
        let at = self.body.len();
        self.body.reserve(input.word_capacity());
        let emitted = unsafe {
            let emitted = encode_dispatch_grid(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.merge_active(end, true);
        collective
    }

    pub(super) fn continuation_byte(&mut self) -> u32 {
        let id = self.id();
        let legacy = (PREFIX_LEVELS * DISPATCH_RECORD_BYTES) as u32;
        let defs = self.defs.len();
        let decorations = self.decorations.len();
        self.defs.reserve(5);
        self.decorations.reserve(4);
        unsafe {
            declare_dispatch_continuation(self.u64_type, id, DISPATCH_CONTINUATION_SPEC_ID,
                legacy, self.defs.as_mut_ptr().add(defs), self.decorations.as_mut_ptr().add(decorations));
            self.defs.set_len(defs + 5);
            self.decorations.set_len(decorations + 4);
        }
        id
    }

    pub(super) fn dispatch_grid_input(
        &mut self,
        count: u32,
        limits: [u32; 2],
        capacity: u32,
        collective: bool,
        sorting: bool,
        first_byte: u32,
    ) -> DispatchGrid {
        DispatchGrid {
            scalars: self.dispatch_scalars(),
            wide_type: self.u64_type,
            table_pointer: self.ptr_table_u64,
            page_pointer: self.physical_pointers[&Scalar::U64],
            word_pointer: self.physical_pointers[&Scalar::U32],
            descriptor: self.buffers[self.execution_buffers.indirect],
            count,
            width: self.constant(Scalar::U32, self.workgroup_width),
            max_x: limits[0],
            max_y: limits[1],
            capacity,
            collective: u32::from(collective),
            bins: if sorting { self.constant(Scalar::U32, 16) } else { 0 },
            first_byte,
            histogram_byte: if sorting { self.continuation_byte() } else { 0 },
            width_shift: self.constant(Scalar::U32, SCAN_WIDTH.trailing_zeros()),
            record_bytes: self.constant(Scalar::U64, DISPATCH_RECORD_BYTES as u32),
            field_bytes: [4, 8, 12].map(|byte| self.constant(Scalar::U64, byte)),
            next: self.next,
        }
    }

    pub(super) fn prefix_index(
        &mut self,
        global: u32,
        global_x: u32,
        num_groups: u32,
        vector_type: u32,
        collective: bool,
    ) -> (u32, u32, [u32; 2]) {
        let three = self.constant(Scalar::U32, 3);
        let zero = self.constant(Scalar::U32, 0);
        let y_limit = self.constant(Scalar::U32, 1);
        let push = self.dispatch_push(zero);
        let at = self.body.len();
        self.body.reserve(15);
        let index = unsafe {
            let index = dispatch_count_index(push, y_limit, three, self.next, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + index.words as usize);
            index
        };
        self.next = index.next;
        let word = index.value;
        let count_pointer = self.address_scalar(Scalar::U32, self.execution_buffers.indirect, word, 0);
        let input = PrefixIndexEmission {
            uint_type: self.types[&Scalar::U32],
            wide_type: self.u64_type,
            bool_type: self.types[&Scalar::Bool],
            vector_type,
            global,
            global_x,
            num_groups,
            count_pointer,
            width: if collective { self.constant(Scalar::U32, self.workgroup_width) } else { 0 },
            wide_width: self.constant(Scalar::U64, self.workgroup_width),
            zero: self.constant(Scalar::U32, 0),
            one: self.constant(Scalar::U32, 1),
            collective: u32::from(collective),
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(if collective { 134 } else { 88 });
        let emitted = unsafe {
            let emitted = encode_prefix_index(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.index = emitted.index;
        if collective {
            self.current_label = emitted.label;
        }
        (emitted.count, emitted.active, [emitted.groups_x, emitted.groups_y])
    }
}
