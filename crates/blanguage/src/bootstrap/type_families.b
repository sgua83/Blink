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

extern "C" NamedSetStep named_set_next(address cursor, u32 entered) on cpu {
    NamedSetCursor set=unsafe_position<NamedSetCursor>(cursor);
    TypeRef none=TypeRef(0u64);
    if (set.phase==0) {
        if (set.members.count==0u64) { return NamedSetStep(NamedSetNeed::Empty,0,none); }
        unsafe_position<NamedSetCursor>(cursor).phase=1;
        return NamedSetStep(NamedSetNeed::Enter,0,none);
    }
    if (entered==0) { return NamedSetStep(NamedSetNeed::Recursive,0,none); }
    if (set.next==set.members.count) {
        return NamedSetStep(NamedSetNeed::Done,set.matched,none);
    }
    u64 nodes=address_bits(set.storage.nodes);
    u64 member=unsafe_position<u64>(unsafe_address(nodes+set.members.offset+set.next*size_of<u64>()));
    TypeNode value=unsafe_position<TypeNode>(unsafe_address(nodes+member));
    if (value.dataset!=0 || value.view!=0 || value.tile!=0) {
        return NamedSetStep(NamedSetNeed::Wrapped,0,none);
    }
    unsafe_position<NamedSetCursor>(cursor).next=set.next+1u64;
    return NamedSetStep(NamedSetNeed::Bind,0,TypeRef(member));
}

extern "C" u32 named_set_member(address cursor, NameBytes expected, NameBytes actual) on cpu {
    NamedSetCursor set=unsafe_position<NamedSetCursor>(cursor);
    if (set.use==NamedSetUse::Enumerate) { return 1; }
    if (set.use==NamedSetUse::Match && set.matched==0 && compare_name(expected,actual)==i32(0)) {
        unsafe_position<NamedSetCursor>(cursor).matched=1;
    }
    return 0;
}
