//! 有效前缀的 GPU 间接参数与多维工作组索引；实际数量始终驻留设备。
use super::*;
use crate::spirv::{DISPATCH_CONTINUATION_SPEC_ID, DISPATCH_RECORD_BYTES, SCAN_WIDTH, sort_histogram_count};
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

    pub(super) fn prefix_arguments(&mut self, prefix: Option<&PrefixCount>, invocation: u32) {
        let zero = self.constant(Scalar::U32, 0);
        let first = self.id();
        self.emit(170, &[self.types[&Scalar::Bool], first, invocation, zero]);
        let run = self.id();
        let end = self.id();
        self.open_active(first, run, end);
        let count = match prefix {
            Some(prefix) => {
                let source = self.address_scalar(Scalar::U32, prefix.binding, zero, prefix.word);
                let count = self.id();
                self.emit(61, &[self.types[&Scalar::U32], count, source, 2, 4]);
                count
            }
            None => self.prefix_push(2 + self.stage.domain as u32),
        };
        let max_x = self.prefix_push(0);
        let max_y = self.prefix_push(1);
        let collective =
            !matches!(self.stage.operation, Operation::Map) || self.stage.selection.is_some();
        let sorting = matches!(self.stage.operation, Operation::StableSort(_));
        let buffer = &self.program.buffers[self.stage.domain];
        let capacity = if collective && buffer.count.is_none_or(|capacity| {
            let elements = if sorting { sort_histogram_count(capacity) } else { capacity };
            elements > SCAN_WIDTH
        }) {
            match buffer.count {
                Some(capacity) => self.constant(Scalar::U32, capacity),
                None => self.prefix_push(2 + self.stage.domain as u32),
            }
        } else {
            0
        };
        let input = self.dispatch_grid_input(count, [max_x, max_y], capacity, collective, sorting, 0);
        let at = self.body.len();
        self.body.reserve(input.word_capacity());
        let emitted = unsafe {
            let emitted = encode_dispatch_grid(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.merge_active(end, true);
    }

    pub(super) fn continuation_byte(&mut self) -> u32 {
        let id = self.id();
        op(&mut self.decorations, 71, &[id, 1, DISPATCH_CONTINUATION_SPEC_ID]); // SpecId
        let legacy = (PREFIX_LEVELS * DISPATCH_RECORD_BYTES) as u32;
        op(&mut self.defs, 50, &[self.u64_type, id, legacy, 0]); // OpSpecConstant
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
            descriptor: self.buffers[self.program.indirect_buffer()],
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
        let base = self.prefix_push(1);
        let word = self.id();
        self.emit(128, &[self.types[&Scalar::U32], word, base, three]);
        let count_pointer = self.address_scalar(Scalar::U32, self.program.indirect_buffer(), word, 0);
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
