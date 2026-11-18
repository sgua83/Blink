module bootstrap.source_bind;

address at(address data, u64 offset) on cpu {
    return unsafe_address(address_bits(data) + offset);
}
u64 element(read StorageView storage, Slice values, u64 index) on cpu {
    return unsafe_position<u64>(at(storage.nodes, values.offset + index * 8u64));
}
bool word(address bytes, read array<u8,8> expected) on cpu {
    for (index in 0..8) {
        if (unsafe_position<u8>(at(bytes,u64(index))) != expected[index]) { return false; }
    }
    return true;
}
extern "C" u64 positional_index(address bytes,u64 length) on cpu {
    u64 missing=18446744073709551615u64;
    if (length<=8u64 || !word(bytes,"argument")) { return missing; }
    address suffix=at(bytes,8u64);
    u8 first=unsafe_position<u8>(suffix);
    if (first<48 || first>57 || (first==48 && length>9u64)) { return missing; }
    ParsedNumber ordinal=decimal_number(suffix,length-8u64,missing);
    return ordinal.error==NumberError::None ? ordinal.value : missing;
}

Optional<u32> literal_shape(read StorageView storage, ShapeRef reference) on cpu {
    ShapeNode shape=unsafe_position<ShapeNode>(at(storage.nodes,reference.offset));
    match(shape) {
        ShapeNode::Literal(value) { return Optional<u32>(1,value); }
        ShapeNode::Constant(name) {}
        ShapeNode::Expression(expression) {}
    }
    return Optional<u32>(0,0);
}

extern "C" BindingCursor type_start(SignatureNode source) on cpu {
    TypeNode ty=unsafe_position<TypeNode>(at(source.storage.nodes,source.offset));
    return BindingCursor(source.storage,source.offset,ty.arguments,0u64,0,0,0,Optional<u32>(0,0));
}
extern "C" BindingCursor arguments_start(StorageView storage,Slice arguments,u32 checked) on cpu {
    return BindingCursor(storage,0u64,arguments,0u64,1,checked,0,Optional<u32>(0,0));
}
BindingRequest binding_request(address state,BindingCursor cursor,u32 kind,u64 reference) on cpu {
    unsafe_position<BindingCursor>(state)=cursor;
    return BindingRequest(kind,reference);
}
extern "C" BindingRequest binding_step(address state,address context,BindingHost host) on cpu
    requires nonzero(host.actual) requires nonzero(host.value) requires nonzero(host.reserve)
    requires nonzero(host.check) requires nonzero(host.append_value) requires nonzero(host.begin_contracts)
    requires nonzero(host.append_predicate) requires nonzero(host.construct)
    requires nonzero(host.clone_actual) requires nonzero(host.wrap) {
    BindingCursor cursor=unsafe_position<BindingCursor>(state);
    while(true){
        if(cursor.stage==8){return binding_request(state,cursor,0,0u64);}
        if(cursor.stage==0){host.reserve(context,cursor.arguments.count);cursor.stage=1;}
        if(cursor.stage==2){
            host.append_value(context,cursor.shape.value);
            cursor.index+=1u64;cursor.stage=1;
        }
        if(cursor.stage==3){cursor.index+=1u64;cursor.stage=1;}
        if(cursor.stage==1){
            if(cursor.index<cursor.arguments.count){
                ArgumentNode node=unsafe_position<ArgumentNode>(at(cursor.storage.nodes,element(cursor.storage,cursor.arguments,cursor.index)));
                match(node){
                    ArgumentNode::Type(reference){
                        TypeNode ty=unsafe_position<TypeNode>(at(cursor.storage.nodes,reference.offset));
                        if(ty.arguments.count==0u64&&ty.dataset==0&&ty.view==0&&ty.tile==0){
                            Optional<u32> bound=host.value(context,ty.name);
                            if(bound.present!=0){host.append_value(context,bound.value);cursor.index+=1u64;continue;}
                        }
                        if(cursor.checked!=0&&!host.check(context,reference)){
                            cursor.stage=8;return binding_request(state,cursor,0,0u64);
                        }
                        cursor.stage=3;return binding_request(state,cursor,1,reference.offset);
                    }
                    ArgumentNode::Value(reference){
                        Optional<u32> literal=literal_shape(cursor.storage,reference);
                        if(literal.present!=0){
                            host.append_value(context,literal.value);cursor.index+=1u64;continue;
                        }
                        cursor.stage=2;return binding_request(state,cursor,2,reference.offset);
                    }
                }
            }
            if(cursor.mode==1){cursor.stage=8;return binding_request(state,cursor,0,0u64);}
            cursor.stage=5;
        }
        if(cursor.stage==5){
            TypeNode ty=unsafe_position<TypeNode>(at(cursor.storage.nodes,cursor.offset));
            cursor.shape=Optional<u32>(0,0);
            if(ty.count.present!=0){
                cursor.shape=literal_shape(cursor.storage,ty.count.value);
                if(cursor.shape.present==0){
                    cursor.stage=6;return binding_request(state,cursor,2,ty.count.value.offset);
                }
            }
            cursor.stage=7;
        }
        if(cursor.stage==6){cursor.stage=7;}
        if(cursor.stage==7){
            TypeNode ty=unsafe_position<TypeNode>(at(cursor.storage.nodes,cursor.offset));
            address bound=host.actual(context,ty.name);
            if(address_bits(bound)!=0u64){host.clone_actual(context,bound);}
            else{
                if(ty.contracts.present!=0){
                    host.begin_contracts(context,ty.contracts.value.count);
                    u64 parameters=ty.arguments.count==0u64 ? 0u64 : ty.arguments.count-1u64;
                    u64 index=0u64;
                    while(index<ty.contracts.value.count){
                        PredicateRef reference=PredicateRef(element(cursor.storage,ty.contracts.value,index));
                        host.append_predicate(context,reference,parameters);index+=1u64;
                    }
                }
                host.construct(context,ty.name,ty.canonical,cursor.shape,ty.contracts.present);
            }
            if(ty.dataset!=0||ty.view!=0||ty.tile!=0){host.wrap(context,ty.dataset,ty.view,ty.tile,cursor.shape);}
            cursor.stage=8;return binding_request(state,cursor,0,0u64);
        }
    }
}

extern "C" void condition(SignatureNode source, Optional<u64> parameters, u32 output, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<void,address,ConditionNode> construct) on cpu
    requires nonzero(value) requires nonzero(construct) {
    ConditionNode node = unsafe_position<ConditionNode>(at(source.storage.nodes, source.offset));
    match (node) {
        ConditionNode::Name(name) {
            address data=name.kind==0 ? source.storage.source : source.storage.text;
            address bytes=at(data,name.range.start);
            u64 length=name.range.end-name.range.start;
            bool local = (output!=0 && length==8u64 && word(bytes,"returned"))
                || (parameters.present!=0 && positional_index(bytes,length)<parameters.value);
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
