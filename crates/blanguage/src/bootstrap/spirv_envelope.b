module bootstrap.spirv_envelope;

const u32 CAPABILITIES = 22;
const array<u32,CAPABILITIES> CAPABILITY_CODES = array<u32,CAPABILITIES>(
    1, 11, 12, 5347, 10, 39, 4448, 22, 4433, 4464, 4466,
    61, 62, 63, 64, 65, 66, 4472, 5391, 50, 5301, 5307);
const u32 EXTENSIONS = 8;
const u32 EXTENSION_WIDTH = 10;

// These fixed OpExtension instructions include the required NUL and
// word padding. Each row's instruction header gives the emitted length.
const array<array<u32,EXTENSION_WIDTH>,EXTENSIONS> EXTENSION_WORDS =
    array<array<u32,EXTENSION_WIDTH>,EXTENSIONS>(
    // SPV_KHR_8bit_storage
    array<u32,EXTENSION_WIDTH>(0x0007000a, 0x5f565053, 0x5f52484b, 0x74696238, 0x6f74735f, 0x65676172, 0, 0, 0, 0),
    // SPV_KHR_16bit_storage
    array<u32,EXTENSION_WIDTH>(0x0007000a, 0x5f565053, 0x5f52484b, 0x69623631, 0x74735f74, 0x6761726f, 0x00000065, 0, 0, 0),
    // SPV_EXT_descriptor_indexing
    array<u32,EXTENSION_WIDTH>(0x0008000a, 0x5f565053, 0x5f545845, 0x63736564, 0x74706972, 0x695f726f, 0x7865646e, 0x00676e69, 0, 0),
    // SPV_KHR_float_controls
    array<u32,EXTENSION_WIDTH>(0x0007000a, 0x5f565053, 0x5f52484b, 0x616f6c66, 0x6f635f74, 0x6f72746e, 0x0000736c, 0, 0, 0),
    // SPV_KHR_maximal_reconvergence
    array<u32,EXTENSION_WIDTH>(0x0009000a, 0x5f565053, 0x5f52484b, 0x6978616d, 0x5f6c616d, 0x6f636572, 0x7265766e, 0x636e6567, 0x00000065, 0),
    // SPV_KHR_ray_query
    array<u32,EXTENSION_WIDTH>(0x0006000a, 0x5f565053, 0x5f52484b, 0x5f796172, 0x72657571, 0x00000079, 0, 0, 0, 0),
    // SPV_KHR_ray_tracing_position_fetch
    array<u32,EXTENSION_WIDTH>(0x000a000a, 0x5f565053, 0x5f52484b, 0x5f796172, 0x63617274, 0x5f676e69, 0x69736f70, 0x6e6f6974, 0x7465665f, 0x00006863),
    // SPV_KHR_physical_storage_buffer
    array<u32,EXTENSION_WIDTH>(0x0009000a, 0x5f565053, 0x5f52484b, 0x73796870, 0x6c616369, 0x6f74735f, 0x65676172, 0x6675625f, 0x00726566, 0)
);

extern "C" u64 envelope_capacity() on cpu {
    // Header, capabilities, extension rows, import, memory model, entry and modes.
    return u64(5 + 2 * CAPABILITIES + EXTENSIONS * EXTENSION_WIDTH + 6 + 3 + 5 + 6 + 3 + 16);
}

address word_at(address words, u64 index) on cpu {
    return unsafe_address(address_bits(words) + 4u64 * index);
}

void put(address output, u64 at, u32 word) on cpu {
    unsafe_position<u32>(word_at(output, at)) = word;
}

u32 instruction(u32 opcode, u32 words) on cpu {
    return (words << 16) | opcode;
}

u64 capabilities(address output, u64 start, Features f) on cpu {
    u64 at = start;
    u32 preserve = f.preserve32 | f.preserve64;
    array<u32,CAPABILITIES> needed = array<u32,CAPABILITIES>(
        1, 1, f.atomic64, 1, f.float64,
        f.integer8, f.integer8, f.integer16, f.integer16, preserve, preserve,
        f.subgroup_basic, f.subgroup_vote, f.subgroup_arithmetic, f.subgroup_ballot,
        f.subgroup_shuffle, f.subgroup_shuffle_relative, f.acceleration,
        f.position_fetch, f.texture, f.descriptor_indexing, f.descriptor_indexing);
    for (index in 0..CAPABILITIES) {
        if (needed[index] != 0) {
            put(output, at, instruction(17, 2));
            put(output, at + 1u64, CAPABILITY_CODES[index]);
            at += 2u64;
        }
    }
    return at;
}

u64 extensions(address output, u64 start, Features f) on cpu {
    u64 at = start;
    array<u32,EXTENSIONS> needed = array<u32,EXTENSIONS>(
        f.integer8, f.integer16, f.descriptor_indexing, f.preserve32 | f.preserve64,
        f.maximal_reconvergence, f.acceleration, f.position_fetch, 1);
    for (index in 0..EXTENSIONS) {
        if (needed[index] != 0) {
            array<u32,EXTENSION_WIDTH> row = EXTENSION_WORDS[index];
            u32 length = row[0] >> 16;
            for (word in 0..EXTENSION_WIDTH) {
                if (word < length) {
                    put(output, at + u64(word), row[word]);
                }
            }
            at += u64(length);
        }
    }
    return at;
}

u64 entry_point(address output, u64 start, u32 entry, address interfaces, u64 count) on cpu {
    put(output, start + 1u64, 5);
    put(output, start + 2u64, entry);
    put(output, start + 3u64, 0x6e69616d); // "main"
    put(output, start + 4u64, 0);
    u64 at = start + 5u64;
    u64 index = 0u64;
    while (index < count) {
        address row = unsafe_address(address_bits(interfaces) + index * size_of<Words>());
        Words part = unsafe_position<Words>(row);
        u64 item = 0u64;
        while (item < part.count) {
            u32 id = unsafe_position<u32>(word_at(part.data, item));
            if (id != 0) {
                put(output, at, id);
                at += 1u64;
            }
            item += 1u64;
        }
        index += 1u64;
    }
    put(output, start, instruction(15, u32(at - start)));
    return at;
}

void float_modes(address output, u64 at, u32 entry, u32 width) on cpu {
    put(output, at, instruction(16, 4));
    put(output, at + 1u64, entry);
    put(output, at + 2u64, 4459);
    put(output, at + 3u64, width);
    put(output, at + 4u64, instruction(16, 4));
    put(output, at + 5u64, entry);
    put(output, at + 6u64, 4461);
    put(output, at + 7u64, width);
}

extern "C" u64 encode(Envelope input, address interfaces, u64 count, address words) on cpu {
    put(words, 0u64, 0x07230203);
    put(words, 1u64, 0x00010400);
    put(words, 2u64, 0);
    put(words, 3u64, input.bound);
    put(words, 4u64, 0);
    u64 at = capabilities(words, 5u64, input.features);
    at = extensions(words, at, input.features);

    put(words, at, instruction(11, 6));
    put(words, at + 1u64, input.ext);
    // "GLSL.std.450"
    put(words, at + 2u64, 0x4c534c47);
    put(words, at + 3u64, 0x6474732e);
    put(words, at + 4u64, 0x3035342e);
    put(words, at + 5u64, 0);
    at += 6u64;

    put(words, at, instruction(14, 3));
    put(words, at + 1u64, 5348);
    put(words, at + 2u64, 1);

    at = entry_point(words, at + 3u64, input.entry, interfaces, count);
    put(words, at, instruction(16, 6));
    put(words, at + 1u64, input.entry);
    put(words, at + 2u64, 17);
    put(words, at + 3u64, input.workgroup_width);
    put(words, at + 4u64, 1);
    put(words, at + 5u64, 1);
    at += 6u64;
    if (input.features.maximal_reconvergence != 0) {
        put(words, at, instruction(16, 3));
        put(words, at + 1u64, input.entry);
        put(words, at + 2u64, 6023);
        at += 3u64;
    }
    if (input.features.preserve32 != 0) {
        float_modes(words, at, input.entry, 32);
        at += 8u64;
    }
    if (input.features.preserve64 != 0) {
        float_modes(words, at, input.entry, 64);
        at += 8u64;
    }
    return at;
}

struct ScalarDefinition { u32 opcode; u32 width; u32 signed; };

ScalarDefinition scalar_definition(BuiltinScalar scalar) on cpu {
    u32 width = 32;
    if (scalar == BuiltinScalar::U64 || scalar == BuiltinScalar::I64 || scalar == BuiltinScalar::F64) { width = 64; }
    if (scalar == BuiltinScalar::U8 || scalar == BuiltinScalar::I8) { width = 8; }
    if (scalar == BuiltinScalar::U16 || scalar == BuiltinScalar::I16) { width = 16; }
    u32 signed = u32(scalar == BuiltinScalar::I8 || scalar == BuiltinScalar::I16 || scalar == BuiltinScalar::I32 || scalar == BuiltinScalar::I64);
    u32 opcode = 21;
    if (scalar == BuiltinScalar::F32 || scalar == BuiltinScalar::F64) { opcode = 22; }
    if (scalar == BuiltinScalar::Bool) { opcode = 20; }
    return ScalarDefinition(opcode, width, signed);
}

extern "C" u64 scalar_declaration(BuiltinScalar scalar, u32 type_id, u32 pointer_id, address words) on cpu {
    ScalarDefinition type = scalar_definition(scalar);
    u64 at = 2u64;
    if (type.opcode != 20) { put(words, at, type.width); at += 1u64; }
    if (type.opcode == 21) { put(words, at, type.signed); at += 1u64; }
    put(words, 0u64, instruction(type.opcode, u32(at)));
    put(words, 1u64, type_id);
    put(words, at, instruction(32, 4));
    put(words, at + 1u64, pointer_id);
    put(words, at + 2u64, 7);
    put(words, at + 3u64, type_id);
    return at + 4u64;
}

extern "C" u64 constant_declaration(BuiltinScalar scalar, u32 type_id, u32 id, u64 bits, address words) on cpu {
    ScalarDefinition type = scalar_definition(scalar);
    put(words, 1u64, type_id);
    put(words, 2u64, id);
    if (type.opcode == 20) {
        put(words, 0u64, instruction(bits == 0u64 ? 42 : 41, 3));
        return 3u64;
    }
    put(words, 3u64, u32(bits));
    if (type.width == 64) {
        put(words, 0u64, instruction(43, 5));
        put(words, 4u64, u32(bits >> 32u64));
        return 5u64;
    }
    put(words, 0u64, instruction(43, 4));
    return 4u64;
}
