module bootstrap.spirv_envelope;

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

extern "C" u64 scalar_declaration(BuiltinScalar scalar, u32 type_id, u32 pointer_id, address words) on cpu {
    ScalarDefinition type = scalar_definition(scalar);
    u64 at = 2u64;
    if (type.opcode != 20) { put(words, at, type.width); at += 1u64; }
    if (type.opcode == 21) { put(words, at, type.signed); at += 1u64; }
    put(words, 0u64, instruction(type.opcode, u32(at)));
    put(words, 1u64, type_id);
    return words_three(words, at, 32, pointer_id, u32(StorageClass::Function), type_id);
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

extern "C" u64 resource_block(ResourceBlock input, address definitions, address decorations) on cpu {
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

extern "C" void variable_declaration(StorageClass storage, u32 pointer, u32 id, address words) on cpu {
    words_three(words, 0u64, 59, pointer, id, u32(storage));
}

extern "C" u64 annotated_variable(VariableAnnotation annotation, u32 pointer, u32 id, u32 operand, address definitions, address decorations) on cpu {
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

extern "C" BinaryWords scalar_binary(BinaryEmission input, address body, address decorations) on cpu {
    ScalarDefinition scalar = scalar_definition(input.scalar);
    u32 opcode = binary_opcode(input.operation, scalar);
    u64 at = 0u64;
    u32 right = input.right;
    if (input.masked != 0) {
        at = words_four(body, at, 199, input.result_type, input.masked, right, input.mask);
        right = input.masked;
    }
    at = words_four(body, at, opcode, input.result_type, input.result, input.left, right);
    if (scalar.opcode == 22 && u32(input.operation) <= u32(BinaryOperation::Divide)) {
        return BinaryWords(at, flag_decoration(decorations, 0u64, FlagDecoration::NoContraction, input.result));
    }
    return BinaryWords(at, 0u64);
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

extern "C" u64 scalar_cast(CastEmission input, address body) on cpu {
    if (input.from == BuiltinScalar::Bool) {
        return select_words(body, 0u64, input.result_type, input.result, input.value, input.one, input.zero);
    }
    ScalarDefinition source = scalar_definition(input.from);
    if (input.to == BuiltinScalar::Bool) {
        return words_four(body, 0u64, source.opcode == 22 ? 183 : 171,
            input.result_type, input.result, input.value, input.zero);
    }
    ScalarDefinition target = scalar_definition(input.to);
    return words_three(body, 0u64, cast_opcode(input.from, input.to, source, target),
        input.result_type, input.result, input.value);
}

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

extern "C" DivisionWords integer_division(DivisionEmission input, address body) on cpu {
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

u64 float_sign(address body, u32 opcode, u32 type, u32 result, u32 value, FloatSignOperands input) on cpu {
    u32 bits = input.temporary;
    u32 changed = bits + 1;
    u64 at = words_three(body, 0u64, 124, input.raw_type, bits, value);
    at = words_four(body, at, opcode, input.raw_type, changed, bits, input.mask);
    return words_three(body, at, 124, type, result, changed);
}

extern "C" u64 scalar_unary(UnaryEmission input, address body) on cpu {
    if (input.operation == UnaryOperation::FloatNegate) {
        return float_sign(body, 198, input.result_type, input.result, input.value, input.sign);
    }
    if (input.operation == UnaryOperation::UnsignedNegate) {
        return words_four(body, 0u64, 130, input.result_type, input.result, input.zero, input.value);
    }
    u32 opcode = input.operation == UnaryOperation::SignedNegate ? 126 :
        input.operation == UnaryOperation::LogicalNot ? 168 :
        input.operation == UnaryOperation::BitNot ? 200 : 124;
    return words_three(body, 0u64, opcode, input.result_type, input.result, input.value);
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
extern "C" u64 scalar_math(MathEmission input, address body) on cpu {
    if (input.operation == MathOperation::Abs) {
        return float_sign(body, 199, input.result_type, input.result, input.left, input.sign);
    }
    u32 length = input.right == 0 ? 6 : 7;
    put(body, 0u64, instruction(12, length));
    put(body, 1u64, input.result_type);
    put(body, 2u64, input.result);
    put(body, 3u64, input.extended_set);
    put(body, 4u64, math_opcode(input.operation));
    put(body, 5u64, input.left);
    if (length == 7) { put(body, 6u64, input.right); }
    return u64(length);
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

extern "C" u64 select_words(address body, u64 at, u32 result_type, u32 result,
                            u32 condition, u32 yes, u32 no) on cpu {
    return words_five(body, at, 169, result_type, result, condition, yes, no);
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

extern "C" PayloadWrite summary_addresses(PageAddress input, u32 step, address pointers, u32 count, address body) on cpu {
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

u64 aligned_load(address body, u64 at, u32 type_id, u32 result, u32 pointer, u32 alignment) on cpu {
    return words_five(body, at, 61, type_id, result, pointer, 2, alignment);
}

u64 aligned_store(address body, u64 at, u32 pointer, u32 value, u32 alignment) on cpu {
    return words_four(body, at, 62, pointer, value, 2, alignment);
}

extern "C" u64 read_summary_words(address body, u32 type_id, u32 first, address pointers, u32 count) on cpu {
    u64 at = 0u64;
    for (part in 0..count) {
        at = aligned_load(body, at, type_id, first + part, read_word(pointers, part), 4);
    }
    return at;
}

extern "C" u64 write_summary_words(address body, address pointers, address values, u32 count) on cpu {
    u64 at = 0u64;
    for (part in 0..count) {
        at = aligned_store(body, at, read_word(pointers, part), read_word(values, part), 4);
    }
    return at;
}

extern "C" PayloadRead payload_read(PayloadEmission input, address operands, address shifts, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    if (input.access != PayloadAccess::Words) {
        for (part in 0..input.count) {
            u32 pointer = read_word(operands, part);
            if (input.access == PayloadAccess::Physical) {
                at = aligned_load(body, at, input.unit_type, next, pointer, input.unit_bytes);
            } else {
                at = words_three(body, at, 61, input.unit_type, next, pointer);
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

extern "C" PayloadWrite typed_payload_read(PayloadEmission input, PayloadResult result, address operands, address shifts, address body) on cpu {
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

extern "C" PayloadWrite read_uniform(UniformRead input, address body) on cpu {
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

extern "C" PayloadWrite payload_write(PayloadEmission input, u32 value, address pointers, address shifts, address output, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    for (part in 0..input.count) {
        u32 word = payload_word(out, input, value, part == 0 ? 0 : read_word(shifts, part));
        if (input.access == PayloadAccess::Words) {
            put(output, u64(part), word);
        } else if (input.access == PayloadAccess::Physical) {
            out.words = aligned_store(body, out.words, read_word(pointers, part), word, input.unit_bytes);
        } else {
            out.words = words_two(body, out.words, 62, read_word(pointers, part), word);
        }
    }
    return PayloadWrite(out.words, out.next);
}

extern "C" PayloadWrite pack_field(PayloadEmission input, PackedField field, u32 high_shift, address packed, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    u32 first = field.offset / 4;
    for (part in 0..input.count) {
        u32 word = payload_word(out, input, field.value, part == 0 ? 0 : high_shift);
        if (field.byte_shift != 0) {
            word = stage_op(out, 196, input.unit_type, word, field.byte_shift);
        }
        u32 slot = first + part;
        u32 previous = read_word(packed, slot);
        if (previous != field.zero) {
            word = stage_op(out, 197, input.unit_type, previous, word);
        }
        put(packed, u64(slot), word);
    }
    return PayloadWrite(out.words, out.next);
}

u64 open_conditional(address body, u64 at, u32 merge, u32 condition, u32 yes, u32 no) on cpu {
    u64 end = words_two(body, at, 247, merge, 0);
    return words_three(body, end, 250, condition, yes, no);
}

extern "C" ControlWords open_selection(u32 merge, u32 condition, u32 yes, u32 no, address body) on cpu {
    u64 at = open_conditional(body, 0u64, merge, condition, yes, no);
    return ControlWords(words_one(body, at, 248, yes), yes, 1);
}

u64 branch_if_reached(address body, u64 at, u32 target, u32 reaches) on cpu {
    return reaches != 0 ? words_one(body, at, 249, target) : at;
}

extern "C" ControlWords next_control_block(u32 target, u32 label, u32 reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, target, reaches);
    return ControlWords(words_one(body, at, 248, label), label, 1);
}

extern "C" ControlWords close_selection(u32 merge, u32 yes_reaches, u32 no_reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, merge, no_reaches);
    at = words_one(body, at, 248, merge);
    u32 reaches = yes_reaches | no_reaches;
    if (reaches == 0) {
        put(body, at, instruction(255, 1));
        at += 1u64;
    }
    return ControlWords(at, merge, reaches);
}

extern "C" ControlWords open_loop(LoopLabels labels, address body) on cpu {
    u64 at = words_one(body, 0u64, 249, labels.header);
    at = words_one(body, at, 248, labels.header);
    at = words_three(body, at, 246, labels.end, labels.next, 0);
    at = words_one(body, at, 249, labels.check);
    return ControlWords(words_one(body, at, 248, labels.check), labels.check, 1);
}

extern "C" ControlWords test_loop(LoopLabels labels, u32 test, address body) on cpu {
    u64 at = words_three(body, 0u64, 250, test, labels.run, labels.end);
    return ControlWords(words_one(body, at, 248, labels.run), labels.run, 1);
}

extern "C" ControlWords close_loop(LoopLabels labels, u32 reaches, address body) on cpu {
    u64 at = branch_if_reached(body, 0u64, labels.next, reaches);
    at = words_one(body, at, 248, labels.next);
    at = words_one(body, at, 249, labels.header);
    return ControlWords(words_one(body, at, 248, labels.end), labels.end, 1);
}

extern "C" ControlWords open_function_scope(u32 merge, u32 selector, u32 begin, address body) on cpu {
    u64 at = words_two(body, 0u64, 247, merge, 0);
    at = words_two(body, at, 251, selector, begin);
    return ControlWords(words_one(body, at, 248, begin), begin, 1);
}

extern "C" ControlWords leave_control(u32 target, address body) on cpu {
    return ControlWords(words_one(body, 0u64, 249, target), 0, 0);
}

extern "C" ControlWords dispatch_loop_return(LoopReturn input, address edges, address body) on cpu {
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

extern "C" SubgroupWords encode_subgroup(SubgroupEmission input, address body) on cpu {
    SubgroupOperation operation = input.operation;
    u32 type = input.result_type;
    u32 result = input.result;
    u32 scope = input.scope_or_pointer;
    if (operation == SubgroupOperation::Lane || operation == SubgroupOperation::Size || operation == SubgroupOperation::Id) {
        return SubgroupWords(words_three(body, 0u64, 61, type, result, input.scope_or_pointer), input.next);
    }
    if (operation == SubgroupOperation::Elect) {
        return SubgroupWords(words_three(body, 0u64, 333, type, result, scope), input.next);
    }
    if (operation == SubgroupOperation::Any || operation == SubgroupOperation::All || operation == SubgroupOperation::BroadcastFirst) {
        u32 opcode = operation == SubgroupOperation::Any ? 335 : (operation == SubgroupOperation::All ? 334 : 338);
        return SubgroupWords(words_four(body, 0u64, opcode, type, result, scope, input.value), input.next);
    }
    if (operation == SubgroupOperation::Count || operation == SubgroupOperation::PrefixCount) {
        u32 ballot = input.next;
        u32 group = operation == SubgroupOperation::PrefixCount ? 2 : 0;
        u64 at = words_four(body, 0u64, 339, input.ballot_type, ballot, scope, input.value);
        at = words_five(body, at, 342, type, result, scope, group, ballot);
        return SubgroupWords(at, ballot + 1);
    }
    if (operation == SubgroupOperation::Shuffle || operation == SubgroupOperation::ShuffleXor ||
        operation == SubgroupOperation::ShuffleUp || operation == SubgroupOperation::ShuffleDown) {
        u32 opcode = 345 + u32(operation) - u32(SubgroupOperation::Shuffle);
        u64 at = words_five(body, 0u64, opcode, type, result, scope, input.value, input.argument);
        return SubgroupWords(at, input.next);
    }
    SubgroupArithmetic arithmetic = subgroup_arithmetic(operation, input.scalar);
    u64 at = words_five(body, 0u64, arithmetic.opcode, type, result, scope, arithmetic.group, input.value);
    return SubgroupWords(at, input.next);
}

extern "C" TextureWords encode_texture_sample(TextureSampleEmission input, address results, address body, address decorations) on cpu {
    u32 sampled = input.next;
    u32 pointer = input.variable;
    u64 at = 0u64;
    u64 annotated = 0u64;
    if (input.element != 0) {
        u32 index = sampled;
        pointer = sampled + 1;
        sampled += 2;
        at = words_three(body, at, 83, input.index_type, index, input.element);
        at = words_four(body, at, 65, input.sampled_pointer, pointer, input.variable, index);
        annotated = flag_decoration(decorations, annotated, FlagDecoration::NonUniform, index);
        annotated = flag_decoration(decorations, annotated, FlagDecoration::NonUniform, pointer);
        annotated = flag_decoration(decorations, annotated, FlagDecoration::NonUniform, sampled);
    }
    u32 coordinate = sampled + 1;
    u32 value = sampled + 2;
    at = words_three(body, at, 61, input.sampled_type, sampled, pointer);
    if (input.z == 0) {
        at = words_four(body, at, 80, input.coordinate_type, coordinate, input.x, input.y);
    } else {
        at = words_five(body, at, 80, input.coordinate_type, coordinate, input.x, input.y, input.z);
    }
    put(body, at, instruction(88, 7));
    put(body, at + 1u64, input.vector_type);
    put(body, at + 2u64, value);
    put(body, at + 3u64, sampled);
    put(body, at + 4u64, coordinate);
    put(body, at + 5u64, 2);
    put(body, at + 6u64, input.lod);
    at += 7u64;
    for (component in 0..4) {
        at = words_four(body, at, 81, input.component_type, read_word(results, component), value, component);
    }
    return TextureWords(at, annotated, sampled + 3);
}

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

extern "C" u64 encode_atomic(AtomicEmission input, address body) on cpu {
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

extern "C" AppendWords encode_append_reservation(AppendEmission input, address body) on cpu {
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
    at += encode_atomic(AtomicEmission(AtomicOperation::Load, BuiltinScalar::U32,
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
    at += encode_atomic(AtomicEmission(AtomicOperation::CompareExchange, BuiltinScalar::U32,
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
    u64 at = out.words;
    u32 result = out.next;
    put(out.body, at, instruction(245, 7));
    put(out.body, at + 1u64, ty);
    put(out.body, at + 2u64, result);
    put(out.body, at + 3u64, a);
    put(out.body, at + 4u64, edge_a);
    put(out.body, at + 5u64, b);
    put(out.body, at + 6u64, edge_b);
    out.words = at + 7u64;
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

u32 sort_unary(inout StageCursor out, u32 op, u32 ty, u32 value) on cpu {
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
        u32 inverted = sort_unary(out, 200, u, selected);
        u32 matching = stage_select(out, u, set, selected, inverted);
        equal = stage_op(out, 199, u, equal, matching);
    }
    u32 bit = stage_op(out, 196, u, types.one, lane);
    u32 lower = stage_op(out, 130, u, bit, types.one);
    u32 prior = stage_op(out, 199, u, equal, lower);
    u32 rank = sort_unary(out, 205, u, prior);
    u32 total = sort_unary(out, 205, u, equal);

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

extern "C" DispatchValue encode_dispatch_grid(DispatchGrid input, address body) on cpu {
    // The command and the shader consume the same first page of indirect records.
    u32 header = input.next;
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
        return dispatch_levels(body, cursor, input, first, input.count, input.capacity);
    }

    DispatchValue root = dispatch_record(body, cursor, input, first, input.count);
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

extern "C" StageWords write_sort_key(SortKeyWrite input, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    u32 key = input.key;
    if (input.shift != 0) {
        u32 negative = stage_op(out, 194, input.type_id, input.sign, input.shift);
        u32 mask = stage_op(out, 130, input.type_id, input.zero, negative);
        if (input.bias != 0) {
            mask = stage_op(out, 197, input.type_id, mask, input.bias);
        }
        key = stage_op(out, 198, input.type_id, key, mask);
    } else if (input.bias != 0) {
        key = stage_op(out, 198, input.type_id, key, input.bias);
    }
    if (input.inverted != 0) {
        key = stage_op(out, 198, input.type_id, key, input.inverted);
    }
    out.words = words_four(body, out.words, 62, input.key_pointer, key, 2, 4);
    out.words = words_four(body, out.words, 62, input.index_pointer, input.original, 2, 4);
    return StageWords(out.words, out.next, input.label);
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
            DispatchGrid grid = indices.grid;
            grid.count = count;
            grid.next = out.next;
            DispatchValue written = encode_dispatch_grid(grid, word_at(out.body, out.words));
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

StageWords emit_scatter(ScatterInput input, read ScatterTarget target, address body) on cpu {
    PageAddress flag = input.flag;
    StageCursor out = StageCursor(body, 0u64, flag.next);
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
    return emit_scatter(input, target, body);
}

extern "C" StageWords encode_index_scatter(ScatterInput input, u32 target_descriptor,
    DispatchGrid grid, address body) on cpu {
    let target = ScatterTarget::Indices(ScatterIndices(target_descriptor, grid));
    return emit_scatter(input, target, body);
}

u64 widen_component(address body, u64 at, PrefixIndexEmission input,
                    u32 vector, u32 component, u32 result) on cpu {
    u32 narrow = result - 1;
    u64 end = words_four(body, at, 81, input.uint_type, narrow, vector, component);
    return words_three(body, end, 113, input.wide_type, result, narrow);
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

extern "C" SharedAddressWords shared_addresses(SharedAddress input, address offsets, address body) on cpu {
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

extern "C" void group_remaining(address body, u32 uint_type, u32 index, u32 lane, u32 count, u32 first) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    stage_group_remaining(out, uint_type, index, lane, count);
}

extern "C" TileRangeWords tile_range(address body, u32 uint_type, u32 bool_type, u32 index, u32 count, u32 width, u32 first) on cpu {
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

extern "C" TreeWords tree_layer(TreeLayer input, address components, u32 count,
    address body, address decorations) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    return stage_tree_layer(out, input, 0, components, count, address(), decorations);
}

extern "C" PrivateAddressWords private_address(PrivateAddress input, address parts, address pointers, address body) on cpu {
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

u64 ray_phi(address body, u64 at, u32 type, u32 result, u32 hit, u32 yes, u32 miss, u32 no) on cpu {
    put(body, at, instruction(245, 7));
    put(body, at + 1u64, type);
    put(body, at + 2u64, result);
    put(body, at + 3u64, hit);
    put(body, at + 4u64, yes);
    put(body, at + 5u64, miss);
    put(body, at + 6u64, no);
    return at + 7u64;
}

RayWords ray_closest(RayEmission input, address results, address body, u64 start, u32 first) on cpu {
    u64 at = start;
    u32 yes = first;
    u32 no = first + 1;
    u32 merge = first + 2;
    u32 distance = first + 3;
    u32 barycentric = first + 4;
    u32 u = first + 5;
    u32 v = first + 6;
    u32 instance = first + 7;
    u32 primitive = first + 8;
    u32 next = first + 9;
    u32 f = input.f32_type;
    u32 q = input.query;
    u32 committed = input.committed;
    at += open_selection(merge, read_word(results, 0), yes, no, word_at(body, at)).words;
    at = words_four(body, at, 6018, f, distance, q, committed);
    at = words_four(body, at, 6024, input.vec2_type, barycentric, q, committed);
    at = words_four(body, at, 81, f, u, barycentric, 0);
    at = words_four(body, at, 81, f, v, barycentric, 1);
    at = words_four(body, at, 6020, input.u32_type, instance, q, committed);
    at = words_four(body, at, 6023, input.u32_type, primitive, q, committed);

    u32 positions = 0;
    if (input.position_type != 0) {
        u32 points = next;
        positions = next + 1;
        next += 10;
        at = words_four(body, at, 5340, input.position_type, points, q, committed);
        for (part in 0..9) {
            at = words_five(body, at, 81, f, positions + part, points, part / 3, part % 3);
        }
    }

    array<u32,2> types = array<u32,2>(input.object_to_world, input.world_to_object);
    array<u32,2> matrices = array<u32,2>(0, 0);
    for (slot in 0..2) {
        if (types[slot] != 0) {
            u32 matrix = next;
            matrices[slot] = next + 1;
            next += 13;
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
    at = ray_phi(body, at, f, read_word(results, 1), distance, yes, input.maximum, no);
    at = ray_phi(body, at, f, read_word(results, 2), u, yes, z, no);
    at = ray_phi(body, at, f, read_word(results, 3), v, yes, z, no);
    at = ray_phi(body, at, input.u32_type, read_word(results, 4), instance, yes, input.zero, no);
    at = ray_phi(body, at, input.u32_type, read_word(results, 5), primitive, yes, input.zero, no);
    if (positions != 0) {
        for (part in 0..9) {
            at = ray_phi(body, at, f, read_word(results, 6 + part), positions + part, yes, z, no);
        }
    }
    for (slot in 0..2) {
        if (matrices[slot] != 0) {
            for (part in 0..12) {
                at = ray_phi(body, at, f, read_word(results, 15 + 12 * slot + part),
                             matrices[slot] + part, yes, z, no);
            }
        }
    }
    return RayWords(at, next, merge);
}

extern "C" RayWords encode_ray(RayEmission input, address results, address body) on cpu {
    u32 first = input.next;
    u32 acceleration = first;
    u32 origin = first + 1;
    u32 direction = first + 2;
    u32 header = first + 3;
    u32 test = first + 4;
    u32 again = first + 5;
    u32 end = first + 6;
    u32 more = first + 7;
    u32 kind = first + 8;
    u32 next = first + 9;
    u32 query = input.query;
    u64 at = words_three(body, 0u64, 61, input.acceleration_type, acceleration, input.acceleration);
    at = words_five(body, at, 80, input.vec3_type, origin, input.origin[0], input.origin[1], input.origin[2]);
    at = words_five(body, at, 80, input.vec3_type, direction, input.direction[0], input.direction[1], input.direction[2]);
    put(body, at, instruction(4473, 9));
    put(body, at + 1u64, query);
    put(body, at + 2u64, acceleration);
    put(body, at + 3u64, input.flags);
    put(body, at + 4u64, input.mask);
    put(body, at + 5u64, origin);
    put(body, at + 6u64, input.minimum);
    put(body, at + 7u64, direction);
    put(body, at + 8u64, input.maximum);
    at += 9u64;

    LoopLabels loop = LoopLabels(header, test, again, again, end);
    at += open_loop(loop, word_at(body, at)).words;
    at = words_three(body, at, 4477, input.bool_type, more, query);
    at += test_loop(loop, more, word_at(body, at)).words;
    at = words_one(body, at, 249, header);
    at = words_one(body, at, 248, end);
    at = words_four(body, at, 4479, input.u32_type, kind, query, input.committed);
    at = words_four(body, at, 171, input.bool_type, read_word(results, 0), kind, input.zero);
    if (input.closest != 0) {
        return ray_closest(input, results, body, at, next);
    }
    return RayWords(at, next, end);
}

extern "C" TileAddressWords tile_address(TileAddress input, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    u32 index = input.index;
    u32 byte = input.offset;
    u32 u = input.index_type;
    if (input.offset != 0) {
        if (input.bias != 0) {
            at = words_four(body, at, 128, u, next, byte, input.bias);
            byte = next;
            next += 1;
        }
        u32 cell = next;
        u32 lane_offset = next + 1;
        u32 lane = next + 2;
        next += 3;
        at = words_four(body, at, 134, u, cell, byte, input.cell_bytes);
        at = words_four(body, at, 132, u, lane_offset, cell, input.width);
        at = words_four(body, at, 128, u, lane, index, lane_offset);
        index = lane;
    }
    if (input.base != 0) {
        at = words_four(body, at, 128, u, next, index, input.base);
        index = next;
        next += 1;
    }
    at = words_four(body, at, 65, input.pointer_type, next, input.shared, index);
    return TileAddressWords(at, next + 1, next, byte);
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

extern "C" StageWords reduction_writeback(ReductionWriteback input, u32 lane, u32 root,
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

extern "C" TreeWords reduction_group(ReductionGroup input, address strides, address components,
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

extern "C" TreeWords scan_group(ScanGroup input, address levels, address components,
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

extern "C" TreeWords add_scan_prefix(ScanPrefix input, address fields, u32 count,
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

extern "C" CollectiveBranch open_custom_segment(u32 bool_type, u32 right_head, u32 zero,
    u32 next, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, next);
    u32 starts = stage_op(out, 171, bool_type, right_head, zero);
    StageBranch branch = stage_branch(out, starts, true);
    return CollectiveBranch(out.words, out.next, branch.run, branch.end);
}

extern "C" StageWords close_custom_segment(CustomSegmentJoin input, address left, address right,
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

extern "C" CustomTreeOpen open_custom_tree(CustomTreeInput input, address offsets, address own,
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


extern "C" StageWords close_custom_tree(CustomTreeClose input, address own, address left,
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

extern "C" CustomPrefixOpen open_custom_prefix(read CustomPrefix input, address fields,
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

extern "C" StageWords close_custom_prefix(read CustomPrefix input, CustomPrefixOpen opened,
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

extern "C" TreeWords segment_add(SegmentAdd input, address fields, u32 count,
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

extern "C" CollectiveBranch custom_collective_read(CustomCollectiveSource input,
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
            array<u32,2> pointer = stage_field_pointers(out, record, input.head_offset, source.result_pointer,
                source.u64_type, 0);
            u32 head = stage_aligned_load(out, source.u32_type, pointer[0], 4);
            if (input.tag_words == 2) {
                u32 tag = segment_indicator(out, source, input.bool_type, head, true);
                put(values, u64(count), tag);
                put(values, u64(count + 1), tag);
            } else {
                u32 starts = stage_op(out, 171, input.bool_type, head, source.zero);
                u32 position = stage_op(out, 128, source.u32_type, input.lane, source.one);
                put(values, u64(count), stage_select(out, source.u32_type, starts, position, source.zero));
            }
        }
    }
    return CollectiveBranch(out.words, out.next, read.run, read.end);
}


extern "C" StageWords custom_collective_join(CustomCollectiveJoin input, address values,
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

extern "C" StageWords custom_scan_write(CustomScanWrite input, address own, address identity,
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

extern "C" SummaryRootWords custom_summary_root(CustomSummaryRoot input, address identity,
    u32 words, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.target.next);
    u32 u = input.target.u32_type;
    array<u32,2> writers = stage_collective_writer(out, input.bool_type, input.lane,
        input.active, input.length, input.target.zero);
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

extern "C" CollectiveBranch open_custom_segment_finish(CustomSegmentPrefix input, address own,
    address identity, u32 words, address workspace, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.parent.next);
    StageBranch write = stage_open(out, input.active);
    read_segment_values(out, input.parent.u32_type, words, own, workspace);
    address parent = word_at(workspace, u64(words));
    u32 run = segment_prefix(out, input.parent, input.bool_type, write.run, input.step, words, identity, parent);
    return CollectiveBranch(out.words, out.next, run, write.end);
}

extern "C" StageWords close_custom_segment_finish(CustomSegmentOutput input, address fields,
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

extern "C" TreeWords segment_group(SegmentGroup input, address levels, address fields,
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


extern "C" PayloadRead tile_extract(TileExtract input, address body) on cpu {
    u32 value = input.next;
    u32 next = value + 1;
    u64 at = words_three(body, 0u64, 61, input.cell_type, value, input.pointer);
    u32 shift = input.shift;
    if (input.byte != 0) {
        u32 part = next;
        shift = next + 1;
        next += 2;
        at = words_four(body, at, 137, input.index_type, part, input.byte, input.cell_bytes);
        at = words_four(body, at, 132, input.index_type, shift, part, input.bits_per_byte);
    }
    if (shift != 0) {
        at = words_four(body, at, 194, input.cell_type, next, value, shift);
        value = next;
        next += 1;
    }
    return PayloadRead(at, next, value);
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

extern "C" StageEntries stage_entries(u32 operation,u32 selected,u32 prefix,u32 root_add,usize keys) on cpu {
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

extern "C" SortShaderEntry sort_shader_entry(usize index) on cpu {
 if(index<8u64){return SortShaderEntry(0,index*4u64);}
 if(index<16u64){return SortShaderEntry(1,(index-8u64)*4u64);}
 if(index<20u64){return SortShaderEntry(u32(index-14u64),0u64);}
 return SortShaderEntry(6,index-20u64);
}

extern "C" u32 custom_kernel_preparation(u32 pass,u32 segmented) on cpu {
 if(pass==3){return 0;}
 if(pass==0 || pass==2 && segmented!=0){return 1;}
 if(pass==1 || pass==2){return 3;}
 return 2;
}
