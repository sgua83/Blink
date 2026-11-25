module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;
import bootstrap.scalar_fold;

enum ResultLocation:u32 { None=0; Integer=1; Float=2; Indirect=3; };
// Offsets are relative to RSP after the prologue. Indirect results use the saved address slot.
struct FunctionFrame { u64 bytes; u64 home; u64 result; u64 stack; ResultLocation result_location; };
struct FunctionReturn { u64 length; u64 value_length; u64 literal; u64 literal_field; };

u32 padded(u32 size, u32 alignment) on cpu {
    return (size + alignment - 1) & ~(alignment - 1);
}
bool indirect(u32 size, u32 alignment) on cpu {
    u32 bytes = padded(size, alignment);
    return bytes != 0 && bytes != 1 && bytes != 2 && bytes != 4 && bytes != 8;
}
bool has_slot(AbiType value) on cpu {
    return value.value == 0 || value.size != 0;
}

u64 slot_home(u64 first_slot, u64 position) on cpu {
    return (first_slot + position) * 8u64;
}

ResultLocation result_location(AbiType result, u32 flags) on cpu {
    if (result.size == 0) { return ResultLocation::None; }
    if ((flags & 1) != 0 || indirect(result.size, result.alignment)) {
        return ResultLocation::Indirect;
    }
    return result.float_size != 0 ? ResultLocation::Float : ResultLocation::Integer;
}

FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu {
    ResultLocation location = result_location(result, flags);
    bool hidden = location == ResultLocation::Indirect;
    bool scratch = (flags & 2) != 0;
    u64 result_at = 0u64;
    if (hidden) { result_at = (data_end + 7u64) & ~7u64; }
    u64 data_bytes = data_end + (hidden ? 8u64 : 0u64);
    u64 bytes = (data_bytes + 15u64) & ~15u64;
    if (!scratch && bytes != 0u64) { bytes += 8u64; }
    u64 home = bytes + (scratch ? 16u64 : 8u64);
    return FunctionFrame(bytes, home, result_at, bytes + (scratch ? 8u64 : 0u64), location);
}

struct CallStack { u64 homes; u64 target; u64 bytes; };

u64 copy_start(u64 end) on cpu { return (end + 15u64) & ~15u64; }

CallStack call_stack(address context, u64 count, AbiType result, u64 first_slot, u32 flags,
    cfn<AbiType,address,u64> argument, bool count_payloads) on cpu requires nonzero(argument) {
    u64 slot = first_slot;
    u64 end = first_slot != 0u64 ? u64(padded(result.size, result.alignment)) : 0u64;
    u64 index = 0u64;
    while (index < count) {
        AbiType value = argument(context, index);
        if (has_slot(value)) {
            slot += 1u64;
            if (count_payloads && value.value != 0 && indirect(value.size, value.alignment)) {
                end = copy_start(end) + u64(padded(value.size, value.alignment));
            }
        }
        index += 1u64;
    }
    // Windows x64 always provides four home slots.
    u64 homes = copy_start(max(4u64, slot) * 8u64);
    end += homes;
    u64 target = 0u64;
    if ((flags & 2) != 0) {
        target = (end + 7u64) & ~7u64;
        end = target + 8u64;
    }
    return CallStack(homes, target, copy_start(end));
}

extern "C" u64 outgoing_bytes(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument) {
    u64 first_slot = result_location(result, flags) == ResultLocation::Indirect ? 1u64 : 0u64;
    return call_stack(context, count, result, first_slot, flags, argument, true).bytes;
}

Node call_argument(Span arguments, u64 index) on cpu {
    address value = unsafe_address(address_bits(arguments.data) + index * u64(arguments.stride));
    return Node(value, N_ARGUMENT);
}

MoveLocation call_home(u64 offset) on cpu {
    // MoveLocation kind 2 is the shared x64 memory operand, without an index.
    return MoveLocation(offset, u8(2), u8(4), u8(255), u8(0));
}

u64 call_value_at(u64 cursor, u32 home, CallValueLoan value, AbiMemoryMode mode,
    cfn<AbiValueComponent,address,u64> component) on cpu requires nonzero(component) {
    u64 count = min(u64(value.components.count), u64(value.values.count));
    return cursor + abi_value(home, count, mode, address_of(value), component, unsafe_address(cursor));
}

extern "C" u64 emit_call(address context, CallInput input, CallHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect)
    requires nonzero(host.addresses.values.location) requires nonzero(host.addresses.values.scalar)
    requires nonzero(host.addresses.tile_stream) requires nonzero(host.local)
    requires nonzero(host.argument) requires nonzero(host.operand)
    requires nonzero(host.component) requires nonzero(host.reserve) requires nonzero(host.commit) {
    ScalarValueHost values = host.addresses.values;
    View call = values.ir.inspect(input.source);
    u64 count = u64(call.first.count);
    ResultLocation location = result_location(input.result, input.flags);
    bool hidden = location == ResultLocation::Indirect;
    bool indirect_target = (input.flags & 2) != 0;
    u64 first_slot = hidden ? 1u64 : 0u64;
    CallStack frame = CallStack(32u64, 0u64, 32u64);
    if (indirect_target || first_slot + count > 4u64) {
        frame = call_stack(context, count, input.result, first_slot, input.flags, host.argument, indirect_target);
    }

    MoveLocation rax = MoveLocation(0u64, u8(0), u8(0), u8(255), u8(0));
    if (indirect_target || hidden) {
        NativeCodeBuffer prefix = host.reserve(context, 48u64);
        u64 start = address_bits(prefix.image) + prefix.end;
        u64 cursor = start;
        if (indirect_target) {
            View target = values.ir.inspect(call.left);
            MoveEncoding saved = copy_scalar_move(call_home(frame.target), values.location(values.context, target.a),
                8, false, unsafe_address(cursor));
            cursor += saved.length;
        }
        if (hidden) { cursor += abi_stack_pointer(u32(frame.homes), 0, unsafe_address(cursor)); }
        host.commit(context, prefix.end + cursor - start);
    }

    array<u32,4> register_widths = array<u32,4>(0, 0, 0, 0);
    u64 slot = first_slot;
    u64 end = frame.homes + (hidden ? u64(padded(input.result.size, input.result.alignment)) : 0u64);
    u64 index = 0u64;
    while (index < count) {
        CallOperand operand = host.operand(context, index);
        AbiType value = operand.abi;
        if (has_slot(value)) {
            u64 home = slot_home(0u64, slot);
            // A reference uses at most 168 address bytes and a 24-byte move.
            // A value needs at most two prefixes and 24 bytes per component.
            u64 capacity = value.value == 0 ? 192u64 : 32u64 + 24u64 * u64(operand.value.values.count);
            NativeCodeBuffer buffer = host.reserve(context, capacity);
            u64 start = address_bits(buffer.image) + buffer.end;
            u64 cursor = start;
            if (value.value == 0) {
                View arg = values.ir.inspect(call_argument(call.first, index));
                cursor += emit_reference_address(arg.left, host.addresses, host.local, unsafe_address(cursor));
                MoveEncoding stored = copy_scalar_move(call_home(home), rax, 8, false, unsafe_address(cursor));
                cursor += stored.length;
            } else if (indirect(value.size, value.alignment)) {
                u64 payload = copy_start(end);
                end = payload + u64(padded(value.size, value.alignment));
                cursor = call_value_at(cursor, u32(payload), operand.value, AbiMemoryMode::StackStore, host.component);
                cursor += abi_stack_pointer(u32(payload), u32(home), unsafe_address(cursor));
            } else {
                cursor = call_value_at(cursor, u32(home), operand.value, AbiMemoryMode::StackStore, host.component);
            }
            host.commit(context, buffer.end + cursor - start);
            if (slot < 4u64) { register_widths[u32(slot)] = value.float_size; }
            slot += 1u64;
        }
        index += 1u64;
    }

    NativeCodeBuffer terminal = host.reserve(context, 192u64 + 48u64 * u64(input.values.values.count));
    u64 start = address_bits(terminal.image) + terminal.end;
    u64 cursor = start;
    for (reg in 0..4) {
        if (u64(reg) == slot) { break; }
        cursor += argument_register(u8(reg), register_widths[reg], false, unsafe_address(cursor));
    }

    u64 relocation = 0u64;
    if (indirect_target) {
        cursor += call_slot(u32(frame.target), unsafe_address(cursor));
    } else {
        SymbolEncoding symbol = symbol_target(true, unsafe_address(cursor));
        relocation = terminal.end + (cursor - start) + symbol.field;
        cursor += symbol.length;
    }

    bool fallible = (input.flags & 1) != 0;
    u64 failed = 0u64;
    if (fallible) {
        ScalarEncoding scalar = values.scalar(values.context, input.error);
        MoveEncoding stored = copy_scalar_move(values.location(values.context, input.error), rax,
            scalar.width / 8, scalar.kind == ScalarKind::Signed, unsafe_address(cursor));
        cursor += stored.length;
        cursor += encode_branch(u8(0x85), 64, unsafe_address(cursor));
        failed = cursor - 4u64;
    }
    if (location == ResultLocation::Indirect) {
        cursor = call_value_at(cursor, u32(frame.homes), input.values, AbiMemoryMode::StackLoad, host.component);
    } else if (location == ResultLocation::Float) {
        usize reg = unsafe_position<usize>(input.values.values.data);
        ScalarEncoding scalar = values.scalar(values.context, reg);
        MoveLocation xmm0 = MoveLocation(0u64, u8(1), u8(0), u8(255), u8(0));
        MoveEncoding returned = copy_scalar_move(values.location(values.context, reg), xmm0,
            scalar.width / 8, scalar.kind == ScalarKind::Signed, unsafe_address(cursor));
        cursor += returned.length;
    } else if (location == ResultLocation::Integer) {
        cursor = call_value_at(cursor, u32(frame.homes), input.values, AbiMemoryMode::UnpackIntegerResult, host.component);
    }
    if (fallible) { patch_relative(unsafe_address(failed), failed - start, cursor - start); }
    host.commit(context, terminal.end + cursor - start);
    return relocation;
}

u64 move_function_parameter(address context, u64 index, u64 slot, u64 home, inout u64 first,
    FunctionParameter parameter, FunctionHost host, u64 image, u64 start) on cpu
    requires nonzero(host.loan) requires nonzero(host.component) requires nonzero(host.publish) {
    AbiType abi = parameter.abi;
    AbiMemoryMode mode = abi.value != 0 && indirect(abi.size, abi.alignment)
        ? AbiMemoryMode::IndirectParameter : AbiMemoryMode::StackLoad;
    CallValueLoan value = host.loan(context, index, first, parameter.values);
    u64 cursor = call_value_at(image + start, u32(home + slot_home(0u64, slot)), value, mode, host.component) - image;
    host.publish(context, FunctionRange(start, cursor, index, slot, FunctionRangeKind::Input));
    first += parameter.values;
    return cursor;
}

FunctionPrologue function_prologue(address context, u64 count, FunctionFrame frame,
    u32 flags, FrameOutput output, FunctionHost host) on cpu
    requires nonzero(host.parameter) requires nonzero(host.loan)
    requires nonzero(host.component) requires nonzero(host.publish) {
    u64 image = address_bits(output.code);
    u64 cursor = 0u64;
    bool hidden = frame.result_location == ResultLocation::Indirect;
    if (hidden) {
        cursor = argument_register(u8(0), 0, true, output.code);
        host.publish(context, FunctionRange(0u64, cursor, 0u64, 0u64, FunctionRangeKind::HiddenHome));
    }
    u64 first_slot = hidden ? 1u64 : 0u64;
    HomeParameter empty = HomeParameter(0u64, FunctionParameter(AbiType(0,1,0,0),0u64));
    array<HomeParameter,4> homes = array<HomeParameter,4>(empty,empty,empty,empty);
    u64 slot = first_slot;
    u64 index = 0u64;
    while (index < count && slot < 4u64) {
        FunctionParameter parameter = host.parameter(context,index);
        if (has_slot(parameter.abi)) {
            homes[u32(slot)] = HomeParameter(index,parameter);
            u64 end = cursor + argument_register(u8(slot), parameter.abi.float_size, true, unsafe_address(image + cursor));
            host.publish(context, FunctionRange(cursor, end, index, slot, FunctionRangeKind::Home));
            cursor = end;
            slot += 1u64;
        }
        index += 1u64;
    }
    FramePrologue saved = function_frame_prologue(u32(frame.bytes), (flags & 2) != 0, cursor, output);
    if (saved.push_end != cursor) {
        host.publish(context, FunctionRange(cursor, saved.push_end, 0u64, 0u64, FunctionRangeKind::ScratchSave));
    }
    host.publish(context, FunctionRange(saved.push_end, saved.allocate_end, 0u64, frame.bytes, FunctionRangeKind::FrameAllocate));
    host.publish(context, FunctionRange(saved.allocate_end, saved.end, 0u64, 0u64, FunctionRangeKind::NonvolatileSave));
    cursor = saved.end;
    if (hidden) {
        MoveEncoding address_save = copy_scalar_move(call_home(frame.result), call_home(frame.home),
            8, false, unsafe_address(image + cursor));
        u64 end = cursor + address_save.length;
        host.publish(context, FunctionRange(cursor, end, 0u64, frame.result, FunctionRangeKind::ResultAddress));
        cursor = end;
    }
    u64 value = 0u64;
    for (home_slot in u32(first_slot)..4) {
        if (u64(home_slot) == slot) { break; }
        HomeParameter row = homes[home_slot];
        cursor = move_function_parameter(context,row.index,u64(home_slot),frame.home,value,row.parameter,host,image,cursor);
    }
    while (index < count) {
        FunctionParameter parameter = host.parameter(context,index);
        if (has_slot(parameter.abi)) {
            cursor = move_function_parameter(context,index,slot,frame.home,value,parameter,host,image,cursor);
            slot += 1u64;
        }
        index += 1u64;
    }
    u8 prolog = saved.operations != 0u64 ? u8(saved.end) : u8(0);
    return FunctionPrologue(cursor, saved.probe_field, saved.operations, prolog);
}

FunctionReturn function_result(FunctionFrame frame, u32 flags, CallValueLoan value,
    cfn<AbiValueComponent,address,u64> component, address output)
    on cpu requires nonzero(component) {
    u64 start = address_bits(output);
    u64 cursor = start;
    u64 literal = 0u64;
    u64 field = 0u64;
    bool fallible = (flags & 1) != 0;
    if (frame.result_location == ResultLocation::Indirect) {
        AbiMemoryMode mode = fallible ? AbiMemoryMode::FallibleResult : AbiMemoryMode::IndirectResult;
        cursor = call_value_at(start, u32(frame.result), value, mode, component);
    } else if (frame.result_location == ResultLocation::Float) {
        AbiValueComponent scalar = component(address_of(value), 0u64);
        MoveLocation xmm0 = MoveLocation(0u64, u8(1), u8(0), u8(255), u8(0));
        MoveEncoding copy = copy_scalar_move(xmm0, scalar.location, scalar.scalar.width / 8, false, output);
        cursor += copy.length;
        literal = scalar.location.value;
        field = copy.literal_field;
    } else if (frame.result_location == ResultLocation::Integer) {
        cursor = call_value_at(start, 0, value, AbiMemoryMode::PackIntegerResult, component);
    }
    u64 value_length = cursor - start;
    if (fallible) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(0x31);
        unsafe_position<u8>(unsafe_address(cursor + 1u64)) = u8(0xc0);
        cursor += 2u64;
    }
    return FunctionReturn(cursor - start, value_length, literal, field);
}

extern "C" FunctionCode emit_function_code(address context, FunctionCodeInput input, FunctionCodeHost host) on cpu
    requires nonzero(host.abi.parameter) requires nonzero(host.abi.loan)
    requires nonzero(host.abi.component) requires nonzero(host.abi.publish)
    requires nonzero(host.entry) requires nonzero(host.body) requires nonzero(host.reserve)
    requires nonzero(host.result) requires nonzero(host.commit) requires nonzero(host.literal)
    requires nonzero(host.exit_buffer) requires nonzero(host.complete) {
    FunctionFrame frame = function_frame(input.data_end, input.result, input.flags);
    if (frame.bytes > 2147483647u64) { return FunctionCode(0u64, 1); }

    u64 entry_bytes = 2u64 + 17u64 + 9u64 * input.saved + 4u64 * 9u64 + 24u64
        + 16u64 * input.parameters + 24u64 * input.inputs;
    FrameOutput entry = host.entry(context, entry_bytes, 2u64 + input.saved);
    FunctionPrologue prologue = function_prologue(context, input.parameters, frame, input.flags, entry, host.abi);
    if (host.body(context, prologue) == 0) { return FunctionCode(0u64, 0); }

    u64 result_fields = input.result_fields < input.outputs ? input.result_fields : input.outputs;
    NativeCodeBuffer buffer = host.reserve(context, 26u64 + 48u64 * result_fields);
    FunctionReturn result = function_result(frame, input.flags, host.result(context), host.abi.component,
        unsafe_address(address_bits(buffer.image) + buffer.end));
    u64 value_end = buffer.end + result.value_length;
    u64 result_end = buffer.end + result.length;
    host.commit(context, result_end);
    if (frame.result_location == ResultLocation::Integer) {
        host.abi.publish(context, FunctionRange(buffer.end, value_end, 0u64, 0u64, FunctionRangeKind::IntegerResult));
    } else if (frame.result_location == ResultLocation::Float) {
        host.abi.publish(context, FunctionRange(buffer.end, value_end, 0u64, 0u64, FunctionRangeKind::FloatResult));
    } else if (frame.result_location == ResultLocation::Indirect) {
        host.abi.publish(context, FunctionRange(buffer.end, value_end, 0u64, frame.result, FunctionRangeKind::IndirectResult));
    }
    host.literal(context, buffer.end, result.literal, result.literal_field);
    if ((input.flags & 1) != 0) {
        host.abi.publish(context, FunctionRange(value_end, result_end, 0u64, 0u64, FunctionRangeKind::SuccessStatus));
    }

    FunctionExitBuffer exiting = host.exit_buffer(context, 10u64 + 9u64 * input.saved);
    FunctionEpilogue tail = function_epilogue(u32(frame.bytes), (input.flags & 2) >> 1,
        exiting.saved, exiting.saved_count, exiting.code.image, exiting.code.end, exiting.exits, exiting.exit_count);
    host.complete(context, tail.end);
    host.abi.publish(context, FunctionRange(exiting.code.end, tail.saved_end, 0u64, 0u64, FunctionRangeKind::NonvolatileRestore));
    if (tail.frame_end != tail.saved_end) {
        host.abi.publish(context, FunctionRange(tail.saved_end, tail.frame_end, 0u64, frame.bytes, FunctionRangeKind::FrameRelease));
    }
    if (tail.scratch_end != tail.frame_end) {
        host.abi.publish(context, FunctionRange(tail.frame_end, tail.scratch_end, 0u64, 0u64, FunctionRangeKind::ScratchRestore));
    }
    host.abi.publish(context, FunctionRange(tail.scratch_end, tail.end, 0u64, 0u64, FunctionRangeKind::FunctionReturn));
    return FunctionCode(frame.stack, 0);
}
