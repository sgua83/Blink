module bootstrap.value_domain;
import bootstrap.ast_storage;

address at(address bytes, u64 offset) on cpu {
    return unsafe_address(address_bits(bytes) + offset);
}

Slice choices_at(address nodes, Slice parameters, u64 column) on cpu {
    u64 offset = unsafe_position<u64>(at(nodes, parameters.offset + column * size_of<u64>()));
    Optional<Slice> choices = unsafe_position<ValueParameterNode>(at(nodes, offset)).choices;
    if (choices.present == 0) { return Slice(0u64, 0u64); }
    return choices.value;
}

void select_value(address nodes, Slice choices, address indices, address row, u64 column, u64 index) on cpu {
    u32 value = unsafe_position<u32>(at(nodes, choices.offset + index * size_of<u32>()));
    unsafe_position<u64>(at(indices, column * size_of<u64>())) = index;
    unsafe_position<u32>(at(row, column * size_of<u32>())) = value;
}

extern "C" u32 advance(address nodes, Slice parameters, address indices, address row, u32 first) on cpu {
    if (first != 0) {
        u64 column = 0u64;
        while (column < parameters.count) {
            Slice choices = choices_at(nodes, parameters, column);
            if (choices.count == 0u64) { return 0; }
            select_value(nodes, choices, indices, row, column, 0u64);
            column += 1u64;
        }
        return 1;
    }

    u64 column = parameters.count;
    while (column > 0u64) {
        column -= 1u64;
        Slice choices = choices_at(nodes, parameters, column);
        u64 index = unsafe_position<u64>(at(indices, column * size_of<u64>())) + 1u64;
        if (index < choices.count) {
            select_value(nodes, choices, indices, row, column, index);
            return 1;
        }
        select_value(nodes, choices, indices, row, column, 0u64);
    }
    return 0;
}
