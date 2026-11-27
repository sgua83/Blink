module bootstrap.x64_instruction;
import bootstrap.unwind;
import bootstrap.ir_dataflow;
import bootstrap.names;

struct BinaryEncoding {
    u64 length;
    u64 left_literal;
    u64 right_literal;
};

const u8 OPCODE_REGISTER = u8(0);
const u8 DIRECT_REGISTER = u8(1);
const u8 MEMORY = u8(2);
const u8 NO_INDEX = u8(255);

u64 instruction_prefix(Encoding input, bool byte_register, u64 at) on cpu {
    u64 cursor = at;
    if (input.prefix != u8(0)) {
        unsafe_position<u8>(unsafe_address(cursor)) = input.prefix;
        cursor += 1u64;
    }
    u32 rex = 0x40 | (u32(input.wide) << 3) | ((u32(input.reg) >> 3) << 2) | (u32(input.base) >> 3);
    bool has_index = input.index != NO_INDEX;
    if (has_index) {
        rex |= (u32(input.index) >> 3) << 1;
    }
    if (rex != 0x40 || byte_register) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(rex);
        cursor += 1u64;
    }
    return cursor;
}

u64 instruction_operand(Encoding input, u64 at) on cpu {
    u64 cursor = at;
    if (input.form == OPCODE_REGISTER) {
        return cursor;
    }
    bool has_index = input.index != NO_INDEX;

    u32 reg_field = (u32(input.reg) & 7) << 3;
    u32 base_field = u32(input.base) & 7;
    // RIP-relative literal reads use 00/r/101 and a zero disp32 for later patching.
    u32 modrm = reg_field | 5;
    u32 displacement = 0;
    u32 remaining = 4;
    bool has_sib = false;
    u32 sib = 0x24;
    if (input.form == DIRECT_REGISTER) {
        modrm = 0xc0 | reg_field | base_field;
        remaining = 0;
    }
    if (input.form == MEMORY) {
        displacement = input.displacement;
        u32 mode = 2;
        if (displacement == 0 && base_field != 5) {
            mode = 0;
            remaining = 0;
        } else if (displacement <= 127) {
            mode = 1;
            remaining = 1;
        }
        has_sib = has_index || base_field == 4;
        u32 rm = base_field;
        if (has_sib) {
            rm = 4;
        }
        modrm = (mode << 6) | reg_field | rm;
        if (has_index) {
            sib = (u32(input.shift) << 6) | ((u32(input.index) & 7) << 3) | base_field;
        }
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(modrm);
    cursor += 1u64;
    if (has_sib) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(sib);
        cursor += 1u64;
    }
    while (remaining != 0) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(displacement);
        cursor += 1u64;
        displacement >>= 8;
        remaining -= 1;
    }
    return cursor;
}

bool uses_byte_register(u8 first, u8 second) on cpu {
    return first == u8(0x88) || first == u8(0x8a) ||
        (first == u8(0x0f) && (second == u8(0xb6) || second == u8(0xbe)));
}

extern "C" u64 encode(Encoding input, address opcode, u64 opcode_length, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    bool byte_register = false;
    if (opcode_length == 1u64) {
        byte_register = uses_byte_register(unsafe_position<u8>(opcode), u8(0));
    }
    if (opcode_length == 2u64) {
        u8 first = unsafe_position<u8>(opcode);
        u8 second = unsafe_position<u8>(unsafe_address(address_bits(opcode) + 1u64));
        if (first == u8(0x0f)) { byte_register = uses_byte_register(first, second); }
    }
    cursor = instruction_prefix(input, byte_register, cursor);
    u64 index = 0u64;
    while (index < opcode_length) {
        u8 byte = unsafe_position<u8>(unsafe_address(address_bits(opcode) + index));
        unsafe_position<u8>(unsafe_address(cursor)) = byte;
        cursor += 1u64;
        index += 1u64;
    }
    return instruction_operand(input, cursor) - start;
}

void write_word(u64 at, u32 word) on cpu {
    unsafe_position<u8>(unsafe_address(at)) = u8(word);
    unsafe_position<u8>(unsafe_address(at + 1u64)) = u8(word >> 8);
    unsafe_position<u8>(unsafe_address(at + 2u64)) = u8(word >> 16);
    unsafe_position<u8>(unsafe_address(at + 3u64)) = u8(word >> 24);
}

extern "C" void patch_relative(address field, u64 offset, u64 target) on cpu {
    write_word(address_bits(field), u32(target) - u32(offset + 4u64));
}

u64 adjust_frame(u64 at, u8 modrm, u32 frame) on cpu {
    put_byte(at, u8(0x48));
    put_byte(at + 1u64, u8(0x81));
    put_byte(at + 2u64, modrm);
    write_word(at + 3u64, frame);
    return at + 7u64;
}

extern "C" u64 release_frame(u32 frame, address output) on cpu {
    u64 start = address_bits(output);
    return adjust_frame(start, u8(0xc4), frame) - start;
}

extern "C" u64 stack_register(u8 register, bool restore, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (register >= u8(8)) {
        put_byte(cursor, u8(0x41));
        cursor += 1u64;
    }
    u32 opcode = restore ? 0x58 : 0x50;
    put_byte(cursor, u8(opcode + (u32(register) & 7)));
    return cursor + 1u64 - start;
}

extern "C" FrameEncoding allocate_frame(u32 frame, address output) on cpu {
    u64 cursor = address_bits(output);
    u64 probe_field = 0u64;
    if (frame >= 4096) {
        // The leaf helper takes the byte count in EAX.
        unsafe_position<u8>(unsafe_address(cursor)) = u8(0xb8);
        write_word(cursor + 1u64, frame);
        unsafe_position<u8>(unsafe_address(cursor + 5u64)) = u8(0xe8);
        probe_field = 6u64;
        write_word(cursor + probe_field, 0);
        cursor += 10u64;
    }
    cursor = adjust_frame(cursor, u8(0xec), frame);
    return FrameEncoding(cursor - address_bits(output), probe_field);
}

// The leaf touches each page from the caller's stack pointer to the new end.
// It preserves RAX and the argument registers. Its internal branches are fixed.
const u32 STACK_PROBE_LENGTH = 52;
const array<u8,STACK_PROBE_LENGTH> STACK_PROBE = array<u8,STACK_PROBE_LENGTH>(
    u8(0x4c), u8(0x8b), u8(0xd4), u8(0x49), u8(0x83), u8(0xc2), u8(0x08), u8(0x49), u8(0x89), u8(0xc3),
    u8(0x49), u8(0x81), u8(0xfb), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0x0f), u8(0x82), u8(0x16), u8(0x00), u8(0x00), u8(0x00),
    u8(0x49), u8(0x81), u8(0xea), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0x41), u8(0x85), u8(0x02), u8(0x49), u8(0x81), u8(0xeb), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0xe9), u8(0xdd), u8(0xff), u8(0xff), u8(0xff),
    u8(0x4d), u8(0x29), u8(0xda), u8(0x41), u8(0x85), u8(0x02), u8(0xc3)
);

u64 append_stack_probe(address image, u64 end, u64 call_field) on cpu {
    u64 base = address_bits(image);
    patch_relative(unsafe_address(base + call_field), call_field, end);
    for (index in 0..STACK_PROBE_LENGTH) {
        unsafe_position<u8>(unsafe_address(base + end + u64(index))) = STACK_PROBE[index];
    }
    return end + u64(STACK_PROBE_LENGTH);
}

address literal_use_at(address uses,u64 index) on cpu {
    return unsafe_address(address_bits(uses)+index*size_of<LiteralUse>());
}

extern "C" bool literal_use_before(address left,address right,address context) on cpu {
    LiteralUse a=unsafe_position<LiteralUse>(left);
    LiteralUse b=unsafe_position<LiteralUse>(right);
    if(a.bits!=b.bits){return a.bits<b.bits;}
    if(a.wide!=b.wide){return a.wide<b.wide;}
    return a.ordinal<b.ordinal;
}

bool same_literal(LiteralUse left,LiteralUse right) on cpu {
    return left.bits==right.bits && left.wide==right.wide;
}

extern "C" TailLayout prepare_tail(u64 probe,address uses,u64 use_count) on cpu {
    // LiteralUse is four consecutive u64 words. The sorter moves each complete row.
    sort_rows<array<u64,4>>(uses,use_count,address(),c_function(literal_use_before));
    u64 bytes=probe!=0u64 ? u64(STACK_PROBE_LENGTH) : 0u64;
    if(use_count!=0u64){bytes+=3u64;}
    u64 literal_count=0u64;
    u64 index=0u64;
    LiteralUse previous=LiteralUse(0u64,0u64,0u64,0u64);
    while(index<use_count){
        LiteralUse item=unsafe_position<LiteralUse>(literal_use_at(uses,index));
        if(index==0u64 || !same_literal(item,previous)){
            bytes+=4u64+item.wide*4u64;
            literal_count+=1u64;
            previous=item;
        }
        index+=1u64;
    }
    return TailLayout(bytes,literal_count);
}

void tail_region(address regions,u64 ordinal,u64 end,TailRegionKind kind) on cpu {
    address target=unsafe_address(address_bits(regions)+ordinal*size_of<TailRegion>());
    unsafe_position<TailRegion>(target)=TailRegion(end,kind);
}

u64 align_literal(u64 base, u64 from) on cpu {
    u64 cursor = from;
    u64 start = ((cursor + 3u64) / 4u64) * 4u64;
    while (cursor < start) {
        put_byte(base + cursor, u8(0));
        cursor += 1u64;
    }
    return start;
}

extern "C" CodeTail write_tail(address image,u64 end,u64 probe,address uses,u64 use_count,address regions,
    address context,cfn<void,address,u64,address,u64> publish) on cpu requires nonzero(publish) {
    u64 cursor=end;
    u64 count=0u64;
    if(probe!=0u64){
        cursor=append_stack_probe(image,cursor,probe);
        tail_region(regions,count,cursor,TailRegionKind::StackProbe);
        count+=1u64;
    }
    u64 base=address_bits(image);
    if(use_count!=0u64){
        u64 start = align_literal(base, cursor);
        if (start != cursor) {
            cursor = start;
            tail_region(regions,count,cursor,TailRegionKind::Alignment);
            count+=1u64;
        }
    }
    u64 index=0u64;
    while(index<use_count){
        u64 first=index;
        address group=literal_use_at(uses,first);
        LiteralUse literal=unsafe_position<LiteralUse>(group);
        u64 start=cursor;
        write_word(base+cursor,u32(literal.bits));
        cursor+=4u64;
        if(literal.wide!=0u64){
            write_word(base+cursor,u32(literal.bits>>32));
            cursor+=4u64;
        }
        while(index<use_count){
            LiteralUse item=unsafe_position<LiteralUse>(literal_use_at(uses,index));
            if(!same_literal(literal,item)){break;}
            patch_relative(unsafe_address(base+item.field),item.field,start);
            index+=1u64;
        }
        publish(context,start,group,index-first);
        tail_region(regions,count,cursor,TailRegionKind::Literal);
        count+=1u64;
    }
    return CodeTail(cursor,count);
}

u64 register_instruction(u64 at, u16 opcode, u8 prefix, bool wide, u8 reg, u8 operand) on cpu {
    return encode_operand_instruction(at, opcode, prefix, wide, reg, move_gpr(operand));
}

void put_byte(u64 at, u8 value) on cpu {
    unsafe_position<u8>(unsafe_address(at)) = value;
}

u64 condition_field(u64 at, u8 condition) on cpu {
    put_byte(at, u8(0x0f));
    put_byte(at + 1u64, condition);
    return at + 2u64;
}

u64 jump_field(u64 at) on cpu {
    put_byte(at, u8(0xe9));
    return at + 1u64;
}

extern "C" u64 jump_to(u64 offset, u64 target, address output) on cpu {
    u64 start = address_bits(output);
    u64 field = jump_field(start);
    patch_relative(unsafe_address(field), offset + 1u64, target);
    return 5u64;
}

extern "C" void patch_branches(address image, u64 target, address fields, u64 count) on cpu {
    u64 base = address_bits(image);
    u64 cursor = address_bits(fields);
    u64 index = 0u64;
    while (index < count) {
        u64 offset = unsafe_position<u64>(unsafe_address(cursor));
        patch_relative(unsafe_address(base + offset), offset, target);
        cursor += 8u64;
        index += 1u64;
    }
}

// Deferred branches all reach the same forward rel32 target. Their fields can
// hold a backwards distance to the previous pending field until that target is known.
void patch_control_branches(address image, u64 target, u64 pending) on cpu {
    u64 base = address_bits(image);
    u64 cursor = pending;
    while (cursor != 0u64) {
        address field = unsafe_address(base + cursor);
        u32 previous = unsafe_position<u32>(field);
        patch_relative(field, cursor, target);
        cursor = previous == 0 ? 0u64 : cursor - u64(previous);
    }
}

void defer_control_branch(address emitter, address target, ControlHost host) on cpu
    requires nonzero(host.buffer) requires nonzero(host.commit) {
    NativeCodeBuffer buffer = host.buffer(emitter, 5u64);
    u64 field = buffer.end + 1u64;
    u64 previous = unsafe_position<u64>(target);
    u64 output = jump_field(address_bits(buffer.image) + buffer.end);
    write_word(output, previous == 0u64 ? 0 : u32(field - previous));
    host.commit(emitter, field + 4u64);
    unsafe_position<u64>(target) = field;
}

u64 append_control_jump(address emitter, ControlJump branch, ControlHost host) on cpu
    requires nonzero(host.buffer) requires nonzero(host.commit) {
    NativeCodeBuffer buffer = host.buffer(emitter, 9u64);
    u8 condition = branch == ControlJump::Always ? u8(0) :
        (branch == ControlJump::Zero ? u8(0x84) : u8(0x85));
    u32 test_bits = branch == ControlJump::Always ? 0 : 32;
    u64 bytes = encode_branch(condition, test_bits, unsafe_address(address_bits(buffer.image) + buffer.end));
    u64 end = buffer.end + bytes;
    host.commit(emitter, end);
    return end - 4u64;
}

void append_control_back_edge(address emitter, u64 target, ControlHost host) on cpu
    requires nonzero(host.buffer) requires nonzero(host.commit) {
    NativeCodeBuffer buffer = host.buffer(emitter, 5u64);
    u64 bytes = jump_to(buffer.end, target, unsafe_address(address_bits(buffer.image) + buffer.end));
    host.commit(emitter, buffer.end + bytes);
}

void patch_control_here(NativeCodeBuffer buffer, u64 field) on cpu {
    patch_relative(unsafe_address(address_bits(buffer.image) + field), field, buffer.end);
}

u32 emit_control(address context, address emitter, ControlInstruction instruction,
    ControlTargets targets, ControlHost host) on cpu
    requires nonzero(host.emit) requires nonzero(host.load) requires nonzero(host.buffer)
    requires nonzero(host.commit) {
    ControlKind kind = instruction.kind;
    if (kind == ControlKind::If) {
        host.load(context, emitter, instruction.register);
        Span first = instruction.first;
        Span other = instruction.second;
        ControlJump branch = ControlJump::Zero;
        if (first.count == 0u64) {
            first = instruction.second;
            other = instruction.first;
            branch = ControlJump::Nonzero;
        }
        u64 alternate = append_control_jump(emitter, branch, host);
        u32 first_status = host.emit(context, emitter, first, targets);
        if (first_status != 0) { return first_status; }
        if (other.count != 0u64) {
            u64 end = append_control_jump(emitter, ControlJump::Always, host);
            patch_control_here(host.buffer(emitter, 0u64), alternate);
            u32 other_status = host.emit(context, emitter, other, targets);
            if (other_status != 0) { return other_status; }
            patch_control_here(host.buffer(emitter, 0u64), end);
        } else {
            patch_control_here(host.buffer(emitter, 0u64), alternate);
        }
    } else if (kind == ControlKind::Loop) {
        u64 begin = host.buffer(emitter, 0u64).end;
        u32 test_status = host.emit(context, emitter, instruction.first, targets);
        if (test_status != 0) { return test_status; }
        host.load(context, emitter, instruction.register);
        u64 end = append_control_jump(emitter, ControlJump::Zero, host);
        u64 breaks = 0u64;
        ControlTargets nested = ControlTargets(targets.function, targets.block, address_of(breaks), begin);
        u32 body_status = host.emit(context, emitter, instruction.second, nested);
        if (body_status != 0) { return body_status; }
        append_control_back_edge(emitter, begin, host);
        NativeCodeBuffer buffer = host.buffer(emitter, 0u64);
        patch_control_here(buffer, end);
        patch_control_branches(buffer.image, buffer.end, breaks);
    } else if (kind == ControlKind::Scope) {
        u64 exits = 0u64;
        ControlTargets nested = ControlTargets(targets.function, address_of(exits), targets.loop_exit, targets.loop_begin);
        u32 body_status = host.emit(context, emitter, instruction.first, nested);
        if (body_status != 0) { return body_status; }
        NativeCodeBuffer buffer = host.buffer(emitter, 0u64);
        patch_control_branches(buffer.image, buffer.end, exits);
    } else if (kind == ControlKind::Continue) {
        append_control_back_edge(emitter, targets.loop_begin, host);
    } else {
        address target = targets.block;
        if (kind == ControlKind::Break) { target = targets.loop_exit; }
        if (kind == ControlKind::Raise) {
            host.load(context, emitter, instruction.register);
            target = targets.function;
        }
        defer_control_branch(emitter, target, host);
    }
    return 0;
}

void body_scalar_value(address emitter, View value, ScalarBodyHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect) requires nonzero(host.addresses.values.location)
    requires nonzero(host.addresses.values.scalar) requires nonzero(host.reserve)
    requires nonzero(host.commit) requires nonzero(host.literal) requires nonzero(host.math_symbol) {
    NativeCodeBuffer buffer = host.reserve(emitter, 128u64);
    address output = unsafe_address(address_bits(buffer.image) + buffer.end);
    ScalarValueEncoding encoded = emit_scalar_value(value, host.addresses.values, output);
    host.commit(emitter, buffer.end + encoded.length);
    if (encoded.math.length != 0) {
        host.math_symbol(emitter, buffer.end, encoded.math);
    }
    if (encoded.first.field != 0u64) {
        host.literal(emitter, buffer.end + encoded.first.field, encoded.first.bits, encoded.first.wide != 0);
    }
    if (encoded.second.field != 0u64) {
        host.literal(emitter, buffer.end + encoded.second.field, encoded.second.bits, encoded.second.wide != 0);
    }
}

ControlKind native_control_kind(u32 tag) on cpu {
    if (tag == OP_LOOP_VALUE) { return ControlKind::Loop; }
    if (tag == OP_SCOPE) { return ControlKind::Scope; }
    if (tag == OP_EXIT_VALUE) { return ControlKind::Exit; }
    if (tag == OP_BREAK_VALUE) { return ControlKind::Break; }
    if (tag == OP_CONTINUE_VALUE) { return ControlKind::Continue; }
    if (tag == OP_RAISE) { return ControlKind::Raise; }
    return ControlKind::If;
}

extern "C" u32 emit_scalar_body(address context, address emitter, Span code, ControlTargets targets, ScalarBodyHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect) requires nonzero(host.addresses.values.location)
    requires nonzero(host.addresses.values.scalar) requires nonzero(host.addresses.tile_stream)
    requires nonzero(host.local) requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.literal) requires nonzero(host.math_symbol)
    requires nonzero(host.mark) requires nonzero(host.publish) requires nonzero(host.emit)
    requires nonzero(host.control.emit) requires nonzero(host.control.load) requires nonzero(host.control.buffer)
    requires nonzero(host.control.commit) {
    u64 index = 0u64;
    while (index < code.count) {
        address instruction = unsafe_address(address_bits(code.data) + index * u64(code.stride));
        View value = host.addresses.values.ir.inspect(Node(instruction, N_INSTRUCTION));
        ScalarCursor cursor = host.mark(emitter);
        u32 kind = value.tag;
        if (kind == OP_INVOKE) {
            host.emit(emitter, ScalarLeaf::Call, value);
        } else if (kind == OP_PRINT) {
            host.emit(emitter, ScalarLeaf::Print, value);
        } else if (kind == OP_TEXTURE_SAMPLE || kind == OP_CLEANUP) {
            return u32(ScalarBodyStatus::Unlowered);
        } else if (kind <= OP_TEXTURE_FETCH) {
            host.emit(emitter, ScalarLeaf::Texture, value);
        } else if (kind == OP_APPEND) {
            NativeCodeBuffer buffer = host.reserve(emitter, 148u64 + u64(value.first.count) * 24u64);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            host.commit(emitter, buffer.end + emit_append(value, host.addresses.values, host.addresses.buffer_bytes, host.addresses.stride_field, output));
        } else if (kind == OP_ATOMIC) {
            NativeCodeBuffer buffer = host.reserve(emitter, 312u64);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            host.commit(emitter, buffer.end + emit_atomic(value, host.addresses, output));
        } else if (kind == OP_TILE_PUBLISH) {
            host.emit(emitter, ScalarLeaf::Publish, value);
        } else if (kind == OP_TILE_COLLECTIVE) {
            host.emit(emitter, ScalarLeaf::Collective, value);
        } else if (kind == OP_TILE_LENGTH) {
            host.emit(emitter, ScalarLeaf::TileLength, value);
        } else if (kind == OP_TILE_LANE) {
            host.emit(emitter, ScalarLeaf::TileLane, value);
        } else if (kind == OP_SUBGROUP) {
            return u32(ScalarBodyStatus::Subgroup);
        } else if (kind == OP_RAY_QUERY) {
            return u32(ScalarBodyStatus::RayQuery);
        } else if (kind == OP_VIEW_LOAD) {
            NativeCodeBuffer buffer = host.reserve(emitter, 112u64);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            host.commit(emitter, buffer.end + emit_view_load(value, host.addresses.values, output));
        } else if (kind == OP_LOAD_INDEXED || kind == OP_STORE_INDEXED ||
            kind == OP_TILE_LOAD || kind == OP_POSITION_ADDRESS) {
            NativeCodeBuffer buffer = host.reserve(emitter, 208u64);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            host.commit(emitter, buffer.end + emit_indexed_storage(value, host.addresses, host.local, output));
        } else if (kind == OP_FUNCTION_ADDRESS) {
            host.emit(emitter, ScalarLeaf::FunctionAddress, value);
        } else if (kind == OP_CONSTANT || (kind >= OP_BUFFER_STRIDE && kind <= OP_STORE)) {
            NativeCodeBuffer buffer = host.reserve(emitter, 68u64);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            ScalarStorageHost storage = ScalarStorageHost(host.addresses.values, host.local,
                host.addresses.buffer_bytes, host.addresses.stride_field);
            ScalarStorageEncoding encoded = emit_scalar_storage(value, storage, output);
            host.commit(emitter, buffer.end + encoded.length);
            if (encoded.field != 0u64) {
                host.literal(emitter, buffer.end + encoded.field, encoded.literal, false);
            }
        } else if (kind >= OP_UNARY && kind <= OP_SELECT_VALUE) {
            body_scalar_value(emitter, value, host);
        } else if (kind >= OP_BRANCH && kind <= OP_RAISE) {
            u32 status = emit_control(context, emitter, ControlInstruction(value.first, value.second, u64(value.a), native_control_kind(kind)), targets, host.control);
            if (status != 0) { return status; }
        } else if (kind != OP_TILE_BARRIER) {
            return u32(ScalarBodyStatus::Unlowered);
        }
        // Tile barriers are split into phases before this block is emitted.
        host.publish(emitter, instruction, cursor);
        index += 1u64;
    }
    return u32(ScalarBodyStatus::Done);
}

extern "C" u64 encode_branch(u8 condition, u32 test_bits, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (test_bits != 0) {
        cursor = register_instruction(cursor, u16(0x85), u8(0), test_bits == 64, u8(0), u8(0));
    }
    u64 field = condition == u8(0) ? jump_field(cursor) : condition_field(cursor, condition);
    write_word(field, 0);
    return field + 4u64 - start;
}

const u32 ODD_HALF_LENGTH = 12;
const array<u8,ODD_HALF_LENGTH> ODD_HALF = array<u8,ODD_HALF_LENGTH>(
    u8(0x48), u8(0x89), u8(0xc1), // mov rcx, rax
    u8(0x83), u8(0xe1), u8(1),    // and ecx, 1
    u8(0x48), u8(0xd1), u8(0xe8), // shr rax, 1
    u8(0x48), u8(0x09), u8(0xc8)  // or rax, rcx
);

u64 unsigned_to_float(u8 target, u8 prefix, address output) on cpu {
    u64 start = address_bits(output);
    put_byte(start, u8(0x48));
    put_byte(start + 1u64, u8(0x85));
    put_byte(start + 2u64, u8(0xc0)); // test rax, rax
    u64 low = condition_field(start + 3u64, u8(0x89)); // jns
    u64 cursor = low + 4u64;
    // Preserve the sticky bit before conversion so doubling rounds correctly.
    for (index in 0..ODD_HALF_LENGTH) {
        put_byte(cursor + u64(index), ODD_HALF[index]);
    }
    cursor += u64(ODD_HALF_LENGTH);
    cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
    cursor = register_instruction(cursor, u16(0x2a0f), prefix, true, target, u8(0));
    cursor = register_instruction(cursor, u16(0x580f), prefix, false, target, target);
    u64 done = jump_field(cursor);
    cursor = done + 4u64;
    patch_relative(unsafe_address(low), low, cursor);
    cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
    cursor = register_instruction(cursor, u16(0x2a0f), prefix, true, target, u8(0));
    patch_relative(unsafe_address(done), done, cursor);
    return cursor - start;
}

u64 float_to_unsigned(u8 target, u8 prefix, address output) on cpu {
    u64 start = address_bits(output);
    u8 compare_prefix = u8(0);
    if (prefix == u8(0xf2)) {
        compare_prefix = u8(0x66);
    }
    u64 cursor = register_instruction(start, u16(0x2e0f), compare_prefix, false, u8(0), u8(1));
    u64 low = condition_field(cursor, u8(0x82)); // jb
    cursor = low + 4u64;
    // Subtract 2^63 for the signed conversion and restore the high bit.
    cursor = register_instruction(cursor, u16(0x5c0f), prefix, false, u8(0), u8(1));
    cursor = register_instruction(cursor, u16(0x2c0f), prefix, true, target, u8(0));
    cursor = register_instruction(cursor, u16(0xba0f), u8(0), true, u8(5), target);
    put_byte(cursor, u8(63));
    u64 done = jump_field(cursor + 1u64);
    cursor = done + 4u64;
    patch_relative(unsafe_address(low), low, cursor);
    cursor = register_instruction(cursor, u16(0x2c0f), prefix, true, target, u8(0));
    patch_relative(unsafe_address(done), done, cursor);
    return cursor - start;
}

const u8 DIVISION_SHIFT = u8(0);
const u8 DIVISION_MULTIPLY = u8(1);
const u8 DIVISION_MULTIPLY_ADD = u8(2);

DivisionPlan division_plan(u32 divisor, bool signed) on cpu requires nonzero(divisor) {
    u32 magnitude = divisor;
    if (signed && i32(divisor) < i32(0)) { magnitude = u32(-i64(i32(divisor))); }
    u32 high = magnitude;
    u32 shift = 0;
    if (high >= 0x10000) { high >>= 16; shift += 16; }
    if (high >= 0x100) { high >>= 8; shift += 8; }
    if (high >= 0x10) { high >>= 4; shift += 4; }
    if (high >= 4) { high >>= 2; shift += 2; }
    if (high >= 2) { shift += 1; }
    if ((magnitude & (magnitude - 1)) == 0) {
        return DivisionPlan(0, u8(shift), DIVISION_SHIFT);
    }

    u64 d = u64(magnitude);
    u64 power = 1u64 << u64(shift);
    u64 numerator = 0x100000000u64 * power;
    u64 quotient = numerator / d;
    u64 remainder = numerator % d;
    // A non-power-of-two magnitude cannot divide the power-of-two numerator.
    if (d - remainder < power) {
        return DivisionPlan(u32(quotient + 1u64), u8(shift), DIVISION_MULTIPLY);
    }

    // Compute ceil(2 * numerator / d) from the quotient and remainder.
    // The multiplier takes the low 32 bits after dropping the implicit high bit.
    u64 ceil_twice = 2u64 * quotient + 1u64;
    if (2u64 * remainder > d) {
        ceil_twice += 1u64;
    }
    return DivisionPlan(u32(ceil_twice), u8(shift), DIVISION_MULTIPLY_ADD);
}

const u8 MOVE_GPR = u8(0);
const u8 MOVE_XMM = u8(1);
const u8 MOVE_MEMORY = u8(2);
const u8 MOVE_IMMEDIATE = u8(3);
const u8 RIP_RELATIVE = u8(3);

MoveLocation move_gpr(u8 register) on cpu {
    return MoveLocation(0u64, MOVE_GPR, register, NO_INDEX, u8(0));
}

u8 move_temporary(MoveLocation memory) on cpu {
    if (memory.base != u8(0) && memory.index != u8(0)) { return u8(0); }
    if (memory.base != u8(1) && memory.index != u8(1)) { return u8(1); }
    return u8(2);
}

u64 encode_operand_instruction(u64 at, u16 opcode, u8 prefix, bool wide, u8 reg, MoveLocation operand) on cpu {
    u8 form = DIRECT_REGISTER;
    if (operand.kind == MOVE_MEMORY) { form = MEMORY; }
    if (operand.kind == MOVE_IMMEDIATE) { form = RIP_RELATIVE; }
    Encoding input = Encoding(u32(operand.value), reg, operand.base, operand.index, operand.shift, form, prefix, u16(wide));
    u8 first = u8(opcode);
    u8 second = u8(opcode >> 8);
    u64 cursor = instruction_prefix(input, uses_byte_register(first, second), at);
    put_byte(cursor, first);
    cursor += 1u64;
    if (second != u8(0)) {
        put_byte(cursor, second);
        cursor += 1u64;
    }
    return instruction_operand(input, cursor);
}

u64 immediate_register_field(u64 at, u8 register, bool full) on cpu {
    Encoding input = Encoding(0, u8(0), register, NO_INDEX, u8(0), OPCODE_REGISTER, u8(0), u16(full));
    u64 opcode = instruction_prefix(input, false, at);
    put_byte(opcode, u8(0xb8) | (register & u8(7)));
    return opcode + 1u64;
}

u64 move_immediate(u64 at, u8 register, u64 value, bool wide) on cpu {
    bool full = wide && value > 0xffffffffu64;
    u64 cursor = immediate_register_field(at, register, full);
    write_word(cursor, u32(value));
    cursor += 4u64;
    if (full) {
        write_word(cursor, u32(value >> 32));
        cursor += 4u64;
    }
    return cursor;
}

u64 load_extended(u64 at, u8 dst, MoveLocation src, u32 size, bool signed) on cpu {
    u16 opcode = u16(0xb60f);
    if (signed) { opcode = u16(0xbe0f); }
    if (size == 2) { opcode += u16(0x100); }
    return encode_operand_instruction(at, opcode, u8(0), false, dst, src);
}

u64 load_bits(u64 at, u8 dst, MoveLocation src, bool wide) on cpu {
    return encode_operand_instruction(at, u16(0x8b), u8(0), wide, dst, src);
}

u64 load_float(u64 at, u8 dst, MoveLocation src, bool wide) on cpu {
    u16 opcode = u16(0x100f);
    if (wide) { opcode = u16(0x7e0f); }
    return encode_operand_instruction(at, opcode, u8(0xf3), false, dst, src);
}

u64 store_bits(u64 at, MoveLocation dst, u8 src, u32 size) on cpu {
    u16 opcode = u16(0x89);
    u8 prefix = u8(0);
    if (size == 1) { opcode = u16(0x88); }
    if (size == 2) { prefix = u8(0x66); }
    return encode_operand_instruction(at, opcode, prefix, size == 8, src, dst);
}

u64 store_float(u64 at, MoveLocation dst, u8 src, bool wide) on cpu {
    if (wide) { return encode_operand_instruction(at, u16(0xd60f), u8(0x66), false, src, dst); }
    return encode_operand_instruction(at, u16(0x110f), u8(0xf3), false, src, dst);
}

u64 call_operand(u64 at, MoveLocation target) on cpu {
    return encode_operand_instruction(at, u16(0xff), u8(0), false, u8(2), target);
}

extern "C" SymbolEncoding symbol_target(bool invoke, address output) on cpu {
    u64 start = address_bits(output);
    u64 field = immediate_register_field(start, u8(0), true);
    write_word(field, 0);
    write_word(field + 4u64, 0);
    u64 end = field + 8u64;
    if (invoke) {
        end = call_operand(end, move_gpr(u8(0)));
    }
    return SymbolEncoding(end - start, field - start);
}

extern "C" SymbolEncoding import_transfer(bool tail, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation slot = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    u64 end = encode_operand_instruction(start, u16(0xff), u8(0), false, tail ? u8(4) : u8(2), slot);
    return SymbolEncoding(end - start, end - start - 4u64);
}

extern "C" ApplicationEntry application_entry(address output) on cpu {
    u64 start = address_bits(output);
    // Four Win64 home slots plus alignment at the process entry.
    u8 frame = u8(40);
    u64 prolog = register_instruction(start, u16(0x83), u8(0), true, u8(5), u8(4));
    put_byte(prolog, frame);
    prolog += 1u64;

    put_byte(prolog, u8(0xe8));
    u64 main_field = prolog + 1u64;
    write_word(main_field, 0);

    // Pass main's return value in ECX to ExitProcess.
    u64 exit = store_bits(main_field + 4u64, move_gpr(u8(1)), u8(0), 4);
    SymbolEncoding call = import_transfer(false, unsafe_address(exit));
    return ApplicationEntry(u32(exit - start + call.length), u32(frame), u32(prolog - start),
                            u32(main_field - start), u32(exit - start + call.field));
}

extern "C" ModuleFactoryCode module_factory_code(u64 bytes, u64 codes, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation data = MoveLocation(0u64, MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    MoveLocation size = MoveLocation(8u64, MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    MoveLocation code = MoveLocation(16u64, MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    MoveLocation count = MoveLocation(24u64, MOVE_MEMORY, u8(1), NO_INDEX, u8(0));

    SymbolEncoding image = symbol_target(false, output);
    u64 cursor = store_bits(start + image.length, data, u8(0), 8);
    cursor = store_bits(move_immediate(cursor, u8(0), bytes, true), size, u8(0), 8);
    u64 ranges = cursor;
    SymbolEncoding table = symbol_target(false, unsafe_address(ranges));
    cursor = store_bits(ranges + table.length, code, u8(0), 8);
    cursor = store_bits(move_immediate(cursor, u8(0), codes, true), count, u8(0), 8);
    cursor = store_bits(cursor, move_gpr(u8(0)), u8(1), 8);
    put_byte(cursor, u8(0xc3));
    return ModuleFactoryCode(cursor + 1u64 - start, image.field, ranges + table.field - start);
}

u64 resource_tail_field(u64 at) on cpu {
    u64 field = jump_field(at);
    write_word(field, 0);
    return field;
}

extern "C" FrameCreateCode frame_create_code(address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation slot = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    // Move on_error and context ahead of the linker-owned factory table and count.
    u64 cursor = store_bits(start, move_gpr(u8(9)), u8(2), 8);
    cursor = store_bits(cursor, move_gpr(u8(8)), u8(1), 8);
    u64 count_end = load_bits(cursor, u8(2), slot, true);
    u64 factory_end = encode_operand_instruction(count_end, u16(0x8d), u8(0), true, u8(1), slot);
    u64 target = resource_tail_field(factory_end) - start;
    return FrameCreateCode(target + 4u64, factory_end - start - 4u64,
                           count_end - start - 4u64, target);
}

extern "C" ResourceTailCode resource_allocate_code(u32 index, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation slot = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    u64 cursor = store_bits(start, move_gpr(u8(9)), u8(8), 4);
    u64 base_end = load_bits(cursor, u8(8), slot, false);
    cursor = base_end;
    if (index != 0) {
        cursor = register_instruction(cursor, u16(0x81), u8(0), false, u8(0), u8(8));
        write_word(cursor, index);
        cursor += 4u64;
    }
    u64 target = resource_tail_field(cursor) - start;
    return ResourceTailCode(target + 4u64, base_end - start - 4u64, target);
}

extern "C" ResourceTailCode task_start_code(address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation slot = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    // The sixth Win64 argument follows the return address, four home slots and argument five.
    MoveLocation index = MoveLocation(48u64, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    u64 base_end = load_bits(start, u8(0), slot, false);
    u64 cursor = encode_operand_instruction(base_end, u16(0x01), u8(0), false, u8(0), index);
    u64 target = resource_tail_field(cursor) - start;
    return ResourceTailCode(target + 4u64, base_end - start - 4u64, target);
}

// These records are the existing blink_run C layout, not program-owned data.
struct ProgramDatasetBinding { u32 handle; u32 offset; };
struct ProgramUniformWord { u32 tag; u64 bits; };

extern "C" ProgramCallLayout program_call_layout(u32 datasets, u32 values, u32 textures) on cpu {
    // Four home slots and the fifth blink_run argument.
    u64 data_start = 40u64;
    u64 values_start = data_start + u64(datasets) * size_of<ProgramDatasetBinding>();
    u64 textures_start = values_start + u64(values) * size_of<ProgramUniformWord>();
    u64 end = textures_start + u64(textures) * 8u64;
    u64 frame = ((end + 7u64) / 16u64) * 16u64 + 8u64;
    u64 arguments = u64(datasets) * 2u64 + u64(values) + u64(textures);
    // Win64 memory displacements and PE relative code addresses are signed 32-bit.
    u64 capacity = 160u64 + u64(datasets) * 32u64 + u64(values) * 48u64 + u64(textures) * 16u64;
    if (frame + arguments * 8u64 + 8u64 > 2147483647u64 || capacity > 2147483647u64) {
        return ProgramCallLayout(0, 0, 0, 0, 0u64);
    }
    return ProgramCallLayout(u32(frame), u32(data_start), u32(values_start), u32(textures_start), capacity);
}

MoveLocation resource_stack(u64 offset) on cpu {
    return MoveLocation(offset, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
}

MoveLocation program_argument(u64 slot, u32 frame, bool float_register) on cpu {
    if (slot >= 4u64) { return resource_stack(u64(frame) + 8u64 + slot * 8u64); }
    if (float_register) { return MoveLocation(0u64, MOVE_XMM, u8(slot), NO_INDEX, u8(0)); }
    return move_gpr(argument_gpr(u8(slot)));
}

u64 resource_stack_address(u64 at, u8 register, u32 offset) on cpu {
    return encode_operand_instruction(at, u16(0x8d), u8(0), true, register, resource_stack(u64(offset)));
}

extern "C" ProgramCallCode program_call_code(ProgramCallLayout layout, u32 program_index,
    u32 datasets, address arguments, u32 values, u32 textures, address output) on cpu {
    u64 start = address_bits(output);
    FrameEncoding prolog = allocate_frame(layout.frame, output);
    u64 cursor = start + prolog.length;

    // RCX already holds the Frame. Consume the other incoming registers before replacing them.
    u64 slot = 1u64;
    for (index in 0..datasets) {
        u64 field = u64(layout.datasets) + u64(index) * size_of<ProgramDatasetBinding>();
        cursor = copy_bits(cursor, resource_stack(field), program_argument(slot, layout.frame, false), 4);
        cursor = copy_bits(cursor, resource_stack(field + 4u64), program_argument(slot + 1u64, layout.frame, false), 4);
        slot += 2u64;
    }
    for (index in 0..values) {
        ProgramUniformArgument argument = unsafe_position<ProgramUniformArgument>(
            unsafe_address(address_bits(arguments) + u64(index) * size_of<ProgramUniformArgument>()));
        u64 field = u64(layout.values) + u64(index) * size_of<ProgramUniformWord>();
        MoveLocation source = program_argument(slot, layout.frame, argument.scalar.kind == ScalarKind::Float);
        MoveLocation accumulator = move_gpr(u8(0));
        u32 width = argument.scalar.width / 8;
        if (argument.scalar.kind == ScalarKind::Boolean) {
            cursor += read_scalar_move(accumulator, source, 4, false, true, unsafe_address(cursor));
        } else if (width < 4) {
            cursor = load_extended(cursor, u8(0), source, width, false);
        } else {
            cursor = copy_bits(cursor, accumulator, source, width);
        }
        cursor = store_immediate(cursor, resource_stack(field), u64(argument.tag), 4);
        cursor = store_bits(cursor, resource_stack(field + 8u64), u8(0), 8);
        slot += 1u64;
    }
    for (index in 0..textures) {
        u64 field = u64(layout.textures) + u64(index) * 8u64;
        cursor = copy_bits(cursor, resource_stack(field), program_argument(slot, layout.frame, false), 8);
        slot += 1u64;
    }

    MoveLocation base = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(2), base, false);
    u64 base_field = cursor - start - 4u64;
    if (program_index != 0) { cursor = integer_immediate(cursor, u8(0), u8(2), u64(program_index), false); }
    cursor = resource_stack_address(cursor, u8(8), layout.datasets);
    cursor = resource_stack_address(cursor, u8(9), layout.values);
    cursor = resource_stack_address(cursor, u8(0), layout.textures);
    cursor = store_bits(cursor, resource_stack(32u64), u8(0), 8);
    put_byte(cursor, u8(0xe8));
    u64 call_field = cursor + 1u64 - start;
    write_word(cursor + 1u64, 0);
    cursor += 5u64;
    CompletionCode body = finish_resource_call(output, cursor, layout.frame, prolog);
    return ProgramCallCode(body.length, body.function_end, body.prolog, base_field, call_field);
}


CompletionCode finish_resource_call(address output, u64 at, u32 frame, FrameEncoding prolog) on cpu {
    u64 cursor = at + release_frame(frame, unsafe_address(at));
    put_byte(cursor, u8(0xc3));
    u64 end = cursor + 1u64 - address_bits(output);
    u64 length = end;
    if (prolog.probe_field != 0u64) { length = append_stack_probe(output, end, prolog.probe_field); }
    return CompletionCode(length, u32(end), u32(prolog.length));
}

CompletionField completion_at(address fields, u32 index) on cpu {
    return unsafe_position<CompletionField>(
        unsafe_address(address_bits(fields) + u64(index) * size_of<CompletionField>()));
}

CompletionLimit completion_limit(CompletionField field, u32 index) on cpu {
    return unsafe_position<CompletionLimit>(
        unsafe_address(address_bits(field.limits) + u64(index) * size_of<CompletionLimit>()));
}

extern "C" CompletionLayout completion_layout(address fields, u32 count) on cpu {
    u64 max_limits = 0u64;
    u64 incoming = 32u64;
    u64 capacity = 160u64 + u64(count) * 160u64;
    for (index in 0..count) {
        CompletionField field = completion_at(fields, index);
        u64 limits = u64(field.limit_count);
        if (limits > max_limits) { max_limits = limits; }
        capacity += limits * 64u64;
        for (i in 0..field.limit_count) {
            u64 end = 32u64 + u64(completion_limit(field, i).parameter) * 8u64;
            if (end > incoming) { incoming = end; }
        }
    }
    // Nine outgoing arguments occupy 72 bytes. Actual limit values follow.
    u64 end = 72u64 + max_limits * 4u64;
    u64 frame = ((end + 7u64) / 16u64) * 16u64 + 8u64;
    if (frame + incoming > 2147483647u64 || capacity > 2147483647u64) {
        return CompletionLayout(0, 0u64);
    }
    return CompletionLayout(u32(frame), capacity);
}

u64 completion_value(u64 at, u64 slot, u64 bits) on cpu {
    u64 cursor = move_immediate(at, u8(0), bits, true);
    return store_bits(cursor, resource_stack(slot), u8(0), 8);
}

u64 completion_limits(u64 at, u64 incoming, CompletionField field) on cpu {
    u64 cursor = register_instruction(at, u16(0x31), u8(0), false, u8(10), u8(10));
    MoveLocation output = MoveLocation(72u64, MOVE_MEMORY, u8(4), u8(10), u8(2));
    for (i in 0..field.limit_count) {
        CompletionLimit limit = completion_limit(field, i);
        u64 input = incoming + 32u64 + u64(limit.parameter) * 8u64;
        cursor = load_bits(cursor, u8(0), resource_stack(input), false);

        u64 omit = 0u64;
        if (limit.distance == 0) {
            // Inclusive u32::MAX has no representable exclusive upper bound.
            cursor = integer_immediate(cursor, u8(0), u8(0), 1u64, false);
            omit = condition_field(cursor, u8(0x82));
            cursor = omit + 4u64;
        } else if (limit.distance > 1) {
            // Exclusive distance saturates at zero.
            cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(11), u8(11));
            cursor = integer_immediate(cursor, u8(5), u8(0), u64(limit.distance - 1), false);
            cursor = register_instruction(cursor, u16(0x420f), u8(0), false, u8(0), u8(11));
        }
        cursor = store_bits(cursor, output, u8(0), 4);
        cursor = integer_immediate(cursor, u8(0), u8(10), 1u64, false);
        if (omit != 0u64) { patch_relative(unsafe_address(omit), omit, cursor); }
    }
    return cursor;
}

extern "C" CompletionCode completion_code(CompletionLayout layout, address fields, u32 count,
    address output, address patches) on cpu {
    u64 start = address_bits(output);
    FrameEncoding prolog = allocate_frame(layout.frame, output);
    u64 cursor = start + prolog.length;
    u64 incoming = u64(layout.frame);
    cursor = store_bits(cursor, resource_stack(incoming + 8u64), u8(1), 8);
    cursor = store_bits(cursor, resource_stack(incoming + 16u64), u8(2), 4);
    cursor = store_bits(cursor, resource_stack(incoming + 24u64), u8(8), 4);
    cursor = store_bits(cursor, resource_stack(incoming + 32u64), u8(9), 4);

    for (index in 0..count) {
        CompletionField field = completion_at(fields, index);
        cursor = completion_limits(cursor, incoming, field);
        cursor = completion_value(cursor, 32u64, u64(field.interval_count));
        cursor = resource_stack_address(cursor, u8(0), 72);
        cursor = store_bits(cursor, resource_stack(40u64), u8(0), 8);
        cursor = store_bits(cursor, resource_stack(48u64), u8(10), 8);
        cursor = completion_value(cursor, 56u64, field.known_mask);
        cursor = completion_value(cursor, 64u64, field.known_value);
        cursor = load_bits(cursor, u8(1), resource_stack(incoming + 8u64), true);
        cursor = load_bits(cursor, u8(2), resource_stack(incoming + 16u64), false);
        cursor = load_bits(cursor, u8(8), resource_stack(incoming + 24u64), false);
        if (field.offset != 0) { cursor = integer_immediate(cursor, u8(0), u8(8), u64(field.offset), false); }
        MoveLocation address_slot = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(9), address_slot);
        u64 interval_field = cursor - start - 4u64;
        put_byte(cursor, u8(0xe8));
        u64 call_field = cursor + 1u64 - start;
        write_word(cursor + 1u64, 0);
        cursor += 5u64;
        unsafe_position<CompletionPatch>(unsafe_address(address_bits(patches)
            + u64(index) * size_of<CompletionPatch>())) = CompletionPatch(interval_field, call_field);
    }
    return finish_resource_call(output, cursor, layout.frame, prolog);
}

LibraryEntryCode library_call(address output, u64 at, u32 frame, FrameEncoding prolog) on cpu {
    put_byte(at, u8(0xe8));
    write_word(at + 1u64, 0);
    CompletionCode body = finish_resource_call(output, at + 5u64, frame, prolog);
    return LibraryEntryCode(body.length, body.function_end, body.prolog, frame, at + 1u64 - address_bits(output));
}

extern "C" LibraryCreateCode library_create_code(u32 has_cpu, u32 has_gpu, u32 has_factories, address output) on cpu {
    u64 start = address_bits(output);
    u32 frame = 72;
    FrameEncoding prolog = allocate_frame(frame, output);
    u64 cursor = start + prolog.length;
    // Keep the host's out/error/capacity before reusing RDX, R8 and R9.
    cursor = store_bits(cursor, resource_stack(40u64), u8(2), 8);
    cursor = store_bits(cursor, resource_stack(48u64), u8(8), 8);
    cursor = store_bits(cursor, resource_stack(56u64), u8(9), 8);

    MoveLocation rip = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    u64 cpu = 0u64;
    u64 gpu = 0u64;
    u64 factories = 0u64;
    u64 count = 0u64;
    if (has_cpu != 0) {
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(2), rip);
        cpu = cursor - start - 4u64;
    } else {
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(2), u8(2));
    }
    if (has_gpu != 0) {
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(8), rip);
        gpu = cursor - start - 4u64;
    } else {
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(8), u8(8));
    }
    if (has_factories != 0) {
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(9), rip);
        factories = cursor - start - 4u64;
        cursor = load_bits(cursor, u8(0), rip, true);
        count = cursor - start - 4u64;
    } else {
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(9), u8(9));
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(0), u8(0));
    }
    cursor = store_bits(cursor, resource_stack(32u64), u8(0), 8);
    return LibraryCreateCode(library_call(output, cursor, frame, prolog), cpu, gpu, factories, count);
}

struct LibraryBuffer { address data; u32 count; };

extern "C" LibraryRunLayout library_run_layout(u32 datasets, u32 dataset_addresses, u32 uniforms) on cpu {
    // Home slots and the fifth argument, followed by the two borrowed arrays.
    u64 uniform_start = 40u64 + u64(datasets) * size_of<LibraryBuffer>();
    u64 end = uniform_start + u64(uniforms) * 8u64;
    u64 frame = ((end + 7u64) / 16u64) * 16u64 + 8u64;
    u64 arguments = 3u64 + u64(datasets) + u64(dataset_addresses) + u64(uniforms);
    u64 capacity = 160u64 + u64(datasets) * 32u64 + u64(uniforms) * 16u64;
    if (frame + arguments * 8u64 > 2147483647u64 || capacity > 2147483647u64) {
        return LibraryRunLayout(0, 0, 0u64);
    }
    return LibraryRunLayout(u32(frame), u32(uniform_start), capacity);
}

extern "C" LibraryEntryCode library_run_code(LibraryRunLayout layout, address dataset_addresses,
    u32 datasets, u32 uniforms, address output) on cpu {
    u64 start = address_bits(output);
    FrameEncoding prolog = allocate_frame(layout.frame, output);
    u64 cursor = start + prolog.length;
    u64 slot = 1u64;
    for (index in 0..datasets) {
        u64 offset = 40u64 + u64(index) * size_of<LibraryBuffer>();
        u8 has_address = unsafe_position<u8>(unsafe_address(address_bits(dataset_addresses) + u64(index)));
        if (has_address != u8(0)) {
            cursor = copy_bits(cursor, resource_stack(offset), program_argument(slot, layout.frame, false), 8);
            slot += 1u64;
        } else {
            cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(0), u8(0));
            cursor = store_bits(cursor, resource_stack(offset), u8(0), 8);
        }
        cursor = copy_bits(cursor, resource_stack(offset + 8u64), program_argument(slot, layout.frame, false), 4);
        slot += 1u64;
    }
    for (index in 0..uniforms) {
        u64 offset = u64(layout.uniforms) + u64(index) * 8u64;
        cursor = copy_bits(cursor, resource_stack(offset), program_argument(slot, layout.frame, false), 8);
        slot += 1u64;
    }
    cursor = copy_bits(cursor, resource_stack(32u64), program_argument(slot + 1u64, layout.frame, false), 8);
    cursor = copy_bits(cursor, move_gpr(u8(9)), program_argument(slot, layout.frame, false), 8);
    cursor = resource_stack_address(cursor, u8(2), 40);
    cursor = resource_stack_address(cursor, u8(8), layout.uniforms);
    return library_call(output, cursor, layout.frame, prolog);
}

extern "C" SymbolEncoding library_destroy_code(address output) on cpu {
    u64 start = address_bits(output);
    u64 field = resource_tail_field(start) - start;
    return SymbolEncoding(field + 4u64, field);
}

extern "C" u64 call_slot(u32 offset, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation target = MoveLocation(u64(offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    return call_operand(start, target) - start;
}

// Positions zero through three are RCX, RDX, R8, and R9.
u8 argument_gpr(u8 slot) on cpu {
    return slot + (slot < u8(2) ? u8(1) : u8(6));
}

extern "C" u64 argument_register(u8 slot, u32 float_size, bool to_home, address output) on cpu {
    u64 start = address_bits(output);
    u64 offset = u64(slot) * 8u64 + (to_home ? 8u64 : 0u64);
    MoveLocation home = MoveLocation(offset, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    if (float_size != 0) {
        u8 prefix = float_size == 8 ? u8(0xf2) : u8(0xf3);
        u16 opcode = to_home ? u16(0x110f) : u16(0x100f);
        return encode_operand_instruction(start, opcode, prefix, false, slot, home) - start;
    }
    u8 register = argument_gpr(slot);
    if (to_home) {
        return store_bits(start, home, register, 8) - start;
    }
    return load_bits(start, register, home, true) - start;
}

u64 store_immediate(u64 at, MoveLocation dst, u64 value, u32 size) on cpu {
    u16 opcode = u16(0xc7);
    u8 prefix = u8(0);
    if (size == 1) { opcode = u16(0xc6); }
    if (size == 2) { prefix = u8(0x66); }
    u64 cursor = encode_operand_instruction(at, opcode, prefix, false, u8(0), dst);
    for (byte in 0..size) {
        put_byte(cursor + u64(byte), u8(value >> u64(byte * 8)));
    }
    return cursor + u64(size);
}

u64 copy_small(u64 at, MoveLocation dst, MoveLocation src, u32 size, bool signed) on cpu {
    if (dst.kind == MOVE_MEMORY) {
        return at + write_scalar_move(dst, src, size, unsafe_address(at));
    }
    u8 target = dst.kind == MOVE_GPR ? dst.base : u8(0);
    if (src.kind == MOVE_IMMEDIATE) {
        u32 sign = size == 2 ? 0x8000 : 0x80;
        u32 bits = u32(src.value) & (2 * sign - 1);
        if (signed) { bits = (bits ^ sign) - sign; }
        return move_immediate(at, target, u64(bits), false);
    }
    return load_extended(at, target, src, size, signed);
}

u64 copy_bits(u64 at, MoveLocation dst, MoveLocation src, u32 size) on cpu {
    bool wide = size == 8;
    if (src.kind == MOVE_IMMEDIATE) {
        if (dst.kind == MOVE_GPR) { return move_immediate(at, dst.base, src.value, wide); }
        if (dst.kind == MOVE_XMM) {
            if (!wide) { return load_float(at, dst.base, src, false); }
            u64 cursor = move_immediate(at, u8(0), src.value, true);
            return encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), true, dst.base, move_gpr(u8(0)));
        }
        if (!wide) { return store_immediate(at, dst, src.value, 4); }
        u64 cursor = move_immediate(at, u8(0), src.value, true);
        return store_bits(cursor, dst, u8(0), size);
    }
    if (dst.kind == MOVE_GPR) {
        if (src.kind == MOVE_XMM) {
            return encode_operand_instruction(at, u16(0x7e0f), u8(0x66), wide, src.base, dst);
        }
        return load_bits(at, dst.base, src, wide);
    }
    if (dst.kind == MOVE_XMM) {
        if (src.kind == MOVE_GPR) {
            return encode_operand_instruction(at, u16(0x6e0f), u8(0x66), wide, dst.base, src);
        }
        if (src.kind == MOVE_XMM) {
            return encode_operand_instruction(at, u16(0x280f), u8(0), false, dst.base, src);
        }
        return load_float(at, dst.base, src, wide);
    }
    if (src.kind == MOVE_XMM) { return store_float(at, dst, src.base, wide); }
    if (src.kind == MOVE_GPR) { return store_bits(at, dst, src.base, size); }
    u64 cursor = load_bits(at, u8(0), src, wide);
    return store_bits(cursor, dst, u8(0), size);
}

extern "C" MoveEncoding copy_scalar_move(MoveLocation dst, MoveLocation src, u32 size, bool signed, address output) on cpu {
    u64 start = address_bits(output);
    if (dst.kind == MOVE_IMMEDIATE) { return MoveEncoding(0u64, 0u64); }
    if (size < 4) { return MoveEncoding(copy_small(start, dst, src, size, signed) - start, 0u64); }
    if (dst.kind == src.kind && dst.value == src.value && dst.base == src.base) {
        return MoveEncoding(0u64, 0u64);
    }
    u64 length = copy_bits(start, dst, src, size) - start;
    u64 literal = 0u64;
    if (dst.kind == MOVE_XMM && src.kind == MOVE_IMMEDIATE && size == 4) { literal = length - 4u64; }
    return MoveEncoding(length, literal);
}

extern "C" SymbolEncoding print_call(MoveLocation dst, MoveLocation src, ScalarEncoding value,
    ScalarEncoding result, address output) on cpu {
    u64 start = address_bits(output);
    MoveEncoding argument = copy_scalar_move(move_gpr(u8(1)), src, value.width / 8,
        value.kind == ScalarKind::Signed, output);
    u64 cursor = start + argument.length;
    if (value.kind == ScalarKind::Float) {
        cursor = move_immediate(cursor, u8(2), u64(value.width), false);
    }
    SymbolEncoding call = symbol_target(true, unsafe_address(cursor));
    MoveEncoding status = copy_scalar_move(dst, move_gpr(u8(0)), result.width / 8,
        result.kind == ScalarKind::Signed, unsafe_address(cursor + call.length));
    return SymbolEncoding(cursor + call.length + status.length - start, cursor + call.field - start);
}

extern "C" u64 read_scalar_move(MoveLocation dst, MoveLocation memory, u32 size, bool signed, bool boolean, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (size < 4) {
        u8 target = dst.base;
        if (dst.kind != MOVE_GPR) { target = move_temporary(memory); }
        cursor = load_extended(cursor, target, memory, size, signed);
        if (dst.kind == MOVE_MEMORY) { cursor = store_bits(cursor, dst, target, size); }
    } else if (boolean) {
        u8 temporary = move_temporary(memory);
        cursor = encode_operand_instruction(cursor, u16(0x83), u8(0), false, u8(7), memory);
        put_byte(cursor, u8(0));
        put_byte(cursor + 1u64, u8(0x0f));
        put_byte(cursor + 2u64, u8(0x95));
        put_byte(cursor + 3u64, u8(0xc0) | temporary);
        MoveLocation value = move_gpr(temporary);
        cursor = load_extended(cursor + 4u64, temporary, value, 1, false);
        if (dst.kind != MOVE_IMMEDIATE && (dst.kind != MOVE_GPR || dst.base != temporary)) {
            cursor = copy_bits(cursor, dst, value, 4);
        }
    } else if (dst.kind == MOVE_GPR) {
        cursor = load_bits(cursor, dst.base, memory, size == 8);
    } else if (dst.kind == MOVE_XMM) {
        cursor = load_float(cursor, dst.base, memory, size == 8);
    } else {
        u8 temporary = move_temporary(memory);
        cursor = load_bits(cursor, temporary, memory, size == 8);
        if (dst.kind == MOVE_MEMORY) { cursor = store_bits(cursor, dst, temporary, size); }
    }
    return cursor - start;
}

extern "C" u64 write_scalar_move(MoveLocation memory, MoveLocation src, u32 size, address output) on cpu {
    u64 start = address_bits(output);
    if (src.kind == MOVE_IMMEDIATE && size != 8) {
        return store_immediate(start, memory, src.value, size) - start;
    }
    if (src.kind == MOVE_XMM && size >= 4) {
        return store_float(start, memory, src.base, size == 8) - start;
    }
    u8 register = src.base;
    u64 cursor = start;
    if (src.kind != MOVE_GPR) {
        register = move_temporary(memory);
        if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, register, src.value, true);
        } else if (src.kind == MOVE_XMM) {
            cursor = encode_operand_instruction(cursor, u16(0x7e0f), u8(0x66), false, src.base, move_gpr(register));
        } else if (size < 4) {
            cursor = load_extended(cursor, register, src, size, false);
        } else {
            cursor = load_bits(cursor, register, src, size == 8);
        }
    }
    return store_bits(cursor, memory, register, size) - start;
}

u64 comparison_result(u64 at, MoveLocation dst, BinaryOperation operation, bool signed, bool floating) on cpu {
    u8 condition = u8(0x94);
    if (operation == BinaryOperation::NotEqual) { condition = u8(0x95); }
    if (operation == BinaryOperation::Less) { condition = u8(0x92); }
    if (operation == BinaryOperation::Greater) { condition = u8(0x97); }
    if (operation == BinaryOperation::LessEqual) { condition = u8(0x96); }
    if (operation == BinaryOperation::GreaterEqual) { condition = u8(0x93); }
    if (signed && !floating) {
        if (operation == BinaryOperation::Less) { condition = u8(0x9c); }
        if (operation == BinaryOperation::Greater) { condition = u8(0x9f); }
        if (operation == BinaryOperation::LessEqual) { condition = u8(0x9e); }
        if (operation == BinaryOperation::GreaterEqual) { condition = u8(0x9d); }
    }
    put_byte(at, u8(0x0f));
    put_byte(at + 1u64, condition);
    put_byte(at + 2u64, u8(0xc0));
    u64 cursor = at + 3u64;
    if (floating) {
        u8 parity = u8(0x9b);
        u8 combine = u8(0x20);
        if (operation == BinaryOperation::NotEqual) {
            parity = u8(0x9a);
            combine = u8(0x08);
        }
        put_byte(cursor, u8(0x0f));
        put_byte(cursor + 1u64, parity);
        put_byte(cursor + 2u64, u8(0xc2));
        put_byte(cursor + 3u64, combine);
        put_byte(cursor + 4u64, u8(0xd0));
        cursor += 5u64;
    }
    u8 target = u8(0);
    if (dst.kind == MOVE_GPR) { target = dst.base; }
    cursor = load_extended(cursor, target, move_gpr(u8(0)), 1, false);
    MoveEncoding result = copy_scalar_move(dst, move_gpr(target), 4, false, unsafe_address(cursor));
    return cursor + result.length;
}

u64 shift_result_component(u64 at, u32 offset, u8 operation) on cpu {
    if (offset == 0) { return at; }
    u64 immediate = register_instruction(at, u16(0xc1), u8(0), true, operation, u8(0));
    put_byte(immediate, u8(offset * 8));
    return immediate + 1u64;
}

u64 pack_integer_component(u64 at, ScalarEncoding scalar, u32 offset, bool merge) on cpu {
    u64 cursor = at;
    if (scalar.kind == ScalarKind::Signed && scalar.width < 32) {
        put_byte(cursor, u8(0x25));
        write_word(cursor + 1u64, (1 << scalar.width) - 1);
        cursor += 5u64;
    }
    cursor = shift_result_component(cursor, offset, u8(4));
    if (merge) {
        cursor = register_instruction(cursor, u16(0x09), u8(0), true, u8(0), u8(1));
    }
    return cursor;
}

u64 unpack_integer_component(u64 at, ScalarEncoding scalar, u32 offset) on cpu {
    u64 cursor = shift_result_component(at, offset, u8(5));
    if (scalar.kind == ScalarKind::Boolean) {
        cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(0), u8(0));
        cursor = comparison_result(cursor, move_gpr(u8(0)), BinaryOperation::NotEqual, false, false);
    }
    return cursor;
}

u64 integer_immediate(u64 at, u8 extension, u8 left, u64 bits, bool wide) on cpu {
    i64 value = i64(bits);
    if (!wide) { value = i64(i32(u32(bits))); }
    u16 opcode = u16(0x81);
    if (value >= -i64(128) && value <= i64(127)) { opcode = u16(0x83); }
    u64 cursor = register_instruction(at, opcode, u8(0), wide, extension, left);
    if (opcode == u16(0x83)) {
        put_byte(cursor, u8(bits));
        return cursor + 1u64;
    }
    write_word(cursor, u32(bits));
    return cursor + 4u64;
}

BinaryEncoding scalar_compare(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, BinaryOperation operation, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = u8(0);
    MoveLocation loaded = move_gpr(target);
    if (floating) {
        if (left.kind == MOVE_XMM) { target = left.base; }
        loaded = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    } else if (left.kind == MOVE_GPR) {
        target = left.base;
        loaded = move_gpr(target);
    }
    MoveEncoding load = copy_scalar_move(loaded, left, size, signed, output);
    u64 cursor = start + load.length;
    u64 right_literal = 0u64;
    bool wide = size == 8;
    if (floating) {
        u8 prefix = u8(0);
        if (wide) { prefix = u8(0x66); }
        cursor = encode_operand_instruction(cursor, u16(0x2e0f), prefix,
            wide && right.kind == MOVE_IMMEDIATE, target, right);
        if (right.kind == MOVE_IMMEDIATE) { right_literal = cursor - start - 4u64; }
    } else if (size < 4) {
        MoveEncoding source = copy_scalar_move(move_gpr(u8(1)), right, size, signed, unsafe_address(cursor));
        cursor = register_instruction(cursor + source.length, u16(0x3b), u8(0), false, target, u8(1));
    } else if (right.kind == MOVE_IMMEDIATE && (!wide || i64(right.value) == i64(i32(u32(right.value))))) {
        cursor = integer_immediate(cursor, u8(7), target, right.value, wide);
    } else {
        MoveLocation source = right;
        if (right.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), right.value, right.value > 0xffffffffu64);
            source = move_gpr(u8(1));
        }
        cursor = encode_operand_instruction(cursor, u16(0x3b), u8(0), wide, target, source);
    }
    cursor = comparison_result(cursor, dst, operation, signed, floating);
    return BinaryEncoding(cursor - start, load.literal_field, right_literal);
}

MoveEncoding cast_boolean(MoveLocation dst, MoveLocation src,
    u32 size, bool signed, bool floating, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u64 literal = 0u64;
    if (floating) {
        u8 left = u8(0);
        if (src.kind == MOVE_XMM) { left = src.base; }
        MoveLocation target = MoveLocation(0u64, MOVE_XMM, left, NO_INDEX, u8(0));
        MoveEncoding load = copy_scalar_move(target, src, size, signed, output);
        cursor += load.length;
        literal = load.literal_field;
        cursor = register_instruction(cursor, u16(0x570f), u8(0), false, u8(1), u8(1));
        u8 prefix = u8(0);
        if (size == 8) { prefix = u8(0x66); }
        cursor = register_instruction(cursor, u16(0x2e0f), prefix, false, left, u8(1));
    } else {
        MoveLocation source = src;
        if (size < 4) {
            MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, size, signed, output);
            cursor += load.length;
            source = move_gpr(u8(0));
        } else if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), src.value, src.value > 0xffffffffu64);
            source = move_gpr(u8(1));
        }
        cursor = encode_operand_instruction(cursor, u16(0x83), u8(0), size == 8, u8(7), source);
        put_byte(cursor, u8(0));
        cursor += 1u64;
    }
    cursor = comparison_result(cursor, dst, BinaryOperation::NotEqual, signed, floating);
    return MoveEncoding(cursor - start, literal);
}

bool same_move_location(MoveLocation left, MoveLocation right) on cpu {
    return left.kind == right.kind && left.value == right.value && left.base == right.base;
}

u8 binary_target(MoveLocation dst, MoveLocation left, MoveLocation right, u8 kind) on cpu {
    if (dst.kind == kind && (!same_move_location(dst, right) || same_move_location(left, right))) {
        return dst.base;
    }
    return u8(0);
}

BinaryEncoding float_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, u8 opcode, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = binary_target(dst, left, right, MOVE_XMM);
    MoveLocation loaded = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    MoveEncoding load = copy_scalar_move(loaded, left, size, false, output);
    bool wide = size == 8;
    u8 prefix = u8(0xf3);
    if (wide) { prefix = u8(0xf2); }
    u64 cursor = encode_operand_instruction(start + load.length, u16(0x0f) | (u16(opcode) << 8),
        prefix, wide && right.kind == MOVE_IMMEDIATE, target, right);
    u64 right_literal = 0u64;
    if (right.kind == MOVE_IMMEDIATE) { right_literal = cursor - start - 4u64; }
    MoveEncoding store = copy_scalar_move(dst, loaded, size, false, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, load.literal_field, right_literal);
}

BinaryEncoding scalar_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, u32 count_size, bool count_signed,
    BinaryOperation operation, address output) on cpu {
    if (floating) {
        u8 opcode = u8(0x58);
        if (operation == BinaryOperation::Subtract) { opcode = u8(0x5c); }
        if (operation == BinaryOperation::Multiply) { opcode = u8(0x59); }
        if (operation == BinaryOperation::Divide) { opcode = u8(0x5e); }
        return float_binary(dst, left, right, size, opcode, output);
    }
    u64 start = address_bits(output);
    u64 cursor = start;
    bool wide = size == 8;
    u8 target = u8(0);
    if (operation == BinaryOperation::ShiftLeft || operation == BinaryOperation::ShiftRight || operation == BinaryOperation::ShiftRightSigned) {
        if (right.kind != MOVE_IMMEDIATE) {
            MoveEncoding count = copy_scalar_move(move_gpr(u8(1)), right, count_size, count_signed, output);
            cursor += count.length;
            if (size < 4) { cursor = integer_immediate(cursor, u8(4), u8(1), u64(size * 8 - 1), false); }
        }
        target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        MoveEncoding load = copy_scalar_move(move_gpr(target), left, size, signed, unsafe_address(cursor));
        cursor += load.length;
        u8 extension = u8(4);
        if (operation == BinaryOperation::ShiftRight) { extension = u8(5); }
        if (operation == BinaryOperation::ShiftRightSigned) { extension = u8(7); }
        u16 opcode = u16(0xd3);
        if (right.kind == MOVE_IMMEDIATE) { opcode = u16(0xc1); }
        cursor = register_instruction(cursor, opcode, u8(0), wide, extension, target);
        if (right.kind == MOVE_IMMEDIATE) {
            put_byte(cursor, u8(right.value & u64(size * 8 - 1)));
            cursor += 1u64;
        }
    } else {
        target = binary_target(dst, left, right, MOVE_GPR);
        MoveEncoding load = copy_scalar_move(move_gpr(target), left, size, signed, output);
        cursor += load.length;
        u16 opcode = u16(0x03);
        u8 extension = u8(0);
        if (operation == BinaryOperation::Subtract) { opcode = u16(0x2b); extension = u8(5); }
        if (operation == BinaryOperation::Multiply) { opcode = u16(0xaf0f); }
        if (operation == BinaryOperation::And) { opcode = u16(0x23); extension = u8(4); }
        if (operation == BinaryOperation::Or) { opcode = u16(0x0b); extension = u8(1); }
        if (operation == BinaryOperation::Xor) { opcode = u16(0x33); extension = u8(6); }
        if (size < 4) {
            MoveEncoding source = copy_scalar_move(move_gpr(u8(1)), right, size, signed, unsafe_address(cursor));
            cursor = register_instruction(cursor + source.length, opcode, u8(0), false, target, u8(1));
        } else if (right.kind == MOVE_IMMEDIATE) {
            if (wide && i64(right.value) != i64(i32(u32(right.value)))) {
                cursor = move_immediate(cursor, u8(1), right.value, true);
                cursor = register_instruction(cursor, opcode, u8(0), true, target, u8(1));
            } else if (operation == BinaryOperation::Multiply) {
                cursor = register_instruction(cursor, u16(0x69), u8(0), wide, target, target);
                write_word(cursor, u32(right.value));
                cursor += 4u64;
            } else {
                cursor = integer_immediate(cursor, extension, target, right.value, wide);
            }
        } else {
            cursor = encode_operand_instruction(cursor, opcode, u8(0), wide, target, right);
        }
    }
    MoveEncoding store = copy_scalar_move(dst, move_gpr(target), size, signed, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, 0u64, 0u64);
}

extern "C" bool bit_copy(ScalarEncoding from, ScalarEncoding into) on cpu {
    if (from.kind == into.kind && from.width == into.width) { return true; }
    bool source_integer = from.kind == ScalarKind::Unsigned || from.kind == ScalarKind::Signed;
    bool target_integer = into.kind == ScalarKind::Unsigned || into.kind == ScalarKind::Signed;
    return target_integer && ((source_integer && from.width == into.width) ||
        (from.kind == ScalarKind::Boolean && into.width == 32));
}

MoveEncoding float_sign(MoveLocation dst, MoveLocation src, u32 size, bool negate, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation bits = move_gpr(u8(0));
    MoveEncoding load = copy_scalar_move(bits, src, size, false, output);
    u8 extension = u8(6);
    if (negate) { extension = u8(7); }
    u64 cursor = register_instruction(start + load.length, u16(0xba0f), u8(0), size == 8, extension, u8(0));
    put_byte(cursor, u8(size * 8 - 1));
    MoveEncoding store = copy_scalar_move(dst, bits, size, false, unsafe_address(cursor + 1u64));
    return MoveEncoding(cursor + 1u64 + store.length - start, 0u64);
}

MoveEncoding value_unary(MoveLocation dst, MoveLocation src, ScalarEncoding scalar,
    ValueUnaryOperation operation, address output) on cpu {
    u32 size = scalar.width / 8;
    bool signed = scalar.kind == ScalarKind::Signed;
    if (operation == ValueUnaryOperation::Bitcast) { return copy_scalar_move(dst, src, size, signed, output); }
    if (operation == ValueUnaryOperation::FloatNegate) { return float_sign(dst, src, size, true, output); }
    MoveLocation target = move_gpr(u8(0));
    if (dst.kind == MOVE_GPR || (dst.kind == MOVE_MEMORY && same_move_location(dst, src) && size >= 4)) { target = dst; }
    u64 start = address_bits(output);
    MoveEncoding load = copy_scalar_move(target, src, size, signed, output);
    u16 opcode = u16(0xf7);
    u8 extension = u8(3);
    if (operation == ValueUnaryOperation::BitNot) { extension = u8(2); }
    if (operation == ValueUnaryOperation::LogicalNot) { opcode = u16(0x83); extension = u8(6); }
    u64 cursor = encode_operand_instruction(start + load.length, opcode, u8(0), size == 8, extension, target);
    if (operation == ValueUnaryOperation::LogicalNot) { put_byte(cursor, u8(1)); cursor += 1u64; }
    MoveEncoding store = copy_scalar_move(dst, target, size, signed, unsafe_address(cursor));
    return MoveEncoding(cursor + store.length - start, 0u64);
}

MoveEncoding unary_math(MoveLocation dst, MoveLocation src, u32 size, bool sqrt, address output) on cpu {
    if (!sqrt) { return float_sign(dst, src, size, false, output); }
    u64 start = address_bits(output);
    u8 target = u8(0);
    if (dst.kind == MOVE_XMM) { target = dst.base; }
    u8 prefix = u8(0xf3);
    if (size == 8) { prefix = u8(0xf2); }
    u64 cursor = encode_operand_instruction(start, u16(0x510f), prefix,
        size == 8 && src.kind == MOVE_IMMEDIATE, target, src);
    u64 literal = 0u64;
    if (src.kind == MOVE_IMMEDIATE) { literal = cursor - start - 4u64; }
    MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    MoveEncoding store = copy_scalar_move(dst, value, size, false, unsafe_address(cursor));
    return MoveEncoding(cursor + store.length - start, literal);
}

enum CrtMath:u32 {
    Floor=0; Sin=1; Cos=2; Pow=3; Exp=4; Log=5; Exp2=6; Log2=7; Ceil=8;
};
enum ScalarMathRoute:u32 { Unary(bool)=0; Binary(u8)=1; Crt(CrtMath)=2; };

ScalarMathRoute scalar_math_route(ScalarMathOperation operation) on cpu {
    if (operation == ScalarMathOperation::Abs) { return ScalarMathRoute::Unary(false); }
    if (operation == ScalarMathOperation::Sqrt) { return ScalarMathRoute::Unary(true); }
    if (operation == ScalarMathOperation::Min) { return ScalarMathRoute::Binary(u8(0x5d)); }
    if (operation == ScalarMathOperation::Max) { return ScalarMathRoute::Binary(u8(0x5f)); }
    if (operation == ScalarMathOperation::Floor) { return ScalarMathRoute::Crt(CrtMath::Floor); }
    if (operation == ScalarMathOperation::Sin) { return ScalarMathRoute::Crt(CrtMath::Sin); }
    if (operation == ScalarMathOperation::Cos) { return ScalarMathRoute::Crt(CrtMath::Cos); }
    if (operation == ScalarMathOperation::Pow) { return ScalarMathRoute::Crt(CrtMath::Pow); }
    if (operation == ScalarMathOperation::Exp) { return ScalarMathRoute::Crt(CrtMath::Exp); }
    if (operation == ScalarMathOperation::Log) { return ScalarMathRoute::Crt(CrtMath::Log); }
    if (operation == ScalarMathOperation::Exp2) { return ScalarMathRoute::Crt(CrtMath::Exp2); }
    if (operation == ScalarMathOperation::Log2) { return ScalarMathRoute::Crt(CrtMath::Log2); }
    return ScalarMathRoute::Crt(CrtMath::Ceil);
}

extern "C" u64 scalar_math_outgoing(ScalarMathOperation operation) on cpu {
    match (scalar_math_route(operation)) {
        ScalarMathRoute::Unary(_) { return 0u64; }
        ScalarMathRoute::Binary(_) { return 0u64; }
        ScalarMathRoute::Crt(_) { return 32u64; }
    }
}

struct CrtMathName {
    array<u8,6> bytes;
    u32 length;
};

CrtMathName crt_math_name(CrtMath operation) on cpu {
    if (operation == CrtMath::Floor) { return CrtMathName("floorf", 5); }
    if (operation == CrtMath::Sin) { return CrtMathName("sinf  ", 3); }
    if (operation == CrtMath::Cos) { return CrtMathName("cosf  ", 3); }
    if (operation == CrtMath::Pow) { return CrtMathName("powf  ", 3); }
    if (operation == CrtMath::Exp) { return CrtMathName("expf  ", 3); }
    if (operation == CrtMath::Log) { return CrtMathName("logf  ", 3); }
    if (operation == CrtMath::Exp2) { return CrtMathName("exp2f ", 4); }
    if (operation == CrtMath::Log2) { return CrtMathName("log2f ", 4); }
    return CrtMathName("ceilf ", 4);
}

ScalarValueEncoding crt_math(CrtMath operation, MoveLocation dst, MoveLocation left,
    MoveLocation right, u32 size, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u64 right_literal = 0u64;
    if (operation == CrtMath::Pow) {
        MoveLocation xmm1 = MoveLocation(0u64, MOVE_XMM, u8(1), NO_INDEX, u8(0));
        MoveEncoding argument = copy_scalar_move(xmm1, right, size, false, output);
        right_literal = argument.literal_field;
        cursor += argument.length;
    }

    MoveLocation xmm0 = MoveLocation(0u64, MOVE_XMM, u8(0), NO_INDEX, u8(0));
    MoveEncoding argument = copy_scalar_move(xmm0, left, size, false, unsafe_address(cursor));
    u64 left_literal = argument.literal_field;
    if (left_literal != 0u64) { left_literal += cursor - start; }
    cursor += argument.length;
    SymbolEncoding call = symbol_target(true, unsafe_address(cursor));
    u64 call_field = cursor - start + call.field;
    cursor += call.length;
    MoveEncoding result = copy_scalar_move(dst, xmm0, size, false, unsafe_address(cursor));
    CrtMathName name = crt_math_name(operation);
    u32 wide = u32(size == 8);
    return ScalarValueEncoding(cursor + result.length - start,
        ScalarLiteral(right.value, right_literal, wide), ScalarLiteral(left.value, left_literal, wide),
        ScalarMathReference(call_field, name.length + u32(size == 4), name.bytes));
}

ScalarValueEncoding scalar_math(ScalarMathOperation operation, MoveLocation dst,
    MoveLocation left, MoveLocation right, u32 size, address output) on cpu {
    match (scalar_math_route(operation)) {
        ScalarMathRoute::Unary(sqrt) {
            MoveEncoding value = unary_math(dst, left, size, sqrt, output);
            return value_encoding(BinaryEncoding(value.length, value.literal_field, 0u64),
                left.value, size == 8, 0u64, false);
        }
        ScalarMathRoute::Binary(opcode) {
            BinaryEncoding value = float_binary(dst, left, right, size, opcode, output);
            return value_encoding(value, left.value, size == 8, right.value, size == 8);
        }
        ScalarMathRoute::Crt(imported) { return crt_math(imported, dst, left, right, size, output); }
    }
}

BinaryEncoding value_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u32 source_size = from.width / 8;
    u32 target_size = into.width / 8;
    bool signed = from.kind == ScalarKind::Signed;
    bool target_signed = into.kind == ScalarKind::Signed;
    bool source_float = from.kind == ScalarKind::Float;
    bool target_float = into.kind == ScalarKind::Float;
    u64 source_literal = 0u64;
    u64 constant_literal = 0u64;
    if (from.kind == ScalarKind::Unsigned && source_size == 8 && target_float) {
        MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, 8, false, output);
        cursor += load.length;
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (target_size == 8) { prefix = u8(0xf2); }
        cursor += unsigned_to_float(target, prefix, unsafe_address(cursor));
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float && into.kind == ScalarKind::Unsigned && target_size == 8) {
        MoveLocation input = MoveLocation(0u64, MOVE_XMM, u8(0), NO_INDEX, u8(0));
        MoveEncoding load = copy_scalar_move(input, src, source_size, false, output);
        cursor += load.length;
        source_literal = load.literal_field;
        u64 bits = 0x5f000000u64;
        if (source_size == 8) { bits = 0x43e0000000000000u64; }
        MoveLocation constant = MoveLocation(bits, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        MoveLocation high = MoveLocation(0u64, MOVE_XMM, u8(1), NO_INDEX, u8(0));
        MoveEncoding bound = copy_scalar_move(high, constant, source_size, false, unsafe_address(cursor));
        if (bound.literal_field != 0u64) { constant_literal = cursor - start + bound.literal_field; }
        cursor += bound.length;
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        cursor += float_to_unsigned(target, prefix, unsafe_address(cursor));
        MoveEncoding store = copy_scalar_move(dst, move_gpr(target), 8, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float && target_float && source_size != target_size) {
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        cursor = encode_operand_instruction(cursor, u16(0x5a0f), prefix,
            source_size == 8 && src.kind == MOVE_IMMEDIATE, target, src);
        if (src.kind == MOVE_IMMEDIATE) { source_literal = cursor - start - 4u64; }
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (bit_copy(from, into)) {
        MoveEncoding move = copy_scalar_move(dst, src, target_size, target_signed, output);
        return BinaryEncoding(move.length, move.literal_field, 0u64);
    } else if (into.kind == ScalarKind::Boolean) {
        MoveEncoding value = cast_boolean(dst, src, source_size, signed, source_float, output);
        return BinaryEncoding(value.length, value.literal_field, 0u64);
    } else if (!source_float && !target_float) {
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        MoveLocation value = move_gpr(target);
        u32 load_size = source_size;
        bool load_signed = signed;
        if (target_size <= 4 && source_size >= 4) { load_size = target_size; load_signed = target_signed; }
        MoveEncoding load = copy_scalar_move(value, src, load_size, load_signed, output);
        cursor += load.length;
        if (target_size <= 4) {
            if (target_size == 4 && source_size >= 4 && same_move_location(src, value)) {
                cursor = register_instruction(cursor, u16(0x8b), u8(0), false, target, target);
            }
        } else if (signed && source_size < 8) {
            cursor = register_instruction(cursor, u16(0x63), u8(0), true, target, target);
        }
        MoveEncoding store = copy_scalar_move(dst, value, target_size, target_signed, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float) {
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        bool wide = (into.kind == ScalarKind::Unsigned && target_size == 4) || target_size == 8;
        if (src.kind == MOVE_IMMEDIATE) { wide = source_size == 8; }
        cursor = encode_operand_instruction(cursor, u16(0x2c0f), prefix, wide, target, src);
        if (src.kind == MOVE_IMMEDIATE) { source_literal = cursor - start - 4u64; }
        if (into.kind == ScalarKind::Unsigned && target_size == 4) {
            cursor = register_instruction(cursor, u16(0x8b), u8(0), false, target, target);
        }
        MoveEncoding store = copy_scalar_move(dst, move_gpr(target), target_size, target_signed, unsafe_address(cursor));
        cursor += store.length;
    } else {
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        bool unsigned32 = from.kind == ScalarKind::Unsigned && source_size == 4;
        MoveLocation input = src;
        if (source_size < 4 || (unsigned32 && src.kind == MOVE_MEMORY)) {
            input = move_gpr(u8(0));
            MoveEncoding load = copy_scalar_move(input, src, source_size, signed, output);
            cursor += load.length;
        } else if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), src.value, src.value > 0xffffffffu64);
            input = move_gpr(u8(1));
        }
        cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
        u8 prefix = u8(0xf3);
        if (target_size == 8) { prefix = u8(0xf2); }
        cursor = encode_operand_instruction(cursor, u16(0x2a0f), prefix, unsigned32 || source_size == 8, target, input);
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    }
    return BinaryEncoding(cursor - start, source_literal, constant_literal);
}

BinaryEncoding value_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, u32 size, bool signed, address output) on cpu {
    if (condition.kind == MOVE_IMMEDIATE) {
        if (condition.value != 0u64) {
            MoveEncoding move = copy_scalar_move(dst, left, size, signed, output);
            return BinaryEncoding(move.length, move.literal_field, 0u64);
        }
        MoveEncoding move = copy_scalar_move(dst, right, size, signed, output);
        return BinaryEncoding(move.length, 0u64, move.literal_field);
    }
    u64 start = address_bits(output);
    u64 cursor = encode_operand_instruction(start, u16(0x83), u8(0), false, u8(7), condition);
    put_byte(cursor, u8(0));
    cursor += 1u64;
    u8 target = u8(0);
    if (dst.kind == MOVE_GPR) { target = dst.base; }
    MoveLocation initial = right;
    MoveLocation source = left;
    u16 opcode = u16(0x450f);
    if (same_move_location(dst, left)) { initial = left; source = right; opcode = u16(0x440f); }
    MoveEncoding load = copy_scalar_move(move_gpr(target), initial, size, signed, unsafe_address(cursor));
    cursor += load.length;
    if (size < 4 || source.kind == MOVE_XMM || source.kind == MOVE_IMMEDIATE) {
        MoveEncoding selected = copy_scalar_move(move_gpr(u8(1)), source, size, signed, unsafe_address(cursor));
        cursor += selected.length;
        source = move_gpr(u8(1));
    }
    cursor = encode_operand_instruction(cursor, opcode, u8(0), size == 8, target, source);
    MoveEncoding store = copy_scalar_move(dst, move_gpr(target), size, signed, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, 0u64, 0u64);
}

u64 division_shift(u64 at, u8 target, u8 kind, bool wide, u8 count) on cpu {
    u64 cursor = register_instruction(at, u16(0xc1), u8(0), wide, kind, target);
    put_byte(cursor, count);
    return cursor + 1u64;
}

u64 integer_magnitude(u64 at, u8 value, u8 mask, bool wide) on cpu {
    u64 cursor = register_instruction(at, u16(0x89), u8(0), wide, value, mask);
    u8 count = u8(31);
    if (wide) { count = u8(63); }
    cursor = division_shift(cursor, mask, u8(7), wide, count);
    cursor = register_instruction(cursor, u16(0x31), u8(0), wide, mask, value);
    return register_instruction(cursor, u16(0x29), u8(0), wide, mask, value);
}

u64 divide_unsigned_instruction(u64 at, bool wide) on cpu {
    u64 cursor = register_instruction(at, u16(0x31), u8(0), false, u8(2), u8(2));
    return register_instruction(cursor, u16(0xf7), u8(0), wide, u8(6), u8(1));
}

u64 divide_value_at(u64 at, bool signed) on cpu {
    if (!signed) { return divide_unsigned_instruction(at, false); }
    u64 cursor = register_instruction(at, u16(0x63), u8(0), true, u8(0), u8(0));
    cursor = register_instruction(cursor, u16(0x63), u8(0), true, u8(1), u8(1));
    put_byte(cursor, u8(0x48));
    put_byte(cursor + 1u64, u8(0x99));
    return register_instruction(cursor + 2u64, u16(0xf7), u8(0), true, u8(7), u8(1));
}

u64 load_magnitude32(u64 at, MoveLocation src, ScalarEncoding scalar) on cpu {
    bool signed = scalar.kind == ScalarKind::Signed;
    MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, scalar.width / 8, signed, unsafe_address(at));
    u64 cursor = at + load.length;
    if (signed) { cursor = integer_magnitude(cursor, u8(0), u8(1), false); }
    return cursor;
}

u64 store_division(u64 at, MoveLocation dst, u8 register, ScalarEncoding scalar) on cpu {
    MoveEncoding store = copy_scalar_move(dst, move_gpr(register), scalar.width / 8,
        scalar.kind == ScalarKind::Signed, unsafe_address(at));
    return at + store.length;
}

struct DivisionOutputs { MoveLocation first; MoveLocation second; u32 kind; };

MoveLocation division_remainder(DivisionOutputs result) on cpu {
    if (result.kind == 3) { return result.second; }
    return result.first;
}

u64 divide_constant32(DivisionOutputs result, MoveLocation src, u32 divisor,
    ScalarEncoding scalar, address output) on cpu requires nonzero(divisor) {
    u64 start = address_bits(output);
    bool signed = scalar.kind == ScalarKind::Signed;
    DivisionPlan plan = division_plan(divisor, signed);
    u64 cursor = load_magnitude32(start, src, scalar);
    if (plan.mode == DIVISION_SHIFT) {
        if (plan.shift != u8(0)) { cursor = division_shift(cursor, u8(0), u8(5), false, plan.shift); }
    } else {
        cursor = move_immediate(cursor, u8(1), u64(plan.magic), false);
        cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(4), u8(1));
        if (plan.mode == DIVISION_MULTIPLY_ADD) {
            cursor = load_magnitude32(cursor, src, scalar);
            cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(2), u8(0));
            cursor = register_instruction(cursor, u16(0xd1), u8(0), false, u8(5), u8(0));
            cursor = register_instruction(cursor, u16(0x01), u8(0), false, u8(2), u8(0));
        } else {
            cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(2), u8(0));
        }
        cursor = division_shift(cursor, u8(0), u8(5), false, plan.shift);
    }
    if (signed) {
        MoveEncoding sign = copy_scalar_move(move_gpr(u8(2)), src, scalar.width / 8, true, unsafe_address(cursor));
        cursor = division_shift(cursor + sign.length, u8(2), u8(7), false, u8(31));
        if (i32(divisor) < i32(0)) { cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(2), u8(2)); }
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(2), u8(0));
        cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(2), u8(0));
    }
    if (result.kind != 1) {
        if (result.kind == 3) { cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(0), u8(1)); }
        cursor = register_instruction(cursor, u16(0x69), u8(0), false, u8(0), u8(0));
        write_word(cursor, divisor);
        cursor += 4u64;
        MoveEncoding source = copy_scalar_move(move_gpr(u8(2)), src, scalar.width / 8, signed, unsafe_address(cursor));
        cursor = register_instruction(cursor + source.length, u16(0x29), u8(0), false, u8(0), u8(2));
        if (result.kind == 3) { cursor = store_division(cursor, result.first, u8(1), scalar); }
        cursor = store_division(cursor, division_remainder(result), u8(2), scalar);
    } else {
        cursor = store_division(cursor, result.first, u8(0), scalar);
    }
    return cursor - start;
}

u64 divide_value(DivisionOutputs result, MoveLocation src, MoveLocation divisor,
    ScalarEncoding scalar, ScalarEncoding divisor_scalar, address output) on cpu {
    u64 start = address_bits(output);
    u32 size = scalar.width / 8;
    bool signed = scalar.kind == ScalarKind::Signed;
    u32 divisor_size = divisor_scalar.width / 8;
    bool divisor_signed = divisor_scalar.kind == ScalarKind::Signed;
    if (size == 8) { divisor_size = size; divisor_signed = signed; }
    MoveEncoding right = copy_scalar_move(move_gpr(u8(1)), divisor, divisor_size, divisor_signed, output);
    u64 cursor = start + right.length;
    MoveEncoding left = copy_scalar_move(move_gpr(u8(0)), src, size, signed, unsafe_address(cursor));
    cursor += left.length;
    if (size == 8) {
        if (signed) {
            cursor = integer_magnitude(cursor, u8(0), u8(2), true);
            cursor = integer_magnitude(cursor, u8(1), u8(2), true);
        }
        cursor = divide_unsigned_instruction(cursor, true);
        if (signed) {
            if (result.kind != 2) {
                MoveEncoding sign = copy_scalar_move(move_gpr(u8(1)), src, size, true, unsafe_address(cursor));
                cursor += sign.length;
                if (divisor.kind == MOVE_IMMEDIATE) {
                    if (i64(divisor.value) < i64(0)) { cursor = register_instruction(cursor, u16(0xf7), u8(0), true, u8(2), u8(1)); }
                } else {
                    cursor = encode_operand_instruction(cursor, u16(0x33), u8(0), true, u8(1), divisor);
                }
                cursor = division_shift(cursor, u8(1), u8(7), true, u8(63));
                cursor = register_instruction(cursor, u16(0x31), u8(0), true, u8(1), u8(0));
                cursor = register_instruction(cursor, u16(0x29), u8(0), true, u8(1), u8(0));
            }
            if (result.kind != 1) {
                MoveEncoding sign = copy_scalar_move(move_gpr(u8(1)), src, size, true, unsafe_address(cursor));
                cursor = division_shift(cursor + sign.length, u8(1), u8(7), true, u8(63));
                cursor = register_instruction(cursor, u16(0x31), u8(0), true, u8(1), u8(2));
                cursor = register_instruction(cursor, u16(0x29), u8(0), true, u8(1), u8(2));
            }
        }
    } else {
        cursor = divide_value_at(cursor, signed);
    }
    if (result.kind != 2) { cursor = store_division(cursor, result.first, u8(0), scalar); }
    if (result.kind != 1) { cursor = store_division(cursor, division_remainder(result), u8(2), scalar); }
    return cursor - start;
}

MoveLocation packet_register(u8 reg) on cpu {
    return MoveLocation(0u64, MOVE_XMM, reg, NO_INDEX, u8(0));
}

u64 packet_instruction(u64 at, u8 prefix, u8 opcode, u8 dst, u8 src) on cpu {
    return register_instruction(at, u16(0x0f) | (u16(opcode) << 8), prefix, false, dst, src);
}

u64 packet_shuffle_at(u64 at, u8 dst, u8 src, u8 pattern) on cpu {
    u64 cursor = packet_instruction(at, u8(0x66), u8(0x70), dst, src);
    put_byte(cursor, pattern);
    return cursor + 1u64;
}

u64 packet_constant_at(u64 at, u8 dst, u32 bits) on cpu {
    if (bits == 0) { return packet_instruction(at, u8(0x66), u8(0xef), dst, dst); }
    if (bits == 0xffffffff) { return packet_instruction(at, u8(0x66), u8(0x76), dst, dst); }
    u64 cursor = move_immediate(at, u8(0), u64(bits), false);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), dst, u8(0));
    return packet_shuffle_at(cursor, dst, dst, u8(0));
}

u64 packet_load_at(u64 at, MoveLocation src, u8 dst) on cpu {
    if (src.kind == MOVE_XMM && src.base == dst) { return at; }
    return encode_operand_instruction(at, u16(0x280f), u8(0), false, dst, src);
}

u64 packet_store_at(u64 at, MoveLocation dst, u8 src) on cpu {
    if (dst.kind == MOVE_XMM) {
        if (dst.base == src) { return at; }
        return packet_instruction(at, u8(0), u8(0x28), dst.base, src);
    }
    return encode_operand_instruction(at, u16(0x290f), u8(0), false, src, dst);
}

u8 packet_destination(MoveLocation dst) on cpu {
    if (dst.kind == MOVE_XMM) { return dst.base; }
    return u8(0);
}

u64 packet_bool_at(u64 at, u8 dst) on cpu {
    u64 cursor = packet_constant_at(at, u8(3), 1);
    return packet_instruction(cursor, u8(0x66), u8(0xdb), dst, u8(3));
}

extern "C" u64 packet_constant(MoveLocation dst, u32 bits, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = packet_destination(dst);
    u64 cursor = packet_constant_at(start, target, bits);
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_index(MoveLocation dst, bool sparse, u32 slots, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = packet_destination(dst);
    u64 cursor = start;
    if (sparse) {
        MoveLocation indices = MoveLocation(u64(slots), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = packet_load_at(start, indices, target);
    } else {
        // Expand four packed bytes to dwords, then add the broadcast range index.
        cursor = move_immediate(start, u8(0), 0x03020100u64, false);
        cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), target, u8(0));
        cursor = packet_constant_at(cursor, u8(1), 0);
        cursor = packet_instruction(cursor, u8(0x66), u8(0x60), target, u8(1));
        cursor = packet_instruction(cursor, u8(0x66), u8(0x61), target, u8(1));
        cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), u8(1), u8(3));
        cursor = packet_shuffle_at(cursor, u8(1), u8(1), u8(0));
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfe), target, u8(1));
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_copy(MoveLocation dst, MoveLocation src, address output) on cpu {
    u64 start = address_bits(output);
    if (same_move_location(dst, src)) { return 0u64; }
    u8 temporary = u8(0);
    if (dst.kind == MOVE_XMM) { temporary = dst.base; }
    if (src.kind == MOVE_XMM) { temporary = src.base; }
    u64 cursor = packet_load_at(start, src, temporary);
    return packet_store_at(cursor, dst, temporary) - start;
}

u64 packet_from_word_at(u64 at, bool boolean, u8 dst) on cpu {
    if (!boolean) { return at; }
    u64 cursor = packet_constant_at(at, u8(1), 0);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x76), dst, u8(1));
    cursor = packet_constant_at(cursor, u8(1), 1);
    return packet_instruction(cursor, u8(0x66), u8(0xfe), dst, u8(1));
}

extern "C" u64 packet_uniform(MoveLocation dst, u32 offset, bool boolean, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = packet_destination(dst);
    MoveLocation word = MoveLocation(u64(offset), MOVE_MEMORY, u8(13), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x6e0f), u8(0x66), false, target, word);
    cursor = packet_shuffle_at(cursor, target, target, u8(0));
    cursor = packet_from_word_at(cursor, boolean, target);
    return packet_store_at(cursor, dst, target) - start;
}

u64 packet_shift_immediate_at(u64 at, u8 opcode, u8 operation, u8 xmm, u8 count) on cpu {
    u64 cursor = packet_instruction(at, u8(0x66), opcode, operation, xmm);
    put_byte(cursor, count);
    return cursor + 1u64;
}

u64 packet_product_at(u64 at, bool broadcast, u8 words) on cpu {
    u64 cursor = packet_instruction(at, u8(0), u8(0x28), u8(2), u8(0));
    if (!broadcast) { cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), u8(1)); }
    cursor = packet_shift_immediate_at(cursor, u8(0x73), u8(3), u8(2), u8(4));
    if (!broadcast) {
        cursor = packet_shift_immediate_at(cursor, u8(0x73), u8(3), u8(3), u8(4));
    }
    cursor = packet_instruction(cursor, u8(0x66), u8(0xf4), u8(0), u8(1));
    u8 right = u8(3);
    if (broadcast) { right = u8(1); }
    cursor = packet_instruction(cursor, u8(0x66), u8(0xf4), u8(2), right);
    cursor = packet_shuffle_at(cursor, u8(0), u8(0), words);
    cursor = packet_shuffle_at(cursor, u8(2), u8(2), words);
    return packet_instruction(cursor, u8(0x66), u8(0x62), u8(0), u8(2));
}

u64 packet_lane_at(u64 at, MoveLocation src, u8 lane, u8 gpr) on cpu {
    if (src.kind == MOVE_XMM) {
        u8 source = src.base;
        u64 cursor = at;
        if (lane != u8(0)) {
            cursor = packet_shuffle_at(cursor, u8(3), source, lane * u8(0x55));
            source = u8(3);
        }
        return packet_instruction(cursor, u8(0x66), u8(0x7e), source, gpr);
    }
    MoveLocation field = src;
    field.value += u64(lane) * 4u64;
    return encode_operand_instruction(at, u16(0x8b), u8(0), false, gpr, field);
}

u64 packet_division_lane_at(u64 at, u8 accumulator, u8 gpr, u8 lane) on cpu {
    // MOVD already zeros the upper three lanes of the first result.
    if (lane == u8(0)) {
        return packet_instruction(at, u8(0x66), u8(0x6e), accumulator, gpr);
    }
    u64 cursor = packet_instruction(at, u8(0x66), u8(0x6e), u8(2), gpr);
    cursor = packet_shift_immediate_at(cursor, u8(0x73), u8(7), u8(2), lane * u8(4));
    return packet_instruction(cursor, u8(0x66), u8(0xeb), accumulator, u8(2));
}

u64 packet_divide_value(DivisionOutputs result, MoveLocation src,
    MoveLocation divisor, bool signed, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u8 remainder = result.kind == 3 ? u8(1) : u8(0);
    for (ordinal in 0..4) {
        u8 lane = u8(ordinal);
        cursor = packet_lane_at(cursor, divisor, lane, u8(1));
        cursor = packet_lane_at(cursor, src, lane, u8(0));
        cursor = divide_value_at(cursor, signed);
        if (result.kind != 2) { cursor = packet_division_lane_at(cursor, u8(0), u8(0), lane); }
        if (result.kind != 1) { cursor = packet_division_lane_at(cursor, remainder, u8(2), lane); }
    }
    cursor = packet_store_at(cursor, result.first, u8(0));
    if (result.kind == 3) { cursor = packet_store_at(cursor, result.second, u8(1)); }
    return cursor - start;
}

u64 packet_magnitude_at(u64 at, MoveLocation src, u8 dst, bool signed) on cpu {
    u64 cursor = packet_load_at(at, src, dst);
    if (signed) {
        cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), dst);
        cursor = packet_shift_immediate_at(cursor, u8(0x72), u8(4), u8(3), u8(31));
        cursor = packet_instruction(cursor, u8(0x66), u8(0xef), dst, u8(3));
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), dst, u8(3));
    }
    return cursor;
}

u64 packet_divide_constant(DivisionOutputs result, MoveLocation src,
    u32 divisor, bool signed, address output) on cpu requires nonzero(divisor) {
    u64 start = address_bits(output);
    DivisionPlan plan = division_plan(divisor, signed);
    u64 cursor = packet_magnitude_at(start, src, u8(0), signed);
    if (plan.mode != DIVISION_SHIFT) {
        cursor = packet_constant_at(cursor, u8(1), plan.magic);
        cursor = packet_product_at(cursor, true, u8(0xdd));
        if (plan.mode == DIVISION_MULTIPLY_ADD) {
            cursor = packet_magnitude_at(cursor, src, u8(1), signed);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), u8(1), u8(0));
            cursor = packet_shift_immediate_at(cursor, u8(0x72), u8(2), u8(1), u8(1));
            cursor = packet_instruction(cursor, u8(0x66), u8(0xfe), u8(0), u8(1));
        }
    }
    if (plan.shift != u8(0)) {
        cursor = packet_shift_immediate_at(cursor, u8(0x72), u8(2), u8(0), plan.shift);
    }
    if (signed) {
        cursor = packet_load_at(cursor, src, u8(1));
        cursor = packet_shift_immediate_at(cursor, u8(0x72), u8(4), u8(1), u8(31));
        if (i32(divisor) < i32(0)) {
            cursor = packet_constant_at(cursor, u8(2), 0xffffffff);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), u8(1), u8(2));
        }
        cursor = packet_instruction(cursor, u8(0x66), u8(0xef), u8(0), u8(1));
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), u8(0), u8(1));
    }
    if (result.kind != 1) {
        if (result.kind == 3) { cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), u8(0)); }
        cursor = packet_constant_at(cursor, u8(1), divisor);
        cursor = packet_product_at(cursor, true, u8(0x88));
        cursor = packet_load_at(cursor, src, u8(1));
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), u8(1), u8(0));
        if (result.kind == 3) { cursor = packet_store_at(cursor, result.first, u8(3)); }
        cursor = packet_store_at(cursor, division_remainder(result), u8(1));
    } else {
        cursor = packet_store_at(cursor, result.first, u8(0));
    }
    return cursor - start;
}

u64 packet_compare_at(u64 at, MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation) on cpu {
    bool floating = scalar.kind == ScalarKind::Float;
    bool swap = floating && (operation == BinaryOperation::Greater || operation == BinaryOperation::GreaterEqual)
        || !floating && (operation == BinaryOperation::Less || operation == BinaryOperation::GreaterEqual);
    MoveLocation a = left; MoveLocation b = right;
    if (swap) { a = right; b = left; }
    bool equality = operation == BinaryOperation::Equal || operation == BinaryOperation::NotEqual;
    bool bias = !equality && (scalar.kind == ScalarKind::Unsigned || scalar.kind == ScalarKind::Boolean);
    u8 target = binary_target(dst, a, b, MOVE_XMM);
    if (bias && dst.kind == MOVE_XMM) { target = dst.base; }
    u64 cursor = at;
    if (bias) { cursor = packet_load_at(cursor, b, u8(1)); }
    cursor = packet_load_at(cursor, a, target);
    if (floating) {
        cursor = encode_operand_instruction(cursor, u16(0xc20f), u8(0), false, target, b);
        u8 predicate = u8(0);
        if (operation == BinaryOperation::NotEqual) { predicate = u8(4); }
        if (operation == BinaryOperation::Less || operation == BinaryOperation::Greater) { predicate = u8(1); }
        if (operation == BinaryOperation::LessEqual || operation == BinaryOperation::GreaterEqual) { predicate = u8(2); }
        put_byte(cursor, predicate); cursor += 1u64;
    } else {
        MoveLocation source = b;
        if (bias) {
            cursor = packet_constant_at(cursor, u8(2), 0x80000000);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), u8(1), u8(2));
            source = packet_register(u8(1));
        }
        u8 opcode = u8(0x66);
        if (equality) { opcode = u8(0x76); }
        cursor = encode_operand_instruction(cursor, u16(0x0f) | (u16(opcode) << 8), u8(0x66), false, target, source);
        if (operation == BinaryOperation::NotEqual || operation == BinaryOperation::LessEqual || operation == BinaryOperation::GreaterEqual) {
            cursor = packet_constant_at(cursor, u8(2), 0xffffffff);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(2));
        }
    }
    cursor = packet_bool_at(cursor, target);
    return packet_store_at(cursor, dst, target);
}

extern "C" u64 packet_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation, bool uniform_count, address output) on cpu {
    u64 start = address_bits(output);
    if (u32(operation) >= u32(BinaryOperation::Equal)) {
        return packet_compare_at(start, dst, left, right, scalar, operation) - start;
    }
    u64 cursor = start;
    bool shift = operation == BinaryOperation::ShiftLeft || operation == BinaryOperation::ShiftRight || operation == BinaryOperation::ShiftRightSigned;
    if (shift) {
        if (uniform_count) {
            u8 target = packet_destination(dst);
            cursor = packet_load_at(cursor, right, u8(1));
            cursor = packet_instruction(cursor, u8(0x66), u8(0x7e), u8(1), u8(0));
            put_byte(cursor, u8(0x83)); put_byte(cursor + 1u64, u8(0xe0)); put_byte(cursor + 2u64, u8(0x1f)); cursor += 3u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), u8(1), u8(0));
            cursor = packet_load_at(cursor, left, target);
            u8 opcode = u8(0xd2);
            if (operation == BinaryOperation::ShiftLeft) { opcode = u8(0xf2); }
            if (operation == BinaryOperation::ShiftRightSigned) { opcode = u8(0xe2); }
            cursor = packet_instruction(cursor, u8(0x66), opcode, target, u8(1));
            return packet_store_at(cursor, dst, target) - start;
        }
        cursor = packet_constant_at(cursor, u8(0), 0);
        u8 kind = u8(5);
        if (operation == BinaryOperation::ShiftLeft) { kind = u8(4); }
        if (operation == BinaryOperation::ShiftRightSigned) { kind = u8(7); }
        for (lane in 0..4) {
            cursor = packet_lane_at(cursor, right, u8(lane), u8(1));
            cursor = packet_lane_at(cursor, left, u8(lane), u8(0));
            put_byte(cursor, u8(0xd3)); put_byte(cursor + 1u64, u8(0xc0) | (kind << u8(3))); cursor += 2u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), u8(1), u8(0));
            if (lane != 0) {
                cursor = packet_instruction(cursor, u8(0x66), u8(0x73), u8(7), u8(1));
                put_byte(cursor, u8(lane * 4)); cursor += 1u64;
            }
            cursor = packet_instruction(cursor, u8(0x66), u8(0xeb), u8(0), u8(1));
        }
        return packet_store_at(cursor, dst, u8(0)) - start;
    }
    bool floating = scalar.kind == ScalarKind::Float;
    if (operation == BinaryOperation::Multiply && !floating) {
        cursor = packet_load_at(cursor, left, u8(0));
        cursor = packet_load_at(cursor, right, u8(1));
        cursor = packet_product_at(cursor, false, u8(0x88));
        return packet_store_at(cursor, dst, u8(0)) - start;
    }
    u8 target = binary_target(dst, left, right, MOVE_XMM);
    cursor = packet_load_at(cursor, left, target);
    u8 prefix = u8(0x66); u8 opcode = u8(0xfe);
    if (operation == BinaryOperation::And) { opcode = u8(0xdb); }
    else if (operation == BinaryOperation::Or) { opcode = u8(0xeb); }
    else if (operation == BinaryOperation::Xor) { opcode = u8(0xef); }
    else if (floating) {
        prefix = u8(0); opcode = u8(0x58);
        if (operation == BinaryOperation::Subtract) { opcode = u8(0x5c); }
        if (operation == BinaryOperation::Multiply) { opcode = u8(0x59); }
        if (operation == BinaryOperation::Divide) { opcode = u8(0x5e); }
    } else if (operation == BinaryOperation::Subtract) { opcode = u8(0xfa); }
    cursor = encode_operand_instruction(cursor, u16(0x0f) | (u16(opcode) << 8), prefix, false, target, right);
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_unary(MoveLocation dst, MoveLocation src, ValueUnaryOperation operation, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    if (operation == ValueUnaryOperation::SignedNegate || operation == ValueUnaryOperation::UnsignedNegate) {
        cursor = packet_load_at(cursor, src, u8(1));
        cursor = packet_constant_at(cursor, target, 0);
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), target, u8(1));
    } else {
        cursor = packet_load_at(cursor, src, target);
        if (operation != ValueUnaryOperation::Bitcast) {
            u32 bits = 0xffffffff;
            if (operation == ValueUnaryOperation::FloatNegate) { bits = 0x80000000; }
            if (operation == ValueUnaryOperation::LogicalNot) { bits = 1; }
            cursor = packet_constant_at(cursor, u8(1), bits);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
        }
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    bool floating = from.kind == ScalarKind::Float;
    if (into.kind == ScalarKind::Float && (from.kind == ScalarKind::Signed || from.kind == ScalarKind::Boolean)
        || floating && into.kind == ScalarKind::Signed) {
        u8 prefix = u8(0);
        if (floating) { prefix = u8(0xf3); }
        cursor = encode_operand_instruction(cursor, u16(0x5b0f), prefix, false, target, src);
    } else {
        cursor = packet_load_at(cursor, src, target);
        if (into.kind == ScalarKind::Boolean) {
            cursor = packet_constant_at(cursor, u8(1), 0);
            if (floating) {
                cursor = packet_instruction(cursor, u8(0), u8(0xc2), target, u8(1));
                put_byte(cursor, u8(4)); cursor += 1u64;
            } else {
                cursor = packet_instruction(cursor, u8(0x66), u8(0x76), target, u8(1));
                cursor = packet_constant_at(cursor, u8(1), 0xffffffff);
                cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
            }
            cursor = packet_bool_at(cursor, target);
        } else if (into.kind == ScalarKind::Float) {
            cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(1), target);
            cursor = packet_constant_at(cursor, u8(2), 0xffff);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), target, u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0x72), u8(2), u8(1));
            put_byte(cursor, u8(16)); cursor += 1u64;
            cursor = packet_instruction(cursor, u8(0), u8(0x5b), target, target);
            cursor = packet_instruction(cursor, u8(0), u8(0x5b), u8(1), u8(1));
            cursor = packet_constant_at(cursor, u8(2), 0x47800000);
            cursor = packet_instruction(cursor, u8(0), u8(0x59), u8(1), u8(2));
            cursor = packet_instruction(cursor, u8(0), u8(0x58), target, u8(1));
        } else {
            cursor = packet_constant_at(cursor, u8(1), 0x4f000000);
            cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), u8(1));
            cursor = packet_instruction(cursor, u8(0), u8(0xc2), u8(1), target);
            put_byte(cursor, u8(2)); cursor += 1u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(3), u8(1));
            cursor = packet_instruction(cursor, u8(0), u8(0x5c), target, u8(3));
            cursor = packet_instruction(cursor, u8(0xf3), u8(0x5b), target, target);
            cursor = packet_constant_at(cursor, u8(2), 0x80000000);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(1), u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
        }
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_math(MoveLocation dst, MoveLocation left, MoveLocation right,
    PacketMathOperation operation, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    if (operation == PacketMathOperation::Sqrt) {
        cursor = encode_operand_instruction(cursor, u16(0x510f), u8(0), false, target, left);
    } else if (operation == PacketMathOperation::Abs) {
        cursor = packet_load_at(cursor, left, target);
        cursor = packet_constant_at(cursor, u8(1), 0x7fffffff);
        cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), target, u8(1));
    } else {
        target = binary_target(dst, left, right, MOVE_XMM);
        cursor = packet_load_at(cursor, left, target);
        u16 opcode = u16(0x5d0f);
        if (operation == PacketMathOperation::Max) { opcode = u16(0x5f0f); }
        cursor = encode_operand_instruction(cursor, opcode, u8(0), false, target, right);
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, address output) on cpu {
    u64 start = address_bits(output); u8 target = packet_destination(dst);
    u64 cursor = packet_load_at(start, right, u8(1));
    MoveLocation source = left;
    if (left.kind == MOVE_XMM && left.base == target) {
        cursor = packet_load_at(cursor, left, u8(2)); source = packet_register(u8(2));
    }
    cursor = packet_load_at(cursor, condition, target);
    cursor = packet_constant_at(cursor, u8(3), 0);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x76), target, u8(3));
    cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(1), target);
    cursor = encode_operand_instruction(cursor, u16(0xdf0f), u8(0x66), false, target, source);
    cursor = packet_instruction(cursor, u8(0x66), u8(0xeb), target, u8(1));
    return packet_store_at(cursor, dst, target) - start;
}

struct PacketMemory {
    MoveLocation index;
    u32 data_offset; u32 stride_offset; u32 frame_offset; u32 offset;
    PacketAddress mode;
};
struct PacketView {
    MoveLocation index; MoveLocation base_low; MoveLocation base_high;
    MoveLocation stride_low; MoveLocation stride_high;
    u64 stride; u32 offset; bool dynamic_stride; PacketViewAddress mode;
};

MoveLocation packet_location(ColorPositions positions, usize slot) on cpu {
    Span rows = positions.rows;
    address source = unsafe_address(address_bits(rows.data) + slot * u64(rows.stride));
    ColorPlace place = unsafe_position<ColorPlace>(source);
    if (place.kind == ColorPlaceKind::Xmm) { return packet_register(u8(place.value)); }
    return resource_stack(u64(u32(place.value)));
}

u64 emit_packet_step(address context, View value, PacketPlanDecision decision, ColorPositions positions,
    PacketLayout layout, PacketBodyHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
    PacketPlanOperation operation = decision.operation;
    if (operation == PacketPlanOperation::Divide) {
        View result = host.ir.inspect(value.right);
        MoveLocation first = packet_location(positions, result.a);
        MoveLocation second = result.tag == 2 ? packet_location(positions, result.b) : first;
        DivisionOutputs outputs = DivisionOutputs(first, second, result.tag + 1);
        MoveLocation numerator = packet_location(positions, value.a);
        ScalarEncoding scalar = host.scalar(context, value.a);
        bool signed = scalar.kind == ScalarKind::Signed;
        View divisor = host.ir.inspect(value.left);
        u32 constant = division_literal32(divisor, scalar);
        if (constant != 0) { return packet_divide_constant(outputs, numerator, constant, signed, output); }
        return packet_divide_value(outputs, numerator, packet_location(positions, divisor.a), signed, output);
    }
    if (operation == PacketPlanOperation::LocalStore) {
        return packet_copy(packet_location(positions, positions.registers + value.a), packet_location(positions, value.b), output);
    }
    if (operation == PacketPlanOperation::Store) {
        u32 data = u32(value.a) * layout.buffer_bytes;
        MoveLocation unused = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        PacketMemory memory = PacketMemory(unused, data, data + layout.stride_field, layout.slots, u32(value.c), decision.dataset);
        return packet_write(packet_location(positions, value.d), memory, output);
    }
    MoveLocation target = packet_location(positions, value.a);
    if (operation == PacketPlanOperation::Constant) {
        return packet_constant(target, u32(value.b), output);
    }
    if (operation == PacketPlanOperation::Index) {
        return packet_index(target, layout.sparse != 0, layout.slots, output);
    }
    if (operation == PacketPlanOperation::Uniform) {
        return packet_uniform(target, u32(value.b), host.scalar(context, value.a).kind == ScalarKind::Boolean, output);
    }
    if (operation == PacketPlanOperation::LocalLoad) {
        return packet_copy(target, packet_location(positions, positions.registers + value.b), output);
    }
    if (operation == PacketPlanOperation::ViewLoad) {
        View stride = host.ir.inspect(value.left);
        MoveLocation unused = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        PacketView input = PacketView(packet_location(positions, value.b),
            packet_location(positions, indexed_pair_word(value.first, 0u64)),
            packet_location(positions, indexed_pair_word(value.first, 1u64)),
            unused, unused, u64(stride.a), u32(value.c), stride.tag != 0, decision.view);
        if (input.dynamic_stride) {
            input.stride_low = packet_location(positions, indexed_pair_word(stride.first, 0u64));
            input.stride_high = packet_location(positions, indexed_pair_word(stride.first, 1u64));
        }
        return packet_view(target, input, host.scalar(context, value.a).kind == ScalarKind::Boolean, output);
    }
    if (operation == PacketPlanOperation::Load) {
        MoveLocation index = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        if (decision.dataset == PacketAddress::Uniform || decision.dataset == PacketAddress::Gather) {
            index = packet_location(positions, value.c);
        }
        u32 data = u32(value.b) * layout.buffer_bytes;
        PacketMemory memory = PacketMemory(index, data, data + layout.stride_field, layout.slots, u32(value.d), decision.dataset);
        return packet_read(target, memory, host.scalar(context, value.a).kind == ScalarKind::Boolean, output);
    }
    if (operation == PacketPlanOperation::Math) {
        MoveLocation first = packet_location(positions, indexed_pair_word(value.first, 0u64));
        MoveLocation last = packet_location(positions, indexed_pair_word(value.first, value.first.count - 1u64));
        return packet_math(target, first, last, decision.math, output);
    }
    MoveLocation first = packet_location(positions, value.b);
    if (operation == PacketPlanOperation::Copy) {
        return packet_copy(target, first, output);
    }
    if (operation == PacketPlanOperation::Unary) {
        return packet_unary(target, first, ValueUnaryOperation(u32(value.c)), output);
    }
    if (operation == PacketPlanOperation::Cast) {
        return packet_cast(target, first, host.scalar(context, value.b), host.scalar(context, value.a), output);
    }
    MoveLocation second = packet_location(positions, value.c);
    if (operation == PacketPlanOperation::Binary) {
        BinaryOperation binary = value_binary_operation(host.ir.inspect(value.right).tag);
        return packet_binary(target, first, second, host.scalar(context, value.b), binary, decision.uniform_count != 0, output);
    }
    return packet_select(target, first, second, packet_location(positions, value.d), output);
}

// Records are in preorder. Child counts exclude the parent record.
struct PacketBodyStep {
    address instruction;
    usize first_count;
    usize second_count;
    PacketPlanDecision decision;
};

extern "C" void emit_packet_body(address context, address emitter, Span code, ControlTargets targets, PacketLayout layout, PacketBodyHost host) on cpu
    requires nonzero(host.mark) requires nonzero(host.publish)
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.control.emit) requires nonzero(host.control.load) requires nonzero(host.control.buffer)
    requires nonzero(host.control.commit) {
    ColorPositions positions = unsafe_position<ColorPositions>(context);
    u64 index = 0u64;
    while (index < code.count) {
        address entry = unsafe_address(address_bits(code.data) + index * u64(code.stride));
        ScalarCursor cursor = host.mark(emitter);
        PacketBodyStep step = unsafe_position<PacketBodyStep>(entry);
        View value = host.ir.inspect(Node(step.instruction, N_INSTRUCTION));
        PacketPlanOperation operation = step.decision.operation;
        if (u32(operation) >= u32(PacketPlanOperation::If)) {
            address first = unsafe_address(address_bits(entry) + u64(code.stride));
            address second = unsafe_address(address_bits(first) + step.first_count * u64(code.stride));
            emit_control(context, emitter, ControlInstruction(Span(first, step.first_count, code.stride),
                Span(second, step.second_count, code.stride), u64(value.a), native_control_kind(value.tag)), targets, host.control);
        } else {
            u64 capacity = 256u64;
            if (operation == PacketPlanOperation::ViewLoad) { capacity = 1024u64; }
            if (operation == PacketPlanOperation::Load || operation == PacketPlanOperation::Store) { capacity = 512u64; }
            NativeCodeBuffer buffer = host.reserve(emitter, capacity);
            address output = unsafe_address(address_bits(buffer.image) + buffer.end);
            u64 count = emit_packet_step(context, value, step.decision, positions, layout, host, output);
            host.commit(emitter, buffer.end + count);
        }
        host.publish(emitter, step.instruction, cursor);
        index += 1u64 + step.first_count + step.second_count;
    }
}

// A scalar input index is zero extended before the existing address arithmetic.
u8 address_shift(u32 stride) on cpu {
    u8 shift = u8(0);
    while (shift < u8(3) && (stride & (1 << u32(shift))) == 0) { shift += u8(1); }
    return shift;
}

u64 address_scale_at(u64 at, u8 index, u32 stride) on cpu {
    if (stride == 1) { return at; }
    u32 remaining = stride;
    u8 shift = u8(0);
    while (remaining > 1 && (remaining & 1) == 0) { remaining >>= 1; shift += u8(1); }
    u64 cursor = at;
    if (remaining == 1) {
        cursor = encode_operand_instruction(cursor, u16(0xc1), u8(0), true, u8(4), move_gpr(index));
        put_byte(cursor, shift);
        return cursor + 1u64;
    }
    cursor = encode_operand_instruction(cursor, u16(0x69), u8(0), true, index, move_gpr(index));
    write_word(cursor, stride);
    return cursor + 4u64;
}

extern "C" AddressEncoding scaled_memory(u8 base, u8 index, u32 stride, u32 offset, address output) on cpu {
    u64 start = address_bits(output);
    u8 shift = address_shift(stride);
    u64 cursor = address_scale_at(start, index, stride >> u32(shift));
    return AddressEncoding(cursor - start, offset, base, index, shift);
}

extern "C" AddressEncoding value_memory(u8 base, MoveLocation index, u32 stride, u32 offset, address output) on cpu {
    u8 shift = address_shift(stride);
    if ((stride >> u32(shift)) == 1 && index.kind == MOVE_GPR) {
        return AddressEncoding(0u64, offset, base, index.base, shift);
    }
    u64 start = address_bits(output);
    MoveEncoding loaded = copy_scalar_move(move_gpr(u8(0)), index, 4, false, output);
    u64 cursor = address_scale_at(start + loaded.length, u8(0), stride >> u32(shift));
    return AddressEncoding(cursor - start, offset, base, u8(0), shift);
}

// The allocation chooses scalar or packet input; both use the same stride arithmetic.
u64 address_word_at(u64 at, MoveLocation src, bool packet, u8 lane, u8 gpr) on cpu {
    if (packet) { return packet_lane_at(at, src, lane, gpr); }
    return at + copy_scalar_move(move_gpr(gpr), src, 4, false, unsafe_address(at)).length;
}

AddressEncoding view_memory_at(u8 base, MoveLocation index, u64 stride, u32 offset,
    bool packet, u8 lane, address output) on cpu {
    u64 start = address_bits(output);
    if (stride <= 0xffffffffu64) {
        if (!packet) { return value_memory(base, index, u32(stride), offset, output); }
        u64 cursor = packet_lane_at(start, index, lane, u8(0));
        AddressEncoding result = scaled_memory(base, u8(0), u32(stride), offset, unsafe_address(cursor));
        result.length += cursor - start;
        return result;
    }
    u64 cursor = start + copy_scalar_move(move_gpr(u8(1)), MoveLocation(stride, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0)), 8, false, output).length;
    cursor = address_word_at(cursor, index, packet, lane, u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(1));
    return AddressEncoding(cursor - start, offset, base, u8(0), u8(0));
}

u64 address_pair_at(u64 at, MoveLocation low, MoveLocation high, bool packet, u8 lane, u8 target) on cpu {
    u64 cursor = address_word_at(at, high, packet, lane, target);
    cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(4), target);
    put_byte(cursor, u8(32));
    cursor = address_word_at(cursor + 1u64, low, packet, lane, u8(0));
    return register_instruction(cursor, u16(0x09), u8(0), true, u8(0), target);
}

extern "C" u64 effective_address(u8 target, MoveLocation memory, address output) on cpu {
    u64 start = address_bits(output);
    return encode_operand_instruction(start, u16(0x8d), u8(0), true, target, memory) - start;
}

AddressEncoding view_memory_dynamic_at(u8 base, MoveLocation index, MoveLocation low,
    MoveLocation high, u32 offset, bool packet, u8 lane, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = address_pair_at(start, low, high, packet, lane, u8(1));
    cursor = address_word_at(cursor, index, packet, lane, u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(1));
    return AddressEncoding(cursor - start, offset, base, u8(0), u8(0));
}

struct ViewElement {
    MoveLocation low; MoveLocation high; MoveLocation index;
    MoveLocation stride_low; MoveLocation stride_high; u64 stride; u32 dynamic;
};

AddressEncoding view_element_memory(ViewElement source, u32 offset, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = address_pair_at(start, source.low, source.high, false, u8(0), u8(15));
    address memory_output = unsafe_address(cursor);
    AddressEncoding memory;
    if (source.dynamic != 0) {
        memory = view_memory_dynamic_at(u8(15), source.index, source.stride_low, source.stride_high,
            offset, false, u8(0), memory_output);
    } else {
        memory = view_memory_at(u8(15), source.index, source.stride, offset, false, u8(0), memory_output);
    }
    memory.length += cursor - start;
    return memory;
}

u64 view_element_base(ViewElement source, address output) on cpu {
    AddressEncoding memory = view_element_memory(source, 0, output);
    u64 start = address_bits(output);
    MoveLocation operand = MoveLocation(u64(memory.offset), MOVE_MEMORY, memory.base, memory.index, memory.shift);
    return memory.length + effective_address(u8(15), operand, unsafe_address(start + memory.length));
}

extern "C" u64 packet_condition(address context, usize register, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation source = packet_location(unsafe_position<ColorPositions>(context), register);
    return packet_lane_at(start, source, u8(0), u8(0)) - start;
}

extern "C" AddressEncoding packet_view_memory(u8 base, MoveLocation index, u8 lane, u64 stride, u32 offset, address output) on cpu {
    return view_memory_at(base, index, stride, offset, true, lane, output);
}

extern "C" AddressEncoding packet_view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u8 lane, u32 offset, address output) on cpu {
    return view_memory_dynamic_at(base, index, low, high, offset, true, lane, output);
}

u64 offset_address(u32 stride, address output) on cpu {
    u64 start = address_bits(output);
    u8 shift = address_shift(stride);
    if ((stride >> u32(shift)) == 1 && stride <= 8) {
        MoveLocation memory = MoveLocation(0u64, MOVE_MEMORY, u8(15), u8(0), shift);
        return effective_address(u8(15), memory, output);
    }
    u64 cursor = address_scale_at(start, u8(0), stride);
    put_byte(cursor, u8(0x49)); put_byte(cursor + 1u64, u8(0x01)); put_byte(cursor + 2u64, u8(0xc7));
    return cursor + 3u64 - start;
}

u64 load_index_at(u64 at, MoveLocation index, ScalarEncoding scalar) on cpu {
    return at + copy_scalar_move(move_gpr(u8(0)), index, scalar.width / 8,
        scalar.kind == ScalarKind::Signed, unsafe_address(at)).length;
}

u64 pointer_element_base(MoveLocation low, MoveLocation high, MoveLocation index,
    ScalarEncoding scalar, u32 stride, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = address_pair_at(start, low, high, false, u8(0), u8(15));
    cursor = load_index_at(cursor, index, scalar);
    return cursor - start + offset_address(stride, unsafe_address(cursor));
}

u64 indexed_word_at(u64 at, MoveLocation index, ScalarEncoding scalar, u32 word) on cpu {
    u64 cursor = load_index_at(at, index, scalar);
    put_byte(cursor, u8(0x05));
    write_word(cursor + 1u64, word);
    return cursor + 5u64;
}

u64 indexed_address(MoveLocation index, ScalarEncoding scalar, u32 stride,
    u32 word, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (word == 0) {
        cursor += copy_scalar_move(move_gpr(u8(0)), index, 4, false, output).length;
    } else {
        cursor = indexed_word_at(start, index, scalar, word);
    }
    return cursor - start + offset_address(stride, unsafe_address(cursor));
}

AddressEncoding indexed_memory(MoveLocation index, ScalarEncoding scalar, u32 stride,
    u32 word, address output) on cpu {
    if (word == 0) { return value_memory(u8(15), index, stride, 0, output); }
    u64 start = address_bits(output);
    u64 cursor = indexed_word_at(start, index, scalar, word);
    AddressEncoding result = value_memory(u8(15), move_gpr(u8(0)), stride, 0, unsafe_address(cursor));
    result.length += cursor - start;
    return result;
}

// The caller supplies the actual descriptor field offsets in the table held by R12.
u64 buffer_field_at(u64 at, u32 offset, u16 opcode, u8 reg) on cpu {
    MoveLocation field = MoveLocation(u64(offset), MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
    return encode_operand_instruction(at, opcode, u8(0), true, reg, field);
}

u64 buffer_index_at(u64 at, u32 stride_offset, u8 index, u8 base) on cpu {
    u64 cursor = buffer_field_at(at, stride_offset, u16(0xaf0f), index);
    if (base != NO_INDEX) {
        cursor = register_instruction(cursor, u16(0x01), u8(0), true, index, base);
    }
    return cursor;
}

u64 dataset_index(u32 data, u32 stride, MoveLocation index, ScalarEncoding scalar,
    bool materialize, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = buffer_field_at(start, data, u16(0x8b), u8(15));
    cursor = load_index_at(cursor, index, scalar);
    return buffer_index_at(cursor, stride, u8(0), materialize ? u8(15) : NO_INDEX) - start;
}

MoveLocation buffer_element_operand(u32 offset) on cpu {
    return MoveLocation(u64(offset), MOVE_MEMORY, u8(15), u8(0), u8(0));
}

u64 buffer_selection_at(u64 at, u32 data_offset, u32 stride_offset, u32 word) on cpu {
    u64 cursor = buffer_field_at(at, data_offset, u16(0x8b), u8(15));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(3), u8(0));
    cursor = buffer_index_at(cursor, stride_offset, u8(0), NO_INDEX);
    cursor = encode_operand_instruction(cursor, u16(0x83), u8(0), false, u8(7), buffer_element_operand(word));
    put_byte(cursor, u8(0));
    return cursor + 1u64;
}

extern "C" u64 range_arguments(address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = register_instruction(start, u16(0x89), u8(0), true, u8(1), u8(12));
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(2), u8(13));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(14));
    return register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(3)) - start;
}

u64 range_exit_field(u64 at, bool packet) on cpu {
    if (packet) {
        // EBX and R14D are assigned as 32-bit values by the range ABI.
        // Compute the packet end in 64 bits before comparing.
        MoveLocation end = MoveLocation(4u64, MOVE_MEMORY, u8(3), NO_INDEX, u8(0));
        u64 cursor = encode_operand_instruction(at, u16(0x8d), u8(0), true, u8(0), end);
        cursor = register_instruction(cursor, u16(0x39), u8(0), true, u8(14), u8(0));
        return condition_field(cursor, u8(0x87));
    }
    u64 cursor = register_instruction(at, u16(0x39), u8(0), false, u8(14), u8(3));
    return condition_field(cursor, u8(0x83));
}

extern "C" u64 range_bounds(bool packet, address output) on cpu {
    u64 start = address_bits(output);
    u64 field = range_exit_field(start, packet);
    write_word(field, 0);
    return field + 4u64 - start;
}

u64 range_step_at(u64 at, u8 reg, u8 count) on cpu {
    if (count == u8(1)) { return register_instruction(at, u16(0xff), u8(0), false, u8(0), reg); }
    u64 immediate = register_instruction(at, u16(0x83), u8(0), false, u8(0), reg);
    put_byte(immediate, count);
    return immediate + 1u64;
}

struct ScalarRangePrefix {
    u64 end;
    u64 exit_field;
    u64 skip_field;
};

ScalarRangePrefix scalar_range_prefix(address image, u64 begin, u32 selected,
    u32 data_offset, u32 stride_offset, u32 word) on cpu {
    u64 base = address_bits(image);
    u64 exit_field = range_exit_field(base + begin, false);
    write_word(exit_field, 0);
    u64 end = exit_field + 4u64;
    if (selected == 0) {
        return ScalarRangePrefix(end - base, exit_field - base, 0u64);
    }

    u64 compare_end = buffer_selection_at(end, data_offset, stride_offset, word);
    u64 skip_field = condition_field(compare_end, u8(0x84));
    write_word(skip_field, 0);
    return ScalarRangePrefix(skip_field + 4u64 - base, exit_field - base, skip_field - base);
}

extern "C" RangeEnd finish_range(address image, u64 at, u64 begin, u64 exit_field,
    u64 skip_field, u8 step) on cpu {
    u64 base = address_bits(image);
    if (skip_field != 0u64) {
        patch_relative(unsafe_address(base + skip_field), skip_field, at);
    }

    u64 back_edge = at;
    if (step != u8(0)) {
        back_edge = range_step_at(base + at, u8(3), step) - base;
    }
    u64 end = back_edge + jump_to(back_edge, begin, unsafe_address(base + back_edge));
    patch_relative(unsafe_address(base + exit_field), exit_field, end);
    return RangeEnd(back_edge, end);
}

struct SparseRangeEncoding {
    u64 length;
    u64 tail_field;
};

SparseRangeEncoding sparse_collect(u32 data_offset, u32 stride_offset,
    u32 word, u32 slots, address output) on cpu {
    u64 start = address_bits(output);
    u64 collect = register_instruction(start, u16(0x31), u8(0), false, u8(1), u8(1));
    u64 tail = range_exit_field(collect, false);
    write_word(tail, 0);
    u64 cursor = buffer_selection_at(tail + 4u64, data_offset, stride_offset, word);
    u64 skip = condition_field(cursor, u8(0x84));
    MoveLocation slot = MoveLocation(u64(slots), MOVE_MEMORY, u8(4), u8(1), u8(2));
    cursor = store_bits(skip + 4u64, slot, u8(3), 4);
    cursor = range_step_at(cursor, u8(1), u8(1));
    patch_relative(unsafe_address(skip), skip - start, cursor - start);
    cursor = range_step_at(cursor, u8(3), u8(1));
    cursor = register_instruction(cursor, u16(0x83), u8(0), false, u8(7), u8(1));
    put_byte(cursor, u8(4));
    u64 back = condition_field(cursor + 1u64, u8(0x85));
    patch_relative(unsafe_address(back), back - start, collect - start);
    return SparseRangeEncoding(back + 4u64 - start, tail - start);
}

// Restore the first pending index when the final group was incomplete.
u64 sparse_tail(u32 slots, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = register_instruction(start, u16(0x85), u8(0), false, u8(1), u8(1));
    u64 empty = condition_field(cursor, u8(0x84));
    MoveLocation first = MoveLocation(u64(slots), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = load_bits(empty + 4u64, u8(3), first, false);
    patch_relative(unsafe_address(empty), empty - start, cursor - start);
    return cursor - start;
}

extern "C" PacketAddress packet_dataset_address(u32 stride, u32 word, bool selected,
    bool writes_binding, bool element, bool uniform) on cpu {
    if (stride > 0x7fffffff) { return PacketAddress::Unsupported; }
    if (!element && writes_binding) { return PacketAddress::Unsupported; }
    bool broadcast = uniform && !writes_binding;
    u64 last = u64(word) * 4u64;
    if (element && !broadcast) { last += u64(stride) * 3u64; }
    if (last > 0x7fffffffu64) { return PacketAddress::Unsupported; }
    if (broadcast) { return PacketAddress::Uniform; }
    if (!element) { return PacketAddress::Gather; }
    if (selected) { return PacketAddress::Selected; }
    if (stride == 4) { return PacketAddress::Contiguous; }
    return PacketAddress::Dense;
}

extern "C" PacketViewAddress packet_view_address(u32 word, bool uniform_base,
    bool uniform_index, bool uniform_stride) on cpu {
    if (u64(word) * 4u64 > 0x7fffffffu64) { return PacketViewAddress::Unsupported; }
    if (!uniform_base) { return PacketViewAddress::Varying; }
    if (uniform_index && uniform_stride) { return PacketViewAddress::Broadcast; }
    return PacketViewAddress::Shared;
}

u64 packet_memory_at(u64 at, PacketMemory memory, u8 lane) on cpu {
    u64 cursor = at;
    if (memory.mode == PacketAddress::Selected) {
        MoveLocation index = MoveLocation(u64(memory.frame_offset + u32(lane) * 4), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), index);
    } else if (memory.mode == PacketAddress::Uniform) {
        cursor = packet_lane_at(cursor, memory.index, u8(0), u8(0));
    } else {
        put_byte(cursor, u8(0x89)); put_byte(cursor + 1u64, u8(0xd8)); cursor += 2u64;
        if (lane != u8(0)) {
            cursor = register_instruction(cursor, u16(0x83), u8(0), true, u8(0), u8(0));
            put_byte(cursor, lane); cursor += 1u64;
        }
    }
    cursor = buffer_field_at(cursor, memory.data_offset, u16(0x8b), u8(15));
    return buffer_index_at(cursor, memory.stride_offset, u8(0), NO_INDEX);
}

u64 packet_contiguous_field(u64 at, u32 stride_offset) on cpu {
    u64 cursor = buffer_field_at(at, stride_offset, u16(0x83), u8(7));
    put_byte(cursor, u8(4));
    return condition_field(cursor + 1u64, u8(0x85));
}

u64 packet_combine_at(u64 at, u8 target, bool broadcast) on cpu {
    if (broadcast) { return packet_shuffle_at(at, target, target, u8(0)); }
    u64 cursor = packet_instruction(at, u8(0x66), u8(0x62), target, u8(1));
    cursor = packet_instruction(cursor, u8(0x66), u8(0x62), u8(2), u8(3));
    return packet_instruction(cursor, u8(0x66), u8(0x6c), target, u8(2));
}

u64 packet_dataset_gather_at(u64 at, MoveLocation dst, PacketMemory memory, bool boolean) on cpu {
    u64 cursor = buffer_field_at(at, memory.data_offset, u16(0x8b), u8(15));
    MoveLocation operand = buffer_element_operand(memory.offset);
    for (ordinal in 0..4) {
        u8 lane = u8(ordinal);
        cursor = packet_lane_at(cursor, memory.index, lane, u8(0));
        cursor = buffer_index_at(cursor, memory.stride_offset, u8(0), NO_INDEX);
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, lane, operand);
    }
    cursor = packet_combine_at(cursor, u8(0), false);
    cursor = packet_from_word_at(cursor, boolean, u8(0));
    return packet_store_at(cursor, dst, u8(0));
}

u64 packet_read(MoveLocation dst, PacketMemory memory, bool boolean, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u64 done = 0u64;
    if (memory.mode == PacketAddress::Gather) {
        return packet_dataset_gather_at(start, dst, memory, boolean) - start;
    }
    u8 target = packet_destination(dst);
    MoveLocation operand = buffer_element_operand(memory.offset);
    if (memory.mode == PacketAddress::Contiguous) {
        u64 strided = packet_contiguous_field(cursor, memory.stride_offset);
        cursor = packet_memory_at(strided + 4u64, memory, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x100f), u8(0), false, target, operand);
        done = jump_field(cursor); cursor = done + 4u64;
        patch_relative(unsafe_address(strided), strided, cursor);
    }
    bool broadcast = memory.mode == PacketAddress::Uniform;
    u8 count = u8(4); if (broadcast) { count = u8(1); }
    for (ordinal in 0..u32(count)) {
        u8 lane = u8(ordinal);
        cursor = packet_memory_at(cursor, memory, lane);
        u8 to = lane; if (lane == u8(0)) { to = target; }
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, to, operand);
    }
    cursor = packet_combine_at(cursor, target, broadcast);
    if (done != 0u64) { patch_relative(unsafe_address(done), done, cursor); }
    cursor = packet_from_word_at(cursor, boolean, target);
    return packet_store_at(cursor, dst, target) - start;
}

u64 packet_write(MoveLocation src, PacketMemory memory, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u64 done = 0u64;
    u8 source = u8(1);
    if (src.kind == MOVE_XMM) { source = src.base; }
    else { cursor = packet_load_at(cursor, src, source); }
    MoveLocation operand = buffer_element_operand(memory.offset);
    if (memory.mode == PacketAddress::Contiguous) {
        u64 strided = packet_contiguous_field(cursor, memory.stride_offset);
        cursor = packet_memory_at(strided + 4u64, memory, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x110f), u8(0), false, source, operand);
        done = jump_field(cursor); cursor = done + 4u64;
        patch_relative(unsafe_address(strided), strided, cursor);
    }
    for (ordinal in 0..4) {
        u8 lane = u8(ordinal);
        cursor = packet_memory_at(cursor, memory, lane);
        u8 from = source;
        if (lane != u8(0)) {
            cursor = packet_shuffle_at(cursor, u8(0), source, lane * u8(0x55)); from = u8(0);
        }
        cursor = encode_operand_instruction(cursor, u16(0x7e0f), u8(0x66), false, from, operand);
    }
    if (done != 0u64) { patch_relative(unsafe_address(done), done, cursor); }
    return cursor - start;
}

u64 packet_view(MoveLocation dst, PacketView input, bool boolean, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    bool shared = input.mode != PacketViewAddress::Varying;
    bool broadcast = input.mode == PacketViewAddress::Broadcast;
    if (shared) {
        cursor = address_pair_at(cursor, input.base_low, input.base_high, true, u8(0), u8(15));
    }
    u8 count = u8(4);
    if (broadcast) { count = u8(1); }
    for (ordinal in 0..u32(count)) {
        u8 lane = u8(ordinal);
        if (!shared) {
            cursor = address_pair_at(cursor, input.base_low, input.base_high, true, lane, u8(15));
        }
        AddressEncoding memory;
        if (input.dynamic_stride) {
            memory = view_memory_dynamic_at(u8(15), input.index, input.stride_low, input.stride_high,
                input.offset, true, lane, unsafe_address(cursor));
        } else {
            memory = view_memory_at(u8(15), input.index, input.stride, input.offset, true, lane, unsafe_address(cursor));
        }
        cursor += memory.length;
        MoveLocation operand = MoveLocation(u64(memory.offset), MOVE_MEMORY, memory.base, memory.index, memory.shift);
        // Extract all original lanes before the allocated destination is overwritten.
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, lane, operand);
    }
    cursor = packet_combine_at(cursor, u8(0), broadcast);
    cursor = packet_from_word_at(cursor, boolean, u8(0));
    return packet_store_at(cursor, dst, u8(0)) - start;
}

u64 atomic_input(u64 at, MoveLocation source, u32 size) on cpu {
    MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), source, size, false, unsafe_address(at));
    return at + load.length;
}

u64 atomic_update(u64 at, AtomicOperation operation, bool wide, bool signed) on cpu {
    u16 opcode = u16(0x39);
    if (operation == AtomicOperation::Add) { opcode = u16(0x01); }
    if (operation == AtomicOperation::Sub) { opcode = u16(0x29); }
    if (operation == AtomicOperation::And) { opcode = u16(0x21); }
    if (operation == AtomicOperation::Or) { opcode = u16(0x09); }
    if (operation == AtomicOperation::Xor) { opcode = u16(0x31); }
    u64 cursor = register_instruction(at, opcode, u8(0), wide, u8(2), u8(1));
    if (operation == AtomicOperation::Min || operation == AtomicOperation::Max) {
        bool minimum = operation == AtomicOperation::Min;
        u8 condition = signed ? (minimum ? u8(0x4f) : u8(0x4c)) : (minimum ? u8(0x47) : u8(0x42));
        cursor = register_instruction(cursor, u16(0x0f) | (u16(condition) << 8), u8(0), wide, u8(1), u8(2));
    }
    return cursor;
}

u64 load_split_words(u64 at, MoveLocation low, MoveLocation high) on cpu {
    // Indexed tile loads can use EAX in both addresses. Read the high word first.
    u64 cursor = load_bits(at, u8(1), high, false);
    cursor = load_bits(cursor, u8(0), low, false);
    cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(4), u8(1));
    put_byte(cursor, u8(32));
    return register_instruction(cursor + 1u64, u16(0x09), u8(0), true, u8(1), u8(0));
}

u64 shift_high_word(u64 at, u8 source) on cpu {
    u64 immediate = register_instruction(at, u16(0xc1), u8(0), true, u8(5), source);
    put_byte(immediate, u8(32));
    return immediate + 1u64;
}

u64 store_split_words(u64 at, MoveLocation low, MoveLocation high, u8 source) on cpu {
    u64 cursor = store_bits(at, low, source, 4);
    cursor = shift_high_word(cursor, source);
    return store_bits(cursor, high, source, 4);
}

enum AtomicOperation:u32 {
    Load=0; Store=1; Add=2; Sub=3; Exchange=4; CompareExchange=5;
    Min=6; Max=7; And=8; Or=9; Xor=10;
};
enum AtomicStorage:u32 { Shared=0; Private=1; Split=2; };
struct AtomicSequence {
    MoveLocation low; MoveLocation high; MoveLocation first; MoveLocation second;
    ScalarEncoding scalar;
    AtomicOperation operation; AtomicStorage storage;
};

u64 atomic_split_at(u64 at, AtomicSequence input) on cpu {
    AtomicOperation operation = input.operation;
    if (operation == AtomicOperation::Load) {
        return load_split_words(at, input.low, input.high);
    }
    u64 cursor = atomic_input(at, input.first, 8);
    if (operation == AtomicOperation::Store) {
        return store_split_words(cursor, input.low, input.high, u8(0));
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(2));
    cursor = load_split_words(cursor, input.low, input.high);
    if (operation == AtomicOperation::Exchange) {
        return store_split_words(cursor, input.low, input.high, u8(2));
    }
    if (operation == AtomicOperation::CompareExchange) {
        cursor = register_instruction(cursor, u16(0x39), u8(0), true, u8(2), u8(0));
        u64 different = condition_field(cursor, u8(0x85));
        cursor = register_instruction(different + 4u64, u16(0x89), u8(0), true, u8(0), u8(1));
        cursor = atomic_input(cursor, input.second, 8);
        cursor = store_split_words(cursor, input.low, input.high, u8(0));
        cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(1), u8(0));
        patch_relative(unsafe_address(different), different - at, cursor - at);
        return cursor;
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(1));
    cursor = atomic_update(cursor, operation, true, input.scalar.kind == ScalarKind::Signed);
    return store_split_words(cursor, input.low, input.high, u8(1));
}

u64 atomic_contiguous_at(u64 at, AtomicSequence input) on cpu {
    AtomicOperation operation = input.operation;
    u32 size = input.scalar.width / 8;
    bool wide = size == 8;
    bool shared = input.storage == AtomicStorage::Shared;
    u8 lock = shared ? u8(0xf0) : u8(0);
    if (operation == AtomicOperation::Load) {
        return encode_operand_instruction(at, u16(0x8b), u8(0), wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::CompareExchange) {
        u64 cursor = atomic_input(at, input.second, size);
        cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(2));
        cursor = atomic_input(cursor, input.first, size);
        return encode_operand_instruction(cursor, u16(0xb10f), lock, wide, u8(2), input.low);
    }
    u64 cursor = atomic_input(at, input.first, size);
    if (operation == AtomicOperation::Store) {
        return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::Add || operation == AtomicOperation::Sub) {
        if (operation == AtomicOperation::Sub) {
            cursor = register_instruction(cursor, u16(0xf7), u8(0), wide, u8(3), u8(0));
        }
        return encode_operand_instruction(cursor, u16(0xc10f), lock, wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::Exchange && shared) {
        // Memory XCHG is implicitly locked.
        return encode_operand_instruction(cursor, u16(0x87), u8(0), wide, u8(0), input.low);
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), wide, u8(0), input.low);
    if (operation == AtomicOperation::Exchange) {
        return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(2), input.low);
    }
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(1));
    cursor = atomic_update(cursor, operation, wide, input.scalar.kind == ScalarKind::Signed);
    if (shared) {
        // CMPXCHG supplies the new observed value in RAX after a failed exchange.
        cursor = encode_operand_instruction(cursor, u16(0xb10f), u8(0xf0), wide, u8(1), input.low);
        u64 again = condition_field(cursor, u8(0x85));
        patch_relative(unsafe_address(again), again - at, begin - at);
        return again + 4u64;
    }
    return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(1), input.low);
}

u64 compact_disp32(u64 at, u32 instruction, u32 displacement) on cpu {
    put_byte(at, u8(instruction));
    put_byte(at + 1u64, u8(instruction >> 8));
    put_byte(at + 2u64, u8(instruction >> 16));
    write_word(at + 3u64, displacement);
    return at + 7u64;
}

u64 load_range_binding(u64 at, u8 target, u32 offset) on cpu {
    MoveLocation slot = MoveLocation(u64(offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    return encode_operand_instruction(at, u16(0x8b), u8(0), true, target, slot);
}

u64 compact_flag_test(u64 at, u32 word) on cpu {
    u64 immediate = compact_disp32(at, 0xba8341, word); // cmp dword [r10 + word], 0
    put_byte(immediate, u8(0));
    return immediate + 1u64;
}

u64 compact_advance(u64 at, u8 pointer, u8 stride, u32 bytes) on cpu {
    u64 cursor = register_instruction(at, u16(0x03), u8(0), true, pointer, stride);
    cursor = register_instruction(cursor, u16(0x81), u8(0), true, u8(5), pointer);
    write_word(cursor, bytes);
    return cursor + 4u64;
}

u64 copy_record_at(u64 at, u32 bytes) on cpu {
    u64 cursor = at;
    u32 vectors = bytes / 16;
    if (vectors != 0) {
        if (vectors != 1) {
            cursor = move_immediate(cursor, u8(1), u64(vectors), false);
        }
        u64 begin = cursor;
        // movups xmm0,[rsi]; movups [rdi],xmm0; advance both record cursors.
        write_word(cursor, 0x0f06100f);
        write_word(cursor + 4u64, 0x83480711);
        write_word(cursor + 8u64, 0x834810c6);
        put_byte(cursor + 12u64, u8(0xc7));
        put_byte(cursor + 13u64, u8(16));
        cursor += 14u64;
        if (vectors != 1) {
            cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(1)); // dec ecx
            u64 again = condition_field(cursor, u8(0x85));
            patch_relative(unsafe_address(again), again - at, begin - at);
            cursor = again + 4u64;
        }
    }
    if ((bytes & 8) != 0) { put_byte(cursor, u8(0x48)); put_byte(cursor + 1u64, u8(0xa5)); cursor += 2u64; }
    if ((bytes & 4) != 0) { put_byte(cursor, u8(0xa5)); cursor += 1u64; }
    if ((bytes & 2) != 0) { put_byte(cursor, u8(0x66)); put_byte(cursor + 1u64, u8(0xa5)); cursor += 2u64; }
    if ((bytes & 1) != 0) { put_byte(cursor, u8(0xa4)); cursor += 1u64; }
    return cursor;
}

const u32 SAVE_R12 = 0x1000;
const u32 SAVE_R12_R13 = 0x3000;

LeafRange leaf_begin(LeafOutput output, u32 saved) on cpu {
    u64 start = address_bits(output.image);
    u64 cursor = start;
    Operation empty = Operation(u8(0), u8(0), u16(0), 0);
    array<Operation,8> operations = array<Operation,8>(empty, empty, empty, empty, empty, empty, empty, empty);
    u64 count = 0u64;
    for (register in 0..16) {
        if ((saved & (u32(1) << register)) != 0) {
            cursor += stack_register(u8(register), false, unsafe_address(cursor));
            address row = unsafe_address(address_bits(address_of(operations)) + count * size_of<Operation>());
            unsafe_position<Operation>(row) = Operation(u8(cursor - start), u8(register), u16(0), 0);
            count += 1u64;
        }
    }
    u64 prolog = cursor - start;
    u64 unwind_bytes = unwind_write(address_of(operations), count, u8(prolog), output.unwind);
    return LeafRange(prolog, unwind_bytes, u32(count * 8u64));
}

u64 leaf_return(u64 at, u32 saved) on cpu {
    put_byte(at, u8(0x31));
    put_byte(at + 1u64, u8(0xc0));
    u64 cursor = at + 2u64;
    u32 register = 16;
    while (register != 0) {
        register -= 1;
        if ((saved & (u32(1) << register)) != 0) {
            cursor += stack_register(u8(register), true, unsafe_address(cursor));
        }
    }
    put_byte(cursor, u8(0xc3));
    return cursor + 1u64;
}

CollectiveLeaf collective_leaf(LeafRange layout, u64 end, CollectiveLeafOutput output,
    u64 identity, u64 reference_count) on cpu {
    LeafRange range = layout;
    u64 base = address_bits(output.range.image);
    u64 function_end = end - base;
    range.end = function_end;
    u64 target = 0u64;
    u64 region_count = 0u64;
    if (reference_count != 0u64) {
        target = align_literal(base, function_end);
        if (target != function_end) {
            tail_region(output.regions, region_count, target, TailRegionKind::Alignment);
            region_count += 1u64;
        }
        write_word(base + target, u32(identity));
        patch_branches(output.range.image, target, output.references, reference_count);
        range.end = target + 4u64;
        tail_region(output.regions, region_count, range.end, TailRegionKind::Literal);
        region_count += 1u64;
    }
    return CollectiveLeaf(range, function_end, target, reference_count, region_count);
}

extern "C" LeafRange compact_count(u32 flags, u32 stride_offset, u32 flag_word, LeafOutput output) on cpu {
    u32 saved = SAVE_R12;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(12), flags + stride_offset);
    cursor = compact_disp32(cursor, 0x918b4c, flags); // mov r10,[rcx + flags]
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(1));
    cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(0), u8(0));
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(8), u8(1));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = compact_flag_test(end + 4u64, flag_word);
    write_word(cursor, 0x0fc2950f); // setne dl; movzx edx,dl
    put_byte(cursor + 4u64, u8(0xb6));
    put_byte(cursor + 5u64, u8(0xd2));
    cursor = register_instruction(cursor + 6u64, u16(0x01), u8(0), false, u8(2), u8(0));
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    // The fifth argument receives the algorithm output; EAX reports success.
    write_word(cursor, 0x24548b4c);
    write_word(cursor + 4u64, 0x02894100 | (range.stack_bytes + 0x28));
    range.end = leaf_return(cursor + 8u64, saved) - image;
    return range;
}

extern "C" LeafRange compact_scatter(CompactScatter input, LeafOutput output) on cpu {
    u32 saved = (u32(1) << 3) | (u32(1) << 6) | (u32(1) << 7) | SAVE_R12_R13 | (u32(1) << 14) | (u32(1) << 15);
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(13), input.source + input.stride_offset);
    cursor = load_range_binding(cursor, u8(14), input.target + input.stride_offset);
    cursor = load_range_binding(cursor, u8(15), input.flags + input.stride_offset);
    cursor = compact_disp32(cursor, 0xb18b48, input.source); // rsi = source
    cursor = compact_disp32(cursor, 0xb98b48, input.target); // rdi = destination
    cursor = compact_disp32(cursor, 0x918b4c, input.flags);  // r10 = flags
    cursor = compact_disp32(cursor, 0x918b48, input.prefix); // rdx = range prefix
    MoveLocation prefix = MoveLocation(0u64, MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), prefix);
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(14));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(7));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(6));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(15));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(3));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(12));
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(12), u8(3));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = compact_flag_test(end + 4u64, input.flag_word);
    u64 skip = condition_field(cursor, u8(0x84));
    cursor = copy_record_at(skip + 4u64, input.record_bytes);
    cursor = compact_advance(cursor, u8(6), u8(13), input.record_bytes);
    cursor = compact_advance(cursor, u8(7), u8(14), input.record_bytes);
    u64 next = jump_field(cursor);
    cursor = next + 4u64;
    patch_relative(unsafe_address(skip), skip - start, cursor - start);
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(6), u8(13));
    patch_relative(unsafe_address(next), next - start, cursor - start);
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(15));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(3));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

u64 sort_range_end(u64 at) on cpu {
    u64 cursor = register_instruction(at, u16(0x39), u8(0), false, u8(8), u8(9)); // cmp r9d,r8d
    return condition_field(cursor, u8(0x83));
}

u64 sort_finish_range(u64 at, u64 start, u64 begin, u64 end) on cpu {
    u64 cursor = register_instruction(at, u16(0xff), u8(0), false, u8(0), u8(9)); // inc r9d
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    return cursor;
}

extern "C" LeafRange sort_key(SortKeyEmission input, LeafOutput output) on cpu {
    u32 saved = SAVE_R12;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(12), input.stride_offset);
    cursor = load_range_binding(cursor, u8(10), 0); // source record base
    cursor = load_range_binding(cursor, u8(11), 3 * input.buffer_bytes);
    if (input.first != 0) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
        cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    }
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = end + 4u64;
    MoveLocation index = MoveLocation(4u64, MOVE_MEMORY, u8(11), u8(9), u8(3));
    MoveLocation key = MoveLocation(u64(input.word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    if (input.first != 0) {
        cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), false, u8(9), index);
    } else {
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), index);
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
        key.index = u8(0);
    }
    if (input.floating != 0 && input.bytes == 8) {
        MoveLocation high = key;
        high.value += 4u64;
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), high);
    }
    u16 load = input.bytes == 1 ? u16(0xb60f) : (input.bytes == 2 ? u16(0xb70f) : u16(0x8b));
    cursor = encode_operand_instruction(cursor, load, u8(0), false, u8(0), key);
    // Each radix pass consumes a 32-bit subkey and its original ordinal.
    if (input.bias != 0) {
        put_byte(cursor, u8(0x35)); // xor eax,imm32
        write_word(cursor + 1u64, input.bias);
        cursor += 5u64;
    } else if (input.floating != 0 && input.bytes == 4) {
        write_word(cursor, 0xfac1c289); // mov edx,eax; sar edx,31
        put_byte(cursor + 4u64, u8(31));
        put_byte(cursor + 5u64, u8(0x81)); // or edx,0x80000000
        put_byte(cursor + 6u64, u8(0xca));
        write_word(cursor + 7u64, 0x80000000);
        cursor = register_instruction(cursor + 11u64, u16(0x31), u8(0), false, u8(2), u8(0));
    } else if (input.floating != 0 && input.bytes == 8) {
        put_byte(cursor, u8(0xc1)); // sar ecx,31
        put_byte(cursor + 1u64, u8(0xf9));
        put_byte(cursor + 2u64, u8(31));
        cursor = register_instruction(cursor + 3u64, u16(0x31), u8(0), false, u8(1), u8(0));
    }
    if (input.descending != 0) {
        cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(2), u8(0)); // not eax
    }
    MoveLocation row = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(9), u8(3));
    cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), false, u8(0), row);
    if (input.first != 0) {
        cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    }
    cursor = sort_finish_range(cursor, start, begin, end);
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

u64 sort_digit(u64 at) on cpu {
    MoveLocation row = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(9), u8(3));
    u64 cursor = encode_operand_instruction(at, u16(0x8b), u8(0), false, u8(0), row);
    write_word(cursor, 0xe083e8d3); // shr eax,cl; and eax,15
    put_byte(cursor + 4u64, u8(15));
    return cursor + 5u64;
}

u64 sort_shift(u64 at, u32 buffer_bytes) on cpu {
    u64 cursor = load_range_binding(at, u8(2), 4 * buffer_bytes);
    MoveLocation params = MoveLocation(0u64, MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
    return encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), params);
}

extern "C" LeafRange sort_histogram(u32 buffer_bytes, LeafOutput output) on cpu {
    u32 saved = 0;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(10), 2 * buffer_bytes);
    cursor = sort_shift(cursor, buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = sort_digit(end + 4u64);
    // Parameter row: shift followed by the range's private counts.
    MoveLocation count = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0xff), u8(0), false, u8(0), count);
    cursor = sort_finish_range(cursor, start, begin, end);
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

extern "C" LeafRange sort_scatter(u32 buffer_bytes, LeafOutput output) on cpu {
    u32 saved = u32(1) << 3;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(10), 2 * buffer_bytes);
    cursor = load_range_binding(cursor, u8(11), 3 * buffer_bytes);
    cursor = sort_shift(cursor, buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = sort_digit(end + 4u64);
    MoveLocation bucket = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(0), bucket);
    MoveLocation next = MoveLocation(0u64, MOVE_MEMORY, u8(0), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(3), next);
    cursor = encode_operand_instruction(cursor, u16(0xff), u8(0), false, u8(0), next);
    MoveLocation source = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(9), u8(3));
    MoveLocation target = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(3), u8(3));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(0), source);
    cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), true, u8(0), target);
    cursor = sort_finish_range(cursor, start, begin, end);
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

extern "C" LeafRange sort_gather(u32 buffer_bytes, u32 stride_offset, u32 record_bytes, LeafOutput output) on cpu {
    u32 saved = (u32(1) << 3) | (u32(1) << 6) | (u32(1) << 7) | SAVE_R12_R13;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    u64 cursor = load_range_binding(start, u8(12), stride_offset);
    cursor = load_range_binding(cursor, u8(13), buffer_bytes + stride_offset);
    cursor = load_range_binding(cursor, u8(10), 0);
    cursor = load_range_binding(cursor, u8(11), buffer_bytes);
    cursor = load_range_binding(cursor, u8(2), 2 * buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    MoveLocation index = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(9), u8(3));
    cursor = encode_operand_instruction(end + 4u64, u16(0x8b), u8(0), false, u8(0), index);
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    MoveLocation source = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(0), u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(6), source);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    MoveLocation target = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(0), u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(7), target);
    cursor = copy_record_at(cursor, record_bytes);
    cursor = sort_finish_range(cursor, start, begin, end);
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

struct AppendBranch {
    u64 length;
    u64 full;
};

AppendBranch reserve_append(u32 counter_slot, u32 counter_word,
    u32 capacity_slot, u32 target_slot, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = buffer_field_at(start, counter_slot, u16(0x8b), u8(15));
    MoveLocation counter = MoveLocation(u64(counter_word), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    MoveLocation capacity = MoveLocation(u64(capacity_slot), MOVE_MEMORY, u8(13), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), capacity);
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), counter);
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(1), u8(0));
    u64 full = condition_field(cursor, u8(0x83));
    MoveLocation successor = MoveLocation(1u64, MOVE_MEMORY, u8(0), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(full + 4u64, u16(0x8d), u8(0), false, u8(2), successor);
    cursor = encode_operand_instruction(cursor, u16(0xb10f), u8(0xf0), false, u8(2), counter);
    u64 again = condition_field(cursor, u8(0x85));
    patch_relative(unsafe_address(again), again - start, begin - start);
    // EAX still holds the successful old count; R15 receives the target base.
    cursor = buffer_field_at(again + 4u64, target_slot, u16(0x8b), u8(15));
    return AppendBranch(cursor - start, full - start);
}

AppendBranch append_result(address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = move_immediate(start, u8(0), 1u64, false);
    u64 done = jump_field(cursor);
    u64 full = done + 4u64;
    cursor = register_instruction(full, u16(0x31), u8(0), false, u8(0), u8(0));
    patch_relative(unsafe_address(done), done - start, cursor - start);
    return AppendBranch(cursor - start, full - start);
}

u8 reduction_opcode(ReductionOperator operation, bool floating) on cpu {
    if (operation == ReductionOperator::Sum) { return floating ? u8(0x58) : u8(0x03); }
    if (operation == ReductionOperator::Min) { return u8(0x5d); }
    if (operation == ReductionOperator::Max) { return u8(0x5f); }
    if (operation == ReductionOperator::BitAnd) { return u8(0x23); }
    if (operation == ReductionOperator::BitOr) { return u8(0x0b); }
    return u8(0x33);
}

u64 collective_accumulate(u64 at, ReductionOperator operation, ScalarEncoding scalar, u8 register, MoveLocation operand) on cpu {
    u32 size = scalar.width / 8;
    bool wide = size == 8;
    bool floating = scalar.kind == ScalarKind::Float;
    u8 opcode = reduction_opcode(operation, floating);
    if (floating) {
        u16 instruction = (u16(opcode) << 8) | u16(0x0f);
        u8 prefix = wide ? u8(0xf2) : u8(0xf3);
        return encode_operand_instruction(at, instruction, prefix, false, register, operand);
    }
    u8 prefix = size == 2 ? u8(0x66) : u8(0);
    u16 byte_form = size == 1 ? u16(1) : u16(0);
    bool minimum = operation == ReductionOperator::Min;
    if (!minimum && operation != ReductionOperator::Max) {
        return encode_operand_instruction(at, u16(opcode) - byte_form, prefix, wide, register, operand);
    }
    bool signed = scalar.kind == ScalarKind::Signed;
    u8 condition = minimum ? (signed ? u8(0x4f) : u8(0x47)) : (signed ? u8(0x4c) : u8(0x42));
    u64 cursor = encode_operand_instruction(at, u16(0x3b) - byte_form, prefix, wide, register, operand);
    if (size != 1) {
        return encode_operand_instruction(cursor, (u16(condition) << 8) | u16(0x0f), prefix, wide, register, operand);
    }
    // There is no byte CMOV. Read the selected byte only when it wins.
    u64 skip = condition_field(cursor, (condition + u8(0x40)) ^ u8(1));
    cursor = encode_operand_instruction(skip + 4u64, u16(0x8a), u8(0), false, register, operand);
    patch_relative(unsafe_address(skip), skip, cursor);
    return cursor;
}

u64 tile_lane_index(u64 at, u32 base_offset) on cpu {
    u64 cursor = register_instruction(at, u16(0x89), u8(0), false, u8(3), u8(0));
    MoveLocation base = MoveLocation(u64(base_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    return encode_operand_instruction(cursor, u16(0x2b), u8(0), false, u8(0), base);
}

u64 tile_lane_pointer(u32 base_offset, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = tile_lane_index(start, base_offset);
    MoveLocation lane = MoveLocation(0u64, MOVE_MEMORY, u8(4), u8(0), u8(2));
    return encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(10), lane) - start;
}

u64 tile_phase_advance(u32 base_offset, u32 length_offset, u64 bytes_back, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = range_step_at(start, u8(3), u8(1));
    cursor = tile_lane_index(cursor, base_offset);
    MoveLocation length = MoveLocation(u64(length_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x3b), u8(0), false, u8(0), length);
    u64 back = condition_field(cursor, u8(0x82));
    patch_relative(unsafe_address(back), back, start - bytes_back);
    return back + 4u64 - start;
}

struct TilePrivateCopy {
    MoveLocation location;
    ScalarEncoding scalar;
    u32 storage_offset;
    u32 plane_stride;
    bool restore;
};

u64 tile_private_copy(TilePrivateCopy input, address output) on cpu {
    u64 start = address_bits(output);
    u32 size = input.scalar.width / 8;
    bool signed = input.scalar.kind == ScalarKind::Signed;
    MoveLocation low = MoveLocation(u64(input.storage_offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    MoveLocation high = MoveLocation(u64(input.storage_offset + input.plane_stride), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    MoveLocation accumulator = move_gpr(u8(0));
    if (input.restore) {
        u64 cursor = size == 8 ? load_split_words(start, low, high) : load_bits(start, u8(0), low, false);
        MoveEncoding copied = copy_scalar_move(input.location, accumulator, size, signed, unsafe_address(cursor));
        return cursor + copied.length - start;
    }
    MoveEncoding copied = copy_scalar_move(accumulator, input.location, size, signed, output);
    u64 cursor = start + copied.length;
    if (size == 8) { return store_split_words(cursor, low, high, u8(0)) - start; }
    // Narrow values use a complete 32-bit word in the private lane stream.
    return store_bits(cursor, low, u8(0), 4) - start;
}

extern "C" u64 abi_value(u32 slot, u64 count, AbiMemoryMode mode, address context,
    cfn<AbiValueComponent,address,u64> component, address output) on cpu requires nonzero(component) {
    if (mode == AbiMemoryMode::PackIntegerResult || mode == AbiMemoryMode::UnpackIntegerResult) {
        return abi_integer_result(count, mode == AbiMemoryMode::PackIntegerResult, context, component, output);
    }
    u64 start = address_bits(output);
    u64 cursor = start;
    u8 base = u8(4);
    u32 offset = slot;
    bool range = mode == AbiMemoryMode::RangeSummary;
    if (mode != AbiMemoryMode::StackLoad && mode != AbiMemoryMode::StackStore) {
        // The return address and four shadow slots precede the range's output pointer.
        u32 pointer_slot = slot + (range ? 40 : 0);
        base = range ? u8(15) : u8(1);
        MoveLocation pointer = MoveLocation(u64(pointer_slot), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, base, pointer, true);
        offset = 0;
    }
    bool writing = mode == AbiMemoryMode::StackStore || mode == AbiMemoryMode::IndirectResult
        || mode == AbiMemoryMode::FallibleResult || range;
    u64 index = 0u64;
    while (index < count) {
        AbiValueComponent value = component(context, index);
        MoveLocation field = MoveLocation(u64(offset + value.offset), MOVE_MEMORY, base, NO_INDEX, u8(0));
        u32 size = value.scalar.width / 8;
        if (writing) {
            cursor += write_scalar_move(field, value.location, size, unsafe_address(cursor));
        } else {
            cursor += read_scalar_move(value.location, field, size, value.scalar.kind == ScalarKind::Signed,
                value.scalar.kind == ScalarKind::Boolean, unsafe_address(cursor));
        }
        index += 1u64;
    }
    if (mode == AbiMemoryMode::IndirectResult) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(1), u8(0));
    }
    return cursor - start;
}

u64 abi_integer_result(u64 count, bool pack, address context,
    cfn<AbiValueComponent,address,u64> component, address output) on cpu requires nonzero(component) {
    u64 start = address_bits(output);
    u64 cursor = start;
    MoveLocation rax = move_gpr(u8(0));
    MoveLocation rcx = move_gpr(u8(1));
    bool multiple = count > 1u64;
    if (pack) {
        if (count == 0u64) {
            return move_immediate(cursor, u8(0), 0u64, false) - start;
        }
        u64 index = 0u64;
        while (index < count) {
            AbiValueComponent value = component(context, index);
            MoveEncoding moved = copy_scalar_move(rax, value.location, value.scalar.width / 8,
                value.scalar.kind == ScalarKind::Signed, unsafe_address(cursor));
            cursor = pack_integer_component(cursor + moved.length, value.field_scalar,
                value.offset, index != 0u64);
            if (index == 0u64 && multiple) {
                cursor = load_bits(cursor, u8(1), rax, true);
            }
            index += 1u64;
        }
        if (multiple) { cursor = load_bits(cursor, u8(0), rcx, true); }
    } else {
        if (multiple) { cursor = load_bits(cursor, u8(1), rax, true); }
        u64 index = 0u64;
        while (index < count) {
            AbiValueComponent value = component(context, index);
            if (multiple) { cursor = load_bits(cursor, u8(0), rcx, true); }
            cursor = unpack_integer_component(cursor, value.field_scalar, value.offset);
            MoveEncoding moved = copy_scalar_move(value.location, rax, value.scalar.width / 8,
                value.scalar.kind == ScalarKind::Signed, unsafe_address(cursor));
            cursor += moved.length;
            index += 1u64;
        }
    }
    return cursor - start;
}

extern "C" u64 abi_stack_pointer(u32 offset, u32 slot, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation object = MoveLocation(u64(offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    MoveLocation home = MoveLocation(u64(slot), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x8d), u8(0), true, u8(0), object);
    return store_bits(cursor, home, u8(0), 8) - start;
}

u64 texture_table_at(u64 cursor, u64 offset) on cpu {
    if (offset <= 0x7fffffffu64) {
        MoveLocation binding = MoveLocation(offset, MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
        return encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(15), binding);
    }
    return register_instruction(move_immediate(cursor, u8(15), offset, true),
        u16(0x01), u8(0), true, u8(12), u8(15));
}

u64 texture_descriptor_at(u64 at) on cpu {
    MoveLocation binding = MoveLocation(0u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    return load_bits(at, u8(15), binding, true);
}

u64 texture_level_at(u64 at, MoveLocation level) on cpu {
    // The CPU descriptor has its mip table pointer at byte 8. Each level is 24 bytes.
    u64 cursor = texture_descriptor_at(at);
    MoveLocation mips = MoveLocation(8u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(15), mips, true);
    MoveEncoding loaded = copy_scalar_move(move_gpr(u8(0)), level, 4, false, unsafe_address(cursor));
    cursor = register_instruction(cursor + loaded.length, u16(0x6b), u8(0), true, u8(0), u8(0));
    put_byte(cursor, u8(24));
    return register_instruction(cursor + 1u64, u16(0x01), u8(0), true, u8(0), u8(15));
}

u64 texture_axis_at(u64 at, u32 extent_offset, MoveLocation coordinate) on cpu {
    MoveLocation extent = MoveLocation(u64(extent_offset), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    u64 cursor = load_bits(at, u8(1), extent, false);
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(1));
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(2));
    MoveEncoding component = copy_scalar_move(move_gpr(u8(0)), coordinate, 8, false, unsafe_address(cursor));
    return register_instruction(cursor + component.length, u16(0x01), u8(0), true, u8(2), u8(0));
}

u64 texture_texel_at(u64 at, Span coordinates, ScalarValueHost values, u32 texel_bytes) on cpu
    requires nonzero(values.location) {
    u64 cursor = at;
    bool volume = coordinates.count == 3u64;
    MoveLocation outer = values.location(values.context, indexed_pair_word(coordinates, volume ? 2u64 : 1u64));
    MoveEncoding loaded = copy_scalar_move(move_gpr(u8(0)), outer, 8, false, unsafe_address(cursor));
    cursor += loaded.length;
    if (volume) {
        cursor = texture_axis_at(cursor, 4, values.location(values.context, indexed_pair_word(coordinates, 1u64)));
    }
    cursor = texture_axis_at(cursor, 0, values.location(values.context, indexed_pair_word(coordinates, 0u64)));
    cursor = address_scale_at(cursor, u8(0), texel_bytes);
    MoveLocation pixels = MoveLocation(16u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(15), pixels, true);
    return register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(15));
}

u64 color_op(SampleHost host, SampleOp op, SampleScalar scalar, u64 a, u64 b) on cpu
    requires nonzero(host.instruction) {
    return host.instruction(host.context, op, scalar, a, b, 0u64);
}

u64 color_word(SampleHost host, u32 bits) on cpu requires nonzero(host.instruction) {
    return color_op(host, SampleOp::Constant, SampleScalar::Word, u64(bits), 0u64);
}

u64 color_float(SampleHost host, f32 value) on cpu requires nonzero(host.instruction) {
    return color_op(host, SampleOp::Constant, SampleScalar::Float, u64(bitcast_u32(value)), 0u64);
}

u64 color_select(SampleHost host, u64 condition, u64 yes, u64 no, SampleScalar scalar) on cpu
    requires nonzero(host.instruction) {
    return host.instruction(host.context, SampleOp::Select, scalar, condition, yes, no);
}

u64 color_srgb(SampleHost host, u64 value) on cpu requires nonzero(host.instruction) {
    u64 threshold = color_float(host, 0.04045);
    u64 low = color_op(host, SampleOp::LessEqual, SampleScalar::Bool, value, threshold);
    u64 divisor = color_float(host, 12.92);
    u64 linear = color_op(host, SampleOp::Divide, SampleScalar::Float, value, divisor);
    u64 offset = color_float(host, 0.055);
    u64 scale = color_float(host, 1.055);
    u64 exponent = color_float(host, 2.4);
    u64 shifted = color_op(host, SampleOp::Add, SampleScalar::Float, value, offset);
    u64 unit = color_op(host, SampleOp::Divide, SampleScalar::Float, shifted, scale);
    u64 curve = color_op(host, SampleOp::Power, SampleScalar::Float, unit, exponent);
    return color_select(host, low, linear, curve, SampleScalar::Float);
}

u64 color_unsigned_float(SampleHost host, u64 value, u32 mantissa_bits, u32 mask) on cpu
    requires nonzero(host.instruction) {
    u64 shift = color_word(host, mantissa_bits);
    u64 exponent = color_op(host, SampleOp::ShiftRight, SampleScalar::Word, value, shift);
    u64 fraction = color_op(host, SampleOp::And, SampleScalar::Word, value, color_word(host, mask));
    shift = color_word(host, 23 - mantissa_bits);
    u64 payload = color_op(host, SampleOp::ShiftLeft, SampleScalar::Word, fraction, shift);
    u64 biased = color_op(host, SampleOp::Add, SampleScalar::Word, exponent, color_word(host, 112));
    shift = color_word(host, 23);
    biased = color_op(host, SampleOp::ShiftLeft, SampleScalar::Word, biased, shift);
    u64 normal = color_op(host, SampleOp::Or, SampleScalar::Word, biased, payload);
    u64 special = color_op(host, SampleOp::Equal, SampleScalar::Bool, exponent, color_word(host, 31));
    u64 exceptional = color_op(host, SampleOp::Or, SampleScalar::Word, color_word(host, 0x7f800000), payload);
    u64 bits = color_select(host, special, exceptional, normal, SampleScalar::Word);
    u64 converted = color_op(host, SampleOp::Bitcast, SampleScalar::Float, bits, 0u64);
    u64 integer = color_op(host, SampleOp::Cast, SampleScalar::Float, fraction, 0u64);
    f32 scale = bitcast_f32((127 - 14 - mantissa_bits) << 23);
    u64 subnormal = color_op(host, SampleOp::Multiply, SampleScalar::Float, integer, color_float(host, scale));
    u64 tiny = color_op(host, SampleOp::Equal, SampleScalar::Bool, exponent, color_word(host, 0));
    return color_select(host, tiny, subnormal, converted, SampleScalar::Float);
}

u64 color_binary16(SampleHost host, u64 raw) on cpu requires nonzero(host.instruction) {
    u64 fraction = color_op(host, SampleOp::And, SampleScalar::Word, raw, color_word(host, 0x7fff));
    u64 magnitude = color_unsigned_float(host, fraction, 10, 0x3ff);
    u64 bits = color_op(host, SampleOp::Bitcast, SampleScalar::Word, magnitude, 0u64);
    u64 sign = color_op(host, SampleOp::And, SampleScalar::Word, raw, color_word(host, 0x8000));
    sign = color_op(host, SampleOp::ShiftLeft, SampleScalar::Word, sign, color_word(host, 16));
    bits = color_op(host, SampleOp::Or, SampleScalar::Word, bits, sign);
    return color_op(host, SampleOp::Bitcast, SampleScalar::Float, bits, 0u64);
}

u64 color_result(address result, u32 channel) on cpu {
    return unsafe_position<u64>(unsafe_address(address_bits(result) + u64(channel) * 8u64));
}

void texture_color(TextureColor format, u64 raw, address result, SampleHost host) on cpu
    requires nonzero(host.instruction) {
    if (format == TextureColor::Srgb) {
        for (channel in 0..3) {
            u64 dst = color_result(result, channel);
            u64 value = color_srgb(host, dst);
            color_op(host, SampleOp::Into, SampleScalar::Float, dst, value);
        }
        return;
    }
    if (format == TextureColor::R8 || format == TextureColor::R16) {
        u64 red;
        if (format == TextureColor::R16) {
            red = color_binary16(host, raw);
        } else {
            u64 value = color_op(host, SampleOp::Cast, SampleScalar::Float, raw, 0u64);
            red = color_op(host, SampleOp::Divide, SampleScalar::Float, value, color_float(host, 255.0));
        }
        color_op(host, SampleOp::Into, SampleScalar::Float, color_result(result, 0), red);
        for (channel in 1..4) {
            u64 bits = channel == 3 ? 0x3f800000u64 : 0u64;
            color_op(host, SampleOp::Set, SampleScalar::Float, color_result(result, channel), bits);
        }
        return;
    }

    u32 channels = format == TextureColor::Rg16 ? 2 : (format == TextureColor::Rg11B10 ? 3 : 4);
    u32 offset = 0;
    for (channel in 0..4) {
        u64 dst = color_result(result, channel);
        if (channel < channels) {
            u32 width = 8;
            if (format == TextureColor::Rg16) { width = 16; }
            if (format == TextureColor::Rgb10A2) { width = channel == 3 ? 2 : 10; }
            if (format == TextureColor::Rg11B10) { width = channel == 2 ? 10 : 11; }
            u64 shift = color_word(host, offset);
            offset += width;
            u64 shifted = color_op(host, SampleOp::ShiftRight, SampleScalar::Word, raw, shift);
            u32 mask = (1 << width) - 1;
            u64 component = color_op(host, SampleOp::And, SampleScalar::Word, shifted, color_word(host, mask));
            u64 value;
            if (format == TextureColor::Rg11B10) {
                u32 mantissa = width - 5;
                value = color_unsigned_float(host, component, mantissa, (1 << mantissa) - 1);
            } else {
                value = color_op(host, SampleOp::Cast, SampleScalar::Float, component, 0u64);
                value = color_op(host, SampleOp::Divide, SampleScalar::Float, value, color_float(host, f32(mask)));
            }
            color_op(host, SampleOp::Into, SampleScalar::Float, dst, value);
        } else {
            u64 bits = channel == 3 ? 0x3f800000u64 : 0u64;
            color_op(host, SampleOp::Set, SampleScalar::Float, dst, bits);
        }
    }
}

SampleRegisters sample_values(u64 r, u64 g, u64 b, u64 a) on cpu {
    return SampleRegisters(array<u64,4>(r, g, b, a));
}

void sample_block(SampleHost host, SampleBlock block) on cpu requires nonzero(host.block) {
    host.block(host.context, SampleBlockAction::Select, block, SampleBlock::Empty, 0u64);
}

void sample_append(SampleHost host, SampleBlock source) on cpu requires nonzero(host.block) {
    host.block(host.context, SampleBlockAction::Append, source, SampleBlock::Empty, 0u64);
}

void sample_branch(SampleHost host, u64 test, SampleBlock yes, SampleBlock no) on cpu requires nonzero(host.block) {
    host.block(host.context, SampleBlockAction::Branch, yes, no, test);
}

u64 sample_equal(SampleHost host, u64 value, u32 tag) on cpu requires nonzero(host.instruction) {
    return color_op(host, SampleOp::Equal, SampleScalar::Bool, value, color_word(host, tag));
}

SampleRegisters sample_texel(SampleHost host, SampleRegisters coordinate, u32 dimensions, u64 level, u64 format) on cpu
    requires nonzero(host.instruction) requires nonzero(host.read_texture) requires nonzero(host.block) {
    u64 r = color_op(host, SampleOp::Register, SampleScalar::Float, 0u64, 0u64);
    u64 g = color_op(host, SampleOp::Register, SampleScalar::Float, 0u64, 0u64);
    u64 b = color_op(host, SampleOp::Register, SampleScalar::Float, 0u64, 0u64);
    u64 a = color_op(host, SampleOp::Register, SampleScalar::Float, 0u64, 0u64);
    SampleRegisters result = sample_values(r, g, b, a);
    u64 floating = sample_equal(host, format, 0);
    sample_block(host, SampleBlock::FloatArm);
    host.read_texture(host.context, SampleRead(SampleReadKind::Float, dimensions, level, coordinate, result));

    sample_block(host, SampleBlock::FormatTest);
    u64 single = sample_equal(host, format, 3);
    sample_block(host, SampleBlock::ByteArm);
    u64 raw = host.read_texture(host.context, SampleRead(SampleReadKind::Byte, dimensions, level, coordinate, result));
    texture_color(TextureColor::R8, raw, address_of(result), host);

    sample_block(host, SampleBlock::PackedArm);
    raw = host.read_texture(host.context, SampleRead(SampleReadKind::Word, dimensions, level, coordinate, result));
    sample_block(host, SampleBlock::PreviousArm);
    texture_color(TextureColor::Rgba8, raw, address_of(result), host);
    array<TextureColor,3> packed = array<TextureColor,3>(TextureColor::Rg16, TextureColor::Rgb10A2, TextureColor::Rg11B10);
    for (index in 0..3) {
        TextureColor kind = packed[index];
        sample_block(host, SampleBlock::FormatArm);
        texture_color(kind, raw, address_of(result), host);
        sample_block(host, SampleBlock::FormatChoice);
        u64 test = sample_equal(host, format, u32(kind));
        sample_branch(host, test, SampleBlock::FormatArm, SampleBlock::PreviousArm);
        sample_block(host, SampleBlock::PreviousArm);
        sample_append(host, SampleBlock::FormatChoice);
    }

    sample_block(host, SampleBlock::PackedArm);
    sample_append(host, SampleBlock::PreviousArm);
    u64 encoded = sample_equal(host, format, 2);
    sample_block(host, SampleBlock::SrgbArm);
    texture_color(TextureColor::Srgb, 0u64, address_of(result), host);
    sample_block(host, SampleBlock::PackedArm);
    sample_branch(host, encoded, SampleBlock::SrgbArm, SampleBlock::Empty);

    sample_block(host, SampleBlock::FormatArm);
    raw = host.read_texture(host.context, SampleRead(SampleReadKind::Half, dimensions, level, coordinate, result));
    texture_color(TextureColor::R16, raw, address_of(result), host);
    sample_block(host, SampleBlock::FormatChoice);
    u64 half = sample_equal(host, format, u32(TextureColor::R16));
    sample_branch(host, half, SampleBlock::FormatArm, SampleBlock::PackedArm);
    sample_block(host, SampleBlock::FormatTest);
    sample_branch(host, single, SampleBlock::ByteArm, SampleBlock::FormatChoice);
    sample_block(host, SampleBlock::Output);
    sample_branch(host, floating, SampleBlock::FloatArm, SampleBlock::FormatTest);
    return result;
}

struct SampleAxis { u64 first; u64 next; u64 weight; };
struct SampleConstants { u64 one; u64 zero_i; u64 zero_u; u64 zero; u64 half; };

u64 sample_min_index(SampleHost host, SampleScalar scalar, u64 a, u64 b) on cpu requires nonzero(host.instruction) {
    u64 test = color_op(host, SampleOp::Less, SampleScalar::Bool, a, b);
    return color_select(host, test, a, b, scalar);
}

SampleAxis sample_axis(SampleHost host, SampleConstants k, u64 coordinate, u64 dimension, u64 address_mode, u32 bit) on cpu
    requires nonzero(host.instruction) {
    // Integer texel addressing stays in 64 bits even when the normalized
    // coordinate or the converted dimension rounds at an edge.
    u64 size = color_op(host, SampleOp::Cast, SampleScalar::Index, dimension, 0u64);
    u64 last = color_op(host, SampleOp::Subtract, SampleScalar::Index, size, k.one);
    u64 size_float = color_op(host, SampleOp::Cast, SampleScalar::Float, dimension, 0u64);
    u64 scaled = color_op(host, SampleOp::Multiply, SampleScalar::Float, coordinate, size_float);
    u64 centered = color_op(host, SampleOp::Subtract, SampleScalar::Float, scaled, k.half);
    u64 positive = color_op(host, SampleOp::Maximum, SampleScalar::Float, centered, k.zero);
    u64 at = color_op(host, SampleOp::Minimum, SampleScalar::Float, positive, size_float);
    u64 raw = color_op(host, SampleOp::Cast, SampleScalar::Index, at, 0u64);
    u64 clamped = sample_min_index(host, SampleScalar::Index, raw, last);
    u64 base = color_op(host, SampleOp::Cast, SampleScalar::Float, raw, 0u64);
    u64 weight_clamped = color_op(host, SampleOp::Subtract, SampleScalar::Float, at, base);
    u64 adjacent = color_op(host, SampleOp::Add, SampleScalar::Index, raw, k.one);
    u64 clamped_next = sample_min_index(host, SampleScalar::Index, adjacent, last);

    u64 floor = color_op(host, SampleOp::Floor, SampleScalar::Float, coordinate, coordinate);
    u64 unit = color_op(host, SampleOp::Subtract, SampleScalar::Float, coordinate, floor);
    scaled = color_op(host, SampleOp::Multiply, SampleScalar::Float, unit, size_float);
    centered = color_op(host, SampleOp::Subtract, SampleScalar::Float, scaled, k.half);
    base = color_op(host, SampleOp::Floor, SampleScalar::Float, centered, centered);
    u64 index = color_op(host, SampleOp::Cast, SampleScalar::Index, base, 0u64);
    u64 negative = color_op(host, SampleOp::Less, SampleScalar::Bool, index, k.zero_i);
    u64 at_limit = color_op(host, SampleOp::GreaterEqual, SampleScalar::Bool, index, size);
    u64 reduced = color_op(host, SampleOp::Subtract, SampleScalar::Index, index, size);
    u64 wrapped = color_select(host, at_limit, reduced, index, SampleScalar::Index);
    wrapped = color_select(host, negative, last, wrapped, SampleScalar::Index);
    u64 next_index = color_op(host, SampleOp::Add, SampleScalar::Index, wrapped, k.one);
    u64 at_end = color_op(host, SampleOp::Equal, SampleScalar::Bool, wrapped, last);
    u64 wrapped_next = color_select(host, at_end, k.zero_i, next_index, SampleScalar::Index);
    u64 weight = color_op(host, SampleOp::Subtract, SampleScalar::Float, centered, base);
    u64 flag = color_op(host, SampleOp::And, SampleScalar::Word, address_mode, color_word(host, bit));
    u64 repeat = color_op(host, SampleOp::NotEqual, SampleScalar::Bool, flag, k.zero_u);
    u64 first = color_select(host, repeat, wrapped, clamped, SampleScalar::Index);
    u64 next = color_select(host, repeat, wrapped_next, clamped_next, SampleScalar::Index);
    u64 amount = color_select(host, repeat, weight, weight_clamped, SampleScalar::Float);
    return SampleAxis(first, next, amount);
}

u64 sample_lerp(SampleHost host, u64 a, u64 b, u64 weight) on cpu requires nonzero(host.instruction) {
    u64 difference = color_op(host, SampleOp::Subtract, SampleScalar::Float, b, a);
    u64 scaled = color_op(host, SampleOp::Multiply, SampleScalar::Float, difference, weight);
    return color_op(host, SampleOp::Add, SampleScalar::Float, a, scaled);
}

SampleRegisters sample_blend(SampleHost host, read SampleRegisters a, read SampleRegisters b, u64 weight) on cpu
    requires nonzero(host.instruction) {
    SampleRegisters result = sample_values(0u64, 0u64, 0u64, 0u64);
    for (channel in 0..4) {
        result.values[channel] = sample_lerp(host, a.values[channel], b.values[channel], weight);
    }
    return result;
}

SampleRegisters sample_level(SampleHost host, read SampleRegisters coordinate, u32 dimensions, u64 level, u64 address_mode, u64 format) on cpu
    requires nonzero(host.instruction) requires nonzero(host.read_texture) requires nonzero(host.block) {
    bool volume = dimensions == 3;
    u64 width = color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64);
    u64 height = color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64);
    u64 depth = volume ? color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64) : 0u64;
    SampleRegisters sizes = sample_values(width, height, depth, 0u64);
    host.read_texture(host.context, SampleRead(SampleReadKind::Size, dimensions, level, coordinate, sizes));
    u64 one = color_op(host, SampleOp::Constant, SampleScalar::Index, 1u64, 0u64);
    u64 zero_i = color_op(host, SampleOp::Constant, SampleScalar::Index, 0u64, 0u64);
    u64 zero_u = color_word(host, 0);
    u64 zero = color_float(host, 0.0);
    u64 half = color_float(host, 0.5);
    SampleConstants constants = SampleConstants(one, zero_i, zero_u, zero, half);
    SampleAxis x = sample_axis(host, constants, coordinate.values[0], width, address_mode, 1);
    SampleAxis y = sample_axis(host, constants, coordinate.values[1], height, address_mode, 2);
    SampleAxis z = SampleAxis(0u64, 0u64, 0u64);
    if (volume) {
        z = sample_axis(host, constants, coordinate.values[2], depth, address_mode, 4);
    }

    SampleRegisters a = sample_texel(host, sample_values(x.first, y.first, z.first, 0u64), dimensions, level, format);
    SampleRegisters b = sample_texel(host, sample_values(x.next, y.first, z.first, 0u64), dimensions, level, format);
    SampleRegisters c = sample_texel(host, sample_values(x.first, y.next, z.first, 0u64), dimensions, level, format);
    SampleRegisters d = sample_texel(host, sample_values(x.next, y.next, z.first, 0u64), dimensions, level, format);
    if (volume) {
        SampleRegisters e = sample_texel(host, sample_values(x.first, y.first, z.next, 0u64), dimensions, level, format);
        SampleRegisters f = sample_texel(host, sample_values(x.next, y.first, z.next, 0u64), dimensions, level, format);
        SampleRegisters g = sample_texel(host, sample_values(x.first, y.next, z.next, 0u64), dimensions, level, format);
        SampleRegisters h = sample_texel(host, sample_values(x.next, y.next, z.next, 0u64), dimensions, level, format);
        SampleRegisters ab = sample_blend(host, a, b, x.weight);
        SampleRegisters cd = sample_blend(host, c, d, x.weight);
        SampleRegisters ef = sample_blend(host, e, f, x.weight);
        SampleRegisters gh = sample_blend(host, g, h, x.weight);
        SampleRegisters lower = sample_blend(host, ab, cd, y.weight);
        SampleRegisters upper = sample_blend(host, ef, gh, y.weight);
        return sample_blend(host, lower, upper, z.weight);
    }
    SampleRegisters ab = sample_blend(host, a, b, x.weight);
    SampleRegisters cd = sample_blend(host, c, d, x.weight);
    return sample_blend(host, ab, cd, y.weight);
}

extern "C" void texture_sample_ir(address coordinate, u32 dimensions, u64 lod, address result, SampleHost host) on cpu
    requires nonzero(host.instruction) requires nonzero(host.read_texture) requires nonzero(host.block) {
    u64 x = color_result(coordinate, 0);
    u64 y = color_result(coordinate, 1);
    u64 z = dimensions == 3 ? color_result(coordinate, 2) : 0u64;
    SampleRegisters point = sample_values(x, y, z, 0u64);

    u64 zero = color_float(host, 0.0);
    u64 one = color_word(host, 1);
    u64 levels = color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64);
    u64 address_mode = color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64);
    u64 format = color_op(host, SampleOp::Register, SampleScalar::Word, 0u64, 0u64);
    color_op(host, SampleOp::Format, SampleScalar::Word, format, 0u64);
    color_op(host, SampleOp::AddressMode, SampleScalar::Word, address_mode, 0u64);
    color_op(host, SampleOp::Levels, SampleScalar::Word, levels, 0u64);

    u64 last = color_op(host, SampleOp::Subtract, SampleScalar::Word, levels, one);
    u64 last_float = color_op(host, SampleOp::Cast, SampleScalar::Float, last, 0u64);
    u64 positive = color_op(host, SampleOp::Maximum, SampleScalar::Float, lod, zero);
    u64 clamped = color_op(host, SampleOp::Minimum, SampleScalar::Float, positive, last_float);
    u64 first = color_op(host, SampleOp::Cast, SampleScalar::Word, clamped, 0u64);
    u64 first_float = color_op(host, SampleOp::Cast, SampleScalar::Float, first, 0u64);
    u64 fraction = color_op(host, SampleOp::Subtract, SampleScalar::Float, clamped, first_float);
    u64 adjacent = color_op(host, SampleOp::Add, SampleScalar::Word, first, one);
    u64 next = sample_min_index(host, SampleScalar::Word, adjacent, last);
    SampleRegisters a = sample_level(host, point, dimensions, first, address_mode, format);
    SampleRegisters b = sample_level(host, point, dimensions, next, address_mode, format);
    for (channel in 0..4) {
        u64 value = sample_lerp(host, a.values[channel], b.values[channel], fraction);
        color_op(host, SampleOp::Into, SampleScalar::Float, color_result(result, channel), value);
    }
}

extern "C" u64 emit_texture_access(View access, TextureAccessHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.offset) {
    ScalarValueHost values = host.values;
    address context = values.context;
    View texture = values.ir.inspect(access.left);
    u64 start = address_bits(output);
    u64 cursor = start;
    if (texture.first.count == 0u64) {
        cursor = texture_table_at(cursor, host.offset(host.table, texture.a));
    } else {
        usize parameter = indexed_pair_word(texture.first, 0u64);
        MoveEncoding binding = copy_scalar_move(move_gpr(u8(15)), values.location(context, parameter), 8, false, output);
        cursor += binding.length;
    }
    if (texture.second.count != 0u64) {
        usize index = indexed_pair_word(texture.second, 0u64);
        ScalarEncoding scalar = values.scalar(context, index);
        MoveEncoding moved = copy_scalar_move(move_gpr(u8(0)), values.location(context, index),
            scalar.width / 8, scalar.kind == ScalarKind::Signed, unsafe_address(cursor));
        cursor += moved.length;
        cursor += offset_address(host.stride, unsafe_address(cursor));
    }
    u32 tag = access.tag;
    if (tag == OP_TEXTURE_BINDING_ADDRESS) {
        MoveEncoding binding = copy_scalar_move(values.location(context, access.a), move_gpr(u8(15)), 8, false,
            unsafe_address(cursor));
        return cursor + binding.length - start;
    }
    bool raw = tag == OP_TEXTURE_READ;
    bool fetch = tag == OP_TEXTURE_FETCH;
    bool size = tag == OP_TEXTURE_SIZE;
    if (raw || fetch) {
        usize level = raw ? access.b : access.a;
        Span coordinates = raw ? access.first : access.second;
        u32 bytes = raw ? u32(access.c) : 16;
        cursor = texture_level_at(cursor, values.location(context, level));
        cursor = texture_texel_at(cursor, coordinates, values, bytes);
        if (bytes < 4) {
            MoveLocation pixel = MoveLocation(0u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
            cursor = load_extended(cursor, u8(0), pixel, bytes, false);
            MoveEncoding value = copy_scalar_move(values.location(context, access.a), move_gpr(u8(0)), 4, false,
                unsafe_address(cursor));
            return cursor + value.length - start;
        }
    } else if (size) {
        cursor = texture_level_at(cursor, values.location(context, access.a));
    } else {
        cursor = texture_descriptor_at(cursor);
    }
    if (size || fetch) {
        u64 i = 0u64;
        while (i < u64(access.first.count)) {
            MoveLocation dst = values.location(context, indexed_pair_word(access.first, i));
            MoveLocation field = MoveLocation(i * 4u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
            cursor += read_scalar_move(dst, field, 4, false, false, unsafe_address(cursor));
            i += 1u64;
        }
    } else {
        u64 offset = tag == OP_TEXTURE_ADDRESS ? 4u64 : (tag == OP_TEXTURE_FORMAT ? 16u64 : 0u64);
        MoveLocation field = MoveLocation(offset, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
        cursor += read_scalar_move(values.location(context, access.a), field, 4, false, false, unsafe_address(cursor));
    }
    return cursor - start;
}

extern "C" u64 tile_index_base(MoveLocation index, ScalarEncoding scalar, u32 storage_offset, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_index_at(start, index, scalar);
    MoveLocation lane = MoveLocation(u64(storage_offset), MOVE_MEMORY, u8(4), u8(0), u8(2));
    return encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(15), lane) - start;
}

extern "C" u64 tile_field_address(u32 width, u32 byte_offset, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (byte_offset != 0) {
        put_byte(cursor, u8(0x05));
        write_word(cursor + 1u64, byte_offset);
        cursor += 5u64;
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(0), u8(1));
    cursor = register_instruction(cursor, u16(0x83), u8(0), false, u8(4), u8(1));
    put_byte(cursor, u8(3));
    cursor = register_instruction(cursor + 1u64, u16(0x83), u8(0), false, u8(4), u8(0));
    put_byte(cursor, u8(0xfc));
    // Keep the byte within its 32-bit word; scale only the word-plane displacement.
    cursor = address_scale_at(cursor + 1u64, u8(0), width);
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(1), u8(0));
    return register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(15)) - start;
}

extern "C" u64 tile_read_split(MoveLocation dst, MoveLocation low, MoveLocation high, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_split_words(start, low, high);
    MoveEncoding copied = copy_scalar_move(dst, move_gpr(u8(0)), 8, false, unsafe_address(cursor));
    return cursor + copied.length - start;
}

TilePublishCode tile_publish_value(TilePublishInput input, address output) on cpu {
    u64 start = address_bits(output);
    u32 size = input.scalar.width / 8;
    MoveEncoding loaded = copy_scalar_move(move_gpr(u8(0)), input.location, size,
        input.scalar.kind == ScalarKind::Signed, output);
    u64 value_end = start + loaded.length;
    u64 cursor = value_end;
    if (input.low_offset != 0xffffffff) {
        MoveLocation low = MoveLocation(u64(input.low_offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(cursor, low, u8(0), size == 8 ? 4 : size);
    }
    u64 high_start = cursor;
    u64 high_store = cursor;
    if (input.high_offset != 0xffffffff) {
        high_store = shift_high_word(cursor, u8(0));
        MoveLocation high = MoveLocation(u64(input.high_offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(high_store, high, u8(0), 4);
    }
    return TilePublishCode(cursor - start, value_end - start, high_start - start, high_store - start);
}

extern "C" void emit_tile_publish(address context, u32 base_offset, TilePublishHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.next) requires nonzero(host.observe) {
    TilePublishField field = host.next(context);
    if (address_bits(field.scalar_origin) == 0u64) { return; }

    NativeCodeBuffer buffer = host.reserve(context, 64u64);
    u64 start = buffer.end;
    address output = unsafe_address(address_bits(buffer.image) + start);
    u64 value_start = start + tile_lane_pointer(base_offset, output);

    while (true) {
        output = unsafe_address(address_bits(buffer.image) + value_start);
        TilePublishCode encoding = tile_publish_value(field.input, output);
        host.commit(context, value_start + encoding.length);
        host.observe(context, start, value_start, encoding, field);
        field = host.next(context);
        if (address_bits(field.scalar_origin) == 0u64) { return; }

        buffer = host.reserve(context, 48u64);
        start = buffer.end;
        value_start = start;
    }
}

u64 tile_collective_store(u64 at, ScalarEncoding scalar, u32 plane_stride) on cpu {
    MoveLocation low = MoveLocation(0u64, MOVE_MEMORY, u8(11), NO_INDEX, u8(0));
    bool wide = scalar.width == 64;
    bool floating = scalar.kind == ScalarKind::Float;
    u64 cursor = at;
    if (floating && !wide) {
        cursor = store_float(cursor, low, u8(0), false);
    } else {
        if (floating) {
            cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), true, u8(0), u8(0));
        }
        // Every lane occupies one 32-bit word in each result plane.
        cursor = store_bits(cursor, low, u8(0), 4);
        if (wide) {
            cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(2));
            cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(5), u8(2));
            put_byte(cursor, u8(32));
            MoveLocation high = MoveLocation(u64(plane_stride), MOVE_MEMORY, u8(11), NO_INDEX, u8(0));
            cursor = store_bits(cursor + 1u64, high, u8(2), 4);
        }
    }
    return cursor;
}

u64 tile_stream_advance(u64 at, u8 pointer) on cpu {
    u64 immediate = register_instruction(at, u16(0x83), u8(0), true, u8(0), pointer);
    put_byte(immediate, u8(4));
    return immediate + 1u64;
}

u64 tile_counted_back_edge(u64 at, u64 begin) on cpu {
    u64 cursor = register_instruction(at, u16(0xff), u8(0), false, u8(1), u8(1));
    u64 back = condition_field(cursor, u8(0x85));
    patch_relative(unsafe_address(back), back, begin);
    return back + 4u64;
}

u64 tile_lane_remainder(u64 at, u32 width) on cpu {
    u64 cursor = register_instruction(at, u16(0x89), u8(0), false, u8(3), u8(0));
    cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(2), u8(2));
    cursor = move_immediate(cursor, u8(1), u64(width), false);
    return register_instruction(cursor, u16(0xf7), u8(0), false, u8(6), u8(1));
}

extern "C" u64 tile_lane(u32 width, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = tile_lane_remainder(start, width);
    return register_instruction(cursor, u16(0x89), u8(0), false, u8(2), u8(0)) - start;
}

struct TileInputBoundaryCode {
    u64 length;
    u64 skip_field;
};

struct TileInputCopy {
    u32 data_offset;
    u32 stride_offset;
    u32 source_offset;
    u32 storage_offset;
    u32 length_offset;
    u32 unit_bytes;
};

TileInputBoundaryCode tile_input_boundary(u32 width, u32 length_offset, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = tile_lane_remainder(start, width);
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(2), u8(2));
    u64 skip = condition_field(cursor, u8(0x85));
    write_word(skip, 0);
    // R14D is the aligned range's exclusive end. Its last tile can be partial.
    cursor = register_instruction(skip + 4u64, u16(0x89), u8(0), false, u8(14), u8(0));
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(3), u8(0));
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(1), u8(0));
    cursor = register_instruction(cursor, u16(0x470f), u8(0), false, u8(0), u8(1));
    MoveLocation length = MoveLocation(u64(length_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = store_bits(cursor, length, u8(0), 4);
    return TileInputBoundaryCode(cursor - start, skip - start);
}

u64 tile_input_copy(TileInputCopy input, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = buffer_field_at(start, input.data_offset, u16(0x8b), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(3), u8(0));
    cursor = buffer_index_at(cursor, input.stride_offset, u8(0), u8(10));
    MoveLocation storage = MoveLocation(u64(input.storage_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(11), storage);
    MoveLocation length = MoveLocation(u64(input.length_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    u64 begin = load_bits(cursor, u8(1), length, false);
    MoveLocation source = MoveLocation(u64(input.source_offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    if (input.unit_bytes == 4) {
        cursor = load_bits(begin, u8(0), source, false);
    } else {
        cursor = load_extended(begin, u8(0), source, input.unit_bytes, false);
    }
    MoveLocation target = MoveLocation(0u64, MOVE_MEMORY, u8(11), NO_INDEX, u8(0));
    cursor = store_bits(cursor, target, u8(0), input.unit_bytes);
    cursor = buffer_field_at(cursor, input.stride_offset, u16(0x03), u8(10));
    cursor = tile_stream_advance(cursor, u8(11));
    return tile_counted_back_edge(cursor, begin) - start;
}

TileCollectiveCode tile_collective(TileCollectiveLayout layout, TileCollectiveField field, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation source = MoveLocation(u64(field.input_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    MoveLocation target = MoveLocation(u64(field.output_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    MoveLocation length = MoveLocation(u64(layout.length_offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x8d), u8(0), true, u8(10), source);
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(11), target);
    u64 identity = load_bits(cursor, u8(1), length, false);

    bool wide = field.scalar.width == 64;
    bool floating = field.scalar.kind == ScalarKind::Float;
    if (field.identity == 0u64) {
        u16 opcode = floating ? u16(0x570f) : u16(0x31);
        cursor = register_instruction(identity, opcode, u8(0), false, u8(0), u8(0));
    } else {
        cursor = move_immediate(identity, u8(0), field.identity, wide);
        if (floating) {
            cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, u8(0), u8(0));
        }
    }

    u64 begin = cursor;
    if (layout.exclusive) {
        cursor = tile_collective_store(cursor, field.scalar, layout.plane_stride);
        cursor = tile_stream_advance(cursor, u8(11));
    }
    u64 accumulate = cursor;
    MoveLocation operand = MoveLocation(0u64, MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    if (wide) {
        MoveLocation high = MoveLocation(u64(field.input_high_delta), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), operand, false);
        cursor = load_bits(cursor, u8(8), high, false);
        cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(4), u8(8));
        put_byte(cursor, u8(32));
        cursor = register_instruction(cursor + 1u64, u16(0x09), u8(0), true, u8(8), u8(2));
        operand = move_gpr(u8(2));
        if (floating) {
            cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), true, u8(1), u8(2));
            operand = packet_register(u8(1));
        }
    }
    cursor = collective_accumulate(cursor, layout.operation, field.scalar, u8(0), operand);
    cursor = tile_stream_advance(cursor, u8(10));
    u64 fill = tile_counted_back_edge(cursor, begin);
    u64 fill_loop = fill;
    cursor = fill;
    if (!layout.exclusive) {
        fill_loop = load_bits(cursor, u8(1), length, false);
        cursor = tile_collective_store(fill_loop, field.scalar, layout.plane_stride);
        cursor = tile_stream_advance(cursor, u8(11));
        cursor = tile_counted_back_edge(cursor, fill_loop);
    }
    return TileCollectiveCode(cursor - start, identity - start, begin - start,
        accumulate - start, fill - start, fill_loop - start);
}

extern "C" void emit_tile_collective(address context, TileCollectiveLayout layout, TileCollectiveHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.next) requires nonzero(host.observe) {
    TileCollectiveField field = host.next(context, layout.operation);
    while (address_bits(field.scalar_origin) != 0u64) {
        NativeCodeBuffer buffer = host.reserve(context, 128u64);
        address output = unsafe_address(address_bits(buffer.image) + buffer.end);
        TileCollectiveCode encoding = tile_collective(layout, field, output);
        host.commit(context, buffer.end + encoding.length);
        host.observe(context, buffer.end, layout, encoding, field);
        field = host.next(context, layout.operation);
    }
}

extern "C" CollectiveLeaf reduce_range(ReductionRange input, CollectiveLeafOutput output) on cpu {
    u32 saved = SAVE_R12;
    LeafRange range = leaf_begin(output.range, saved);
    u64 image = address_bits(output.range.image);
    u64 start = image + range.end;
    MoveLocation stride = MoveLocation(u64(input.source + input.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x8b), u8(0), true, u8(12), stride);
    // Keep source address in R10, stride in R12, and range index in ECX.
    MoveLocation source = MoveLocation(u64(input.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(10), source);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(1));

    let integer_registers = array<u8,4>(u8(0), u8(2), u8(9), u8(11));
    u64 reference_count = 0u64;
    u32 size = input.scalar.width / 8;
    bool floating = input.scalar.kind == ScalarKind::Float;
    for (component in 0..4) {
        if (component == input.count) { break; }
        if (floating) {
            MoveLocation dst = MoveLocation(0u64, MOVE_XMM, u8(component), NO_INDEX, u8(0));
            MoveLocation src = MoveLocation(input.identity, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
            MoveEncoding value = copy_scalar_move(dst, src, size, false, unsafe_address(cursor));
            if (value.literal_field != 0u64) {
                address field = unsafe_address(address_bits(output.references) + reference_count * 8u64);
                unsafe_position<u64>(field) = cursor - image + value.literal_field;
                reference_count += 1u64;
            }
            cursor += value.length;
        } else {
            cursor = move_immediate(cursor, integer_registers[component], input.identity, size == 8);
        }
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(8), u8(1));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = end + 4u64;
    for (component in 0..4) {
        if (component == input.count) { break; }
        u8 register = floating ? u8(component) : integer_registers[component];
        MoveLocation operand = MoveLocation(u64(input.source_word + input.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = collective_accumulate(cursor, input.operation, input.scalar, register, operand);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);

    // Return address, four shadow slots and saved R12 precede the result pointer.
    MoveLocation result = MoveLocation(0x28u64 + u64(range.stack_bytes), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(10), result);
    for (component in 0..4) {
        if (component == input.count) { break; }
        MoveLocation destination = MoveLocation(u64(input.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        if (floating) {
            cursor = store_float(cursor, destination, u8(component), size == 8);
        } else {
            cursor = store_bits(cursor, destination, integer_registers[component], size);
        }
    }
    return collective_leaf(range, leaf_return(cursor, saved), output, input.identity, reference_count);
}

u64 segmented_identity(u64 at, ReductionRange values) on cpu {
    bool wide = values.scalar.width == 64;
    u64 cursor = move_immediate(at, u8(0), values.identity, wide);
    for (component in 0..4) {
        if (component == values.count) { break; }
        cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, u8(component), u8(0));
    }
    return cursor;
}

u64 segmented_store(u64 at, ReductionRange values, u8 base, u32 field) on cpu {
    u64 cursor = at;
    u32 size = values.scalar.width / 8;
    for (component in 0..4) {
        if (component == values.count) { break; }
        MoveLocation destination = MoveLocation(u64(field + values.offsets[component]), MOVE_MEMORY, base, NO_INDEX, u8(0));
        MoveLocation source = MoveLocation(0u64, MOVE_XMM, u8(component), NO_INDEX, u8(0));
        cursor += write_scalar_move(destination, source, size, unsafe_address(cursor));
    }
    return cursor;
}

u64 segmented_accumulate(u64 at, ReductionRange values) on cpu {
    u64 cursor = at;
    bool wide = values.scalar.width == 64;
    bool floating = values.scalar.kind == ScalarKind::Float;
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        u8 register = lane;
        if (!floating) {
            cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), wide, lane, u8(0));
            register = u8(0);
        }
        MoveLocation operand = MoveLocation(u64(values.source_word + values.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = collective_accumulate(cursor, values.operation, values.scalar, register, operand);
        if (!floating) {
            cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, lane, u8(0));
        }
    }
    return cursor;
}

extern "C" LeafRange segmented_reduce_range(SegmentedRange input, u32 finish, LeafOutput output) on cpu {
    u32 saved = SAVE_R12_R13;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    ReductionRange values = input.values;
    bool wide = values.scalar.width == 64;
    u32 summary_flag = input.value_words * 4;
    u32 summary_count = summary_flag + 4;
    MoveLocation source_stride = MoveLocation(u64(values.source + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), source_stride, true);
    MoveLocation source = MoveLocation(u64(values.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), source, true);
    if (finish != 0) {
        MoveLocation target_stride = MoveLocation(u64(input.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(13), target_stride, true);
        MoveLocation target = MoveLocation(u64(input.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(11), target, true);
    }
    // R10 walks source records, R9D holds the logical index, R8D the remaining count.
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    if (finish != 0) {
        // The existing prefix row holds the incoming value, total length and earlier segment count.
        MoveLocation parameters = MoveLocation(u64(input.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
        MoveLocation earlier = MoveLocation(u64(summary_count), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(1), earlier, false);
        MoveLocation extent = MoveLocation(u64(summary_flag), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, u8(4), extent);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation prefix = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            cursor = load_float(cursor, u8(component), prefix, wide);
        }
    } else {
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(1), u8(1));
        cursor = register_instruction(cursor, u16(0xef0f), u8(0x66), false, u8(5), u8(5));
        cursor = segmented_identity(cursor, values);
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    cursor = register_instruction(end + 4u64, u16(0x85), u8(0), false, u8(9), u8(9));
    u64 first = condition_field(cursor, u8(0x84));
    MoveLocation head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(first + 4u64, u16(0x83), u8(0), false, u8(7), head);
    put_byte(cursor, u8(0));
    u64 no_head = condition_field(cursor + 1u64, u8(0x84));
    cursor = no_head + 4u64;
    patch_relative(unsafe_address(first), first, cursor);
    cursor = segmented_identity(cursor, values);
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    if (finish == 0) {
        cursor = register_instruction(cursor, u16(0x760f), u8(0x66), false, u8(5), u8(5));
    }
    patch_relative(unsafe_address(no_head), no_head, cursor);
    cursor = segmented_accumulate(cursor, values);
    if (finish != 0) {
        // A segment ends at the logical last record or before the next record's head.
        MoveLocation next_index = MoveLocation(1u64, MOVE_MEMORY, u8(9), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), false, u8(0), next_index);
        cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), false, u8(4), u8(2));
        cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(2), u8(0));
        u64 last = condition_field(cursor, u8(0x84));
        MoveLocation next_head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), u8(12), u8(0));
        cursor = encode_operand_instruction(last + 4u64, u16(0x83), u8(0), false, u8(7), next_head);
        put_byte(cursor, u8(0));
        u64 not_tail = condition_field(cursor + 1u64, u8(0x84));
        cursor = not_tail + 4u64;
        patch_relative(unsafe_address(last), last, cursor);
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(1), u8(0));
        cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
        MoveLocation destination = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(0), u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(2), destination);
        cursor = segmented_store(cursor, values, u8(2), input.target_word);
        patch_relative(unsafe_address(not_tail), not_tail, cursor);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(9));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (finish == 0) {
        // The fifth RangeEntry argument follows the return address, shadow space and saved registers.
        MoveLocation result = MoveLocation(0x28u64 + u64(range.stack_bytes), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        cursor = segmented_store(cursor, values, u8(10), 0);
        MoveLocation reset = MoveLocation(u64(summary_flag), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_float(cursor, reset, u8(5), false);
        MoveLocation segments = MoveLocation(u64(summary_count), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(cursor, segments, u8(1), 4);
    }
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

u64 scan_float_correction(u64 at, ReductionOperator operation, ScalarEncoding scalar, u8 lane, MoveLocation destination) on cpu {
    u64 cursor = register_instruction(at, u16(0x280f), u8(0), false, u8(4), lane);
    if (operation == ReductionOperator::Sum) {
        cursor = collective_accumulate(cursor, operation, scalar, u8(4), destination);
    } else {
        // The first local element has no previous value in this range.
        cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(9), u8(8));
        u64 first = condition_field(cursor, u8(0x84));
        cursor = collective_accumulate(first + 4u64, operation, scalar, u8(4), destination);
        patch_relative(unsafe_address(first), first, cursor);
    }
    return store_float(cursor, destination, u8(4), scalar.width == 64);
}

extern "C" CollectiveLeaf scan_range(ScanRange input, u32 add, CollectiveLeafOutput output) on cpu {
    u32 saved = add != 0 ? SAVE_R12 : SAVE_R12_R13;
    LeafRange range = leaf_begin(output.range, saved);
    u64 image = address_bits(output.range.image);
    u64 start = image + range.end;
    ReductionRange values = input.values;
    bool correction = add != 0;
    u32 size = values.scalar.width / 8;
    bool wide = size == 8;
    bool floating = values.scalar.kind == ScalarKind::Float;
    bool extrema = values.operation == ReductionOperator::Min || values.operation == ReductionOperator::Max;
    u32 binding = correction ? input.target : values.source;
    MoveLocation stride = MoveLocation(u64(binding + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), stride, true);
    MoveLocation current = MoveLocation(u64(binding), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), current, true);
    if (correction) {
        MoveLocation parameters = MoveLocation(u64(input.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
    } else {
        MoveLocation output_stride = MoveLocation(u64(input.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(13), output_stride, true);
        MoveLocation target = MoveLocation(u64(input.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(11), target, true);
    }
    // R8D holds the remaining count. R10 walks the current binding.
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    if (!correction) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
        cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(11));
    }
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    array<u8,4> registers = array<u8,4>(u8(0), u8(1), u8(2), u8(9));
    if (correction) { registers = array<u8,4>(u8(0), u8(1), u8(9), u8(11)); }
    u64 reference_count = 0u64;
    u8 prefix = size == 2 ? u8(0x66) : u8(0);
    u16 byte_form = size == 1 ? u16(1) : u16(0);
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        if (correction) {
            MoveLocation incoming = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            if (floating) {
                cursor = load_float(cursor, lane, incoming, wide);
            } else {
                cursor = encode_operand_instruction(cursor, u16(0x8b) - byte_form, prefix, wide, registers[component], incoming);
            }
        } else if (!floating) {
            cursor = move_immediate(cursor, registers[component], values.identity, wide);
        } else if (values.operation == ReductionOperator::Sum) {
            cursor = register_instruction(cursor, u16(0x570f), u8(0), false, lane, lane);
        } else {
            MoveLocation dst = MoveLocation(0u64, MOVE_XMM, lane, NO_INDEX, u8(0));
            MoveLocation src = MoveLocation(values.identity, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
            MoveEncoding identity = copy_scalar_move(dst, src, size, false, unsafe_address(cursor));
            if (identity.literal_field != 0u64) {
                address field = unsafe_address(address_bits(output.references) + reference_count * 8u64);
                unsafe_position<u64>(field) = cursor - image + identity.literal_field;
                reference_count += 1u64;
            }
            cursor += identity.length;
        }
    }
    if (correction && floating && extrema) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(9));
    }
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    cursor = end + 4u64;
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        u8 register = registers[component];
        u32 offset = values.offsets[component];
        u8 target_base = correction ? u8(10) : u8(11);
        MoveLocation destination = MoveLocation(u64(input.target_word + offset), MOVE_MEMORY, target_base, NO_INDEX, u8(0));
        if (!correction) {
            MoveLocation source = MoveLocation(u64(values.source_word + offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
            if (floating) {
                cursor = store_float(cursor, destination, lane, wide);
                register = lane;
            } else {
                cursor = store_bits(cursor, destination, register, size);
            }
            cursor = collective_accumulate(cursor, values.operation, values.scalar, register, source);
        } else if (floating) {
            cursor = scan_float_correction(cursor, values.operation, values.scalar, lane, destination);
        } else if (extrema) {
            cursor = encode_operand_instruction(cursor, u16(0x8b) - byte_form, prefix, wide, u8(2), destination);
            cursor = collective_accumulate(cursor, values.operation, values.scalar, u8(2), move_gpr(register));
            cursor = store_bits(cursor, destination, u8(2), size);
        } else {
            // Integer sum and bitwise operators update the target field directly.
            u16 opcode = u16(reduction_opcode(values.operation, false)) - u16(2) - byte_form;
            cursor = encode_operand_instruction(cursor, opcode, prefix, wide, register, destination);
        }
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    if (!correction) { cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(11), u8(13)); }
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (!correction) {
        MoveLocation result = MoveLocation(0x28u64 + u64(range.stack_bytes), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation destination = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
            if (floating) {
                cursor = store_float(cursor, destination, u8(component), wide);
            } else {
                cursor = store_bits(cursor, destination, registers[component], size);
            }
        }
    }
    return collective_leaf(range, leaf_return(cursor, saved), output, values.identity, reference_count);
}

extern "C" LeafRange segmented_scan_range(SegmentedScanRange input, u32 add, LeafOutput output) on cpu {
    u32 saved = SAVE_R12_R13;
    LeafRange range = leaf_begin(output, saved);
    u64 image = address_bits(output.image);
    u64 start = image + range.end;
    ScanRange scan = input.scan;
    ReductionRange values = scan.values;
    bool correction = add != 0;
    u32 size = values.scalar.width / 8;
    bool wide = size == 8;
    bool floating = values.scalar.kind == ScalarKind::Float;
    MoveLocation source_stride = MoveLocation(u64(values.source + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), source_stride, true);
    MoveLocation target_stride = MoveLocation(u64(scan.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(13), target_stride, true);
    MoveLocation source = MoveLocation(u64(values.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), source, true);
    MoveLocation target = MoveLocation(u64(scan.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(11), target, true);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(11));
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    if (correction) {
        MoveLocation parameters = MoveLocation(u64(scan.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation incoming = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            cursor = load_float(cursor, u8(component), incoming, wide);
        }
        if (floating && values.operation != ReductionOperator::Sum) {
            cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(9));
        }
    } else {
        cursor = segmented_identity(cursor, values);
        // R9D records whether the local range reset at a segment head.
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(9), u8(9));
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    MoveLocation head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(end + 4u64, u16(0x83), u8(0), false, u8(7), head);
    put_byte(cursor, u8(0));
    cursor += 1u64;
    u64 head_end = 0u64;
    if (correction) {
        // At a head, the local scan already owns the rest of the range.
        head_end = condition_field(cursor, u8(0x85));
        cursor = head_end + 4u64;
        for (component in 0..4) {
            if (component == values.count) { break; }
            u8 lane = u8(component);
            MoveLocation destination = MoveLocation(u64(scan.target_word + values.offsets[component]), MOVE_MEMORY, u8(11), NO_INDEX, u8(0));
            if (floating) {
                cursor = scan_float_correction(cursor, values.operation, values.scalar, lane, destination);
            } else {
                cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), wide, lane, u8(0));
                cursor = collective_accumulate(cursor, values.operation, values.scalar, u8(0), destination);
                cursor = store_bits(cursor, destination, u8(0), size);
            }
        }
    } else {
        u64 no_head = condition_field(cursor, u8(0x84));
        cursor = segmented_identity(no_head + 4u64, values);
        cursor = move_immediate(cursor, u8(9), 1u64, false);
        patch_relative(unsafe_address(no_head), no_head, cursor);
        cursor = segmented_store(cursor, values, u8(11), scan.target_word);
        cursor = segmented_accumulate(cursor, values);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(11), u8(13));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (correction) {
        patch_relative(unsafe_address(head_end), head_end, cursor);
    } else {
        MoveLocation result = MoveLocation(0x28u64 + u64(range.stack_bytes), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        cursor = segmented_store(cursor, values, u8(10), 0);
        MoveLocation reset = MoveLocation(u64(input.value_words * 4), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(cursor, reset, u8(9), 4);
    }
    range.end = leaf_return(cursor, saved) - image;
    return range;
}

MoveLocation output_stack(u64 offset) on cpu {
    return MoveLocation(offset, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
}

u64 output_address(u64 at, u8 register, u64 offset) on cpu {
    return encode_operand_instruction(at, u16(0x8d), u8(0), true, register, output_stack(offset));
}

void patch_output_jump(u64 field, u64 target) on cpu {
    patch_relative(unsafe_address(field), field, target);
}

extern "C" PrintEncoding print_helper(ScalarEncoding scalar, address output, address unwind) on cpu {
    u64 start = address_bits(output);
    bool signed = scalar.kind == ScalarKind::Signed;
    bool wide = scalar.width == 64;
    bool floating = scalar.kind == ScalarKind::Float;
    u32 frame = floating ? 120 : (signed ? 80 : 88);
    u64 cursor = start;
    cursor += stack_register(u8(3), false, unsafe_address(cursor)); // RBX
    Operation rbx = Operation(u8(cursor - start), u8(3), u16(0), 0);
    cursor += stack_register(u8(6), false, unsafe_address(cursor)); // RSI
    Operation rsi = Operation(u8(cursor - start), u8(6), u16(0), 0);
    Operation rdi = Operation(u8(0), u8(7), u16(0), 0);
    if (signed) {
        cursor += stack_register(u8(7), false, unsafe_address(cursor));
        rdi.offset = u8(cursor - start);
    }
    cursor = register_instruction(cursor, u16(0x83), u8(0), true, u8(5), u8(4));
    put_byte(cursor, u8(frame));
    cursor += 1u64;
    u8 prolog = u8(cursor - start);
    Operation allocation = Operation(prolog, u8(0), u16(1), frame);
    array<Operation,4> operations = array<Operation,4>(rbx, rsi, signed ? rdi : allocation, allocation);
    u64 unwind_length = unwind_write(address_of(operations), signed ? 4u64 : 3u64, prolog, unwind);

    u64 locale_failure = 0u64;
    u64 format_failure = 0u64;
    u32 create = 0;
    u32 format = 0;
    u32 destroy = 0;
    if (floating) {
        // The print intrinsic passes the original bits in RCX. C's %g receives a double.
        cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, u8(0), u8(1));
        if (!wide) {
            cursor = register_instruction(cursor, u16(0x5a0f), u8(0xf3), false, u8(0), u8(0));
        }
        cursor = encode_operand_instruction(cursor, u16(0x110f), u8(0xf2), false, u8(0), output_stack(56u64));
        cursor = store_immediate(cursor, output_stack(64u64), 67u64, 2); // "C"
        // "%.17g\n" or "%.9g\n", followed by NUL.
        cursor = store_immediate(cursor, output_stack(72u64), wide ? 0x37312e25u64 : 0x67392e25u64, 4);
        cursor = store_immediate(cursor, output_stack(76u64), wide ? 0x0a67u64 : 10u64, 4);
        cursor = move_immediate(cursor, u8(1), 4u64, false); // LC_NUMERIC
        cursor = output_address(cursor, u8(2), 64u64);
        SymbolEncoding call = import_transfer(false, unsafe_address(cursor));
        create = u32(cursor - start + call.field);
        cursor += call.length;
        cursor = register_instruction(cursor, u16(0x85), u8(0), true, u8(0), u8(0));
        locale_failure = condition_field(cursor, u8(0x84));
        cursor = store_bits(locale_failure + 4u64, move_gpr(u8(6)), u8(0), 8);
        cursor = move_immediate(cursor, u8(1), 2u64, false);
        cursor = output_address(cursor, u8(2), 80u64);
        cursor = move_immediate(cursor, u8(8), 32u64, false);
        cursor = output_address(cursor, u8(9), 72u64);
        cursor = store_bits(cursor, output_stack(32u64), u8(6), 8);
        cursor = output_address(cursor, u8(0), 56u64);
        cursor = store_bits(cursor, output_stack(40u64), u8(0), 8);
        call = import_transfer(false, unsafe_address(cursor));
        format = u32(cursor - start + call.field);
        cursor += call.length;
        cursor = store_bits(cursor, move_gpr(u8(3)), u8(0), 4);
        cursor = store_bits(cursor, move_gpr(u8(1)), u8(6), 8);
        call = import_transfer(false, unsafe_address(cursor));
        destroy = u32(cursor - start + call.field);
        cursor += call.length;
        cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(3), u8(3));
        format_failure = condition_field(cursor, u8(0x88));
        cursor = output_address(format_failure + 4u64, u8(6), 80u64);
    } else if (scalar.kind == ScalarKind::Boolean) {
        cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(1), u8(1));
        u64 no = condition_field(cursor, u8(0x84));
        cursor = store_immediate(no + 4u64, output_stack(48u64), 0x65757274u64, 4); // "true"
        cursor = store_immediate(cursor, output_stack(52u64), 10u64, 1);
        cursor = move_immediate(cursor, u8(3), 5u64, false);
        u64 end = jump_field(cursor);
        patch_output_jump(no, end + 4u64);
        cursor = store_immediate(end + 4u64, output_stack(48u64), 0x736c6166u64, 4); // "fals"
        cursor = store_immediate(cursor, output_stack(52u64), 0x0a65u64, 2);
        cursor = move_immediate(cursor, u8(3), 6u64, false);
        patch_output_jump(end, cursor);
        cursor = output_address(cursor, u8(6), 48u64);
    } else {
        u32 width = wide ? 8 : 4;
        cursor = store_bits(cursor, move_gpr(u8(0)), u8(1), width);
        if (signed) {
            cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(7), u8(7));
            cursor = register_instruction(cursor, u16(0x85), u8(0), wide, u8(0), u8(0));
            u64 positive = condition_field(cursor, u8(0x89));
            cursor = register_instruction(positive + 4u64, u16(0xf7), u8(0), wide, u8(3), u8(0));
            cursor = move_immediate(cursor, u8(7), 1u64, false);
            patch_output_jump(positive, cursor);
        }
        MoveLocation digit = MoveLocation(0u64, MOVE_MEMORY, u8(6), NO_INDEX, u8(0));
        cursor = output_address(cursor, u8(6), wide ? 72u64 : 64u64);
        cursor = store_immediate(cursor, digit, 10u64, 1);
        cursor = move_immediate(cursor, u8(3), 1u64, false);
        cursor = move_immediate(cursor, u8(1), 10u64, false);
        // Convert the unsigned magnitude backwards into the existing stack buffer.
        u64 begin = cursor;
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(2), u8(2));
        cursor = register_instruction(cursor, u16(0xf7), u8(0), wide, u8(6), u8(1));
        cursor = register_instruction(cursor, u16(0x80), u8(0), false, u8(0), u8(2));
        put_byte(cursor, u8(48));
        cursor = register_instruction(cursor + 1u64, u16(0xff), u8(0), true, u8(1), u8(6));
        // MOV [RSI],DL. No REX prefix is needed for DL.
        put_byte(cursor, u8(0x88));
        put_byte(cursor + 1u64, u8(0x16));
        cursor = register_instruction(cursor + 2u64, u16(0xff), u8(0), false, u8(0), u8(3));
        cursor = register_instruction(cursor, u16(0x85), u8(0), wide, u8(0), u8(0));
        u64 back = condition_field(cursor, u8(0x85));
        patch_output_jump(back, begin);
        cursor = back + 4u64;
        if (signed) {
            cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(7), u8(7));
            u64 positive = condition_field(cursor, u8(0x84));
            cursor = register_instruction(positive + 4u64, u16(0xff), u8(0), true, u8(1), u8(6));
            cursor = store_immediate(cursor, digit, 45u64, 1);
            cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(3));
            patch_output_jump(positive, cursor);
        }
    }

    cursor = move_immediate(cursor, u8(1), 0xfffffff5u64, false); // STD_OUTPUT_HANDLE
    SymbolEncoding call = import_transfer(false, unsafe_address(cursor));
    u32 handle = u32(cursor - start + call.field);
    cursor += call.length;
    cursor = store_bits(cursor, move_gpr(u8(1)), u8(0), 8);
    cursor = store_bits(cursor, move_gpr(u8(2)), u8(6), 8);
    cursor = store_bits(cursor, move_gpr(u8(8)), u8(3), 4);
    cursor = output_address(cursor, u8(9), 44u64);
    cursor = encode_operand_instruction(cursor, u16(0xc7), u8(0), true, u8(0), output_stack(32u64));
    write_word(cursor, 0); // lpOverlapped
    cursor += 4u64;
    call = import_transfer(false, unsafe_address(cursor));
    u32 write = u32(cursor - start + call.field);
    cursor += call.length;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(0), u8(0));
    u64 write_failure = condition_field(cursor, u8(0x84));
    cursor = encode_operand_instruction(write_failure + 4u64, u16(0x39), u8(0), false, u8(3), output_stack(44u64));
    // SETE AL; MOVZX EAX,AL.
    put_byte(cursor, u8(0x0f));
    put_byte(cursor + 1u64, u8(0x94));
    put_byte(cursor + 2u64, u8(0xc0));
    put_byte(cursor + 3u64, u8(0x0f));
    put_byte(cursor + 4u64, u8(0xb6));
    put_byte(cursor + 5u64, u8(0xc0));
    u64 finish = jump_field(cursor + 6u64);
    u64 failure = finish + 4u64;
    if (floating) {
        patch_output_jump(locale_failure, failure);
        patch_output_jump(format_failure, failure);
    }
    patch_output_jump(write_failure, failure);
    cursor = register_instruction(failure, u16(0x31), u8(0), false, u8(0), u8(0));
    patch_output_jump(finish, cursor);
    cursor = register_instruction(cursor, u16(0x83), u8(0), true, u8(0), u8(4));
    put_byte(cursor, u8(frame));
    cursor += 1u64;
    if (signed) { cursor += stack_register(u8(7), true, unsafe_address(cursor)); }
    cursor += stack_register(u8(6), true, unsafe_address(cursor));
    cursor += stack_register(u8(3), true, unsafe_address(cursor));
    put_byte(cursor, u8(0xc3));
    return PrintEncoding(u32(cursor + 1u64 - start), u32(unwind_length),
        frame + (signed ? 24 : 16), create, format, destroy, handle, write);
}

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address bytes) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address target, address source, usize bytes) on cpu from "KERNEL32.dll";

const u32 HEAP_GENERATE_EXCEPTIONS = 4;
const u32 HEAP_ZERO_MEMORY = 8;

PacketPlanState packet_plan_empty() on cpu {
    return PacketPlanState(unsafe_address(0u64), 0u64);
}
PacketPlanFlow packet_plan_no_flow() on cpu {
    PacketPlanState empty=packet_plan_empty();return PacketPlanFlow(empty,empty,empty,empty);
}
u8 packet_plan_byte(address bytes,u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(bytes)+index));
}
void packet_plan_set_byte(address bytes,u64 index,u8 value) on cpu {
    unsafe_position<u8>(unsafe_address(address_bits(bytes)+index))=value;
}
PacketPlanState packet_plan_state(u64 count, u32 flags) on cpu {
    // A reachable state may have no slots. Its storage still marks a live path.
    return PacketPlanState(HeapAlloc(GetProcessHeap(), HEAP_GENERATE_EXCEPTIONS | flags, max(count, 1u64)), count);
}
void packet_plan_release(PacketPlanState state) on cpu {
    if (address_bits(state.bytes) != 0u64) { HeapFree(GetProcessHeap(), 0, state.bytes); }
}
void packet_plan_release_flow(PacketPlanFlow flow) on cpu {
    packet_plan_release(flow.next);
    packet_plan_release(flow.breaks);
    packet_plan_release(flow.continues);
    packet_plan_release(flow.exits);
}
PacketPlanState packet_plan_clone(PacketPlanState state) on cpu {
    PacketPlanState result = packet_plan_state(state.count, 0);
    RtlMoveMemory(result.bytes, state.bytes, state.count);
    return result;
}
void packet_plan_intersect(PacketPlanState left,PacketPlanState right) on cpu {
    u64 index=0u64;while(index<left.count){
        packet_plan_set_byte(left.bytes,index,packet_plan_byte(left.bytes,index)&packet_plan_byte(right.bytes,index));index+=1u64;
    }
}
bool packet_plan_update(PacketPlanState fixed, PacketPlanState entry, PacketPlanState back) on cpu {
    bool constrained = address_bits(back.bytes) != 0u64;
    bool changed = false;
    u64 index = 0u64;
    while (index < fixed.count) {
        u8 next = packet_plan_byte(entry.bytes, index);
        if (constrained) { next = next & packet_plan_byte(back.bytes, index); }
        if (next != packet_plan_byte(fixed.bytes, index)) {
            packet_plan_set_byte(fixed.bytes, index, next);
            changed = true;
        }
        index += 1u64;
    }
    return changed;
}
PacketPlanState packet_plan_merge(PacketPlanState target, PacketPlanState other) on cpu {
    if (address_bits(other.bytes) == 0u64) { return target; }
    if (address_bits(target.bytes) == 0u64) { return other; }
    packet_plan_intersect(target, other);
    packet_plan_release(other);
    return target;
}
PacketPlanFlow packet_plan_join(PacketPlanFlow left, PacketPlanFlow right) on cpu {
    return PacketPlanFlow(packet_plan_merge(left.next, right.next), packet_plan_merge(left.breaks, right.breaks),
        packet_plan_merge(left.continues, right.continues), packet_plan_merge(left.exits, right.exits));
}
extern "C" u32 packet_plan_uniform_operand(address bytes,u64 reg) on cpu {
    return packet_plan_byte(bytes,reg)!=u8(0)?1:0;
}
extern "C" u32 packet_plan_mark_operand(address bytes,u64 reg) on cpu {
    packet_plan_set_byte(bytes,reg,packet_plan_byte(bytes,reg)|u8(2));return 1;
}
extern "C" u32 packet_plan_width(usize bytes) on cpu { return bytes == 4u64 ? 1 : 0; }

address packet_plan_instruction(Span code, u64 index) on cpu {
    return unsafe_address(address_bits(code.data) + index * u64(code.stride));
}

PacketPlanFlow packet_plan_uniform_block(address context, Span code, PacketPlanHeader header,
    PacketPlanState state, address definitions, bool record, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.written) {
    PacketPlanFlow flow = packet_plan_no_flow();
    flow.next = state;
    u64 index = 0u64;
    while (index < code.count && address_bits(flow.next.bytes) != 0u64) {
        PacketPlanState current = flow.next;
        View value = host.ir.inspect(Node(packet_plan_instruction(code, index), N_INSTRUCTION));
        index += 1u64;
        PacketPlanFlow step;
        if (value.tag == OP_BRANCH) {
            PacketPlanFlow yes = packet_plan_uniform_block(context, value.first, header, packet_plan_clone(current), definitions, record, host);
            PacketPlanFlow no = packet_plan_uniform_block(context, value.second, header, current, definitions, record, host);
            step = packet_plan_join(yes, no);
        } else if (value.tag == OP_LOOP_VALUE) {
            step = packet_plan_loop(context, value.first, value.second, header, current, definitions, record, host);
        } else if (value.tag == OP_SCOPE) {
            step = packet_plan_uniform_block(context, value.first, header, current, definitions, record, host);
            step.next = packet_plan_merge(step.next, step.exits);
            step.exits = packet_plan_empty();
        } else if (value.tag == OP_BREAK_VALUE) {
            flow.breaks = packet_plan_merge(flow.breaks, current);
            flow.next = packet_plan_empty();
            break;
        } else if (value.tag == OP_CONTINUE_VALUE) {
            flow.continues = packet_plan_merge(flow.continues, current);
            flow.next = packet_plan_empty();
            break;
        } else if (value.tag == OP_EXIT_VALUE) {
            flow.exits = packet_plan_merge(flow.exits, current);
            flow.next = packet_plan_empty();
            break;
        } else {
            u32 uniform = 0;
            if (value.tag == OP_LOAD) {
                uniform = host.written(context, value.b) == 0 && packet_plan_byte(current.bytes, value.c) != u8(0) ? 1 : 0;
            } else if (value.tag == OP_LOAD_LOCAL) {
                uniform = u32(packet_plan_byte(current.bytes, header.registers + value.b));
            } else if (value.tag != OP_INDEX_VALUE) {
                uniform = visit_instruction(value, host.ir, current.bytes, c_function(packet_plan_uniform_operand));
            }
            Results outputs = view_results(value, host.ir);
            Word output = next_result(outputs);
            while (output.present != 0) {
                packet_plan_set_byte(current.bytes, output.value, u8(uniform));
                if (record) {
                    u8 previous = packet_plan_byte(definitions, output.value);
                    packet_plan_set_byte(definitions, output.value, previous != u8(1) && uniform != 0 ? u8(2) : u8(1));
                }
                output = next_result(outputs);
            }
            if (value.tag == OP_STORE_LOCAL) {
                packet_plan_set_byte(current.bytes, header.registers + value.a, packet_plan_byte(current.bytes, value.b));
            }
            continue;
        }
        flow.next = step.next;
        flow.breaks = packet_plan_merge(flow.breaks, step.breaks);
        flow.continues = packet_plan_merge(flow.continues, step.continues);
        flow.exits = packet_plan_merge(flow.exits, step.exits);
    }
    return flow;
}
PacketPlanFlow packet_plan_loop(address context,Span condition,Span body,PacketPlanHeader header,
    PacketPlanState entry,address definitions,bool record,PacketPlanHost host) on cpu 
    requires nonzero(host.ir.inspect) requires nonzero(host.written) {
    PacketPlanState fixed=packet_plan_clone(entry);
    while(true){
        PacketPlanFlow condition_flow=packet_plan_uniform_block(context,condition,header,packet_plan_clone(fixed),definitions,false,host);
        PacketPlanFlow body_flow=packet_plan_no_flow();
        if(address_bits(condition_flow.next.bytes)!=0u64){
            body_flow=packet_plan_uniform_block(context,body,header,packet_plan_clone(condition_flow.next),definitions,false,host);
        }
        PacketPlanState back=packet_plan_merge(body_flow.next,body_flow.continues);
        body_flow.next=packet_plan_empty();body_flow.continues=packet_plan_empty();
        bool changed = packet_plan_update(fixed, entry, back);
        packet_plan_release(back);
        if (!changed) {
            if(!record){
                packet_plan_release(fixed);packet_plan_release(entry);
                condition_flow.next=packet_plan_merge(condition_flow.next,body_flow.breaks);body_flow.breaks=packet_plan_empty();
                condition_flow.exits=packet_plan_merge(condition_flow.exits,body_flow.exits);body_flow.exits=packet_plan_empty();
                packet_plan_release_flow(body_flow);return condition_flow;
            }
            packet_plan_release_flow(condition_flow);packet_plan_release_flow(body_flow);break;
        }
        packet_plan_release_flow(condition_flow);
        packet_plan_release_flow(body_flow);
    }
    packet_plan_release(entry);
    PacketPlanFlow condition_flow=packet_plan_uniform_block(context,condition,header,fixed,definitions,record,host);
    PacketPlanFlow body_flow=packet_plan_no_flow();
    if(address_bits(condition_flow.next.bytes)!=0u64){
        body_flow=packet_plan_uniform_block(context,body,header,packet_plan_clone(condition_flow.next),definitions,record,host);
    }
    condition_flow.next=packet_plan_merge(condition_flow.next,body_flow.breaks);body_flow.breaks=packet_plan_empty();
    condition_flow.exits=packet_plan_merge(condition_flow.exits,body_flow.exits);body_flow.exits=packet_plan_empty();
    packet_plan_release_flow(body_flow);return condition_flow;
}
void packet_plan_uses(Span code, address uses, Host ir) on cpu requires nonzero(ir.inspect) {
    u64 index = 0u64;
    while (index < code.count) {
        View value = ir.inspect(Node(packet_plan_instruction(code, index), N_INSTRUCTION));
        if (value.tag == OP_INDEX_VALUE) {
            packet_plan_set_byte(uses, value.a, packet_plan_byte(uses, value.a) | u8(1));
        } else if (value.tag == OP_STORE) {
            packet_plan_mark_operand(uses, value.d);
        } else if (value.tag != OP_LOAD) {
            visit_instruction(value, ir, uses, c_function(packet_plan_mark_operand));
        }
        if (value.tag == OP_BRANCH || value.tag == OP_LOOP_VALUE) {
            packet_plan_uses(value.first, uses, ir);
            packet_plan_uses(value.second, uses, ir);
        } else if (value.tag == OP_SCOPE) {
            packet_plan_uses(value.first, uses, ir);
        }
        index += 1u64;
    }
}
struct PacketPlanRecords { address data; usize count; usize capacity; };

usize packet_plan_slot(inout PacketPlanRecords records) on cpu {
    if (records.count == records.capacity) {
        handle heap = GetProcessHeap();
        if (records.capacity == 0u64) {
            records.capacity = 8u64;
            records.data = HeapAlloc(heap, HEAP_GENERATE_EXCEPTIONS, records.capacity * size_of<PacketBodyStep>());
        } else {
            records.capacity *= 2u64;
            records.data = HeapReAlloc(heap, HEAP_GENERATE_EXCEPTIONS, records.data, records.capacity * size_of<PacketBodyStep>());
        }
    }
    usize index = records.count;
    records.count += 1u64;
    return index;
}

bool packet_plan_build(address context, Span code, PacketPlanHeader header, address uniform, address uses,
    inout PacketPlanRecords records, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.stride) requires nonzero(host.written) requires nonzero(host.scalar) {
    u64 index = 0u64;
    while(index<code.count){
        address instruction = packet_plan_instruction(code, index);
        View value = host.ir.inspect(Node(instruction, N_INSTRUCTION));
        PacketPlanDecision decision=PacketPlanDecision(PacketPlanOperation::Constant,0,PacketAddress::Unsupported,PacketViewAddress::Unsupported,PacketMathOperation::Sqrt);
        bool children = value.tag == OP_BRANCH || value.tag == OP_LOOP_VALUE || value.tag == OP_SCOPE;
        if(value.tag==OP_CONSTANT){decision.operation=PacketPlanOperation::Constant;
        }else if(value.tag==OP_INDEX_VALUE){
            if(packet_plan_byte(uses,value.a)==u8(1)){index+=1u64;continue;}decision.operation=PacketPlanOperation::Index;
        }else if(value.tag==OP_UNIFORM){decision.operation=PacketPlanOperation::Uniform;
        }else if(value.tag==OP_LOAD_LOCAL){decision.operation=PacketPlanOperation::LocalLoad;
        }else if(value.tag==OP_STORE_LOCAL){decision.operation=PacketPlanOperation::LocalStore;
        }else if(value.tag==OP_LOAD){
            decision.operation=PacketPlanOperation::Load;
            decision.dataset=packet_dataset_address(host.stride(context,value.b),u32(value.d),header.selected!=0,host.written(context,value.b)!=0,(packet_plan_byte(uses,value.c)&u8(1))!=u8(0),packet_plan_byte(uniform,value.c)!=u8(0));
            if(decision.dataset==PacketAddress::Unsupported){return false;}
        }else if(value.tag==OP_VIEW_LOAD){
            decision.operation=PacketPlanOperation::ViewLoad;
            bool stride_uniform=operands(value.left,host.ir,uniform,c_function(packet_plan_uniform_operand))!=0;
            decision.view=packet_view_address(u32(value.c),packet_plan_byte(uniform,indexed_pair_word(value.first,0u64))!=u8(0)&&packet_plan_byte(uniform,indexed_pair_word(value.first,1u64))!=u8(0),packet_plan_byte(uniform,value.b)!=u8(0),stride_uniform);
            if(decision.view==PacketViewAddress::Unsupported){return false;}
        }else if(value.tag==OP_STORE){
            decision.operation=PacketPlanOperation::Store;
            decision.dataset=packet_dataset_address(host.stride(context,value.a),u32(value.c),header.selected!=0,true,(packet_plan_byte(uses,value.b)&u8(1))!=u8(0),false);
            if(decision.dataset==PacketAddress::Unsupported){return false;}
        }else if(value.tag==OP_UNARY){decision.operation=PacketPlanOperation::Unary;
        }else if(value.tag==OP_BINARY){decision.operation=PacketPlanOperation::Binary;decision.uniform_count=packet_plan_byte(uniform,value.c)!=u8(0)?1:0;
        }else if(value.tag==OP_INTEGER_DIVIDE){decision.operation=PacketPlanOperation::Divide;
        }else if(value.tag==OP_CAST){decision.operation=bit_copy(host.scalar(context,value.b),host.scalar(context,value.a))?PacketPlanOperation::Copy:PacketPlanOperation::Cast;
        }else if(value.tag==OP_MATH){
            decision.operation=PacketPlanOperation::Math;
            if(value.b==9){decision.math=PacketMathOperation::Sqrt;
            }else if(value.b==0){decision.math=PacketMathOperation::Abs;
            }else if(value.b==10){decision.math=PacketMathOperation::Min;
            }else if(value.b==11){decision.math=PacketMathOperation::Max;
            }else{return false;}
        }else if(value.tag==OP_SELECT_VALUE){decision.operation=PacketPlanOperation::Select;
        }else if(children){
            if(value.tag!=OP_SCOPE&&packet_plan_byte(uniform,value.a)==u8(0)){return false;}
            decision.operation=value.tag==OP_BRANCH?PacketPlanOperation::If:(value.tag==OP_LOOP_VALUE?PacketPlanOperation::Loop:PacketPlanOperation::Scope);
        }else if(value.tag==OP_EXIT_VALUE){decision.operation=PacketPlanOperation::Exit;
        }else if(value.tag==OP_BREAK_VALUE){decision.operation=PacketPlanOperation::Break;
        }else if(value.tag==OP_CONTINUE_VALUE){decision.operation=PacketPlanOperation::Continue;
        }else{return false;}
        usize slot = packet_plan_slot(records);
        usize first_count = 0u64;
        usize second_count = 0u64;
        if (children) {
            usize first_start = records.count;
            if (!packet_plan_build(context, value.first, header, uniform, uses, records, host)) { return false; }
            first_count = records.count - first_start;
            if (value.tag != OP_SCOPE) {
                usize second_start = records.count;
                if (!packet_plan_build(context, value.second, header, uniform, uses, records, host)) { return false; }
                second_count = records.count - second_start;
            }
        }
        address destination = unsafe_address(address_bits(records.data) + slot * size_of<PacketBodyStep>());
        unsafe_position<PacketBodyStep>(destination) = PacketBodyStep(instruction, first_count, second_count, decision);
        index += 1u64;
    }
    return true;
}
extern "C" void packet_plan_drop(Span plan) on cpu {
    if (address_bits(plan.data) != 0u64) { HeapFree(GetProcessHeap(), 0, plan.data); }
}

bool packet_plan(address context, Span code, PacketPlanHeader header, address output, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.stride) requires nonzero(host.written) requires nonzero(host.scalar)
    requires nonzero(host.widths) {
    if (header.tile != 0 || header.writes == 0u64) { return false; }
    if (host.widths(context, c_function(packet_plan_width)) == 0) { return false; }
    PacketPlanState definitions = packet_plan_state(header.registers, HEAP_ZERO_MEMORY);
    PacketPlanState state = packet_plan_state(header.slots, HEAP_ZERO_MEMORY);
    PacketPlanFlow flow = packet_plan_uniform_block(context, code, header, state, definitions.bytes, true, host);
    packet_plan_release_flow(flow);
    u64 index = 0u64;
    while (index < header.registers) {
        packet_plan_set_byte(definitions.bytes, index, packet_plan_byte(definitions.bytes, index) == u8(2) ? u8(1) : u8(0));
        index += 1u64;
    }
    PacketPlanState uses = packet_plan_state(header.registers, HEAP_ZERO_MEMORY);
    packet_plan_uses(code, uses.bytes, host.ir);
    PacketPlanRecords records = PacketPlanRecords(unsafe_address(0u64), 0u64, 0u64);
    bool valid = packet_plan_build(context, code, header, definitions.bytes, uses.bytes, records, host);
    packet_plan_release(definitions);
    Span plan = Span(records.data, records.count, size_of<PacketBodyStep>());
    if (!valid) {
        packet_plan_release(uses);
        packet_plan_drop(plan);
        return false;
    }
    unsafe_position<PacketPlanProduct>(output) = PacketPlanProduct(plan, Span(uses.bytes, uses.count, 1u64));
    return true;
}

extern "C" RangeProduct emit_ranges(RangeEntry entry, usize words_or_keys, u32 complete, RangeSink sink)
    on cpu requires nonzero(sink.emit) {
    RangeProduct single = RangeProduct(RangeShape::Element, 0, 0u64);
    if (entry == RangeEntry::Sort) {
        usize key = words_or_keys - 1u64;
        RangeDestination first = complete == 0 ? RangeDestination::Main : RangeDestination::Initializers;
        sink.emit(sink.context, RangeEntry::Sort, first, key);
        if (complete == 0) { return single; }
        while (key != 0u64) {
            key -= 1u64;
            sink.emit(sink.context, RangeEntry::SortKey, RangeDestination::Initializers, key);
        }
        sink.emit(sink.context, RangeEntry::SortHistogram, RangeDestination::Histogram, 0u64);
        sink.emit(sink.context, RangeEntry::SortScatter, RangeDestination::Scatter, 0u64);
        sink.emit(sink.context, RangeEntry::SortGather, RangeDestination::Gather, 0u64);
        return RangeProduct(RangeShape::Sort, 0, 0u64);
    }
    if (sink.emit(sink.context, entry, RangeDestination::Main, 0u64) == 0
        || complete == 0 || entry == RangeEntry::Map) {
        return single;
    }

    RangeShape shape = RangeShape::Element;
    if (entry == RangeEntry::CustomReduce) {
        sink.emit(sink.context, RangeEntry::ReduceTail, RangeDestination::Tail, 0u64);
        shape = RangeShape::ReduceTail;
    } else if (entry == RangeEntry::CustomScan) {
        if (sink.emit(sink.context, RangeEntry::ScanPrefix, RangeDestination::Tail, 0u64) != 0) {
            sink.emit(sink.context, RangeEntry::ScanApply, RangeDestination::Apply, 0u64);
        }
        shape = RangeShape::CustomScan;
    } else if (entry == RangeEntry::CustomSegmentedReduce) {
        if (sink.emit(sink.context, RangeEntry::SegmentPrefix, RangeDestination::Tail, 0u64) != 0) {
            sink.emit(sink.context, RangeEntry::SegmentApply, RangeDestination::Apply, 0u64);
        }
        shape = RangeShape::CustomScan;
    } else if (entry == RangeEntry::SegmentedReduce) {
        sink.emit(sink.context, RangeEntry::SegmentedFinish, RangeDestination::Tail, 0u64);
        shape = RangeShape::SegmentedFinish;
    } else if (entry == RangeEntry::Scan) {
        sink.emit(sink.context, RangeEntry::ScanAdd, RangeDestination::Tail, 0u64);
        shape = RangeShape::ScanAdd;
    } else if (entry == RangeEntry::Compact) {
        sink.emit(sink.context, RangeEntry::CompactScatter, RangeDestination::Tail, 0u64);
        shape = RangeShape::CompactScatter;
    }
    return RangeProduct(shape, 1, words_or_keys);
}

ScalarValueEncoding value_encoding(BinaryEncoding body, u64 left, bool left_wide,
    u64 right, bool right_wide) on cpu {
    return ScalarValueEncoding(body.length, ScalarLiteral(left, body.left_literal, u32(left_wide)),
        ScalarLiteral(right, body.right_literal, u32(right_wide)), ScalarMathReference(0u64, 0, array<u8,6>(u8(0))));
}

BinaryOperation value_binary_operation(u32 operation) on cpu {
    // The original IR distinguishes integer and floating arithmetic. The same
    // x64 operation uses the actual scalar kind to pick the instruction.
    if (operation < 4) { return BinaryOperation(operation); }
    if (operation < 7) { return BinaryOperation(operation - 4); }
    if (operation < 13) { return BinaryOperation(operation + 3); }
    return BinaryOperation(operation - 9);
}

ScalarValueEncoding emit_scalar_math_value(View value, ScalarValueHost host,
    MoveLocation dst, address output) on cpu requires nonzero(host.location) requires nonzero(host.scalar) {
    usize operand = unsafe_position<usize>(value.first.data);
    MoveLocation left = host.location(host.context, operand);
    u32 size = host.scalar(host.context, operand).width / 8;
    MoveLocation right = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    if (value.first.count > usize(1)) {
        usize other = unsafe_position<usize>(unsafe_address(address_bits(value.first.data) + u64(value.first.stride)));
        right = host.location(host.context, other);
    }
    return scalar_math(ScalarMathOperation(u32(value.b)), dst, left, right, size, output);
}

u32 division_literal32(View divisor, ScalarEncoding scalar) on cpu {
    // The original IR supplies nonzero scalar-width constant bits, or a register.
    if (divisor.tag != 0) { return 0; }
    u32 bits = u32(divisor.a);
    if (scalar.kind == ScalarKind::Signed) {
        if (scalar.width == 8) { return u32(i32(i8(u8(bits)))); }
        if (scalar.width == 16) { return u32(i32(i16(u16(bits)))); }
    }
    return bits;
}

u64 emit_scalar_integer_division(View value, ScalarValueHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar) {
    View result = host.ir.inspect(value.right);
    MoveLocation first = host.location(host.context, result.a);
    MoveLocation second = first;
    if (result.tag == 2) { second = host.location(host.context, result.b); }
    DivisionOutputs outputs = DivisionOutputs(first, second, result.tag + 1);
    MoveLocation numerator = host.location(host.context, value.a);
    ScalarEncoding scalar = host.scalar(host.context, value.a);
    View divisor = host.ir.inspect(value.left);
    if (scalar.width != 64) {
        u32 constant = division_literal32(divisor, scalar);
        if (constant != 0) {
            return divide_constant32(outputs, numerator, constant, scalar, output);
        }
    } else if (divisor.tag == 0) {
        MoveLocation literal = MoveLocation(u64(divisor.a), MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        return divide_value(outputs, numerator, literal, scalar, scalar, output);
    }

    MoveLocation right = host.location(host.context, divisor.a);
    return divide_value(outputs, numerator, right, scalar, host.scalar(host.context, divisor.a), output);
}
ScalarValueEncoding emit_scalar_value(View value,
    ScalarValueHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar) {
    if (value.tag == OP_INTEGER_DIVIDE) {
        u64 length = emit_scalar_integer_division(value, host, output);
        return value_encoding(BinaryEncoding(length, 0u64, 0u64), 0u64, false, 0u64, false);
    }
    MoveLocation dst = host.location(host.context, value.a);
    if (value.tag == OP_MATH) {
        return emit_scalar_math_value(value, host, dst, output);
    }
    bool comparison = value.tag == OP_BINARY && value.left.kind == N_COMPARISON;
    usize scalar_reg = comparison ? value.b : value.a;
    ScalarEncoding scalar = host.scalar(host.context, scalar_reg);
    MoveLocation first = host.location(host.context, value.b);
    if (value.tag == OP_UNARY) {
        MoveEncoding emitted = value_unary(dst, first, scalar, ValueUnaryOperation(u32(value.c)), output);
        BinaryEncoding body = BinaryEncoding(emitted.length, emitted.literal_field, 0u64);
        return value_encoding(body, first.value, false, 0u64, false);
    }
    if (value.tag == OP_CAST) {
        ScalarEncoding from = host.scalar(host.context, value.b);
        BinaryEncoding emitted = value_cast(dst, first, from, scalar, output);
        return value_encoding(emitted, first.value, from.width == 64, 0x5f000000u64, false);
    }
    MoveLocation second = host.location(host.context, value.c);
    if (value.tag == OP_BINARY) {
        BinaryOperation operation = value_binary_operation(host.ir.inspect(value.right).tag);
        if (comparison) {
            BinaryEncoding emitted = scalar_compare(dst, first, second, scalar.width / 8,
                scalar.kind == ScalarKind::Signed, scalar.kind == ScalarKind::Float, operation, output);
            return value_encoding(emitted, first.value, false, second.value, scalar.width == 64);
        }
        ScalarEncoding count = host.scalar(host.context, value.c);
        BinaryEncoding emitted = scalar_binary(dst, first, second, scalar.width / 8,
            scalar.kind == ScalarKind::Signed, scalar.kind == ScalarKind::Float, count.width / 8,
            count.kind == ScalarKind::Signed, operation, output);
        return value_encoding(emitted, first.value, false, second.value, scalar.width == 64);
    }
    MoveLocation right = host.location(host.context, value.d);
    BinaryEncoding emitted = value_select(dst, first, second, right, scalar.width / 8,
        scalar.kind == ScalarKind::Signed, output);
    return value_encoding(emitted, second.value, false, right.value, scalar.width == 64);
}


void map_range_part(MapRangeHost host, MapRangePart part, u64 start, u64 end) on cpu
    requires nonzero(host.observe) {
    host.observe(host.context, MapRangeObservation(start, end, usize(0), 0, part));
}

u64 close_map_loop(MapRangeHost host, u64 begin, u64 exit_field, u64 skip_field, u8 step,
    MapRangePart advance, MapRangePart back_edge) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit) requires nonzero(host.observe) {
    NativeCodeBuffer buffer = host.reserve(host.context, 8u64);
    RangeEnd range = finish_range(buffer.image, buffer.end, begin, exit_field, skip_field, step);
    host.commit(host.context, range.end);
    if (range.back_edge != buffer.end) {
        map_range_part(host, advance, buffer.end, range.back_edge);
    }
    map_range_part(host, back_edge, range.back_edge, range.end);
    return range.end;
}

void emit_map_packet(MapRangeInput input, MapRangeHost host, ControlTargets targets, u32 data, u32 stride) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.packet) requires nonzero(host.observe) {
    bool sparse = input.selected != 0;
    NativeCodeBuffer buffer = host.reserve(host.context, sparse ? 96u64 : 16u64);
    u64 begin = buffer.end;
    address output = unsafe_address(address_bits(buffer.image) + begin);
    u64 tail = 0u64;
    if (sparse) {
        SparseRangeEncoding encoded = sparse_collect(data, stride, input.word, input.slots, output);
        u64 end = begin + encoded.length;
        host.commit(host.context, end);
        host.observe(host.context, MapRangeObservation(begin, end, input.binding, input.word, MapRangePart::PacketCollect));
        tail = begin + encoded.tail_field;
    } else {
        u64 end = begin + range_bounds(true, output);
        host.commit(host.context, end);
        map_range_part(host, MapRangePart::PacketBounds, begin, end);
        tail = end - 4u64;
    }
    host.packet(host.context, input.packet, targets);
    close_map_loop(host, begin, tail, 0u64, sparse ? u8(0) : u8(4),
        MapRangePart::PacketAdvance, MapRangePart::PacketBackEdge);
    if (sparse) {
        NativeCodeBuffer tail_buffer = host.reserve(host.context, 16u64);
        address tail_output = unsafe_address(address_bits(tail_buffer.image) + tail_buffer.end);
        u64 end = tail_buffer.end + sparse_tail(input.slots, tail_output);
        host.commit(host.context, end);
        map_range_part(host, MapRangePart::PacketTail, tail_buffer.end, end);
    }
}

u64 move_tile_base(MapRangeHost host, u32 offset, u64 phase, bool restore) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit) requires nonzero(host.observe) {
    NativeCodeBuffer buffer = host.reserve(host.context, 24u64);
    MoveLocation memory = MoveLocation(u64(offset), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    MoveLocation index = move_gpr(u8(3));
    address output = unsafe_address(address_bits(buffer.image) + buffer.end);
    MoveEncoding move = copy_scalar_move(restore ? index : memory, restore ? memory : index, 4, false, output);
    u64 end = buffer.end + move.length;
    host.commit(host.context, end);
    MapRangePart part = restore ? MapRangePart::TilePhaseEnter : MapRangePart::TileBase;
    host.observe(host.context, MapRangeObservation(buffer.end, end, usize(phase), offset, part));
    return end;
}

void emit_map_tile_input(MapRangeInput input, MapRangeHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit) requires nonzero(host.observe)
    requires nonzero(host.next_input) requires nonzero(host.observe_input) {
    NativeCodeBuffer buffer = host.reserve(host.context, 48u64);
    address output = unsafe_address(address_bits(buffer.image) + buffer.end);
    TileInputBoundaryCode boundary = tile_input_boundary(input.workgroup, input.slots, output);
    u64 end = buffer.end + boundary.length;
    u64 skip = buffer.end + boundary.skip_field;
    host.commit(host.context, end);
    host.observe(host.context, MapRangeObservation(buffer.end, end, usize(0), input.workgroup, MapRangePart::TileInputBoundary));
    TileInputFragment item = host.next_input(host.context);
    while (item.unit_bytes != 0) {
        u32 data = u32(item.binding) * input.buffer_bytes;
        u32 storage = item.storage_base + item.source_offset % 4;
        TileInputCopy copy = TileInputCopy(data, data + input.stride_field,
            item.source_offset, storage, input.slots, item.unit_bytes);
        buffer = host.reserve(host.context, 80u64);
        output = unsafe_address(address_bits(buffer.image) + buffer.end);
        end = buffer.end + tile_input_copy(copy, output);
        host.commit(host.context, end);
        host.observe_input(host.context, buffer.end, end, item, storage);
        item = host.next_input(host.context);
    }
    buffer = host.reserve(host.context, 0u64);
    patch_relative(unsafe_address(address_bits(buffer.image) + skip), skip, buffer.end);
}

void emit_tile_private_values(MapRangeInput input, MapRangeHost host, u64 phase, bool restore) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit) requires nonzero(host.observe)
    requires nonzero(host.next_private) requires nonzero(host.observe_private) {
    TilePrivateSlot item = host.next_private(host.context, restore);
    if (address_bits(item.scalar_origin) == 0u64) { return; }
    NativeCodeBuffer buffer = host.reserve(host.context, 16u64);
    address output = unsafe_address(address_bits(buffer.image) + buffer.end);
    u64 end = buffer.end + tile_lane_pointer(input.slots + 4, output);
    host.commit(host.context, end);
    host.observe(host.context, MapRangeObservation(buffer.end, end, usize(phase), u32(restore), MapRangePart::TilePrivatePointer));
    u32 stride = input.workgroup * 4;
    while (address_bits(item.scalar_origin) != 0u64) {
        buffer = host.reserve(host.context, 64u64);
        output = unsafe_address(address_bits(buffer.image) + buffer.end);
        TilePrivateCopy copy = TilePrivateCopy(item.location, item.scalar, item.storage_offset, stride, restore);
        end = buffer.end + tile_private_copy(copy, output);
        host.commit(host.context, end);
        host.observe_private(host.context, buffer.end, end, phase, restore, item);
        item = host.next_private(host.context, restore);
    }
}

void advance_tile_phase(MapRangeInput input, MapRangeHost host, u64 phase, u32 base, u64 begin) on cpu
    requires nonzero(host.next_private) requires nonzero(host.observe_private) requires nonzero(host.reserve)
    requires nonzero(host.commit) requires nonzero(host.observe) {
    emit_tile_private_values(input, host, phase, false);
    NativeCodeBuffer buffer = host.reserve(host.context, 32u64);
    address output = unsafe_address(address_bits(buffer.image) + buffer.end);
    u64 end = buffer.end + tile_phase_advance(base, input.slots, buffer.end - begin, output);
    host.commit(host.context, end);
    host.observe(host.context, MapRangeObservation(buffer.end, end, usize(phase), 0, MapRangePart::TilePhaseAdvance));
}

bool emit_map_tile_phases(MapRangeInput input, MapRangeHost host, ControlTargets targets) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit) requires nonzero(host.observe)
    requires nonzero(host.scalar) requires nonzero(host.next_input) requires nonzero(host.observe_input)
    requires nonzero(host.ir.inspect) requires nonzero(host.phase)
    requires nonzero(host.next_private) requires nonzero(host.observe_private) requires nonzero(host.tile_step) {
    u32 base = input.slots + 4;
    move_tile_base(host, base, 0u64, false);
    emit_map_tile_input(input, host);
    u64 phase = 0u64;
    while (phase < input.phase_count) {
        TilePhaseView row = host.phase(host.context, phase);
        u64 begin = move_tile_base(host, base, phase, true);
        emit_tile_private_values(input, host, phase, true);
        if (host.scalar(host.context, row.code, targets) == 0) { return false; }
        if (address_bits(row.step.data) == 0u64) {
            advance_tile_phase(input, host, phase, base, begin);
        } else {
            View value = host.ir.inspect(row.step);
            if (value.tag == OP_TILE_PUBLISH) {
                host.tile_step(host.context, row.step.data, ScalarLeaf::Publish, value);
            }
            advance_tile_phase(input, host, phase, base, begin);
            if (value.tag == OP_TILE_COLLECTIVE) {
                host.tile_step(host.context, row.step.data, ScalarLeaf::Collective, value);
            }
        }
        phase += 1u64;
    }
    return true;
}

u64 finish_range_frame(u32 frame, RangePrologue entry, u64 exits, address context,
    cfn<NativeCodeBuffer,address,u64> reserve, cfn<void,address,u64> commit) on cpu
    requires nonzero(reserve) requires nonzero(commit) {
    NativeCodeBuffer buffer = reserve(context, 20u64 + 9u64 * entry.count);
    u64 end = range_epilogue(frame, entry, buffer.image, buffer.end, exits);
    commit(context, end);
    return end;
}

extern "C" u32 emit_map_range(MapRangeInput input, MapRangeHost host) on cpu
    requires nonzero(host.frame.reserve) requires nonzero(host.frame.publish) requires nonzero(host.reserve)
    requires nonzero(host.commit) requires nonzero(host.observe)
    requires nonzero(host.packet) requires nonzero(host.scalar) requires nonzero(host.next_input) requires nonzero(host.observe_input)
    requires nonzero(host.ir.inspect) requires nonzero(host.phase)
    requires nonzero(host.next_private) requires nonzero(host.observe_private) requires nonzero(host.tile_step) {
    u64 exits = 0u64;
    ControlTargets targets = ControlTargets(address_of(exits), address_of(exits), address(), 0u64);
    RangePrologue entry = range_prologue(input.frame, input.saved, host.context, host.frame);
    host.observe(host.context, MapRangeObservation(0u64, entry.end, usize(0), input.frame, MapRangePart::Enter));
    u32 data = u32(input.binding) * input.buffer_bytes;
    u32 stride = data + input.stride_field;
    if (address_bits(input.packet) != 0u64) { emit_map_packet(input, host, targets, data, stride); }

    NativeCodeBuffer buffer = host.reserve(host.context, 64u64);
    ScalarRangePrefix prefix = scalar_range_prefix(buffer.image, buffer.end, input.selected, data, stride, input.word);
    host.commit(host.context, prefix.end);
    u64 bounds_end = prefix.exit_field + 4u64;
    map_range_part(host, MapRangePart::ScalarBounds, buffer.end, bounds_end);
    if (input.selected != 0) {
        host.observe(host.context, MapRangeObservation(bounds_end, prefix.end, input.binding, input.word,
            MapRangePart::ScalarSelection));
    }
    u8 step = u8(0);
    if (input.phase_count != 0u64) {
        if (!emit_map_tile_phases(input, host, targets)) { return entry.stack_bytes; }
    } else {
        if (input.tile_scope != 0) { emit_map_tile_input(input, host); }
        if (host.scalar(host.context, input.code, targets) == 0) { return entry.stack_bytes; }
        step = u8(1);
    }
    u64 exit = close_map_loop(host, buffer.end, prefix.exit_field, prefix.skip_field, step,
        MapRangePart::ScalarAdvance, MapRangePart::ScalarBackEdge);
    u64 end = finish_range_frame(input.frame, entry, exits, host.context, host.reserve, host.commit);
    host.observe(host.context, MapRangeObservation(exit, end, usize(0), input.frame, MapRangePart::Exit));
    return entry.stack_bytes;
}

u64 scalar_storage_read(MoveLocation dst, MoveLocation memory, ScalarEncoding scalar, address output) on cpu {
    return read_scalar_move(dst, memory, scalar.width / 8, scalar.kind == ScalarKind::Signed,
        scalar.kind == ScalarKind::Boolean, output);
}

ScalarStorageEncoding scalar_storage_copy(MoveLocation dst, MoveLocation src, ScalarEncoding scalar,
    address output) on cpu {
    MoveEncoding move = copy_scalar_move(dst, src, scalar.width / 8, scalar.kind == ScalarKind::Signed, output);
    return ScalarStorageEncoding(move.length, src.value, move.literal_field);
}

ScalarStorageEncoding emit_scalar_storage(View value, ScalarStorageHost host,
    address output) on cpu
    requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.local) {
    ScalarValueHost values = host.values;
    address context = values.context;
    u32 kind = value.tag;
    if (kind == OP_CONSTANT) {
        MoveLocation immediate = MoveLocation(u64(value.b), MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        return scalar_storage_copy(values.location(context, value.a), immediate, values.scalar(context, value.a), output);
    }
    if (kind == OP_INDEX_VALUE) {
        MoveEncoding move = copy_scalar_move(values.location(context, value.a), move_gpr(u8(3)), 4, false, output);
        return ScalarStorageEncoding(move.length, 0u64, 0u64);
    }
    if (kind == OP_UNIFORM) {
        MoveLocation memory = MoveLocation(u64(value.b), MOVE_MEMORY, u8(13), NO_INDEX, u8(0));
        u64 length = scalar_storage_read(values.location(context, value.a), memory, values.scalar(context, value.a), output);
        return ScalarStorageEncoding(length, 0u64, 0u64);
    }
    if (kind == OP_LOAD_LOCAL) {
        MoveLocation src = host.local(context, value.b);
        MoveLocation dst = values.location(context, value.a);
        ScalarEncoding scalar = values.scalar(context, value.a);
        if (src.kind == MOVE_MEMORY && scalar.kind == ScalarKind::Boolean) {
            u64 length = scalar_storage_read(dst, src, scalar, output);
            return ScalarStorageEncoding(length, 0u64, 0u64);
        }
        return scalar_storage_copy(dst, src, scalar, output);
    }
    if (kind == OP_STORE_LOCAL) {
        return scalar_storage_copy(host.local(context, value.a), values.location(context, value.b),
            values.scalar(context, value.b), output);
    }
    u64 start = address_bits(output);
    if (kind == OP_BUFFER_STRIDE) {
        u32 offset = u32(value.b) * host.buffer_bytes + host.stride_field;
        u64 cursor = buffer_field_at(start, offset, u16(0x8b), u8(0));
        ScalarStorageEncoding stored = scalar_storage_copy(values.location(context, value.a),
            move_gpr(u8(0)), values.scalar(context, value.a), unsafe_address(cursor));
        return ScalarStorageEncoding(cursor - start + stored.length, 0u64, 0u64);
    }

    bool local = kind == OP_LOAD_LOCAL_INDEXED || kind == OP_STORE_LOCAL_INDEXED;
    bool read = kind == OP_LOAD_LOCAL_INDEXED || kind == OP_LOAD;
    MoveLocation memory;
    u64 prefix;
    if (local) {
        // Addressable local spans are placed in the original stack frame before emission.
        MoveLocation stack = host.local(context, read ? value.b : value.a);
        u32 word = unsafe_position<u32>(value.first.data);
        MoveLocation index = values.location(context, read ? value.d : value.c);
        AddressEncoding indexed = value_memory(u8(4), index, 1, u32(stack.value) + word, output);
        memory = MoveLocation(u64(indexed.offset), MOVE_MEMORY, indexed.base, indexed.index, indexed.shift);
        prefix = indexed.length;
    } else {
        usize binding = read ? value.b : value.a;
        usize index = read ? value.c : value.b;
        u32 data = u32(binding) * host.buffer_bytes;
        prefix = dataset_index(data, data + host.stride_field, values.location(context, index),
            values.scalar(context, index), false, output);
        memory = buffer_element_operand(u32(read ? value.d : value.c));
    }
    usize reg = read ? value.a : value.d;
    MoveLocation location = values.location(context, reg);
    ScalarEncoding scalar = values.scalar(context, reg);
    address cursor = unsafe_address(start + prefix);
    u64 length = read ? scalar_storage_read(location, memory, scalar, cursor)
        : write_scalar_move(memory, location, scalar.width / 8, cursor);
    return ScalarStorageEncoding(prefix + length, 0u64, 0u64);
}

usize indexed_pair_word(Span pair, u64 index) on cpu {
    return unsafe_position<usize>(unsafe_address(address_bits(pair.data) + index * u64(pair.stride)));
}

ViewElement indexed_view_element(Span pair, usize index, View stride, ScalarValueHost values) on cpu
    requires nonzero(values.location) {
    address context = values.context;
    MoveLocation low = values.location(context, indexed_pair_word(pair, 0u64));
    MoveLocation high = values.location(context, indexed_pair_word(pair, 1u64));
    MoveLocation bound_index = values.location(context, index);
    if (stride.tag == 0) {
        return ViewElement(low, high, bound_index, move_gpr(u8(0)), move_gpr(u8(0)), u64(stride.a), 0);
    }
    MoveLocation step_low = values.location(context, indexed_pair_word(stride.first, 0u64));
    MoveLocation step_high = values.location(context, indexed_pair_word(stride.first, 1u64));
    return ViewElement(low, high, bound_index, step_low, step_high, 0u64, 1);
}

enum IndexedAddressMode:u32 { Base=0; Memory=1; Address=2; };

AddressEncoding indexed_operand(View base, usize offset, u32 word,
    IndexedAddressMode mode, IndexedAddressHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) {
    ScalarValueHost values = host.values;
    address context = values.context;
    u64 prefix;
    u32 scale = 1;
    if (base.tag == INDEXED_DATA) {
        u32 data = u32(base.a) * host.buffer_bytes;
        prefix = dataset_index(data, data + host.stride_field, values.location(context, base.b),
            values.scalar(context, base.b), true, output);
    } else if (base.tag == INDEXED_POINTER) {
        MoveLocation low = values.location(context, indexed_pair_word(base.first, 0u64));
        MoveLocation high = values.location(context, indexed_pair_word(base.first, 1u64));
        prefix = pointer_element_base(low, high, values.location(context, base.a),
            values.scalar(context, base.a), u32(base.b), output);
    } else if (base.tag == INDEXED_TILE) {
        prefix = tile_index_base(values.location(context, base.b), values.scalar(context, base.b),
            host.tile_stream(context, base.a, 0).offset, output);
        scale = host.workgroup;
    } else {
        ViewElement view = indexed_view_element(base.first, base.a, values.ir.inspect(base.left), values);
        prefix = view_element_base(view, output);
    }

    if (mode == IndexedAddressMode::Base) {
        return AddressEncoding(prefix, 0, u8(15), NO_INDEX, u8(0));
    }
    address cursor = unsafe_address(address_bits(output) + prefix);
    MoveLocation index = values.location(context, offset);
    ScalarEncoding scalar = values.scalar(context, offset);
    if (mode == IndexedAddressMode::Address) {
        u64 length = indexed_address(index, scalar, scale, word, cursor);
        return AddressEncoding(prefix + length, 0, u8(15), NO_INDEX, u8(0));
    }
    AddressEncoding memory = indexed_memory(index, scalar, scale, word, cursor);
    memory.length += prefix;
    return memory;
}

extern "C" u64 emit_reference_address(Node reference, IndexedAddressHost host,
    cfn<MoveLocation,address,usize> ensures memory(read) local, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) requires nonzero(local) {
    ScalarValueHost values = host.values;
    View address_view = values.ir.inspect(reference);
    View storage = values.ir.inspect(address_view.left);
    bool indexed = address_view.first.count != 0u64;
    usize index = 0u64;
    if (indexed) { index = unsafe_position<usize>(address_view.first.data); }
    u32 word = u32(address_view.a);
    AddressEncoding place;
    if (storage.tag == 0) {
        // Addressable locals were assigned their real stack position during layout.
        if (storage.b != 0u64) { word += u32(local(values.context, storage.a).value); }
        if (indexed) {
            place = value_memory(u8(4), values.location(values.context, index), 1, word, output);
        } else {
            place = AddressEncoding(0u64, word, u8(4), u8(255), u8(0));
        }
    } else if (indexed) {
        place = indexed_operand(values.ir.inspect(storage.left), index, word, IndexedAddressMode::Memory, host, output);
    } else {
        place = indexed_operand(values.ir.inspect(storage.left), 0u64, 0, IndexedAddressMode::Base, host, output);
        place.offset = word;
    }
    // MoveLocation kind 2 is the memory operand of the shared x64 ABI.
    MoveLocation memory = MoveLocation(u64(place.offset), u8(2), place.base, place.index, place.shift);
    address cursor = unsafe_address(address_bits(output) + place.length);
    return place.length + effective_address(u8(0), memory, cursor);
}

MoveLocation indexed_move_location(AddressEncoding memory) on cpu {
    return MoveLocation(u64(memory.offset), MOVE_MEMORY, memory.base, memory.index, memory.shift);
}

u64 indexed_tile_read(View inst, View source, bool dynamic, IndexedAddressHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) {
    ScalarValueHost values = host.values;
    address context = values.context;
    ScalarEncoding scalar = values.scalar(context, inst.a);
    MoveLocation dst = values.location(context, inst.a);
    u64 start = address_bits(output);
    u64 prefix;
    MoveLocation low;
    if (dynamic) {
        if (scalar.width < 32) {
            IndexedTileWord row = host.tile_stream(context, source.a, 0);
            prefix = tile_index_base(values.location(context, source.b), values.scalar(context, source.b), row.offset, output);
            ScalarEncoding offset = values.scalar(context, inst.b);
            MoveEncoding moved = copy_scalar_move(move_gpr(u8(0)), values.location(context, inst.b),
                offset.width / 8, offset.kind == ScalarKind::Signed, unsafe_address(start + prefix));
            prefix += moved.length;
            prefix += tile_field_address(host.workgroup, u32(inst.c) + row.byte_offset, unsafe_address(start + prefix));
            low = MoveLocation(0u64, MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
        } else {
            AddressEncoding memory = indexed_operand(source, inst.b, u32(inst.c), IndexedAddressMode::Memory, host, output);
            prefix = memory.length;
            low = indexed_move_location(memory);
        }
    } else {
        prefix = tile_index_base(values.location(context, inst.c), values.scalar(context, inst.c), 0, output);
        IndexedTileWord word = host.tile_stream(context, inst.b, u32(inst.d));
        low = MoveLocation(u64(word.offset + word.byte_offset), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    }
    address cursor = unsafe_address(start + prefix);
    if (scalar.width == 64) {
        MoveLocation high = low;
        high.value = dynamic ? u64(u32(low.value) + host.workgroup * 4)
            : u64(host.tile_stream(context, inst.b, u32(inst.d) + 4).offset);
        return prefix + tile_read_split(dst, low, high, cursor);
    }
    return prefix + scalar_storage_read(dst, low, scalar, cursor);
}

u64 emit_indexed_storage(View inst, IndexedAddressHost host,
    cfn<MoveLocation,address,usize> ensures memory(read) local, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) requires nonzero(local) {
    ScalarValueHost values = host.values;
    address context = values.context;
    if (inst.tag == OP_POSITION_ADDRESS) {
        u64 prefix = emit_reference_address(inst.left, host, local, output);
        ScalarEncoding scalar = values.scalar(context, inst.a);
        MoveEncoding moved = copy_scalar_move(values.location(context, inst.a), move_gpr(u8(0)),
            scalar.width / 8, scalar.kind == ScalarKind::Signed, unsafe_address(address_bits(output) + prefix));
        return prefix + moved.length;
    }
    if (inst.tag == OP_TILE_LOAD) { return indexed_tile_read(inst, inst, false, host, output); }
    View source = values.ir.inspect(inst.left);
    if (inst.tag == OP_LOAD_INDEXED && source.tag == INDEXED_TILE) {
        return indexed_tile_read(inst, source, true, host, output);
    }
    bool read = inst.tag == OP_LOAD_INDEXED;
    usize offset = read ? inst.b : inst.a;
    usize reg = read ? inst.a : inst.b;
    AddressEncoding memory = indexed_operand(source, offset, u32(inst.c), IndexedAddressMode::Memory, host, output);
    MoveLocation operand = indexed_move_location(memory);
    MoveLocation location = values.location(context, reg);
    ScalarEncoding scalar = values.scalar(context, reg);
    address cursor = unsafe_address(address_bits(output) + memory.length);
    u64 length = read ? scalar_storage_read(location, operand, scalar, cursor)
        : write_scalar_move(operand, location, scalar.width / 8, cursor);
    return memory.length + length;
}

u64 emit_view_load(View load, ScalarValueHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar) {
    ViewElement view = indexed_view_element(load.first, load.b, host.ir.inspect(load.left), host);
    AddressEncoding memory = view_element_memory(view, u32(load.c), output);
    MoveLocation operand = MoveLocation(u64(memory.offset), MOVE_MEMORY, memory.base, memory.index, memory.shift);
    address cursor = unsafe_address(address_bits(output) + memory.length);
    return memory.length + scalar_storage_read(host.location(host.context, load.a), operand,
        host.scalar(host.context, load.a), cursor);
}

u64 emit_atomic(View value, IndexedAddressHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) {
    ScalarValueHost values = host.values;
    address context = values.context;
    View target = values.ir.inspect(value.left);
    View op = values.ir.inspect(value.right);
    AtomicOperation operation = AtomicOperation(op.tag);
    u32 word = u32(value.a);
    bool tile = target.tag == INDEXED_TILE;
    u64 prefix;
    u32 low_word = 0;
    bool indexed = value.second.count != 0u64;
    if (indexed) {
        usize offset = indexed_pair_word(value.second, 0u64);
        prefix = indexed_operand(target, offset, word, IndexedAddressMode::Address, host, output).length;
    } else if (tile) {
        prefix = tile_index_base(values.location(context, target.b), values.scalar(context, target.b), 0, output);
        low_word = host.tile_stream(context, target.a, word).offset;
    } else {
        prefix = indexed_operand(target, 0u64, 0, IndexedAddressMode::Base, host, output).length;
        low_word = word;
    }

    MoveLocation low = MoveLocation(u64(low_word), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    MoveLocation high = low;
    AtomicStorage storage = AtomicStorage::Shared;
    if (tile) {
        storage = AtomicStorage::Private;
        if (value.b == 8u64) {
            storage = AtomicStorage::Split;
            u32 high_stream = host.tile_stream(context, target.a, word + 4).offset;
            if (indexed) { high_stream -= host.tile_stream(context, target.a, word).offset; }
            high.value = u64(high_stream);
        }
    }

    MoveLocation first = MoveLocation(0u64, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
    MoveLocation second = first;
    if (operation != AtomicOperation::Load) { first = values.location(context, op.a); }
    if (operation == AtomicOperation::CompareExchange) { second = values.location(context, op.b); }
    ScalarEncoding scalar = ScalarEncoding(value.c != 0u64 ? ScalarKind::Signed : ScalarKind::Unsigned, u32(value.b) * 8);
    u64 start = address_bits(output);
    u64 cursor = start + prefix;
    AtomicSequence input = AtomicSequence(low, high, first, second, scalar, operation, storage);
    cursor = storage == AtomicStorage::Split ? atomic_split_at(cursor, input) : atomic_contiguous_at(cursor, input);
    if (value.first.count != 0u64) {
        usize reg = indexed_pair_word(value.first, 0u64);
        ScalarEncoding result = values.scalar(context, reg);
        MoveEncoding stored = copy_scalar_move(values.location(context, reg), move_gpr(u8(0)),
            result.width / 8, result.kind == ScalarKind::Signed, unsafe_address(cursor));
        cursor += stored.length;
    }
    return cursor - start;
}

u64 emit_append(View value, ScalarValueHost host, u32 buffer_bytes,
    u32 stride_field, address output) on cpu
    requires nonzero(host.location) requires nonzero(host.scalar) {
    u32 target = u32(value.c) * buffer_bytes;
    // R13 contains the runtime's two header words, then one length per binding.
    u32 capacity = (2 + u32(value.c)) * 4;
    AppendBranch reservation = reserve_append(u32(value.b) * buffer_bytes, u32(value.d), capacity, target, output);
    u64 start = address_bits(output);
    u64 cursor = buffer_index_at(start + reservation.length, target + stride_field, u8(0), u8(15));
    u64 index = 0u64;
    while (index < u64(value.first.count)) {
        usize reg = indexed_pair_word(value.first, index);
        u32 word = unsafe_position<u32>(unsafe_address(address_bits(value.second.data) + index * u64(value.second.stride)));
        MoveLocation memory = MoveLocation(u64(word), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
        cursor += write_scalar_move(memory, host.location(host.context, reg),
            host.scalar(host.context, reg).width / 8, unsafe_address(cursor));
        index += 1u64;
    }
    AppendBranch result = append_result(unsafe_address(cursor));
    patch_relative(unsafe_address(start + reservation.full), reservation.full, cursor - start + result.full);
    cursor += result.length;
    ScalarEncoding flag = host.scalar(host.context, value.a);
    MoveEncoding stored = copy_scalar_move(host.location(host.context, value.a), move_gpr(u8(0)),
        flag.width / 8, flag.kind == ScalarKind::Signed, unsafe_address(cursor));
    return cursor + stored.length - start;
}

extern "C" CustomRangeProduct emit_custom_range(address emitter, CustomRangeInput input,
    ScalarBodyHost scalar, CustomRangeHost host) on cpu
    requires nonzero(host.frame.reserve) requires nonzero(host.frame.publish)
    requires nonzero(host.summary_component)
    requires nonzero(scalar.addresses.values.ir.inspect) requires nonzero(scalar.addresses.values.location)
    requires nonzero(scalar.addresses.values.scalar) requires nonzero(scalar.addresses.tile_stream)
    requires nonzero(scalar.local) requires nonzero(scalar.reserve) requires nonzero(scalar.commit)
    requires nonzero(scalar.literal) requires nonzero(scalar.math_symbol)
    requires nonzero(scalar.mark) requires nonzero(scalar.publish) requires nonzero(scalar.emit)
    requires nonzero(scalar.control.emit) requires nonzero(scalar.control.load) requires nonzero(scalar.control.buffer)
    requires nonzero(scalar.control.commit) {
    RangePrologue entry = range_prologue(input.frame, input.saved, emitter, host.frame);
    u32 stack_bytes = entry.stack_bytes;
    u64 exits = 0u64;
    ControlTargets targets = ControlTargets(address_of(exits), address_of(exits), address(), 0u64);
    ScalarBodyHost dispatch = scalar;
    address context = address_of(dispatch);
    u32 status = emit_scalar_body(context, emitter, input.initialize, targets, scalar);
    if (status != 0) { return CustomRangeProduct(stack_bytes, status); }

    NativeCodeBuffer buffer = scalar.reserve(emitter, 16u64);
    u64 begin = buffer.end;
    address output = unsafe_address(address_bits(buffer.image) + begin);
    u64 bounds_end = begin + range_bounds(false, output);
    scalar.commit(emitter, bounds_end);

    u64 breaks = 0u64;
    ControlTargets body_targets = targets;
    if (input.break_on_head != 0) {
        body_targets = ControlTargets(targets.function, targets.block, address_of(breaks), begin);
    }
    status = emit_scalar_body(context, emitter, input.body, body_targets, scalar);
    if (status != 0) { return CustomRangeProduct(stack_bytes, status); }
    buffer = scalar.reserve(emitter, 8u64);
    RangeEnd range = finish_range(buffer.image, buffer.end, begin, bounds_end - 4u64, 0u64, u8(1));
    scalar.commit(emitter, range.end);
    patch_control_branches(buffer.image, range.end, breaks);

    status = emit_scalar_body(context, emitter, input.finish, targets, scalar);
    if (status != 0) { return CustomRangeProduct(stack_bytes, status); }
    if (input.publish != 0) {
        buffer = scalar.reserve(emitter, 16u64 + 24u64 * input.summary_count);
        output = unsafe_address(address_bits(buffer.image) + buffer.end);
        u64 bytes = abi_value(stack_bytes, input.summary_count, AbiMemoryMode::RangeSummary,
            input.summary, host.summary_component, output);
        scalar.commit(emitter, buffer.end + bytes);
    }
    finish_range_frame(input.frame, entry, exits, emitter, scalar.reserve, scalar.commit);
    return CustomRangeProduct(stack_bytes, 0);
}
