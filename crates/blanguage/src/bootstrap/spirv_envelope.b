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

extern "C" PageAddressResult reduction_record(PageAddress input, address body) on cpu {
    PhysicalAddress record = physical_page_address(input, body, input.next);
    return PageAddressResult(record.words, record.next, record.value, 0);
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

extern "C" PayloadWrite payload_write(PayloadEmission input, u32 value, address pointers, address shifts, address output, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    for (part in 0..input.count) {
        u32 word = value;
        if (part != 0) {
            at = words_four(body, at, 194, input.raw_type, next, value, read_word(shifts, part));
            word = next;
            next += 1;
        }
        if (input.raw_type != input.unit_type) {
            at = words_three(body, at, 113, input.unit_type, next, word);
            word = next;
            next += 1;
        }
        if (input.access == PayloadAccess::Words) {
            put(output, u64(part), word);
        } else if (input.access == PayloadAccess::Physical) {
            at = aligned_store(body, at, read_word(pointers, part), word, input.unit_bytes);
        } else {
            at = words_two(body, at, 62, read_word(pointers, part), word);
        }
    }
    return PayloadWrite(at, next);
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

u32 stage_load(inout StageCursor out, u32 ty, u32 pointer, bool physical) on cpu {
    u32 result = out.next;
    if (physical) {
        out.words = aligned_load(out.body, out.words, ty, result, pointer, 4);
    } else {
        out.words = words_three(out.body, out.words, 61, ty, result, pointer);
    }
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

StageBranch stage_open(inout StageCursor out, u32 condition) on cpu {
    u32 run = out.next;
    u32 end = run + 1;
    out.words = open_conditional(out.body, out.words, end, condition, run, end);
    out.words = words_one(out.body, out.words, 248, run);
    out.next = end + 1;
    return StageBranch(run, end);
}

void stage_close(inout StageCursor out, StageBranch branch) on cpu {
    out.words = words_one(out.body, out.words, 249, branch.end);
    out.words = words_one(out.body, out.words, 248, branch.end);
}

u32 sort_phi(inout StageCursor out, u32 ty, u32 zero, u32 before, u32 value, u32 active) on cpu {
    u64 at = out.words;
    u32 result = out.next;
    put(out.body, at, instruction(245, 7));
    put(out.body, at + 1u64, ty);
    put(out.body, at + 2u64, result);
    put(out.body, at + 3u64, zero);
    put(out.body, at + 4u64, before);
    put(out.body, at + 5u64, value);
    put(out.body, at + 6u64, active);
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
    stage_close(out, read);
    u32 key = sort_phi(out, u, zero, input.label, loaded, read.run);
    if (!histogram) { ordinal = sort_phi(out, u, zero, input.label, ordinal, read.run); }

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
    stage_close(out, clear);
    stage_barrier(out, input.workgroup_scope, input.shared_semantics);

    u32 first = stage_op(out, 170, b, rank, zero);
    u32 leader = stage_op(out, 167, b, first, active);
    u32 wave_bin = stage_op(out, 132, u, wave, input.bins);
    u32 own_bin = stage_op(out, 128, u, wave_bin, digit);
    StageBranch write = stage_open(out, leader);
    u32 cell_out = stage_op(out, 65, input.shared_pointer, input.shared, own_bin);
    out.words = words_two(body, out.words, 62, cell_out, total);
    stage_close(out, write);
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
        stage_close(out, sum);
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
    stage_close(out, scatter);
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
                              u32 base, u32 count, address offsets, u32 slot) on cpu {
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
        DispatchValue pointer = dispatch_pointer(body, output, input, base, read_word(offsets, slot + i));
        u64 at = words_four(body, pointer.words, 62, pointer.value, values[i], 2, 4);
        output = DispatchValue(at, pointer.next, 0);
    }
    return DispatchValue(output.words, output.next, groups.value);
}

DispatchValue dispatch_levels(address body, DispatchValue cursor, DispatchGrid input,
                              u32 base, address offsets, u32 start, u32 count) on cpu {
    DispatchValue first = dispatch_record(body, cursor, input, base, count, offsets, start);
    DispatchValue level = first;
    for (index in 1..input.levels) {
        level = dispatch_record(body, level, input, base, level.value, offsets, start + index * 4);
    }
    return DispatchValue(level.words, level.next, first.value);
}

extern "C" DispatchValue encode_dispatch_grid(DispatchGrid input, address offsets, address body) on cpu {
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
    DispatchValue cursor = DispatchValue(at, base + 1, 0);
    DispatchValue levels = dispatch_levels(body, cursor, input, base, offsets, 0, input.count);
    if (input.bins != 0) {
        u32 histogram = levels.next;
        at = words_four(body, levels.words, 132, input.scalars.uint_type, histogram, levels.value, input.bins);
        levels = dispatch_levels(body, DispatchValue(at, histogram + 1, 0), input, base,
                                 offsets, input.levels * 4, histogram);
    }
    return levels;
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
    stage_close(out, run);
    return StageWords(out.words, out.next, run.end);
}

struct ScatterRecords { address copy; u32 count_descriptor; u32 count_word; };
struct ScatterIndices { u32 descriptor; DispatchGrid grid; address offsets; };
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
            DispatchValue written = encode_dispatch_grid(grid, indices.offsets, word_at(out.body, out.words));
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
    stage_close(out, empty_branch);
    stage_close(out, first_branch);

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
    stage_close(out, keep_branch);

    u32 after_index = stage_op(out, 128, flag.u32_type, flag.index, flag.one);
    u32 last = stage_op(out, 170, input.bool_type, after_index, input.count);
    StageBranch last_branch = stage_open(out, last);
    u32 chosen = out.next;
    out.next += 1;
    out.words = select_words(body, out.words, flag.u32_type, chosen, keep, flag.one, flag.zero);
    u32 total = stage_op(out, 128, flag.u32_type, prefix, chosen);
    scatter_count(out, flag, target, total);
    stage_close(out, last_branch);
    stage_close(out, active_branch);
    return StageWords(out.words, out.next, active_branch.end);
}

extern "C" StageWords encode_record_scatter(ScatterInput input, address record_copy,
    u32 count_descriptor, u32 count_word, address body) on cpu {
    let target = ScatterTarget::Records(ScatterRecords(record_copy, count_descriptor, count_word));
    return emit_scatter(input, target, body);
}

extern "C" StageWords encode_index_scatter(ScatterInput input, u32 target_descriptor,
    DispatchGrid grid, address offsets, address body) on cpu {
    let target = ScatterTarget::Indices(ScatterIndices(target_descriptor, grid, offsets));
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

extern "C" SharedAddressWords shared_addresses(SharedAddress input, address offsets, address body) on cpu {
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
        put(offsets, u64(part), next);
        next += 1;
    }
    return SharedAddressWords(at, next);
}

void tree_node_address(inout StageCursor out, TreeComponent component, u32 uint_type,
    u32 node, address pointers) on cpu {
    SharedAddress request = SharedAddress(uint_type, component.pointer_type, component.shared,
        node, component.scale, component.count, out.next);
    SharedAddressWords result = shared_addresses(request, pointers, word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
}

u32 tree_node_load(inout StageCursor out, PayloadEmission input, address pointers, address shifts) on cpu {
    PayloadEmission request = input;
    request.next = out.next;
    PayloadRead result = payload_read(request, pointers, shifts, word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
    return result.value;
}

void tree_node_store(inout StageCursor out, PayloadEmission input, u32 value, address pointers, address shifts) on cpu {
    PayloadEmission request = input;
    request.next = out.next;
    PayloadWrite result = payload_write(request, value, pointers, shifts, address(), word_at(out.body, out.words));
    out.words += result.words;
    out.next = result.next;
}


extern "C" PageAddressResult reduction_field(u32 record, u32 offset, u32 pointer_type, u32 wide_type, u32 step, u32 first, address body) on cpu {
    u64 at = 0u64;
    u32 next = first;
    u32 physical = record;
    if (offset != 0) {
        at = words_four(body, at, 128, wide_type, next, physical, offset);
        physical = next;
        next += 1;
    }
    u32 pointer = next;
    at = words_three(body, at, 120, pointer_type, pointer, physical);
    next += 1;
    u32 second = 0;
    if (step != 0) {
        at = words_four(body, at, 128, wide_type, next, physical, step);
        second = next + 1;
        at = words_three(body, at, 120, pointer_type, second, next);
        next += 2;
    }
    return PageAddressResult(at, next, pointer, second);
}

extern "C" void choose_reduction_target(u32 uint_type, u32 bool_type, u32 push_pointer, u32 push, u32 index,
    u32 width, u32 zero, u32 one, u32 length_word, u32 target_bias, u32 first, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 group = stage_op(out, 134, uint_type, index, width);
    u32 length_pointer = out.next;
    out.words = words_five(body, out.words, 65, push_pointer, length_pointer, push, zero, length_word);
    out.next += 1;
    u32 length = stage_load(out, uint_type, length_pointer, false);
    u32 final_pass = stage_op(out, 170, bool_type, length, one);
    stage_select(out, uint_type, final_pass, zero, group);
    if (target_bias != 0) {
        stage_select(out, uint_type, final_pass, target_bias, zero);
    }
}

extern "C" void group_remaining(address body, u32 uint_type, u32 index, u32 lane, u32 count, u32 first) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 begin = stage_op(out, 130, uint_type, index, lane);
    stage_op(out, 130, uint_type, count, begin);
}

extern "C" void collective_writer(address body, u32 bool_type, u32 lane, u32 active, u32 count, u32 zero, u32 first) on cpu {
    StageCursor out = StageCursor(body, 0u64, first);
    u32 first_lane = stage_op(out, 170, bool_type, lane, zero);
    u32 empty = stage_op(out, 170, bool_type, count, zero);
    u32 needed = stage_op(out, 166, bool_type, active, empty);
    stage_op(out, 167, bool_type, first_lane, needed);
}

StageBranch scan_tree_branch(inout StageCursor out, u32 uint_type, u32 bool_type, u32 lane, u32 mask) on cpu {
    u32 bits = stage_op(out, 199, uint_type, lane, mask);
    u32 selected = stage_op(out, 170, bool_type, bits, mask);
    return stage_open(out, selected);
}

extern "C" ScanTreeOpen open_scan_tree(ScanTreeInput input, address body) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.first);
    StageBranch branch = scan_tree_branch(out, input.uint_type, input.bool_type, input.lane, input.mask);
    out.next = input.other;
    stage_op(out, 130, input.uint_type, input.lane, input.stride);
    return ScanTreeOpen(out.words, branch.run, branch.end);
}

extern "C" TreeWords tree_layer(TreeLayer input, address components, u32 count, address own,
    address body, address decorations) on cpu {
    StageCursor out = StageCursor(body, 0u64, input.next);
    u32 node = input.lane;
    bool reduction = address_bits(own) != 0u64;
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
            left = unsafe_address(address_bits(own) + u64(index) * size_of<TreeParts>());
        } else {
            tree_node_address(out, component, input.uint_type, other, left);
        }
        u32 a = tree_node_load(out, payload, left, shifts);
        tree_node_address(out, component, input.uint_type, reduction ? other : node, right);
        u32 b = tree_node_load(out, payload, right, shifts);
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
                    ? stage_op(out, 128, input.uint_type, input.index, input.stride)
                    : stage_op(out, 128, input.uint_type, other, input.one);
                has_right = stage_op(out, 176, input.bool_type, right_begin, input.remaining);
            }
            value = stage_select(out, component.raw_type, has_right, value, a);
        }
        if (input.down != 0) {
            tree_node_store(out, payload, b, left, shifts);
        }
        tree_node_store(out, payload, value, reduction ? left : right, shifts);
    }
    stage_close(out, branch);
    stage_barrier(out, input.scope, input.semantics);
    return TreeWords(out.words, decorated, out.next, branch.end);
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
