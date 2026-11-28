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

extern "C" RegisterDefinition application_register_definition(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) {
    usize block = source.blocks;
    usize end = source.instructions;
    while (block != 0u64) {
        block -= 1u64;
        Span code = host.block(source.context, block);
        end -= code.count;
        RegisterDefinition definition = register_definition(code, reg, host.ir);
        if (definition.instruction != address()) { return RegisterDefinition(definition.instruction, end + definition.top); }
    }
    return RegisterDefinition(address(), 0u64);
}

address top_instruction(SnapshotSource source, RegisterHost host, usize position) on cpu requires nonzero(host.block) {
    usize block = 0u64;
    usize at = position;
    while (block < source.blocks) {
        Span code = host.block(source.context, block);
        if (at < code.count) { return item_at(code, at); }
        at -= code.count;
        block += 1u64;
    }
    return address();
}

RegisterDefinition preceding_local_write(SnapshotSource source, RegisterHost host, usize before, usize slot) on cpu
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
                if (host.writes_local(source.context, item, slot) != 0) {
                    return RegisterDefinition(item, start + index);
                }
            }
        }
        end = start;
    }
    return RegisterDefinition(address(), 0u64);
}

Word local_read_source(SnapshotSource source, RegisterHost host, RegisterDefinition read, View load) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) {
    if (load.tag != OP_LOAD_LOCAL) { return Word(0, 0u64); }
    address containing = top_instruction(source, host, read.top);
    if (containing == address() || host.writes_local(source.context, containing, load.b) != 0) {
        return Word(0, 0u64);
    }
    RegisterDefinition write = preceding_local_write(source, host, read.top, load.b);
    if (write.instruction == address() || host.loop_writes(source.context, write.top, read.top, read.instruction) != 0) {
        return Word(0, 0u64);
    }
    View store = instruction(write.instruction, host.ir);
    return Word(u32(store.tag == OP_STORE_LOCAL), store.b);
}

extern "C" Word application_register_constant(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast) {
    RegisterDefinition definition = application_register_definition(source, host, reg);
    if (definition.instruction == address()) { return Word(0, 0u64); }
    View inst = instruction(definition.instruction, host.ir);
    if (inst.tag == OP_CONSTANT) { return Word(1, inst.b); }
    if (inst.tag == OP_CAST) {
        Word input = application_register_constant(source, host, inst.b);
        if (input.present != 0) { return host.cast(source.context, inst.b, reg, input.value); }
        return Word(0, 0u64);
    }
    Word local_source = local_read_source(source, host, definition, inst);
    if (local_source.present != 0) { return application_register_constant(source, host, local_source.value); }
    return Word(0, 0u64);
}

usize condition_source(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.writes_local) {
    usize current = reg;
    while (true) {
        RegisterDefinition definition = application_register_definition(source, host, current);
        if (definition.instruction == address()) { return current; }
        View load = instruction(definition.instruction, host.ir);
        if (load.tag != OP_LOAD_LOCAL) { return current; }
        RegisterDefinition write = preceding_local_write(source, host, definition.top, load.b);
        if (write.instruction == address()) { return current; }
        View store = instruction(write.instruction, host.ir);
        if (store.tag != OP_STORE_LOCAL) { return current; }
        current = store.b;
    }
}

bool equal_register_lists(SnapshotSource source, RegisterHost host, Span left, Span right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    if (left.count != right.count) { return false; }
    usize index = 0u64;
    while (index < left.count) {
        if (application_register_equal(source, host, register_at(left, index), register_at(right, index)) == 0) {
            return false;
        }
        index += 1u64;
    }
    return true;
}

Word register_stride(SnapshotSource source, RegisterHost host, View stride) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast) {
    if (stride.tag == 0) { return Word(1, stride.a); }
    Word low = application_register_constant(source, host, register_at(stride.first, 0u64));
    if (low.present == 0) { return low; }
    Word high = application_register_constant(source, host, register_at(stride.first, 1u64));
    return Word(high.present, low.value | (high.value << 32u64));
}

bool equal_register_strides(SnapshotSource source, RegisterHost host, Node left, Node right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    View a = host.ir.inspect(left);
    View b = host.ir.inspect(right);
    if (a.tag != 0 && b.tag != 0) { return equal_register_lists(source, host, a.first, b.first); }
    Word first = register_stride(source, host, a);
    Word second = register_stride(source, host, b);
    return first.present != 0 && second.present != 0 && first.value == second.value;
}

bool equal_loaded_values(SnapshotSource source, RegisterHost host,
    address read, View a, View b, usize a_top, usize b_top) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    if (a.tag != b.tag || register_scalar(source.registers, a.a) != register_scalar(source.registers, b.a)) {
        return false;
    }
    if (a.tag == OP_LOAD) {
        return a.b == b.b && a.d == b.d && host.loop_writes(source.context, a_top, b_top, read) == 0
            && application_register_equal(source, host, a.c, b.c) != 0;
    }
    return a.tag == OP_VIEW_LOAD && equal_register_strides(source, host, a.left, b.left) && a.c == b.c
        && host.loop_writes(source.context, a_top, b_top, read) == 0
        && equal_register_lists(source, host, a.first, b.first)
        && application_register_equal(source, host, a.b, b.b) != 0;
}

bool equal_snapshot_loads(SnapshotSource source, RegisterHost host, address item, address read) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    View a = instruction(item, host.ir);
    View b = instruction(read, host.ir);
    if (a.tag != b.tag) { return false; }
    RegisterDefinition left = application_register_definition(source, host, a.a);
    RegisterDefinition right = application_register_definition(source, host, b.a);
    return equal_loaded_values(source, host, item, a, b, left.top, right.top);
}

bool top_produces(SnapshotSource source, RegisterHost host, usize position, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) {
    address item = top_instruction(source, host, position);
    Results output = results(Node(item, N_INSTRUCTION), host.ir);
    Word found = next_result(output);
    while (found.present != 0) {
        if (found.value == reg) { return true; }
        found = next_result(output);
    }
    return false;
}

bool intervening_write(SnapshotSource source, RegisterHost host, usize begin, usize end,
    address read, View load) on cpu
    requires nonzero(host.block) requires nonzero(host.writes_local) requires nonzero(host.writes_load) {
    usize block = 0u64;
    usize start = 0u64;
    while (block < source.blocks && start < end) {
        Span code = host.block(source.context, block);
        usize stop = start + code.count;
        if (begin < stop) {
            usize index = max(begin, start) - start;
            usize limit = min(end, stop) - start;
            while (index < limit) {
                address item = item_at(code, index);
                u32 hit = load.tag == OP_LOAD_LOCAL
                    ? host.writes_local(source.context, item, load.b)
                    : host.writes_load(source.context, item, read);
                if (hit != 0) { return true; }
                index += 1u64;
            }
        }
        start = stop;
        block += 1u64;
    }
    return false;
}

Word register_result_slot(Span span, usize reg) on cpu {
    usize index = 0u64;
    while (index < span.count) {
        if (register_at(span, index) == reg) { return Word(1, index); }
        index += 1u64;
    }
    return Word(0, 0u64);
}

bool equal_address_results(SnapshotSource source, RegisterHost host, View a, View b, usize left, usize right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    Word a_root = host.pointer_root(source.context, a.left.data);
    Word b_root = host.pointer_root(source.context, b.left.data);
    if (a_root.present == 0 || b_root.present == 0) { return false; }
    Word a_slot = register_result_slot(a.first, left);
    Word b_slot = register_result_slot(b.first, right);
    if (a_slot.present != b_slot.present || a_slot.value != b_slot.value
        || application_register_equal(source, host, a_root.value, b_root.value) == 0) { return false; }
    View a_call = host.ir.inspect(a.left);
    View b_call = host.ir.inspect(b.left);
    if (a_call.first.count == 0u64 || b_call.first.count == 0u64) { return false; }
    View a_frame = host.ir.inspect(Node(a_call.first.data, N_ARGUMENT));
    View b_frame = host.ir.inspect(Node(b_call.first.data, N_ARGUMENT));
    return a_frame.tag == 0 && b_frame.tag == 0 && equal_register_lists(source, host, a_frame.first, b_frame.first);
}

extern "C" u32 application_register_equal(SnapshotSource source, RegisterHost host, usize left, usize right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_writes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.pointer_root) {
    if (left == right) { return 1; }
    if (register_scalar(source.registers, left) != register_scalar(source.registers, right)) { return 0; }
    RegisterDefinition a_definition = application_register_definition(source, host, left);
    RegisterDefinition b_definition = application_register_definition(source, host, right);
    if (a_definition.instruction != address()) {
        View inst = instruction(a_definition.instruction, host.ir);
        Word local_source = local_read_source(source, host, a_definition, inst);
        if (local_source.present != 0) {
            return application_register_equal(source, host, local_source.value, right);
        }
    }
    if (b_definition.instruction != address()) {
        View inst = instruction(b_definition.instruction, host.ir);
        Word local_source = local_read_source(source, host, b_definition, inst);
        if (local_source.present != 0) {
            return application_register_equal(source, host, left, local_source.value);
        }
    }
    if (a_definition.instruction == address() || b_definition.instruction == address()) { return 0; }
    View a = instruction(a_definition.instruction, host.ir);
    View b = instruction(b_definition.instruction, host.ir);
    if (a.tag != b.tag) { return 0; }
    if (a.tag == OP_CONSTANT) { return u32(a.c == b.c && a.b == b.b); }
    if (a.tag == OP_CAST) { return application_register_equal(source, host, a.b, b.b); }
    if (a.tag == OP_UNARY) {
        return u32(a.c == b.c && application_register_equal(source, host, a.b, b.b) != 0);
    }
    if (a.tag == OP_BINARY) {
        return u32(host.ir.inspect(a.right).tag == host.ir.inspect(b.right).tag
            && application_register_equal(source, host, a.b, b.b) != 0
            && application_register_equal(source, host, a.c, b.c) != 0);
    }
    if (a.tag == OP_MATH) { return u32(a.b == b.b && equal_register_lists(source, host, a.first, b.first)); }
    if (a.tag == OP_INVOKE) { return u32(equal_address_results(source, host, a, b, left, right)); }
    if (a.tag != OP_LOAD_LOCAL && a.tag != OP_LOAD && a.tag != OP_VIEW_LOAD) { return 0; }
    usize ai = a_definition.top;
    usize bi = b_definition.top;
    bool direct = top_produces(source, host, ai, left) && top_produces(source, host, bi, right);
    usize begin = min(ai, bi) + u64(direct);
    usize end = max(ai, bi) + u64(!direct);
    bool matching;
    if (a.tag == OP_LOAD_LOCAL) {
        matching = a.b == b.b && host.loop_writes(source.context, ai, bi, a_definition.instruction) == 0;
    } else {
        matching = equal_loaded_values(source, host, a_definition.instruction, a, b, ai, bi);
    }
    return u32(matching && !intervening_write(source, host, begin, end, a_definition.instruction, a));
}

struct SnapshotWalk { address read; u32 stopped; };

SnapshotWalk scan_block(SnapshotSource source, SnapshotHost host, Span code, address newest) on cpu
    requires nonzero(host.registers.ir.inspect) requires nonzero(host.registers.block)
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_writes)
    requires nonzero(host.registers.cast) requires nonzero(host.registers.writes_load)
    requires nonzero(host.registers.pointer_root) requires nonzero(host.truth) {
    address read = newest;
    usize index = code.count;
    while (index != 0u64) {
        index -= 1u64;
        address item = item_at(code, index);
        View inst = instruction(item, host.registers.ir);
        if (inst.tag == OP_SCOPE) {
            read = scan_block(source, host, inst.first, read).read;
        } else if (inst.tag == OP_BRANCH) {
            View loaded = instruction(read, host.registers.ir);
            if (has_candidate(inst.first, source.registers, loaded, host.registers.ir) ||
                has_candidate(inst.second, source.registers, loaded, host.registers.ir)) {
                usize branch = condition_source(source, host.registers, inst.a);
                SnapshotTruth truth = host.truth(host.truth_context, branch);
                if (truth != SnapshotTruth::Unknown) {
                    Span selected = truth == SnapshotTruth::Yes ? inst.first : inst.second;
                    read = scan_block(source, host, selected, read).read;
                }
            }
        }
        if (host.registers.writes_load(source.context, item, read) != 0) { return SnapshotWalk(read, 1); }
        if ((inst.tag == OP_LOAD || inst.tag == OP_VIEW_LOAD) &&
            equal_snapshot_loads(source, host.registers, item, read)) { read = item; }
    }
    return SnapshotWalk(read, 0);
}

extern "C" usize read_snapshot(SnapshotSource source, SnapshotHost host, usize register) on cpu
    requires nonzero(host.registers.ir.inspect) requires nonzero(host.registers.block)
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_writes)
    requires nonzero(host.registers.cast) requires nonzero(host.registers.writes_load)
    requires nonzero(host.registers.pointer_root) requires nonzero(host.truth) {
    RegisterDefinition definition = application_register_definition(source, host.registers, register);
    if (definition.instruction == address()) { return register; }
    address read = definition.instruction;
    View first = instruction(read, host.registers.ir);
    if (first.tag != OP_LOAD && first.tag != OP_VIEW_LOAD) { return register; }
    usize block = source.blocks;
    usize end = source.instructions;
    while (block != 0u64) {
        block -= 1u64;
        Span code = host.registers.block(source.context, block);
        usize start = end - code.count;
        if (definition.top > start) {
            code.count = min(code.count, definition.top - start);
            SnapshotWalk result = scan_block(source, host, code, read);
            read = result.read;
            if (result.stopped != 0) { break; }
        }
        end = start;
    }
    return instruction(read, host.registers.ir).a;
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
