module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;
import bootstrap.scalar_fold;

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

extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu {
    ResultLocation location = result_location(result, flags);
    bool hidden = location == ResultLocation::Indirect;
    bool scratch = (flags & 2) != 0;
    u64 result_at = 0u64;
    if (hidden) { result_at = (data_end + 7u64) & ~7u64; }
    u64 data_bytes = data_end + (hidden ? 8u64 : 0u64);
    u64 bytes = (data_bytes + 15u64) & ~15u64;
    if (!scratch && bytes != 0u64) { bytes += 8u64; }
    u64 home = bytes + (scratch ? 16u64 : 8u64);
    return FunctionFrame(bytes, home, result_at, location);
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

extern "C" SymbolEncoding emit_call(address context, CallInput input, CallHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect)
    requires nonzero(host.addresses.values.location) requires nonzero(host.addresses.values.scalar)
    requires nonzero(host.addresses.tile_stream) requires nonzero(host.local)
    requires nonzero(host.argument) requires nonzero(host.loan)
    requires nonzero(host.component) requires nonzero(host.reserve) {
    ScalarValueHost values = host.addresses.values;
    View call = values.ir.inspect(input.source);
    View target = values.ir.inspect(call.left);
    u64 count = u64(call.first.count);
    ResultLocation location = result_location(input.result, input.flags);
    bool hidden = location == ResultLocation::Indirect;
    bool indirect_target = target.tag == 2;
    u64 first_slot = hidden ? 1u64 : 0u64;
    CallStack frame = CallStack(32u64, 0u64, 32u64);
    if (indirect_target || first_slot + count > 4u64) {
        frame = call_stack(context, count, input.result, first_slot, input.flags, host.argument, indirect_target);
    }

    // A reference can use 168 address bytes plus a 24-byte move. Values
    // need at most two 16-byte prefixes plus 24 bytes per component.
    u64 capacity = 192u64 + 192u64 * count + 48u64 * u64(input.values.values.count);
    u64 index = 0u64;
    while (index < count) {
        View arg = values.ir.inspect(call_argument(call.first, index));
        capacity += 24u64 * u64(arg.first.count);
        index += 1u64;
    }
    u64 start = address_bits(host.reserve(context, capacity));
    u64 cursor = start;
    MoveLocation rax = MoveLocation(0u64, u8(0), u8(0), u8(255), u8(0));
    if (indirect_target) {
        MoveEncoding saved = copy_scalar_move(call_home(frame.target), values.location(values.context, target.a),
            8, false, unsafe_address(cursor));
        cursor += saved.length;
    }
    if (hidden) { cursor += abi_stack_pointer(u32(frame.homes), 0, unsafe_address(cursor)); }
    array<u32,4> register_widths = array<u32,4>(0, 0, 0, 0);
    u64 slot = first_slot;
    u64 end = frame.homes + (hidden ? u64(padded(input.result.size, input.result.alignment)) : 0u64);
    index = 0u64;
    while (index < count) {
        AbiType value = host.argument(context, index);
        if (has_slot(value)) {
            u64 home = slot_home(0u64, slot);
            if (value.value == 0) {
                View arg = values.ir.inspect(call_argument(call.first, index));
                cursor += emit_reference_address(arg.left, host.addresses, host.local, unsafe_address(cursor));
                MoveEncoding stored = copy_scalar_move(call_home(home), rax, 8, false, unsafe_address(cursor));
                cursor += stored.length;
            } else {
                CallValueLoan loan = host.loan(context, index);
                if (indirect(value.size, value.alignment)) {
                    u64 payload = copy_start(end);
                    end = payload + u64(padded(value.size, value.alignment));
                    cursor = call_value_at(cursor, u32(payload), loan, AbiMemoryMode::StackStore, host.component);
                    cursor += abi_stack_pointer(u32(payload), u32(home), unsafe_address(cursor));
                } else {
                    cursor = call_value_at(cursor, u32(home), loan, AbiMemoryMode::StackStore, host.component);
                }
            }
            if (slot < 4u64) { register_widths[u32(slot)] = value.float_size; }
            slot += 1u64;
        }
        index += 1u64;
    }
    for (reg in 0..4) {
        if (u64(reg) == slot) { break; }
        cursor += argument_register(u8(reg), register_widths[reg], false, unsafe_address(cursor));
    }

    u64 relocation = 0u64;
    if (indirect_target) {
        cursor += call_slot(u32(frame.target), unsafe_address(cursor));
    } else {
        SymbolEncoding symbol = symbol_target(true, unsafe_address(cursor));
        relocation = cursor - start + symbol.field;
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
    return SymbolEncoding(cursor - start, relocation);
}

u64 move_function_parameter(address context, u64 index, u64 slot, u64 home, u64 first,
    FunctionParameter parameter, FunctionHost host) on cpu requires nonzero(host.move_parameter) {
    AbiType abi = parameter.abi;
    ParameterMoveKind kind = abi.value != 0 && indirect(abi.size, abi.alignment)
        ? ParameterMoveKind::Indirect : ParameterMoveKind::Direct;
    host.move_parameter(context, ParameterMove(
        index, slot, home + slot_home(0u64, slot), first, parameter.values, 0, kind));
    return first + parameter.values;
}

extern "C" void function_prologue(address context, u64 count, FunctionFrame frame, u32 flags, FunctionHost host)
    on cpu requires nonzero(host.parameter) requires nonzero(host.move_parameter) requires nonzero(host.action) {
    bool hidden = frame.result_location == ResultLocation::Indirect;
    if (hidden) { host.action(context, FunctionAction::HiddenResultHome, 0u64, 0u64); }
    u64 first_slot = hidden ? 1u64 : 0u64;
    HomeParameter empty = HomeParameter(0u64, FunctionParameter(AbiType(0,1,0,0),0u64));
    array<HomeParameter,4> homes = array<HomeParameter,4>(empty,empty,empty,empty);
    u64 slot = first_slot;
    u64 index = 0u64;
    while (index < count && slot < 4u64) {
        FunctionParameter parameter = host.parameter(context,index);
        if (has_slot(parameter.abi)) {
            homes[u32(slot)] = HomeParameter(index,parameter);
            host.move_parameter(context, ParameterMove(
                index, slot, 0u64, 0u64, 0u64, parameter.abi.float_size, ParameterMoveKind::Home));
            slot += 1u64;
        }
        index += 1u64;
    }
    if ((flags & 2) != 0) { host.action(context, FunctionAction::ScratchSave, 15u64, 0u64); }
    host.action(context, FunctionAction::FrameAllocate, frame.bytes, 0u64);
    host.action(context, FunctionAction::NonvolatileSave, 0u64, 0u64);
    if (hidden) { host.action(context, FunctionAction::ResultAddressSave, frame.result, frame.home); }
    u64 value = 0u64;
    for (home_slot in u32(first_slot)..4) {
        if (u64(home_slot) == slot) { break; }
        HomeParameter row = homes[home_slot];
        value = move_function_parameter(context,row.index,u64(home_slot),frame.home,value,row.parameter,host);
    }
    while (index < count) {
        FunctionParameter parameter = host.parameter(context,index);
        if (has_slot(parameter.abi)) {
            value = move_function_parameter(context,index,slot,frame.home,value,parameter,host);
            slot += 1u64;
        }
        index += 1u64;
    }
}

extern "C" u64 function_epilogue(address context, FunctionFrame frame, u32 flags,
    cfn<void,address,FunctionAction,u64,u64> action) on cpu requires nonzero(action) {
    bool fallible = (flags & 1) != 0;
    if (frame.result_location == ResultLocation::Indirect) {
        action(context, fallible ? FunctionAction::FallibleResult : FunctionAction::IndirectResult, frame.result, 0u64);
    } else if (frame.result_location == ResultLocation::Float) {
        action(context, FunctionAction::FloatResult, 0u64, 0u64);
    } else if (frame.result_location == ResultLocation::Integer) {
        action(context, FunctionAction::IntegerResult, 0u64, 0u64);
    }
    if (fallible) { action(context, FunctionAction::FallibleSuccess, 0xc031u64, 2u64); }
    action(context, FunctionAction::CloseExits, 0u64, 0u64);
    action(context, FunctionAction::NonvolatileRestore, 0u64, 0u64);
    if (frame.bytes != 0u64) { action(context, FunctionAction::FrameRelease, frame.bytes, 0u64); }
    bool scratch = (flags & 2) != 0;
    if (scratch) { action(context, FunctionAction::ScratchRestore, 15u64, 0u64); }
    action(context, FunctionAction::Return, 0xc3u64, 1u64);
    return frame.bytes + (scratch ? 8u64 : 0u64);
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
        place = emit_indexed_operand(storage.left, index, word, true, host, output);
    } else {
        place = emit_indexed_operand(storage.left, 0u64, 0, false, host, output);
        place.offset = word;
    }
    // MoveLocation kind 2 is the memory operand of the shared x64 ABI.
    MoveLocation memory = MoveLocation(u64(place.offset), u8(2), place.base, place.index, place.shift);
    address cursor = unsafe_address(address_bits(output) + place.length);
    return place.length + effective_address(u8(0), memory, cursor);
}