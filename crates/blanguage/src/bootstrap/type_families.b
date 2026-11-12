module bootstrap.type_families;

const u32 FLOATING = 1;
const u32 INTEGER = 2;
const u32 SCALAR = 4;


BuiltinName member<const N:u32 in {2,3,4,5,6}>(read array<u8,N> name) on cpu {
    array<u8,6> bytes = array<u8,6>(u8(0),u8(0),u8(0),u8(0),u8(0),u8(0));
    for (index in 0..N) { bytes[index] = name[index]; }
    return BuiltinName(bytes,N);
}

Optional<u32> family_requirement(read TypeFamilyNode family) on cpu {
    match (family) {
        TypeFamilyNode::Numeric() { return Optional<u32>(1, 0); }
        TypeFamilyNode::Floating() { return Optional<u32>(1, FLOATING); }
        TypeFamilyNode::Integer() { return Optional<u32>(1, INTEGER); }
        TypeFamilyNode::ScalarNumeric() { return Optional<u32>(1, SCALAR); }
        TypeFamilyNode::ScalarInteger() { return Optional<u32>(1, SCALAR | INTEGER); }
        TypeFamilyNode::Named(name) { return Optional<u32>(0, 0); }
    }
}

array<BuiltinName,19> ordered_members() on cpu {
    return array<BuiltinName,19>(
        member("f32"),
        member("float2"),member("float3"),member("float4"),
        member("u32"),
        member("uint2"),member("uint3"),member("uint4"),
        member("i32"),
        member("int2"),member("int3"),member("int4"),
        member("u64"),member("i64"),
        member("f64"),
        member("u8"),member("i8"),
        member("u16"),member("i16")
    );
}

bool matches_family(read BuiltinName name, u32 required) on cpu {
    BuiltinType builtin = recognize(address_of(name.bytes), u64(name.length));
    u32 traits = builtin.scalar == BuiltinScalar::F32 || builtin.scalar == BuiltinScalar::F64 ? FLOATING : INTEGER;
    if (builtin.rows == 1 && builtin.columns == 1) { traits |= SCALAR; }
    return (traits & required) == required;
}

extern "C" u32 builtin_members(read TypeFamilyNode family, address output) on cpu {
    Optional<u32> required = family_requirement(family);
    if (required.present == 0) { return 0; }
    array<BuiltinName,19> members = ordered_members();
    u32 count = 0;
    for (index in 0..members.length) {
        BuiltinName name = members[index];
        if (matches_family(name, required.value)) {
            address slot = unsafe_address(address_bits(output) + u64(count) * size_of<BuiltinName>());
            unsafe_position<BuiltinName>(slot) = name;
            count += 1;
        }
    }
    return count;
}

bool matches_name(read BuiltinName member, address name, usize length) on cpu {
    if (length != usize(member.length)) { return false; }
    address spelling = address_of(member.bytes);
    for (index in 0..member.length) {
        if (name_byte(spelling, u64(index)) != name_byte(name, u64(index))) { return false; }
    }
    return true;
}

extern "C" bool builtin_has_member(read TypeFamilyNode family, address name, usize length) on cpu {
    Optional<u32> required = family_requirement(family);
    if (required.present == 0) { return false; }
    array<BuiltinName,19> members = ordered_members();
    for (index in 0..members.length) {
        if (matches_name(members[index], name, length)) { return matches_family(members[index], required.value); }
    }
    return false;
}

u8 name_byte(address name, u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(name) + index));
}
bool same<const N:u32 in {2,3,4,5,6,7,16}>(address name, u64 length, read array<u8,N> word) on cpu {
    if (length != u64(N)) { return false; }
    for (index in 0..N) {
        if (name_byte(name, u64(index)) != word[index]) { return false; }
    }
    return true;
}
BuiltinType numeric(BuiltinScalar scalar, u32 rows, u32 columns) on cpu {
    return BuiltinType(BuiltinKind::Numeric, scalar, rows, columns);
}
extern "C" BuiltinType recognize(address name, u64 length) on cpu {
    if (length == 8u64 && same(name, 5u64, "float")) {
        u8 rows = name_byte(name, 5u64);
        u8 columns = name_byte(name, 7u64);
        if (rows >= 50 && rows <= 52 && name_byte(name, 6u64) == 120 && columns >= 50 && columns <= 52) {
            return numeric(BuiltinScalar::F32, u32(rows) - 48, u32(columns) - 48);
        }
    }
    if (length == 6u64 && same(name, 5u64, "float")) {
        u8 columns = name_byte(name, 5u64);
        if (columns >= 50 && columns <= 52) { return numeric(BuiltinScalar::F32, 1, u32(columns) - 48); }
    }
    if (length == 5u64 && same(name, 4u64, "uint")) {
        u8 columns = name_byte(name, 4u64);
        if (columns >= 50 && columns <= 52) { return numeric(BuiltinScalar::U32, 1, u32(columns) - 48); }
    }
    if (length == 4u64 && same(name, 3u64, "int")) {
        u8 columns = name_byte(name, 3u64);
        if (columns >= 50 && columns <= 52) { return numeric(BuiltinScalar::I32, 1, u32(columns) - 48); }
    }
    if (same(name, length, "f32") || same(name, length, "float")) { return numeric(BuiltinScalar::F32, 1, 1); }
    if (same(name, length, "u32") || same(name, length, "uint")) { return numeric(BuiltinScalar::U32, 1, 1); }
    if (same(name, length, "i32") || same(name, length, "int")) { return numeric(BuiltinScalar::I32, 1, 1); }
    if (same(name, length, "bool")) { return numeric(BuiltinScalar::Bool, 1, 1); }
    if (same(name, length, "u64") || same(name, length, "usize")) { return numeric(BuiltinScalar::U64, 1, 1); }
    if (same(name, length, "i64") || same(name, length, "isize")) { return numeric(BuiltinScalar::I64, 1, 1); }
    if (same(name, length, "f64")) { return numeric(BuiltinScalar::F64, 1, 1); }
    if (same(name, length, "u8")) { return numeric(BuiltinScalar::U8, 1, 1); }
    if (same(name, length, "i8")) { return numeric(BuiltinScalar::I8, 1, 1); }
    if (same(name, length, "u16")) { return numeric(BuiltinScalar::U16, 1, 1); }
    if (same(name, length, "i16")) { return numeric(BuiltinScalar::I16, 1, 1); }
    if (same(name, length, "address")) { return BuiltinType(BuiltinKind::Address, BuiltinScalar::U64, 1, 1); }
    if (same(name, length, "handle")) { return BuiltinType(BuiltinKind::Handle, BuiltinScalar::U32, 1, 2); }
    if (same(name, length, "RayHit")) { return BuiltinType(BuiltinKind::RayHit, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "array")) { return BuiltinType(BuiltinKind::Array, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "result")) { return BuiltinType(BuiltinKind::Result, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "owned")) { return BuiltinType(BuiltinKind::Owned, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "task")) { return BuiltinType(BuiltinKind::Task, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "task_start_error")) { return BuiltinType(BuiltinKind::TaskStartError, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "void")) { return BuiltinType(BuiltinKind::Void, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "never")) { return BuiltinType(BuiltinKind::Never, BuiltinScalar::F32, 0, 0); }
    if (same(name, length, "cfn")) { return BuiltinType(BuiltinKind::CFunction, BuiltinScalar::F32, 0, 0); }
    return BuiltinType(BuiltinKind::Missing, BuiltinScalar::F32, 0, 0);
}
