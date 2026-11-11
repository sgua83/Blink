module bootstrap.type_families;

const u32 FLOATING = 1;
const u32 INTEGER = 2;
const u32 SCALAR = 4;

struct Member {
    BuiltinName name;
    u32 traits;
};

Member member<const N:u32 in {2,3,4,5,6}>(read array<u8,N> name, u32 traits) on cpu {
    array<u8,6> bytes = array<u8,6>(u8(0),u8(0),u8(0),u8(0),u8(0),u8(0));
    for (index in 0..N) { bytes[index] = name[index]; }
    return Member(BuiltinName(bytes,N),traits);
}

extern "C" u32 builtin_members(read TypeFamilyNode family, address output) on cpu {
    u32 required;
    match (family) {
        TypeFamilyNode::Numeric() { required = 0; }
        TypeFamilyNode::Floating() { required = FLOATING; }
        TypeFamilyNode::Integer() { required = INTEGER; }
        TypeFamilyNode::ScalarNumeric() { required = SCALAR; }
        TypeFamilyNode::ScalarInteger() { required = SCALAR | INTEGER; }
        TypeFamilyNode::Named(name) { return 0; }
    }

    array<Member,19> members = array<Member,19>(
        member("f32",FLOATING|SCALAR),
        member("float2",FLOATING),member("float3",FLOATING),member("float4",FLOATING),
        member("u32",INTEGER|SCALAR),
        member("uint2",INTEGER),member("uint3",INTEGER),member("uint4",INTEGER),
        member("i32",INTEGER|SCALAR),
        member("int2",INTEGER),member("int3",INTEGER),member("int4",INTEGER),
        member("u64",INTEGER|SCALAR),member("i64",INTEGER|SCALAR),
        member("f64",FLOATING|SCALAR),
        member("u8",INTEGER|SCALAR),member("i8",INTEGER|SCALAR),
        member("u16",INTEGER|SCALAR),member("i16",INTEGER|SCALAR)
    );
    u32 count = 0;
    for (index in 0..members.length) {
        if ((members[index].traits & required) == required) {
            address slot = unsafe_address(address_bits(output)+u64(count)*size_of<BuiltinName>());
            unsafe_position<BuiltinName>(slot) = members[index].name;
            count += 1;
        }
    }
    return count;
}
