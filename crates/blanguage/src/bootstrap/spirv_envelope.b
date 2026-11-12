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
        put(body, 0u64, instruction(169, 6));
        put(body, 1u64, input.result_type);
        put(body, 2u64, input.result);
        put(body, 3u64, input.value);
        put(body, 4u64, input.one);
        put(body, 5u64, input.zero);
        return 6u64;
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

struct PageOrigin { u64 words; u32 next; u32 table; u32 stride; u32 field; };

u64 dataset_header_word(address body, u64 at, PageAddress input, u32 field, u32 pointer, u32 value) on cpu {
    u64 end = words_five(body, at, 65, input.descriptor_pointer, pointer, input.descriptor, input.zero, field);
    return words_three(body, end, 61, input.u64_type, value, pointer);
}

PageOrigin page_origin(PageAddress input, address body) on cpu {
    u32 first = input.next;
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

extern "C" PageAddressResult page_address(PageAddress input, address body) on cpu {
    PageOrigin origin = page_origin(input, body);
    u32 first = origin.next;
    u32 capacity = first;
    u32 page = first + 1;
    u32 local = first + 2;
    u32 scaled = first + 3;
    u32 byte_offset = first + 4;
    u32 next = first + 5;
    u32 t32 = input.u32_type;
    u32 t64 = input.u64_type;
    u64 at = words_four(body, origin.words, 134, t32, capacity, input.page_bytes, origin.stride);
    at = words_four(body, at, 134, t32, page, input.index, capacity);
    at = words_four(body, at, 137, t32, local, input.index, capacity);
    at = words_four(body, at, 132, t32, scaled, local, origin.stride);
    at = words_four(body, at, 128, t32, byte_offset, scaled, input.word);
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
    at = words_three(body, at, 120, input.result_pointer, next, physical);
    return PageAddressResult(at, next + 1, next);
}

u32 read_word(address words, u32 part) on cpu {
    return unsafe_position<u32>(word_at(words, u64(part)));
}

extern "C" PayloadRead payload_read(PayloadEmission input, address operands, address shifts, address body) on cpu {
    u64 at = 0u64;
    u32 next = input.next;
    if (input.access != PayloadAccess::Words) {
        for (part in 0..input.count) {
            u32 pointer = read_word(operands, part);
            if (input.access == PayloadAccess::Physical) {
                at = words_five(body, at, 61, input.unit_type, next, pointer, 2, input.unit_bytes);
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
            at = words_four(body, at, 62, read_word(pointers, part), word, 2, input.unit_bytes);
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
    at = words_five(body, at, 169, uint_type, amount, partial, available, size);
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
