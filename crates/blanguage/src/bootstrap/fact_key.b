module bootstrap.fact_key;

// Tagged Rust keys have a u32 tag followed by their C-layout fields.
// Only the selected fields are read; padding does not participate in the order.
enum IdentityKind : u32 {
    Register=0; DataLength=1; TileLength=2; ParameterContent=3; ResourceParameter=4;
    ParameterStorage=5; ElementComponent=6; ElementParent=7; ElementIndex=8;
    DatasetContent=9; MemoryValue=10; LoopValue=11; TaskResult=12; TaskCompletion=13;
    TaskWorkCompletion=14; TaskControl=15; TaskReturnSource=16;
};
enum DomainKind : u32 {
    Data=0; Fixed=1; Tile=2; DataLength=3; TileLength=4; View=5; ViewLength=6;
    ApplicationData=7; ApplicationLength=8; Upper=9; Lower=10; Room=11; Sum=12; Offset=13;
    Shifted=14; Computed=15; CommonRange=16; KnownBits=17; Floating=18;
};
enum KeyKind : u32 {
    Equal=0; Index=1; Minimum=2; Maximum=3; NonZero=4; Boolean=5; Below=6; Outside=7;
    Zero=8; NotEqual=9; Predicate=10;
};

address field(address key, u64 at) on cpu {
    return unsafe_address(address_bits(key) + at);
}

i32 unsigned_order(u64 left, u64 right) on cpu {
    return i32(left > right) - i32(left < right);
}

i32 tag_order(u32 left, u32 right) on cpu {
    return i32(left > right) - i32(left < right);
}

i32 words(address left, address right, u64 count) on cpu {
    u64 i = 0u64;
    while (i < count) {
        u64 offset = i * size_of<u64>();
        i32 order = unsigned_order(unsafe_position<u64>(field(left,offset)),
            unsafe_position<u64>(field(right,offset)));
        if (order != 0) { return order; }
        i += 1u64;
    }
    return i32(0);
}

i32 bytes(address left, address right, u64 count) on cpu {
    u64 i = 0u64;
    while (i < count) {
        u8 a = unsafe_position<u8>(field(left,i));
        u8 b = unsafe_position<u8>(field(right,i));
        if (a != b) { return i32(a > b) - i32(a < b); }
        i += 1u64;
    }
    return i32(0);
}

i32 word32(address left, address right, u64 at) on cpu {
    return tag_order(unsafe_position<u32>(field(left,at)),unsafe_position<u32>(field(right,at)));
}

i32 signed_wide(address left, address right) on cpu {
    i64 a = unsafe_position<i64>(field(left,8u64));
    i64 b = unsafe_position<i64>(field(right,8u64));
    if (a != b) { return i32(a > b) - i32(a < b); }
    return words(left,right,1u64);
}

extern "C" i32 order_identity(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    if (a == u32(IdentityKind::TileLength)) { return i32(0); }
    if (a == u32(IdentityKind::TaskWorkCompletion)) {
        i32 first = word32(left,right,4u64);
        return first != 0 ? first : word32(left,right,8u64);
    }
    u64 count = 1u64;
    if (a == u32(IdentityKind::DatasetContent)) {
        count = 3u64;
    } else if (a == u32(IdentityKind::ParameterContent) || a == u32(IdentityKind::ResourceParameter)
        || a == u32(IdentityKind::ElementParent) || a == u32(IdentityKind::MemoryValue)
        || a == u32(IdentityKind::LoopValue) || a == u32(IdentityKind::TaskCompletion)) {
        count = 2u64;
    }
    return words(field(left,8u64),field(right,8u64),count);
}

i32 identities(address left, address right, u64 count) on cpu {
    u64 i = 0u64;
    while (i < count) {
        u64 offset = i * 32u64;
        i32 order = order_identity(field(left,offset),field(right,offset));
        if (order != 0) { return order; }
        i += 1u64;
    }
    return i32(0);
}

extern "C" i32 order_operand(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    if (a == 0) { return order_identity(field(left,8u64),field(right,8u64)); }
    i32 scalar = bytes(field(left,4u64),field(right,4u64),1u64);
    return scalar != 0 ? scalar : words(field(left,8u64),field(right,8u64),1u64);
}

extern "C" i32 order_selector(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    // Direct or Prefix contains one Operand, and Affine begins with its base.
    if (a != 1) { return order_operand(field(left,8u64),field(right,8u64)); }
    i32 base = order_identity(field(left,8u64),field(right,8u64));
    if (base != 0) { return base; }
    i32 scale = signed_wide(field(left,48u64),field(right,48u64));
    return scale != 0 ? scale : signed_wide(field(left,64u64),field(right,64u64));
}

i32 operands(address left, address right) on cpu {
    i32 first = order_operand(left,right);
    return first != 0 ? first : order_operand(field(left,40u64),field(right,40u64));
}

extern "C" i32 order_predicate(address left, address right) on cpu {
    i32 operation_scalar = bytes(left,right,2u64);
    return operation_scalar != 0 ? operation_scalar : operands(field(left,8u64),field(right,8u64));
}

extern "C" i32 order_expression(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    if (a != 0) { return order_selector(field(left,8u64),field(right,8u64)); }
    i32 operation_scalar = bytes(field(left,4u64),field(right,4u64),2u64);
    return operation_scalar != 0 ? operation_scalar : operands(field(left,8u64),field(right,8u64));
}

extern "C" i32 order_floating(address left, address right) on cpu {
    i32 source = order_identity(left,right);
    if (source != 0) { return source; }
    i32 bounds = words(field(left,32u64),field(right,32u64),5u64);
    return bounds != 0 ? bounds : bytes(field(left,72u64),field(right,72u64),2u64);
}

extern "C" i32 order_domain(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    address first = field(left,8u64);
    address second = field(right,8u64);
    if (a == u32(DomainKind::Data) || a == u32(DomainKind::DataLength)) {
        return words(first,second,1u64);
    }
    if (a == u32(DomainKind::Fixed)) { return word32(left,right,4u64); }
    if (a == u32(DomainKind::Tile) || a == u32(DomainKind::TileLength)) { return i32(0); }
    if (a == u32(DomainKind::Computed)) { return order_expression(first,second); }
    if (a == u32(DomainKind::KnownBits)) {
        i32 kind = bytes(field(left,4u64),field(right,4u64),1u64);
        return kind != 0 ? kind : words(first,second,2u64);
    }
    if (a == u32(DomainKind::Floating)) { return order_floating(first,second); }
    if (a == u32(DomainKind::View) || a == u32(DomainKind::ViewLength)) {
        return identities(first,second,3u64);
    }
    if (a == u32(DomainKind::Sum) || a == u32(DomainKind::Shifted)) {
        i32 pair = identities(first,second,2u64);
        if (pair != 0 || a == u32(DomainKind::Sum)) { return pair; }
        return word32(left,right,72u64);
    }
    i32 key = order_identity(first,second);
    if (key != 0) { return key; }
    if (a == u32(DomainKind::Upper) || a == u32(DomainKind::Lower) || a == u32(DomainKind::Offset)) {
        return word32(left,right,40u64);
    }
    if (a == u32(DomainKind::CommonRange)) {
        return bytes(field(left,40u64),field(right,40u64),4u64);
    }
    return i32(0);
}

extern "C" i32 order_fact(address left, address right) on cpu {
    u32 a = unsafe_position<u32>(left);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    address first = field(left,8u64);
    address other = field(right,8u64);
    if (a == u32(KeyKind::Predicate)) {
        i32 predicate = order_predicate(first,other);
        return predicate != 0 ? predicate : bytes(field(left,96u64),field(right,96u64),1u64);
    }
    i32 identity = order_identity(first,other);
    if (identity != 0) { return identity; }
    if (a == u32(KeyKind::NonZero) || a == u32(KeyKind::Zero)) { return i32(0); }
    address payload = field(left,40u64);
    address payload_other = field(right,40u64);
    if (a == u32(KeyKind::Index) || a == u32(KeyKind::Outside)) {
        return order_domain(payload,payload_other);
    }
    if (a == u32(KeyKind::Minimum) || a == u32(KeyKind::Maximum)) {
        i32 scalar = bytes(payload,payload_other,1u64);
        return scalar != 0 ? scalar : words(field(left,48u64),field(right,48u64),1u64);
    }
    if (a == u32(KeyKind::Boolean)) { return bytes(payload,payload_other,1u64); }
    i32 value = order_identity(payload,payload_other);
    if (value != 0 || a != u32(KeyKind::Below)) { return value; }
    return bytes(field(left,72u64),field(right,72u64),2u64);
}
