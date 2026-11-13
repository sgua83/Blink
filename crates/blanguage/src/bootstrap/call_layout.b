module bootstrap.call_layout;

u32 padded(u32 size, u32 alignment) on cpu {
    return (size + alignment - 1) & ~(alignment - 1);
}
bool indirect(u32 size, u32 alignment) on cpu {
    u32 bytes = padded(size, alignment);
    return bytes != 0 && bytes != 1 && bytes != 2 && bytes != 4 && bytes != 8;
}
bool has_slot(AbiType value) on cpu {
    return value.value == 0u64 || value.size != 0;
}

u64 slot_home(u64 first_slot, u64 position) on cpu {
    return (first_slot + position) * 8u64;
}

struct ValueCopy { u64 offset; u64 end; u64 present; };

ValueCopy copy_value(u64 start, u32 size, u32 alignment) on cpu {
    if (!indirect(size, alignment)) { return ValueCopy(0u64, start, 0u64); }
    u64 at = (start + 15u64) & ~15u64;
    return ValueCopy(at, at + u64(padded(size, alignment)), 1u64);
}
bool hidden_result(AbiType result, bool fallible) on cpu {
    return result.size != 0 && (fallible || indirect(result.size, result.alignment));
}

extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu {
    bool hidden = hidden_result(result, (flags & 1) != 0);
    bool scratch = (flags & 2) != 0;
    u64 result_at = 0u64;
    if (hidden) { result_at = (data_end + 7u64) & ~7u64; }
    u64 data_bytes = data_end + (hidden ? 8u64 : 0u64);
    u64 bytes = (data_bytes + 15u64) & ~15u64;
    if (!scratch && bytes != 0u64) { bytes += 8u64; }
    u64 home = bytes + (scratch ? 16u64 : 8u64);
    return FunctionFrame(bytes, home, result_at, hidden ? 1u64 : 0u64);
}

extern "C" CallPlan plan(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument, address positions) on cpu requires nonzero(argument) {
    bool hidden = hidden_result(result, (flags & 1) != 0);
    u64 slots = 0u64;
    u64 index = 0u64;
    while (index < count) {
        AbiType value = argument(context, index);
        if (has_slot(value)) { slots += 1u64; }
        index += 1u64;
    }
    u64 arguments = slots;
    u64 first_slot = hidden ? 1u64 : 0u64;
    slots += first_slot;
    // Windows x64 owns four home slots even when fewer arguments are present.
    if (slots < 4u64) { slots = 4u64; }
    u64 bytes = (slots * 8u64 + 15u64) & ~15u64;
    u64 result_at = 0u64;
    if (hidden) {
        result_at = bytes;
        bytes += u64(padded(result.size, result.alignment));
    }
    u64 position = 0u64;
    index = 0u64;
    while (index < count) {
        AbiType value = argument(context, index);
        if (has_slot(value)) {
            u64 home = slot_home(first_slot, position);
            ValueCopy copy = ValueCopy(home, bytes, 0u64);
            if (value.value != 0u64) {
                copy = copy_value(bytes, value.size, value.alignment);
            }
            if (address_bits(positions) != 0u64) {
                u64 payload = copy.present == 0u64 ? home : copy.offset;
                address row = unsafe_address(address_bits(positions) + position * size_of<ArgumentSlot>());
                unsafe_position<ArgumentSlot>(row) = ArgumentSlot(index, home, payload, copy.present);
            }
            bytes = copy.end;
            position += 1u64;
        }
        index += 1u64;
    }
    u64 target = 0u64;
    if ((flags & 2) != 0) {
        bytes = (bytes + 7u64) & ~7u64;
        target = bytes;
        bytes += 8u64;
    }
    return CallPlan((bytes + 15u64) & ~15u64, result_at, target, first_slot, arguments);
}

extern "C" u64 parameter_slots(address context, u64 count, u64 first_slot,
    cfn<AbiType,address,u64> parameter, address positions) on cpu requires nonzero(parameter) {
    u64 position = 0u64;
    u64 index = 0u64;
    while (index < count) {
        AbiType value = parameter(context, index);
        if (has_slot(value)) {
            bool indirect_value = value.value != 0u64 && indirect(value.size, value.alignment);
            address row = unsafe_address(address_bits(positions) + position * size_of<ParameterSlot>());
            unsafe_position<ParameterSlot>(row) = ParameterSlot(
                index, slot_home(first_slot, position), indirect_value ? 1u64 : 0u64);
            position += 1u64;
        }
        index += 1u64;
    }
    return position;
}
