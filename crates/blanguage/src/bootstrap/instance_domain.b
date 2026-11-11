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
