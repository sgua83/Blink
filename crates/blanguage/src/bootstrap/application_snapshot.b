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
