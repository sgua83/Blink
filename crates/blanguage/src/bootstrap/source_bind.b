module bootstrap.source_bind;

address at(address data, u64 offset) on cpu {
    return unsafe_address(address_bits(data) + offset);
}
u64 element(read StorageView storage, Slice values, u64 index) on cpu {
    return unsafe_position<u64>(at(storage.nodes, values.offset + index * 8u64));
}
u8 text_byte(read StorageView storage, Text text, u64 index) on cpu {
    address data = text.kind == 0 ? storage.source : storage.text;
    return unsafe_position<u8>(at(data, text.range.start + index));
}
bool word(read StorageView storage, Text text, read array<u8,8> name) on cpu {
    if (text.range.end - text.range.start != 8u64) { return false; }
    for (index in 0..8) {
        if (text_byte(storage, text, u64(index)) != name[index]) { return false; }
    }
    return true;
}
bool positional(read StorageView storage, Text name, u64 count) on cpu {
    u64 length = name.range.end - name.range.start;
    if (length <= 8u64 || count == 0u64) { return false; }
    array<u8,8> prefix = "argument";
    for (index in 0..8) {
        if (text_byte(storage, name, u64(index)) != prefix[index]) { return false; }
    }
    if (length > 9u64 && text_byte(storage, name, 8u64) == 48) { return false; }
    // Compare the decimal identity directly against the actual parameter count.
    // This also leaves names beyond the machine index domain as ordinary consts.
    u64 maximum = count - 1u64;
    u64 value = 0u64;
    u64 index = 8u64;
    while (index < length) {
        u8 digit = text_byte(storage, name, index);
        if (digit < 48 || digit > 57) { return false; }
        u64 part = u64(digit) - 48u64;
        if (value > maximum / 10u64 || (value == maximum / 10u64 && part > maximum % 10u64)) { return false; }
        value = value * 10u64 + part;
        index += 1u64;
    }
    return true;
}

bool argument(StorageView storage, u64 offset, u32 checked, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<Optional<u32>,address,ShapeRef> shape,
    cfn<bool,address,TypeRef> check,
    cfn<bool,address,TypeRef> append_type,
    cfn<void,address,u32> append_value) on cpu
    requires nonzero(value) requires nonzero(shape) requires nonzero(check)
    requires nonzero(append_type) requires nonzero(append_value) {
        ArgumentNode node = unsafe_position<ArgumentNode>(at(storage.nodes, offset));
        match (node) {
            ArgumentNode::Type(reference) {
                TypeNode ty = unsafe_position<TypeNode>(at(storage.nodes, reference.offset));
                if (ty.arguments.count == 0u64 && ty.dataset == 0 && ty.view == 0 && ty.tile == 0) {
                    Optional<u32> bound = value(context, ty.name);
                    if (bound.present != 0) { append_value(context, bound.value); return true; }
                }
                if (checked != 0 && !check(context, reference)) { return false; }
                if (!append_type(context, reference)) { return false; }
            }
            ArgumentNode::Value(reference) {
                Optional<u32> bound = shape(context, reference);
                if (bound.present == 0) { return false; }
                append_value(context, bound.value);
            }
        }
    return true;
}

extern "C" bool arguments(StorageView storage, Slice arguments, u32 checked, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<Optional<u32>,address,ShapeRef> shape,
    cfn<void,address,u64> reserve,
    cfn<bool,address,TypeRef> check,
    cfn<bool,address,TypeRef> append_type,
    cfn<void,address,u32> append_value) on cpu
    requires nonzero(value) requires nonzero(shape) requires nonzero(reserve) requires nonzero(check)
    requires nonzero(append_type) requires nonzero(append_value) {
    reserve(context, arguments.count);
    u64 index = 0u64;
    while (index < arguments.count) {
        if (!argument(storage, element(storage, arguments, index), checked, context, value, shape, check, append_type, append_value)) { return false; }
        index += 1u64;
    }
    return true;
}

extern "C" void type_name(SignatureNode source, address context,
    cfn<address,address,Text> actual,
    cfn<Optional<u32>,address,Text> value,
    cfn<Optional<u32>,address,ShapeRef> shape,
    cfn<void,address,u64> reserve,
    cfn<bool,address,TypeRef> check,
    cfn<bool,address,TypeRef> append_type,
    cfn<void,address,u32> append_value,
    cfn<void,address,u64> begin_contracts,
    cfn<void,address,PredicateRef,u64> append_predicate,
    cfn<void,address,Text,u32,Optional<u32>,u32> construct,
    cfn<void,address,address> clone_actual,
    cfn<void,address,u32,u32,u32,Optional<u32>> wrap) on cpu
    requires nonzero(actual) requires nonzero(value) requires nonzero(shape)
    requires nonzero(reserve) requires nonzero(check) requires nonzero(append_type)
    requires nonzero(append_value) requires nonzero(begin_contracts)
    requires nonzero(append_predicate) requires nonzero(construct)
    requires nonzero(clone_actual) requires nonzero(wrap) {
    TypeNode ty = unsafe_position<TypeNode>(at(source.storage.nodes, source.offset));
    // Arguments and shape retain the original evaluation and diagnostic order.
    if (!arguments(source.storage, ty.arguments, 0, context, value, shape, reserve, check, append_type, append_value)) { return; }
    Optional<u32> count = Optional<u32>(0, 0);
    if (ty.count.present != 0) {
        count = shape(context, ty.count.value);
        if (count.present == 0) { return; }
    }
    address bound = actual(context, ty.name);
    if (address_bits(bound) != 0u64) { clone_actual(context, bound); }
    else {
        if (ty.contracts.present != 0) {
            begin_contracts(context, ty.contracts.value.count);
            u64 parameters = ty.arguments.count == 0u64 ? 0u64 : ty.arguments.count - 1u64;
            u64 index = 0u64;
            while (index < ty.contracts.value.count) {
                PredicateRef reference = PredicateRef(element(source.storage, ty.contracts.value, index));
                append_predicate(context, reference, parameters);
                index += 1u64;
            }
        }
        construct(context, ty.name, ty.canonical, count, ty.contracts.present);
    }
    if (ty.dataset != 0 || ty.view != 0 || ty.tile != 0) {
        wrap(context, ty.dataset, ty.view, ty.tile, count);
    }
}

extern "C" void condition(SignatureNode source, Optional<u64> parameters, u32 output, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<void,address,ConditionNode> construct) on cpu
    requires nonzero(value) requires nonzero(construct) {
    ConditionNode node = unsafe_position<ConditionNode>(at(source.storage.nodes, source.offset));
    match (node) {
        ConditionNode::Name(name) {
            bool local = (output != 0 && word(source.storage, name, "returned"))
                || (parameters.present != 0 && positional(source.storage, name, parameters.value));
            if (!local) {
                Optional<u32> bound = value(context, name);
                if (bound.present != 0) { construct(context, ConditionNode::StaticU32(bound.value)); return; }
            }
        }
        ConditionNode::Number(number) {}
        ConditionNode::Bool(boolean) {}
        ConditionNode::Member(member) {}
        ConditionNode::Unary(unary) {}
        ConditionNode::Binary(binary) {}
        ConditionNode::StaticU32(number) {}
    }
    construct(context, node);
}

extern "C" void predicate(SignatureNode source, Optional<u64> parameters, address context,
    cfn<void,address,u64> reserve,
    cfn<void,address,ConditionRef,ConditionRef> append_interval,
    cfn<void,address,PredicateNode,u32> construct) on cpu
    requires nonzero(reserve) requires nonzero(append_interval) requires nonzero(construct) {
    PredicateNode node = unsafe_position<PredicateNode>(at(source.storage.nodes, source.offset));
    u32 output = 0;
    match (node) {
        PredicateNode::Range(range) {
            reserve(context, range.intervals.count);
            u64 index = 0u64;
            while (index < range.intervals.count) {
                IntervalNode interval = unsafe_position<IntervalNode>(at(source.storage.nodes, element(source.storage, range.intervals, index)));
                append_interval(context, interval.lower, interval.upper);
                index += 1u64;
            }
        }
        PredicateNode::Initialized(conditional) { output = 1; }
        PredicateNode::Cleanup(conditional) { output = 1; }
        PredicateNode::Prefix(prefix) {}
        PredicateNode::Nonzero(name) {}
        PredicateNode::Nonempty(name) {}
        PredicateNode::Index(index) {}
        PredicateNode::Contiguous(name) {}
        PredicateNode::SameLength(pair) {}
        PredicateNode::Memory(access) {}
    }
    construct(context, node, output);
}

extern "C" bool runtime_contract(SignatureNode source, address context,
    cfn<Optional<u32>,address,Text> value) on cpu requires nonzero(value) {
    ContractNode contract = unsafe_position<ContractNode>(at(source.storage.nodes, source.offset));
    if (contract.kind == 0) {
        PredicateNode predicate = unsafe_position<PredicateNode>(at(source.storage.nodes, contract.predicate.offset));
        match (predicate) {
            PredicateNode::Nonzero(name) { return value(context, name).present == 0; }
            PredicateNode::Prefix(prefix) {}
            PredicateNode::Nonempty(name) {}
            PredicateNode::Index(index) {}
            PredicateNode::Contiguous(name) {}
            PredicateNode::Initialized(conditional) {}
            PredicateNode::SameLength(pair) {}
            PredicateNode::Cleanup(conditional) {}
            PredicateNode::Memory(access) {}
            PredicateNode::Range(range) {}
        }
    }
    return true;
}
