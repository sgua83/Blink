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

u32 domain_value(address nodes, Slice choices, u64 index) on cpu {
    return unsafe_position<u32>(at(nodes, choices.offset + index * size_of<u32>()));
}

extern "C" bool accepts_const(address nodes, Optional<Slice> choices, u32 value) on cpu {
    if (choices.present == 0) { return true; }

    u64 first = 0u64;
    u64 last = choices.value.count;
    while (first < last) {
        u64 middle = first + (last - first) / 2u64;
        u32 candidate = domain_value(nodes, choices.value, middle);
        if (candidate == value) { return true; }
        if (candidate < value) { first = middle + 1u64; }
        else { last = middle; }
    }
    return false;
}

void select_choice(InstanceDomains domains, InstanceRow row, Slice domain, u64 column, u64 index) on cpu {
    unsafe_position<u64>(at(row.indices, column * size_of<u64>())) = index;
    if (column >= domains.type_count) {
        u32 value = domain_value(domains.nodes, domain, index);
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

bool key_byte<const Compare:u32 in {0,1}>(address bytes, u64 offset, u8 value) on cpu {
    address slot = unsafe_address(address_bits(bytes) + offset);
    if (Compare == 1) { return unsafe_position<u8>(slot) == value; }
    unsafe_position<u8>(slot) = value;
    return true;
}

bool key_text<const Compare:u32 in {0,1}>(NameBytes text, address bytes, u64 offset) on cpu {
    u64 index = 0u64;
    while (index < text.length) {
        u8 value = unsafe_position<u8>(unsafe_address(address_bits(text.bytes) + index));
        address slot = unsafe_address(address_bits(bytes) + offset + index);
        if (Compare == 1) {
            if (unsafe_position<u8>(slot) != value) { return false; }
        } else {
            unsafe_position<u8>(slot) = value;
        }
        index += 1u64;
    }
    return true;
}

bool key_bytes<const Compare:u32 in {0,1}>(InstanceName name, address bytes) on cpu {
    if (!key_text<Compare>(name.base, bytes, 0u64)) { return false; }
    u64 offset = name.base.length;
    if (!key_byte<Compare>(bytes, offset, u8(60))) { return false; }
    offset += 1u64;
    u64 index = 0u64;
    while (index < name.type_count) {
        if (index != 0u64) {
            if (!key_byte<Compare>(bytes, offset, u8(44))) { return false; }
            offset += 1u64;
        }
        NameBytes text = unsafe_position<NameBytes>(at(name.types, index * size_of<NameBytes>()));
        if (!key_text<Compare>(text, bytes, offset)) { return false; }
        offset += text.length;
        index += 1u64;
    }
    index = 0u64;
    while (index < name.value_count) {
        if (name.type_count != 0u64 || index != 0u64) {
            if (!key_byte<Compare>(bytes, offset, u8(44))) { return false; }
            offset += 1u64;
        }
        u32 value = unsafe_position<u32>(at(name.values, index * size_of<u32>()));
        u64 end = offset + decimal_size(value);
        u64 cursor = end;
        while (cursor > offset) {
            cursor -= 1u64;
            u8 digit = u8(48 + value % 10);
            address slot = unsafe_address(address_bits(bytes) + cursor);
            if (Compare == 1) {
                if (unsafe_position<u8>(slot) != digit) { return false; }
            } else {
                unsafe_position<u8>(slot) = digit;
            }
            value /= 10;
        }
        offset = end;
        index += 1u64;
    }
    return key_byte<Compare>(bytes, offset, u8(62));
}

extern "C" void write_key(InstanceName name, address output) on cpu {
    key_bytes<0>(name, output);
}

extern "C" bool matches_key(InstanceName name, NameBytes actual) on cpu {
    return actual.length == key_size(name) && key_bytes<1>(name, actual.bytes);
}
