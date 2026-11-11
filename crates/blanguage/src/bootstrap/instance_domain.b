module bootstrap.instance_domain;
import bootstrap.ast_storage;

address at(address bytes, u64 offset) on cpu {
    return unsafe_address(address_bits(bytes) + offset);
}

Slice domain_at(InstanceDomains domains, u64 column) on cpu {
    if (column < domains.type_count) {
        u64 count = unsafe_position<u64>(at(domains.type_sizes, column * size_of<u64>()));
        return Slice(0u64, count);
    }
    u64 slot = domains.parameters.offset + (column - domains.type_count) * size_of<u64>();
    u64 offset = unsafe_position<u64>(at(domains.nodes, slot));
    Optional<Slice> choices = unsafe_position<ValueParameterNode>(at(domains.nodes, offset)).choices;
    if (choices.present == 0) { return Slice(0u64, 0u64); }
    return choices.value;
}

void select_choice(InstanceDomains domains, InstanceRow row, Slice domain, u64 column, u64 index) on cpu {
    unsafe_position<u64>(at(row.indices, column * size_of<u64>())) = index;
    if (column >= domains.type_count) {
        u64 offset = domain.offset + index * size_of<u32>();
        u32 value = unsafe_position<u32>(at(domains.nodes, offset));
        unsafe_position<u32>(at(row.values, (column - domains.type_count) * size_of<u32>())) = value;
    }
}

extern "C" u32 advance(InstanceDomains domains, InstanceRow row, u32 first) on cpu {
    u64 count = domains.type_count + domains.parameters.count;
    if (first != 0) {
        u64 column = 0u64;
        while (column < count) {
            Slice domain = domain_at(domains, column);
            if (domain.count == 0u64) { return 0; }
            select_choice(domains, row, domain, column, 0u64);
            column += 1u64;
        }
        return 1;
    }

    u64 column = count;
    while (column > 0u64) {
        column -= 1u64;
        Slice domain = domain_at(domains, column);
        u64 index = unsafe_position<u64>(at(row.indices, column * size_of<u64>())) + 1u64;
        if (index < domain.count) {
            select_choice(domains, row, domain, column, index);
            return 1;
        }
        select_choice(domains, row, domain, column, 0u64);
    }
    return 0;
}

u64 decimal_size(u32 value) on cpu {
    u32 remaining = value;
    u64 count = 1u64;
    while (remaining >= 10) {
        remaining /= 10;
        count += 1u64;
    }
    return count;
}

extern "C" u64 key_size(InstanceName name) on cpu {
    u64 size = name.base.length + 2u64;
    u64 count = name.type_count + name.value_count;
    if (count != 0u64) { size += count - 1u64; }
    u64 index = 0u64;
    while (index < name.type_count) {
        NameBytes text = unsafe_position<NameBytes>(at(name.types, index * size_of<NameBytes>()));
        size += text.length;
        index += 1u64;
    }
    index = 0u64;
    while (index < name.value_count) {
        size += decimal_size(unsafe_position<u32>(at(name.values, index * size_of<u32>())));
        index += 1u64;
    }
    return size;
}

void copy_name(NameBytes text, address output, u64 offset) on cpu {
    u64 index = 0u64;
    while (index < text.length) {
        unsafe_position<u8>(at(output, offset + index)) = unsafe_position<u8>(at(text.bytes, index));
        index += 1u64;
    }
}

extern "C" void write_key(InstanceName name, address output) on cpu {
    copy_name(name.base, output, 0u64);
    u64 offset = name.base.length;
    unsafe_position<u8>(at(output, offset)) = u8(60);
    offset += 1u64;
    u64 index = 0u64;
    while (index < name.type_count) {
        if (index != 0u64) {
            unsafe_position<u8>(at(output, offset)) = u8(44);
            offset += 1u64;
        }
        NameBytes text = unsafe_position<NameBytes>(at(name.types, index * size_of<NameBytes>()));
        copy_name(text, output, offset);
        offset += text.length;
        index += 1u64;
    }
    index = 0u64;
    while (index < name.value_count) {
        if (name.type_count != 0u64 || index != 0u64) {
            unsafe_position<u8>(at(output, offset)) = u8(44);
            offset += 1u64;
        }
        u32 value = unsafe_position<u32>(at(name.values, index * size_of<u32>()));
        u64 end = offset + decimal_size(value);
        u64 cursor = end;
        while (cursor > offset) {
            cursor -= 1u64;
            unsafe_position<u8>(at(output, cursor)) = u8(48 + value % 10);
            value /= 10;
        }
        offset = end;
        index += 1u64;
    }
    unsafe_position<u8>(at(output, offset)) = u8(62);
}
