module bootstrap.spirv_envelope;
import bootstrap.ir_dataflow;
import bootstrap.stage_fusion;

const u32 CAPABILITIES = 21;
const array<u32,CAPABILITIES> CAPABILITY_CODES = array<u32,CAPABILITIES>(
    1, 11, 12, 5347, 10, 39, 4448, 22, 4433, 4464, 4466,
    61, 62, 63, 64, 65, 66, 4472, 5391, 5301, 5307);
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
        f.position_fetch, f.descriptor_indexing, f.descriptor_indexing);
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

u64 entry_point(address output, u64 start, u32 entry, address interfaces, u64 count, bool prepare) on cpu {
    put(output, start + 1u64, 5);
    put(output, start + 2u64, entry);
    // "main" and "prepare", including the final NUL.
    put(output, start + 3u64, prepare ? 0x70657270 : 0x6e69616d);
    put(output, start + 4u64, prepare ? 0x00657261 : 0);
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

    at = entry_point(words, at + 3u64, input.entry, interfaces, count, false);
    if (input.prepare != 0) {
        at = entry_point(words, at, input.entry, interfaces, count, true);
    }
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

u64 scalar_declaration(BuiltinScalar scalar, u32 type_id, u32 pointer_id, address words) on cpu {
    ScalarDefinition type = scalar_definition(scalar);
    u64 at = 2u64;
    if (type.opcode != 20) { put(words, at, type.width); at += 1u64; }
    if (type.opcode == 21) { put(words, at, type.signed); at += 1u64; }
    put(words, 0u64, instruction(type.opcode, u32(at)));
    put(words, 1u64, type_id);
    return words_three(words, at, 32, pointer_id, u32(StorageClass::Function), type_id);
}

u64 constant_declaration(ScalarDefinition type, u32 type_id, u32 id, u64 bits, address words) on cpu {
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

u64 words_one(address words, u64 at, u32 opcode, u32 id) on cpu {
    put(words, at, instruction(opcode, 2));
    put(words, at + 1u64, id);
    return at + 2u64;
}

u64 words_two(address words, u64 at, u32 opcode, u32 id, u32 first) on cpu {
    put(words, at, instruction(opcode, 3));
    put(words, at + 1u64, id);
    put(words, at + 2u64, first);
    return at + 3u64;
}

u64 words_three(address words, u64 at, u32 opcode, u32 id, u32 first, u32 second) on cpu {
    put(words, at, instruction(opcode, 4));
    put(words, at + 1u64, id);
    put(words, at + 2u64, first);
    put(words, at + 3u64, second);
    return at + 4u64;
}

extern "C" u64 type_declaration(TypeShape shape, u32 id, u32 first, u32 second, address words) on cpu {
    if (shape == TypeShape::Void) { return words_one(words, 0u64, 19, id); }
    if (shape == TypeShape::Vector) { return words_three(words, 0u64, 23, id, first, second); }
    if (shape == TypeShape::Matrix) { return words_three(words, 0u64, 24, id, first, second); }
    if (shape == TypeShape::SampledImage) { return words_two(words, 0u64, 27, id, first); }
    if (shape == TypeShape::Array) { return words_three(words, 0u64, 28, id, first, second); }
    if (shape == TypeShape::Function) { return words_two(words, 0u64, 33, id, first); }
    if (shape == TypeShape::Acceleration) { return words_one(words, 0u64, 5341, id); }
    if (shape == TypeShape::RayQuery) { return words_one(words, 0u64, 4472, id); }
    if (shape == TypeShape::Pointer) { return words_three(words, 0u64, 32, id, first, second); }

    // The sampled image's dimensionality is the source texture coordinate width.
    put(words, 0u64, instruction(25, 9));
    put(words, 1u64, id);
    put(words, 2u64, first);
    put(words, 3u64, second - 1);
    put(words, 4u64, 0); // Not a depth image.
    put(words, 5u64, 0); // Descriptor arrays wrap the sampled image type.
    put(words, 6u64, 0); // Single sample.
    put(words, 7u64, 1); // Used with a sampler.
    put(words, 8u64, 0); // Unknown image format.
    return 9u64;
}

struct ResourceBlock { u32 element; u32 length; u32 array; u32 block; u32 pointer; };

u64 resource_block(ResourceBlock input, address definitions, address decorations) on cpu {
    u64 at = 0u64;
    StorageClass storage = StorageClass::StorageBuffer;
    u32 stride = 8;
    if (input.length == 0) {
        at = words_two(definitions, at, 29, input.array, input.element);
    } else {
        at = words_three(definitions, at, 28, input.array, input.element, input.length);
        storage = StorageClass::PushConstant;
        stride = 4;
    }
    at = words_two(definitions, at, 30, input.block, input.array);
    at = words_three(definitions, at, 32, input.pointer, u32(storage), input.block);

    // ArrayStride, Block, and Offset of the single array member.
    u64 next = words_three(decorations, 0u64, 71, input.array, 6, stride);
    words_two(decorations, next, 71, input.block, 2);
    put(decorations, 7u64, instruction(72, 5));
    put(decorations, 8u64, input.block);
    put(decorations, 9u64, 0);
    put(decorations, 10u64, 35);
    put(decorations, 11u64, 0);
    return at;
}

void variable_declaration(StorageClass storage, u32 pointer, u32 id, address words) on cpu {
    words_three(words, 0u64, 59, pointer, id, u32(storage));
}

u64 annotated_variable(VariableAnnotation annotation, u32 pointer, u32 id, u32 operand, address definitions, address decorations) on cpu {
    if (annotation == VariableAnnotation::BuiltIn) {
        variable_declaration(StorageClass::Input, pointer, id, definitions);
        return words_three(decorations, 0u64, 71, id, 11, operand);
    }
    StorageClass storage = StorageClass::UniformConstant;
    if (annotation == VariableAnnotation::PageTable) {
        storage = StorageClass::StorageBuffer;
    }
    variable_declaration(storage, pointer, id, definitions);
    u64 at = words_three(decorations, 0u64, 71, id, 34, 0);
    at = words_three(decorations, at, 71, id, 33, operand);
    if (annotation == VariableAnnotation::PageTable) {
        // Page-table access is read-only even when referenced elements can be written.
        at = words_two(decorations, at, 71, id, 24);
    }
    return at;
}

struct ResourceCursor {
    address definitions;
    address decorations;
    u64 words;
    u64 annotations;
    u32 next;
};

u32 definition_id(inout ResourceCursor out) on cpu {
    u32 id = out.next;
    out.next += 1;
    return id;
}

u32 define_type(inout ResourceCursor out, TypeShape shape, u32 first, u32 second) on cpu {
    u32 id = definition_id(out);
    out.words += type_declaration(shape, id, first, second, word_at(out.definitions, out.words));
    return id;
}

u32 define_pointer(inout ResourceCursor out, StorageClass storage, u32 pointee) on cpu {
    return define_type(out, TypeShape::Pointer, u32(storage), pointee);
}

struct ConstantEntry { ScalarDefinition type; u64 bits; u32 id; };

ConstantEntry constant_entry(ShaderConstant input, ShaderConstants constants) on cpu requires nonzero(constants.intern) {
    ScalarDefinition type = scalar_definition(input.scalar);
    u64 bits = input.bits;
    if (type.width == 8) { bits = bits & 255u64; }
    if (type.width == 16) { bits = bits & 65535u64; }
    if (type.width == 32) { bits = u64(u32(bits)); }
    u32 id = constants.intern(constants.context, input.scalar, bits, input.next);
    return ConstantEntry(type, bits, id);
}

extern "C" ConstantWords shader_constant(ShaderConstant input, ShaderConstants constants,
    address definitions, cfn<address,address> reserve) on cpu requires nonzero(constants.intern) requires nonzero(reserve) {
    ConstantEntry entry = constant_entry(input, constants);
    if (entry.id != input.next) { return ConstantWords(0u64, entry.id, input.next); }
    address output = reserve(definitions);
    u64 words = constant_declaration(entry.type, input.type_id, entry.id, entry.bits, output);
    return ConstantWords(words, entry.id, input.next + 1);
}

u32 define_word(inout ResourceCursor out, ShaderConstants constants, u32 word_type, u32 bits) on cpu
    requires nonzero(constants.intern) {
    ShaderConstant input = ShaderConstant(BuiltinScalar::U32, word_type, out.next, u64(bits));
    ConstantEntry entry = constant_entry(input, constants);
    if (entry.id == out.next) {
        out.next += 1;
        out.words += constant_declaration(entry.type, word_type, entry.id, entry.bits,
                                          word_at(out.definitions, out.words));
    }
    return entry.id;
}

void define_scalar(inout ResourceCursor out, BuiltinScalar scalar, address types, address pointers) on cpu {
    u32 type_id = definition_id(out);
    u32 pointer_id = definition_id(out);
    u64 slot = u64(u32(scalar));
    put(types, slot, type_id);
    put(pointers, slot, pointer_id);
    out.words += scalar_declaration(scalar, type_id, pointer_id, word_at(out.definitions, out.words));
}

extern "C" ScalarDeclarations shader_scalars(u32 next, u32 float64, u32 integer8, u32 integer16,
    address definitions, address types, address pointers) on cpu {
    ResourceCursor out = ResourceCursor(definitions, address(), 0u64, 0u64, next);
    u32 void_id = define_type(out, TypeShape::Void, 0, 0);
    define_scalar(out, BuiltinScalar::F32, types, pointers);
    define_scalar(out, BuiltinScalar::U32, types, pointers);
    define_scalar(out, BuiltinScalar::I32, types, pointers);
    define_scalar(out, BuiltinScalar::Bool, types, pointers);
    define_scalar(out, BuiltinScalar::U64, types, pointers);
    define_scalar(out, BuiltinScalar::I64, types, pointers);
    if (float64 != 0) { define_scalar(out, BuiltinScalar::F64, types, pointers); }
    if (integer8 != 0) {
        define_scalar(out, BuiltinScalar::U8, types, pointers);
        define_scalar(out, BuiltinScalar::I8, types, pointers);
    }
    if (integer16 != 0) {
        define_scalar(out, BuiltinScalar::U16, types, pointers);
        define_scalar(out, BuiltinScalar::I16, types, pointers);
    }
    return ScalarDeclarations(out.words, void_id, out.next);
}

u32 define_resource_block(inout ResourceCursor out, u32 element, u32 length) on cpu {
    u32 array = definition_id(out);
    u32 record = definition_id(out);
    u32 pointer = definition_id(out);
    out.words += resource_block(ResourceBlock(element, length, array, record, pointer),
                                word_at(out.definitions, out.words), word_at(out.decorations, out.annotations));
    out.annotations += 12u64;
    return pointer;
}

u32 define_annotated(inout ResourceCursor out, VariableAnnotation annotation, u32 pointer, u32 operand) on cpu {
    u32 id = definition_id(out);
    out.annotations += annotated_variable(annotation, pointer, id, operand,
                                          word_at(out.definitions, out.words), word_at(out.decorations, out.annotations));
    out.words += 4u64;
    return id;
}

SampledType texture_type(inout ResourceCursor out, u32 float_type, u32 dimensions) on cpu {
    u32 image = define_type(out, TypeShape::Image, float_type, dimensions);
    u32 sampled = define_type(out, TypeShape::SampledImage, image, 0);
    u32 pointer = define_pointer(out, StorageClass::UniformConstant, sampled);
    return SampledType(sampled, pointer);
}

extern "C" ShaderResourceIds shader_resources(ShaderResourceInput input, ShaderResourceHost host, ResourceStreams streams) on cpu
    requires nonzero(host.texture) requires nonzero(host.words.intern) {
    ResourceCursor out = ResourceCursor(streams.definitions, streams.decorations, 0u64, 0u64, input.next);
    u32 vec3 = define_type(out, TypeShape::Vector, input.float_type, 3);
    u32 vec2 = define_type(out, TypeShape::Vector, input.float_type, 2);
    u32 vec4 = 0;
    SampledType sampled2 = SampledType(0, 0);
    SampledType sampled3 = SampledType(0, 0);
    if (input.textures != 0u64) {
        vec4 = define_type(out, TypeShape::Vector, input.float_type, 4);
        usize i = 0u64;
        while (i < input.textures) {
            TextureDeclaration texture = host.texture(host.textures, i);
            SampledType sampled = texture.dimensions == 2 ? sampled2 : sampled3;
            if (sampled.sampled == 0) {
                sampled = texture_type(out, input.float_type, texture.dimensions);
                if (texture.dimensions == 2) { sampled2 = sampled; }
                else { sampled3 = sampled; }
            }
            u32 pointer = sampled.pointer;
            if (texture.arrayed != 0) {
                u32 length = define_word(out, host.words, input.word_type, texture.length);
                u32 array = define_type(out, TypeShape::Array, sampled.sampled, length);
                pointer = define_pointer(out, StorageClass::UniformConstant, array);
            }
            u32 binding = u32(input.buffers + input.accelerations + i);
            u32 variable = define_annotated(out, VariableAnnotation::Descriptor, pointer, binding);
            put(streams.textures, i, variable);
            i += 1u64;
        }
    }

    u32 acceleration = 0;
    u32 ray_pointer = 0;
    if (input.accelerations != 0u64) {
        acceleration = define_type(out, TypeShape::Acceleration, 0, 0);
        u32 pointer = define_pointer(out, StorageClass::UniformConstant, acceleration);
        usize i = 0u64;
        while (i < input.accelerations) {
            u32 variable = define_annotated(out, VariableAnnotation::Descriptor, pointer, u32(input.buffers + i));
            put(streams.accelerations, i, variable);
            i += 1u64;
        }
        u32 query = define_type(out, TypeShape::RayQuery, 0, 0);
        ray_pointer = define_pointer(out, StorageClass::Function, query);
    }

    u32 index_type = define_type(out, TypeShape::Vector, input.word_type, 3);
    u32 index_pointer = define_pointer(out, StorageClass::Input, index_type);
    u32 global = define_annotated(out, VariableAnnotation::BuiltIn, index_pointer, 28);
    u32 num_groups = 0;
    if (input.grid != 0 && input.argument_pass == 0) {
        num_groups = define_annotated(out, VariableAnnotation::BuiltIn, index_pointer, 24);
    }

    u32 page = define_resource_block(out, input.wide_type, 0);
    u32 words32 = define_pointer(out, StorageClass::PhysicalStorageBuffer, input.word_type);
    u32 words64 = define_pointer(out, StorageClass::PhysicalStorageBuffer, input.wide_type);
    u32 bytes = input.byte_type == 0 ? 0 : define_pointer(out, StorageClass::PhysicalStorageBuffer, input.byte_type);
    u32 shorts = input.short_type == 0 ? 0 : define_pointer(out, StorageClass::PhysicalStorageBuffer, input.short_type);
    u32 table_pointer = define_pointer(out, StorageClass::StorageBuffer, input.wide_type);
    bool omit_summary = input.publishes_summary == 0 || input.single_group != 0
        || (input.argument_pass != 0 && input.indirect != input.buffers);
    usize i = 0u64;
    while (i < input.slots) {
        u32 variable = 0;
        if (i != input.buffers || !omit_summary) {
            usize binding = i < input.buffers ? i : i + input.accelerations + input.textures;
            variable = define_annotated(out, VariableAnnotation::PageTable, page, u32(binding));
        }
        put(streams.pages, i, variable);
        i += 1u64;
    }

    u32 length = define_word(out, host.words, input.word_type, input.uniform_bytes / 4);
    u32 push_block = define_resource_block(out, input.word_type, length);
    u32 push = definition_id(out);
    variable_declaration(StorageClass::PushConstant, push_block, push, word_at(out.definitions, out.words));
    out.words += 4u64;
    u32 push_pointer = define_pointer(out, StorageClass::PushConstant, input.word_type);
    return ShaderResourceIds(out.words, out.annotations, out.next, vec3, vec2, vec4, acceleration,
        ray_pointer, index_type, global, num_groups, words32, words64, bytes, shorts,
        table_pointer, push, push_pointer, sampled2, sampled3);
}

u32 function_variable(inout ResourceCursor out, address body, inout u64 words, u32 pointer) on cpu {
    u32 id = definition_id(out);
    variable_declaration(StorageClass::Function, pointer, id, word_at(body, words));
    words += 4u64;
    return id;
}

struct FunctionArrayMarks { address locals; u64 count; };

extern "C" void mark_function_array(address context, usize begin, usize end) on cpu {
    address slot = word_at(unsafe_position<FunctionArrayMarks>(context).locals, end - 1u64);
    if (unsafe_position<u32>(slot) == 0) {
        unsafe_position<u32>(slot) = 1;
        unsafe_position<FunctionArrayMarks>(context).count += 1u64;
    }
}

View function_component(Span components, usize index, Host ir) on cpu requires nonzero(ir.inspect) {
    address value = unsafe_address(address_bits(components.data) + index * u64(components.stride));
    return ir.inspect(Node(value, N_COMPONENT));
}

address private_array_at(address arrays, usize index) on cpu {
    return unsafe_address(address_bits(arrays) + index * size_of<PrivateArray>());
}

extern "C" FunctionStorageWords function_storage(FunctionStorageInput input, FunctionStorageHost host,
    ShaderConstants constants) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.reserve) requires nonzero(host.local_type)
    requires nonzero(host.next_local) requires nonzero(constants.intern) {
    address locals = input.locals.data;
    FunctionArrayMarks marks = FunctionArrayMarks(locals, 0u64);
    // Each complete declaration marks its final local slot. Duplicate uses share that slot.
    private_spans(input.code, host.ir, address_of(marks), c_function(mark_function_array));
    FunctionStorageStreams streams = host.reserve(host.context, 12u64 * marks.count,
        4u64 * (input.rays + input.locals.count), marks.count);
    ResourceCursor out = ResourceCursor(streams.definitions, address(), 0u64, 0u64, input.next);
    u64 words = 0u64;
    usize ray = 0u64;
    while (ray < input.rays) {
        function_variable(out, streams.body, words, input.ray_pointer);
        ray += 1u64;
    }

    // Array declarations precede scalar variables, both in original local order.
    usize count = 0u64;
    usize slot = 0u64;
    while (slot < input.locals.count) {
        if (unsafe_position<u32>(word_at(locals, slot)) != 0) {
            usize end = slot + 1u64;
            Node shape = host.local_type(host.context, end);
            View ty = host.ir.inspect(shape);
            Span components = ty.second;
            u32 unit = 4;
            usize field = 0u64;
            while (field < components.count) {
                unit = min(unit, u32(function_component(components, field, host.ir).b));
                field += 1u64;
            }
            u32 element = unit == 1 ? input.byte_type : (unit == 2 ? input.short_type : input.word_type);
            u32 shift = unit == 1 ? 0 : (unit == 2 ? 1 : 2);
            u32 length = define_word(out, constants, input.word_type, u32(ty.a) >> shift);
            u32 array_type = define_type(out, TypeShape::Array, element, length);
            u32 pointer = define_pointer(out, StorageClass::Function, array_type);
            u32 variable = function_variable(out, streams.body, words, pointer);
            BuiltinScalar storage = unit == 1 ? BuiltinScalar::U8 :
                (unit == 2 ? BuiltinScalar::U16 : BuiltinScalar::U32);
            unsafe_position<PrivateArray>(private_array_at(streams.arrays, count)) =
                PrivateArray(end, components, variable, storage);
            count += 1u64;
        }
        slot += 1u64;
    }

    FunctionLocal group = host.next_local(host.context);
    while (group.ty.data != address()) {
        usize last = group.end - 1u64;
        if (unsafe_position<u32>(word_at(locals, last)) != 0) {
            put(locals, last, 0);
        } else {
            Span components = host.ir.inspect(group.ty).second;
            usize start = group.end - components.count;
            usize field = 0u64;
            while (field < components.count) {
                View component = function_component(components, field, host.ir);
                u32 pointer = unsafe_position<u32>(word_at(input.pointers, component.c));
                u32 variable = function_variable(out, streams.body, words, pointer);
                put(locals, start + field, variable);
                field += 1u64;
            }
        }
        group = host.next_local(host.context);
    }

    return FunctionStorageWords(out.words, words, count, out.next, input.next);
}

extern "C" u64 flag_decoration(address decorations, u64 at, FlagDecoration decoration, u32 id) on cpu {
    return words_two(decorations, at, 71, id, u32(decoration));
}

u64 words_four(address words, u64 at, u32 opcode, u32 id, u32 first, u32 second, u32 third) on cpu {
    put(words, at, instruction(opcode, 5));
    put(words, at + 1u64, id);
    put(words, at + 2u64, first);
    put(words, at + 3u64, second);
    put(words, at + 4u64, third);
    return at + 5u64;
}

u32 comparison_opcode(BinaryOperation operation, ScalarDefinition scalar) on cpu {
    bool floating = scalar.opcode == 22;
    bool signed = scalar.signed != 0;
    if (operation == BinaryOperation::Equal) { return floating ? 180 : (scalar.opcode == 20 ? 164 : 170); }
    if (operation == BinaryOperation::NotEqual) { return floating ? 183 : (scalar.opcode == 20 ? 165 : 171); }
    if (operation == BinaryOperation::Less) { return floating ? 184 : (signed ? 177 : 176); }
    if (operation == BinaryOperation::Greater) { return floating ? 186 : (signed ? 173 : 172); }
    if (operation == BinaryOperation::LessEqual) { return floating ? 188 : (signed ? 179 : 178); }
    return floating ? 190 : (signed ? 175 : 174);
}

u32 binary_opcode(BinaryOperation operation, ScalarDefinition scalar) on cpu {
    bool floating = scalar.opcode == 22;
    if (operation == BinaryOperation::Add) { return floating ? 129 : 128; }
    if (operation == BinaryOperation::Subtract) { return floating ? 131 : 130; }
    if (operation == BinaryOperation::Multiply) { return floating ? 133 : 132; }
    if (operation == BinaryOperation::Divide) { return 136; }
    if (operation == BinaryOperation::And) { return 199; }
    if (operation == BinaryOperation::Or) { return 197; }
    if (operation == BinaryOperation::Xor) { return 198; }
    if (operation == BinaryOperation::ShiftLeft) { return 196; }
    if (operation == BinaryOperation::ShiftRight) { return 194; }
    if (operation == BinaryOperation::ShiftRightSigned) { return 195; }
    return comparison_opcode(operation, scalar);
}

ScalarWords scalar_binary(BinaryEmission input, ScalarHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.ids) requires nonzero(host.reserve) {
    ScalarDefinition scalar = scalar_definition(input.scalar);
    u32 opcode = binary_opcode(input.operation, scalar);
    u32 mask = 0;
    u32 masked = 0;
    if (input.operation == BinaryOperation::ShiftLeft || input.operation == BinaryOperation::ShiftRight ||
        input.operation == BinaryOperation::ShiftRightSigned) {
        mask = host.constant(host.context, input.scalar, u64(scalar.width - 1));
        masked = host.ids(host.context, 1);
    }
    bool decorate = scalar.opcode == 22 && u32(input.operation) <= u32(BinaryOperation::Divide);
    ScalarStreams streams = host.reserve(host.context, masked == 0 ? 5u64 : 10u64, decorate ? 3u64 : 0u64);
    u64 at = 0u64;
    u32 right = input.right;
    if (masked != 0) {
        at = words_four(streams.body, at, 199, input.result_type, masked, right, mask);
        right = masked;
    }
    at = words_four(streams.body, at, opcode, input.result_type, input.result, input.left, right);
    if (decorate) {
        return ScalarWords(at, flag_decoration(streams.decorations, 0u64, FlagDecoration::NoContraction, input.result));
    }
    return ScalarWords(at, 0u64);
}

extern "C" CollectiveWords collective_pair(CollectivePair input, address body, address decorations) on cpu {
    CollectiveOperation operation = input.operation;
    ScalarDefinition scalar = scalar_definition(input.scalar);
    u32 next = input.next;
    if (operation == CollectiveOperation::Sum && scalar.opcode == 22) {
        u32 left = next;
        u32 right = next + 1;
        u32 value = next + 2;
        u32 sum = next + 3;
        u64 at = words_three(body, 0u64, 124, input.value_type, left, input.left);
        at = words_three(body, at, 124, input.value_type, right, input.right);
        at = words_four(body, at, binary_opcode(BinaryOperation::Add, scalar), input.value_type, sum, left, right);
        at = words_three(body, at, 124, input.raw_type, value, sum);
        u64 decorated = flag_decoration(decorations, 0u64, FlagDecoration::NoContraction, sum);
        return CollectiveWords(at, decorated, next + 4, value);
    }

    if (operation == CollectiveOperation::Min || operation == CollectiveOperation::Max) {
        u64 at = 0u64;
        u32 left = input.left;
        u32 right = input.right;
        if (input.value_type != input.raw_type) {
            left = next;
            right = next + 1;
            next += 2;
            at = words_three(body, at, 124, input.value_type, left, input.left);
            at = words_three(body, at, 124, input.value_type, right, input.right);
        }
        BinaryOperation comparison = operation == CollectiveOperation::Min
            ? BinaryOperation::Less : BinaryOperation::Greater;
        u32 value = next;
        u32 choose = next + 1;
        at = words_four(body, at, comparison_opcode(comparison, scalar), input.bool_type, choose, left, right);
        at = select_words(body, at, input.raw_type, value, choose, input.left, input.right);
        return CollectiveWords(at, 0u64, next + 2, value);
    }

    BinaryOperation binary = BinaryOperation::Add;
    if (operation == CollectiveOperation::BitAnd) { binary = BinaryOperation::And; }
    if (operation == CollectiveOperation::BitOr) { binary = BinaryOperation::Or; }
    if (operation == CollectiveOperation::BitXor) { binary = BinaryOperation::Xor; }
    u64 at = words_four(body, 0u64, binary_opcode(binary, scalar), input.raw_type, next, input.left, input.right);
    return CollectiveWords(at, 0u64, next + 1, next);
}

u32 cast_opcode(BuiltinScalar from, BuiltinScalar to, ScalarDefinition source, ScalarDefinition target) on cpu {
    if (source.opcode == 22) {
        if (target.opcode == 21) { return target.signed != 0 ? 110 : 109; }
        return from == to ? 83 : 115;
    }
    if (target.opcode == 22) { return source.signed != 0 ? 111 : 112; }
    if (source.width != target.width) { return source.signed != 0 ? 114 : 113; }
    return 124;
}

ScalarWords scalar_cast(CastEmission input, ScalarHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.reserve) {
    address context = host.context;
    if (input.from == BuiltinScalar::Bool) {
        u64 one_bits = input.to == BuiltinScalar::F32 ? 0x3f800000u64 :
            input.to == BuiltinScalar::F64 ? 0x3ff0000000000000u64 : 1u64;
        u32 one = host.constant(context, input.to, one_bits);
        u32 zero = host.constant(context, input.to, 0u64);
        address body = host.reserve(context, 6u64, 0u64).body;
        u64 words = select_words(body, 0u64, input.result_type, input.result, input.value, one, zero);
        return ScalarWords(words, 0u64);
    }
    ScalarDefinition source = scalar_definition(input.from);
    if (input.to == BuiltinScalar::Bool) {
        u32 zero = host.constant(context, input.from, 0u64);
        address body = host.reserve(context, 5u64, 0u64).body;
        u64 words = words_four(body, 0u64, source.opcode == 22 ? 183 : 171,
            input.result_type, input.result, input.value, zero);
        return ScalarWords(words, 0u64);
    }
    ScalarDefinition target = scalar_definition(input.to);
    address body = host.reserve(context, 4u64, 0u64).body;
    u64 words = words_three(body, 0u64, cast_opcode(input.from, input.to, source, target),
        input.result_type, input.result, input.value);
    return ScalarWords(words, 0u64);
}

struct DivisionEmission {
    u32 integer_type; u32 magnitude_type; u32 numerator; u32 denominator;
    u32 quotient; u32 remainder; u32 sign_shift; u32 next;
};
struct DivisionWords { u32 body; u32 next; };
struct SignedMagnitude { u64 end; u32 value; u32 sign; u32 next; };

SignedMagnitude emit_magnitude(address body, u64 at, DivisionEmission input, u32 value, u32 next) on cpu {
    u32 sign = next;
    u32 flipped = next + 1;
    u32 magnitude = next + 2;
    u32 bits = next + 3;
    u32 type = input.integer_type;
    u64 end = words_four(body, at, 195, type, sign, value, input.sign_shift);
    end = words_four(body, end, 198, type, flipped, value, sign);
    end = words_four(body, end, 130, type, magnitude, flipped, sign);
    end = words_three(body, end, 124, input.magnitude_type, bits, magnitude);
    return SignedMagnitude(end, bits, sign, next + 4);
}

DivisionWords emit_signed_division_result(address body, DivisionWords cursor, DivisionEmission input,
    SignedMagnitude numerator, SignedMagnitude denominator, u32 result, bool quotient) on cpu {
    u32 raw = cursor.next;
    u32 signed = raw + 1;
    u32 next = signed + 1;
    u32 type = input.integer_type;
    u64 at = words_four(body, u64(cursor.body), quotient ? 134 : 137,
        input.magnitude_type, raw, numerator.value, denominator.value);
    at = words_three(body, at, 124, type, signed, raw);
    u32 sign = numerator.sign;
    if (quotient) {
        sign = next;
        next += 1;
        at = words_four(body, at, 198, type, sign, numerator.sign, denominator.sign);
    }
    u32 flipped = next;
    at = words_four(body, at, 198, type, flipped, signed, sign);
    at = words_four(body, at, 130, type, result, flipped, sign);
    return DivisionWords(u32(at), next + 1);
}

DivisionWords encode_integer_division(DivisionEmission input, address body) on cpu {
    if (input.sign_shift == 0) {
        u64 at = 0u64;
        if (input.quotient != 0) {
            at = words_four(body, at, 134, input.integer_type, input.quotient, input.numerator, input.denominator);
        }
        if (input.remainder != 0) {
            at = words_four(body, at, 137, input.integer_type, input.remainder, input.numerator, input.denominator);
        }
        return DivisionWords(u32(at), input.next);
    }

    // Unsigned magnitudes represent signed MIN exactly. Restore quotient and
    // remainder signs separately so the remainder follows the numerator.
    SignedMagnitude numerator = emit_magnitude(body, 0u64, input, input.numerator, input.next);
    SignedMagnitude denominator = emit_magnitude(body, numerator.end, input, input.denominator, numerator.next);
    DivisionWords cursor = DivisionWords(u32(denominator.end), denominator.next);
    if (input.quotient != 0) {
        cursor = emit_signed_division_result(body, cursor, input, numerator, denominator, input.quotient, true);
    }
    if (input.remainder != 0) {
        cursor = emit_signed_division_result(body, cursor, input, numerator, denominator, input.remainder, false);
    }
    return cursor;
}

BuiltinScalar unsigned_scalar(u32 width) on cpu {
    if (width == 8) { return BuiltinScalar::U8; }
    if (width == 16) { return BuiltinScalar::U16; }
    return width == 64 ? BuiltinScalar::U64 : BuiltinScalar::U32;
}

// Zero operand ID selects an immediate divisor or an absent output.
ScalarWords scalar_division(ScalarDivision input, ScalarHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.ids) requires nonzero(host.reserve) {
    address context = host.context;
    ScalarDefinition type = scalar_definition(input.scalar);
    u32 denominator = input.denominator;
    if (denominator == 0) { denominator = host.constant(context, input.scalar, input.immediate); }
    u32 sign_shift = 0;
    if (type.signed != 0) { sign_shift = host.constant(context, input.scalar, u64(type.width - 1)); }
    u32 integer_type = scalar_id(host.types, input.scalar);
    u32 magnitude_type = scalar_id(host.types, unsigned_scalar(type.width));
    u32 next = host.ids(context, 0);
    address body = host.reserve(context, sign_shift == 0 ? 10u64 : 81u64, 0u64).body;
    DivisionEmission lowered = DivisionEmission(integer_type, magnitude_type, input.numerator,
        denominator, input.quotient, input.remainder, sign_shift, next);
    DivisionWords words = encode_integer_division(lowered, body);
    host.ids(context, words.next - next);
    return ScalarWords(u64(words.body), 0u64);
}

ScalarWords float_sign(BuiltinScalar scalar, u32 type, u32 result, u32 value, bool negate, ScalarHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.ids) requires nonzero(host.reserve) {
    address context = host.context;
    u32 width = scalar_definition(scalar).width;
    u64 sign = 1u64 << u64(width - 1);
    BuiltinScalar raw = unsigned_scalar(width);
    u32 raw_type = scalar_id(host.types, raw);
    u32 mask = host.constant(context, raw, negate ? sign : sign - 1u64);
    u32 bits = host.ids(context, 2);
    u32 changed = bits + 1;
    address body = host.reserve(context, 13u64, 0u64).body;
    u64 at = words_three(body, 0u64, 124, raw_type, bits, value);
    at = words_four(body, at, negate ? 198 : 199, raw_type, changed, bits, mask);
    return ScalarWords(words_three(body, at, 124, type, result, changed), 0u64);
}

ScalarWords scalar_unary(UnaryEmission input, ScalarHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.ids) requires nonzero(host.reserve) {
    if (input.operation == UnaryOperation::FloatNegate) {
        return float_sign(input.scalar, input.result_type, input.result, input.value, true, host);
    }
    if (input.operation == UnaryOperation::UnsignedNegate) {
        u32 zero = host.constant(host.context, input.scalar, 0u64);
        address body = host.reserve(host.context, 5u64, 0u64).body;
        return ScalarWords(words_four(body, 0u64, 130, input.result_type, input.result, zero, input.value), 0u64);
    }
    u32 opcode = input.operation == UnaryOperation::SignedNegate ? 126 :
        input.operation == UnaryOperation::LogicalNot ? 168 :
        input.operation == UnaryOperation::BitNot ? 200 : 124;
    address body = host.reserve(host.context, 4u64, 0u64).body;
    return ScalarWords(words_three(body, 0u64, opcode, input.result_type, input.result, input.value), 0u64);
}

u32 math_opcode(MathOperation operation) on cpu {
    if (operation == MathOperation::Floor) { return 8; }
    if (operation == MathOperation::Ceil) { return 9; }
    if (operation == MathOperation::Sin) { return 13; }
    if (operation == MathOperation::Cos) { return 14; }
    if (operation == MathOperation::Pow) { return 26; }
    if (operation == MathOperation::Exp) { return 27; }
    if (operation == MathOperation::Log) { return 28; }
    if (operation == MathOperation::Exp2) { return 29; }
    if (operation == MathOperation::Log2) { return 30; }
    if (operation == MathOperation::Sqrt) { return 31; }
    if (operation == MathOperation::Min) { return 37; }
    return 40;
}
// A zero right operand denotes unary math. Zero body words report unsupported f64 math.
ScalarWords scalar_math(MathEmission input, ScalarHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.ids) requires nonzero(host.reserve) {
    MathOperation op = input.operation;
    if (input.scalar == BuiltinScalar::F64 && (
        op == MathOperation::Sin || op == MathOperation::Cos || op == MathOperation::Pow ||
        op == MathOperation::Exp || op == MathOperation::Exp2 ||
        op == MathOperation::Log || op == MathOperation::Log2)) { return ScalarWords(0u64, 0u64); }
    if (input.operation == MathOperation::Abs) {
        return float_sign(input.scalar, input.result_type, input.result, input.left, false, host);
    }
    u32 length = input.right == 0 ? 6 : 7;
    address body = host.reserve(host.context, u64(length), 0u64).body;
    put(body, 0u64, instruction(12, length));
    put(body, 1u64, input.result_type);
    put(body, 2u64, input.result);
    put(body, 3u64, input.extended_set);
    put(body, 4u64, math_opcode(input.operation));
    put(body, 5u64, input.left);
    if (length == 7) { put(body, 6u64, input.right); }
    return ScalarWords(u64(length), 0u64);
}

u64 words_five(address body, u64 at, u32 opcode, u32 a, u32 b, u32 c, u32 d, u32 e) on cpu {
    put(body, at, instruction(opcode, 6));
    put(body, at + 1u64, a);
    put(body, at + 2u64, b);
    put(body, at + 3u64, c);
    put(body, at + 4u64, d);
    put(body, at + 5u64, e);
    return at + 6u64;
}

u64 select_words(address body, u64 at, u32 result_type, u32 result,
                            u32 condition, u32 yes, u32 no) on cpu {
    return words_five(body, at, 169, result_type, result, condition, yes, no);
}

extern "C" PageAddress prepare_page(PageInputs input, u32 result_pointer, ScalarHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.ids) {
    address context = host.context;
    u32 zero = 0;
    u32 one = 0;
    if (input.descriptor != 0) {
        zero = host.constant(context, BuiltinScalar::U32, 0u64);
        one = host.constant(context, BuiltinScalar::U32, 1u64);
    }
    u32 high_shift = host.constant(context, BuiltinScalar::U32, 32u64);
    u32 page_shift = host.constant(context, BuiltinScalar::U32, 3u64);
    u32 page_bytes = host.constant(context, BuiltinScalar::U32, u64(input.page_bytes));
    u32 word = input.has_word == 0 ? 0 : host.constant(context, BuiltinScalar::U32, u64(input.word));
    return PageAddress(scalar_id(host.types, BuiltinScalar::U32), scalar_id(host.types, BuiltinScalar::U64),
        input.descriptor_pointer, input.page_pointer, result_pointer, input.descriptor, input.low,
        input.high, input.stride, input.index, word, input.extra, zero, one, high_shift, page_shift,
        page_bytes, host.ids(context, 0));
}

struct PageOrigin { u64 words; u32 next; u32 table; u32 stride; u32 field; };

u64 dataset_header_word(address body, u64 at, PageAddress input, u32 field, u32 pointer, u32 value) on cpu {
    u64 end = words_five(body, at, 65, input.descriptor_pointer, pointer, input.descriptor, input.zero, field);
    return words_three(body, end, 61, input.u64_type, value, pointer);
}

PageOrigin page_origin(PageAddress input, address body, u32 first) on cpu {
    if (input.descriptor != 0) {
        u32 table = first + 1;
        u32 layout = first + 3;
        u64 at = dataset_header_word(body, 0u64, input, input.zero, first, table);
        at = dataset_header_word(body, at, input, input.one, first + 2, layout);
        u32 stride = first + 4;
        u32 high = first + 5;
        u32 field = first + 6;
        at = words_three(body, at, 113, input.u32_type, stride, layout);
        at = words_four(body, at, 194, input.u64_type, high, layout, input.high_shift);
        at = words_three(body, at, 113, input.u32_type, field, high);
        return PageOrigin(at, first + 7, table, stride, field);
    }
    u32 low = first;
    u32 high = first + 1;
    u32 shifted = first + 2;
    u32 table = first + 3;
    u64 at = words_three(body, 0u64, 113, input.u64_type, low, input.low);
    at = words_three(body, at, 113, input.u64_type, high, input.high);
    at = words_four(body, at, 196, input.u64_type, shifted, high, input.high_shift);
    at = words_four(body, at, 197, input.u64_type, table, low, shifted);
    return PageOrigin(at, first + 4, table, input.stride, 0);
}

struct PhysicalAddress { u64 words; u32 next; u32 value; };

PhysicalAddress physical_page_address(PageAddress input, address body, u32 start) on cpu {
    PageOrigin origin = page_origin(input, body, start);
    u32 first = origin.next;
    u32 capacity = first;
    u32 page = first + 1;
    u32 local = first + 2;
    u32 scaled = first + 3;
    u32 byte_offset = scaled;
    u32 next = first + 4;
    u32 t32 = input.u32_type;
    u32 t64 = input.u64_type;
    u64 at = words_four(body, origin.words, 134, t32, capacity, input.page_bytes, origin.stride);
    at = words_four(body, at, 134, t32, page, input.index, capacity);
    at = words_four(body, at, 137, t32, local, input.index, capacity);
    at = words_four(body, at, 132, t32, scaled, local, origin.stride);
    if (input.word != 0) {
        at = words_four(body, at, 128, t32, next, scaled, input.word);
        byte_offset = next;
        next += 1;
    }
    if (origin.field != 0) {
        at = words_four(body, at, 128, t32, next, byte_offset, origin.field);
        byte_offset = next;
        next += 1;
    }

    u32 page64 = next;
    u32 page_bytes = next + 1;
    u32 entry = next + 2;
    u32 page_pointer = next + 3;
    u32 base = next + 4;
    u32 offset64 = next + 5;
    u32 physical = next + 6;
    next += 7;
    at = words_three(body, at, 113, t64, page64, page);
    at = words_four(body, at, 196, t64, page_bytes, page64, input.page_shift);
    at = words_four(body, at, 128, t64, entry, origin.table, page_bytes);
    at = words_three(body, at, 120, input.page_pointer, page_pointer, entry);
    at = words_five(body, at, 61, t64, base, page_pointer, 2, 8);
    at = words_three(body, at, 113, t64, offset64, byte_offset);
    at = words_four(body, at, 128, t64, physical, base, offset64);
    if (input.extra != 0) {
        at = words_three(body, at, 113, t64, next, input.extra);
        at = words_four(body, at, 128, t64, next + 1, physical, next);
        physical = next + 1;
        next += 2;
    }
    return PhysicalAddress(at, next, physical);
}

extern "C" PageAddressResult page_address(PageAddress input, u32 step, address body) on cpu {
    PhysicalAddress address_value = physical_page_address(input, body, input.next);
    u32 next = address_value.next;
    u64 at = words_three(body, address_value.words, 120, input.result_pointer, next, address_value.value);
    if (step == 0) {
        return PageAddressResult(at, next + 1, next, 0);
    }
    u32 high = next + 1;
    at = words_four(body, at, 128, input.u64_type, high, address_value.value, step);
    at = words_three(body, at, 120, input.result_pointer, next + 2, high);
    return PageAddressResult(at, next + 3, next, next + 2);
}


struct StageCursor { address body; u64 words; u32 next; };

void copy_record(inout StageCursor out, address layout, u32 source_index, u32 target_index) on cpu {
    if (address_bits(layout) == 0) { return; }
    let input = unsafe_position<RecordCopy>(layout);
    PageAddress source = input.source;
    source.index = source_index;
    PageAddress target = source;
    target.descriptor = input.target_descriptor;
    target.index = target_index;
    address body = out.body;
    u64 start = out.words;
    PhysicalAddress from = physical_page_address(source, word_at(body, start), out.next);
    PhysicalAddress to = physical_page_address(target, word_at(body, start + from.words), from.next);
    u32 from_address = from.value;
    u32 to_address = to.value;
    u64 at = start + from.words + to.words;
    u32 next = to.next;
    for (unit in 0..input.count) {
        if (unit != 0) {
            at = words_four(body, at, 128, source.u64_type, next, from_address, input.step);
            at = words_four(body, at, 128, source.u64_type, next + 1, to_address, input.step);
            from_address = next;
            to_address = next + 1;
            next += 2;
        }
        u32 from_pointer = next;
        u32 to_pointer = next + 1;
        u32 value = next + 2;
        at = words_three(body, at, 120, source.result_pointer, from_pointer, from_address);
        at = words_three(body, at, 120, source.result_pointer, to_pointer, to_address);
        at = words_five(body, at, 61, input.unit_type, value, from_pointer, 2, input.unit_bytes);
        at = words_four(body, at, 62, to_pointer, value, 2, input.unit_bytes);
        next += 3;
    }
    out.words = at;
    out.next = next;
}

u32 read_word(address words, u32 part) on cpu {
    return unsafe_position<u32>(word_at(words, u64(part)));
}

u32 scalar_id(address ids, BuiltinScalar scalar) on cpu {
    return read_word(ids, u32(scalar));
}

u64 copy_shader_value(address body, u64 at, u32 type_id, u32 result, u32 value) on cpu {
    return words_three(body, at, 83, type_id, result, value);
}

extern "C" u64 plain_load(address body, u64 at, u32 type_id, u32 result, u32 pointer) on cpu {
    return words_three(body, at, 61, type_id, result, pointer);
}

extern "C" u64 plain_store(address body, u64 at, u32 pointer, u32 value) on cpu {
    return words_two(body, at, 62, pointer, value);
}

extern "C" u64 aligned_load(address body, u64 at, u32 type_id, u32 result, u32 pointer, u32 alignment) on cpu {
    return words_five(body, at, 61, type_id, result, pointer, 2, alignment);
}

u64 aligned_store(address body, u64 at, u32 pointer, u32 value, u32 alignment) on cpu {
    return words_four(body, at, 62, pointer, value, 2, alignment);
}

extern "C" u64 read_shader_index(u32 vector_type, u32 uint_type, u32 input, u32 global, u32 x, address body) on cpu {
    u64 at = plain_load(body, 0u64, vector_type, global, input);
    return words_four(body, at, 81, uint_type, x, global, 0);
}

u64 read_summary_words(address body, u32 type_id, u32 first, address pointers, u32 count) on cpu {
    u64 at = 0u64;
    for (part in 0..count) {
        at = aligned_load(body, at, type_id, first + part, read_word(pointers, part), 4);
    }
    return at;
}

u64 write_summary_words(address body, address pointers, address values, u32 count) on cpu {
    u64 at = 0u64;
    for (part in 0..count) {
        at = aligned_store(body, at, read_word(pointers, part), read_word(values, part), 4);
    }
    return at;
}

PayloadRead payload_read(PayloadEmission input, address operands, address shifts, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    if (input.access != PayloadAccess::Words) {
        for (part in 0..input.count) {
            u32 pointer = read_word(operands, part);
            if (input.access == PayloadAccess::Physical) {
                at = aligned_load(body, at, input.unit_type, next, pointer, input.unit_bytes);
            } else {
                at = plain_load(body, at, input.unit_type, next, pointer);
            }
            next += 1;
        }
    }
    u32 value = 0;
    for (part in 0..input.count) {
        u32 word = input.access == PayloadAccess::Words ? read_word(operands, part) : input.next + part;
        if (input.raw_type != input.unit_type) {
            at = words_three(body, at, 113, input.raw_type, next, word);
            word = next;
            next += 1;
        }
        if (part == 0) {
            value = word;
        } else {
            at = words_four(body, at, 196, input.raw_type, next, word, read_word(shifts, part));
            at = words_four(body, at, 197, input.raw_type, next + 1, value, next);
            value = next + 1;
            next += 2;
        }
    }
    return PayloadRead(at, next, value);
}

PayloadWrite typed_payload_read(PayloadEmission input, PayloadResult result, address operands, address shifts, address body) on cpu {
    PayloadRead read = payload_read(input, operands, shifts, body);
    if (result.compare_zero != 0) {
        return PayloadWrite(words_four(body, read.words, 171, result.type_id,
            result.id, read.value, result.compare_zero), read.next);
    }
    u32 opcode = result.type_id == input.raw_type ? 83 : 124;
    return PayloadWrite(words_three(body, read.words, opcode, result.type_id,
        result.id, read.value), read.next);
}

u32 uniform_word(inout StageCursor out, UniformRead input, u32 word) on cpu {
    u32 pointer = out.next;
    u32 value = pointer + 1;
    out.words = words_five(out.body, out.words, 65, input.push_pointer, pointer,
        input.push_block, input.zero, word);
    out.words = aligned_load(out.body, out.words, input.uint_type, value, pointer, 4);
    out.next = value + 1;
    if (input.byte_shift != 0) {
        u32 shifted = out.next;
        out.next += 1;
        out.words = words_four(out.body, out.words, 194, input.uint_type,
            shifted, value, input.byte_shift);
        return shifted;
    }
    return value;
}

PayloadWrite read_uniform(UniformRead input, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    u32 low = uniform_word(out, input, input.low_word);
    u32 high = 0;
    u32 count = 1;
    if (input.high_word != 0) {
        high = uniform_word(out, input, input.high_word);
        count = 2;
    }
    array<u32,2> words = array<u32,2>(low, high);
    array<u32,2> shifts = array<u32,2>(0, input.high_shift);
    PayloadEmission payload = PayloadEmission(PayloadAccess::Words, input.raw_type,
        input.uint_type, 4, count, out.next);
    PayloadWrite read = typed_payload_read(payload, input.result, address_of(words[0]),
        address_of(shifts[0]), word_at(body, out.words));
    return PayloadWrite(out.words + read.words, read.next);
}

u32 payload_word(inout StageCursor out, PayloadEmission input, u32 value, u32 shift) on cpu {
    u32 word = value;
    if (shift != 0) {
        word = stage_op(out, 194, input.raw_type, value, shift);
    }
    if (input.raw_type != input.unit_type) {
        u32 converted = out.next;
        out.words = words_three(out.body, out.words, 113, input.unit_type, converted, word);
        out.next += 1;
        word = converted;
    }
    return word;
}

PayloadWrite payload_write(PayloadEmission input, u32 value, address pointers, address shifts, address output, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    for (part in 0..input.count) {
        u32 word = payload_word(out, input, value, part == 0 ? 0 : read_word(shifts, part));
        if (input.access == PayloadAccess::Words) {
            put(output, u64(part), word);
        } else if (input.access == PayloadAccess::Physical) {
            out.words = aligned_store(body, out.words, read_word(pointers, part), word, input.unit_bytes);
        } else {
            out.words = plain_store(body, out.words, read_word(pointers, part), word);
        }
    }
    return PayloadWrite(out.words, out.next);
}

PayloadEmission prepare_payload(PayloadShape shape, inout array<u32,8> shifts, ScalarHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.ids) {
    address context = host.context;
    u32 width = scalar_definition(shape.unit).width;
    for (part in 1..shape.count) {
        put(address_of(shifts[0]), u64(part), host.constant(context, BuiltinScalar::U32, u64(part * width)));
    }
    BuiltinScalar raw = unsigned_scalar(scalar_definition(shape.scalar).width);
    return PayloadEmission(shape.access, scalar_id(host.types, raw), scalar_id(host.types, shape.unit),
        width / 8, shape.count, host.ids(context, 0));
}

u32 emit_payload_load(PayloadShape shape, address operands, u32 result, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) requires nonzero(host.finish_body) {
    ScalarHost scalars = host.scalars;
    address context = scalars.context;
    array<u32,8> shifts = array<u32,8>(0);
    PayloadEmission input = prepare_payload(shape, shifts, scalars);
    u64 capacity = 20u64 * u64(shape.count);
    if (result != 0) {
        u32 zero = shape.scalar == BuiltinScalar::Bool ? scalars.constant(context, BuiltinScalar::U32, 0u64) : 0;
        input.next = scalars.ids(context, 0);
        PayloadResult target = PayloadResult(scalar_id(scalars.types, shape.scalar), result, zero);
        address body = scalars.reserve(context, capacity + 5u64, 0u64).body;
        PayloadWrite read = typed_payload_read(input, target, operands, address_of(shifts[0]), body);
        host.finish_body(context, read.words);
        scalars.ids(context, read.next - input.next);
        return result;
    }
    address body = scalars.reserve(context, capacity, 0u64).body;
    PayloadRead read = payload_read(input, operands, address_of(shifts[0]), body);
    host.finish_body(context, read.words);
    scalars.ids(context, read.next - input.next);
    return read.value;
}

extern "C" void emit_payload_store(PayloadShape shape, u32 value, address pointers, address output, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) requires nonzero(host.finish_body) {
    ScalarHost scalars = host.scalars;
    address context = scalars.context;
    array<u32,8> shifts = array<u32,8>(0);
    PayloadEmission input = prepare_payload(shape, shifts, scalars);
    address body = scalars.reserve(context, 14u64 * u64(shape.count), 0u64).body;
    PayloadWrite emitted = payload_write(input, value, pointers, address_of(shifts[0]), output, body);
    host.finish_body(context, emitted.words);
    scalars.ids(context, emitted.next - input.next);
}

u32 encode_raw_bits(BuiltinScalar scalar, u32 value, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    BuiltinScalar raw = unsigned_scalar(scalar_definition(scalar).width);
    if (scalar == raw) { return value; }
    address context = host.scalars.context;
    u32 bits = host.scalars.ids(context, 1);
    u32 type_id = scalar_id(host.scalars.types, raw);
    u64 words = 0u64;
    if (scalar == BuiltinScalar::Bool) {
        u32 one = host.scalars.constant(context, BuiltinScalar::U32, 1u64);
        u32 zero = host.scalars.constant(context, BuiltinScalar::U32, 0u64);
        address body = host.scalars.reserve(context, 6u64, 0u64).body;
        words = select_words(body, 0u64, type_id, bits, value, one, zero);
    } else {
        address body = host.scalars.reserve(context, 4u64, 0u64).body;
        words = words_three(body, 0u64, 124, type_id, bits, value);
    }
    host.finish_body(context, words);
    return bits;
}

struct PhysicalShape { BuiltinScalar raw; BuiltinScalar unit; u32 bytes; u32 count; };

PhysicalShape physical_shape(BuiltinScalar scalar) on cpu {
    u32 width = scalar_definition(scalar).width;
    u32 unit = width > 32 ? 32 : width;
    return PhysicalShape(unsigned_scalar(width), unsigned_scalar(unit), unit / 8, width == 64 ? 2 : 1);
}

PageAddressResult locate_scalar(PageInputs input, PhysicalShape shape, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    u32 step = shape.count == 2 ? host.scalars.constant(context, BuiltinScalar::U64, u64(shape.bytes)) : 0;
    PageAddress page = prepare_page(input, scalar_id(host.scalars.physical_pointers, shape.unit), host.scalars);
    address body = host.scalars.reserve(context, shape.count == 2 ? 118u64 : 109u64, 0u64).body;
    PageAddressResult range = page_address(page, step, body);
    host.finish_body(context, range.words);
    host.scalars.ids(context, range.next - page.next);
    return range;
}

PayloadEmission scalar_payload(PhysicalShape shape, ScalarHost host) on cpu
    requires nonzero(host.ids) {
    return PayloadEmission(PayloadAccess::Physical, scalar_id(host.types, shape.raw),
        scalar_id(host.types, shape.unit), shape.bytes, shape.count, host.ids(host.context, 0));
}

extern "C" u32 physical_scalar_read(PhysicalAccess input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    PhysicalShape shape = physical_shape(input.scalar);
    PageAddressResult range = locate_scalar(input.page, shape, host);
    u32 high = shape.count == 2 ? host.scalars.constant(context, BuiltinScalar::U32, 32u64) : 0;
    array<u32,2> pointers = array<u32,2>(range.pointer, range.second);
    array<u32,2> shifts = array<u32,2>(0, high);
    PayloadEmission access = scalar_payload(shape, host.scalars);
    address body = host.scalars.reserve(context, 20u64 * u64(shape.count), 0u64).body;
    PayloadRead raw = payload_read(access, address_of(pointers[0]), address_of(shifts[0]), body);
    host.finish_body(context, raw.words);
    host.scalars.ids(context, raw.next - access.next);
    if (input.value != 0) {
        u32 zero = input.scalar == BuiltinScalar::Bool ? host.scalars.constant(context, BuiltinScalar::U32, 0u64) : 0;
        u32 next = host.scalars.ids(context, 0);
        u32 value = raw.value;
        PayloadEmission materialize = PayloadEmission(PayloadAccess::Words, access.raw_type, access.raw_type,
            shape.count * shape.bytes, 1, next);
        PayloadResult result = PayloadResult(input.value_type, input.value, zero);
        address output = host.scalars.reserve(context, 5u64, 0u64).body;
        PayloadWrite written = typed_payload_read(materialize, result, address_of(value), address_of(shifts[0]), output);
        host.finish_body(context, written.words);
        host.scalars.ids(context, written.next - next);
    }
    return raw.value;
}

extern "C" void physical_scalar_write(PhysicalAccess input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    PhysicalShape shape = physical_shape(input.scalar);
    // Keep the existing value and address emission order for direct and indexed stores.
    u32 value = input.value;
    if (input.page.extra == 0) { value = encode_raw_bits(input.scalar, value, host); }
    PageAddressResult range = locate_scalar(input.page, shape, host);
    if (input.page.extra != 0) { value = encode_raw_bits(input.scalar, value, host); }
    u32 high = shape.count == 2 ? host.scalars.constant(context, BuiltinScalar::U32, 32u64) : 0;
    array<u32,2> pointers = array<u32,2>(range.pointer, range.second);
    array<u32,2> shifts = array<u32,2>(0, high);
    PayloadEmission access = scalar_payload(shape, host.scalars);
    address body = host.scalars.reserve(context, 14u64 * u64(shape.count), 0u64).body;
    PayloadWrite written = payload_write(access, value, address_of(pointers[0]), address_of(shifts[0]), unsafe_address(0u64), body);
    host.finish_body(context, written.words);
    host.scalars.ids(context, written.next - access.next);
}

void pack_field(inout StageCursor out, PackedField field, u32 word_type, u32 zero,
    u32 byte_shift, u32 high_shift, address packed) on cpu {
    u32 first = field.offset / 4;
    u32 parts = (field.bytes + 3) / 4;
    PayloadEmission input = PayloadEmission(PayloadAccess::Words, field.raw_type, word_type, 4, parts, out.next);
    for (part in 0..parts) {
        u32 word = payload_word(out, input, field.value, part == 0 ? 0 : high_shift);
        if (byte_shift != 0) {
            word = stage_op(out, 196, word_type, word, byte_shift);
        }
        u32 slot = first + part;
        u32 previous = read_word(packed, slot);
        if (previous != zero) {
            word = stage_op(out, 197, word_type, previous, word);
        }
        put(packed, u64(slot), word);
    }
}

u64 open_conditional(address body, u64 at, u32 merge, u32 condition, u32 yes, u32 no) on cpu {
    u64 end = words_two(body, at, 247, merge, 0);
    return words_three(body, end, 250, condition, yes, no);
}

extern "C" ControlWords begin_shader_body(u32 result_type, u32 function, u32 signature, u32 entry, address body) on cpu {
    u64 at = words_four(body, 0u64, 54, result_type, function, 0, signature);
    return ControlWords(words_one(body, at, 248, entry), entry, 1);
}

extern "C" ControlWords end_shader_body(address body) on cpu {
    put(body, 0u64, instruction(253, 1));
    put(body, 1u64, instruction(56, 1));
    return ControlWords(2u64, 0, 0);
}

ControlWords selection_entry(address body, u64 at, u32 merge, u32 condition, u32 yes, u32 no) on cpu {
    u64 branch = open_conditional(body, at, merge, condition, yes, no);
    return ControlWords(words_one(body, branch, 248, yes), yes, 1);
}

extern "C" ControlWords open_selection(u32 merge, u32 condition, u32 yes, u32 no, address body) on cpu {
    return selection_entry(body, 0u64, merge, condition, yes, no);
}

ControlWords index_selection(address body, u64 at, u32 bool_type, u32 test,
    u32 index, u32 bound, bool less, u32 run, u32 end) on cpu {
    u64 compared = words_four(body, at, less ? 176 : 170, bool_type, test, index, bound);
    return selection_entry(body, compared, end, test, run, end);
}

ControlWords open_index_selection(u32 bool_type, u32 test, u32 index, u32 bound,
    bool less, u32 run, u32 end, address body) on cpu {
    return index_selection(body, 0u64, bool_type, test, index, bound, less, run, end);
}

ControlWords open_tile_padding(u32 uint_type, u32 bool_type, u32 lane, u32 width,
    u32 capacity, u32 index, u32 test, u32 run, u32 end, address body) on cpu {
    u64 at = words_four(body, 0u64, 128, uint_type, index, lane, width);
    return index_selection(body, at, bool_type, test, index, capacity, true, run, end);
}

u64 branch_if_reached(address body, u64 at, u32 target, u32 reaches) on cpu {
    return reaches != 0 ? words_one(body, at, 249, target) : at;
}

extern "C" ControlWords next_control_block(u32 target, u32 label, u32 reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, target, reaches);
    return ControlWords(words_one(body, at, 248, label), label, 1);
}

ControlWords close_selection(u32 merge, u32 yes_reaches, u32 no_reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, merge, no_reaches);
    at = words_one(body, at, 248, merge);
    u32 reaches = yes_reaches | no_reaches;
    if (reaches == 0) {
        put(body, at, instruction(255, 1));
        at += 1u64;
    }
    return ControlWords(at, merge, reaches);
}

struct ReturnEdges { EdgeStorage storage; usize used; };
struct ControlTargets {
    u32 end; u32 next;
    u32 function_merge; u32 return_loop;
};

void record_return_edge(inout ReturnEdges edges, u32 label, u32 returned, StructuredHost host) on cpu
    requires nonzero(host.reserve_edges) {
    usize at = edges.used;
    usize needed = at + 2u64;
    if (needed > edges.storage.capacity) { edges.storage = host.reserve_edges(host.context, at, needed); }
    put(edges.storage.words, at, label);
    put(edges.storage.words, at + 1u64, returned);
    edges.used = needed;
}

struct LoopLabels { u32 header; u32 check; u32 run; u32 next; u32 end; };

ControlWords open_loop(LoopLabels labels, address body) on cpu {
    u64 at = words_one(body, 0u64, 249, labels.header);
    at = words_one(body, at, 248, labels.header);
    at = words_three(body, at, 246, labels.end, labels.next, 0);
    at = words_one(body, at, 249, labels.check);
    return ControlWords(words_one(body, at, 248, labels.check), labels.check, 1);
}

ControlWords test_loop(LoopLabels labels, u32 test, address body) on cpu {
    u64 at = words_three(body, 0u64, 250, test, labels.run, labels.end);
    return ControlWords(words_one(body, at, 248, labels.run), labels.run, 1);
}

ControlWords close_loop(LoopLabels labels, u32 reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, labels.next, reaches);
    at = words_one(body, at, 248, labels.next);
    at = words_one(body, at, 249, labels.header);
    return ControlWords(words_one(body, at, 248, labels.end), labels.end, 1);
}

ControlWords open_function_scope(u32 merge, u32 selector, u32 begin, address body) on cpu {
    u64 at = words_two(body, 0u64, 247, merge, 0);
    at = words_two(body, at, 251, selector, begin);
    return ControlWords(words_one(body, at, 248, begin), begin, 1);
}

ControlWords leave_control(u32 target, address body) on cpu {
    return ControlWords(words_one(body, 0u64, 249, target), 0, 0);
}

struct LoopReturn {
    u32 bool_type;
    u32 result;
    u32 target;
    u32 merge;
    u32 not_returned;
    u32 returned;
    u32 edge_count;
};

ControlWords dispatch_loop_return(LoopReturn input, address edges, address body) on cpu {
    put(body, 0u64, instruction(245, 3 + 2 * input.edge_count));
    put(body, 1u64, input.bool_type);
    put(body, 2u64, input.result);
    u64 at = 3u64;
    for (edge in 0..input.edge_count) {
        u32 label = read_word(edges, edge * 2);
        u32 returned = read_word(edges, edge * 2 + 1);
        put(body, at, returned != 0 ? input.returned : input.not_returned);
        put(body, at + 1u64, label);
        at += 2u64;
    }
    at = open_conditional(body, at, input.merge, input.result, input.target, input.merge);
    return ControlWords(words_one(body, at, 248, input.merge), input.merge, 1);
}

u32 block_register(address registers, usize reg) on cpu {
    return unsafe_position<u32>(unsafe_address(address_bits(registers) + u64(reg) * 4u64));
}

void block_float_constant(View instruction, BlockValues values, MemoryHost memory) on cpu
    requires nonzero(values.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost host = memory.scalars;
    address context = host.context;
    BuiltinScalar scalar = values.scalar(context, instruction.a);
    u32 count = BuiltinScalar(u32(instruction.c)) == BuiltinScalar::F64 ? 2 : 1;
    array<u32,2> words = array<u32,2>(host.constant(context, BuiltinScalar::U32, u64(u32(instruction.b))), 0);
    if (count == 2) { words[1] = host.constant(context, BuiltinScalar::U32, u64(instruction.b) >> 32u64); }
    PayloadShape shape = PayloadShape(PayloadAccess::Words, scalar, BuiltinScalar::U32, count);
    emit_payload_load(shape, address_of(words[0]), block_register(values.registers, instruction.a), memory);
}

void block_uniform(View instruction, BlockValues values) on cpu
    requires nonzero(values.scalar) requires nonzero(values.finish)
    requires nonzero(values.scalars.constant)
    requires nonzero(values.scalars.ids) requires nonzero(values.scalars.reserve) {
    ScalarHost host = values.scalars;
    address context = host.context;
    BuiltinScalar scalar = values.scalar(context, instruction.a);
    u32 width = scalar_definition(scalar).width;
    u32 offset = u32(instruction.b);
    u32 zero = host.constant(context, BuiltinScalar::U32, 0u64);
    u32 word = offset / 4;
    u32 low_word = host.constant(context, BuiltinScalar::U32, u64(word));
    u32 shift = offset % 4 * 8;
    u32 byte_shift = shift == 0 ? 0 : host.constant(context, BuiltinScalar::U32, u64(shift));
    u32 high_word = 0;
    u32 high_shift = 0;
    if (width == 64) {
        high_word = host.constant(context, BuiltinScalar::U32, u64(word + 1));
        high_shift = host.constant(context, BuiltinScalar::U32, 32u64);
    }
    u32 next = host.ids(context, 0);
    PayloadResult result = PayloadResult(scalar_id(host.types, scalar), block_register(values.registers, instruction.a),
        scalar == BuiltinScalar::Bool ? zero : 0);
    UniformRead input = UniformRead(next, values.push_pointer, values.push_block,
        scalar_id(host.types, BuiltinScalar::U32), scalar_id(host.types, unsigned_scalar(width)),
        result, zero, low_word, high_word, byte_shift, high_shift);
    PayloadWrite emitted = read_uniform(input, host.reserve(context, 57u64, 0u64).body);
    values.finish(context, ScalarWords(emitted.words, 0u64));
    host.ids(context, emitted.next - next);
}

ScalarWords block_calculation(View instruction, BlockValues values, Host ir) on cpu
    requires nonzero(ir.inspect) requires nonzero(values.scalar) requires nonzero(values.binary)
    requires nonzero(values.scalars.constant)
    requires nonzero(values.scalars.ids) requires nonzero(values.scalars.reserve) {
    ScalarHost host = values.scalars;
    address context = host.context;
    address registers = values.registers;
    if (instruction.tag == OP_INTEGER_DIVIDE) {
        View result = ir.inspect(instruction.right);
        View divisor = ir.inspect(instruction.left);
        u32 quotient = result.tag == 1 ? 0 : block_register(registers, result.a);
        u32 remainder = result.tag == 0 ? 0 : block_register(registers, result.tag == 1 ? result.a : result.b);
        u32 denominator = divisor.tag == 0 ? 0 : block_register(registers, divisor.a);
        u64 immediate = divisor.tag == 0 ? u64(divisor.a) : 0u64;
        ScalarDivision input = ScalarDivision(values.scalar(context, instruction.a), block_register(registers, instruction.a),
            denominator, quotient, remainder, immediate);
        return scalar_division(input, host);
    }
    BuiltinScalar dest = values.scalar(context, instruction.a);
    u32 type_id = scalar_id(host.types, dest);
    u32 result = block_register(registers, instruction.a);
    if (instruction.tag == OP_MATH) {
        usize first = unsafe_position<usize>(instruction.first.data);
        u32 left = block_register(registers, first);
        u32 right = 0;
        if (instruction.first.count > 1u64) {
            usize second = unsafe_position<usize>(unsafe_address(address_bits(instruction.first.data) + instruction.first.stride));
            right = block_register(registers, second);
        }
        return scalar_math(MathEmission(MathOperation(u32(instruction.b)), dest, type_id, result,
            left, right, values.extended_set), host);
    }
    u32 source = block_register(registers, instruction.b);
    if (instruction.tag == OP_SELECT_VALUE) {
        address body = host.reserve(context, 6u64, 0u64).body;
        return ScalarWords(select_words(body, 0u64, type_id, result, source,
            block_register(registers, instruction.c), block_register(registers, instruction.d)), 0u64);
    }
    BuiltinScalar scalar = values.scalar(context, instruction.b);
    if (instruction.tag == OP_UNARY) {
        return scalar_unary(UnaryEmission(UnaryOperation(u32(instruction.c)), scalar, type_id, result, source), host);
    }
    if (instruction.tag == OP_CAST) {
        return scalar_cast(CastEmission(scalar, dest, type_id, result, source), host);
    }
    return scalar_binary(BinaryEmission(values.binary(instruction.right.data), scalar, type_id, result,
        source, block_register(registers, instruction.c)), host);
}

PageInputs block_dataset_page(BlockPaging paging, usize binding) on cpu {
    PageInputs input = paging.base;
    input.descriptor = block_register(paging.descriptors, binding);
    return input;
}

PageInputs block_view_page(PageInputs base, address registers, Span table, View stride, ScalarHost scalars) on cpu
    requires nonzero(scalars.constant) {
    PageInputs input = base;
    usize low = unsafe_position<usize>(table.data);
    usize high = unsafe_position<usize>(unsafe_address(address_bits(table.data) + u64(table.stride)));
    input.low = block_register(registers, low);
    input.high = block_register(registers, high);
    input.stride = stride.tag == 0 ? scalars.constant(scalars.context, BuiltinScalar::U32, u64(u32(stride.a))) :
        block_register(registers, unsafe_position<usize>(stride.first.data));
    return input;
}

void block_physical_access(PageInputs page, usize reg, bool write, BlockValues values, MemoryHost memory) on cpu
    requires nonzero(values.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    BuiltinScalar scalar = values.scalar(memory.scalars.context, reg);
    u32 type_id = write ? 0 : scalar_id(memory.scalars.types, scalar);
    PhysicalAccess access = PhysicalAccess(page, scalar, block_register(values.registers, reg), type_id);
    if (write) {
        physical_scalar_write(access, memory);
    } else {
        physical_scalar_read(access, memory);
    }
}

void block_append(View instruction, BlockValues values, BlockPaging paging, u32 ballot_type,
    MemoryHost memory, StructuredHost control) on cpu
    requires nonzero(values.scalar) requires nonzero(paging.current_label)
    requires nonzero(control.reserve_words) requires nonzero(control.finish_words)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    ScalarHost scalars = memory.scalars;
    address context = scalars.context;
    u32 relaxed = scalars.constant(context, BuiltinScalar::U32, 0u64);
    PageInputs count = block_dataset_page(paging, instruction.b);
    count.index = relaxed;
    count.word = u32(instruction.d);
    count.has_word = 1;
    u32 counter = locate_scalar(count, physical_shape(BuiltinScalar::U32), memory).pointer;
    u32 device = scalars.constant(context, BuiltinScalar::U32, 1u64);
    u32 subgroup = scalars.constant(context, BuiltinScalar::U32, 3u64);
    u32 active = scalars.constant(context, BuiltinScalar::Bool, 1u64);
    u32 capacity = scalars.constant(context, BuiltinScalar::U32, u64(2 + u32(instruction.c)));
    u32 outer = paging.current_label(context);
    u32 next = scalars.ids(context, 0);
    AppendEmission input = AppendEmission(scalar_id(scalars.types, BuiltinScalar::U32),
        scalar_id(scalars.types, BuiltinScalar::Bool), ballot_type,
        device, relaxed, subgroup, active, counter, values.push_pointer, values.push_block,
        capacity, outer, block_register(values.registers, instruction.a), next);
    AppendWords result = encode_append_reservation(input, scalars.reserve(context, 173u64, 0u64).body);
    control.finish_words(control.context, ControlWords(result.words, result.write_label, 1));
    scalars.ids(context, result.next - next);

    PageInputs page = block_dataset_page(paging, instruction.c);
    page.index = result.ticket;
    page.has_word = 1;
    usize field = 0u64;
    while (field < instruction.first.count) {
        address reg = unsafe_address(address_bits(instruction.first.data) + u64(field) * u64(instruction.first.stride));
        address byte = unsafe_address(address_bits(instruction.second.data) + u64(field) * u64(instruction.second.stride));
        page.word = unsafe_position<u32>(byte);
        block_physical_access(page, unsafe_position<usize>(reg), true, values, memory);
        field += 1u64;
    }
    control.finish_words(control.context, next_control_block(result.finish, result.finish, 1,
        control.reserve_words(control.context, 4u64)));
}

struct PrivateField { u32 variable; u32 pointer_type; BuiltinScalar unit; u32 offset; };

PrivateField private_array_field(BlockLocals locals, usize base, Host ir) on cpu requires nonzero(ir.inspect) {
    usize low = 0u64;
    usize high = locals.arrays.count;
    while (low < high) {
        usize mid = low + (high - low) / 2u64;
        PrivateArray array = unsafe_position<PrivateArray>(private_array_at(locals.arrays.data, mid));
        if (array.end <= base) { low = mid + 1u64; }
        else { high = mid; }
    }
    PrivateArray array = unsafe_position<PrivateArray>(private_array_at(locals.arrays.data, low));
    usize start = array.end - array.components.count;
    View component = function_component(array.components, base - start, ir);
    u32 pointer = scalar_id(locals.pointers, array.unit);
    return PrivateField(array.variable, pointer, array.unit, u32(component.a));
}

void block_private_storage(View instruction, BlockValues values, BlockLocals locals, MemoryHost memory, Host ir) on cpu
    requires nonzero(values.scalar) requires nonzero(ir.inspect)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost scalars = memory.scalars;
    address context = scalars.context;
    bool indexed = instruction.tag == OP_LOAD_LOCAL_INDEXED || instruction.tag == OP_STORE_LOCAL_INDEXED;
    bool write = instruction.tag == OP_STORE_LOCAL || instruction.tag == OP_STORE_LOCAL_INDEXED;
    usize base = write ? instruction.a : instruction.b;
    usize reg = write ? (indexed ? instruction.d : instruction.b) : instruction.a;
    BuiltinScalar scalar = values.scalar(context, reg);
    u32 value = block_register(values.registers, reg);
    if (!indexed) {
        u32 pointer = block_register(locals.slots, base);
        if (pointer != 0) {
            address body = scalars.reserve(context, 4u64, 0u64).body;
            u64 words = write ? plain_store(body, 0u64, pointer, value) :
                plain_load(body, 0u64, scalar_id(scalars.types, scalar), value, pointer);
            memory.finish_body(context, words);
            return;
        }
    }

    PrivateField field = private_array_field(locals, base, ir);
    u32 bytes = scalar_definition(field.unit).width / 8;
    u32 scalar_bytes = scalar_definition(scalar).width / 8;
    u32 count = (scalar_bytes + bytes - 1) / bytes;
    u32 byte_offset = field.offset + (indexed ? read_word(instruction.first.data, 0) : 0);
    array<u32,8> parts = array<u32,8>(0);
    address slots = address_of(parts[0]);
    u32 index = 0;
    u32 offset = 0;
    u32 width = 0;
    if (indexed) {
        offset = byte_offset == 0 ? 0 : scalars.constant(context, BuiltinScalar::U32, u64(byte_offset));
        width = bytes == 1 ? 0 : scalars.constant(context, BuiltinScalar::U32, u64(bytes));
        for (part in 1..count) {
            put(slots, u64(part), scalars.constant(context, BuiltinScalar::U32, u64(part)));
        }
        index = block_register(values.registers, write ? instruction.c : instruction.d);
    } else {
        u32 first = byte_offset / bytes;
        for (part in 0..count) {
            put(slots, u64(part), scalars.constant(context, BuiltinScalar::U32, u64(first + part)));
        }
    }
    u32 next = scalars.ids(context, 0);
    PrivateAddress input = PrivateAddress(scalar_id(scalars.types, BuiltinScalar::U32), field.pointer_type,
        field.variable, index, offset, width, count, next);
    address body = scalars.reserve(context, 10u64 * u64(count) + 5u64, 0u64).body;
    // Each address replaces its part index after that index has been consumed.
    PrivateAddressWords emitted = private_address(input, slots, slots, body);
    memory.finish_body(context, emitted.words);
    scalars.ids(context, emitted.next - next);

    PayloadShape storage = PayloadShape(PayloadAccess::Local, scalar, field.unit, count);
    if (write) {
        u32 raw = encode_raw_bits(scalar, value, memory);
        emit_payload_store(storage, raw, slots, address(), memory);
    } else {
        u32 raw = emit_payload_load(storage, slots, 0, memory);
        BuiltinScalar unit = unsigned_scalar(scalar_definition(scalar).width);
        PayloadShape result = PayloadShape(PayloadAccess::Words, scalar, unit, 1);
        emit_payload_load(result, address_of(raw), value, memory);
    }
}

void finish_return_loop(ControlTargets targets, usize first, u32 predecessor, inout ReturnEdges scratch,
    StructuredHost host, ScalarHost scalars) on cpu
    requires nonzero(host.labels) requires nonzero(host.reserve_words) requires nonzero(host.finish_words)
    requires nonzero(host.reserve_edges) requires nonzero(scalars.constant) {
    address context = host.context;
    address edges = word_at(scratch.storage.words, first);
    u32 count = u32((scratch.used - first) / 2u64);
    u32 result = host.labels(context, 1);
    u32 not_returned = 0;
    u32 returned = 0;
    for (edge in 0..count) {
        if (read_word(edges, edge * 2 + 1) == 0) {
            if (not_returned == 0) { not_returned = scalars.constant(scalars.context, BuiltinScalar::Bool, 0u64); }
        } else {
            if (returned == 0) { returned = scalars.constant(scalars.context, BuiltinScalar::Bool, 1u64); }
        }
        if (not_returned != 0 && returned != 0) { break; }
    }
    u32 target = targets.return_loop != 0 ? targets.return_loop : targets.function_merge;
    u32 bool_type = scalar_id(scalars.types, BuiltinScalar::Bool);
    u32 merge = host.labels(context, 1);
    LoopReturn input = LoopReturn(bool_type, result, target, merge, not_returned, returned, count);
    host.finish_words(context, dispatch_loop_return(input, edges, host.reserve_words(context, 12u64 + 2u64 * u64(count))));
    // The child Phi consumed these pairs. Reuse that space for its parent predecessor.
    scratch.used = first;
    if (targets.return_loop != 0) {
        record_return_edge(scratch, predecessor, 1, host);
    }
}

BlockFlow emit_leaf(View value, StructuredHost host, BlockHost emit) on cpu
    requires nonzero(emit.ir.inspect) requires nonzero(emit.reject)
    requires nonzero(emit.textures.declaration) requires nonzero(emit.rays.declare_type)
    requires nonzero(emit.tile.components)
    requires nonzero(emit.tile.cell)
    requires nonzero(emit.paging.finish_body)
    requires nonzero(emit.paging.current_label)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.binary) requires nonzero(emit.values.finish)
    requires nonzero(emit.values.scalars.constant)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve)
    requires nonzero(host.labels) requires nonzero(host.reserve_words) requires nonzero(host.finish_words) {
    address context = host.context;
    MemoryHost memory = MemoryHost(emit.values.scalars, emit.paging.finish_body);
    u32 kind = value.tag;
    if (kind == OP_PRINT) {
        return emit.reject(context, BlockRejection::Print);
    } else if (kind == OP_INVOKE || kind == OP_RAISE || kind == OP_CLEANUP) {
        return emit.reject(context, BlockRejection::CpuCall);
    } else if (kind == OP_TEXTURE_SAMPLE) {
        block_texture_sample(value, emit.ir.inspect(value.left), emit.values, emit.textures);
    } else if (kind <= OP_TEXTURE_FETCH) {
        return emit.reject(context, BlockRejection::CpuTexture);
    } else if (kind == OP_APPEND) {
        block_append(value, emit.values, emit.paging, emit.subgroup.ballot_type, memory, host);
    } else if (kind == OP_ATOMIC) {
        View target = emit.ir.inspect(value.left);
        if (target.tag != INDEXED_DATA && target.tag != INDEXED_TILE) {
            return emit.reject(context, BlockRejection::AtomicAddress);
        }
        block_atomic(value, target, emit.ir.inspect(value.right), emit, memory);
    } else if (kind == OP_TILE_BARRIER) {
        tile_workgroup_sync(0x148, memory);
    } else if (kind == OP_TILE_PUBLISH) {
        block_tile_publish(value, emit.values, emit.tile, memory);
    } else if (kind == OP_TILE_COLLECTIVE) {
        block_tile_collective(value, emit, memory, host);
    } else if (kind == OP_TILE_LENGTH || kind == OP_TILE_LANE) {
        ScalarHost stream = memory.scalars;
        u32 type_id = scalar_id(stream.types, emit.values.scalar(stream.context, value.a));
        u32 result = block_register(emit.values.registers, value.a);
        u32 id = kind == OP_TILE_LANE ? emit.tile.lane : emit.tile.length;
        address body = stream.reserve(stream.context, 4u64, 0u64).body;
        memory.finish_body(stream.context, copy_shader_value(body, 0u64, type_id, result, id));
    } else if (kind == OP_TILE_LOAD) {
        TileKey key = tile_key(emit.tile.tiles, value.b, u32(value.d), emit.ir);
        u32 index = block_register(emit.values.registers, value.c);
        block_tile_read(emit.values, emit.tile, key, index, 0, value.a, memory);
    } else if (kind == OP_SUBGROUP) {
        block_subgroup(value, emit.values, emit.subgroup, memory);
    } else if (kind == OP_VIEW_LOAD) {
        PageInputs page = block_view_page(emit.paging.base, emit.values.registers, value.first,
            emit.ir.inspect(value.left), emit.values.scalars);
        page.index = block_register(emit.values.registers, value.b);
        page.word = u32(value.c);
        page.has_word = 1;
        block_physical_access(page, value.a, false, emit.values, memory);
    } else if (kind == OP_RAY_QUERY) {
        block_ray_query(value, emit.ir.inspect(value.left).first, emit.values, emit.rays, host);
    } else if (kind == OP_POSITION_ADDRESS || kind == OP_BUFFER_STRIDE) {
        return emit.reject(context, BlockRejection::CpuAddress);
    } else if (kind == OP_LOAD_INDEXED || kind == OP_STORE_INDEXED) {
        View source = emit.ir.inspect(value.left);
        if (source.tag == INDEXED_POINTER) {
            return emit.reject(context, BlockRejection::CpuAddress);
        }
        bool write = kind == OP_STORE_INDEXED;
        if (source.tag == INDEXED_TILE) {
            TileKey key = tile_key(emit.tile.tiles, source.a, u32(value.c), emit.ir);
            u32 index = block_register(emit.values.registers, source.b);
            u32 offset = block_register(emit.values.registers, write ? value.a : value.b);
            if (write) {
                block_tile_write(emit.values, emit.tile, key, index, offset, value.b, memory);
            } else {
                block_tile_read(emit.values, emit.tile, key, index, offset, value.a, memory);
            }
        } else {
            address registers = emit.values.registers;
            PageInputs page = emit.paging.base;
            if (source.tag == INDEXED_DATA) {
                page = block_dataset_page(emit.paging, source.a);
                page.index = block_register(registers, source.b);
            } else {
                page = block_view_page(page, registers, source.first, emit.ir.inspect(source.left), emit.values.scalars);
                page.index = block_register(registers, source.a);
            }
            page.word = u32(value.c);
            page.has_word = 1;
            page.extra = block_register(registers, write ? value.a : value.b);
            block_physical_access(page, write ? value.b : value.a, write, emit.values, memory);
        }
    } else if (kind == OP_FUNCTION_ADDRESS) {
        return emit.reject(context, BlockRejection::CpuFunction);
    } else if (kind == OP_CONSTANT) {
        BuiltinScalar scalar = BuiltinScalar(u32(value.c));
        if (scalar == BuiltinScalar::F32 || scalar == BuiltinScalar::F64) {
            block_float_constant(value, emit.values, memory);
        } else {
            ScalarHost stream = emit.values.scalars;
            put(emit.values.registers, u64(value.a), stream.constant(stream.context, scalar, u64(value.b)));
        }
    } else if (kind == OP_INDEX_VALUE) {
        ScalarHost stream = emit.values.scalars;
        u32 type_id = scalar_id(stream.types, emit.values.scalar(stream.context, value.a));
        u32 result = block_register(emit.values.registers, value.a);
        address body = stream.reserve(stream.context, 4u64, 0u64).body;
        u64 length = copy_shader_value(body, 0u64, type_id, result, emit.values.index);
        emit.values.finish(stream.context, ScalarWords(length, 0u64));
    } else if (kind == OP_UNIFORM) {
        block_uniform(value, emit.values);
    } else if (kind >= OP_LOAD_LOCAL && kind <= OP_STORE_LOCAL_INDEXED) {
        block_private_storage(value, emit.values, emit.locals, memory, emit.ir);
    } else if (kind == OP_LOAD || kind == OP_STORE) {
        bool write = kind == OP_STORE;
        PageInputs page = block_dataset_page(emit.paging, write ? value.a : value.b);
        page.index = block_register(emit.values.registers, write ? value.b : value.c);
        page.word = u32(write ? value.c : value.d);
        page.has_word = 1;
        block_physical_access(page, write ? value.d : value.a, write, emit.values, memory);
    } else if (kind >= OP_UNARY && kind <= OP_SELECT_VALUE) {
        ScalarWords written = block_calculation(value, emit.values, emit.ir);
        if (kind == OP_MATH && written.body == 0u64) {
            return emit.reject(context, BlockRejection::FloatMath);
        }
        emit.values.finish(emit.values.scalars.context, written);
    }
    return BlockFlow::Next;
}

BlockFlow emit_block(Span code, ControlTargets targets, inout ReturnEdges edges, StructuredHost host, BlockHost emit) on cpu
    requires nonzero(emit.ir.inspect) requires nonzero(emit.reject)
    requires nonzero(emit.textures.declaration) requires nonzero(emit.rays.declare_type)
    requires nonzero(emit.tile.components)
    requires nonzero(emit.tile.cell)
    requires nonzero(emit.paging.finish_body)
    requires nonzero(emit.paging.current_label)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.binary) requires nonzero(emit.values.finish)
    requires nonzero(emit.values.scalars.constant)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve)
    requires nonzero(host.labels) requires nonzero(host.reserve_words) requires nonzero(host.finish_words)
    requires nonzero(host.reserve_edges) {
    address context = host.context;
    ScalarHost scalars = emit.values.scalars;
    usize i = 0u64;
    while (i < code.count) {
        address instruction = unsafe_address(address_bits(code.data) + i * code.stride);
        View value = emit.ir.inspect(Node(instruction, N_INSTRUCTION));
        u32 kind = value.tag;
        if (kind == OP_SCOPE) {
            u32 merge = host.labels(context, 2);
            u32 selector = scalars.constant(scalars.context, BuiltinScalar::U32, 0u64);
            host.finish_words(context, open_function_scope(merge, selector, merge + 1, host.reserve_words(context, 8u64)));
            BlockFlow flow = emit_block(value.first, ControlTargets(0, 0, merge, 0), edges, host, emit);
            if (flow == BlockFlow::Error) { return flow; }
            host.finish_words(context, next_control_block(merge, merge, u32(flow == BlockFlow::Next),
                host.reserve_words(context, 4u64)));
        } else if (kind == OP_EXIT_VALUE) {
            u32 destination = targets.function_merge;
            if (targets.return_loop != 0) {
                u32 label = emit.paging.current_label(scalars.context);
                record_return_edge(edges, label, 1, host);
                destination = targets.return_loop;
            }
            host.finish_words(context, leave_control(destination, host.reserve_words(context, 2u64)));
            return BlockFlow::Stop;
        } else if (kind == OP_BRANCH) {
            u32 yes = host.labels(context, 3);
            u32 no = yes + 1;
            u32 merge = yes + 2;
            u32 condition = block_register(emit.values.registers, value.a);
            host.finish_words(context, open_selection(merge, condition, yes, no, host.reserve_words(context, 9u64)));
            BlockFlow first = emit_block(value.first, targets, edges, host, emit);
            if (first == BlockFlow::Error) { return first; }
            u32 yes_reaches = u32(first == BlockFlow::Next);
            host.finish_words(context, next_control_block(merge, no, yes_reaches, host.reserve_words(context, 4u64)));
            BlockFlow second = emit_block(value.second, targets, edges, host, emit);
            if (second == BlockFlow::Error) { return second; }
            ControlWords close = close_selection(merge, yes_reaches, u32(second == BlockFlow::Next),
                host.reserve_words(context, 4u64));
            host.finish_words(context, close);
            if (close.reaches_next == 0) { return BlockFlow::Stop; }
        } else if (kind == OP_LOOP_VALUE) {
            u32 head = host.labels(context, 5);
            LoopLabels labels = LoopLabels(head, head + 1, head + 2, head + 3, head + 4);
            bool returning = targets.function_merge != 0 && has_function_exit(value.second, emit.ir) != 0;
            usize first = returning ? edges.used : 0u64;
            ControlTargets inner = ControlTargets(labels.end, labels.next, targets.function_merge,
                returning ? labels.end : targets.return_loop);
            host.finish_words(context, open_loop(labels, host.reserve_words(context, 12u64)));
            BlockFlow condition = emit_block(value.first, inner, edges, host, emit);
            if (condition == BlockFlow::Error) { return condition; }
            if (returning) {
                u32 label = emit.paging.current_label(scalars.context);
                record_return_edge(edges, label, 0, host);
            }
            u32 test = block_register(emit.values.registers, value.a);
            host.finish_words(context, test_loop(labels, test, host.reserve_words(context, 6u64)));
            BlockFlow flow = emit_block(value.second, inner, edges, host, emit);
            if (flow == BlockFlow::Error) { return flow; }
            host.finish_words(context, close_loop(labels, u32(flow == BlockFlow::Next), host.reserve_words(context, 8u64)));
            if (returning) { finish_return_loop(targets, first, labels.end, edges, host, scalars); }
        } else if (kind == OP_BREAK_VALUE) {
            if (targets.return_loop == targets.end && targets.end != 0) {
                u32 label = emit.paging.current_label(scalars.context);
                record_return_edge(edges, label, 0, host);
            }
            host.finish_words(context, leave_control(targets.end, host.reserve_words(context, 2u64)));
            return BlockFlow::Stop;
        } else if (kind == OP_CONTINUE_VALUE) {
            host.finish_words(context, leave_control(targets.next, host.reserve_words(context, 2u64)));
            return BlockFlow::Stop;
        } else {
            BlockFlow flow = emit_leaf(value, host, emit);
            if (flow != BlockFlow::Next) { return flow; }
        }
        i += 1u64;
    }
    return BlockFlow::Next;
}

extern "C" BlockFlow emit_instruction_block(Span code, StructuredHost host, BlockHost emit) on cpu
    requires nonzero(emit.ir.inspect) requires nonzero(emit.reject)
    requires nonzero(emit.textures.declaration) requires nonzero(emit.rays.declare_type)
    requires nonzero(emit.tile.components)
    requires nonzero(emit.tile.cell)
    requires nonzero(emit.paging.finish_body)
    requires nonzero(emit.paging.current_label)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.binary) requires nonzero(emit.values.finish)
    requires nonzero(emit.values.scalars.constant)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve)
    requires nonzero(host.labels) requires nonzero(host.reserve_words) requires nonzero(host.finish_words)
    requires nonzero(host.reserve_edges) {
    ReturnEdges edges = ReturnEdges(EdgeStorage(address(), 0u64), 0u64);
    return emit_block(code, ControlTargets(0, 0, 0, 0), edges, host, emit);
}

extern "C" void emit_tile_phases(u32 active, StructuredHost control, TilePhaseHost host, BlockHost emit) on cpu
    requires nonzero(emit.ir.inspect) requires nonzero(emit.reject)
    requires nonzero(emit.textures.declaration) requires nonzero(emit.rays.declare_type)
    requires nonzero(emit.tile.components)
    requires nonzero(emit.tile.cell)
    requires nonzero(emit.paging.finish_body)
    requires nonzero(emit.paging.current_label)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.binary) requires nonzero(emit.values.finish)
    requires nonzero(emit.values.scalars.constant)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve)
    requires nonzero(control.labels) requires nonzero(control.reserve_words) requires nonzero(control.finish_words)
    requires nonzero(control.reserve_edges)
    requires nonzero(host.phase) requires nonzero(host.value) {
    address context = control.context;
    ScalarHost stream = emit.values.scalars;
    MemoryHost memory = MemoryHost(stream, emit.paging.finish_body);
    ReturnEdges edges = ReturnEdges(EdgeStorage(address(), 0u64), 0u64);
    ControlTargets targets = ControlTargets(0, 0, 0, 0);
    usize index = 0u64;
    while (index < host.phases.count) {
        TilePhaseView phase = host.phase(host.phases.data, index);
        index += 1u64;
        u32 before = emit.paging.current_label(stream.context);
        u32 execute = control.labels(context, 2);
        u32 end = execute + 1;
        control.finish_words(context, open_selection(end, active, execute, end, control.reserve_words(context, 9u64)));
        if (emit_block(phase.code, targets, edges, control, emit) == BlockFlow::Error) { return; }
        Span empty = Span(address(), 0u64, 0u64);
        View step = address_bits(phase.step.data) == 0u64
            ? View(0, 0u64, 0u64, 0u64, 0u64, empty, empty, phase.step, phase.step)
            : emit.ir.inspect(phase.step);
        if (step.tag == OP_TILE_PUBLISH) { block_tile_publish(step, emit.values, emit.tile, memory); }
        u32 producer = emit.paging.current_label(stream.context);
        control.finish_words(context, next_control_block(end, end, 1, control.reserve_words(context, 4u64)));
        usize saved_index = 0u64;
        while (saved_index < phase.saved.count) {
            TilePrivateKey slot = tile_private_at(phase.saved, saved_index);
            saved_index += 1u64;
            if (slot.space == TilePrivateSpace::Local) { continue; }
            TilePhaseValue saved = host.value(context, slot.index);
            // Module constants dominate both paths. Only live SSA registers need a Phi for the inactive tail.
            if (saved.module_constant == 0) {
                u32 result = control.labels(context, 1);
                u32 zero = stream.constant(stream.context, emit.values.scalar(stream.context, slot.index), 0u64);
                address body = control.reserve_words(context, 7u64);
                u64 words = phi_two(body, 0u64, saved.value_type, result, saved.value, producer, zero, before);
                control.finish_words(context, ControlWords(words, 0, 1));
                put(emit.values.registers, slot.index, result);
            }
        }
        if (step.tag == OP_TILE_PUBLISH || step.tag == OP_TILE_BARRIER) {
            tile_workgroup_sync(0x148, memory);
        } else if (step.tag == OP_TILE_COLLECTIVE) {
            block_tile_collective(step, emit, memory, control);
        }
    }
}

struct SubgroupArithmetic { u32 opcode; u32 group; };

SubgroupArithmetic subgroup_arithmetic(SubgroupOperation operation, BuiltinScalar scalar) on cpu {
    if (operation == SubgroupOperation::Sum || operation == SubgroupOperation::ExclusiveSum) {
        ScalarDefinition type = scalar_definition(scalar);
        return SubgroupArithmetic(type.opcode == 22 ? 350 : 349, operation == SubgroupOperation::ExclusiveSum ? 2 : 0);
    }
    if (operation == SubgroupOperation::Min || operation == SubgroupOperation::Max) {
        ScalarDefinition type = scalar_definition(scalar);
        u32 base = operation == SubgroupOperation::Min ? 353 : 356;
        u32 offset = type.signed != 0 ? 0 : (type.opcode == 21 ? 1 : 2);
        return SubgroupArithmetic(base + offset, 0);
    }
    if (operation == SubgroupOperation::BitAnd) { return SubgroupArithmetic(359, 0); }
    if (operation == SubgroupOperation::BitOr) { return SubgroupArithmetic(360, 0); }
    if (operation == SubgroupOperation::BitXor) { return SubgroupArithmetic(361, 0); }
    if (operation == SubgroupOperation::ExclusiveBitAnd) { return SubgroupArithmetic(359, 2); }
    if (operation == SubgroupOperation::ExclusiveBitOr) { return SubgroupArithmetic(360, 2); }
    return SubgroupArithmetic(361, 2);
}

void block_subgroup(View instruction, BlockValues values, BlockSubgroup group, MemoryHost memory) on cpu
    requires nonzero(values.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    BuiltinScalar scalar = values.scalar(context, instruction.a);
    u32 type = scalar_id(stream.types, scalar);
    u32 result = block_register(values.registers, instruction.a);
    u32 code = u32(instruction.c);
    if (code < 3) {
        u32 pointer = code == 0 ? group.lane : (code == 1 ? group.size : group.id);
        address body = stream.reserve(context, 4u64, 0u64).body;
        memory.finish_body(context, words_three(body, 0u64, 61, type, result, pointer));
        return;
    }

    SubgroupOperation operation = SubgroupOperation(code);
    u32 scope = stream.constant(context, BuiltinScalar::U32, 3u64);
    address body = stream.reserve(context, 11u64, 0u64).body;
    if (operation == SubgroupOperation::Elect) {
        memory.finish_body(context, words_three(body, 0u64, 333, type, result, scope));
        return;
    }

    u32 value = block_register(values.registers, unsafe_position<usize>(instruction.first.data));
    u64 words;
    if (operation == SubgroupOperation::Any || operation == SubgroupOperation::All || operation == SubgroupOperation::BroadcastFirst) {
        u32 opcode = operation == SubgroupOperation::Any ? 335 : (operation == SubgroupOperation::All ? 334 : 338);
        words = words_four(body, 0u64, opcode, type, result, scope, value);
    } else if (operation == SubgroupOperation::Count || operation == SubgroupOperation::PrefixCount) {
        u32 ballot = stream.ids(context, 1);
        u32 mode = operation == SubgroupOperation::PrefixCount ? 2 : 0;
        u64 at = words_four(body, 0u64, 339, group.ballot_type, ballot, scope, value);
        words = words_five(body, at, 342, type, result, scope, mode, ballot);
    } else if (operation == SubgroupOperation::Shuffle || operation == SubgroupOperation::ShuffleXor ||
        operation == SubgroupOperation::ShuffleUp || operation == SubgroupOperation::ShuffleDown) {
        u32 opcode = 345 + code - u32(SubgroupOperation::Shuffle);
        u32 argument = block_register(values.registers, unsafe_position<usize>(instruction.second.data));
        words = words_five(body, 0u64, opcode, type, result, scope, value, argument);
    } else {
        SubgroupArithmetic arithmetic = subgroup_arithmetic(operation, scalar);
        words = words_five(body, 0u64, arithmetic.opcode, type, result, scope, arithmetic.group, value);
    }
    memory.finish_body(context, words);
}

u32 block_span_register(address registers, Span values, usize index) on cpu {
    address at = unsafe_address(address_bits(values.data) + index * values.stride);
    return block_register(registers, unsafe_position<usize>(at));
}

void block_texture_sample(View sample, View texture, BlockValues values, TextureSampling bindings) on cpu
    requires nonzero(bindings.declaration) requires nonzero(values.finish)
    requires nonzero(values.scalars.ids) requires nonzero(values.scalars.reserve) {
    ScalarHost stream = values.scalars;
    address context = stream.context;
    address registers = values.registers;
    bool volume = bindings.declaration(bindings.bindings, texture.a).dimensions == 3;
    SampledType type = volume ? bindings.sampled3 : bindings.sampled2;
    u32 coordinate_type = volume ? bindings.vec3 : bindings.vec2;
    u32 variable = block_register(bindings.variables, texture.a);
    bool arrayed = texture.second.count != 0u64;
    u32 sampled = stream.ids(context, arrayed ? 5 : 3);
    u32 pointer = variable;
    u64 at = 0u64;
    u64 annotated = 0u64;
    ScalarStreams words = stream.reserve(context, 46u64, arrayed ? 9u64 : 0u64);
    if (arrayed) {
        u32 element = block_span_register(registers, texture.second, 0u64);
        u32 index = sampled;
        pointer = sampled + 1;
        sampled += 2;
        u32 index_type = scalar_id(stream.types, BuiltinScalar::U32);
        at = words_three(words.body, at, 83, index_type, index, element);
        at = words_four(words.body, at, 65, type.pointer, pointer, variable, index);
        annotated = flag_decoration(words.decorations, annotated, FlagDecoration::NonUniform, index);
        annotated = flag_decoration(words.decorations, annotated, FlagDecoration::NonUniform, pointer);
        annotated = flag_decoration(words.decorations, annotated, FlagDecoration::NonUniform, sampled);
    }
    u32 coordinate = sampled + 1;
    u32 result = sampled + 2;
    at = words_three(words.body, at, 61, type.sampled, sampled, pointer);
    u32 x = block_span_register(registers, sample.second, 0u64);
    u32 y = block_span_register(registers, sample.second, 1u64);
    if (volume) {
        u32 z = block_span_register(registers, sample.second, 2u64);
        at = words_five(words.body, at, 80, coordinate_type, coordinate, x, y, z);
    } else {
        at = words_four(words.body, at, 80, coordinate_type, coordinate, x, y);
    }
    put(words.body, at, instruction(88, 7));
    put(words.body, at + 1u64, bindings.vec4);
    put(words.body, at + 2u64, result);
    put(words.body, at + 3u64, sampled);
    put(words.body, at + 4u64, coordinate);
    put(words.body, at + 5u64, 2);
    put(words.body, at + 6u64, block_register(registers, sample.a));
    at += 7u64;
    u32 component_type = scalar_id(stream.types, BuiltinScalar::F32);
    for (component in 0..4) {
        u32 dst = block_span_register(registers, sample.first, u64(component));
        at = words_four(words.body, at, 81, component_type, dst, result, component);
    }
    values.finish(context, ScalarWords(at, annotated));
}

enum AtomicOperation:u32 {
    Load=0; Store=1; Add=2; Sub=3; Exchange=4; CompareExchange=5;
    Min=6; Max=7; And=8; Or=9; Xor=10;
};
struct AtomicEmission {
    AtomicOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 pointer;
    u32 scope;
    u32 semantics;
    u32 value;
    u32 compare;
    u32 failure;
};

u32 atomic_update_opcode(AtomicOperation operation, BuiltinScalar scalar) on cpu {
    if (operation == AtomicOperation::Store || operation == AtomicOperation::Exchange) { return 229; }
    if (operation == AtomicOperation::Add || operation == AtomicOperation::Sub) {
        return 234 + u32(operation) - u32(AtomicOperation::Add);
    }
    if (operation == AtomicOperation::Min || operation == AtomicOperation::Max) {
        ScalarDefinition type = scalar_definition(scalar);
        return 237 + 2 * (u32(operation) - u32(AtomicOperation::Min)) - type.signed;
    }
    return 240 + u32(operation) - u32(AtomicOperation::And);
}

u64 atomic_instruction(AtomicEmission input, address body) on cpu {
    AtomicOperation operation = input.operation;
    if (operation == AtomicOperation::Load) {
        return words_five(body, 0u64, 227, input.result_type, input.result,
                          input.pointer, input.scope, input.semantics);
    }
    if (operation == AtomicOperation::Store && input.result == 0) {
        return words_four(body, 0u64, 228, input.pointer, input.scope, input.semantics, input.value);
    }
    bool compare_exchange = operation == AtomicOperation::CompareExchange;
    u32 length = compare_exchange ? 9 : 7;
    u32 opcode = compare_exchange ? 230 : atomic_update_opcode(operation, input.scalar);
    put(body, 0u64, instruction(opcode, length));
    put(body, 1u64, input.result_type);
    put(body, 2u64, input.result);
    put(body, 3u64, input.pointer);
    put(body, 4u64, input.scope);
    put(body, 5u64, input.semantics);
    if (compare_exchange) {
        put(body, 6u64, input.failure);
        put(body, 7u64, input.value);
        put(body, 8u64, input.compare);
    } else {
        put(body, 6u64, input.value);
    }
    return u64(length);
}

u32 atomic_memory_semantics(u32 order, u32 storage) on cpu {
    // Checked IR encodes Acquire in bit 0 and Release in bit 1.
    return order == 0 ? 0 : storage | (u32(1) << order);
}

u32 atomic_operand(BlockValues values, usize reg, MemoryHost memory) on cpu
    requires nonzero(values.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    BuiltinScalar scalar = values.scalar(memory.scalars.context, reg);
    return encode_raw_bits(scalar, block_register(values.registers, reg), memory);
}

void block_atomic(View instruction, View target, View op, BlockHost emit, MemoryHost memory) on cpu
    requires nonzero(emit.values.scalar) requires nonzero(emit.ir.inspect) requires nonzero(emit.tile.cell)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    address registers = emit.values.registers;
    u32 index = block_register(registers, target.b);
    u32 offset = instruction.second.count == 0u64 ? 0 :
        block_register(registers, unsafe_position<usize>(instruction.second.data));
    bool wide = instruction.b == 8u64;
    BuiltinScalar unit = wide ? BuiltinScalar::U64 : BuiltinScalar::U32;
    BuiltinScalar scalar = instruction.c == 0u64 ? unit : (wide ? BuiltinScalar::I64 : BuiltinScalar::I32);
    bool workgroup = target.tag == INDEXED_TILE;
    u32 pointer;
    if (workgroup) {
        TileKey key = tile_key(emit.tile.tiles, target.a, u32(instruction.a), emit.ir);
        pointer = tile_storage_pointer(emit.tile, key, index, offset, memory);
    } else {
        PageInputs page = block_dataset_page(emit.paging, target.a);
        page.index = index;
        page.word = u32(instruction.a);
        page.has_word = 1;
        page.extra = offset;
        PhysicalShape shape = PhysicalShape(unit, unit, u32(instruction.b), 1);
        pointer = locate_scalar(page, shape, memory).pointer;
    }

    AtomicOperation operation = AtomicOperation(op.tag);
    u32 result_type = scalar_id(stream.types, unit);
    u32 scope = stream.constant(context, BuiltinScalar::U32, workgroup ? 2u64 : 1u64);
    u32 storage = workgroup ? 0x100 : 0x40;
    u32 semantics = stream.constant(context, BuiltinScalar::U32, u64(atomic_memory_semantics(u32(instruction.d), storage)));
    u32 result = operation == AtomicOperation::Store && !wide ? 0 : stream.ids(context, 1);
    bool cas = operation == AtomicOperation::CompareExchange;
    u32 failure = cas ? stream.constant(context, BuiltinScalar::U32, u64(atomic_memory_semantics(u32(op.c), storage))) : 0;
    u32 value = operation == AtomicOperation::Load ? 0 : atomic_operand(emit.values, cas ? op.b : op.a, memory);
    u32 compare = cas ? atomic_operand(emit.values, op.a, memory) : 0;
    address body = stream.reserve(context, 9u64, 0u64).body;
    AtomicEmission emitted = AtomicEmission(operation, scalar, result_type, result,
        pointer, scope, semantics, value, compare, failure);
    memory.finish_body(context, atomic_instruction(emitted, body));
    if (instruction.first.count != 0u64) {
        usize dst = unsafe_position<usize>(instruction.first.data);
        BuiltinScalar target_type = emit.values.scalar(context, dst);
        emit_payload_load(PayloadShape(PayloadAccess::Words, target_type, unit, 1),
            address_of(result), block_register(registers, dst), memory);
    }
}

u64 phi_two(address body, u64 at, u32 type, u32 result,
            u32 first_value, u32 first_block, u32 second_value, u32 second_block) on cpu {
    put(body, at, instruction(245, 7));
    put(body, at + 1u64, type);
    put(body, at + 2u64, result);
    put(body, at + 3u64, first_value);
    put(body, at + 4u64, first_block);
    put(body, at + 5u64, second_value);
    put(body, at + 6u64, second_block);
    return at + 7u64;
}

AppendWords encode_append_reservation(AppendEmission input, address body) on cpu {
    u32 uint_type = input.uint_type;
    u32 bool_type = input.bool_type;
    u32 subgroup_scope = input.subgroup_scope;
    u32 zero = input.relaxed;
    u32 ballot = input.next;
    u32 size = ballot + 1;
    u32 rank = size + 1;
    u32 leader = rank + 1;
    u32 reserve = leader + 1;
    u32 gather = reserve + 1;
    u64 at = words_four(body, 0u64, 339, input.ballot_type, ballot, subgroup_scope, input.active);
    at = words_five(body, at, 342, uint_type, size, subgroup_scope, 0, ballot);
    at = words_five(body, at, 342, uint_type, rank, subgroup_scope, 2, ballot);
    at = words_three(body, at, 333, bool_type, leader, subgroup_scope);
    at = open_conditional(body, at, gather, leader, reserve, gather);
    at = words_one(body, at, 248, reserve);

    u32 pointer = gather + 1;
    u32 capacity = pointer + 1;
    u32 initial = capacity + 1;
    at = words_five(body, at, 65, input.push_pointer_type, pointer, input.push, zero, input.capacity_word);
    at = words_five(body, at, 61, uint_type, capacity, pointer, 2, 4);
    at += atomic_instruction(AtomicEmission(AtomicOperation::Load, BuiltinScalar::U32,
                        uint_type, initial, input.counter, input.device_scope, zero, 0, 0, 0), word_at(body, at));

    u32 header = initial + 1;
    u32 attempt = header + 1;
    u32 retry = attempt + 1;
    u32 end = retry + 1;
    u32 old = end + 1;
    u32 observed = old + 1;
    u32 room = observed + 1;
    at = words_one(body, at, 249, header);
    at = words_one(body, at, 248, header);
    at = phi_two(body, at, uint_type, old, initial, reserve, observed, retry);
    at = words_four(body, at, 176, bool_type, room, old, capacity);
    at = words_three(body, at, 246, end, retry, 0);
    at = words_three(body, at, 250, room, attempt, end);
    at = words_one(body, at, 248, attempt);

    u32 available = room + 1;
    u32 partial = available + 1;
    u32 amount = partial + 1;
    u32 desired = amount + 1;
    u32 accepted = desired + 1;
    at = words_four(body, at, 130, uint_type, available, capacity, old);
    at = words_four(body, at, 176, bool_type, partial, available, size);
    at = select_words(body, at, uint_type, amount, partial, available, size);
    at = words_four(body, at, 128, uint_type, desired, old, amount);
    at += atomic_instruction(AtomicEmission(AtomicOperation::CompareExchange, BuiltinScalar::U32,
                        uint_type, observed, input.counter, input.device_scope, zero,
                        desired, old, zero), word_at(body, at));
    at = words_four(body, at, 170, bool_type, accepted, observed, old);
    at = words_three(body, at, 250, accepted, end, retry);
    at = words_one(body, at, 248, retry);
    at = words_one(body, at, 249, header);

    u32 leased = accepted + 1;
    u32 base = leased + 1;
    u32 length = base + 1;
    u32 shared_base = length + 1;
    u32 shared_length = shared_base + 1;
    at = words_one(body, at, 248, end);
    at = phi_two(body, at, uint_type, leased, zero, header, amount, attempt);
    at = words_one(body, at, 249, gather);
    at = words_one(body, at, 248, gather);
    at = phi_two(body, at, uint_type, base, zero, input.outer, old, end);
    at = phi_two(body, at, uint_type, length, zero, input.outer, leased, end);
    at = words_four(body, at, 338, uint_type, shared_base, subgroup_scope, base);
    at = words_four(body, at, 338, uint_type, shared_length, subgroup_scope, length);
    at = words_four(body, at, 176, bool_type, input.accepted, rank, shared_length);

    u32 write_label = shared_length + 1;
    u32 finish = write_label + 1;
    u32 ticket = finish + 1;
    at = open_conditional(body, at, finish, input.accepted, write_label, finish);
    at = words_one(body, at, 248, write_label);
    at = words_four(body, at, 128, uint_type, ticket, shared_base, rank);
    return AppendWords(at, ticket + 1, ticket, write_label, finish);
}

DispatchValue dispatch_ceil(address body, DispatchValue cursor, DispatchScalars types,
                            u32 value, u32 divisor) on cpu {
    u32 quotient = cursor.next;
    u32 remainder = quotient + 1;
    u32 partial = remainder + 1;
    u32 carry = partial + 1;
    u32 result = carry + 1;
    u32 uint_type = types.uint_type;
    u64 at = words_four(body, cursor.words, 134, uint_type, quotient, value, divisor);
    at = words_four(body, at, 137, uint_type, remainder, value, divisor);
    at = words_four(body, at, 171, types.bool_type, partial, remainder, types.zero);
    at = select_words(body, at, uint_type, carry, partial, types.one, types.zero);
    at = words_four(body, at, 128, uint_type, result, quotient, carry);
    return DispatchValue(at, result + 1, result);
}

DispatchValue dispatch_groups(address body, DispatchValue cursor, DispatchScalars types,
                              u32 count, u32 width, bool collective) on cpu {
    DispatchValue groups = dispatch_ceil(body, cursor, types, count, width);
    if (!collective) { return groups; }
    u32 empty = groups.next;
    u32 nonzero = empty + 1;
    u64 at = words_four(body, groups.words, 170, types.bool_type, empty, groups.value, types.zero);
    at = select_words(body, at, types.uint_type, nonzero, empty, types.one, groups.value);
    return DispatchValue(at, nonzero + 1, nonzero);
}

struct StageBranch { u32 run; u32 end; };

u32 stage_op(inout StageCursor out, u32 op, u32 ty, u32 a, u32 b) on cpu {
    u32 result = out.next;
    out.words = words_four(out.body, out.words, op, ty, result, a, b);
    out.next = result + 1;
    return result;
}

u32 stage_aligned_load(inout StageCursor out, u32 ty, u32 pointer, u32 alignment) on cpu {
    u32 result = out.next;
    out.words = aligned_load(out.body, out.words, ty, result, pointer, alignment);
    out.next = result + 1;
    return result;
}

u32 stage_load(inout StageCursor out, u32 ty, u32 pointer, bool physical) on cpu {
    if (physical) {
        return stage_aligned_load(out, ty, pointer, 4);
    }
    u32 result = out.next;
    out.words = words_three(out.body, out.words, 61, ty, result, pointer);
    out.next = result + 1;
    return result;
}

PageAddressResult stage_address(inout StageCursor out, PageAddress input, u32 index, u32 step) on cpu {
    PageAddress request = input;
    request.index = index;
    request.next = out.next;
    PageAddressResult result = page_address(request, step, word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
    return result;
}

StageBranch stage_branch(inout StageCursor out, u32 condition, bool skip_selected) on cpu {
    u32 run = out.next;
    u32 end = run + 1;
    out.words = open_conditional(out.body, out.words, end, condition,
        skip_selected ? end : run, skip_selected ? run : end);
    out.words = words_one(out.body, out.words, 248, run);
    out.next = end + 1;
    return StageBranch(run, end);
}

StageBranch stage_open(inout StageCursor out, u32 condition) on cpu {
    return stage_branch(out, condition, false);
}

void stage_close(inout StageCursor out, u32 end) on cpu {
    out.words = words_one(out.body, out.words, 249, end);
    out.words = words_one(out.body, out.words, 248, end);
}

u32 stage_phi(inout StageCursor out, u32 ty, u32 a, u32 edge_a, u32 b, u32 edge_b) on cpu {
    u32 result = out.next;
    out.words = phi_two(out.body, out.words, ty, result, a, edge_a, b, edge_b);
    out.next = result + 1;
    return result;
}

u32 sort_ballot(inout StageCursor out, u32 ty, u32 vector, u32 scope, u32 predicate) on cpu {
    u32 vote = stage_op(out, 339, vector, scope, predicate);
    return stage_op(out, 81, ty, vote, 0);
}

u32 stage_select(inout StageCursor out, u32 ty, u32 condition, u32 yes, u32 no) on cpu {
    u32 result = out.next;
    out.words = select_words(out.body, out.words, ty, result, condition, yes, no);
    out.next = result + 1;
    return result;
}

u32 stage_unary(inout StageCursor out, u32 op, u32 ty, u32 value) on cpu {
    u32 result = out.next;
    out.words = words_three(out.body, out.words, op, ty, result, value);
    out.next = result + 1;
    return result;
}

extern "C" u64 workgroup_barrier(address body, u64 at, u32 scope, u32 semantics) on cpu {
    return words_three(body, at, 224, scope, scope, semantics);
}

void stage_barrier(inout StageCursor out, u32 scope, u32 semantics) on cpu {
    out.words = workgroup_barrier(out.body, out.words, scope, semantics);
}

extern "C" StageWords encode_sort_rank(SortRankEmission input, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    DispatchScalars types = input.scalars;
    u32 u = types.uint_type;
    u32 b = types.bool_type;
    u32 zero = types.zero;
    bool histogram = input.histogram != 0;

    u32 wave = stage_load(out, u, input.wave_input, false);
    u32 lane = stage_load(out, u, input.lane_input, false);
    u32 group = stage_op(out, 134, u, input.index, input.width);
    u32 wave_base = stage_op(out, 132, u, wave, input.wave_width);
    u32 local = stage_op(out, 128, u, wave_base, lane);
    u32 base = stage_op(out, 132, u, group, input.width);
    u32 index = stage_op(out, 128, u, base, local);
    u32 active = stage_op(out, 176, b, index, input.count);

    // Both values stay in SSA. Histogram needs only the key.
    StageBranch read = stage_open(out, active);
    PageAddressResult source = stage_address(out, input.source, index, histogram ? 0 : input.pair_step);
    u32 loaded = stage_load(out, u, source.pointer, true);
    u32 ordinal = zero;
    if (!histogram) { ordinal = stage_load(out, u, source.second, true); }
    stage_close(out, read.end);
    u32 key = stage_phi(out, u, zero, input.label, loaded, read.run);
    if (!histogram) { ordinal = stage_phi(out, u, zero, input.label, ordinal, read.run); }

    u32 shifted = stage_op(out, 194, u, key, input.shift);
    u32 digit = stage_op(out, 199, u, shifted, input.mask);
    u32 equal = sort_ballot(out, u, input.ballot_type, input.subgroup_scope, active);
    for (bit in 0..4) {
        u32 masked = stage_op(out, 199, u, digit, input.bit_masks[bit]);
        u32 set = stage_op(out, 171, b, masked, zero);
        u32 selected = sort_ballot(out, u, input.ballot_type, input.subgroup_scope, set);
        u32 inverted = stage_unary(out, 200, u, selected);
        u32 matching = stage_select(out, u, set, selected, inverted);
        equal = stage_op(out, 199, u, equal, matching);
    }
    u32 bit = stage_op(out, 196, u, types.one, lane);
    u32 lower = stage_op(out, 130, u, bit, types.one);
    u32 prior = stage_op(out, 199, u, equal, lower);
    u32 rank = stage_unary(out, 205, u, prior);
    u32 total = stage_unary(out, 205, u, equal);

    // Each of the two waves owns its sixteen bins.
    u32 init = stage_op(out, 176, b, local, input.wave_width);
    StageBranch clear = stage_open(out, init);
    u32 cell = stage_op(out, 65, input.shared_pointer, input.shared, local);
    out.words = words_two(body, out.words, 62, cell, zero);
    stage_close(out, clear.end);
    stage_barrier(out, input.workgroup_scope, input.shared_semantics);

    u32 first = stage_op(out, 170, b, rank, zero);
    u32 leader = stage_op(out, 167, b, first, active);
    u32 wave_bin = stage_op(out, 132, u, wave, input.bins);
    u32 own_bin = stage_op(out, 128, u, wave_bin, digit);
    StageBranch write = stage_open(out, leader);
    u32 cell_out = stage_op(out, 65, input.shared_pointer, input.shared, own_bin);
    out.words = words_two(body, out.words, 62, cell_out, total);
    stage_close(out, write.end);
    stage_barrier(out, input.workgroup_scope, input.shared_semantics);

    DispatchValue groups = dispatch_groups(body, DispatchValue(out.words, out.next, 0), types, input.count, input.width, true);
    out.words = groups.words;
    out.next = groups.next;
    if (histogram) {
        u32 writes_bin = stage_op(out, 176, b, local, input.bins);
        StageBranch sum = stage_open(out, writes_bin);
        u32 a = stage_op(out, 65, input.shared_pointer, input.shared, local);
        u32 next_bin = stage_op(out, 128, u, local, input.bins);
        u32 c = stage_op(out, 65, input.shared_pointer, input.shared, next_bin);
        u32 x = stage_load(out, u, a, false);
        u32 y = stage_load(out, u, c, false);
        u32 sum_bin = stage_op(out, 128, u, x, y);
        u32 row = stage_op(out, 132, u, local, groups.value);
        u32 slot = stage_op(out, 128, u, row, group);
        PageAddressResult target = stage_address(out, input.totals, slot, 0);
        out.words = words_four(body, out.words, 62, target.pointer, sum_bin, 2, 4);
        stage_close(out, sum.end);
        return StageWords(out.words, out.next, sum.end);
    }

    StageBranch scatter = stage_open(out, active);
    u32 prior_bin = stage_op(out, 65, input.shared_pointer, input.shared, digit);
    u32 prior_wave = stage_load(out, u, prior_bin, false);
    u32 is_first_wave = stage_op(out, 170, b, wave, zero);
    u32 previous = stage_select(out, u, is_first_wave, zero, prior_wave);
    u32 own_rank = stage_op(out, 128, u, rank, previous);
    u32 row = stage_op(out, 132, u, digit, groups.value);
    u32 slot = stage_op(out, 128, u, row, group);
    PageAddressResult offsets = stage_address(out, input.totals, slot, 0);
    u32 offset = stage_load(out, u, offsets.pointer, true);
    u32 index_out = stage_op(out, 128, u, offset, own_rank);
    PageAddressResult target = stage_address(out, input.target, index_out, input.pair_step);
    out.words = words_four(body, out.words, 62, target.pointer, key, 2, 4);
    out.words = words_four(body, out.words, 62, target.second, ordinal, 2, 4);
    stage_close(out, scatter.end);
    return StageWords(out.words, out.next, scatter.end);
}

DispatchValue dispatch_min(address body, DispatchValue cursor, DispatchScalars types, u32 a, u32 b) on cpu {
    u32 less = cursor.next;
    u32 result = less + 1;
    u64 at = words_four(body, cursor.words, 176, types.bool_type, less, a, b);
    at = select_words(body, at, types.uint_type, result, less, a, b);
    return DispatchValue(at, result + 1, result);
}

struct DispatchValue { u64 words; u32 next; u32 value; };
struct DispatchGrid {
    DispatchScalars scalars;
    u32 wide_type;
    u32 table_pointer;
    u32 page_pointer;
    u32 word_pointer;
    u32 descriptor;
    u32 width;
    u32 max_x;
    u32 max_y;
    u32 capacity;
    u32 collective;
    u32 bins;
    u32 first_byte;
    u32 histogram_byte;
    u32 width_shift;
    u32 record_bytes;
    array<u32,3> field_bytes;
};

DispatchGrid prepare_dispatch_grid(DispatchLayout layout, u32 max_x, u32 max_y,
    u32 capacity, bool collective, u32 first_byte, u32 histogram_byte, ScalarHost host) on cpu
    requires nonzero(host.constant) {
    address context = host.context;
    DispatchScalars types = DispatchScalars(scalar_id(host.types, BuiltinScalar::U32),
        scalar_id(host.types, BuiltinScalar::Bool), host.constant(context, BuiltinScalar::U32, 0u64),
        host.constant(context, BuiltinScalar::U32, 1u64));
    u32 width = host.constant(context, BuiltinScalar::U32, u64(layout.width));
    u32 bins = histogram_byte == 0 ? 0 : host.constant(context, BuiltinScalar::U32, 16u64);
    u32 shift = host.constant(context, BuiltinScalar::U32, u64(layout.width_shift));
    u32 stride = host.constant(context, BuiltinScalar::U64, u64(layout.record_bytes));
    array<u32,3> fields = array<u32,3>(host.constant(context, BuiltinScalar::U64, 4u64),
        host.constant(context, BuiltinScalar::U64, 8u64), host.constant(context, BuiltinScalar::U64, 12u64));
    return DispatchGrid(types, scalar_id(host.types, BuiltinScalar::U64),
        layout.table_pointer, layout.page_pointer, layout.word_pointer, layout.descriptor,
        width, max_x, max_y, capacity, u32(collective), bins, first_byte, histogram_byte, shift, stride, fields);
}

u64 dispatch_grid_capacity(read DispatchGrid input) on cpu {
    // First record, optional histogram setup and one loop body for the hierarchy.
    return 25u64 + 167u64 + (input.bins != 0 ? 56u64 + 167u64 : 0u64)
        + (input.capacity != 0 ? 96u64 + 167u64 : 0u64);
}

DispatchValue dispatch_pointer(address body, DispatchValue cursor, DispatchGrid input,
                               u32 base, u32 offset) on cpu {
    u32 physical = base;
    u32 next = cursor.next;
    u64 at = cursor.words;
    if (offset != 0) {
        physical = next;
        at = words_four(body, at, 128, input.wide_type, physical, base, offset);
        next += 1;
    }
    at = words_three(body, at, 120, input.word_pointer, next, physical);
    return DispatchValue(at, next + 1, next);
}

DispatchValue dispatch_record(address body, DispatchValue cursor, DispatchGrid input,
                              u32 base, u32 count) on cpu {
    DispatchScalars types = input.scalars;
    u32 zero = types.zero;
    DispatchValue output = cursor;
    DispatchValue groups = DispatchValue(cursor.words, cursor.next, zero);
    u32 x = zero;
    u32 y = zero;
    u32 z = zero;
    if (count != zero || input.collective != 0) {
        groups = dispatch_groups(body, cursor, types, count, input.width, input.collective != 0);
        DispatchValue columns = dispatch_min(body, groups, types, groups.value, input.max_x);
        DispatchValue rows = dispatch_ceil(body, columns, types, groups.value, input.max_x);
        DispatchValue height = dispatch_min(body, rows, types, rows.value, input.max_y);
        DispatchValue depth = dispatch_ceil(body, height, types, rows.value, input.max_y);
        x = columns.value;
        y = height.value;
        z = depth.value;
        output = depth;
    }
    array<u32,4> values = array<u32,4>(x, y, z, count);
    for (i in 0..4) {
        u32 offset = i == 0 ? 0 : input.field_bytes[i - 1];
        DispatchValue pointer = dispatch_pointer(body, output, input, base, offset);
        u64 at = words_four(body, pointer.words, 62, pointer.value, values[i], 2, 4);
        output = DispatchValue(at, pointer.next, 0);
    }
    return DispatchValue(output.words, output.next, groups.value);
}

DispatchValue dispatch_levels(address body, DispatchValue cursor, DispatchGrid input,
                              u32 base, u32 count, u32 capacity) on cpu {
    DispatchValue first = dispatch_record(body, cursor, input, base, count);
    if (capacity == 0) { return first; }

    StageCursor out = StageCursor(body, first.words, first.next);
    DispatchScalars types = input.scalars;
    u32 u = types.uint_type;
    u32 first_next = stage_op(out, 128, input.wide_type, base, input.record_bytes);
    u32 before = out.next;
    u32 header = before + 1;
    u32 run = header + 1;
    u32 repeat = run + 1;
    u32 end = repeat + 1;
    u32 next_count = end + 1;
    u32 next_capacity = next_count + 1;
    u32 next_base = next_capacity + 1;
    out.next = next_base + 1;
    stage_close(out, before);
    stage_close(out, header);

    u32 current = stage_phi(out, u, first.value, before, next_count, repeat);
    u32 remaining = stage_phi(out, u, capacity, before, next_capacity, repeat);
    u32 record = stage_phi(out, input.wide_type, first_next, before, next_base, repeat);
    u32 more = stage_op(out, 172, types.bool_type, remaining, input.width);
    out.words = words_three(body, out.words, 246, end, repeat, 0);
    out.words = words_three(body, out.words, 250, more, run, end);
    out.words = words_one(body, out.words, 248, run);

    DispatchValue emitted = dispatch_record(body, DispatchValue(out.words, out.next, 0), input, record, current);
    out.words = emitted.words;
    out.next = emitted.next;
    stage_close(out, repeat);
    u32 last = stage_op(out, 130, u, remaining, types.one);
    u32 groups_minus_one = stage_op(out, 194, u, last, input.width_shift);
    out.words = words_three(body, out.words, 83, u, next_count, emitted.value);
    out.words = words_four(body, out.words, 128, u, next_capacity, groups_minus_one, types.one);
    out.words = words_four(body, out.words, 128, input.wide_type, next_base, record, input.record_bytes);
    out.words = words_one(body, out.words, 249, header);
    out.words = words_one(body, out.words, 248, end);
    return DispatchValue(out.words, out.next, first.value);
}

DispatchValue encode_dispatch_grid(DispatchGrid input, u32 count, u32 next, address body) on cpu {
    // The command and the shader consume the same first page of indirect records.
    u32 header = next;
    u32 table = header + 1;
    u32 page = table + 1;
    u32 base = page + 1;
    u32 zero = input.scalars.zero;
    u64 at = words_five(body, 0u64, 65, input.table_pointer, header, input.descriptor, zero, zero);
    at = words_three(body, at, 61, input.wide_type, table, header);
    at = words_three(body, at, 120, input.page_pointer, page, table);
    at = words_five(body, at, 61, input.wide_type, base, page, 2, 8);
    StageCursor out = StageCursor(body, at, base + 1);
    u32 first = input.first_byte == 0 ? base : stage_op(out, 128, input.wide_type, base, input.first_byte);
    DispatchValue cursor = DispatchValue(out.words, out.next, 0);
    if (input.bins == 0) {
        return dispatch_levels(body, cursor, input, first, count, input.capacity);
    }

    DispatchValue root = dispatch_record(body, cursor, input, first, count);
    out.words = root.words;
    out.next = root.next;
    u32 histogram = stage_op(out, 132, input.scalars.uint_type, root.value, input.bins);
    u32 slots = 0;
    if (input.capacity != 0) {
        DispatchValue groups = dispatch_groups(body, DispatchValue(out.words, out.next, 0),
            input.scalars, input.capacity, input.width, true);
        out.words = groups.words;
        out.next = groups.next;
        slots = stage_op(out, 132, input.scalars.uint_type, groups.value, input.bins);
    }
    u32 histogram_base = stage_op(out, 128, input.wide_type, base, input.histogram_byte);
    return dispatch_levels(body, DispatchValue(out.words, out.next, 0), input, histogram_base, histogram, slots);
}

extern "C" u32 sort_initialize(SortKeyInput input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    ScalarDefinition definition = scalar_definition(input.scalar);
    PhysicalShape shape = physical_shape(input.scalar);
    bool wide_float = input.scalar == BuiltinScalar::F64;
    bool floating = input.scalar == BuiltinScalar::F32 || wide_float;
    u32 step = host.scalars.constant(context, BuiltinScalar::U64, 4u64);
    PageAddress target = prepare_page(input.target, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), host.scalars);
    PageAddress source = prepare_page(input.source, scalar_id(host.scalars.physical_pointers, shape.unit), host.scalars);
    u32 unit_type = scalar_id(host.scalars.types, shape.unit);
    u32 shift = floating ? host.scalars.constant(context, BuiltinScalar::U32, 31u64) : 0;
    u32 bias_bits = input.scalar == BuiltinScalar::F32 ? 0x80000000
        : definition.signed == 0 ? 0 : u32(1u64 << u64(definition.width - 1));
    u32 bias = bias_bits == 0 ? 0 : host.scalars.constant(context, BuiltinScalar::U32, u64(bias_bits));
    u32 inverted = input.descending == 0 ? 0 : host.scalars.constant(context, BuiltinScalar::U32, 4294967295u64);

    // One active branch, at most two paired page addresses, key loads and stores.
    u32 next = host.scalars.ids(context, 0);
    address body = host.scalars.reserve(context, 306u64, 0u64).body;
    StageCursor out = StageCursor(body, 0u64, next);
    StageBranch run = stage_open(out, input.active);
    u32 word_type = target.u32_type;
    PageAddressResult work = stage_address(out, target, target.index, step);
    u32 original = input.first != 0 ? target.index : stage_load(out, word_type, work.second, true);
    PageAddressResult field = stage_address(out, source, original, wide_float ? step : 0);
    u32 key = stage_aligned_load(out, unit_type, field.pointer, shape.bytes);
    if (unit_type != word_type) { key = stage_unary(out, 113, word_type, key); }
    if (floating) {
        u32 sign = wide_float ? stage_load(out, word_type, field.second, true) : key;
        u32 negative = stage_op(out, 194, word_type, sign, shift);
        u32 mask = stage_op(out, 130, word_type, target.zero, negative);
        if (bias != 0) { mask = stage_op(out, 197, word_type, mask, bias); }
        key = stage_op(out, 198, word_type, key, mask);
    } else if (bias != 0) {
        key = stage_op(out, 198, word_type, key, bias);
    }
    if (inverted != 0) { key = stage_op(out, 198, word_type, key, inverted); }
    out.words = aligned_store(body, out.words, work.pointer, key, 4);
    out.words = aligned_store(body, out.words, work.second, original, 4);
    stage_close(out, run.end);
    host.finish_body(context, out.words);
    host.scalars.ids(context, out.next - next);
    return run.end;
}

extern "C" StageWords encode_sort_gather(PageAddress indices, address layout, u32 active, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, indices.next);
    StageBranch run = stage_open(out, active);
    PageAddressResult at = stage_address(out, indices, indices.index, 0);
    u32 original = stage_load(out, indices.u32_type, at.pointer, true);
    copy_record(out, layout, original, indices.index);
    stage_close(out, run.end);
    return StageWords(out.words, out.next, run.end);
}

struct ScatterRecords { address copy; u32 count_descriptor; u32 count_word; };
struct ScatterIndices { u32 descriptor; DispatchGrid grid; };
enum ScatterTarget:u32 { Records(ScatterRecords)=0; Indices(ScatterIndices)=1; };

void scatter_count(inout StageCursor out, PageAddress flag, read ScatterTarget target, u32 count) on cpu {
    match (target) {
        ScatterTarget::Records(records) {
            PageAddress field = flag;
            field.descriptor = records.count_descriptor;
            field.word = records.count_word;
            PageAddressResult at = stage_address(out, field, field.zero, 0);
            out.words = words_four(out.body, out.words, 62, at.pointer, count, 2, 4);
        }
        ScatterTarget::Indices(indices) {
            DispatchValue written = encode_dispatch_grid(indices.grid, count, out.next, word_at(out.body, out.words));
            out.words += written.words;
            out.next = written.next;
        }
    }
}

void scatter_selected(inout StageCursor out, PageAddress flag, read ScatterTarget target, u32 prefix) on cpu {
    match (target) {
        ScatterTarget::Records(records) {
            copy_record(out, records.copy, flag.index, prefix);
        }
        ScatterTarget::Indices(indices) {
            PageAddress field = flag;
            field.descriptor = indices.descriptor;
            field.word = flag.zero;
            PageAddressResult at = stage_address(out, field, prefix, 0);
            out.words = words_four(out.body, out.words, 62, at.pointer, flag.index, 2, 4);
        }
    }
}

StageWords emit_scatter(ScatterInput input, read ScatterTarget target, u32 next, address body) on cpu {
    PageAddress flag = input.flag;
    StageCursor out = StageCursor(body, 0u64, next);
    u32 first = stage_op(out, 170, input.bool_type, flag.index, flag.zero);
    StageBranch first_branch = stage_open(out, first);
    u32 empty = stage_op(out, 170, input.bool_type, input.count, flag.zero);
    StageBranch empty_branch = stage_open(out, empty);
    scatter_count(out, flag, target, flag.zero);
    stage_close(out, empty_branch.end);
    stage_close(out, first_branch.end);

    StageBranch active_branch = stage_open(out, input.active);
    PageAddressResult flag_at = stage_address(out, flag, flag.index, 0);
    u32 flag_value = stage_load(out, flag.u32_type, flag_at.pointer, true);
    u32 keep = stage_op(out, 171, input.bool_type, flag_value, flag.zero);

    PageAddress prefix_field = flag;
    prefix_field.descriptor = input.prefix_descriptor;
    prefix_field.word = flag.zero;
    PageAddressResult prefix_at = stage_address(out, prefix_field, flag.index, 0);
    u32 prefix = stage_load(out, flag.u32_type, prefix_at.pointer, true);
    StageBranch keep_branch = stage_open(out, keep);
    scatter_selected(out, flag, target, prefix);
    stage_close(out, keep_branch.end);

    u32 after_index = stage_op(out, 128, flag.u32_type, flag.index, flag.one);
    u32 last = stage_op(out, 170, input.bool_type, after_index, input.count);
    StageBranch last_branch = stage_open(out, last);
    u32 chosen = out.next;
    out.next += 1;
    out.words = select_words(body, out.words, flag.u32_type, chosen, keep, flag.one, flag.zero);
    u32 total = stage_op(out, 128, flag.u32_type, prefix, chosen);
    scatter_count(out, flag, target, total);
    stage_close(out, last_branch.end);
    stage_close(out, active_branch.end);
    return StageWords(out.words, out.next, active_branch.end);
}

extern "C" StageWords encode_record_scatter(ScatterInput input, address record_copy,
    u32 count_descriptor, u32 count_word, address body) on cpu {
    let target = ScatterTarget::Records(ScatterRecords(record_copy, count_descriptor, count_word));
    return emit_scatter(input, target, input.flag.next, body);
}

extern "C" u32 encode_index_scatter(ScatterInput input, u32 target_descriptor,
    DispatchLayout layout, u32 max_x, u32 max_y, u32 first_byte, usize scatter_words, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    DispatchGrid grid = prepare_dispatch_grid(layout, max_x, max_y, 0, false, first_byte, 0, host.scalars);
    u32 next = host.scalars.ids(context, 0);
    address body = host.scalars.reserve(context, scatter_words + 2u64 * dispatch_grid_capacity(grid), 0u64).body;
    let target = ScatterTarget::Indices(ScatterIndices(target_descriptor, grid));
    StageWords emitted = emit_scatter(input, target, next, body);
    host.finish_body(context, emitted.words);
    host.scalars.ids(context, emitted.next - next);
    return emitted.label;
}

u64 widen_component(address body, u64 at, PrefixIndexEmission input,
                    u32 vector, u32 component, u32 result) on cpu {
    u32 narrow = result - 1;
    u64 end = words_four(body, at, 81, input.uint_type, narrow, vector, component);
    return words_three(body, end, 113, input.wide_type, result, narrow);
}

u32 dispatch_push_word(inout StageCursor out, DispatchPush push, u32 word) on cpu {
    u32 pointer = out.next;
    out.words = words_five(out.body, out.words, 65, push.pointer_type, pointer, push.block, push.zero, word);
    out.next = pointer + 1;
    return stage_load(out, push.uint_type, pointer, false);
}

extern "C" void declare_dispatch_continuation(u32 type_id, u32 result, u32 spec_id, u32 bytes,
    address definitions, address decorations) on cpu {
    words_three(decorations, 0u64, 71, result, 1, spec_id);
    words_four(definitions, 0u64, 50, type_id, result, bytes, 0);
}

extern "C" PayloadRead dispatch_count_index(DispatchPush push, u32 word, u32 field, u32 first, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 base = dispatch_push_word(out, push, word);
    u32 index = stage_op(out, 128, push.uint_type, base, field);
    return PayloadRead(out.words, out.next, index);
}

extern "C" DispatchArgumentResult emit_dispatch_arguments(DispatchArgumentInput input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body) {
    address context = host.scalars.context;
    bool prefixed = input.prefix.descriptor != 0;
    bool collective = input.operation != ENTRY_MAP || input.selected != 0;
    bool levels = collective && (input.has_fixed == 0 || input.scan_count > input.scan_width);
    bool read_domain = levels && input.has_fixed == 0 && prefixed;
    u32 zero = input.push.zero;
    u32 bool_type = scalar_id(host.scalars.types, BuiltinScalar::Bool);
    u32 first = host.scalars.ids(context, 3);
    u32 end = first + 2;
    address head = host.scalars.reserve(context, 14u64, 0u64).body;
    ControlWords opened = open_index_selection(bool_type, first, input.invocation, zero, false, first + 1, end, head);
    host.finish_body(context, opened.words);

    u32 count = prefixed ? physical_scalar_read(PhysicalAccess(input.prefix, BuiltinScalar::U32, 0, 0), host) : 0;
    u32 count_word = !prefixed || read_domain
        ? host.scalars.constant(context, BuiltinScalar::U32, u64(input.count_word)) : 0;
    u32 one = host.scalars.constant(context, BuiltinScalar::U32, 1u64);
    u32 next = host.scalars.ids(context, 0);
    u64 read_words = prefixed && !read_domain ? 20u64 : 30u64;
    address reads = host.scalars.reserve(context, read_words, 0u64).body;
    StageCursor out = StageCursor(reads, 0u64, next);
    if (!prefixed) { count = dispatch_push_word(out, input.push, count_word); }
    u32 max_x = dispatch_push_word(out, input.push, zero);
    u32 max_y = dispatch_push_word(out, input.push, one);
    u32 capacity = read_domain ? dispatch_push_word(out, input.push, count_word)
        : levels && input.has_fixed == 0 ? count : 0;
    host.finish_body(context, out.words);
    host.scalars.ids(context, out.next - next);
    if (levels && input.has_fixed != 0) {
        capacity = host.scalars.constant(context, BuiltinScalar::U32, u64(input.fixed));
    }

    DispatchGrid grid = prepare_dispatch_grid(input.layout, max_x, max_y, capacity, collective,
        0, input.histogram_byte, host.scalars);
    next = host.scalars.ids(context, 0);
    address body = host.scalars.reserve(context, dispatch_grid_capacity(grid) + 4u64, 0u64).body;
    DispatchValue emitted = encode_dispatch_grid(grid, count, next, body);
    out = StageCursor(body, emitted.words, emitted.next);
    stage_close(out, end);
    host.finish_body(context, out.words);
    host.scalars.ids(context, out.next - next);
    return DispatchArgumentResult(end, u32(collective));
}

extern "C" PrefixIndexWords encode_linear_index(DispatchPush push, u32 wide_type, u32 bool_type,
    u32 local, u32 one, u32 first, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 base = dispatch_push_word(out, push, one);
    u32 local_wide = stage_unary(out, 113, wide_type, local);
    u32 base_wide = stage_unary(out, 113, wide_type, base);
    u32 linear = stage_op(out, 128, wide_type, local_wide, base_wide);
    u32 count = dispatch_push_word(out, push, push.zero);
    u32 count_wide = stage_unary(out, 113, wide_type, count);
    u32 active = stage_op(out, 176, bool_type, linear, count_wide);
    u32 index = stage_unary(out, 113, push.uint_type, linear);
    return PrefixIndexWords(out.words, out.next, index, count, active, 0, 0, 0);
}

extern "C" PrefixIndexWords encode_prefix_index(PrefixIndexEmission input, address body) on cpu {
    u32 groups = input.next;
    u32 x = groups + 1;
    u32 y = x + 2;
    u32 z = y + 2;
    u32 groups_x = z + 1;
    u32 groups_x_wide = groups_x + 1;
    u32 groups_y = groups_x_wide + 1;
    u32 groups_y_wide = groups_y + 1;
    u32 wide_type = input.wide_type;
    u64 at = words_three(body, 0u64, 61, input.vector_type, groups, input.num_groups);
    at = words_three(body, at, 113, wide_type, x, input.global_x);
    at = widen_component(body, at, input, input.global, 1, y);
    at = widen_component(body, at, input, input.global, 2, z);
    at = widen_component(body, at, input, groups, 0, groups_x_wide);
    at = widen_component(body, at, input, groups, 1, groups_y_wide);

    u32 zrows = groups_y_wide + 1;
    u32 row = zrows + 1;
    u32 pitch = row + 1;
    u32 offset = pitch + 1;
    u32 linear = offset + 1;
    at = words_four(body, at, 132, wide_type, zrows, z, groups_y_wide);
    at = words_four(body, at, 128, wide_type, row, y, zrows);
    at = words_four(body, at, 132, wide_type, pitch, groups_x_wide, input.wide_width);
    at = words_four(body, at, 132, wide_type, offset, row, pitch);
    at = words_four(body, at, 128, wide_type, linear, x, offset);

    u32 uint_type = input.uint_type;
    u32 bool_type = input.bool_type;
    u32 count = linear + 1;
    u32 count_wide = count + 1;
    u32 active = count_wide + 1;
    at = words_five(body, at, 61, uint_type, count, input.count_pointer, 2, 4);
    at = words_three(body, at, 113, wide_type, count_wide, count);
    at = words_four(body, at, 176, bool_type, active, linear, count_wide);
    u32 index = active + 1;
    u32 label = 0;

    if (input.collective != 0) {
        // Each row begins on a workgroup boundary, so X alone gives the lane.
        u32 lane = index;
        u32 lane_wide = lane + 1;
        u32 group_first = lane_wide + 1;
        at = words_four(body, at, 137, uint_type, lane, input.global_x, input.width);
        at = words_three(body, at, 113, wide_type, lane_wide, lane);
        at = words_four(body, at, 130, wide_type, group_first, linear, lane_wide);

        // An empty collective still runs its first group to publish its identity.
        u32 empty = group_first + 1;
        u32 nonzero_count = empty + 1;
        u32 group_limit = nonzero_count + 1;
        u32 present = group_limit + 1;
        at = words_four(body, at, 170, bool_type, empty, count, input.zero);
        at = select_words(body, at, uint_type, nonzero_count, empty, input.one, count);
        at = words_three(body, at, 113, wide_type, group_limit, nonzero_count);
        at = words_four(body, at, 176, bool_type, present, group_first, group_limit);

        u32 proceed = present + 1;
        u32 stop = proceed + 1;
        at = open_conditional(body, at, proceed, present, proceed, stop);
        at = words_one(body, at, 248, stop);
        put(body, at, instruction(253, 1));
        at = words_one(body, at + 1u64, 248, proceed);
        index = stop + 1;
        label = proceed;
    }

    at = words_three(body, at, 113, uint_type, index, linear);
    return PrefixIndexWords(at, index + 1, index, count, active, label, groups_x, groups_y);
}

SharedAddressWords write_shared_addresses(SharedAddress input, address offsets, address pointers, address body) on cpu {
    u64 at = 0u64;
    u32 node = input.node;
    u32 next = input.next;
    if (input.scale != 0) {
        at = words_four(body, at, 132, input.index_type, next, node, input.scale);
        node = next;
        next += 1;
    }
    for (part in 0..input.count) {
        u32 offset = read_word(offsets, part);
        u32 index = node;
        if (offset != 0) {
            at = words_four(body, at, 128, input.index_type, next, node, offset);
            index = next;
            next += 1;
        }
        at = words_four(body, at, 65, input.pointer_type, next, input.variable, index);
        put(pointers, u64(part), next);
        next += 1;
    }
    return SharedAddressWords(at, next);
}

SharedAddressWords shared_addresses(SharedAddress input, address offsets, address body) on cpu {
    return write_shared_addresses(input, offsets, offsets, body);
}

void tree_node_address(inout StageCursor out, TreeComponent component, u32 uint_type,
    u32 node, address pointers) on cpu {
    SharedAddress request = SharedAddress(uint_type, component.pointer_type, component.shared,
        node, component.scale, component.count, out.next);
    SharedAddressWords result = shared_addresses(request, pointers, word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
}

u32 read_stage_payload(inout StageCursor out, PayloadEmission input, address pointers, address shifts) on cpu {
    PayloadEmission request = input;
    request.next = out.next;
    PayloadRead result = payload_read(request, pointers, shifts, word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
    return result.value;
}

void write_stage_payload(inout StageCursor out, PayloadEmission input, u32 value, address pointers, address shifts) on cpu {
    PayloadEmission request = input;
    request.next = out.next;
    PayloadWrite result = payload_write(request, value, pointers, shifts, address(), word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
}


u32 stage_group_remaining(inout StageCursor out, u32 uint_type, u32 index, u32 lane, u32 count) on cpu {
    u32 begin = stage_op(out, 130, uint_type, index, lane);
    return stage_op(out, 130, uint_type, count, begin);
}

void group_remaining(address body, u32 uint_type, u32 index, u32 lane, u32 count, u32 first) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    stage_group_remaining(out, uint_type, index, lane, count);
}

struct TileRangeWords { u64 words; u32 next; u32 lane; u32 length; };

TileRangeWords tile_range(address body, u32 uint_type, u32 bool_type, u32 index, u32 count, u32 width, u32 first) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 lane = stage_op(out, 137, uint_type, index, width);
    u32 remaining = stage_group_remaining(out, uint_type, index, lane, count);
    u32 tail = stage_op(out, 176, bool_type, remaining, width);
    u32 length = stage_select(out, uint_type, tail, remaining, width);
    return TileRangeWords(out.words, out.next, lane, length);
}

array<u32,2> stage_collective_writer(inout StageCursor out, u32 bool_type, u32 lane,
    u32 active, u32 count, u32 zero) on cpu {
    u32 first_lane = stage_op(out, 170, bool_type, lane, zero);
    u32 empty = stage_op(out, 170, bool_type, count, zero);
    u32 needed = stage_op(out, 166, bool_type, active, empty);
    u32 writer = stage_op(out, 167, bool_type, first_lane, needed);
    return array<u32,2>(writer, empty);
}

StageBranch scan_tree_branch(inout StageCursor out, u32 uint_type, u32 bool_type, u32 lane, u32 mask) on cpu {
    u32 bits = stage_op(out, 199, uint_type, lane, mask);
    u32 selected = stage_op(out, 170, bool_type, bits, mask);
    return stage_open(out, selected);
}

TreeWords stage_tree_layer(inout StageCursor out, TreeLayer input, u32 global_index,
    address components, u32 count, address own, address decorations) on cpu {
    u32 node = input.lane;
    bool reduction = global_index != 0;
    bool local_slots = address_bits(own) != 0u64;
    StageBranch branch;
    if (reduction) {
        u32 selected = stage_op(out, 176, input.bool_type, node, input.stride);
        branch = stage_open(out, selected);
    } else if (input.tile_capacity == 0) {
        branch = scan_tree_branch(out, input.uint_type, input.bool_type, node, input.selector);
    } else {
        u32 one_based = stage_op(out, 128, input.uint_type, node, input.one);
        u32 scaled = stage_op(out, 132, input.uint_type, one_based, input.selector);
        node = stage_op(out, 130, input.uint_type, scaled, input.one);
        u32 selected = stage_op(out, 176, input.bool_type, node, input.tile_capacity);
        branch = stage_open(out, selected);
    }
    u32 other = stage_op(out, reduction ? 128 : 130, input.uint_type, node, input.stride);
    u32 has_right = 0;
    u64 decorated = 0u64;
    for (index in 0..count) {
        TreeComponent component = unsafe_position<TreeComponent>(
            unsafe_address(address_bits(components) + u64(index) * size_of<TreeComponent>()));
        array<u32,2> left_slots = array<u32,2>(component.low, component.high);
        array<u32,2> right_slots = left_slots;
        array<u32,2> shift_slots = array<u32,2>(0, component.shift);
        address left = address_of(left_slots[0]);
        address right = address_of(right_slots[0]);
        address shifts = address_of(shift_slots[0]);
        PayloadEmission payload = PayloadEmission(PayloadAccess::Local, component.raw_type, input.uint_type,
            4, component.count, out.next);

        if (reduction) {
            left = word_at(own, 2u64 * u64(index));
        } else {
            tree_node_address(out, component, input.uint_type, other, left);
        }
        u32 a = read_stage_payload(out, payload, left, shifts);
        if (!reduction && local_slots) {
            right = word_at(own, 2u64 * u64(index));
        } else {
            tree_node_address(out, component, input.uint_type, reduction ? other : node, right);
        }
        u32 b = read_stage_payload(out, payload, right, shifts);
        bool reverse = input.down != 0 && component.operation != CollectiveOperation::Sum;
        CollectivePair pair = CollectivePair(component.operation, component.scalar, component.raw_type,
            component.value_type, input.bool_type, reverse ? b : a, reverse ? a : b, out.next);
        CollectiveWords combined = collective_pair(pair, word_at(out.body, out.words), word_at(decorations, decorated));
        out.words += combined.body;
        decorated += combined.decorations;
        out.next = combined.next;

        u32 value = combined.value;
        bool floating = component.scalar == BuiltinScalar::F32 || component.scalar == BuiltinScalar::F64;
        if (input.down == 0 && floating && component.operation != CollectiveOperation::Sum) {
            if (has_right == 0) {
                u32 right_begin = reduction
                    ? stage_op(out, 128, input.uint_type, global_index, input.stride)
                    : stage_op(out, 128, input.uint_type, other, input.one);
                has_right = stage_op(out, 176, input.bool_type, right_begin, input.remaining);
            }
            value = stage_select(out, component.raw_type, has_right, value, a);
        }
        if (input.down != 0) {
            write_stage_payload(out, payload, b, left, shifts);
        }
        write_stage_payload(out, payload, value, reduction ? left : right, shifts);
    }
    stage_close(out, branch.end);
    stage_barrier(out, input.scope, input.semantics);
    return TreeWords(out.words, decorated, out.next, branch.end);
}

PrivateAddressWords private_address(PrivateAddress input, address parts, address pointers, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    u32 position = input.index;
    if (input.offset != 0) {
        at = words_four(body, at, 128, input.index_type, next, position, input.offset);
        position = next;
        next += 1;
    }
    if (input.width != 0) {
        at = words_four(body, at, 134, input.index_type, next, position, input.width);
        position = next;
        next += 1;
    }
    for (part in 0..input.count) {
        u32 index = read_word(parts, part);
        if (position != 0) {
            if (part == 0) {
                index = position;
            } else {
                at = words_four(body, at, 128, input.index_type, next, position, index);
                index = next;
                next += 1;
            }
        }
        at = words_four(body, at, 65, input.pointer_type, next, input.variable, index);
        put(pointers, u64(part), next);
        next += 1;
    }
    return PrivateAddressWords(at, next);
}

struct RayIntersection {
    u32 query;
    u32 u32_type;
    u32 f32_type;
    u32 vec2_type;
    u32 maximum;
    u32 committed;
    u32 zero;
    u32 float_zero;
    u32 position_type;
    u32 object_to_world;
    u32 world_to_object;
};

ControlWords ray_closest(RayIntersection input, Span results, BlockValues values, address body, u64 start) on cpu
    requires nonzero(values.scalars.ids) {
    u64 at = start;
    ScalarHost stream = values.scalars;
    address registers = values.registers;
    u32 first = stream.ids(stream.context, 9);
    u32 yes = first;
    u32 no = first + 1;
    u32 merge = first + 2;
    u32 distance = first + 3;
    u32 barycentric = first + 4;
    u32 u = first + 5;
    u32 v = first + 6;
    u32 instance = first + 7;
    u32 primitive = first + 8;
    u32 f = input.f32_type;
    u32 q = input.query;
    u32 committed = input.committed;
    at += open_selection(merge, block_span_register(registers, results, 0u64), yes, no, word_at(body, at)).words;
    at = words_four(body, at, 6018, f, distance, q, committed);
    at = words_four(body, at, 6024, input.vec2_type, barycentric, q, committed);
    at = words_four(body, at, 81, f, u, barycentric, 0);
    at = words_four(body, at, 81, f, v, barycentric, 1);
    at = words_four(body, at, 6020, input.u32_type, instance, q, committed);
    at = words_four(body, at, 6023, input.u32_type, primitive, q, committed);

    u32 positions = 0;
    if (input.position_type != 0) {
        u32 points = stream.ids(stream.context, 10);
        positions = points + 1;
        at = words_four(body, at, 5340, input.position_type, points, q, committed);
        for (part in 0..9) {
            at = words_five(body, at, 81, f, positions + part, points, part / 3, part % 3);
        }
    }

    array<u32,2> types = array<u32,2>(input.object_to_world, input.world_to_object);
    array<u32,2> matrices = array<u32,2>(0, 0);
    for (slot in 0..2) {
        if (types[slot] != 0) {
            u32 matrix = stream.ids(stream.context, 13);
            matrices[slot] = matrix + 1;
            at = words_four(body, at, 6031 + slot, types[slot], matrix, q, committed);
            // SPIR-V uses columns, while the B value has rows.
            for (part in 0..12) {
                at = words_five(body, at, 81, f, matrices[slot] + part, matrix, part % 4, part / 4);
            }
        }
    }

    at += next_control_block(merge, no, 1, word_at(body, at)).words;
    at += close_selection(merge, 1, 1, word_at(body, at)).words;
    u32 z = input.float_zero;
    at = phi_two(body, at, f, block_span_register(registers, results, 1u64), distance, yes, input.maximum, no);
    at = phi_two(body, at, f, block_span_register(registers, results, 2u64), u, yes, z, no);
    at = phi_two(body, at, f, block_span_register(registers, results, 3u64), v, yes, z, no);
    at = phi_two(body, at, input.u32_type, block_span_register(registers, results, 4u64), instance, yes, input.zero, no);
    at = phi_two(body, at, input.u32_type, block_span_register(registers, results, 5u64), primitive, yes, input.zero, no);
    if (positions != 0) {
        for (part in 0..9) {
            u32 dst = block_span_register(registers, results, 6u64 + u64(part));
            at = phi_two(body, at, f, dst, positions + part, yes, z, no);
        }
    }
    for (slot in 0..2) {
        if (matrices[slot] != 0) {
            for (part in 0..12) {
                u32 dst = block_span_register(registers, results, 15u64 + 12u64 * u64(slot) + u64(part));
                at = phi_two(body, at, f, dst, matrices[slot] + part, yes, z, no);
            }
        }
    }
    return ControlWords(at, merge, 1);
}

RayQuery ray_at(Span queries, usize index) on cpu {
    return unsafe_position<RayQuery>(unsafe_address(address_bits(queries.data) + index * size_of<RayQuery>()));
}

RayQuery ray_declaration(Span queries, usize result) on cpu {
    usize low = 0u64;
    usize high = queries.count;
    while (low < high) {
        usize mid = low + (high - low) / 2u64;
        if (ray_at(queries, mid).result <= result) { low = mid + 1u64; }
        else { high = mid; }
    }
    return ray_at(queries, low - 1u64);
}

void block_ray_query(View source, Span direction_values, BlockValues values, RayBindings bindings, StructuredHost host) on cpu
    requires nonzero(bindings.declare_type)
    requires nonzero(host.reserve_words) requires nonzero(host.finish_words)
    requires nonzero(values.scalars.constant) requires nonzero(values.scalars.ids) {
    Span results = source.first;
    RayQuery declared = ray_declaration(bindings.queries, unsafe_position<usize>(results.data));
    u32 used = declared.used;
    ScalarHost stream = values.scalars;
    address context = stream.context;
    address registers = values.registers;
    bool closest = results.count == 39u64;

    u32 position_type = 0;
    if ((used & RAY_POSITIONS) != 0) {
        position_type = read_word(bindings.types, 0);
        if (position_type == 0) {
            u32 length = stream.constant(context, BuiltinScalar::U32, 3u64);
            position_type = bindings.declare_type(context, TypeShape::Array, bindings.vec3, length);
            put(bindings.types, 0u64, position_type);
        }
    }
    u32 transform_type = 0;
    if ((used & (RAY_OBJECT_TO_WORLD | RAY_WORLD_TO_OBJECT)) != 0) {
        transform_type = read_word(bindings.types, 1);
        if (transform_type == 0) {
            transform_type = bindings.declare_type(context, TypeShape::Matrix, bindings.vec3, 4);
            put(bindings.types, 1u64, transform_type);
        }
    }

    bool culling = source.d != 0u64;
    u64 base_flags = closest ? 1u64 : 5u64;
    u32 flags = stream.constant(context, BuiltinScalar::U32, base_flags);
    u32 cull_flags = 0;
    u32 selected = 0;
    if (culling) {
        cull_flags = stream.constant(context, BuiltinScalar::U32, base_flags | 16u64);
        selected = stream.ids(context, 1);
    }
    u32 mask = stream.constant(context, BuiltinScalar::U32, 255u64);
    u32 committed = stream.constant(context, BuiltinScalar::U32, 1u64);
    u32 zero = stream.constant(context, BuiltinScalar::U32, 0u64);
    u32 float_zero = closest ? stream.constant(context, BuiltinScalar::F32, 0u64) : 0;
    u32 u32_type = scalar_id(stream.types, BuiltinScalar::U32);
    u32 bool_type = scalar_id(stream.types, BuiltinScalar::Bool);
    u32 first = stream.ids(context, 9);
    address body = host.reserve_words(host.context, (closest ? 587u64 : 61u64) + (culling ? 6u64 : 0u64));
    u64 at = 0u64;
    if (culling) {
        u32 condition = block_register(registers, source.d - 1u64);
        at = select_words(body, at, u32_type, selected, condition, cull_flags, flags);
        flags = selected;
    }
    u32 acceleration = first;
    u32 origin = first + 1;
    u32 direction = first + 2;
    u32 header = first + 3;
    u32 test = first + 4;
    u32 again = first + 5;
    u32 end = first + 6;
    u32 more = first + 7;
    u32 kind = first + 8;
    u32 query = bindings.base + declared.ordinal;
    at = words_three(body, at, 61, bindings.acceleration_type, acceleration,
                     block_register(bindings.accelerations, source.c));
    at = words_five(body, at, 80, bindings.vec3, origin,
        block_span_register(registers, source.second, 0u64), block_span_register(registers, source.second, 1u64),
        block_span_register(registers, source.second, 2u64));
    at = words_five(body, at, 80, bindings.vec3, direction,
        block_span_register(registers, direction_values, 0u64), block_span_register(registers, direction_values, 1u64),
        block_span_register(registers, direction_values, 2u64));
    u32 maximum = block_register(registers, source.b);
    put(body, at, instruction(4473, 9));
    put(body, at + 1u64, query);
    put(body, at + 2u64, acceleration);
    put(body, at + 3u64, flags);
    put(body, at + 4u64, mask);
    put(body, at + 5u64, origin);
    put(body, at + 6u64, block_register(registers, source.a));
    put(body, at + 7u64, direction);
    put(body, at + 8u64, maximum);
    at += 9u64;

    LoopLabels loop = LoopLabels(header, test, again, again, end);
    at += open_loop(loop, word_at(body, at)).words;
    at = words_three(body, at, 4477, bool_type, more, query);
    at += test_loop(loop, more, word_at(body, at)).words;
    at = words_one(body, at, 249, header);
    at = words_one(body, at, 248, end);
    at = words_four(body, at, 4479, u32_type, kind, query, committed);
    at = words_four(body, at, 171, bool_type, block_span_register(registers, results, 0u64), kind, zero);
    if (closest) {
        u32 f32_type = scalar_id(stream.types, BuiltinScalar::F32);
        RayIntersection intersection = RayIntersection(query, u32_type, f32_type, bindings.vec2, maximum, committed,
            zero, float_zero, position_type, (used & RAY_OBJECT_TO_WORLD) != 0 ? transform_type : 0,
            (used & RAY_WORLD_TO_OBJECT) != 0 ? transform_type : 0);
        host.finish_words(host.context, ray_closest(intersection, results, values, body, at));
    } else {
        host.finish_words(host.context, ControlWords(at, end, 1));
    }
}

struct TileLocation { u32 pointer; u32 byte; };

TileCell tile_storage_cell(TileStorage storage, TileKey key, address context) on cpu requires nonzero(storage.cell) {
    TileKey aligned = TileKey(key.storage, key.owner, key.byte & 0xfffffffc);
    TileCell cell = storage.cell(context, aligned);
    cell.byte += key.byte % 4;
    return cell;
}

TileLocation tile_cell_address(TileCell cell, TilePlanes planes, u32 index, u32 offset, u32 width, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 bias = 0;
    u32 bytes = 0;
    u32 lanes = 0;
    if (offset != 0) {
        if (cell.byte != 0) { bias = stream.constant(context, BuiltinScalar::U32, u64(cell.byte)); }
        bytes = stream.constant(context, BuiltinScalar::U32, u64(scalar_definition(cell.unit).width / 8));
        lanes = stream.constant(context, BuiltinScalar::U32, u64(width));
    }
    u32 base = cell.slot == 0 ? 0 : stream.constant(context, BuiltinScalar::U32, u64(cell.slot * width));
    u32 next = stream.ids(context, 0);
    StageCursor out = StageCursor(stream.reserve(context, 30u64, 0u64).body, 0u64, next);
    u32 ty = scalar_id(stream.types, BuiltinScalar::U32);
    u32 at = index;
    u32 byte = cell.byte;
    if (offset != 0) {
        byte = bias == 0 ? offset : stage_op(out, 128, ty, offset, bias);
        u32 cell_index = stage_op(out, 134, ty, byte, bytes);
        u32 lane_offset = stage_op(out, 132, ty, cell_index, lanes);
        at = stage_op(out, 128, ty, index, lane_offset);
    }
    if (base != 0) { at = stage_op(out, 128, ty, at, base); }
    bool wide = cell.unit == BuiltinScalar::U64;
    u32 pointer_type = wide ? planes.wide_pointer : planes.narrow_pointer;
    u32 variable = wide ? planes.wide : planes.narrow;
    u32 pointer = stage_op(out, 65, pointer_type, variable, at);
    memory.finish_body(context, out.words);
    stream.ids(context, out.next - next);
    return TileLocation(pointer, byte);
}

u32 tile_storage_pointer(TileStorage storage, TileKey key, u32 index, u32 offset, MemoryHost memory) on cpu
    requires nonzero(storage.cell)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    TileCell cell = tile_storage_cell(storage, key, memory.scalars.context);
    return tile_cell_address(cell, storage.planes, index, offset, storage.width, memory).pointer;
}

u32 tile_storage_read(TileStorage storage, TileKey key, u32 index, u32 offset,
    BuiltinScalar scalar, MemoryHost memory) on cpu
    requires nonzero(storage.cell)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    TileCell cell = tile_storage_cell(storage, key, context);
    TileLocation at = tile_cell_address(cell, storage.planes, index, offset, storage.width, memory);
    u32 width = scalar_definition(scalar).width;
    u32 cell_width = scalar_definition(cell.unit).width;
    if (width < cell_width) {
        u32 bytes = cell_width / 8;
        u32 ty = scalar_id(stream.types, BuiltinScalar::U32);
        u32 cell_type = scalar_id(stream.types, cell.unit);
        u32 shift = 0;
        u32 cell_bytes = 0;
        u32 bits_per_byte = 0;
        if (offset == 0) {
            u32 bits = at.byte % bytes * 8;
            if (bits != 0) { shift = stream.constant(context, BuiltinScalar::U32, u64(bits)); }
        } else {
            cell_bytes = stream.constant(context, BuiltinScalar::U32, u64(bytes));
            bits_per_byte = stream.constant(context, BuiltinScalar::U32, 8u64);
        }
        u32 next = stream.ids(context, 0);
        StageCursor out = StageCursor(stream.reserve(context, 19u64, 0u64).body, 0u64, next);
        u32 raw = stage_load(out, cell_type, at.pointer, false);
        if (offset != 0) {
            u32 byte = stage_op(out, 137, ty, at.byte, cell_bytes);
            shift = stage_op(out, 132, ty, byte, bits_per_byte);
        }
        if (shift != 0) { raw = stage_op(out, 194, cell_type, raw, shift); }
        memory.finish_body(context, out.words);
        stream.ids(context, out.next - next);
        return emit_payload_load(PayloadShape(PayloadAccess::Words, scalar, cell.unit, 1), address_of(raw), 0, memory);
    }
    array<u32,2> pointers = array<u32,2>(at.pointer, 0);
    u32 count = 1;
    if (width > cell_width) {
        TileKey high = TileKey(key.storage, key.owner, key.byte + 4);
        pointers[1] = tile_storage_pointer(storage, high, index, offset, memory);
        count = 2;
    }
    return emit_payload_load(PayloadShape(PayloadAccess::Local, scalar, cell.unit, count), address_of(pointers[0]), 0, memory);
}

void block_tile_read(BlockValues values, TileStorage storage, TileKey key, u32 index, u32 offset,
    usize dst, MemoryHost memory) on cpu
    requires nonzero(values.scalar) requires nonzero(storage.cell)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    BuiltinScalar scalar = values.scalar(memory.scalars.context, dst);
    u32 raw = tile_storage_read(storage, key, index, offset, scalar, memory);
    PayloadShape shape = PayloadShape(PayloadAccess::Words, scalar, unsigned_scalar(scalar_definition(scalar).width), 1);
    emit_payload_load(shape, address_of(raw), block_register(values.registers, dst), memory);
}

void block_tile_write(BlockValues values, TileStorage storage, TileKey key, u32 index, u32 offset,
    usize src, MemoryHost memory) on cpu
    requires nonzero(values.scalar) requires nonzero(storage.cell)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    BuiltinScalar scalar = values.scalar(memory.scalars.context, src);
    PhysicalShape shape = physical_shape(scalar);
    array<u32,2> pointers = array<u32,2>(0);
    for (part in 0..shape.count) {
        TileKey word = TileKey(key.storage, key.owner, key.byte + part * 4);
        pointers[part] = tile_storage_pointer(storage, word, index, offset, memory);
    }
    u32 raw = encode_raw_bits(scalar, block_register(values.registers, src), memory);
    emit_payload_store(PayloadShape(PayloadAccess::Physical, scalar, shape.unit, shape.count),
        raw, address_of(pointers[0]), address(), memory);
}

void emit_shared_store(TileCell cell, TilePlanes planes, u32 index, u32 width, u32 value, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    u32 pointer = tile_cell_address(cell, planes, index, 0, width, memory).pointer;
    ScalarHost stream = memory.scalars;
    address body = stream.reserve(stream.context, 3u64, 0u64).body;
    memory.finish_body(stream.context, plain_store(body, 0u64, pointer, value));
}

void pack_record_field(PackedField field, u32 word_type, u32 zero, address packed, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 bits = field.offset % 4 * 8;
    u32 shift = bits == 0 ? 0 : stream.constant(context, BuiltinScalar::U32, u64(bits));
    u32 high = field.bytes > 4 ? stream.constant(context, BuiltinScalar::U32, 32u64) : 0;
    u32 next = stream.ids(context, 0);
    u64 capacity = field.bytes > 4 ? 33u64 : 14u64;
    StageCursor out = StageCursor(stream.reserve(context, capacity, 0u64).body, 0u64, next);
    pack_field(out, field, word_type, zero, shift, high, packed);
    memory.finish_body(context, out.words);
    stream.ids(context, out.next - next);
}

TileInputRow next_tile_snapshot(TileInputSource source) on cpu
    requires nonzero(source.row) {
    TileInputRow row = source.row(source.cursor);
    while (row.cell.present != 0) {
        if (row.cell.unit == BuiltinScalar::U64 ? row.cell.byte == 0 : row.fragments != 0u64) {
            break;
        }
        row = source.row(source.cursor);
    }
    return row;
}

extern "C" TileInputRange emit_tile_input(TileInputLayout input, TileInputSource source, MemoryHost memory) on cpu
    requires nonzero(source.row) requires nonzero(source.fragment)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 width = stream.constant(context, BuiltinScalar::U32, u64(input.width));
    u32 uint_type = scalar_id(stream.types, BuiltinScalar::U32);
    u32 bool_type = scalar_id(stream.types, BuiltinScalar::Bool);
    u32 next = stream.ids(context, 0);
    address range_body = stream.reserve(context, 26u64, 0u64).body;
    TileRangeWords range = tile_range(range_body, uint_type, bool_type, input.page.index, input.count, width, next);
    memory.finish_body(context, range.words);
    stream.ids(context, range.next - next);

    TileInputRow row = next_tile_snapshot(source);
    if (row.cell.present == 0) { return TileInputRange(range.lane, range.length, input.label); }

    u32 load = stream.ids(context, 2);
    u32 end = load + 1;
    address selection = stream.reserve(context, 9u64, 0u64).body;
    memory.finish_body(context, open_selection(end, input.active, load, end, selection).words);
    // Each aligned word has four positions and three fragment widths (1, 2, 4 bytes).
    // The ordered input set has unique (position, width) pairs.
    array<PackedField,12> fields = array<PackedField,12>(PackedField(0, 0, 0, 0));
    address scratch = address_of(fields[0]);
    while (row.cell.present != 0) {
        PageInputs page = input.page;
        page.descriptor = block_register(input.descriptors, row.binding);
        page.word = row.word;
        u32 value;
        if (row.cell.unit == BuiltinScalar::U64) {
            value = physical_scalar_read(PhysicalAccess(page, BuiltinScalar::U64, 0, 0), memory);
        } else {
            usize part = 0u64;
            while (part < row.fragments) {
                TileInputFragment fragment = source.fragment(source.cursor);
                page.word = fragment.byte;
                u32 raw = physical_scalar_read(PhysicalAccess(page, fragment.unit, 0, 0), memory);
                u32 ty = scalar_id(stream.types, fragment.unit);
                u32 bytes = scalar_definition(fragment.unit).width / 8;
                address slot = unsafe_address(address_bits(scratch) + part * size_of<PackedField>());
                unsafe_position<PackedField>(slot) = PackedField(raw, fragment.byte % 4, ty, bytes);
                part += 1u64;
            }
            u32 zero = stream.constant(context, BuiltinScalar::U32, 0u64);
            value = zero;
            part = 0u64;
            while (part < row.fragments) {
                address slot = unsafe_address(address_bits(scratch) + part * size_of<PackedField>());
                pack_record_field(unsafe_position<PackedField>(slot), uint_type, zero, address_of(value), memory);
                part += 1u64;
            }
        }
        emit_shared_store(row.cell, input.planes, range.lane, input.width, value, memory);
        row = next_tile_snapshot(source);
    }

    address merge = stream.reserve(context, 4u64, 0u64).body;
    memory.finish_body(context, next_control_block(end, end, 1, merge).words);
    tile_workgroup_sync(0x148, memory);
    return TileInputRange(range.lane, range.length, end);
}

void tile_workgroup_sync(u32 semantics_value, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 scope = stream.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = stream.constant(context, BuiltinScalar::U32, u64(semantics_value));
    address barrier = stream.reserve(context, 4u64, 0u64).body;
    memory.finish_body(context, workgroup_barrier(barrier, 0u64, scope, semantics));
}

void block_tile_publish(View publication, BlockValues values, TileStorage storage, MemoryHost memory) on cpu
    requires nonzero(storage.cell) requires nonzero(values.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    Span registers = publication.first;
    Span offsets = publication.second;
    u32 word_type = scalar_id(stream.types, BuiltinScalar::U32);
    u32 packed_word = 0;
    u32 packed = 0;
    // Publication fields follow the value layout. Pruning keeps their relative order.
    usize i = 0u64;
    while (i < registers.count) {
        address field = unsafe_address(address_bits(offsets.data) + i * offsets.stride);
        u32 offset = unsafe_position<u32>(field);
        u32 word = offset - offset % 4;
        if (packed != 0 && word != packed_word) {
            TileKey key = TileKey(1u64, publication.a, packed_word);
            emit_shared_store(tile_storage_cell(storage, key, context), storage.planes, storage.lane, storage.width, packed, memory);
            packed = 0;
        }
        address reg = unsafe_address(address_bits(registers.data) + i * registers.stride);
        usize source = unsafe_position<usize>(reg);
        BuiltinScalar type = values.scalar(context, source);
        u32 raw = encode_raw_bits(type, block_register(values.registers, source), memory);
        u32 width = scalar_definition(type).width;
        if (width == 64) {
            TileKey key = TileKey(1u64, publication.a, offset);
            TileCell cell = tile_storage_cell(storage, key, context);
            if (cell.present != 0) { emit_shared_store(cell, storage.planes, storage.lane, storage.width, raw, memory); }
        } else {
            u32 raw_type = scalar_id(stream.types, unsigned_scalar(width));
            packed_word = word;
            pack_record_field(PackedField(raw, offset % 4, raw_type, width / 8), word_type, 0, address_of(packed), memory);
        }
        i += 1u64;
    }
    if (packed != 0) {
        TileKey key = TileKey(1u64, publication.a, packed_word);
        emit_shared_store(tile_storage_cell(storage, key, context), storage.planes, storage.lane, storage.width, packed, memory);
    }
}

array<u32,2> tile_tree_pointers(TreeComponent component, u32 node, u32 uint_type, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    array<u32,2> pointers = array<u32,2>(component.low, component.high);
    u32 next = stream.ids(context, 0);
    StageCursor out = StageCursor(stream.reserve(context, 25u64, 0u64).body, 0u64, next);
    tree_node_address(out, component, uint_type, node, address_of(pointers[0]));
    memory.finish_body(context, out.words);
    stream.ids(context, out.next - next);
    return pointers;
}

void tile_tree_store(TreeComponent component, u32 node, u32 value, u32 uint_type, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    array<u32,2> pointers = tile_tree_pointers(component, node, uint_type, memory);
    emit_payload_store(PayloadShape(PayloadAccess::Local, component.scalar, BuiltinScalar::U32, component.count),
        value, address_of(pointers[0]), address(), memory);
}

void collective_tree(CollectiveLayout layout, CollectiveTree tree, ScalarHost host,
    address components) on cpu
    requires nonzero(layout.component) requires nonzero(host.constant) {
    usize index = 0u64;
    while (index < layout.count) {
        CollectiveComponent field = layout.component(layout.source, index);
        u32 width = scalar_definition(field.scalar).width;
        u32 base = (field.offset / 4) * 64;
        u32 low = base != 0 ? host.constant(host.context, BuiltinScalar::U32, u64(base)) : 0;
        u32 high = 0;
        u32 scale = 0;
        u32 shift = 0;
        if (width == 64) {
            high = host.constant(host.context, BuiltinScalar::U32, u64(base + 1));
            scale = host.constant(host.context, BuiltinScalar::U32, 2u64);
            shift = host.constant(host.context, BuiltinScalar::U32, 32u64);
        }
        address at = unsafe_address(address_bits(components) + index * size_of<TreeComponent>());
        unsafe_position<TreeComponent>(at) = TreeComponent(tree.shared, tree.pointer, low, high,
            scale, shift, (width + 31) / 32, tree.operation, field.scalar,
            scalar_id(host.types, unsigned_scalar(width)), scalar_id(host.types, field.scalar));
        index += 1u64;
    }
}

void collective_fields(CollectiveLayout layout, u32 source_word, u32 has_source,
    MemoryHost host, address fields) on cpu
    requires nonzero(layout.component) requires nonzero(host.scalars.constant)
    {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    usize index = 0u64;
    while (index < layout.count) {
        CollectiveComponent field = layout.component(layout.source, index);
        PhysicalShape shape = physical_shape(field.scalar);
        u32 source = 0;
        u32 slot = field.offset / 4;
        if (has_source != 0) {
            source = field_byte_id(stream, source_word + field.offset);
            slot = 2 * u32(index);
        }
        u32 target = field_byte_id(stream, field.offset);
        u32 step = high_word_step(stream, shape);
        u32 byte = field.offset % 4;
        u32 shift = shape.bytes < 4 && byte != 0 ? stream.constant(context, BuiltinScalar::U32, u64(byte * 8)) : 0;
        address at = unsafe_address(address_bits(fields) + index * size_of<CollectiveField>());
        unsafe_position<CollectiveField>(at) = CollectiveField(source, target, scalar_id(stream.types, shape.unit),
            shape.bytes, scalar_id(host.scalars.physical_pointers, shape.unit), step, shift, slot);
        index += 1u64;
    }
}

u32 field_byte_id(ScalarHost host, u32 byte_offset) on cpu requires nonzero(host.constant) {
    return byte_offset == 0 ? 0 : host.constant(host.context, BuiltinScalar::U64, u64(byte_offset));
}

u32 high_word_step(ScalarHost host, PhysicalShape shape) on cpu requires nonzero(host.constant) {
    return shape.count == 2 ? host.constant(host.context, BuiltinScalar::U64, u64(shape.bytes)) : 0;
}

u32 high_word_shift(ScalarHost host, PhysicalShape shape) on cpu requires nonzero(host.constant) {
    return shape.count == 2 ? host.constant(host.context, BuiltinScalar::U32, 32u64) : 0;
}

void physical_fields(CollectiveLayout layout, u32 first, MemoryHost host, address output) on cpu
    requires nonzero(layout.component) requires nonzero(host.scalars.constant)
    {
    ScalarHost stream = host.scalars;
    usize index = 0u64;
    while (index < layout.count) {
        CollectiveComponent component = layout.component(layout.source, index);
        PhysicalShape shape = physical_shape(component.scalar);
        u32 step = high_word_step(stream, shape);
        u32 shift = high_word_shift(stream, shape);
        u32 offset = field_byte_id(stream, first + component.offset);
        address at = unsafe_address(address_bits(output) + index * size_of<PhysicalField>());
        unsafe_position<PhysicalField>(at) = PhysicalField(component.scalar, scalar_id(stream.types, shape.raw),
            scalar_id(stream.types, component.scalar), scalar_id(stream.types, shape.unit), shape.bytes,
            scalar_id(host.scalars.physical_pointers, shape.unit), offset, step, shift);
        index += 1u64;
    }
}

void tile_collective_layer(u32 stride, u32 padded, u32 down, address components, u32 count,
    BlockHost emit, MemoryHost memory, StructuredHost control) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(emit.values.finish) requires nonzero(control.finish_words) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 selector = stream.constant(context, BuiltinScalar::U32, u64(stride * 2));
    u32 capacity = stream.constant(context, BuiltinScalar::U32, u64(padded));
    u32 distance = stream.constant(context, BuiltinScalar::U32, u64(stride));
    u32 one = stream.constant(context, BuiltinScalar::U32, 1u64);
    u32 scope = stream.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = stream.constant(context, BuiltinScalar::U32, 0x108u64);
    u32 start = stream.ids(context, 0);
    TreeLayer input = TreeLayer(scalar_id(stream.types, BuiltinScalar::U32), scalar_id(stream.types, BuiltinScalar::Bool),
        emit.tile.lane, distance, selector, capacity, emit.tile.length, one, scope, semantics, down, start);
    ScalarStreams words = stream.reserve(context, 42u64 + 176u64 * u64(count), 3u64 * u64(count));
    StageCursor out = StageCursor(words.body, 0u64, start);
    TreeWords result = stage_tree_layer(out, input, 0, components, count, address(), words.decorations);
    control.finish_words(control.context, ControlWords(result.body, result.label, 1));
    emit.values.finish(context, ScalarWords(0u64, result.decorations));
    stream.ids(context, result.next - start);
}

void block_tile_collective(View operation, BlockHost emit, MemoryHost memory, StructuredHost control) on cpu
    requires nonzero(emit.tile.cell) requires nonzero(emit.ir.inspect)
    requires nonzero(emit.tile.components)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.finish) requires nonzero(emit.paging.current_label)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body)
    requires nonzero(control.labels) requires nonzero(control.reserve_words) requires nonzero(control.finish_words) {
    if (operation.first.count == 0u64) {
        tile_workgroup_sync(0x148, memory);
        return;
    }
    TileStorage storage = emit.tile;
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    address control_context = control.context;
    CollectiveOperation opcode = CollectiveOperation(u32(operation.b));
    u32 uint_type = scalar_id(stream.types, BuiltinScalar::U32);
    u32 bool_type = scalar_id(stream.types, BuiltinScalar::Bool);
    u32 zero = stream.constant(context, BuiltinScalar::U32, 0u64);
    u64 padded_width = 1u64;
    while (padded_width < u64(storage.width)) { padded_width *= 2u64; }
    u32 padded = u32(padded_width);
    u32 batch = opcode == CollectiveOperation::Sum ? 1 : u32(operation.first.count);
    address components = storage.components(context, u64(batch));
    usize first = 0u64;
    while (first < operation.first.count) {
        u32 word = storage.data_words;
        for (slot in 0..batch) {
            usize row = first + u64(slot);
            usize register = unsafe_position<usize>(unsafe_address(address_bits(operation.first.data) + row * operation.first.stride));
            u32 offset = unsafe_position<u32>(unsafe_address(address_bits(operation.second.data) + row * operation.second.stride));
            BuiltinScalar scalar = emit.values.scalar(context, register);
            ScalarDefinition type = scalar_definition(scalar);
            BuiltinScalar raw = unsigned_scalar(type.width);
            u32 parts = (type.width + 31) / 32;
            u64 bits = collective_identity(opcode, type.width, type.signed, u32(type.opcode == 22));
            u32 identity = stream.constant(context, raw, bits);
            u32 scale = parts == 2 ? stream.constant(context, BuiltinScalar::U32, 2u64) : 0;
            u32 low = word != 0 ? stream.constant(context, BuiltinScalar::U32, u64(word)) : 0;
            u32 high = parts == 2 ? stream.constant(context, BuiltinScalar::U32, u64(word + 1)) : 0;
            u32 shift = parts == 2 ? stream.constant(context, BuiltinScalar::U32, 32u64) : 0;
            TreeComponent component = TreeComponent(storage.planes.narrow, storage.planes.narrow_pointer, low, high, scale, shift, parts,
                opcode, scalar, scalar_id(stream.types, raw), scalar_id(stream.types, scalar));
            address component_at = unsafe_address(address_bits(components) + u64(slot) * size_of<TreeComponent>());
            unsafe_position<TreeComponent>(component_at) = component;
            word += padded * parts;

            u32 before = emit.paging.current_label(context);
            u32 test = control.labels(control_context, 3);
            u32 read = test + 1;
            u32 end = test + 2;
            address branch = control.reserve_words(control_context, 14u64);
            control.finish_words(control_context, open_index_selection(bool_type, test, storage.lane, storage.length, true, read, end, branch));
            TileKey key = tile_key(storage.tiles, operation.a, offset, emit.ir);
            u32 input = tile_storage_read(storage, key, storage.lane, 0, scalar, memory);
            control.finish_words(control_context, next_control_block(end, end, 1, control.reserve_words(control_context, 4u64)));
            u32 value = stream.ids(context, 1);
            address merge = control.reserve_words(control_context, 7u64);
            control.finish_words(control_context, ControlWords(phi_two(merge, 0u64, component.raw_type,
                value, input, read, identity, before), 0, 1));
            tile_tree_store(component, storage.lane, value, uint_type, memory);

            if (padded > storage.width) {
                u32 capacity = stream.constant(context, BuiltinScalar::U32, u64(padded));
                u32 width = stream.constant(context, BuiltinScalar::U32, u64(storage.width));
                u32 index = control.labels(control_context, 4);
                u32 pad_end = index + 3;
                address body = control.reserve_words(control_context, 19u64);
                control.finish_words(control_context, open_tile_padding(uint_type, bool_type, storage.lane, width, capacity,
                    index, index + 1, index + 2, pad_end, body));
                tile_tree_store(component, index, identity, uint_type, memory);
                control.finish_words(control_context, next_control_block(pad_end, pad_end, 1, control.reserve_words(control_context, 4u64)));
            }
        }
        tile_workgroup_sync(0x148, memory);
        u32 stride = 1;
        while (stride < padded) {
            tile_collective_layer(stride, padded, 0, components, batch, emit, memory, control);
            stride *= 2;
        }
        u32 index = storage.lane;
        if (operation.c != 0u64) {
            u32 test = control.labels(control_context, 3);
            u32 end = test + 2;
            address branch = control.reserve_words(control_context, 14u64);
            control.finish_words(control_context, open_index_selection(bool_type, test, storage.lane, zero, false, test + 1, end, branch));
            u32 root = stream.constant(context, BuiltinScalar::U32, u64(padded - 1));
            for (slot in 0..batch) {
                TreeComponent component = unsafe_position<TreeComponent>(
                    unsafe_address(address_bits(components) + u64(slot) * size_of<TreeComponent>()));
                ScalarDefinition type = scalar_definition(component.scalar);
                BuiltinScalar raw = unsigned_scalar(type.width);
                u64 bits = collective_identity(opcode, type.width, type.signed, u32(type.opcode == 22));
                u32 identity = stream.constant(context, raw, bits);
                tile_tree_store(component, root, identity, uint_type, memory);
            }
            control.finish_words(control_context, next_control_block(end, end, 1, control.reserve_words(control_context, 4u64)));
            tile_workgroup_sync(0x108, memory);
            stride = padded / 2;
            while (stride != 0) {
                tile_collective_layer(stride, padded, 1, components, batch, emit, memory, control);
                stride /= 2;
            }
        } else {
            index = stream.constant(context, BuiltinScalar::U32, u64(padded - 1));
        }
        for (slot in 0..batch) {
            TreeComponent component = unsafe_position<TreeComponent>(
                unsafe_address(address_bits(components) + u64(slot) * size_of<TreeComponent>()));
            array<u32,2> pointers = tile_tree_pointers(component, index, uint_type, memory);
            u32 result = block_span_register(emit.values.registers, operation.first, first + u64(slot));
            emit_payload_load(PayloadShape(PayloadAccess::Local, component.scalar, BuiltinScalar::U32, component.count),
                address_of(pointers[0]), result, memory);
        }
        // All lanes take the result before another batch can reuse the shared words.
        tile_workgroup_sync(0x148, memory);
        first += u64(batch);
    }
}

array<u32,2> stage_field_pointers(inout StageCursor out, u32 record, u32 offset, u32 pointer, u32 wide_type, u32 step) on cpu {
    u32 physical = offset == 0 ? record : stage_op(out, 128, wide_type, record, offset);
    u32 low = out.next;
    out.words = words_three(out.body, out.words, 120, pointer, low, physical);
    out.next += 1;
    u32 high = 0;
    if (step != 0) {
        u32 second = stage_op(out, 128, wide_type, physical, step);
        high = out.next;
        out.words = words_three(out.body, out.words, 120, pointer, high, second);
        out.next += 1;
    }
    return array<u32,2>(low, high);
}

u32 collective_record(inout StageCursor out, PageAddress page) on cpu {
    PhysicalAddress record = physical_page_address(page, word_at(out.body, out.words), out.next);
    out.words += record.words;
    out.next = record.next;
    return record.value;
}

PageAddress reduction_target(inout StageCursor out, ReductionWriteback input) on cpu {
    PageAddress target = input.target;
    u32 u = target.u32_type;
    u32 group = stage_op(out, 134, u, target.index, input.width);
    u32 pointer = out.next;
    out.words = words_five(out.body, out.words, 65, input.push_pointer, pointer, input.push,
        target.zero, input.length_word);
    out.next += 1;
    u32 length = stage_load(out, u, pointer, false);
    u32 final_pass = stage_op(out, 170, input.bool_type, length, target.one);
    target.index = stage_select(out, u, final_pass, target.zero, group);
    if (input.bias != 0) {
        target.extra = stage_select(out, u, final_pass, input.bias, target.zero);
    }
    return target;
}


void write_collective_record(inout StageCursor out, PageAddress target, address fields,
    u32 count, address values, bool shared, address locations) on cpu {
    bool located = address_bits(locations) != 0u64;
    u32 record = located ? 0 : collective_record(out, target);
    u32 u = target.u32_type;
    for (index in 0..count) {
        let field = unsafe_position<CollectiveField>(
            unsafe_address(address_bits(fields) + u64(index) * size_of<CollectiveField>()));
        array<u32,2> to;
        if (located) {
            to = array<u32,2>(read_word(locations, 2 * index), read_word(locations, 2 * index + 1));
        } else {
            to = stage_field_pointers(out, record, field.target_offset, field.pointer_type, target.u64_type, field.step);
        }
        u32 low = read_word(values, field.slot);
        if (shared) { low = stage_load(out, u, low, false); }
        if (field.shift != 0) {
            low = stage_op(out, 194, u, low, field.shift);
        }
        PayloadEmission narrowed = PayloadEmission(PayloadAccess::Words, u, field.unit_type,
            field.unit_bytes, 1, out.next);
        low = payload_word(out, narrowed, low, 0);
        out.words = aligned_store(out.body, out.words, to[0], low, field.unit_bytes);
        if (field.step != 0) {
            u32 high = read_word(values, field.slot + 1);
            if (shared) { high = stage_load(out, u, high, false); }
            out.words = aligned_store(out.body, out.words, to[1], high, 4);
        }
    }

}

void read_collective_record(inout StageCursor out, read PageAddress target, address fields,
    u32 count, address values, address locations) on cpu {
    u32 record = collective_record(out, target);
    u32 u = target.u32_type;
    for (index in 0..count) {
        let field = unsafe_position<CollectiveField>(
            unsafe_address(address_bits(fields) + u64(index) * size_of<CollectiveField>()));
        array<u32,2> at = stage_field_pointers(out, record, field.target_offset, field.pointer_type,
            target.u64_type, field.step);
        put(locations, 2u64 * u64(index), at[0]);
        put(locations, 2u64 * u64(index) + 1u64, at[1]);
        u32 low = stage_aligned_load(out, field.unit_type, at[0], field.unit_bytes);
        if (field.unit_bytes < 4) {
            PayloadEmission widened = PayloadEmission(PayloadAccess::Words, field.unit_type, u, 4, 1, out.next);
            low = payload_word(out, widened, low, 0);
            if (field.shift != 0) { low = stage_op(out, 196, u, low, field.shift); }
            u32 prior = read_word(values, field.slot);
            if (prior != target.zero) { low = stage_op(out, 197, u, prior, low); }
        }
        put(values, u64(field.slot), low);
        if (field.step != 0) {
            put(values, u64(field.slot + 1), stage_aligned_load(out, u, at[1], 4));
        }
    }
}

u32 write_reduction(inout StageCursor out, ReductionWriteback input, u32 lane, u32 root,
    address fields, u32 count, address pointers) on cpu {
    u32 owner = stage_op(out, 170, input.bool_type, lane, root);
    StageBranch write = stage_open(out, owner);
    PageAddress target = reduction_target(out, input);
    write_collective_record(out, target, fields, count, pointers, true, address());
    stage_close(out, write.end);
    return write.end;
}

StageWords reduction_writeback(ReductionWriteback input, u32 lane, u32 root,
    address fields, u32 count, address pointers, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    u32 label = write_reduction(out, input, lane, root, fields, count, pointers);
    return StageWords(out.words, out.next, label);
}


void initialize_collective(inout StageCursor out, PageAddress source, CollectiveInput input,
    u32 bool_type, bool normalize, address components, address fields, u32 count, address workspace) on cpu {
    u32 u = source.u32_type;
    address values = word_at(workspace, 2u64 * u64(count));

    StageBranch read = stage_open(out, input.active);
    u32 record = collective_record(out, source);
    for (index in 0..count) {
        let field = unsafe_position<CollectiveField>(
            unsafe_address(address_bits(fields) + u64(index) * size_of<CollectiveField>()));
        array<u32,2> from = stage_field_pointers(out, record, field.source_offset, field.pointer_type,
            source.u64_type, field.step);
        u32 low = stage_aligned_load(out, field.unit_type, from[0], field.unit_bytes);
        PayloadEmission widened = PayloadEmission(PayloadAccess::Words, field.unit_type, u, 4, 1, out.next);
        low = payload_word(out, widened, low, 0);
        if (normalize) {
            u32 keep = stage_op(out, 171, bool_type, low, source.zero);
            low = stage_select(out, u, keep, source.one, source.zero);
        }
        put(values, 2u64 * u64(index), low);
        if (field.step != 0) {
            put(values, 2u64 * u64(index) + 1u64, stage_aligned_load(out, u, from[1], 4));
        }
    }
    stage_close(out, read.end);

    for (index in 0..count) {
        let component = unsafe_position<TreeComponent>(
            unsafe_address(address_bits(components) + u64(index) * size_of<TreeComponent>()));
        u32 slot = 2 * index;
        u32 low = stage_phi(out, u, read_word(values, slot), read.run, input.identity[0], input.before);
        put(values, u64(slot), low);
        if (component.count == 2) {
            u32 high = stage_phi(out, u, read_word(values, slot + 1), read.run, input.identity[1], input.before);
            put(values, u64(slot + 1), high);
        }
    }
    for (index in 0..count) {
        let component = unsafe_position<TreeComponent>(
            unsafe_address(address_bits(components) + u64(index) * size_of<TreeComponent>()));
        u32 slot = 2 * index;
        address own = word_at(workspace, u64(slot));
        put(own, 0u64, component.low);
        put(own, 1u64, component.high);
        tree_node_address(out, component, u, input.lane, own);
        for (part in 0..component.count) {
            out.words = words_two(out.body, out.words, 62, read_word(own, part), read_word(values, slot + part));
        }
    }
    stage_barrier(out, input.scope, input.semantics);
}

TreeWords reduction_group(ReductionGroup input, address strides, address components,
    address fields, u32 count, address workspace, address body, address decorations) on cpu {
    PageAddress source = input.output.target;
    source.descriptor = input.lanes.source;
    StageCursor out = StageCursor(body, 0u64, source.next);
    u32 u = source.u32_type;
    u32 b = input.output.bool_type;
    initialize_collective(out, source, input.lanes, b, false, components, fields, count, workspace);

    u64 decorated = 0u64;
    for (level in 0..6) {
        TreeLayer layer = TreeLayer(u, b, input.lanes.lane, read_word(strides, level), 0, 0,
            input.lanes.count, 0, input.lanes.scope, input.lanes.semantics, 0, out.next);
        TreeWords part = stage_tree_layer(out, layer, source.index, components, count, workspace,
            word_at(decorations, decorated));
        decorated += part.decorations;
    }
    u32 label = write_reduction(out, input.output, input.lanes.lane, source.zero, fields, count, workspace);
    return TreeWords(out.words, decorated, out.next, label);
}


u64 scan_tree_levels(inout StageCursor out, ScanGroup input, u32 remaining, bool down,
    address levels, address components, u32 count, address pointers, address decorations) on cpu {
    u64 decorated = 0u64;
    for (index in 0..6) {
        u32 ordinal = down ? 5 - index : index;
        let level = unsafe_position<ScanLevel>(unsafe_address(address_bits(levels) + u64(ordinal) * size_of<ScanLevel>()));
        TreeLayer layer = TreeLayer(input.target.u32_type, input.bool_type, input.lanes.lane, level.stride,
            level.mask, 0, remaining, input.target.one, input.lanes.scope, input.lanes.semantics, down ? 1 : 0, out.next);
        TreeWords words = stage_tree_layer(out, layer, 0, components, count, pointers, word_at(decorations, decorated));
        decorated += words.decorations;
    }
    return decorated;
}

TreeWords scan_group(ScanGroup input, address levels, address components,
    address fields, u32 count, address workspace, address body, address decorations) on cpu {
    PageAddress target = input.target;
    PageAddress source = target;
    source.descriptor = input.lanes.source;
    source.word = 0;
    StageCursor out = StageCursor(body, 0u64, target.next);
    u32 u = target.u32_type;
    u32 b = input.bool_type;
    initialize_collective(out, source, input.lanes, b, input.normalize != 0, components, fields, count, workspace);

    let first = unsafe_position<TreeComponent>(components);
    bool extreme = (first.scalar == BuiltinScalar::F32 || first.scalar == BuiltinScalar::F64)
        && first.operation != CollectiveOperation::Sum;
    u32 remaining = extreme ? stage_group_remaining(out, u, target.index, input.lanes.lane, input.lanes.count) : 0;
    u64 decorated = scan_tree_levels(out, input, remaining, false, levels, components, count, workspace, decorations);

    u32 last = stage_op(out, 170, b, input.lanes.lane, input.last);
    StageBranch save = stage_open(out, last);
    if (input.summary != 0) {
        PageAddress summary = target;
        summary.descriptor = input.summary;
        summary.index = stage_op(out, 134, u, target.index, input.width);
        summary.word = 0;
        write_collective_record(out, summary, fields, count, workspace, true, address());
    }

    for (index in 0..count) {
        let component = unsafe_position<TreeComponent>(
            unsafe_address(address_bits(components) + u64(index) * size_of<TreeComponent>()));
        u32 slot = 2 * index;
        out.words = words_two(body, out.words, 62, read_word(workspace, slot), input.lanes.identity[0]);
        if (component.count == 2) {
            out.words = words_two(body, out.words, 62, read_word(workspace, slot + 1), input.lanes.identity[1]);
        }
    }
    stage_close(out, save.end);
    stage_barrier(out, input.lanes.scope, input.lanes.semantics);

    decorated += scan_tree_levels(out, input, remaining, true, levels, components, count, workspace,
        word_at(decorations, decorated));
    StageBranch write = stage_open(out, input.lanes.active);
    write_collective_record(out, target, fields, count, workspace, true, address());
    stage_close(out, write.end);
    return TreeWords(out.words, decorated, out.next, write.end);
}


u64 write_scan_prefix(inout StageCursor out, ScanPrefix input, u32 group, u32 first,
    address fields, u32 count, address decorations) on cpu {
    address body = out.body;

    PageAddress parent = input.target;
    parent.descriptor = input.prefix_descriptor;
    parent.index = group;
    parent.word = 0;
    PhysicalAddress prefix = physical_page_address(parent, word_at(body, out.words), out.next);
    out.words += prefix.words;
    PhysicalAddress target = physical_page_address(input.target, word_at(body, out.words), prefix.next);
    out.words += target.words;
    out.next = target.next;

    u64 decorated = 0u64;

    for (index in 0..count) {
        let field = unsafe_position<PhysicalField>(unsafe_address(address_bits(fields) + u64(index) * size_of<PhysicalField>()));
        array<u32,2> from = stage_field_pointers(out, prefix.value, field.offset, field.pointer_type,
            input.target.u64_type, field.step);
        array<u32,2> to = stage_field_pointers(out, target.value, field.offset, field.pointer_type,
            input.target.u64_type, field.step);
        array<u32,2> shifts = array<u32,2>(0, field.shift);
        address from_slots = address_of(from[0]);
        address to_slots = address_of(to[0]);
        address shift_slots = address_of(shifts[0]);
        u32 parts = field.step == 0 ? 1 : 2;
        PayloadEmission payload = PayloadEmission(PayloadAccess::Physical, field.raw_type, field.unit_type,
            field.unit_bytes, parts, out.next);
        u32 left = read_stage_payload(out, payload, from_slots, shift_slots);
        u32 right = read_stage_payload(out, payload, to_slots, shift_slots);
        CollectivePair pair = CollectivePair(input.operation, field.scalar, field.raw_type, field.value_type,
            input.bool_type, left, right, out.next);
        CollectiveWords merged = collective_pair(pair, word_at(body, out.words), word_at(decorations, decorated));
        out.words += merged.body;
        out.next = merged.next;
        decorated += merged.decorations;
        u32 value = merged.value;
        if (first != 0) { value = stage_select(out, field.raw_type, first, left, value); }
        write_stage_payload(out, payload, value, to_slots, shift_slots);
    }
    return decorated;
}

StageBranch open_marked_scan_prefix(inout StageCursor out, PageAddress head, u32 group, u32 lane,
    u32 bool_type) on cpu {
    u32 u = head.u32_type;
    PageAddressResult pointer = stage_address(out, head, group, 0);
    u32 head_lane = stage_load(out, u, pointer.pointer, true);
    u32 lane_number = stage_op(out, 128, u, lane, head.one);
    u32 before = stage_op(out, 176, bool_type, lane_number, head_lane);
    u32 absent = stage_op(out, 170, bool_type, head_lane, head.zero);
    u32 apply = stage_op(out, 166, bool_type, before, absent);
    return stage_open(out, apply);
}

TreeWords add_scan_prefix(ScanPrefix input, address fields, u32 count,
    address body, address decorations) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    u32 uint_type = input.target.u32_type;
    u32 group = stage_op(out, 134, uint_type, input.target.index, input.width);
    let scalar = unsafe_position<PhysicalField>(fields).scalar;
    bool extreme = (scalar == BuiltinScalar::F32 || scalar == BuiltinScalar::F64) && input.operation != CollectiveOperation::Sum;
    bool segmented = input.head_descriptor != 0;
    u32 lane = segmented || extreme ? stage_op(out, 137, uint_type, input.target.index, input.width) : 0;
    u32 first = extreme ? stage_op(out, 170, input.bool_type, lane, input.target.zero) : 0;
    if (segmented) {
        PageAddress head = input.target;
        head.descriptor = input.head_descriptor;
        head.word = input.head_word;
        StageBranch branch = open_marked_scan_prefix(out, head, group, lane, input.bool_type);
        u64 decorated = write_scan_prefix(out, input, group, first, fields, count, decorations);
        stage_close(out, branch.end);
        return TreeWords(out.words, decorated, out.next, branch.end);
    }
    u64 decorated = write_scan_prefix(out, input, group, first, fields, count, decorations);
    return TreeWords(out.words, decorated, out.next, input.label);
}

void write_segment_tags(inout StageCursor out, u32 uint_type, u32 k, u32 counted,
    address left, address right, address values) on cpu {
    u32 left_head = read_word(left, k);
    u32 right_head = read_word(right, k);
    put(values, u64(k), stage_op(out, 197, uint_type, left_head, right_head));
    if (counted != 0) {
        u32 segments = stage_op(out, 128, uint_type, read_word(left, k + 1), read_word(right, k + 1));
        put(values, u64(k + 1), segments);
    }
}

usize collective_register(Span registers, usize index) on cpu {
    return unsafe_position<usize>(unsafe_address(address_bits(registers.data) + index * registers.stride));
}

void custom_pack_values(CustomCollectiveHost host, Span registers, address output, u32 count, MemoryHost memory) on cpu
    requires nonzero(host.layout.component) requires nonzero(host.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 zero = stream.constant(context, BuiltinScalar::U32, 0u64);
    u32 word_type = scalar_id(stream.types, BuiltinScalar::U32);
    for (word in 0..count) { put(output, u64(word), zero); }
    usize index = 0u64;
    while (index < registers.count && index < host.layout.count) {
        CollectiveComponent component = host.layout.component(host.layout.source, index);
        usize reg = collective_register(registers, index);
        BuiltinScalar scalar = host.scalar(context, reg);
        u32 raw = encode_raw_bits(scalar, block_register(host.registers, reg), memory);
        u32 width = scalar_definition(component.scalar).width;
        u32 raw_type = scalar_id(stream.types, unsigned_scalar(width));
        pack_record_field(PackedField(raw, component.offset, raw_type, width / 8), word_type, zero, output, memory);
        index += 1u64;
    }
}

void custom_read_arguments(CustomCollectiveHost host, Span registers, address values, MemoryHost memory) on cpu
    requires nonzero(host.layout.component) requires nonzero(host.scalar)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 word_type = scalar_id(stream.types, BuiltinScalar::U32);
    usize index = 0u64;
    while (index < registers.count && index < host.layout.count) {
        CollectiveComponent component = host.layout.component(host.layout.source, index);
        usize reg = collective_register(registers, index);
        BuiltinScalar scalar = host.scalar(context, reg);
        u32 bits = component.offset % 4 * 8;
        address operands = word_at(values, u64(component.offset / 4));
        u32 count = scalar_definition(component.scalar).width > 32 ? 2 : 1;
        u32 raw = 0;
        if (bits != 0) {
            u32 shift = stream.constant(context, BuiltinScalar::U32, u64(bits));
            raw = stream.ids(context, 1);
            address body = stream.reserve(context, 5u64, 0u64).body;
            memory.finish_body(context, words_four(body, 0u64, 194, word_type, raw, read_word(operands, 0), shift));
            count = 1;
            operands = address_of(raw);
        }
        emit_payload_load(PayloadShape(PayloadAccess::Words, scalar, BuiltinScalar::U32, count), operands,
            block_register(host.registers, reg), memory);
        index += 1u64;
    }
}

u32 emit_checked_combine(CustomCollectiveHost host, address left, address right, address result,
    u32 count, u32 label, MemoryHost memory) on cpu
    requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    usize reg = host.combine_first;
    while (reg < host.combine_end) {
        u8 defined = unsafe_position<u8>(unsafe_address(address_bits(host.definitions) + reg));
        if (defined == 0) { put(host.registers, reg, memory.scalars.ids(memory.scalars.context, 1)); }
        reg += 1u64;
    }
    custom_read_arguments(host, host.left, left, memory);
    custom_read_arguments(host, host.right, right, memory);
    u32 from = host.emit(host.context, host.combine_code, label);
    if (from == 0) { return 0; }
    custom_pack_values(host, host.result, result, count, memory);
    return from;
}

u32 emit_checked_projection(CustomCollectiveHost host, address values, u32 fields, u32 count, u32 label, MemoryHost memory) on cpu
    requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    usize index = 0u64;
    while (index < host.source.count && index < u64(fields)) {
        usize reg = collective_register(host.source, index);
        BuiltinScalar scalar = host.scalar(memory.scalars.context, reg);
        BuiltinScalar unit = unsigned_scalar(scalar_definition(scalar).width);
        emit_payload_load(PayloadShape(PayloadAccess::Words, scalar, unit, 1), word_at(values, index),
            block_register(host.registers, reg), memory);
        index += 1u64;
    }
    u32 from = host.emit(host.context, host.project_code, label);
    if (from == 0) { return 0; }
    custom_pack_values(host, host.projected, values, count, memory);
    return from;
}

struct CustomSegmentJoin {
    u32 uint_type;
    u32 bool_type;
    u32 zero;
    u32 skipped;
    u32 combined;
    u32 end;
    u32 value_words;
    u32 counted;
    u32 next;
};

CollectiveBranch open_custom_segment(u32 bool_type, u32 right_head, u32 zero,
    u32 next, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, next);
    u32 starts = stage_op(out, 171, bool_type, right_head, zero);
    StageBranch branch = stage_branch(out, starts, true);
    return CollectiveBranch(out.words, out.next, branch.run, branch.end);
}

StageWords close_custom_segment(CustomSegmentJoin input, address left, address right,
    address values, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    stage_close(out, input.end);
    for (part in 0..input.value_words) {
        u32 value = stage_phi(out, input.uint_type, read_word(right, part), input.skipped,
            read_word(values, part), input.combined);
        put(values, u64(part), value);
    }
    u32 k = input.value_words;
    if (input.counted != 0) {
        write_segment_tags(out, input.uint_type, k, input.counted, left, right, values);
    } else {
        u32 first = read_word(left, k);
        u32 has_head = stage_op(out, 171, input.bool_type, first, input.zero);
        put(values, u64(k), stage_select(out, input.uint_type, has_head, first, read_word(right, k)));
    }
    return StageWords(out.words, out.next, input.end);
}

u32 emit_custom_merge(CustomMerge input, address left, address right, address result,
    MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit) {
    u32 count = input.value_words;
    if (input.tag_words == 0) {
        return emit_checked_combine(host, left, right, result, count, input.label, memory);
    }
    address context = memory.scalars.context;
    u32 uint_type = scalar_id(memory.scalars.types, BuiltinScalar::U32);
    u32 bool_type = scalar_id(memory.scalars.types, BuiltinScalar::Bool);
    u32 zero = memory.scalars.constant(context, BuiltinScalar::U32, 0u64);
    u32 next = memory.scalars.ids(context, 0);
    address body = memory.scalars.reserve(context, 14u64, 0u64).body;
    CollectiveBranch opened = open_custom_segment(bool_type, read_word(right, count), zero, next, body);
    memory.finish_body(context, opened.words);
    memory.scalars.ids(context, opened.next - next);
    u32 combined = emit_checked_combine(host, left, right, result, count, opened.run, memory);
    if (combined == 0) { return 0; }

    next = memory.scalars.ids(context, 0);
    body = memory.scalars.reserve(context, 15u64 + 7u64 * u64(count), 0u64).body;
    CustomSegmentJoin join = CustomSegmentJoin(uint_type, bool_type, zero, input.label,
        combined, opened.end, count, u32(input.tag_words == 2), next);
    StageWords emitted = close_custom_segment(join, left, right, result, body);
    memory.finish_body(context, emitted.words);
    memory.scalars.ids(context, emitted.next - next);
    return emitted.label;
}

u64 merge_segment(inout StageCursor out, SegmentMerge input, address fields, u32 count,
    address left, address right, address values, address decorations) on cpu {
    address body = out.body;
    u32 k = input.value_words;
    u32 right_head = read_word(right, k);
    u32 starts = stage_op(out, 171, input.bool_type, right_head, input.zero);
    u64 decorated = 0u64;
    for (index in 0..count) {
        let field = unsafe_position<SegmentField>(unsafe_address(address_bits(fields) + u64(index) * size_of<SegmentField>()));
        array<u32,2> shifts = array<u32,2>(0, field.shift);
        address shift_slots = address_of(shifts[0]);
        u32 parts = field.shift == 0 ? 1 : 2;
        PayloadEmission payload = PayloadEmission(PayloadAccess::Words, field.raw_type, input.uint_type, 4, parts, out.next);
        u32 a = read_stage_payload(out, payload, word_at(left, u64(field.word)), shift_slots);
        u32 b = read_stage_payload(out, payload, word_at(right, u64(field.word)), shift_slots);
        CollectivePair pair = CollectivePair(input.operation, field.scalar, field.raw_type, field.value_type,
            input.bool_type, a, b, out.next);
        CollectiveWords merged = collective_pair(pair, word_at(body, out.words), word_at(decorations, decorated));
        out.words += merged.body;
        out.next = merged.next;
        decorated += merged.decorations;
        u32 tail = stage_select(out, field.raw_type, starts, b, merged.value);
        payload.next = out.next;
        PayloadWrite words = payload_write(payload, tail, address(), shift_slots,
            word_at(values, u64(field.word)), word_at(body, out.words));
        out.words += words.words;
        out.next = words.next;
    }
    write_segment_tags(out, input.uint_type, k, input.counted, left, right, values);
    return decorated;
}

struct SegmentField {
    BuiltinScalar scalar;
    u32 word; u32 raw_type; u32 value_type; u32 shift;
};
struct SegmentMerge {
    CollectiveOperation operation;
    u32 uint_type; u32 bool_type; u32 zero; u32 value_words; u32 counted;
};
struct SegmentAdd {
    SegmentMerge merge;
    PageAddress target;
    u32 prefix_descriptor; u32 width; u32 step; u32 active; u32 run; u32 end;
};
struct SegmentStorageField {
    u32 word; u32 offset; u32 unit_type; u32 unit_bytes; u32 pointer_type; u32 step;
};
enum SegmentPass : u32 { Main = 0; Tail = 1; Finish = 2; };
struct SegmentGroup {
    SegmentMerge merge;
    PageAddress source;
    SegmentPass pass;
    u32 target; u32 summary; u32 head_offset; u32 head_word; u32 marker_word;
    u32 count_target; u32 count_word; u32 active; u32 count; u32 lane;
    u32 shared; u32 pointer; u32 width; u32 last; u32 scope; u32 semantics;
    u32 step; u32 before;
};

void segment_plane_addresses(inout StageCursor out, SegmentGroup input, u32 lane, u32 words,
    address offsets, address pointers) on cpu {
    SharedAddress plane = SharedAddress(input.merge.uint_type, input.pointer, input.shared, lane, 0, words, out.next);
    SharedAddressWords emitted = write_shared_addresses(plane, offsets, pointers, word_at(out.body, out.words));
    out.words += emitted.words;
    out.next = emitted.next;
}

void read_segment_values(inout StageCursor out, u32 type_id, u32 words, address pointers, address values) on cpu {
    out.words += read_summary_words(word_at(out.body, out.words), type_id, out.next, pointers, words);
    for (part in 0..words) {
        put(values, u64(part), out.next + part);
    }
    out.next += words;
}

struct CustomTreeInput {
    u32 uint_type;
    u32 bool_type;
    u32 lane;
    u32 shared;
    u32 pointer;
    u32 mask;
    u32 stride;
    u32 remaining;
    u32 one;
    u32 down;
    u32 next;
};

struct CustomTreeOpen { u64 words; u32 next; u32 run; u32 absent; u32 merged; u32 end; };
struct CustomTreeClose {
    u32 uint_type;
    u32 from;
    u32 absent;
    u32 merged;
    u32 end;
    u32 scope;
    u32 semantics;
    u32 down;
    u32 next;
};

CustomTreeOpen open_custom_tree(CustomTreeInput input, address offsets, address own,
    u32 count, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    u32 u = input.uint_type;
    StageBranch layer = scan_tree_branch(out, u, input.bool_type, input.lane, input.mask);
    u32 other = stage_op(out, 130, u, input.lane, input.stride);
    u32 right_start = stage_op(out, 128, u, other, input.one);
    u32 valid = stage_op(out, 176, input.bool_type, right_start, input.remaining);
    SharedAddress plane = SharedAddress(u, input.pointer, input.shared, other, 0, count, out.next);
    SharedAddressWords located = write_shared_addresses(plane, offsets, workspace, word_at(body, out.words));
    out.words += located.words;
    out.next = located.next;

    address left = word_at(workspace, u64(count));
    address right = word_at(workspace, 2u64 * u64(count));
    read_segment_values(out, u, count, workspace, left);
    read_segment_values(out, u, count, own, right);
    if (input.down != 0) {
        out.words += write_summary_words(word_at(body, out.words), workspace, right, count);
    }
    StageBranch run = stage_open(out, valid);
    return CustomTreeOpen(out.words, out.next, run.run, layer.run, run.end, layer.end);
}


StageWords close_custom_tree(CustomTreeClose input, address own, address left,
    address values, u32 count, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    if (input.down != 0) {
        out.words += write_summary_words(word_at(body, out.words), own, values, count);
        stage_close(out, input.merged);
    } else {
        stage_close(out, input.merged);
        for (part in 0..count) {
            u32 value = stage_phi(out, input.uint_type, read_word(values, part), input.from,
                read_word(left, part), input.absent);
            put(values, u64(part), value);
        }
        out.words += write_summary_words(word_at(body, out.words), own, values, count);
    }
    stage_close(out, input.end);
    stage_barrier(out, input.scope, input.semantics);
    return StageWords(out.words, out.next, input.end);
}

void custom_plane_offsets(u32 words, address offsets, ScalarHost stream) on cpu
    requires nonzero(stream.constant) {
    for (part in 0..words) {
        u32 offset = part == 0 ? 0 : stream.constant(stream.context, BuiltinScalar::U32, 64u64 * u64(part));
        put(offsets, u64(part), offset);
    }
}

u32 emit_custom_tree(CustomTree tree, address own, address workspace,
    MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit) {
    address context = memory.scalars.context;
    u32 uint_type = scalar_id(memory.scalars.types, BuiltinScalar::U32);
    u32 bool_type = scalar_id(memory.scalars.types, BuiltinScalar::Bool);
    u32 one = memory.scalars.constant(context, BuiltinScalar::U32, 1u64);
    u32 scope = memory.scalars.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = memory.scalars.constant(context, BuiltinScalar::U32, 0x108u64);
    u32 count = tree.value_words + tree.tag_words;
    u64 span = u64(count);
    address neighbors = word_at(workspace, span);
    address left = word_at(neighbors, span);
    address right = word_at(left, span);
    address result = word_at(right, span);
    custom_plane_offsets(count, workspace, memory.scalars);

    bool down = tree.down != 0;
    address before = down ? right : left;
    address after = down ? left : right;
    u32 label = 0;
    for (depth in 0..6) {
        u32 width = down ? 32 >> depth : 1 << depth;
        u32 mask = memory.scalars.constant(context, BuiltinScalar::U32, u64(2 * width - 1));
        u32 stride = memory.scalars.constant(context, BuiltinScalar::U32, u64(width));
        u32 next = memory.scalars.ids(context, 0);
        address body = memory.scalars.reserve(context, 48u64 + 25u64 * span, 0u64).body;
        CustomTreeInput input = CustomTreeInput(uint_type, bool_type, tree.lane, tree.shared, tree.pointer,
            mask, stride, tree.remaining, one, tree.down, next);
        CustomTreeOpen opened = open_custom_tree(input, workspace, own, count, neighbors, body);
        memory.finish_body(context, opened.words);
        memory.scalars.ids(context, opened.next - next);

        u32 combined = emit_custom_merge(CustomMerge(tree.value_words, tree.tag_words, opened.run),
            before, after, result, memory, host);
        if (combined == 0) { return 0; }
        next = memory.scalars.ids(context, 0);
        body = memory.scalars.reserve(context, 12u64 + 12u64 * span, 0u64).body;
        CustomTreeClose close = CustomTreeClose(uint_type, combined, opened.absent, opened.merged,
            opened.end, scope, semantics, tree.down, next);
        StageWords emitted = close_custom_tree(close, own, left, result, count, body);
        memory.finish_body(context, emitted.words);
        memory.scalars.ids(context, emitted.next - next);
        label = emitted.label;
    }
    return label;
}


u64 segment_tree(inout StageCursor out, SegmentGroup input, u32 remaining, bool down,
    address levels, address fields, u32 count, address workspace, address decorations) on cpu {
    u32 words = input.merge.value_words + 1 + input.merge.counted;
    u64 span = u64(words);
    address a_ptr = word_at(workspace, span);
    address b_ptr = word_at(workspace, 2u64 * span);
    address a = word_at(workspace, 3u64 * span);
    address b = word_at(workspace, 4u64 * span);
    address merged = word_at(workspace, 5u64 * span);
    u32 type_id = input.merge.uint_type;
    u32 bool_type = input.merge.bool_type;
    u64 decorated = 0u64;
    for (index in 0..6) {
        u32 ordinal = down ? 5 - index : index;
        let level = unsafe_position<ScanLevel>(unsafe_address(address_bits(levels) + u64(ordinal) * size_of<ScanLevel>()));
        StageBranch branch = scan_tree_branch(out, type_id, bool_type, input.lane, level.mask);
        u32 other = stage_op(out, 130, type_id, input.lane, level.stride);
        segment_plane_addresses(out, input, other, words, workspace, a_ptr);
        segment_plane_addresses(out, input, input.lane, words, workspace, b_ptr);
        read_segment_values(out, type_id, words, a_ptr, a);
        read_segment_values(out, type_id, words, b_ptr, b);

        for (part in 0..words) {
            put(merged, u64(part), input.merge.zero);
        }
        decorated += merge_segment(out, input.merge, fields, count, down ? b : a, down ? a : b,
            merged, word_at(decorations, decorated));

        if (remaining != 0) {
            // Padding is not the right subtree: selecting it can change NaN or negative zero.
            u32 right_start = stage_op(out, 128, type_id, other, input.source.one);
            u32 has_right = stage_op(out, 176, bool_type, right_start, remaining);
            for (part in 0..words) {
                u32 value = stage_select(out, type_id, has_right, read_word(merged, part), read_word(a, part));
                put(merged, u64(part), value);
            }
        }
        if (down) {
            out.words += write_summary_words(word_at(out.body, out.words), a_ptr, b, words);
        }
        out.words += write_summary_words(word_at(out.body, out.words), b_ptr, merged, words);
        stage_close(out, branch.end);
        stage_barrier(out, input.scope, input.semantics);
    }
    return decorated;
}

PayloadWrite summary_addresses(PageAddress input, u32 step, address pointers, u32 count, address body) on cpu {
    PhysicalAddress start = physical_page_address(input, body, input.next);
    u64 at = start.words;
    u32 next = start.next;
    u32 physical = start.value;
    for (part in 0..count) {
        if (part != 0) {
            at = words_four(body, at, 128, input.u64_type, next, physical, step);
            physical = next;
            next += 1;
        }
        at = words_three(body, at, 120, input.result_pointer, next, physical);
        put(pointers, u64(part), next);
        next += 1;
    }
    return PayloadWrite(at, next);
}

void segment_summary_addresses(inout StageCursor out, PageAddress source, u32 step, u32 words,
    address pointers) on cpu {
    PageAddress request = source;
    request.next = out.next;
    PayloadWrite located = summary_addresses(request, step, pointers, words, word_at(out.body, out.words));
    out.words += located.words;
    out.next = located.next;
}

void read_segment_summary(inout StageCursor out, PageAddress source, u32 step, u32 words,
    address pointers, address values) on cpu {
    segment_summary_addresses(out, source, step, words, pointers);
    read_segment_values(out, source.u32_type, words, pointers, values);
}

CustomPrefixOpen open_custom_prefix(read CustomPrefix input, address fields,
    address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    StageBranch write = stage_open(out, input.active);
    u32 u = input.target.u32_type;
    u32 group = stage_op(out, 134, u, input.target.index, input.width);
    u32 label = write.run;
    u32 head_end = 0;
    if (input.head_descriptor != 0) {
        PageAddress head = input.target;
        head.descriptor = input.head_descriptor;
        head.word = input.head_word;
        u32 lane = stage_op(out, 137, u, input.target.index, input.width);
        StageBranch apply = open_marked_scan_prefix(out, head, group, lane, input.bool_type);
        label = apply.run;
        head_end = apply.end;
    }
    if (input.words != 0) {
        PageAddress parent = input.target;
        parent.descriptor = input.prefix_descriptor;
        parent.index = group;
        parent.word = parent.zero;
        u64 span = u64(input.words);
        address target = word_at(workspace, span);
        address locations = word_at(workspace, 2u64 * span);
        read_segment_summary(out, parent, input.step, input.words, workspace, workspace);
        if (input.packed != 0) {
            read_segment_summary(out, input.target, input.step, input.words, locations, target);
        } else if (input.field_count != 0) {
            read_collective_record(out, input.target, fields, input.field_count, target, locations);
        }
    }
    return CustomPrefixOpen(out.words, out.next, label, write.end, head_end);
}

StageWords close_custom_prefix(read CustomPrefix input, CustomPrefixOpen opened,
    address fields, address locations, address values, u32 next, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, next);
    if (input.packed != 0) {
        out.words += write_summary_words(body, locations, values, input.words);
    } else if (input.field_count != 0) {
        write_collective_record(out, input.target, fields, input.field_count, values, false, locations);
    }
    if (opened.head_end != 0) { stage_close(out, opened.head_end); }
    stage_close(out, opened.end);
    return StageWords(out.words, out.next, opened.end);
}

void segment_fields(CollectiveLayout layout, ScalarHost host, address output) on cpu
    requires nonzero(layout.component) requires nonzero(host.constant) {
    usize index = 0u64;
    while (index < layout.count) {
        CollectiveComponent component = layout.component(layout.source, index);
        PhysicalShape shape = physical_shape(component.scalar);
        u32 shift = high_word_shift(host, shape);
        address at = unsafe_address(address_bits(output) + index * size_of<SegmentField>());
        unsafe_position<SegmentField>(at) = SegmentField(component.scalar, component.offset / 4,
            scalar_id(host.types, shape.raw), scalar_id(host.types, component.scalar), shift);
        index += 1u64;
    }
}

void segment_storage_fields(CollectiveLayout layout, u32 first, MemoryHost host, address output) on cpu
    requires nonzero(layout.component) requires nonzero(host.scalars.constant)
    {
    ScalarHost stream = host.scalars;
    usize index = 0u64;
    while (index < layout.count) {
        CollectiveComponent component = layout.component(layout.source, index);
        PhysicalShape shape = physical_shape(component.scalar);
        u32 offset = field_byte_id(stream, first + component.offset);
        u32 step = high_word_step(stream, shape);
        address at = unsafe_address(address_bits(output) + index * size_of<SegmentStorageField>());
        unsafe_position<SegmentStorageField>(at) = SegmentStorageField(component.offset / 4, offset,
            scalar_id(stream.types, shape.unit), shape.bytes, scalar_id(host.scalars.physical_pointers, shape.unit), step);
        index += 1u64;
    }
}

void segment_identity(ScalarHost host, SegmentMerge merge, address fields, u32 count, address output) on cpu
    requires nonzero(host.constant) {
    for (word in 0..(merge.value_words + 1 + merge.counted)) {
        put(output, u64(word), merge.zero);
    }
    for (index in 0..count) {
        address at = unsafe_address(address_bits(fields) + u64(index) * size_of<SegmentField>());
        SegmentField component = unsafe_position<SegmentField>(at);
        ScalarDefinition type = scalar_definition(component.scalar);
        u64 bits = collective_identity(merge.operation, type.width, type.signed, u32(type.opcode == 22));
        u64 word = u64(component.word);
        put(output, word, host.constant(host.context, BuiltinScalar::U32, u64(u32(bits))));
        if (type.width == 64) {
            put(output, word + 1u64, host.constant(host.context, BuiltinScalar::U32, bits >> 32u64));
        }
    }
}

TreeWords segment_add(SegmentAdd input, address fields, u32 count,
    address workspace, address body, address decorations) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    StageBranch branch = StageBranch(input.run, input.end);
    out.words = open_conditional(body, out.words, branch.end, input.active, branch.run, branch.end);
    out.words = words_one(body, out.words, 248, branch.run);

    u32 words = input.merge.value_words + 1 + input.merge.counted;
    u64 span = u64(words);
    address target_ptrs = word_at(workspace, span);
    address prefix = word_at(workspace, 2u64 * span);
    address target = word_at(workspace, 3u64 * span);
    address merged = word_at(workspace, 4u64 * span);
    u32 type_id = input.merge.uint_type;
    u32 group = stage_op(out, 134, type_id, input.target.index, input.width);
    PageAddress parent = input.target;
    parent.descriptor = input.prefix_descriptor;
    parent.index = group;

    read_segment_summary(out, parent, input.step, words, workspace, prefix);
    read_segment_summary(out, input.target, input.step, words, target_ptrs, target);
    for (part in 0..words) {
        put(merged, u64(part), input.merge.zero);
    }
    u64 decorated = merge_segment(out, input.merge, fields, count, prefix, target, merged, decorations);

    let scalar = unsafe_position<SegmentField>(fields).scalar;
    bool extreme = (scalar == BuiltinScalar::F32 || scalar == BuiltinScalar::F64)
        && input.merge.operation != CollectiveOperation::Sum;
    if (input.merge.counted == 0 && extreme) {
        u32 lane = stage_op(out, 137, type_id, input.target.index, input.width);
        u32 first = stage_op(out, 170, input.merge.bool_type, lane, input.merge.zero);
        for (part in 0..words) {
            u32 value = stage_select(out, type_id, first, read_word(prefix, part), read_word(merged, part));
            put(merged, u64(part), value);
        }
    }

    out.words += write_summary_words(word_at(body, out.words), target_ptrs, merged, words);
    stage_close(out, branch.end);
    return TreeWords(out.words, decorated, out.next, branch.end);
}

u32 segment_indicator(inout StageCursor out, PageAddress source, u32 bool_type, u32 head, bool implicit_first) on cpu {
    u32 flag = stage_op(out, 171, bool_type, head, source.zero);
    if (implicit_first) {
        u32 first = stage_op(out, 170, bool_type, source.index, source.zero);
        flag = stage_op(out, 166, bool_type, flag, first);
    }
    return stage_select(out, source.u32_type, flag, source.one, source.zero);
}

struct CustomCollectiveSource {
    PageAddress source;
    u32 active;
    u32 lane;
    u32 bool_type;
    u32 value_words;
    u32 tag_words;
    u32 head_offset;
    u32 step;
    u32 tail;
};

CollectiveBranch custom_collective_read(CustomCollectiveSource input,
    address fields, u32 count, address values, address body) on cpu {
    PageAddress source = input.source;
    StageCursor out = StageCursor(body, 0u64, source.next);
    StageBranch read = stage_open(out, input.active);
    if (input.tail != 0) {
        u32 words = input.value_words + input.tag_words;
        if (words != 0) {
            read_segment_summary(out, source, input.step, words, values, values);
        }
    } else if (count + input.tag_words != 0) {
        u32 record = collective_record(out, source);
        for (index in 0..count) {
            let field = unsafe_position<PhysicalField>(
                unsafe_address(address_bits(fields) + u64(index) * size_of<PhysicalField>()));
            array<u32,2> pointers = stage_field_pointers(out, record, field.offset, field.pointer_type,
                source.u64_type, field.step);
            array<u32,2> shifts = array<u32,2>(0, field.shift);
            PayloadEmission payload = PayloadEmission(PayloadAccess::Physical, field.raw_type, field.unit_type,
                field.unit_bytes, field.step == 0 ? 1 : 2, out.next);
            u32 value = read_stage_payload(out, payload, address_of(pointers[0]), address_of(shifts[0]));
            put(values, u64(index), value);
        }
        if (input.tag_words != 0) {
            u32 tag_at = count > input.value_words ? count : input.value_words;
            array<u32,2> pointer = stage_field_pointers(out, record, input.head_offset, source.result_pointer,
                source.u64_type, 0);
            u32 head = stage_aligned_load(out, source.u32_type, pointer[0], 4);
            if (input.tag_words == 2) {
                u32 tag = segment_indicator(out, source, input.bool_type, head, true);
                put(values, u64(tag_at), tag);
                put(values, u64(tag_at + 1), tag);
            } else {
                u32 starts = stage_op(out, 171, input.bool_type, head, source.zero);
                u32 position = stage_op(out, 128, source.u32_type, input.lane, source.one);
                put(values, u64(tag_at), stage_select(out, source.u32_type, starts, position, source.zero));
            }
        }
    }
    return CollectiveBranch(out.words, out.next, read.run, read.end);
}


struct CustomCollectiveJoin {
    u32 uint_type;
    u32 from;
    u32 inactive;
    u32 end;
    u32 next;
    u32 scope;
    u32 semantics;
};

StageWords custom_collective_join(CustomCollectiveJoin input, address values,
    address identity, address shared, u32 count, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    out.words = words_one(body, out.words, 249, input.end);
    out.words = words_one(body, out.words, 248, input.end);
    for (part in 0..count) {
        u32 value = stage_phi(out, input.uint_type, read_word(values, part), input.from,
            read_word(identity, part), input.inactive);
        put(values, u64(part), value);
    }
    out.words += write_summary_words(word_at(body, out.words), shared, values, count);
    stage_barrier(out, input.scope, input.semantics);
    return StageWords(out.words, out.next, input.end);
}

u32 emit_custom_input(CustomInput input, address own, address identity,
    address values, address fields, MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body)
    requires nonzero(input.layout.component) requires nonzero(host.layout.component)
    requires nonzero(host.scalar) requires nonzero(host.emit) {
    u32 inactive = host.emit(host.context, host.identity_code, input.before);
    if (inactive == 0) { return 0; }
    custom_pack_values(host, host.initial, identity, input.value_words, memory);
    address context = memory.scalars.context;
    u32 zero = memory.scalars.constant(context, BuiltinScalar::U32, 0u64);
    u32 words = input.value_words + input.tag_words;
    for (part in input.value_words..words) { put(identity, u64(part), zero); }
    custom_plane_offsets(words, own, memory.scalars);
    u32 uint_type = scalar_id(memory.scalars.types, BuiltinScalar::U32);
    u32 next = memory.scalars.ids(context, 0);
    address body = memory.scalars.reserve(context, 5u64 + 10u64 * u64(words), 0u64).body;
    SharedAddress shared = SharedAddress(uint_type, input.pointer, input.shared, input.lane, 0, words, next);
    SharedAddressWords located = shared_addresses(shared, own, body);
    memory.finish_body(context, located.words);
    memory.scalars.ids(context, located.next - next);

    bool tail = input.tail != 0;
    u64 count = tail ? 0u64 : input.layout.count;
    if (!tail) { physical_fields(input.layout, input.source_word, memory, fields); }
    u32 head_offset = !tail && input.head_word != 0
        ? memory.scalars.constant(context, BuiltinScalar::U64, u64(input.head_word)) : 0;
    u32 step = tail && words > 1 ? memory.scalars.constant(context, BuiltinScalar::U64, 4u64) : 0;
    PageInputs raw = input.source;
    if (tail) { raw.descriptor = input.summary_descriptor; }
    PageAddress source = prepare_page(raw, scalar_id(memory.scalars.physical_pointers, BuiltinScalar::U32), memory.scalars);
    u32 bool_type = scalar_id(memory.scalars.types, BuiltinScalar::Bool);
    CustomCollectiveSource read = CustomCollectiveSource(source, input.active, input.lane, bool_type,
        input.value_words, input.tag_words, head_offset, step, input.tail);
    body = memory.scalars.reserve(context, 155u64 + 74u64 * count + 14u64 * u64(words), 0u64).body;
    CollectiveBranch opened = custom_collective_read(read, fields, u32(count), values, body);
    memory.finish_body(context, opened.words);
    memory.scalars.ids(context, opened.next - source.next);
    u32 from = opened.run;
    if (!tail) {
        from = emit_checked_projection(host, values, u32(count), input.value_words, from, memory);
        if (from == 0) { return 0; }
        if (count > u64(input.value_words)) {
            for (part in 0..input.tag_words) {
                put(values, u64(input.value_words + part), read_word(values, u32(count) + part));
            }
        }
    }

    u32 scope = memory.scalars.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = memory.scalars.constant(context, BuiltinScalar::U32, 0x108u64);
    next = memory.scalars.ids(context, 0);
    body = memory.scalars.reserve(context, 8u64 + 12u64 * u64(words), 0u64).body;
    CustomCollectiveJoin join = CustomCollectiveJoin(uint_type, from, inactive, opened.end, next, scope, semantics);
    StageWords emitted = custom_collective_join(join, values, identity, own, words, body);
    memory.finish_body(context, emitted.words);
    memory.scalars.ids(context, emitted.next - next);
    return emitted.label;
}


void segment_source(inout StageCursor out, SegmentGroup input, address fields, u32 count,
    address words) on cpu {
    address body = out.body;
    PhysicalAddress record = physical_page_address(input.source, word_at(body, out.words), out.next);
    out.words += record.words;
    out.next = record.next;
    for (part in 0..input.merge.value_words) {
        put(words, u64(part), input.merge.zero);
    }
    u32 uint_type = input.source.u32_type;
    u32 wide_type = input.source.u64_type;
    for (index in 0..count) {
        let field = unsafe_position<SegmentStorageField>(
            unsafe_address(address_bits(fields) + u64(index) * size_of<SegmentStorageField>()));
        array<u32,2> pointers = stage_field_pointers(out, record.value, field.offset, field.pointer_type,
            wide_type, field.step);
        u32 value = stage_aligned_load(out, field.unit_type, pointers[0], field.unit_bytes);
        if (field.unit_type != uint_type) {
            out.words = words_three(body, out.words, 113, uint_type, out.next, value);
            value = out.next;
            out.next += 1;
        }
        put(words, u64(field.word), value);
        if (field.step != 0) {
            u32 high = stage_aligned_load(out, uint_type, pointers[1], 4);
            put(words, u64(field.word + 1), high);
        }
    }

    array<u32,2> head_ptr = stage_field_pointers(out, record.value, input.head_offset, input.source.result_pointer, wide_type, 0);
    u32 head = stage_aligned_load(out, uint_type, head_ptr[0], 4);
    u32 indicator = segment_indicator(out, input.source, input.merge.bool_type, head, input.merge.counted != 0);
    put(words, u64(input.merge.value_words), indicator);
    if (input.merge.counted != 0) { put(words, u64(input.merge.value_words + 1), indicator); }
}

void write_segment_result(inout StageCursor out, PageAddress target, u32 index,
    address storage, u32 count, address values, u32 replace, address identity) on cpu {
    PageAddress row = target;
    row.index = index;
    PhysicalAddress record = physical_page_address(row, word_at(out.body, out.words), out.next);
    out.words += record.words;
    out.next = record.next;
    for (component in 0..count) {
        let field = unsafe_position<SegmentStorageField>(
            unsafe_address(address_bits(storage) + u64(component) * size_of<SegmentStorageField>()));
        array<u32,2> pointers = stage_field_pointers(out, record.value, field.offset, field.pointer_type,
            row.u64_type, field.step);
        u32 value = read_word(values, field.word);
        if (replace != 0) {
            value = stage_select(out, row.u32_type, replace, read_word(identity, field.word), value);
        }
        if (field.unit_type != row.u32_type) {
            out.words = words_three(out.body, out.words, 113, field.unit_type, out.next, value);
            value = out.next;
            out.next += 1;
        }
        out.words = aligned_store(out.body, out.words, pointers[0], value, field.unit_bytes);
        if (field.step != 0) {
            u32 high = read_word(values, field.word + 1);
            if (replace != 0) {
                high = stage_select(out, row.u32_type, replace, read_word(identity, field.word + 1), high);
            }
            out.words = aligned_store(out.body, out.words, pointers[1], high, 4);
        }
    }
}

PageAddress segment_page(PageAddress layout, u32 descriptor, u32 index, u32 word) on cpu {
    PageAddress page = layout;
    page.descriptor = descriptor;
    page.index = index;
    page.word = word;
    return page;
}

u32 segment_scan_write(inout StageCursor out, SegmentGroup input, u32 group, u32 is_last,
    address storage, u32 field_count, address exclusive, address item, address identity) on cpu {
    u32 u = input.merge.uint_type;
    u32 b = input.merge.bool_type;
    u32 zero = input.merge.zero;
    u32 current = read_word(item, input.merge.value_words);
    u32 is_head = stage_op(out, 171, b, current, zero);

    StageBranch write = stage_open(out, input.active);
    PageAddress target = segment_page(input.source, input.target, input.source.index, 0);
    write_segment_result(out, target, target.index, storage, field_count,
        exclusive, is_head, identity);
    stage_close(out, write.end);
    if (input.summary == 0) { return write.end; }

    u32 previous = read_word(exclusive, input.merge.value_words);
    u32 no_previous = stage_op(out, 170, b, previous, zero);
    u32 last_or_head = stage_op(out, 166, b, is_last, is_head);
    u32 owner = stage_op(out, 167, b, no_previous, last_or_head);
    StageBranch save = stage_open(out, owner);
    u32 lane = stage_op(out, 128, u, input.lane, input.source.one);
    u32 marker = stage_select(out, u, is_head, lane, zero);
    PageAddress summary = segment_page(input.source, input.summary, group, input.marker_word);
    PageAddressResult pointer = stage_address(out, summary, group, 0);
    out.words = aligned_store(out.body, out.words, pointer.pointer, marker, 4);
    stage_close(out, save.end);
    return save.end;
}

void write_segment_count(inout StageCursor out, PageAddress target, u32 count) on cpu {
    PageAddressResult pointer = stage_address(out, target, target.zero, 0);
    out.words = aligned_store(out.body, out.words, pointer.pointer, count, 4);
}

void publish_summary_root(inout StageCursor out, PageAddress target, u32 step, u32 words,
    address root, address pointers, address values) on cpu {
    if (words == 0) { return; }
    read_segment_values(out, target.u32_type, words, root, values);
    segment_summary_addresses(out, target, step, words, pointers);
    out.words += write_summary_words(word_at(out.body, out.words), pointers, values, words);
}

StageWords custom_scan_write(CustomScanWrite input, address own, address identity,
    u32 words, address fields, u32 field_count, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    u32 u = input.target.u32_type;
    StageBranch write = stage_open(out, input.active);
    if (input.tail != 0) {
        publish_summary_root(out, input.target, input.step, words, own,
            word_at(workspace, u64(words)), workspace);
    } else if (field_count != 0) {
        u32 value_words = input.head != 0 ? words - 1 : words;
        read_segment_values(out, u, value_words, own, workspace);
        if (input.head != 0) {
            u32 starts = stage_op(out, 171, input.bool_type, input.head, input.target.zero);
            for (part in 0..value_words) {
                u32 value = stage_select(out, u, starts, read_word(identity, part), read_word(workspace, part));
                put(workspace, u64(part), value);
            }
        }
        write_collective_record(out, input.target, fields, field_count, workspace, false, address());
    }
    stage_close(out, write.end);
    return StageWords(out.words, out.next, write.end);
}

void write_empty_segment_count(inout StageCursor out, PageAddress counter, u32 empty) on cpu {
    StageBranch zero = stage_open(out, empty);
    write_segment_count(out, counter, counter.zero);
    stage_close(out, zero.end);
}

void reset_summary_root(inout StageCursor out, u32 end, u32 words, u32 scope, u32 semantics,
    address root, address identity) on cpu {
    out.words += write_summary_words(word_at(out.body, out.words), root, identity, words);
    stage_close(out, end);
    stage_barrier(out, scope, semantics);
}

SummaryRootWords custom_summary_root(CustomSummaryRoot input, address identity,
    u32 words, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    u32 u = input.target.u32_type;
    array<u32,2> writers = stage_collective_writer(out, input.bool_type, input.lane,
        input.active, input.length, input.zero);
    u32 group = input.target.descriptor != 0 ? stage_op(out, 134, u, input.target.index, input.width) : 0;
    StageBranch save = stage_open(out, writers[0]);
    SharedAddress root = SharedAddress(u, input.pointer, input.shared, input.last, 0, words, out.next);
    SharedAddressWords emitted = write_shared_addresses(root, workspace, workspace, word_at(body, out.words));
    out.words += emitted.words;
    out.next = emitted.next;
    if (input.mode == SummaryRootMode::EmptyCountReset) {
        write_empty_segment_count(out, input.target, writers[1]);
    } else if (input.target.descriptor != 0) {
        PageAddress summary = input.target;
        summary.index = group;
        u64 span = u64(words);
        publish_summary_root(out, summary, input.step, words, workspace,
            word_at(workspace, span), word_at(workspace, 2u64 * span));
    }
    if (input.mode == SummaryRootMode::Publish) {
        stage_close(out, save.end);
    } else {
        reset_summary_root(out, save.end, words, input.scope, input.semantics, workspace, identity);
    }
    return SummaryRootWords(out.words, out.next, save.end, group);
}

u32 segment_prefix(inout StageCursor out, PageAddress parent, u32 bool_type, u32 from, u32 step,
    u32 words, address identity, address values) on cpu {
    if (parent.descriptor == 0) {
        for (part in 0..words) {
            put(values, u64(part), read_word(identity, part));
        }
        return from;
    }
    u32 first_group = stage_op(out, 170, bool_type, parent.index, parent.zero);
    StageBranch load = stage_branch(out, first_group, true);
    read_segment_summary(out, parent, step, words, values, values);
    stage_close(out, load.end);
    for (part in 0..words) {
        u32 value = stage_phi(out, parent.u32_type, read_word(values, part), load.run,
            read_word(identity, part), from);
        put(values, u64(part), value);
    }
    return load.end;
}

struct SegmentOutput { u32 last; u32 end; u32 index; };

SegmentOutput open_segment_output(inout StageCursor out, PageAddress source, u32 bool_type,
    u32 length, u32 from, u32 segments) on cpu {
    u32 u = source.u32_type;
    u32 next = stage_op(out, 128, u, source.index, source.one);
    u32 last = stage_op(out, 170, bool_type, next, length);
    StageBranch look = stage_branch(out, last, true);
    PageAddressResult pointer = stage_address(out, source, next, 0);
    u32 head = stage_aligned_load(out, u, pointer.pointer, 4);
    u32 starts = stage_op(out, 171, bool_type, head, source.zero);
    stage_close(out, look.end);
    u32 tail = stage_phi(out, bool_type, last, from, starts, look.run);

    StageBranch store = stage_open(out, tail);
    u32 index = stage_op(out, 130, u, segments, source.one);
    return SegmentOutput(last, store.end, index);
}

void close_segment_output(inout StageCursor out, SegmentOutput output, PageAddress counter, u32 segments) on cpu {
    stage_close(out, output.end);
    StageBranch counted = stage_open(out, output.last);
    write_segment_count(out, counter, segments);
    stage_close(out, counted.end);
}

CollectiveBranch open_custom_segment_finish(CustomSegmentPrefix input, address own,
    address identity, u32 words, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.parent.next);
    StageBranch write = stage_open(out, input.active);
    read_segment_values(out, input.parent.u32_type, words, own, workspace);
    address parent = word_at(workspace, u64(words));
    u32 run = segment_prefix(out, input.parent, input.bool_type, write.run, input.step, words, identity, parent);
    return CollectiveBranch(out.words, out.next, run, write.end);
}

StageWords close_custom_segment_finish(CustomSegmentOutput input, address fields,
    u32 field_count, address total, u32 segments, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.source.next);
    SegmentOutput output = open_segment_output(out, input.source, input.bool_type, input.length, input.from, segments);
    if (field_count != 0) {
        PageAddress row = segment_page(input.source, input.target, output.index, input.target_word);
        write_collective_record(out, row, fields, field_count, total, false, address());
    }
    PageAddress counter = segment_page(input.source, input.count_target, input.source.zero, input.count_word);
    close_segment_output(out, output, counter, segments);
    stage_close(out, input.end);
    return StageWords(out.words, out.next, input.end);
}

TreeWords segment_finish(inout StageCursor out, SegmentGroup input, u32 group, address fields,
    address storage, u32 field_count, address exclusive, address item, address identity,
    address workspace, address decorations) on cpu {
    StageBranch write = stage_open(out, input.active);
    u32 words = input.merge.value_words + 1 + input.merge.counted;
    address prefix = word_at(workspace, u64(words));
    address total = word_at(workspace, 2u64 * u64(words));
    u64 decorated = merge_segment(out, input.merge, fields, field_count, exclusive, item, workspace, decorations);

    PageAddress parent = segment_page(input.source, input.summary, group, 0);
    u32 joined = segment_prefix(out, parent, input.merge.bool_type, write.run, input.step, words, identity, prefix);
    decorated += merge_segment(out, input.merge, fields, field_count, prefix, workspace, total,
        word_at(decorations, decorated));

    u32 segments = read_word(total, input.merge.value_words + 1);
    PageAddress source = segment_page(input.source, input.source.descriptor, input.source.index, input.head_word);
    SegmentOutput output = open_segment_output(out, source, input.merge.bool_type, input.count, joined, segments);
    PageAddress row = segment_page(input.source, input.target, output.index, 0);
    write_segment_result(out, row, output.index, storage, field_count, total, 0, identity);
    PageAddress counter = segment_page(input.source, input.count_target, input.merge.zero, input.count_word);
    close_segment_output(out, output, counter, segments);
    stage_close(out, write.end);
    return TreeWords(out.words, decorated, out.next, write.end);
}

TreeWords segment_group(SegmentGroup input, address levels, address fields,
    address source_fields, address target_fields, u32 field_count, address identity,
    address workspace, address body, address decorations) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.source.next);
    u32 u = input.merge.uint_type;
    u32 b = input.merge.bool_type;
    u32 words = input.merge.value_words + 1 + input.merge.counted;
    u64 span = u64(words);
    address pointers = word_at(workspace, span);
    address values = word_at(workspace, 4u64 * span);
    address item = word_at(workspace, 6u64 * span);
    address own = word_at(workspace, 7u64 * span);
    bool tail = input.pass == SegmentPass::Tail;
    bool finish = input.pass == SegmentPass::Finish;
    bool counted = input.merge.counted != 0;
    bool down = !counted || tail || finish;

    StageBranch read = stage_open(out, input.active);
    if (tail) {
        read_segment_summary(out, input.source, input.step, words, pointers, item);
    } else {
        segment_source(out, input, source_fields, field_count, item);
    }
    stage_close(out, read.end);
    for (part in 0..words) {
        u32 value = stage_phi(out, u, read_word(item, part), read.run, read_word(identity, part), input.before);
        put(item, u64(part), value);
    }
    segment_plane_addresses(out, input, input.lane, words, workspace, own);
    out.words += write_summary_words(word_at(body, out.words), own, item, words);
    stage_barrier(out, input.scope, input.semantics);

    let scalar = unsafe_position<SegmentField>(fields).scalar;
    bool extreme = (scalar == BuiltinScalar::F32 || scalar == BuiltinScalar::F64)
        && input.merge.operation != CollectiveOperation::Sum;
    u32 remaining = counted || extreme
        ? stage_group_remaining(out, u, input.source.index, input.lane, input.count) : 0;
    u64 decorated = segment_tree(out, input, remaining, false, levels, fields, field_count, workspace, decorations);

    u32 group = finish || input.summary != 0 ? stage_op(out, 134, u, input.source.index, input.width) : 0;
    u32 is_last = 0;
    u32 owner = 0;
    u32 empty = 0;
    if (counted) {
        array<u32,2> writers = stage_collective_writer(out, b, input.lane, input.active, input.count, input.merge.zero);
        owner = writers[0];
        empty = writers[1];
    } else {
        is_last = stage_op(out, 170, b, input.lane, input.last);
        owner = is_last;
    }
    StageBranch save = stage_open(out, owner);
    address root = own;
    if (counted) {
        root = word_at(workspace, 2u64 * span);
        segment_plane_addresses(out, input, input.last, words, workspace, root);
    }
    if (finish) {
        PageAddress counter = segment_page(input.source, input.count_target, input.merge.zero, input.count_word);
        write_empty_segment_count(out, counter, empty);
    } else if (input.summary != 0) {
        u32 published = counted || tail ? words : input.merge.value_words;
        PageAddress target = segment_page(input.source, input.summary, group, 0);
        publish_summary_root(out, target, input.step, published, root, pointers, values);
    }
    if (!down) {
        stage_close(out, save.end);
        return TreeWords(out.words, decorated, out.next, save.end);
    }
    reset_summary_root(out, save.end, words, input.scope, input.semantics, root, identity);
    decorated += segment_tree(out, input, 0, true, levels, fields, field_count, workspace,
        word_at(decorations, decorated));
    read_segment_values(out, u, words, own, values);

    if (tail) {
        StageBranch write = stage_open(out, input.active);
        PageAddress target = segment_page(input.source, input.target, input.source.index, 0);
        segment_summary_addresses(out, target, input.step, words, pointers);
        out.words += write_summary_words(word_at(body, out.words), pointers, values, words);
        stage_close(out, write.end);
        return TreeWords(out.words, decorated, out.next, write.end);
    }
    if (finish) {
        TreeWords result = segment_finish(out, input, group, fields, target_fields, field_count, values,
            item, identity, workspace, word_at(decorations, decorated));
        result.decorations += decorated;
        return result;
    }
    u32 label = segment_scan_write(out, input, group, is_last, target_fields, field_count, values, item, identity);
    return TreeWords(out.words, decorated, out.next, label);
}


void scan_levels(ScalarHost host, address output) on cpu requires nonzero(host.constant) {
    for (index in 0..6) {
        u32 width = 1 << index;
        u32 mask = host.constant(host.context, BuiltinScalar::U32, u64(width * 2 - 1));
        u32 stride = host.constant(host.context, BuiltinScalar::U32, u64(width));
        address at = unsafe_address(address_bits(output) + u64(index) * size_of<ScanLevel>());
        unsafe_position<ScanLevel>(at) = ScanLevel(mask, stride);
    }
}

u32 segment_descriptor(SegmentBindings bindings, usize index) on cpu {
    return unsafe_position<u32>(word_at(bindings.descriptors, index));
}

SegmentMerge segment_merge(SegmentLayout layout, u32 value_words, ScalarHost host) on cpu
    requires nonzero(host.constant) {
    address context = host.context;
    u32 zero = host.constant(context, BuiltinScalar::U32, 0u64);
    return SegmentMerge(layout.operation, scalar_id(host.types, BuiltinScalar::U32),
        scalar_id(host.types, BuiltinScalar::Bool), zero, value_words, layout.counted);
}

void clear_workspace(address workspace, u64 words) on cpu {
    u64 index = 0u64;
    while (index < words) {
        put(workspace, index, 0);
        index += 1u64;
    }
}

TreeWords emit_segment_add(SegmentLayout layout, SegmentBindings bindings, SegmentDispatch dispatch,
    PageInputs raw_page, WordScratch scratch, MemoryHost host, u32 value_words, u64 field_words, u64 span) on cpu
    requires nonzero(layout.fields.component) requires nonzero(scratch.reserve)
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u64 count = layout.fields.count;
    address fields = scratch.reserve(scratch.context, field_words + 5u64 * span);
    address workspace = word_at(fields, field_words);
    segment_fields(layout.fields, stream, fields);
    clear_workspace(workspace, 5u64 * span);
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    u32 step = stream.constant(context, BuiltinScalar::U64, 4u64);
    SegmentMerge merge = segment_merge(layout, value_words, stream);
    PageInputs page = raw_page;
    page.descriptor = segment_descriptor(bindings, bindings.target);
    page.has_word = 1;
    PageAddress target = prepare_page(page, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), stream);
    SegmentAdd input = SegmentAdd(merge, target, segment_descriptor(bindings, bindings.prefix), width, step,
        dispatch.active, dispatch.run, dispatch.end);
    ScalarStreams output = stream.reserve(context, 243u64 + 41u64 * span + 74u64 * count, 3u64 * count);
    return segment_add(input, fields, u32(count), workspace, output.body, output.decorations);
}

extern "C" TreeWords emit_segmented(EntryPass pass, u32 single_group, SegmentLayout layout,
    SegmentBindings bindings, SegmentDispatch dispatch, PageInputs raw_page, WordScratch scratch, MemoryHost host) on cpu
    requires nonzero(layout.fields.component) requires nonzero(scratch.reserve)
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) {
    u32 value_words = layout.value_bytes / 4 + u32(layout.value_bytes % 4 != 0);
    u64 span = u64(value_words + 1 + layout.counted);
    u64 count = layout.fields.count;
    u64 field_words = count * size_of<SegmentField>() / 4u64;
    if (pass == EntryPass::ScanAdd) {
        return emit_segment_add(layout, bindings, dispatch, raw_page, scratch, host, value_words, field_words, span);
    }

    ScalarHost stream = host.scalars;
    address context = stream.context;
    bool tail = pass == EntryPass::ScanTail;
    bool finish = pass == EntryPass::ScanRootAdd;
    bool counted = layout.counted != 0;
    bool writes = finish || (!tail && !counted);
    u64 field_storage = count * size_of<SegmentStorageField>() / 4u64;
    u64 source_words = tail ? 0u64 : field_storage;
    u64 target_words = writes ? field_storage : 0u64;
    address fields = scratch.reserve(scratch.context,
        field_words + source_words + target_words + 9u64 * span + 6u64 * size_of<ScanLevel>() / 4u64);
    address source_fields = word_at(fields, field_words);
    address target_fields = word_at(source_fields, source_words);
    address identity = word_at(target_fields, target_words);
    address workspace = word_at(identity, span);
    address levels = word_at(workspace, 8u64 * span);

    segment_fields(layout.fields, stream, fields);
    SegmentMerge merge = segment_merge(layout, value_words, stream);
    segment_identity(stream, merge, fields, u32(count), identity);
    scan_levels(stream, levels);
    clear_workspace(workspace, 8u64 * span);
    for (index in 1..u32(span)) {
        put(workspace, u64(index), stream.constant(context, BuiltinScalar::U32, u64(index * 64)));
    }
    u32 head_offset = 0;
    if (!tail) {
        head_offset = field_byte_id(stream, layout.head_byte);
        segment_storage_fields(layout.fields, layout.source_byte, host, source_fields);
    }
    if (writes) {
        segment_storage_fields(layout.fields, layout.target_byte, host, target_fields);
    }
    u32 count_target = 0;
    u32 count_word = 0;
    u32 head_word = 0;
    if (finish) {
        count_target = segment_descriptor(bindings, bindings.counter);
        count_word = stream.constant(context, BuiltinScalar::U32, u64(layout.count_byte));
        head_word = stream.constant(context, BuiltinScalar::U32, u64(layout.head_byte));
    }
    u32 summary = single_group != 0 ? 0 : segment_descriptor(bindings, bindings.summary);
    u32 marker_word = writes && !counted && summary != 0
        ? stream.constant(context, BuiltinScalar::U32, u64(value_words * 4)) : 0;
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    u32 last = stream.constant(context, BuiltinScalar::U32, 63u64);
    u32 scope = stream.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = stream.constant(context, BuiltinScalar::U32, 0x108u64);
    u32 step = stream.constant(context, BuiltinScalar::U64, 4u64);
    PageInputs page = raw_page;
    page.descriptor = segment_descriptor(bindings, tail ? bindings.prefix : bindings.source);
    PageAddress source = prepare_page(page, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), stream);
    SegmentPass phase = tail ? SegmentPass::Tail : (finish ? SegmentPass::Finish : SegmentPass::Main);
    SegmentGroup input = SegmentGroup(merge, source, phase, segment_descriptor(bindings, bindings.target),
        summary, head_offset, head_word, marker_word, count_target, count_word, dispatch.active, dispatch.count,
        dispatch.lane, dispatch.shared, dispatch.pointer, width, last, scope, semantics, step, dispatch.before);
    u64 layers = counted && !tail && !finish ? 6u64 : 12u64;
    u64 final_merges = finish ? 2u64 : 0u64;
    ScalarStreams output = stream.reserve(context, layers * (57u64 + 48u64 * span + 74u64 * count)
        + 1050u64 + 80u64 * span + 214u64 * count, 3u64 * count * (layers + final_merges));
    return segment_group(input, levels, fields, source_fields, target_fields, u32(count), identity,
        workspace, output.body, output.decorations);
}

struct NumericWorkspace {
    array<u32,2> identity;
    address components; address fields; address values; address levels;
};

NumericWorkspace numeric_workspace(NumericGroupInput input, u64 level_words, WordScratch temporary,
    MemoryHost host) on cpu
    requires nonzero(input.fields.component) requires nonzero(temporary.reserve)
    requires nonzero(host.scalars.constant) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u64 count = input.fields.count;
    u64 tree_words = count * size_of<TreeComponent>() / 4u64;
    u64 field_words = count * size_of<CollectiveField>() / 4u64;
    address components = temporary.reserve(temporary.context, tree_words + field_words + 4u64 * count + level_words);
    address fields = word_at(components, tree_words);
    address values = word_at(fields, field_words);
    address levels = word_at(values, 4u64 * count);
    BuiltinScalar scalar = input.fields.component(input.fields.source, 0u64).scalar;
    ScalarDefinition type = scalar_definition(scalar);
    u64 identity = collective_identity(input.operation, type.width, type.signed, u32(type.opcode == 22));
    u32 low = stream.constant(context, BuiltinScalar::U32, u64(u32(identity)));
    u32 high = type.width == 64
        ? stream.constant(context, BuiltinScalar::U32, identity >> 32u64) : 0;
    CollectiveTree tree = CollectiveTree(input.dispatch.shared, input.dispatch.pointer, input.operation);
    collective_tree(input.fields, tree, stream, components);
    collective_fields(input.fields, input.tail == 0 ? input.source_word : 0, 1, host, fields);
    clear_workspace(values, 4u64 * count);
    return NumericWorkspace(array<u32,2>(low, high), components, fields, values, levels);
}

CollectiveInput numeric_lanes(NumericDispatch dispatch, array<u32,2> identity, u32 source, ScalarHost stream) on cpu
    requires nonzero(stream.constant) {
    u32 scope = stream.constant(stream.context, BuiltinScalar::U32, 2u64);
    u32 semantics = stream.constant(stream.context, BuiltinScalar::U32, 0x108u64);
    return CollectiveInput(identity, source, dispatch.lane, dispatch.active, dispatch.count,
        dispatch.before, scope, semantics);
}

u32 collective_descriptor(CollectiveBindings bindings, usize index) on cpu {
    return unsafe_position<u32>(word_at(bindings.descriptors, index));
}

extern "C" TreeWords emit_numeric_scan(NumericGroupInput input, CollectiveBindings bindings, NumericScanOptions options,
    PageInputs raw_page, WordScratch temporary, MemoryHost host) on cpu
    requires nonzero(input.fields.component) requires nonzero(temporary.reserve)
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u64 count = input.fields.count;
    NumericWorkspace work = numeric_workspace(input, 6u64 * size_of<ScanLevel>() / 4u64, temporary, host);
    scan_levels(stream, work.levels);
    u32 last = stream.constant(context, BuiltinScalar::U32, 63u64);
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    bool tail = input.tail != 0;
    CollectiveInput lanes = numeric_lanes(input.dispatch, work.identity,
        collective_descriptor(bindings, tail ? bindings.prefix : bindings.source), stream);
    PageInputs page = raw_page;
    page.descriptor = collective_descriptor(bindings, bindings.target);
    page.word = options.target_word;
    page.has_word = u32(!tail && options.root_operation != 0 && options.target_word != 0);
    PageAddress target = prepare_page(page, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), stream);
    ScanGroup group = ScanGroup(target, lanes, collective_descriptor(bindings, bindings.summary), width,
        last, scalar_id(stream.types, BuiltinScalar::Bool), u32(!tail && options.normalized_input != 0));
    ScalarStreams words = stream.reserve(context, 12u64 * (42u64 + 176u64 * count) + 430u64 + 128u64 * count,
        36u64 * count);
    return scan_group(group, work.levels, work.components, work.fields, u32(count), work.values, words.body, words.decorations);
}

ReductionWriteback numeric_writeback(NumericOutput output, PageInputs page, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    u32 length_word = stream.constant(context, BuiltinScalar::U32, u64(2 + u32(output.binding)));
    u32 bias = output.word == 0 ? 0 : stream.constant(context, BuiltinScalar::U32, u64(output.word));
    PageAddress target = prepare_page(page, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), stream);
    return ReductionWriteback(target, scalar_id(stream.types, BuiltinScalar::Bool), output.push_pointer,
        output.push, width, length_word, bias);
}

extern "C" TreeWords emit_numeric_reduction(NumericGroupInput input, u32 source, NumericOutput output,
    PageInputs raw_page, WordScratch temporary, MemoryHost host) on cpu
    requires nonzero(input.fields.component) requires nonzero(temporary.reserve)
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u64 count = input.fields.count;
    NumericWorkspace work = numeric_workspace(input, 6u64, temporary, host);
    for (index in 0..6) {
        put(work.levels, u64(index), stream.constant(context, BuiltinScalar::U32, u64(32 >> index)));
    }
    CollectiveInput lanes = numeric_lanes(input.dispatch, work.identity, source, stream);
    ReductionGroup group = ReductionGroup(numeric_writeback(output, raw_page, host), lanes);
    ScalarStreams words = stream.reserve(context, 6u64 * (42u64 + 176u64 * count) + 350u64 + 108u64 * count,
        18u64 * count);
    return reduction_group(group, work.levels, work.components, work.fields, u32(count), work.values,
        words.body, words.decorations);
}


extern "C" TreeWords emit_numeric_prefix(NumericPrefix input, CollectiveBindings bindings,
    PageInputs raw_page, WordScratch temporary, MemoryHost host) on cpu
    requires nonzero(input.fields.component) requires nonzero(temporary.reserve)
    requires nonzero(host.scalars.constant)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) {
    ScalarHost stream = host.scalars;
    address context = stream.context;
    u64 count = input.fields.count;
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    address fields = temporary.reserve(temporary.context, count * size_of<PhysicalField>() / 4u64);
    physical_fields(input.fields, 0, host, fields);
    u32 head_descriptor = 0;
    u32 head_word = 0;
    if (input.segmented != 0) {
        head_descriptor = collective_descriptor(bindings, bindings.summary);
        u32 value_words = input.value_bytes / 4 + u32(input.value_bytes % 4 != 0);
        head_word = stream.constant(context, BuiltinScalar::U32, u64(value_words * 4));
    }
    PageInputs page = raw_page;
    page.descriptor = collective_descriptor(bindings, bindings.target);
    page.word = input.target_word;
    page.has_word = u32(input.root != 0 && input.target_word != 0);
    PageAddress target = prepare_page(page, scalar_id(host.scalars.physical_pointers, BuiltinScalar::U32), stream);
    ScanPrefix prefix = ScanPrefix(target, collective_descriptor(bindings, bindings.prefix), head_descriptor,
        head_word, width, scalar_id(stream.types, BuiltinScalar::Bool), input.operation, input.label);
    ScalarStreams words = stream.reserve(context, 233u64 + 148u64 * u64(input.segmented) + 144u64 * count, 3u64 * count);
    return add_scan_prefix(prefix, fields, u32(count), words.body, words.decorations);
}


struct CustomWorkspace { address fields; address own; address identity; address item; address work; };

PageAddress custom_page(PageInputs raw, u32 descriptor, u32 index, u32 word, u32 has_word, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) {
    PageInputs page = raw;
    page.descriptor = descriptor;
    page.index = index;
    page.word = word;
    page.has_word = has_word;
    u32 pointer = scalar_id(memory.scalars.physical_pointers, BuiltinScalar::U32);
    return prepare_page(page, pointer, memory.scalars);
}

void custom_finish(MemoryHost memory, u64 words, u32 first, u32 next) on cpu
    requires nonzero(memory.scalars.ids) requires nonzero(memory.finish_body) {
    address context = memory.scalars.context;
    memory.finish_body(context, words);
    memory.scalars.ids(context, next - first);
}

u32 custom_group_remaining(NumericDispatch dispatch, u32 index, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.ids)
    requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 first = stream.ids(context, 0);
    address body = stream.reserve(context, 10u64, 0u64).body;
    group_remaining(body, scalar_id(stream.types, BuiltinScalar::U32), index, dispatch.lane, dispatch.count, first);
    custom_finish(memory, 10u64, first, first + 2);
    return first + 1;
}

SummaryRootWords emit_custom_root(PageInputs page, u32 descriptor, u32 word, SummaryRootMode mode,
    NumericDispatch dispatch, u32 words, CustomWorkspace slots, MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    custom_plane_offsets(words, slots.work, stream);
    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    u32 last = stream.constant(context, BuiltinScalar::U32, 63u64);
    u32 step = stream.constant(context, BuiltinScalar::U64, 4u64);
    u32 scope = stream.constant(context, BuiltinScalar::U32, 2u64);
    u32 semantics = stream.constant(context, BuiltinScalar::U32, 0x108u64);
    u32 zero = stream.constant(context, BuiltinScalar::U32, 0u64);
    PageAddress target = custom_page(page, descriptor, page.index, word, 1, memory);
    u32 bool_type = scalar_id(stream.types, BuiltinScalar::Bool);
    CustomSummaryRoot input = CustomSummaryRoot(target, mode, bool_type, zero, dispatch.lane,
        dispatch.active, dispatch.count, width, last, dispatch.shared, dispatch.pointer, step, scope, semantics);
    address body = stream.reserve(context, 169u64 + 36u64 * u64(words), 0u64).body;
    SummaryRootWords result = custom_summary_root(input, slots.identity, words, slots.work, body);
    custom_finish(memory, result.words, target.next, result.next);
    return result;
}

u32 emit_custom_prefix_stage(CustomCollective input, CollectiveBindings bindings, NumericDispatch dispatch,
    PageInputs page, u32 value_words, WordScratch temporary, MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(temporary.reserve)
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    bool root = input.pass == EntryPass::ScanRootAdd;
    u32 tags = root ? 0 : input.tags;
    u32 words = value_words + tags;
    u64 span = u64(words);
    u64 field_count = root ? host.layout.count : 0u64;
    u64 field_words = field_count * size_of<CollectiveField>() / 4u64;
    u64 location_words = root ? 2u64 * field_count : span;
    address fields = temporary.reserve(temporary.context, field_words + 3u64 * span + location_words);
    address prefix = word_at(fields, field_words);
    address target_words = word_at(prefix, span);
    address locations = word_at(target_words, span);
    address values = word_at(locations, location_words);

    u32 width = stream.constant(context, BuiltinScalar::U32, 64u64);
    u32 step = words > 1 ? stream.constant(context, BuiltinScalar::U64, 4u64) : 0;
    bool has_head = root && input.tags != 0;
    u32 head_descriptor = has_head ? collective_descriptor(bindings, bindings.summary) : 0;
    u32 head_word = has_head ? stream.constant(context, BuiltinScalar::U32, u64(value_words) * 4u64) : 0;
    if (root) { collective_fields(host.layout, 0, 0, memory, fields); }
    u32 target_word = root ? input.target_word : 0;
    PageAddress target = custom_page(page, collective_descriptor(bindings, bindings.target),
        page.index, target_word, u32(!root || target_word != 0), memory);
    u64 part = 0u64;
    while (part < 2u64 * span + location_words) {
        put(prefix, part, target.zero);
        part += 1u64;
    }

    CustomPrefix request = CustomPrefix(target, collective_descriptor(bindings, bindings.prefix),
        head_descriptor, head_word, width, scalar_id(stream.types, BuiltinScalar::Bool),
        dispatch.active, step, u32(!root), words, u32(field_count));
    address body = stream.reserve(context, 375u64 + 30u64 * span + 60u64 * field_count, 0u64).body;
    CustomPrefixOpen opened = open_custom_prefix(request, fields, prefix, body);
    custom_finish(memory, opened.words, target.next, opened.next);
    u32 label = emit_custom_merge(CustomMerge(value_words, tags, opened.label), prefix, target_words, values, memory, host);
    if (label == 0) { return 0; }
    u32 first = stream.ids(context, 0);
    body = stream.reserve(context, 8u64 + 5u64 * span + 44u64 * field_count, 0u64).body;
    StageWords emitted = close_custom_prefix(request, opened, fields, locations, values, first, body);
    custom_finish(memory, emitted.words, first, emitted.next);
    return emitted.label;
}

u32 emit_custom_finish_stage(CustomCollective input, CollectiveBindings bindings, NumericDispatch dispatch,
    PageInputs page, u32 group, u32 value_words, CustomWorkspace slots, MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids)
    requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(host.layout.component) requires nonzero(host.scalar) requires nonzero(host.emit) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u32 words = value_words + input.tags;
    u64 span = u64(words);
    u32 bool_type = scalar_id(stream.types, BuiltinScalar::Bool);
    u32 step = stream.constant(context, BuiltinScalar::U64, 4u64);
    u32 summary = input.single_group != 0 ? 0 : collective_descriptor(bindings, bindings.summary);
    PageAddress parent = custom_page(page, summary, group, 0, 1, memory);
    CustomSegmentPrefix prefix = CustomSegmentPrefix(parent, bool_type, dispatch.active, step);
    address body = stream.reserve(context, 136u64 + 28u64 * span, 0u64).body;
    CollectiveBranch opened = open_custom_segment_finish(prefix, slots.own, slots.identity, words, slots.work, body);
    custom_finish(memory, opened.words, parent.next, opened.next);
    address preceding = word_at(slots.work, span);
    address inclusive = word_at(preceding, span);
    address total = word_at(inclusive, span);
    u32 label = emit_custom_merge(CustomMerge(value_words, input.tags, opened.run), slots.work, slots.item, inclusive, memory, host);
    if (label == 0) { return 0; }
    label = emit_custom_merge(CustomMerge(value_words, input.tags, label), preceding, inclusive, total, memory, host);
    if (label == 0) { return 0; }

    collective_fields(host.layout, 0, 0, memory, slots.fields);
    u64 field_count = host.layout.count;
    u32 target_word = input.target_word == 0 ? 0 : stream.constant(context, BuiltinScalar::U32, u64(input.target_word));
    u32 counter_word = stream.constant(context, BuiltinScalar::U32, u64(input.counter_word));
    PageAddress source = custom_page(page, collective_descriptor(bindings, bindings.source), page.index, input.head_word, 1, memory);
    CustomSegmentOutput output = CustomSegmentOutput(source, bool_type, dispatch.count, label, opened.end,
        collective_descriptor(bindings, bindings.target), target_word,
        collective_descriptor(bindings, input.counter_binding), counter_word);
    body = stream.reserve(context, 420u64 + 44u64 * field_count, 0u64).body;
    StageWords emitted = close_custom_segment_finish(output, slots.fields, u32(field_count), total, read_word(total, words - 1), body);
    custom_finish(memory, emitted.words, source.next, emitted.next);
    return emitted.label;
}

u32 emit_custom_counted_tail(PageInputs page, u32 descriptor, u32 active, u32 words, CustomWorkspace slots,
    MemoryHost memory) on cpu
    requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) {
    ScalarHost stream = memory.scalars;
    address context = stream.context;
    u64 span = u64(words);
    u32 first = stream.ids(context, 0);
    address body = stream.reserve(context, 6u64 * span + 9u64, 0u64).body;
    StageCursor out = StageCursor(body, 0u64, first);
    read_segment_values(out, scalar_id(stream.types, BuiltinScalar::U32), words, slots.own, slots.work);
    StageBranch run = stage_open(out, active);
    custom_finish(memory, out.words, first, out.next);

    u32 step = words > 1 ? stream.constant(context, BuiltinScalar::U64, 4u64) : 0;
    PageAddress target = custom_page(page, descriptor, page.index, 0, 1, memory);
    body = stream.reserve(context, 113u64 + 14u64 * span, 0u64).body;
    out = StageCursor(body, 0u64, target.next);
    address pointers = word_at(slots.work, span);
    segment_summary_addresses(out, target, step, words, pointers);
    out.words += write_summary_words(word_at(body, out.words), pointers, slots.work, words);
    stage_close(out, run.end);
    custom_finish(memory, out.words, target.next, out.next);
    return run.end;
}

extern "C" u32 emit_custom_collective(CustomCollective input, CollectiveBindings bindings,
    NumericDispatch dispatch, PageInputs page, WordScratch temporary, MemoryHost memory, CustomCollectiveHost host) on cpu
    requires nonzero(input.source_layout.component) requires nonzero(host.layout.component)
    requires nonzero(temporary.reserve) requires nonzero(memory.scalars.constant)
    requires nonzero(memory.scalars.ids)
    requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(host.scalar) requires nonzero(host.emit) {
    u32 value_words = input.value_bytes / 4 + u32(input.value_bytes % 4 != 0);
    bool counted = input.step == BodyStep::CustomSegmented;
    if (input.step == BodyStep::CustomScanAdd || (counted && input.pass == EntryPass::ScanAdd)) {
        return emit_custom_prefix_stage(input, bindings, dispatch, page, value_words, temporary, memory, host);
    }

    ScalarHost stream = memory.scalars;
    address context = stream.context;
    bool tail = input.pass == EntryPass::ScanTail || input.pass == EntryPass::ReductionTail;
    bool reduce = input.step == BodyStep::CustomReduce;
    bool publish_only = counted && input.pass == EntryPass::Main;
    u32 words = value_words + input.tags;
    u64 span = u64(words);
    u64 source_count = tail ? 0u64 : input.source_layout.count;
    u64 source_words = source_count * size_of<PhysicalField>() / 4u64;
    u64 target_count = reduce || (!tail && !publish_only) ? host.layout.count : 0u64;
    u64 target_words = target_count * size_of<CollectiveField>() / 4u64;
    u64 field_words = source_words > target_words ? source_words : target_words;
    u64 item_words = (source_count > u64(value_words) ? source_count : u64(value_words)) + u64(input.tags);
    address fields = temporary.reserve(temporary.context, field_words + 7u64 * span + item_words);
    address own = word_at(fields, field_words);
    address identity = word_at(own, span);
    address item = word_at(identity, span);
    address work = word_at(item, item_words);
    clear_workspace(own, 7u64 * span + item_words);
    CustomWorkspace slots = CustomWorkspace(fields, own, identity, item, work);

    PageInputs source = page;
    source.descriptor = collective_descriptor(bindings, bindings.source);
    u32 summary = tail ? collective_descriptor(bindings, bindings.prefix) : 0;
    CustomInput read = CustomInput(source, input.source_layout, input.source_word, summary, dispatch.active,
        dispatch.lane, dispatch.shared, dispatch.pointer, value_words, input.tags, input.head_word, u32(tail), dispatch.before);
    u32 label = emit_custom_input(read, own, identity, item, fields, memory, host);
    if (label == 0) { return 0; }
    u32 remaining = custom_group_remaining(dispatch, page.index, memory);
    CustomTree tree = CustomTree(dispatch.lane, dispatch.shared, dispatch.pointer, remaining, value_words, input.tags, 0);
    label = emit_custom_tree(tree, own, work, memory, host);
    if (label == 0) { return 0; }
    u32 descriptor = collective_descriptor(bindings, bindings.target);
    if (reduce) {
        if (target_count == 0u64) { return label; }
        collective_fields(host.layout, 0, 0, memory, fields);
        u32 root = stream.constant(context, BuiltinScalar::U32, 63u64);
        PageInputs raw_target = page;
        raw_target.descriptor = descriptor;
        NumericOutput output = NumericOutput(bindings.target, input.target_word, input.push_pointer, input.push);
        ReductionWriteback write = numeric_writeback(output, raw_target, memory);
        address body = stream.reserve(context, 180u64 + 44u64 * target_count, 0u64).body;
        StageWords emitted = reduction_writeback(write, dispatch.lane, root, fields, u32(target_count), own, body);
        custom_finish(memory, emitted.words, write.target.next, emitted.next);
        return emitted.label;
    }

    bool counted_end = counted && !tail && !publish_only;
    usize binding = counted_end ? input.counter_binding : bindings.summary;
    u32 word = counted_end ? input.counter_word : 0;
    SummaryRootMode mode = publish_only ? SummaryRootMode::Publish
        : (counted_end ? SummaryRootMode::EmptyCountReset : SummaryRootMode::PublishReset);
    SummaryRootWords root = emit_custom_root(page, collective_descriptor(bindings, binding), word, mode, dispatch, words, slots, memory);
    if (publish_only) { return root.label; }
    tree.down = 1;
    label = emit_custom_tree(tree, own, work, memory, host);
    if (label == 0) { return 0; }
    if (counted) {
        if (tail) { return emit_custom_counted_tail(page, descriptor, dispatch.active, words, slots, memory); }
        return emit_custom_finish_stage(input, bindings, dispatch, page, root.group, value_words, slots, memory, host);
    }

    u32 step = tail && words > 1 ? stream.constant(context, BuiltinScalar::U64, 4u64) : 0;
    if (!tail) { collective_fields(host.layout, 0, 0, memory, fields); }
    u32 target_word = tail ? 0 : input.target_word;
    PageAddress target = custom_page(page, descriptor, page.index, target_word, u32(tail || target_word != 0), memory);
    u32 head = !tail && input.tags != 0 ? read_word(item, value_words) : 0;
    CustomScanWrite write = CustomScanWrite(target, dispatch.active, scalar_id(stream.types, BuiltinScalar::Bool), head, step, u32(tail));
    address body = stream.reserve(context, 128u64 + 20u64 * span + 44u64 * target_count, 0u64).body;
    StageWords emitted = custom_scan_write(write, own, identity, words, fields, u32(target_count), work, body);
    custom_finish(memory, emitted.words, target.next, emitted.next);
    return emitted.label;
}

const u32 ENTRY_MAP=0;
const u32 ENTRY_REDUCE=1;
const u32 ENTRY_SCAN=2;
const u32 ENTRY_SEGMENTED=3;
const u32 ENTRY_COMPACT=4;
const u32 ENTRY_SORT=5;
const u32 ENTRY_CUSTOM_SCAN=6;
const u32 ENTRY_CUSTOM_SEGMENTED=7;
const u32 ENTRY_CUSTOM_REDUCE=8;

struct StageEntries {
    u32 tail; u32 finish; u32 dispatch; u32 scan_add;
    u32 finish_kernel; u32 original_main; usize sort_passes;
};

StageEntries stage_entries(u32 operation,u32 selected,u32 prefix,u32 root_add,usize keys) on cpu {
 u32 original_main=u32(operation!=ENTRY_CUSTOM_REDUCE &&
     operation!=ENTRY_CUSTOM_SCAN && operation!=ENTRY_CUSTOM_SEGMENTED);
 StageEntries entries=StageEntries(0,0,selected!=0 ? 2 : prefix,selected,0,original_main,0u64);
 if(operation==ENTRY_MAP) {
  if(selected!=0) {
   entries.tail=2;
   entries.finish=1;
  }
 }else if(operation==ENTRY_REDUCE || operation==ENTRY_CUSTOM_REDUCE) {
  entries.tail=1;
 }else if(operation==ENTRY_SORT) {
  entries.sort_passes=19u64+keys;
 }else {
  entries.tail=2;
  entries.scan_add=1;
  if(operation==ENTRY_COMPACT) {
   entries.finish=2;
  }else if(operation==ENTRY_SEGMENTED || operation==ENTRY_CUSTOM_SEGMENTED) {
   entries.finish=3;
   if(operation==ENTRY_CUSTOM_SEGMENTED){entries.finish_kernel=1;}
  }else if(root_add!=0) {
   entries.finish=4;
   if(operation==ENTRY_CUSTOM_SCAN){entries.finish_kernel=2;}
  }
 }
 return entries;
}

const u32 ENTRY_SUMMARY = 1;
const u32 ENTRY_RANKS = 2;
const u32 ENTRY_SHARED_FACTS = 4;
const u32 SCAN_TERMINAL = 1;
const u32 SCAN_BOTH = 2;

bool argument_pass(EntryPass pass) on cpu {
    return pass == EntryPass::DispatchArguments || pass == EntryPass::SelectionArguments;
}

bool rank_pass(EntryPass pass) on cpu {
    return pass == EntryPass::SortHistogram || pass == EntryPass::SortScatter;
}

extern "C" ShaderPass shader_pass(EntryPass kind, usize parameter, u32 word) on cpu {
    bool numeric = kind == EntryPass::Main || kind == EntryPass::ReductionTail || kind == EntryPass::ScanTail
        || kind == EntryPass::ScanAdd || kind == EntryPass::ScanRootAdd || kind == EntryPass::SegmentedSingleGroup;
    return ShaderPass(kind, u32(numeric), parameter, word, u32(argument_pass(kind)));
}

bool send_stage_entry(StageEntrySink sink, EntryPass pass, EntryOutput output, EntryKernel kernel,
    u32 flags, usize parameter, u32 word) on cpu requires nonzero(sink.emit) {
    return sink.emit(sink.context, StageEntryRequest(shader_pass(pass, parameter, word), output, kernel, flags)) != 0;
}

StageEntryRequest sort_stage_entry(usize index, u32 root, u32 tail, u32 shared) on cpu {
    EntryPass pass = EntryPass::SortGather;
    u32 flags = shared | ENTRY_SUMMARY;
    usize parameter = 0u64;
    if (index < 8u64) {
        pass = EntryPass::SortHistogram;
        parameter = index * 4u64;
    } else if (index < 16u64) {
        pass = EntryPass::SortScatter;
        parameter = (index - 8u64) * 4u64;
    } else if (index == 16u64) {
        pass = EntryPass::SortScan;
        if (root == SCAN_TERMINAL) { flags = shared; }
    } else if (index == 17u64) {
        pass = EntryPass::ScanTail;
        if (tail == SCAN_TERMINAL) { flags = shared; }
    } else if (index == 18u64) {
        pass = EntryPass::ScanAdd;
    } else if (index >= 20u64) {
        pass = EntryPass::SortKey;
        parameter = index - 20u64;
    }
    if (rank_pass(pass)) { flags |= ENTRY_RANKS; }
    return StageEntryRequest(shader_pass(pass, parameter, 0), EntryOutput::Additional, EntryKernel::None, flags);
}

extern "C" void emit_stage_entries(StageEntryInput input, StageEntrySink sink) on cpu requires nonzero(sink.emit) {
    StageEntries entries = stage_entries(input.operation, input.selected, input.prefix, input.root_add, input.keys);
    bool custom = entries.original_main == 0;
    u32 shared = custom ? 0 : ENTRY_SHARED_FACTS;
    u32 summary = shared | ENTRY_SUMMARY;
    bool sorting = entries.sort_passes != 0u64;
    bool selected = entries.dispatch == 2;
    usize index = 0u64;
    while (index < entries.sort_passes) {
        StageEntryRequest request = sort_stage_entry(index, input.root_scan, input.tail_scan, shared);
        if (sink.emit(sink.context, request) == 0) { return; }
        index += 1u64;
    }

    u32 main_flags = selected || sorting || input.root_scan != SCAN_TERMINAL ? summary : shared;
    if (!send_stage_entry(sink, EntryPass::Main, EntryOutput::Main,
        custom ? EntryKernel::PrepareMain : EntryKernel::None, main_flags, 0u64, 0)) { return; }

    if (entries.tail != 0) {
        EntryPass pass = entries.tail == 1 ? EntryPass::ReductionTail : EntryPass::ScanTail;
        u32 flags = input.tail_scan != SCAN_TERMINAL ? summary : shared;
        if (!send_stage_entry(sink, pass, EntryOutput::Tail,
            custom ? EntryKernel::PrepareTail : EntryKernel::None, flags, 0u64, 0)) { return; }
    }

    if (entries.scan_add != 0) {
        EntryKernel kernel = EntryKernel::None;
        if (custom) { kernel = entries.finish_kernel == 2 ? EntryKernel::PrepareAdd : EntryKernel::Temporary; }
        if (!send_stage_entry(sink, EntryPass::ScanAdd, EntryOutput::ScanAdd, kernel, summary, 0u64, 0)) { return; }
    }

    if (entries.finish != 0) {
        EntryKernel kernel = EntryKernel::None;
        EntryPass pass = EntryPass::ScanRootAdd;
        EntryOutput output = EntryOutput::RootAdd;
        if (entries.finish_kernel == 1) { kernel = EntryKernel::Main; }
        if (entries.finish_kernel == 2) { kernel = EntryKernel::Add; }
        if (entries.finish == 1) {
            pass = EntryPass::SelectionScatter;
            output = EntryOutput::Scatter;
        } else if (entries.finish == 2) {
            pass = EntryPass::CompactScatter;
            output = EntryOutput::Scatter;
        } else if (entries.finish == 3) {
            output = EntryOutput::SegmentedFinish;
            if (input.single_group != 0 && !send_stage_entry(sink, EntryPass::SegmentedSingleGroup,
                EntryOutput::Additional, kernel, summary, 0u64, 0)) { return; }
        }
        if (!send_stage_entry(sink, pass, output, kernel, summary, 0u64, 0)) { return; }
    }

    if (input.root_scan == SCAN_BOTH) {
        EntryPass pass = sorting ? EntryPass::SortScan : (selected ? EntryPass::SelectionScan : EntryPass::Main);
        if (!send_stage_entry(sink, pass, EntryOutput::Additional,
            custom ? EntryKernel::Main : EntryKernel::None, shared, 0u64, 0)) { return; }
    }
    if (input.tail_scan == SCAN_BOTH && !send_stage_entry(sink, EntryPass::ScanTail, EntryOutput::Additional,
        custom ? EntryKernel::Tail : EntryKernel::None, shared, 0u64, 0)) { return; }

    if (selected) {
        if (!send_stage_entry(sink, EntryPass::SelectionArguments, EntryOutput::Dispatch,
            EntryKernel::None, summary, 0u64, 0)) { return; }
        u32 flags = input.root_scan != SCAN_TERMINAL ? summary : shared;
        send_stage_entry(sink, EntryPass::SelectionScan, EntryOutput::Selector, EntryKernel::None, flags, 0u64, 0);
    } else if (entries.dispatch == 1) {
        send_stage_entry(sink, EntryPass::DispatchArguments, EntryOutput::Dispatch,
            EntryKernel::None, summary, input.prefix_binding, input.prefix_word);
    }
}

BodyStepResult body_step(ShaderBodySink sink, BodyStep action, u32 flag) on cpu
    requires nonzero(sink.emit) {
    return sink.emit(sink.context, action, flag);
}

extern "C" CollectiveComponent count_component(address source, usize index) on cpu {
    return CollectiveComponent(BuiltinScalar::U32, 0);
}

ScanParameters count_scan(usize source, usize target, u32 source_word) on cpu {
    CollectiveLayout layout = CollectiveLayout(address(), 1u64, c_function(count_component));
    return ScanParameters(layout, source, target, source_word, 0, CollectiveOperation::Sum, 4);
}

void body_scan(ShaderBodyInput input, ShaderBodySink sink, ScanParameters parameters, u32 local, u32 tail, u32 root) on cpu
    requires nonzero(sink.emit) requires nonzero(sink.scan) {
    if (local != 0) {
        body_step(sink, BodyStep::Local, 0);
        u32 ordinary = u32(input.operation == ENTRY_SCAN);
        u32 normalized = u32(input.operation == ENTRY_COMPACT || input.selection != 0);
        sink.scan(sink.context, parameters, 1, tail, ordinary, normalized);
    } else {
        body_step(sink, BodyStep::Open, 0);
        sink.scan(sink.context, parameters, 0, root, 0, 0);
        body_step(sink, BodyStep::Merge, 1);
    }
}

extern "C" ShaderBodyEnd emit_shader_body(ShaderBodyInput input, ShaderBodySink sink) on cpu
    requires nonzero(input.ir.inspect) requires nonzero(sink.emit) requires nonzero(sink.scan) {
    EntryPass pass = input.pass;
    u32 operation = input.operation;
    if (pass == EntryPass::DispatchArguments || pass == EntryPass::SelectionArguments) {
        BodyStep step = pass == EntryPass::DispatchArguments ? BodyStep::DispatchArguments : BodyStep::SelectionArguments;
        BodyStepResult result = body_step(sink, step, 0);
        return result == BodyStepResult::Yes ? ShaderBodyEnd::Prepared : ShaderBodyEnd::Plain;
    }
    if (input.prefix_index != 0) {
        bool linear = operation != ENTRY_MAP || (input.selection != 0 && pass != EntryPass::Main);
        body_step(sink, BodyStep::PrefixIndex, u32(linear));
    } else {
        body_step(sink, BodyStep::LinearIndex, 0);
    }
    if (input.invariants != 0 && body_step(sink, BodyStep::Invariants, 0) == BodyStepResult::Error) {
        return ShaderBodyEnd::Plain;
    }
    u32 tail = u32(pass == EntryPass::ScanTail);
    u32 root = u32(pass == EntryPass::ScanRootAdd);
    if (input.selection != 0 && pass != EntryPass::Main) {
        if (pass == EntryPass::SelectionScatter) {
            body_step(sink, BodyStep::SelectionScatter, 0);
        } else {
            ScanParameters parameters = count_scan(input.selection_source, input.selection_target, input.selection_word);
            body_scan(input, sink, parameters, input.scan_local, tail, 0);
        }
        return ShaderBodyEnd::Plain;
    }
    if (operation == ENTRY_SORT) {
        if (pass == EntryPass::Main || pass == EntryPass::SortKey) {
            body_step(sink, BodyStep::SortKey, u32(pass == EntryPass::Main));
        } else if (pass == EntryPass::SortGather) {
            body_step(sink, BodyStep::SortGather, 0);
        } else if (pass == EntryPass::SortHistogram || pass == EntryPass::SortScatter) {
            body_step(sink, BodyStep::SortRank, u32(pass == EntryPass::SortHistogram));
        } else if (pass == EntryPass::SortScan || pass == EntryPass::ScanTail || pass == EntryPass::ScanAdd) {
            View source = input.ir.inspect(input.source_operation);
            u32 local = u32(pass != EntryPass::ScanAdd);
            body_scan(input, sink, count_scan(source.a, source.b, 0), local, local, 0);
        } else {
            return ShaderBodyEnd::BadSortPass;
        }
        return ShaderBodyEnd::Plain;
    }
    if (operation == ENTRY_CUSTOM_SEGMENTED) {
        if (input.scan_local != 0) { body_step(sink, BodyStep::Local, 0); }
        body_step(sink, BodyStep::CustomSegmented, 0);
    } else if (operation == ENTRY_CUSTOM_SCAN) {
        if (input.scan_local != 0) {
            body_step(sink, BodyStep::Local, 0);
            body_step(sink, BodyStep::CustomScan, tail);
        } else {
            body_step(sink, BodyStep::CustomScanAdd, root);
        }
    } else if (operation == ENTRY_CUSTOM_REDUCE || operation == ENTRY_REDUCE) {
        body_step(sink, BodyStep::Local, 0);
        BodyStep step = operation == ENTRY_CUSTOM_REDUCE ? BodyStep::CustomReduce : BodyStep::Reduce;
        body_step(sink, step, u32(pass == EntryPass::ReductionTail));
    } else if (operation == ENTRY_SEGMENTED) {
        if (pass == EntryPass::ScanAdd) {
            body_step(sink, BodyStep::SegmentedReduceAdd, 0);
        } else {
            if (input.scan_local != 0) { body_step(sink, BodyStep::Local, 0); }
            body_step(sink, BodyStep::SegmentedReduce, 0);
        }
    } else if (operation == ENTRY_COMPACT) {
        if (pass == EntryPass::CompactScatter) {
            body_step(sink, BodyStep::CompactScatter, 0);
        } else {
            View source = input.ir.inspect(input.source_operation);
            body_scan(input, sink, count_scan(source.c, source.b, u32(source.d)), input.scan_local, tail, 0);
        }
    } else if (operation == ENTRY_SCAN) {
        if (input.segmented_scan == 0) {
            ScanParameters parameters = unsafe_position<ScanParameters>(input.ordinary_scan);
            body_scan(input, sink, parameters, input.scan_local, tail, root);
        } else if (input.scan_local != 0) {
            body_step(sink, BodyStep::Local, 0);
            body_step(sink, BodyStep::SegmentedScan, tail);
        } else if (pass == EntryPass::ScanAdd) {
            body_step(sink, BodyStep::SegmentedAdd, 0);
        } else if (pass == EntryPass::ScanRootAdd) {
            body_step(sink, BodyStep::Open, 0);
            body_step(sink, BodyStep::SegmentedRoot, 0);
            body_step(sink, BodyStep::Merge, 1);
        } else {
            return ShaderBodyEnd::BadSegmentedPass;
        }
    } else {
        if (input.tile_scope != 0) { body_step(sink, BodyStep::TileInput, 0); }
        if (body_step(sink, BodyStep::TilePhases, 0) != BodyStepResult::No) {
            return ShaderBodyEnd::Plain;
        }
        body_step(sink, BodyStep::Open, 0);
        if (input.selection != 0) { body_step(sink, BodyStep::SelectedIndex, 0); }
        BodyStepResult flow = body_step(sink, BodyStep::Map, 0);
        if (flow != BodyStepResult::Error) {
            body_step(sink, BodyStep::Merge, u32(flow == BodyStepResult::Yes));
        }
    }
    return ShaderBodyEnd::Plain;
}

extern "C" WorkgroupLayout workgroup_layout(
    u32 operation, EntryPass pass, u32 selected, u32 summary_words, u32 scan_width
) on cpu {
    bool main = pass == EntryPass::Main;
    bool tail = pass == EntryPass::ScanTail;
    bool scan = ((operation == ENTRY_SCAN || operation == ENTRY_CUSTOM_SCAN || operation == ENTRY_COMPACT) && (main || tail))
        || (selected != 0 && (pass == EntryPass::SelectionScan || tail))
        || (operation == ENTRY_SORT && (pass == EntryPass::SortScan || tail))
        || ((operation == ENTRY_SEGMENTED || operation == ENTRY_CUSTOM_SEGMENTED)
            && (main || tail || pass == EntryPass::ScanRootAdd || pass == EntryPass::SegmentedSingleGroup));
    bool local_index = scan || ((operation == ENTRY_REDUCE || operation == ENTRY_CUSTOM_REDUCE)
        && (main || pass == EntryPass::ReductionTail));
    bool ranks = rank_pass(pass);
    u32 words = ranks ? 32 : (local_index ? scan_width * summary_words : 0);
    return WorkgroupLayout(u32(scan), u32(local_index), u32(ranks), words);
}

u32 define_scalar_input(inout ResourceCursor out, u32 type_id, u32 builtin) on cpu {
    u32 pointer = define_pointer(out, StorageClass::Input, type_id);
    return define_annotated(out, VariableAnnotation::BuiltIn, pointer, builtin);
}

SharedArray define_shared(inout ResourceCursor out, ShaderConstants constants, u32 word_type, u32 element, u32 words)
    on cpu requires nonzero(constants.intern) {
    u32 length = define_word(out, constants, word_type, words);
    u32 array = define_type(out, TypeShape::Array, element, length);
    u32 array_pointer = define_pointer(out, StorageClass::Workgroup, array);
    u32 variable = definition_id(out);
    variable_declaration(StorageClass::Workgroup, array_pointer, variable, word_at(out.definitions, out.words));
    out.words += 4u64;
    u32 pointer = define_pointer(out, StorageClass::Workgroup, element);
    return SharedArray(variable, pointer);
}

extern "C" u32 workgroup_declarations(WorkgroupDeclaration input, ShaderConstants constants, address definitions,
    address decorations, address output) on cpu requires nonzero(constants.intern) {
    u64 tile_words = input.narrow_cells * u64(input.width);
    u64 narrow_words = tile_words + input.scratch_words;
    u64 wide_words = input.wide_cells * u64(input.width);
    if (narrow_words > 4294967295u64 || wide_words > 4294967295u64) { return 0; }

    ResourceCursor out = ResourceCursor(definitions, decorations, 0u64, 0u64, input.next);
    u32 local = input.layout.local_index != 0 ? define_scalar_input(out, input.word_type, 29) : 0;
    u32 rank_id = 0;
    u32 rank_lane = 0;
    if (input.layout.ranks != 0) {
        u32 pointer = define_pointer(out, StorageClass::Input, input.word_type);
        rank_id = define_annotated(out, VariableAnnotation::BuiltIn, pointer, 40);
        rank_lane = define_annotated(out, VariableAnnotation::BuiltIn, pointer, 41);
    }
    u32 ballot = input.ballot != 0 ? define_type(out, TypeShape::Vector, input.word_type, 4) : 0;
    u32 lane = (input.subgroup_mask & 1) != 0 ? define_scalar_input(out, input.word_type, 41) : 0;
    u32 size = (input.subgroup_mask & 2) != 0 ? define_scalar_input(out, input.word_type, 36) : 0;
    u32 id = (input.subgroup_mask & 4) != 0 ? define_scalar_input(out, input.word_type, 40) : 0;

    SharedArray reduction = SharedArray(0, 0);
    u64 bytes = narrow_words * 4u64 + wide_words * 8u64;
    if (input.layout.local_index != 0 || input.layout.ranks != 0) {
        reduction = define_shared(out, constants, input.word_type, input.word_type, input.layout.words);
        bytes += u64(input.layout.words) * 4u64;
    }
    SharedArray tile = SharedArray(0, 0);
    if (narrow_words != 0u64) {
        tile = define_shared(out, constants, input.word_type, input.word_type, u32(narrow_words));
    }
    SharedArray tile_wide = SharedArray(0, 0);
    if (wide_words != 0u64) {
        tile_wide = define_shared(out, constants, input.word_type, input.wide_type, u32(wide_words));
    }
    unsafe_position<WorkgroupVariables>(output) = WorkgroupVariables(out.words, out.annotations, bytes, out.next,
        local, rank_id, rank_lane, ballot, lane, size, id, u32(tile_words), reduction, tile, tile_wide);
    return 1;
}

extern "C" KernelPreparation custom_kernel_preparation(EntryPass pass, u32 segmented) on cpu {
    if (argument_pass(pass)) { return KernelPreparation::Original; }
    bool root = pass == EntryPass::ScanRootAdd || pass == EntryPass::SegmentedSingleGroup;
    if (pass == EntryPass::Main || (root && segmented != 0)) { return KernelPreparation::ProjectIdentity; }
    if (pass == EntryPass::ScanAdd || root) { return KernelPreparation::Combine; }
    return KernelPreparation::Identity;
}
