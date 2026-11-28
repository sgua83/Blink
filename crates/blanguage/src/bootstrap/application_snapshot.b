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

struct LoopScan { usize earlier; usize later; usize block; usize boundary; };

Word next_crossed_loop(SnapshotSource source, RegisterHost host, inout LoopScan scan) on cpu
    requires nonzero(host.block) {
    while (scan.block + 1u64 < source.blocks) {
        usize block = scan.block;
        Span code = host.block(source.context, block);
        scan.boundary += code.count;
        scan.block += 1u64;
        if (scan.boundary > scan.later) { break; }
        if (scan.earlier < scan.boundary) { return Word(1, block); }
    }
    return Word(0, 0u64);
}

bool loop_local_changes(SnapshotSource source, RegisterHost host, usize first, usize second, usize slot) on cpu
    requires nonzero(host.block) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) {
    LoopScan scan = LoopScan(min(first, second), max(first, second), 0u64, 0u64);
    Word crossed = next_crossed_loop(source, host, scan);
    while (crossed.present != 0) {
        LoopEffects effects = host.loop_effects(source.context, crossed.value);
        if (effects.external != 0 || host.loop_local(effects.locals, slot) != 0) { return true; }
        crossed = next_crossed_loop(source, host, scan);
    }
    return false;
}

Word local_read_source(SnapshotSource source, RegisterHost host, RegisterDefinition read, View load) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) {
    if (load.tag != OP_LOAD_LOCAL) { return Word(0, 0u64); }
    address containing = top_instruction(source, host, read.top);
    if (containing == address() || host.writes_local(source.context, containing, load.b) != 0) {
        return Word(0, 0u64);
    }
    RegisterDefinition write = preceding_local_write(source, host, read.top, load.b);
    if (write.instruction == address() || loop_local_changes(source, host, write.top, read.top, load.b)) {
        return Word(0, 0u64);
    }
    View store = instruction(write.instruction, host.ir);
    return Word(u32(store.tag == OP_STORE_LOCAL), store.b);
}

extern "C" Word application_register_constant(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast) {
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
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
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
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast) {
    if (stride.tag == 0) { return Word(1, stride.a); }
    Word low = application_register_constant(source, host, register_at(stride.first, 0u64));
    if (low.present == 0) { return low; }
    Word high = application_register_constant(source, host, register_at(stride.first, 1u64));
    return Word(high.present, low.value | (high.value << 32u64));
}

bool equal_register_strides(SnapshotSource source, RegisterHost host, Node left, Node right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
    View a = host.ir.inspect(left);
    View b = host.ir.inspect(right);
    if (a.tag != 0 && b.tag != 0) { return equal_register_lists(source, host, a.first, b.first); }
    Word first = register_stride(source, host, a);
    Word second = register_stride(source, host, b);
    return first.present != 0 && second.present != 0 && first.value == second.value;
}

bool equal_loaded_values(SnapshotSource source, RegisterHost host,
    View a, View b, usize a_top, usize b_top) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
    if (a.tag != b.tag || register_scalar(source.registers, a.a) != register_scalar(source.registers, b.a)) {
        return false;
    }
    if (a.tag == OP_LOAD) {
        return a.b == b.b && a.d == b.d && !loop_memory_changes(source, host, a_top, b_top, a)
            && application_register_equal(source, host, a.c, b.c) != 0;
    }
    return a.tag == OP_VIEW_LOAD && equal_register_strides(source, host, a.left, b.left) && a.c == b.c
        && !loop_memory_changes(source, host, a_top, b_top, a)
        && equal_register_lists(source, host, a.first, b.first)
        && application_register_equal(source, host, a.b, b.b) != 0;
}

bool equal_snapshot_loads(SnapshotSource source, RegisterHost host, address item, address read) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
    View a = instruction(item, host.ir);
    View b = instruction(read, host.ir);
    if (a.tag != b.tag) { return false; }
    RegisterDefinition left = application_register_definition(source, host, a.a);
    RegisterDefinition right = application_register_definition(source, host, b.a);
    return equal_loaded_values(source, host, a, b, left.top, right.top);
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
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
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
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) requires nonzero(host.pointer_root) {
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
        matching = a.b == b.b && !loop_local_changes(source, host, ai, bi, a.b);
    } else {
        matching = equal_loaded_values(source, host, a, b, ai, bi);
    }
    return u32(matching && !intervening_write(source, host, begin, end, a_definition.instruction, a));
}

struct SnapshotWalk { address read; u32 stopped; };

SnapshotWalk scan_block(SnapshotSource source, SnapshotHost host, Span code, address newest) on cpu
    requires nonzero(host.registers.ir.inspect) requires nonzero(host.registers.block)
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_effects) requires nonzero(host.registers.loop_local)
    requires nonzero(host.registers.loop_storage) requires nonzero(host.registers.scalar_bytes)
    requires nonzero(host.registers.cast) requires nonzero(host.registers.writes_load) requires nonzero(host.registers.allocation)
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
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_effects) requires nonzero(host.registers.loop_local)
    requires nonzero(host.registers.loop_storage) requires nonzero(host.registers.scalar_bytes)
    requires nonzero(host.registers.cast) requires nonzero(host.registers.writes_load) requires nonzero(host.registers.allocation)
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

u32 application_ranges_overlap(ApplicationByteRange left, ApplicationByteRange right) on cpu {
    return u32(byte_before(left.start, right.end) && byte_before(right.start, left.end));
}

usize register_at(Span words, usize index) on cpu {
    return unsafe_position<usize>(item_at(words, index));
}

OriginalAddress original_address(Span registers) on cpu {
    return OriginalAddress(register_at(registers, 0u64), register_at(registers, 1u64));
}

// These operators are the original BinaryOperation projection identities.
enum AddressOperation:u32 { Add=4; Multiply=6; Combine=14; ShiftLeft=16; ShiftRight=17; };

bool equal_address(SnapshotSource source, RegisterHost host, OriginalAddress a, OriginalAddress b) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    return application_register_equal(source, host, a.low, b.low) != 0
        && application_register_equal(source, host, a.high, b.high) != 0;
}

extern "C" Word application_frame_root(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root) {
    RegisterDefinition low = application_register_definition(source, host, address_parts.low);
    RegisterDefinition high = application_register_definition(source, host, address_parts.high);
    if (low.instruction == address() || high.instruction == address() || low.top != high.top) {
        return Word(0, 0u64);
    }
    View inst = instruction(low.instruction, host.ir);
    if (inst.tag == OP_INVOKE) { return host.pointer_root(source.context, inst.left.data); }
    return Word(0, 0u64);
}

extern "C" u32 application_allocated(SnapshotSource source, RegisterHost host, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.allocation) {
    RegisterDefinition definition = application_register_definition(source, host, root);
    if (definition.instruction == address()) { return 0; }
    View inst = instruction(definition.instruction, host.ir);
    if (inst.tag == OP_INVOKE) { return host.allocation(inst.left.data); }
    return 0;
}

u32 application_frame_descendant(SnapshotSource source, RegisterHost host, usize child, usize parent) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    usize current = child;
    while (true) {
        RegisterDefinition definition = application_register_definition(source, host, current);
        if (definition.instruction == address()) { return 0; }
        View inst = instruction(definition.instruction, host.ir);
        if (inst.tag != OP_VIEW_LOAD) { return 0; }
        Word container = application_frame_root(source, host, original_address(inst.first));
        if (container.present == 0) { return 0; }
        if (application_register_equal(source, host, container.value, parent) != 0) { return 1; }
        current = container.value;
    }
}

extern "C" u32 application_storage_includes_root(SnapshotSource source, RegisterHost host, LoopStorage written, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.allocation) {
    if (written.descendants == 0) { return u32(written.root == root); }
    return u32(application_register_equal(source, host, root, written.root) == 0
        && application_frame_descendant(source, host, written.root, root) == 0);
}

bool distinct_loop_roots(SnapshotSource source, RegisterHost host, usize first, usize second) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.allocation) {
    return (first != second && application_allocated(source, host, first) != 0
        && application_allocated(source, host, second) != 0)
        || application_frame_descendant(source, host, first, second) != 0
        || application_frame_descendant(source, host, second, first) != 0;
}

extern "C" u32 application_storage_changes(SnapshotSource source, RegisterHost host, LoopEffects effects,
    Word root, ApplicationByteRange range, u32 has_range) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.allocation) {
    if (effects.unknown_storage != 0) { return 1; }
    usize index = 0u64;
    while (index < effects.storage.count) {
        LoopStorage written = host.loop_storage(item_at(effects.storage, index));
        if (root.present == 0) { return 1; }
        if (written.descendants != 0) {
            if (application_storage_includes_root(source, host, written, root.value) != 0) { return 1; }
        } else if (!distinct_loop_roots(source, host, root.value, written.root)) {
            if (has_range == 0 || written.has_range == 0 ||
                application_register_equal(source, host, root.value, written.root) == 0 ||
                application_ranges_overlap(range, written.range) != 0) { return 1; }
        }
        index += 1u64;
    }
    return 0;
}

bool loop_memory_changes(SnapshotSource source, RegisterHost host, usize first, usize second, View load) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.allocation) {
    LoopScan scan = LoopScan(min(first, second), max(first, second), 0u64, 0u64);
    Word crossed = next_crossed_loop(source, host, scan);
    while (crossed.present != 0) {
        LoopEffects effects = host.loop_effects(source.context, crossed.value);
        if (effects.unknown_storage != 0) { return true; }
        if (load.tag == OP_LOAD) {
            if (effects.external != 0) { return true; }
        } else if (effects.storage.count != 0u64) {
            Word index = application_register_constant(source, host, load.b);
            Word stride = register_stride(source, host, host.ir.inspect(load.left));
            bool has_range = index.present != 0 && stride.present != 0;
            ApplicationByteOffset zero = byte_offset(0u64);
            ApplicationByteRange range = ApplicationByteRange(zero, zero);
            if (has_range) {
                range = application_byte_range(index.value, stride.value, u32(load.c),
                    host.scalar_bytes(source.context, load.a));
            }
            Word root = application_frame_root(source, host, original_address(load.first));
            if (application_storage_changes(source, host, effects, root, range, u32(has_range)) != 0) { return true; }
        }
        crossed = next_crossed_loop(source, host, scan);
    }
    return false;
}

Word cast_input(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) {
    RegisterDefinition definition = application_register_definition(source, host, reg);
    if (definition.instruction == address()) { return Word(0, 0u64); }
    View inst = instruction(definition.instruction, host.ir);
    return Word(u32(inst.tag == OP_CAST), inst.b);
}

struct AddressOperands { u32 present; usize left; usize right; };

AddressOperands address_operands(SnapshotSource source, RegisterHost host, usize reg, AddressOperation op) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) {
    RegisterDefinition definition = application_register_definition(source, host, reg);
    if (definition.instruction == address()) { return AddressOperands(0, 0u64, 0u64); }
    View inst = instruction(definition.instruction, host.ir);
    bool matched = inst.tag == OP_BINARY && host.ir.inspect(inst.right).tag == u32(op);
    return AddressOperands(u32(matched), inst.b, inst.c);
}

bool register_is(SnapshotSource source, RegisterHost host, usize reg, usize value) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast) {
    Word constant = application_register_constant(source, host, reg);
    return constant.present != 0 && constant.value == value;
}

extern "C" Word application_split_address(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    Word low = cast_input(source, host, address_parts.low);
    Word high = cast_input(source, host, address_parts.high);
    if (low.present == 0 || high.present == 0) { return Word(0, 0u64); }
    AddressOperands shift = address_operands(source, host, high.value, AddressOperation::ShiftRight);
    bool matched = shift.present != 0 && register_is(source, host, shift.right, 32u64)
        && application_register_equal(source, host, low.value, shift.left) != 0;
    return Word(u32(matched), low.value);
}

extern "C" OffsetPosition application_offset_position(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    OffsetPosition missing = OffsetPosition(0, OriginalAddress(0u64, 0u64), 0u64, 0u64, 0u64);
    Word pointer = application_split_address(source, host, address_parts);
    if (pointer.present == 0) { return missing; }
    AddressOperands pointer_parts = address_operands(source, host, pointer.value, AddressOperation::Add);
    if (pointer_parts.present == 0) { return missing; }
    AddressOperands displacement = address_operands(source, host, pointer_parts.right, AddressOperation::Add);
    if (displacement.present == 0) { return missing; }
    AddressOperands element = address_operands(source, host, displacement.left, AddressOperation::Multiply);
    if (element.present == 0) { return missing; }
    AddressOperands base = address_operands(source, host, pointer_parts.left, AddressOperation::Combine);
    if (base.present == 0) { return missing; }
    AddressOperands upper = address_operands(source, host, base.right, AddressOperation::ShiftLeft);
    if (upper.present == 0 || !register_is(source, host, upper.right, 32u64)) { return missing; }
    Word low = cast_input(source, host, base.left);
    Word high = cast_input(source, host, upper.left);
    Word index = cast_input(source, host, element.left);
    Word stride = cast_input(source, host, element.right);
    Word offset = cast_input(source, host, displacement.right);
    if (low.present == 0 || high.present == 0 || index.present == 0 || stride.present == 0 || offset.present == 0) {
        return missing;
    }
    return OffsetPosition(1, OriginalAddress(low.value, high.value), index.value, stride.value, offset.value);
}

bool zero_stride(SnapshotSource source, RegisterHost host, Node stride) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast) {
    Word value = register_stride(source, host, host.ir.inspect(stride));
    return value.present != 0 && value.value == 0u64;
}

extern "C" u32 application_same_position(SnapshotSource source, RegisterHost host, Node left, Node right, u32 exact_index) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    View a = host.ir.inspect(left);
    View b = host.ir.inspect(right);
    if (a.tag != INDEXED_VIEW || b.tag != INDEXED_VIEW) { return 0; }
    OriginalAddress a_base = original_address(a.first);
    OriginalAddress b_base = original_address(b.first);
    if (equal_address(source, host, a_base, b_base) && equal_register_strides(source, host, a.left, b.left)
        && (exact_index == 0 || application_register_equal(source, host, a.a, b.a) != 0)) { return 1; }
    if (!register_is(source, host, a.a, 0u64) || !register_is(source, host, b.a, 0u64)
        || !zero_stride(source, host, a.left) || !zero_stride(source, host, b.left)) { return 0; }
    OffsetPosition ap = application_offset_position(source, host, a_base);
    OffsetPosition bp = application_offset_position(source, host, b_base);
    return u32(ap.present != 0 && bp.present != 0 && equal_address(source, host, ap.base, bp.base)
        && application_register_equal(source, host, ap.stride, bp.stride) != 0
        && application_register_equal(source, host, ap.offset, bp.offset) != 0
        && (exact_index == 0 || application_register_equal(source, host, ap.index, bp.index) != 0));
}

Word original_offset(SnapshotSource source, RegisterHost host, Span optional) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast) {
    if (optional.count == 0u64) { return Word(1, 0u64); }
    return application_register_constant(source, host, register_at(optional, 0u64));
}

Word address_root(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    Word frame = application_frame_root(source, host, address_parts);
    if (frame.present != 0) { return frame; }
    OffsetPosition position = application_offset_position(source, host, address_parts);
    if (position.present == 0) { return Word(0, 0u64); }
    return application_frame_root(source, host, position.base);
}

bool disjoint_address_roots(SnapshotSource source, RegisterHost host, OriginalAddress left, OriginalAddress right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    Word a = address_root(source, host, left);
    Word b = address_root(source, host, right);
    if (a.present == 0 || b.present == 0) { return false; }
    bool a_allocated = application_allocated(source, host, a.value) != 0;
    bool b_allocated = application_allocated(source, host, b.value) != 0;
    return (a.value != b.value && a_allocated && b_allocated)
        || (a_allocated && application_register_definition(source, host, b.value).instruction == address())
        || (b_allocated && application_register_definition(source, host, a.value).instruction == address())
        || application_frame_descendant(source, host, a.value, b.value) != 0
        || application_frame_descendant(source, host, b.value, a.value) != 0;
}

extern "C" u32 application_store_overlap(IndexedByteAccess read, IndexedByteAccess written,
    Span offset, SnapshotSource source, RegisterHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.pointer_root)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(host.allocation) {
    View from = host.ir.inspect(read.source);
    View to = host.ir.inspect(written.source);
    ApplicationByteOffset written_word = byte_offset(u64(written.word));
    ApplicationByteOffset read_word = byte_offset(u64(read.word));
    if (from.tag == INDEXED_DATA && to.tag == INDEXED_DATA) {
        if (from.a != to.a) { return 0; }
        Word extra = original_offset(source, host, offset);
        if (extra.present == 0) { return 1; }
        Word a = application_register_constant(source, host, from.b);
        Word b = application_register_constant(source, host, to.b);
        if (a.present != 0 && b.present != 0 && a.value != b.value) { return 0; }
        return application_ranges_overlap(bytes_at(read_word, read.width),
            bytes_at(add_bytes(written_word, byte_offset(extra.value)), written.width));
    }
    if (from.tag != INDEXED_VIEW) { return 1; }
    while (to.tag == INDEXED_POINTER) {
        Word pointer = application_split_address(source, host, original_address(to.first));
        if (pointer.present == 0) { break; }
        RegisterDefinition definition = application_register_definition(source, host, pointer.value);
        if (definition.instruction == address()) { break; }
        View origin = instruction(definition.instruction, host.ir);
        if (origin.tag != OP_POSITION_ADDRESS) { break; }
        View reference = host.ir.inspect(origin.left);
        View storage = host.ir.inspect(reference.left);
        if (storage.tag != STORAGE_INDEXED) { break; }
        if (to.b != 0u64) {
            Word index = application_register_constant(source, host, to.a);
            if (index.present == 0) { return 1; }
            written_word = add_bytes(written_word, scale_bytes(index.value, to.b));
        }
        Word extra = original_offset(source, host, reference.first);
        if (extra.present == 0) { return 1; }
        written_word = add_bytes(add_bytes(written_word, byte_offset(extra.value)), byte_offset(reference.a));
        to = host.ir.inspect(storage.left);
    }
    if (to.tag != INDEXED_VIEW && to.tag != INDEXED_POINTER) { return 1; }
    OriginalAddress a_root = original_address(from.first);
    OriginalAddress b_root = original_address(to.first);
    if (!equal_address(source, host, a_root, b_root)) {
        return u32(!disjoint_address_roots(source, host, a_root, b_root));
    }
    Word extra = original_offset(source, host, offset);
    if (extra.present == 0) { return 1; }
    written_word = add_bytes(written_word, byte_offset(extra.value));
    View a_stride = host.ir.inspect(from.left);
    View b_stride = a_stride;
    Word a_step = register_stride(source, host, a_stride);
    Word b_step = Word(1, to.b);
    if (to.tag == INDEXED_VIEW) {
        b_stride = host.ir.inspect(to.left);
        b_step = register_stride(source, host, b_stride);
    }
    Word a_index = application_register_constant(source, host, from.a);
    Word b_index = application_register_constant(source, host, to.a);
    if (a_index.present != 0 && b_index.present != 0 && a_step.present != 0 && b_step.present != 0) {
        read_word = add_bytes(scale_bytes(a_index.value, a_step.value), read_word);
        written_word = add_bytes(scale_bytes(b_index.value, b_step.value), written_word);
    } else {
        bool same_step;
        if (to.tag == INDEXED_VIEW && a_stride.tag != 0 && b_stride.tag != 0) {
            same_step = equal_register_lists(source, host, a_stride.first, b_stride.first);
        } else {
            same_step = a_step.present != 0 && b_step.present != 0 && a_step.value == b_step.value;
        }
        if (!same_step || application_register_equal(source, host, from.a, to.a) == 0) { return 1; }
    }
    return application_ranges_overlap(bytes_at(read_word, read.width), bytes_at(written_word, written.width));
}
