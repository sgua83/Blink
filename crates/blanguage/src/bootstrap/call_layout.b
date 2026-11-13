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

extern "C" CallPlan plan(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument, address positions) on cpu requires nonzero(argument) {
    ResultLocation location = result_location(result, flags);
    bool hidden = location == ResultLocation::Indirect;
    u64 first_slot = hidden ? 1u64 : 0u64;
    // Payloads are measured from the end of the 16-byte-aligned home area.
    u64 bytes = hidden ? u64(padded(result.size, result.alignment)) : 0u64;
    bool emit_positions = address_bits(positions) != 0u64;
    u64 arguments = 0u64;
    u64 index = 0u64;
    while (index < count) {
        AbiType value = argument(context, index);
        if (has_slot(value)) {
            u64 home = slot_home(first_slot, arguments);
            u64 payload = home;
            bool copied = value.value != 0 && indirect(value.size, value.alignment);
            if (copied) {
                payload = (bytes + 15u64) & ~15u64;
                bytes = payload + u64(padded(value.size, value.alignment));
            }
            if (emit_positions) {
                address row = unsafe_address(address_bits(positions) + arguments * size_of<ArgumentSlot>());
                unsafe_position<ArgumentSlot>(row) = ArgumentSlot(
                    index, home, payload, copied ? 1 : 0, value.float_size);
            }
            arguments += 1u64;
        }
        index += 1u64;
    }
    // Windows x64 owns four home slots even when fewer arguments are present.
    u64 slots = max(4u64, first_slot + arguments);
    u64 homes = (slots * 8u64 + 15u64) & ~15u64;
    if (emit_positions) {
        u64 position = 0u64;
        while (position < arguments) {
            address row = unsafe_address(address_bits(positions) + position * size_of<ArgumentSlot>());
            if (unsafe_position<ArgumentSlot>(row).indirect != 0) {
                unsafe_position<ArgumentSlot>(row).payload += homes;
            }
            position += 1u64;
        }
    }
    u64 result_at = hidden ? homes : 0u64;
    bytes += homes;
    u64 target = 0u64;
    if ((flags & 2) != 0) {
        bytes = (bytes + 7u64) & ~7u64;
        target = bytes;
        bytes += 8u64;
    }
    return CallPlan((bytes + 15u64) & ~15u64, result_at, target, arguments, location);
}

extern "C" u64 parameter_slots(address context, u64 count, u64 first_slot,
    cfn<AbiType,address,u64> parameter, address positions) on cpu requires nonzero(parameter) {
    u64 position = 0u64;
    u64 index = 0u64;
    while (index < count) {
        AbiType value = parameter(context, index);
        if (has_slot(value)) {
            bool indirect_value = value.value != 0 && indirect(value.size, value.alignment);
            address row = unsafe_address(address_bits(positions) + position * size_of<ParameterSlot>());
            unsafe_position<ParameterSlot>(row) = ParameterSlot(
                index, slot_home(first_slot, position), indirect_value ? 1 : 0, value.float_size);
            position += 1u64;
        }
        index += 1u64;
    }
    return position;
}
