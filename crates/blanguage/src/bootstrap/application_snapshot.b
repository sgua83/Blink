module bootstrap.application_snapshot;
import bootstrap.ir_dataflow;

address item_at(Span span, usize index) on cpu {
    return unsafe_address(address_bits(span.data) + index * span.stride);
}

View instruction(address item, Host ir) on cpu requires nonzero(ir.inspect) {
    return ir.inspect(Node(item, N_INSTRUCTION));
}

u8 register_scalar(Span registers, usize reg) on cpu {
    return unsafe_position<u8>(item_at(registers, reg));
}

bool has_candidate(Span code, Span registers, View read, Host ir) on cpu
    requires nonzero(ir.inspect) {
    usize index = 0u64;
    while (index < code.count) {
        View inst = instruction(item_at(code, index), ir);
        if (inst.tag == OP_BRANCH) {
            if (has_candidate(inst.first, registers, read, ir) ||
                has_candidate(inst.second, registers, read, ir)) { return true; }
        } else if (inst.tag == OP_SCOPE) {
            if (has_candidate(inst.first, registers, read, ir)) { return true; }
        } else if ((inst.tag == OP_LOAD && read.tag == OP_LOAD && inst.b == read.b && inst.d == read.d) ||
                   (inst.tag == OP_VIEW_LOAD && read.tag == OP_VIEW_LOAD && inst.c == read.c)) {
            if (register_scalar(registers, inst.a) == register_scalar(registers, read.a)) { return true; }
        }
        index += 1u64;
    }
    return false;
}

address preceding_local_write(SnapshotSource source, SnapshotHost host, usize before, usize slot) on cpu
    requires nonzero(host.block) requires nonzero(host.writes_local) {
    usize block = source.blocks;
    usize end = source.instructions;
    while (block != 0u64) {
        block -= 1u64;
        Span code = host.block(source.context, block);
        usize start = end - code.count;
        if (before > start) {
            usize index = min(code.count, before - start);
            while (index != 0u64) {
                index -= 1u64;
                address item = item_at(code, index);
                if (host.writes_local(source.context, item, slot) != 0) { return item; }
            }
        }
        end = start;
    }
    return address();
}

usize condition_source(SnapshotSource source, SnapshotHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.definition) requires nonzero(host.writes_local) {
    usize current = reg;
    while (true) {
        RegisterDefinition definition = host.definition(source.context, current);
        if (definition.instruction == address()) { return current; }
        View load = instruction(definition.instruction, host.ir);
        if (load.tag != OP_LOAD_LOCAL) { return current; }
        address write = preceding_local_write(source, host, definition.top, load.b);
        if (write == address()) { return current; }
        View store = instruction(write, host.ir);
        if (store.tag != OP_STORE_LOCAL) { return current; }
        current = store.b;
    }
}

struct SnapshotWalk { address read; u32 stopped; };

SnapshotWalk scan_block(SnapshotSource source, SnapshotHost host, Span code, address newest) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.definition) requires nonzero(host.writes_local)
    requires nonzero(host.truth) requires nonzero(host.writes_load) requires nonzero(host.same_load) {
    address read = newest;
    usize index = code.count;
    while (index != 0u64) {
        index -= 1u64;
        address item = item_at(code, index);
        View inst = instruction(item, host.ir);
        if (inst.tag == OP_SCOPE) {
            read = scan_block(source, host, inst.first, read).read;
        } else if (inst.tag == OP_BRANCH) {
            View loaded = instruction(read, host.ir);
            if (has_candidate(inst.first, source.registers, loaded, host.ir) ||
                has_candidate(inst.second, source.registers, loaded, host.ir)) {
                usize branch = condition_source(source, host, inst.a);
                SnapshotTruth truth = host.truth(source.context, branch);
                if (truth != SnapshotTruth::Unknown) {
                    Span selected = truth == SnapshotTruth::Yes ? inst.first : inst.second;
                    read = scan_block(source, host, selected, read).read;
                }
            }
        }
        if (host.writes_load(source.context, item, read) != 0) { return SnapshotWalk(read, 1); }
        if ((inst.tag == OP_LOAD || inst.tag == OP_VIEW_LOAD) &&
            host.same_load(source.context, item, read) != 0) { read = item; }
    }
    return SnapshotWalk(read, 0);
}

extern "C" usize read_snapshot(SnapshotSource source, SnapshotHost host, usize register) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.definition) requires nonzero(host.writes_local)
    requires nonzero(host.truth) requires nonzero(host.writes_load) requires nonzero(host.same_load) {
    RegisterDefinition definition = host.definition(source.context, register);
    if (definition.instruction == address()) { return register; }
    address read = definition.instruction;
    View first = instruction(read, host.ir);
    if (first.tag != OP_LOAD && first.tag != OP_VIEW_LOAD) { return register; }
    usize block = source.blocks;
    usize end = source.instructions;
    while (block != 0u64) {
        block -= 1u64;
        Span code = host.block(source.context, block);
        usize start = end - code.count;
        if (definition.top > start) {
            code.count = min(code.count, definition.top - start);
            SnapshotWalk result = scan_block(source, host, code, read);
            read = result.read;
            if (result.stopped != 0) { break; }
        }
        end = start;
    }
    return instruction(read, host.ir).a;
}

bool writes_slot(address item, usize slot, Host ir) on cpu requires nonzero(ir.inspect) {
    Locals written = locals(Node(item, N_INSTRUCTION), 1, ir);
    Word current = next_local(written, ir);
    while (current.present != 0) {
        if (current.value == slot) { return true; }
        current = next_local(written, ir);
    }
    return false;
}

bool reference_writes(Span arguments, WriteQuery query, WriteHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.overlaps) {
    usize index = 0u64;
    while (index < arguments.count) {
        View argument = host.ir.inspect(Node(item_at(arguments, index), N_ARGUMENT));
        index += 1u64;
        if (argument.tag != ARGUMENT_REFERENCE || !accessed(argument.a, 1)) { continue; }
        View reference = host.ir.inspect(argument.left);
        View storage = host.ir.inspect(reference.left);
        if (storage.tag != STORAGE_INDEXED) { continue; }
        usize part = 0u64;
        while (part < argument.second.count) {
            address component = item_at(argument.second, part);
            if (host.overlaps(query.context, WriteEffect(argument.left.data, component, WriteEffectKind::Reference)) != 0) {
                return true;
            }
            part += 1u64;
        }
    }
    return false;
}

extern "C" u32 application_instruction_writes(address item, WriteQuery query, WriteHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.overlaps) {
    View inst = instruction(item, host.ir);
    if (query.has_slot != 0 && writes_slot(item, query.slot, host.ir)) { return 1; }
    if (inst.tag == OP_BRANCH || inst.tag == OP_LOOP_VALUE) {
        if (application_block_writes(inst.first, query, host) != 0) { return 1; }
        return application_block_writes(inst.second, query, host);
    }
    if (inst.tag == OP_SCOPE || (inst.tag == OP_CLEANUP && query.kind == WriteQueryKind::Condition)) {
        return application_block_writes(inst.first, query, host);
    }
    if (inst.tag == OP_INVOKE) {
        if (query.kind == WriteQueryKind::Indexed &&
            reference_writes(host.ir.inspect(inst.left).first, query, host)) { return 1; }
        return host.overlaps(query.context, WriteEffect(inst.left.data, address(), WriteEffectKind::Call));
    }
    if (query.kind == WriteQueryKind::Local) { return 0; }
    if (inst.tag == OP_ATOMIC || inst.tag == OP_APPEND) { return 1; }
    if (inst.tag == OP_STORE || inst.tag == OP_STORE_INDEXED) {
        if (query.kind == WriteQueryKind::Condition) { return 1; }
        return host.overlaps(query.context, WriteEffect(item, address(), WriteEffectKind::Store));
    }
    return 0;
}

extern "C" u32 application_block_writes(Span code, WriteQuery query, WriteHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.overlaps) {
    usize index = 0u64;
    while (index < code.count) {
        if (application_instruction_writes(item_at(code, index), query, host) != 0) { return 1; }
        index += 1u64;
    }
    return 0;
}

const u32 INDEXED_VIEW = 1;
const u64 HALF_WORD = 0xffffffffu64;

ApplicationByteOffset byte_offset(u64 low) on cpu { return ApplicationByteOffset(low, 0u64); }

ApplicationByteOffset add_bytes(ApplicationByteOffset left, ApplicationByteOffset right) on cpu {
    u64 low = left.low + right.low;
    return ApplicationByteOffset(low, left.high + right.high + u64(low < left.low));
}

ApplicationByteOffset scale_bytes(u64 index, u64 stride) on cpu {
    u64 index_low = index & HALF_WORD;
    u64 stride_low = stride & HALF_WORD;
    u64 index_high = index >> 32u64;
    u64 stride_high = stride >> 32u64;
    u64 first = index_low * stride_low;
    u64 middle = index_high * stride_low + (first >> 32u64);
    u64 last = index_low * stride_high + (middle & HALF_WORD);
    u64 high = index_high * stride_high + (middle >> 32u64) + (last >> 32u64);
    return ApplicationByteOffset((last << 32u64) | (first & HALF_WORD), high);
}

bool byte_before(ApplicationByteOffset left, ApplicationByteOffset right) on cpu {
    return left.high < right.high || (left.high == right.high && left.low < right.low);
}

ApplicationByteRange bytes_at(ApplicationByteOffset start, u32 width) on cpu {
    return ApplicationByteRange(start, add_bytes(start, byte_offset(u64(width))));
}

extern "C" ApplicationByteRange application_byte_range(usize index, usize stride, u32 word, u32 width) on cpu {
    return bytes_at(add_bytes(scale_bytes(index, stride), byte_offset(u64(word))), width);
}

extern "C" u32 application_ranges_overlap(ApplicationByteRange left, ApplicationByteRange right) on cpu {
    return u32(byte_before(left.start, right.end) && byte_before(right.start, left.end));
}

usize register_at(Span words, usize index) on cpu {
    return unsafe_position<usize>(item_at(words, index));
}

OriginalAddress original_address(Span registers) on cpu {
    return OriginalAddress(register_at(registers, 0u64), register_at(registers, 1u64));
}

bool same_original_address(OriginalAddress left, OriginalAddress right, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.same) {
    return host.same(context, left.low, right.low) != 0 &&
        host.same(context, left.high, right.high) != 0;
}

Word original_stride(View stride, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.constant) {
    if (stride.tag == 0) { return Word(1, stride.a); }
    OriginalAddress parts = original_address(stride.first);
    Word low = host.constant(context, parts.low);
    if (low.present == 0) { return Word(0, 0u64); }
    Word high = host.constant(context, parts.high);
    return Word(high.present, low.value | (high.value << 32u64));
}

Word original_offset(Span optional, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.constant) {
    if (optional.count == 0u64) { return Word(1, 0u64); }
    return host.constant(context, register_at(optional, 0u64));
}

bool disjoint_address_roots(OriginalAddress left, OriginalAddress right, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.frame) requires nonzero(host.allocated) requires nonzero(host.defined)
    requires nonzero(host.descendant) {
    Word a = host.frame(context, left);
    Word b = host.frame(context, right);
    if (a.present == 0 || b.present == 0) { return false; }
    return (a.value != b.value && host.allocated(context, a.value) != 0 && host.allocated(context, b.value) != 0)
        || (host.allocated(context, a.value) != 0 && host.defined(context, b.value) == 0)
        || (host.allocated(context, b.value) != 0 && host.defined(context, a.value) == 0)
        || host.descendant(context, a.value, b.value) != 0
        || host.descendant(context, b.value, a.value) != 0;
}

extern "C" u32 application_store_overlap(IndexedByteAccess read, IndexedByteAccess written,
    Span offset, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.constant) requires nonzero(host.same)
    requires nonzero(host.definition) requires nonzero(host.frame) requires nonzero(host.allocated)
    requires nonzero(host.defined) requires nonzero(host.descendant) {
    View from = host.ir.inspect(read.source);
    View to = host.ir.inspect(written.source);
    ApplicationByteOffset written_word = byte_offset(u64(written.word));
    ApplicationByteOffset read_word = byte_offset(u64(read.word));
    if (from.tag == INDEXED_DATA && to.tag == INDEXED_DATA) {
        if (from.a != to.a) { return 0; }
        Word extra = original_offset(offset, context, host);
        if (extra.present == 0) { return 1; }
        Word a = host.constant(context, from.b);
        Word b = host.constant(context, to.b);
        if (a.present != 0 && b.present != 0 && a.value != b.value) { return 0; }
        return application_ranges_overlap(bytes_at(read_word, read.width),
            bytes_at(add_bytes(written_word, byte_offset(extra.value)), written.width));
    }
    if (from.tag != INDEXED_VIEW) { return 1; }
    while (to.tag == INDEXED_POINTER) {
        address definition = host.definition(context, original_address(to.first));
        if (definition == address()) { break; }
        View origin = instruction(definition, host.ir);
        if (origin.tag != OP_POSITION_ADDRESS) { break; }
        View reference = host.ir.inspect(origin.left);
        View storage = host.ir.inspect(reference.left);
        if (storage.tag != STORAGE_INDEXED) { break; }
        if (to.b != 0u64) {
            Word index = host.constant(context, to.a);
            if (index.present == 0) { return 1; }
            written_word = add_bytes(written_word, scale_bytes(index.value, to.b));
        }
        Word extra = original_offset(reference.first, context, host);
        if (extra.present == 0) { return 1; }
        written_word = add_bytes(add_bytes(written_word, byte_offset(extra.value)), byte_offset(reference.a));
        to = host.ir.inspect(storage.left);
    }
    if (to.tag != INDEXED_VIEW && to.tag != INDEXED_POINTER) { return 1; }
    OriginalAddress a_root = original_address(from.first);
    OriginalAddress b_root = original_address(to.first);
    if (!same_original_address(a_root, b_root, context, host)) {
        return u32(!disjoint_address_roots(a_root, b_root, context, host));
    }
    Word extra = original_offset(offset, context, host);
    if (extra.present == 0) { return 1; }
    written_word = add_bytes(written_word, byte_offset(extra.value));
    View a_stride = host.ir.inspect(from.left);
    View b_stride = a_stride;
    Word a_step = original_stride(a_stride, context, host);
    Word b_step = Word(1, to.b);
    if (to.tag == INDEXED_VIEW) {
        b_stride = host.ir.inspect(to.left);
        b_step = original_stride(b_stride, context, host);
    }
    Word a_index = host.constant(context, from.a);
    Word b_index = host.constant(context, to.a);
    if (a_index.present != 0 && b_index.present != 0 && a_step.present != 0 && b_step.present != 0) {
        read_word = add_bytes(scale_bytes(a_index.value, a_step.value), read_word);
        written_word = add_bytes(scale_bytes(b_index.value, b_step.value), written_word);
    } else {
        bool same_step;
        if (to.tag == INDEXED_VIEW && a_stride.tag != 0 && b_stride.tag != 0) {
            same_step = same_original_address(original_address(a_stride.first), original_address(b_stride.first), context, host);
        } else {
            same_step = a_step.present != 0 && b_step.present != 0 && a_step.value == b_step.value;
        }
        if (!same_step || host.same(context, from.a, to.a) == 0) { return 1; }
    }
    return application_ranges_overlap(bytes_at(read_word, read.width), bytes_at(written_word, written.width));
}
