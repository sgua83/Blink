module bootstrap.fact_key;

// Tagged Rust keys have a u32 tag followed by their C-layout fields.
// Only the selected fields are read; padding does not participate in the order.

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

u64 identity_words(u32 tag) on cpu {
    if (tag == u32(IdentityKind::TileLength)) { return 0u64; }
    if (tag == u32(IdentityKind::DatasetContent)) { return 3u64; }
    if (tag == u32(IdentityKind::ParameterContent) || tag == u32(IdentityKind::ResourceParameter)
        || tag == u32(IdentityKind::ElementParent) || tag == u32(IdentityKind::MemoryValue)
        || tag == u32(IdentityKind::LoopValue) || tag == u32(IdentityKind::TaskCompletion)) {
        return 2u64;
    }
    return 1u64;
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
    i32 first = unsigned_order(unsafe_position<u64>(field(left,8u64)),
        unsafe_position<u64>(field(right,8u64)));
    if (first != 0) { return first; }
    u64 tail = identity_words(a) - 1u64;
    return tail == 0u64 ? i32(0) : words(field(left,16u64),field(right,16u64),tail);
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

// The original native key owns the scalar bytes and both fixed-bit words.
extern "C" DomainCastView inspect_domain_cast(address domain) on cpu {
    DomainKind kind = unsafe_position<DomainKind>(domain);
    if (kind == DomainKind::CommonRange) {
        return DomainCastView(kind, field(domain,41u64), field(domain,42u64), 0u64, 0u64);
    }
    if (kind == DomainKind::KnownBits) {
        return DomainCastView(kind, field(domain,4u64), address(),
            unsafe_position<u64>(field(domain,8u64)), unsafe_position<u64>(field(domain,16u64)));
    }
    return DomainCastView(kind, address(), address(), 0u64, 0u64);
}

extern "C" void write_common_range_cast(address domain, u8 low, u8 extended, u8 target) on cpu {
    unsafe_position<u8>(field(domain,41u64)) = low;
    unsafe_position<u8>(field(domain,42u64)) = extended;
    unsafe_position<u8>(field(domain,43u64)) = target;
}

extern "C" void write_known_bits_cast(address domain, u8 target, u64 mask, u64 value) on cpu {
    unsafe_position<u8>(field(domain,4u64)) = target;
    unsafe_position<u64>(field(domain,8u64)) = mask;
    unsafe_position<u64>(field(domain,16u64)) = value;
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

extern "C" i32 order_fact<const VIEW:u32>(address left,address right) on cpu {
    u32 a = VIEW == 0 ? unsafe_position<u32>(left) : u32(unsafe_position<FactView>(left).kind);
    u32 b = unsafe_position<u32>(right);
    if (a != b) { return tag_order(a,b); }
    address other = field(right,8u64);
    if (a == u32(FactKind::Predicate)) {
        address first = VIEW == 0 ? field(left,8u64) : unsafe_position<FactView>(left).payload;
        i32 predicate = order_predicate(first,other);
        u32 flag = VIEW == 0 ? u32(unsafe_position<u8>(field(left,96u64))) : unsafe_position<FactView>(left).flag;
        return predicate != 0 ? predicate : tag_order(flag,u32(unsafe_position<u8>(field(right,96u64))));
    }
    address first = VIEW == 0 ? field(left,8u64) : unsafe_position<FactView>(left).first;
    i32 identity = order_identity(first,other);
    if (identity != 0) { return identity; }
    if (a == u32(FactKind::NonZero) || a == u32(FactKind::Zero)) { return i32(0); }
    address payload_other = field(right,40u64);
    if (a == u32(FactKind::Index) || a == u32(FactKind::Outside)) {
        address payload = VIEW == 0 ? field(left,40u64) : unsafe_position<FactView>(left).payload;
        return order_domain(payload,payload_other);
    }
    if (a == u32(FactKind::Minimum) || a == u32(FactKind::Maximum)) {
        address scalar_key = VIEW == 0 ? field(left,40u64) : unsafe_position<FactView>(left).scalar;
        i32 scalar = bytes(scalar_key,payload_other,1u64);
        u64 value = VIEW == 0 ? unsafe_position<u64>(field(left,48u64)) : unsafe_position<FactView>(left).value;
        return scalar != 0 ? scalar : unsigned_order(value,unsafe_position<u64>(field(right,48u64)));
    }
    if (a == u32(FactKind::Boolean)) {
        u32 flag = VIEW == 0 ? u32(unsafe_position<u8>(field(left,40u64))) : unsafe_position<FactView>(left).flag;
        return tag_order(flag,u32(unsafe_position<u8>(payload_other)));
    }
    address payload = VIEW == 0 ? field(left,40u64) : unsafe_position<FactView>(left).second;
    i32 value = order_identity(payload,payload_other);
    if (value != 0 || a != u32(FactKind::Below)) { return value; }
    u32 flag = VIEW == 0 ? u32(unsafe_position<u8>(field(left,72u64))) : unsafe_position<FactView>(left).flag;
    i32 direction = tag_order(flag,u32(unsafe_position<u8>(field(right,72u64))));
    address scalar = VIEW == 0 ? field(left,73u64) : unsafe_position<FactView>(left).scalar;
    return direction != 0 ? direction : bytes(scalar,field(right,73u64),1u64);
}

extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";

extern "C" ScalarTag fact_scalar_tag(address scalar) on cpu {
    u32 code = u32(unsafe_position<u8>(scalar));
    return unsafe_position<ScalarTag>(address_of(code));
}

extern "C" FactScalarView inspect_fact_scalar(address scalar) on cpu {
    ScalarTag tag = fact_scalar_tag(scalar);
    bool signed = tag == ScalarTag::I8 || tag == ScalarTag::I16 || tag == ScalarTag::I32 || tag == ScalarTag::I64;
    bool floating = tag == ScalarTag::F32 || tag == ScalarTag::F64;
    u32 kind = floating ? 2 : signed ? 1 : tag == ScalarTag::Bool ? 3 : 0;
    u32 width = 32;
    if (tag == ScalarTag::U8 || tag == ScalarTag::I8) { width = 8; }
    else if (tag == ScalarTag::U16 || tag == ScalarTag::I16) { width = 16; }
    else if (tag == ScalarTag::U64 || tag == ScalarTag::I64 || tag == ScalarTag::F64) { width = 64; }
    u64 mask = 18446744073709551615u64 >> u64(64 - width);
    u64 bias = signed ? 1u64 << u64(width - 1) : 0u64;
    return FactScalarView(kind,width,mask,bias);
}

extern "C" FactView read_fact_fields(address source) on cpu {
    FactKind kind = unsafe_position<FactKind>(source);
    FactView view = FactView(1,kind,address(),address(),address(),address(),0u64,0u64,0,0);
    address first = field(source,8u64);
    address tail = field(source,40u64);
    if (kind == FactKind::Predicate) {
        view.payload = first;
        view.flag = u32(unsafe_position<u8>(field(source,96u64)));
        return view;
    }

    view.first = first;
    if (kind == FactKind::Equal || kind == FactKind::NotEqual || kind == FactKind::Below) {
        view.second = tail;
        if (kind == FactKind::Below) {
            view.flag = u32(unsafe_position<u8>(field(source,72u64)));
            view.scalar = field(source,73u64);
        }
    } else if (kind == FactKind::Index || kind == FactKind::Outside) {
        view.payload = tail;
    } else if (kind == FactKind::Minimum || kind == FactKind::Maximum) {
        view.scalar = tail;
        view.value = unsafe_position<u64>(field(source,48u64));
    } else if (kind == FactKind::Boolean) {
        view.flag = u32(unsafe_position<u8>(tail));
    }
    return view;
}

extern "C" FactView inspect_fact(address source) on cpu {
    FactView view = read_fact_fields(source);
    if (view.kind == FactKind::Minimum || view.kind == FactKind::Maximum) {
        view.mask = inspect_fact_scalar(view.scalar).mask;
    } else if (view.kind == FactKind::Index && unsafe_position<DomainKind>(view.payload) == DomainKind::Fixed) {
        view.fixed = 1;
        view.value = u64(unsafe_position<u32>(field(view.payload,4u64)));
    }
    return view;
}

extern "C" FactView fact_bound(address identity, address scalar, u64 value, u32 minimum) on cpu {
    FactKind kind = minimum != 0 ? FactKind::Minimum : FactKind::Maximum;
    return FactView(1,kind,identity,address(),address(),scalar,value,inspect_fact_scalar(scalar).mask,0,0);
}

extern "C" void materialize_fact(FactView source, address output) on cpu {
    address first = field(output,8u64);
    address tail = field(output,40u64);
    u8 truth = u8(source.flag != 0);
    if (source.kind == FactKind::Predicate) {
        RtlMoveMemory(first,source.payload,88u64);
        unsafe_position<u8>(field(output,96u64)) = truth;
    } else {
        RtlMoveMemory(first,source.first,32u64);
        if (source.kind == FactKind::Equal || source.kind == FactKind::NotEqual || source.kind == FactKind::Below) {
            RtlMoveMemory(tail,source.second,32u64);
            if (source.kind == FactKind::Below) {
                unsafe_position<u8>(field(output,72u64)) = truth;
                unsafe_position<u8>(field(output,73u64)) = unsafe_position<u8>(source.scalar);
            }
        } else if (source.kind == FactKind::Index || source.kind == FactKind::Outside) {
            RtlMoveMemory(tail,source.payload,104u64);
        } else if (source.kind == FactKind::Minimum || source.kind == FactKind::Maximum) {
            unsafe_position<u8>(tail) = unsafe_position<u8>(source.scalar);
            unsafe_position<u64>(field(output,48u64)) = source.value;
        } else if (source.kind == FactKind::Boolean) {
            unsafe_position<u8>(tail) = truth;
        }
    }
    unsafe_position<FactKind>(output) = source.kind;
}

extern "C" NumericIdentity capture_key_identity(address source) on cpu {
    IdentityKind kind = unsafe_position<IdentityKind>(source);
    if (kind == IdentityKind::TaskWorkCompletion) {
        return NumericIdentity(kind,u64(unsafe_position<u32>(field(source,4u64))),
            u64(unsafe_position<u32>(field(source,8u64))),0u64);
    }

    NumericIdentity value = NumericIdentity(kind,0u64,0u64,0u64);
    u64 count = identity_words(u32(kind));
    if (count >= 1u64) { value.first = unsafe_position<u64>(field(source,8u64)); }
    if (count >= 2u64) { value.second = unsafe_position<u64>(field(source,16u64)); }
    if (count >= 3u64) { value.third = unsafe_position<u64>(field(source,24u64)); }
    return value;
}

IdentityKey make_key_identity(NumericIdentity source) on cpu {
    if (source.kind == IdentityKind::TaskWorkCompletion) {
        return IdentityKey(source.kind,u32(source.first),u64(u32(source.second)),0u64,0u64);
    }
    return IdentityKey(source.kind,0,source.first,source.second,source.third);
}

extern "C" NumericOperand capture_key_operand(address source) on cpu {
    OperandKind kind = unsafe_position<OperandKind>(source);
    if (kind == OperandKind::Value) {
        return NumericOperand(kind,capture_key_identity(field(source,8u64)),ScalarTag::F32,0u64);
    }
    return NumericOperand(kind,NumericIdentity(IdentityKind::Register,0u64,0u64,0u64),
        fact_scalar_tag(field(source,4u64)),unsafe_position<u64>(field(source,8u64)));
}

OperandKey make_key_operand(NumericOperand source) on cpu {
    if (source.kind == OperandKind::Value) {
        IdentityKey identity = make_key_identity(source.identity);
        u64 header = u64(u32(identity.kind)) | (u64(identity.packed) << 32u64);
        return OperandKey(source.kind,u8(0),header,identity.first,identity.second,identity.third);
    }
    return OperandKey(source.kind,u8(u32(source.scalar)),source.bits,0u64,0u64,0u64);
}

extern "C" NumericPredicate capture_key_predicate(address source) on cpu {
    u32 code = u32(unsafe_position<u8>(source));
    return NumericPredicate(unsafe_position<ComparisonKind>(address_of(code)),fact_scalar_tag(field(source,1u64)),
        capture_key_operand(field(source,8u64)),capture_key_operand(field(source,48u64)));
}

PredicateKey make_key_predicate(NumericPredicate source) on cpu {
    return PredicateKey(u8(u32(source.operation)),u8(u32(source.scalar)),
        make_key_operand(source.left),make_key_operand(source.right));
}

