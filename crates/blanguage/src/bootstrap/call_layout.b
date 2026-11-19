module bootstrap.call_layout;

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

extern "C" void emit_call(address context, u64 count, AbiType result, u32 flags, CallHost host)
    on cpu requires nonzero(host.argument) requires nonzero(host.place) requires nonzero(host.action) {
    ResultLocation location = result_location(result, flags);
    bool hidden = location == ResultLocation::Indirect;
    bool indirect_target = (flags & 2) != 0;
    u64 first_slot = hidden ? 1u64 : 0u64;
    CallStack frame = CallStack(32u64, 0u64, 32u64);
    if (indirect_target || first_slot + count > 4u64) {
        frame = call_stack(context, count, result, first_slot, flags, host.argument, indirect_target);
    }
    if (indirect_target) { host.action(context, CallAction::SaveTarget, frame.target, 0); }
    if (hidden) { host.action(context, CallAction::ResultAddress, frame.homes, 0); }

    // The register kinds are used after all real argument homes have been written.
    array<u32,4> register_widths = array<u32,4>(0, 0, 0, 0);
    u64 slot = first_slot;
    u64 end = frame.homes + (hidden ? u64(padded(result.size, result.alignment)) : 0u64);
    u64 index = 0u64;
    while (index < count) {
        AbiType value = host.argument(context, index);
        if (has_slot(value)) {
            u64 home = slot_home(0u64, slot);
            u64 payload = home;
            bool copied = value.value != 0 && indirect(value.size, value.alignment);
            if (copied) {
                payload = copy_start(end);
                end = payload + u64(padded(value.size, value.alignment));
            }
            if (!host.place(context, CallPlace(index, home, payload, copied ? 1 : 0))) { return; }
            if (slot < 4u64) { register_widths[u32(slot)] = value.float_size; }
            slot += 1u64;
        }
        index += 1u64;
    }
    for (reg in 0..4) {
        if (u64(reg) == slot) { break; }
        host.action(context, CallAction::LoadRegister, u64(reg), register_widths[reg]);
    }
    host.action(context, CallAction::Invoke, frame.target, 0);

    bool fallible = (flags & 1) != 0;
    u64 failed = 0u64;
    if (fallible) { failed = host.action(context, CallAction::CaptureError, 0u64, 0); }
    if (location == ResultLocation::Indirect) {
        host.action(context, CallAction::ReadIndirect, frame.homes, 0);
    } else if (location == ResultLocation::Float) {
        host.action(context, CallAction::ReadFloat, 0u64, 0);
    } else if (location == ResultLocation::Integer) {
        host.action(context, CallAction::ReadInteger, 0u64, 0);
    }
    if (fallible) { host.action(context, CallAction::CloseError, failed, 0); }
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
