//! 有效前缀的 GPU 间接参数与多维工作组索引；实际数量始终驻留设备。
use super::*;
impl Emitter<'_> {
    fn prefix_push(&mut self, word: u32) -> u32 {
        let zero = self.constant(Scalar::U32, 0);
        let word = self.constant(Scalar::U32, word);
        let at = self.id();
        self.emit(65, &[self.ptr_push_u32, at, self.push, zero, word]);
        let value = self.id();
        self.emit(61, &[self.types[&Scalar::U32], value, at]);
        value
    }
    fn dispatch_scalars(&mut self) -> DispatchScalars {
        DispatchScalars {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            zero: self.constant(Scalar::U32, 0),
            one: self.constant(Scalar::U32, 1),
        }
    }

    pub(super) fn dispatch_groups(&mut self, count: u32, width: u32) -> u32 {
        let input = DispatchGroups {
            scalars: self.dispatch_scalars(),
            count,
            width,
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(37);
        let emitted = unsafe {
            let emitted = encode_dispatch_groups(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        emitted.value
    }

    pub(super) fn prefix_arguments(&mut self, prefix: Option<&PrefixCount>, invocation: u32) {
        let zero = self.constant(Scalar::U32, 0);
        let first = self.id();
        self.emit(170, &[self.types[&Scalar::Bool], first, invocation, zero]);
        let run = self.id();
        let end = self.id();
        self.emit_control(9, |body| unsafe { open_selection(end, first, run, end, body) });
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
        let levels = if collective { PREFIX_LEVELS } else { 1 };
        let sorting = matches!(self.stage.operation, Operation::StableSort(_));
        self.dispatch_grid(count, [max_x, max_y], 0, levels, collective, sorting);
        self.emit_control(4, |body| unsafe { next_control_block(end, end, 1, body) });
    }

    pub(super) fn dispatch_grid(
        &mut self,
        count: u32,
        limits: [u32; 2],
        first: usize,
        levels: usize,
        collective: bool,
        sorting: bool,
    ) {
        let records = levels * (1 + usize::from(sorting));
        let mut offsets = [0; PREFIX_LEVELS * 2 * 4];
        for (slot, offset) in offsets[..records * 4].iter_mut().enumerate() {
            let byte = ((first * 4 + slot) * 4) as u32;
            if byte != 0 {
                *offset = self.constant(Scalar::U64, byte);
            }
        }
        let input = DispatchGrid {
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
            levels: levels as u32,
            collective: u32::from(collective),
            bins: if sorting { self.constant(Scalar::U32, 16) } else { 0 },
            next: self.next,
        };
        let at = self.body.len();
        // Page header, optional histogram count and up to 167 words per record.
        self.body.reserve(25 + 167 * records);
        let emitted = unsafe {
            let emitted = encode_dispatch_grid(input, offsets.as_ptr(), self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
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
