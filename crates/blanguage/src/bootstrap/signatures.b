module bootstrap.signatures;
import bootstrap.ast_storage;
import bootstrap.names;

T read_node<T>(StorageView store, u64 offset) on cpu {
    return unsafe_position<T>(unsafe_address(address_bits(store.nodes) + offset));
}

u64 child_at(StorageView store, Slice list, u64 index) on cpu {
    return read_node<u64>(store, list.offset + index * size_of<u64>());
}

NameBytes text_bytes(StorageView store, Text value) on cpu {
    address bytes = store.source;
    if (value.kind != 0) { bytes = store.text; }
    return NameBytes(unsafe_address(address_bits(bytes) + value.range.start), value.range.end - value.range.start);
}

i32 text_order(StorageView left, Text a, StorageView right, Text b) on cpu {
    return compare_name(text_bytes(left, a), text_bytes(right, b));
}

i32 integer_order(u64 a, u64 b) on cpu {
    if (a < b) { return i32(-1); }
    if (a > b) { return i32(1); }
    return i32(0);
}

// These are the original semantic variant ranks and the payloads used by comparison.
struct ConditionKey { u32 kind; Text text; u64 first; u64 second; };

ConditionKey condition_key(ConditionNode node) on cpu {
    Text empty = Text(0, Span(0u64, 0u64));
    match (node) {
        ConditionNode::Name(value) { return ConditionKey(0, value, 0u64, 0u64); }
        ConditionNode::Number(value) { return ConditionKey(1, value, 0u64, 0u64); }
        ConditionNode::Bool(value) { return ConditionKey(2, empty, u64(value != 0), 0u64); }
        ConditionNode::Member(value) { return ConditionKey(3, value.field, value.base.offset, 0u64); }
        ConditionNode::Unary(value) { return ConditionKey(4, value.operation, value.value.offset, 0u64); }
        ConditionNode::Binary(value) { return ConditionKey(5, value.operation, value.left.offset, value.right.offset); }
        ConditionNode::StaticU32(value) { return ConditionKey(6, empty, u64(value), 0u64); }
    }
}

extern "C" i32 compare_condition(SignatureNode left, SignatureNode right) on cpu {
    ConditionKey a = condition_key(read_node<ConditionNode>(left.storage, left.offset));
    ConditionKey b = condition_key(read_node<ConditionNode>(right.storage, right.offset));
    i32 kind = integer_order(u64(a.kind), u64(b.kind));
    if (kind != i32(0)) { return kind; }
    if (a.kind == 3) {
        i32 base = compare_condition(SignatureNode(left.storage, a.first), SignatureNode(right.storage, b.first));
        if (base != i32(0)) { return base; }
        return text_order(left.storage, a.text, right.storage, b.text);
    }
    i32 text = text_order(left.storage, a.text, right.storage, b.text);
    if (text != i32(0)) { return text; }
    if (a.kind == 4 || a.kind == 5) {
        i32 first = compare_condition(SignatureNode(left.storage, a.first), SignatureNode(right.storage, b.first));
        if (first != i32(0) || a.kind == 4) { return first; }
        return compare_condition(SignatureNode(left.storage, a.second), SignatureNode(right.storage, b.second));
    }
    return integer_order(a.first, b.first);
}

struct PredicateKey { u32 kind; Text first; Text second; Text third; Slice intervals; u64 value; };

PredicateKey predicate_key(PredicateNode node) on cpu {
    Text empty = Text(0, Span(0u64, 0u64));
    Slice none = Slice(0u64, 0u64);
    match (node) {
        PredicateNode::Prefix(v) { return PredicateKey(0, v.data, v.count, v.field, none, 0u64); }
        PredicateNode::Nonzero(v) { return PredicateKey(1, v, empty, empty, none, 0u64); }
        PredicateNode::Nonempty(v) { return PredicateKey(2, v, empty, empty, none, 0u64); }
        PredicateNode::Contiguous(v) { return PredicateKey(3, v, empty, empty, none, 0u64); }
        PredicateNode::Memory(v) {
            u64 access = 0u64;
            if (v.present != 0) { access = u64(v.value) + 1u64; }
            return PredicateKey(4, empty, empty, empty, none, access);
        }
        PredicateNode::Index(v) { return PredicateKey(5, v.value, v.resource, empty, none, 0u64); }
        PredicateNode::Range(v) { return PredicateKey(6, v.value, empty, empty, v.intervals, 0u64); }
        PredicateNode::SameLength(v) { return PredicateKey(7, v.left, v.right, empty, none, 0u64); }
        PredicateNode::Initialized(v) { return PredicateKey(8, v.value, empty, empty, none, v.condition.offset); }
        PredicateNode::Cleanup(v) { return PredicateKey(9, v.value, empty, empty, none, v.condition.offset); }
    }
}

i32 interval_order(StorageView left, Slice a, StorageView right, Slice b) on cpu {
    u64 index = 0u64;
    while (index < a.count && index < b.count) {
        IntervalNode first = read_node<IntervalNode>(left, child_at(left, a, index));
        IntervalNode second = read_node<IntervalNode>(right, child_at(right, b, index));
        i32 lower = compare_condition(SignatureNode(left, first.lower.offset), SignatureNode(right, second.lower.offset));
        if (lower != i32(0)) { return lower; }
        i32 upper = compare_condition(SignatureNode(left, first.upper.offset), SignatureNode(right, second.upper.offset));
        if (upper != i32(0)) { return upper; }
        index += 1u64;
    }
    return integer_order(a.count, b.count);
}

extern "C" i32 compare_predicate(SignatureNode left, SignatureNode right) on cpu {
    PredicateKey a = predicate_key(read_node<PredicateNode>(left.storage, left.offset));
    PredicateKey b = predicate_key(read_node<PredicateNode>(right.storage, right.offset));
    i32 kind = integer_order(u64(a.kind), u64(b.kind));
    if (kind != i32(0)) { return kind; }
    i32 first = text_order(left.storage, a.first, right.storage, b.first);
    if (first != i32(0)) { return first; }
    i32 second = text_order(left.storage, a.second, right.storage, b.second);
    if (second != i32(0)) { return second; }
    i32 third = text_order(left.storage, a.third, right.storage, b.third);
    if (third != i32(0)) { return third; }
    if (a.kind == 6) { return interval_order(left.storage, a.intervals, right.storage, b.intervals); }
    if (a.kind == 8 || a.kind == 9) {
        return compare_condition(SignatureNode(left.storage, a.value), SignatureNode(right.storage, b.value));
    }
    return integer_order(a.value, b.value);
}

i32 contract_order(StorageView left, u64 a, StorageView right, u64 b) on cpu {
    ContractNode first = read_node<ContractNode>(left, a);
    ContractNode second = read_node<ContractNode>(right, b);
    i32 kind = integer_order(u64(first.kind), u64(second.kind));
    if (kind != i32(0)) { return kind; }
    return compare_predicate(SignatureNode(left, first.predicate.offset), SignatureNode(right, second.predicate.offset));
}

extern "C" bool contract_before(address first, address second, address context) on cpu {
    StorageView store = unsafe_position<StorageView>(context);
    return contract_order(store, unsafe_position<u64>(first), store, unsafe_position<u64>(second)) < i32(0);
}

address offset_slot(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<u64>());
}

u64 order_contracts(StorageView store, Slice contracts, address rows) on cpu {
    u64 index = 0u64;
    while (index < contracts.count) {
        unsafe_position<u64>(offset_slot(rows, index)) = child_at(store, contracts, index);
        index += 1u64;
    }
    sort_rows<u64>(rows, contracts.count, address_of(store), c_function(contract_before));
    u64 unique = 0u64;
    u64 previous = 0u64;
    index = 0u64;
    while (index < contracts.count) {
        u64 current = unsafe_position<u64>(offset_slot(rows, index));
        if (unique == 0u64 || contract_order(store, previous, store, current) != i32(0)) {
            unsafe_position<u64>(offset_slot(rows, unique)) = current;
            previous = current;
            unique += 1u64;
        }
        index += 1u64;
    }
    return unique;
}

struct FamilyKey { u32 kind; Text name; };

FamilyKey family_key(TypeFamilyNode family) on cpu {
    Text empty = Text(0, Span(0u64, 0u64));
    match (family) {
        TypeFamilyNode::Numeric() { return FamilyKey(0, empty); }
        TypeFamilyNode::Floating() { return FamilyKey(1, empty); }
        TypeFamilyNode::Integer() { return FamilyKey(2, empty); }
        TypeFamilyNode::ScalarNumeric() { return FamilyKey(3, empty); }
        TypeFamilyNode::ScalarInteger() { return FamilyKey(4, empty); }
        TypeFamilyNode::Named(name) { return FamilyKey(5, name); }
    }
}

bool same_type_parameter(StorageView left, TypeParameterNode a, StorageView right, TypeParameterNode b) on cpu {
    if (text_order(left, a.name, right, b.name) != i32(0) || a.family.present != b.family.present) { return false; }
    if (a.family.present == 0) { return true; }
    FamilyKey first = family_key(read_node<TypeFamilyNode>(left, a.family.value.offset));
    FamilyKey second = family_key(read_node<TypeFamilyNode>(right, b.family.value.offset));
    return first.kind == second.kind && text_order(left, first.name, right, second.name) == i32(0);
}

bool same_value_parameter(StorageView left, ValueParameterNode a, StorageView right, ValueParameterNode b) on cpu {
    if (text_order(left, a.name, right, b.name) != i32(0) || a.choices.present != b.choices.present) { return false; }
    if (a.choices.present == 0) { return true; }
    if (a.choices.value.count != b.choices.value.count) { return false; }
    u64 index = 0u64;
    while (index < a.choices.value.count) {
        if (read_node<u32>(left, a.choices.value.offset + index * size_of<u32>())
            != read_node<u32>(right, b.choices.value.offset + index * size_of<u32>())) { return false; }
        index += 1u64;
    }
    return true;
}

extern "C" bool same_signature(SignatureNode left, SignatureNode right,
    address left_contracts, address right_contracts) on cpu {
    StorageView a_store = left.storage;
    StorageView b_store = right.storage;
    FunctionNode a = read_node<FunctionNode>(a_store, left.offset);
    FunctionNode b = read_node<FunctionNode>(b_store, right.offset);
    if (a.kind != b.kind || a.c_abi != b.c_abi || a.target != b.target
        || a.external.present != b.external.present || a.types.count != b.types.count
        || a.values.count != b.values.count || a.params.count != b.params.count) { return false; }
    if (a.external.present != 0) {
        ExternalNode first = read_node<ExternalNode>(a_store, a.external.value.offset);
        ExternalNode second = read_node<ExternalNode>(b_store, b.external.value.offset);
        if (text_order(a_store, first.library, b_store, second.library) != i32(0)
            || text_order(a_store, first.symbol, b_store, second.symbol) != i32(0)) { return false; }
    }
    u64 index = 0u64;
    while (index < a.types.count) {
        TypeParameterNode first = read_node<TypeParameterNode>(a_store, child_at(a_store, a.types, index));
        TypeParameterNode second = read_node<TypeParameterNode>(b_store, child_at(b_store, b.types, index));
        if (!same_type_parameter(a_store, first, b_store, second)) { return false; }
        index += 1u64;
    }
    index = 0u64;
    while (index < a.values.count) {
        ValueParameterNode first = read_node<ValueParameterNode>(a_store, child_at(a_store, a.values, index));
        ValueParameterNode second = read_node<ValueParameterNode>(b_store, child_at(b_store, b.values, index));
        if (!same_value_parameter(a_store, first, b_store, second)) { return false; }
        index += 1u64;
    }
    index = 0u64;
    while (index < a.params.count) {
        ParamNode first = read_node<ParamNode>(a_store, child_at(a_store, a.params, index));
        ParamNode second = read_node<ParamNode>(b_store, child_at(b_store, b.params, index));
        if (first.access != second.access || text_order(a_store, first.name, b_store, second.name) != i32(0)) { return false; }
        index += 1u64;
    }
    u64 left_count = order_contracts(a_store, a.contracts, left_contracts);
    u64 right_count = order_contracts(b_store, b.contracts, right_contracts);
    if (left_count != right_count) { return false; }
    index = 0u64;
    while (index < left_count) {
        u64 first = unsafe_position<u64>(offset_slot(left_contracts, index));
        u64 second = unsafe_position<u64>(offset_slot(right_contracts, index));
        if (contract_order(a_store, first, b_store, second) != i32(0)) { return false; }
        index += 1u64;
    }
    return true;
}

extern "C" RecordMember record_member(SignatureNode record, address bytes, u64 length) on cpu {
    StorageView store = record.storage;
    RecordNode declaration = read_node<RecordNode>(store, record.offset);
    NameBytes member = NameBytes(bytes, length);

    u64 index = 0u64;
    while (index < declaration.variants.count) {
        VariantNode variant = read_node<VariantNode>(store, child_at(store, declaration.variants, index));
        if (compare_name(text_bytes(store, variant.name), member) == i32(0)) {
            return RecordMember::Variant();
        }
        index += 1u64;
    }
    index = 0u64;
    while (index < declaration.fields.count) {
        FieldNode field = read_node<FieldNode>(store, child_at(store, declaration.fields, index));
        if (compare_name(text_bytes(store, field.name), member) == i32(0)) {
            return RecordMember::Field(field.ty);
        }
        index += 1u64;
    }
    return RecordMember::Missing();
}

u32 cfn_access(StorageView store, TypeNode formal) on cpu {
    if (formal.dataset != 0 || formal.view != 0 || formal.tile != 0) { return 0; }
    NameBytes name = text_bytes(store, formal.name);
    array<u8,4> read_name = "read";
    if (compare_name(name, NameBytes(address_of(read_name[0]), 4u64)) == i32(0)) { return 1; }
    array<u8,5> write_name = "write";
    if (compare_name(name, NameBytes(address_of(write_name[0]), 5u64)) == i32(0)) { return 2; }
    array<u8,5> inout_name = "inout";
    if (compare_name(name, NameBytes(address_of(inout_name[0]), 5u64)) == i32(0)) { return 3; }
    return 0;
}

extern "C" CfnParameter cfn_parameter(SignatureNode formal) on cpu {
    TypeNode declared = read_node<TypeNode>(formal.storage, formal.offset);
    u32 access = cfn_access(formal.storage, declared);
    if (access == 0) { return CfnParameter(access, Optional<TypeRef>(1, TypeRef(formal.offset))); }

    Optional<TypeRef> child = Optional<TypeRef>(0, TypeRef(0u64));
    if (declared.arguments.count == 1u64) {
        match (read_node<ArgumentNode>(formal.storage, child_at(formal.storage, declared.arguments, 0u64))) {
            ArgumentNode::Type(type) { child = Optional<TypeRef>(1, type); }
            ArgumentNode::Value(shape) {}
        }
    }
    return CfnParameter(access, child);
}

bool type_contains_name(StorageView store, u64 offset, NameBytes name) on cpu {
    TypeNode type = read_node<TypeNode>(store, offset);
    if (compare_name(text_bytes(store, type.name), name) == i32(0)) { return true; }
    u64 index = 0u64;
    while (index < type.arguments.count) {
        ArgumentNode argument = read_node<ArgumentNode>(store, child_at(store, type.arguments, index));
        match (argument) {
            ArgumentNode::Type(child) {
                if (type_contains_name(store, child.offset, name)) { return true; }
            }
            ArgumentNode::Value(shape) {}
        }
        index += 1u64;
    }
    return false;
}

extern "C" bool type_mentions(SignatureNode type, address bytes, u64 length) on cpu {
    return type_contains_name(type.storage, type.offset, NameBytes(bytes, length));
}

bool shape_is_name(StorageView store, ShapeRef shape, NameBytes name) on cpu {
    match (read_node<ShapeNode>(store, shape.offset)) {
        ShapeNode::Constant(text) { return compare_name(text_bytes(store, text), name) == i32(0); }
        ShapeNode::Literal(value) { return false; }
        ShapeNode::Expression(expr) { return false; }
    }
}

bool value_argument_is_name(StorageView store, u64 offset, NameBytes name) on cpu {
    match (read_node<ArgumentNode>(store, offset)) {
        ArgumentNode::Type(child) {
            TypeNode type = read_node<TypeNode>(store, child.offset);
            return type.arguments.count == 0u64 && type.dataset == 0 && type.view == 0 && type.tile == 0
                && compare_name(text_bytes(store, type.name), name) == i32(0);
        }
        ArgumentNode::Value(shape) { return shape_is_name(store, shape, name); }
    }
}

bool resource_names_shape(SignatureNode type, NameBytes name) on cpu {
    StorageView store = type.storage;
    TypeNode outer = read_node<TypeNode>(store, type.offset);
    if (outer.dataset != 0) {
        return outer.count.present != 0 && shape_is_name(store, outer.count.value, name);
    }
    if (outer.view != 0 || outer.tile != 0 || outer.arguments.count != 2u64) { return false; }
    array<u8,5> array_name = "array";
    if (compare_name(text_bytes(store, outer.name), NameBytes(address_of(array_name[0]), 5u64)) != i32(0)) {
        return false;
    }
    match (read_node<ArgumentNode>(store, child_at(store, outer.arguments, 0u64))) {
        ArgumentNode::Type(child) {
            TypeNode element = read_node<TypeNode>(store, child.offset);
            if (element.dataset != 0 || element.view != 0 || element.tile != 0) { return false; }
            NameBytes element_name = text_bytes(store, element.name);
            array<u8,9> texture_2d = "texture2d";
            array<u8,9> texture_3d = "texture3d";
            if (compare_name(element_name, NameBytes(address_of(texture_2d[0]), 9u64)) != i32(0)
                && compare_name(element_name, NameBytes(address_of(texture_3d[0]), 9u64)) != i32(0)) {
                return false;
            }
            return value_argument_is_name(store, child_at(store, outer.arguments, 1u64), name);
        }
        ArgumentNode::Value(shape) { return false; }
    }
}

bool merge_const(u32 value, inout Optional<u32> inferred) on cpu {
    if (inferred.present != 0) { return inferred.value == value; }
    inferred = Optional<u32>(1, value);
    return true;
}

extern "C" bool infer_const_count(SignatureNode type, address name, u64 length, u32 value,
    inout Optional<u32> inferred) on cpu {
    if (!resource_names_shape(type, NameBytes(name, length))) { return true; }
    return merge_const(value, inferred);
}

extern "C" Optional<u32> infer_const_arguments(SignatureNode type, address name, u64 length,
    u64 first, address values, u64 count, inout Optional<u32> inferred) on cpu {
    StorageView store = type.storage;
    Slice arguments = read_node<TypeNode>(store, type.offset).arguments;
    NameBytes parameter = NameBytes(name, length);
    u64 index = 0u64;
    while (index < count) {
        if (value_argument_is_name(store, child_at(store, arguments, first + index), parameter)) {
            address unit = unsafe_address(address_bits(values) + index * size_of<u32>());
            u32 value = unsafe_position<u32>(unit);
            if (!merge_const(value, inferred)) { return Optional<u32>(1, value); }
        }
        index += 1u64;
    }
    return Optional<u32>(0, 0);
}
