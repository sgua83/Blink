module codec.builtin_type;

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
