module bootstrap.generic_instance;

address at(address data, u64 offset) on cpu { return unsafe_address(address_bits(data)+offset); }
T row<T>(StorageView storage, u64 offset) on cpu { return unsafe_position<T>(at(storage.nodes,offset)); }
u64 item(StorageView storage, Slice values, u64 index) on cpu { return row<u64>(storage,values.offset+index*8u64); }
SignatureNode child(SignatureNode parent, u64 offset) on cpu { return SignatureNode(parent.storage,offset); }
NameBytes text(StorageView storage, Text value) on cpu {
    return NameBytes(at(value.kind == 0 ? storage.source : storage.text,value.range.start),value.range.end-value.range.start);
}
bool same(NameBytes left, NameBytes right) on cpu {
    if (left.length != right.length) { return false; }
    u64 index=0u64;
    while (index<left.length) {
        if (unsafe_position<u8>(at(left.bytes,index))!=unsafe_position<u8>(at(right.bytes,index))) { return false; }
        index+=1u64;
    }
    return true;
}
bool word<const N:u32 in {3,4,5,6}>(NameBytes name, read array<u8,N> expected) on cpu {
    if (name.length!=u64(N)) { return false; }
    for (index in 0..N) { if (unsafe_position<u8>(at(name.bytes,u64(index)))!=expected[index]) { return false; } }
    return true;
}
bool plain_void(SemanticInfo value) on cpu { return value.arguments==0u64 && value.flags==0 && word(value.name,"void"); }
bool is_shape(SemanticInfo value) on cpu { return address_bits(value.value)==0u64 || (value.arguments==0u64 && (value.flags&7)==0); }
u64 smaller(u64 left,u64 right) on cpu { return left<right ? left : right; }

bool infer_type(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name) on cpu
requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    if (!type_mentions(formal,name.bytes,name.length)) { return true; }
    return type_mentioned(host,context,formal,actual,index,name);
}

extern "C" bool specialize(SignatureNode function, u64 actual_count, Optional<u64> explicit_count, Host host, address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    FunctionNode declaration=row<FunctionNode>(function.storage,function.offset);
    NameBytes function_name=text(function.storage,declaration.name);
    if (declaration.types.count==0u64 && declaration.values.count==0u64 && explicit_count.present==0) { return host.finish(context); }
    u32 explicit_types=0;
    u64 first_value=0u64;
    if (explicit_count.present!=0) {
        if (explicit_count.value==declaration.types.count+declaration.values.count && explicit_count.value!=0u64) {
            explicit_types=1; first_value=declaration.types.count;
        } else {
            if (explicit_count.value!=declaration.values.count || declaration.values.count==0u64) {
                host.fail(context,Problem::GenericArguments,function_name,address(),0,0); return false;
            }
        }
    }
    u64 index=0u64;
    while (index<declaration.types.count) {
        TypeParameterNode parameter=row<TypeParameterNode>(function.storage,item(function.storage,declaration.types,index));
        NameBytes name=text(function.storage,parameter.name);
        if (explicit_types!=0) {
            SemanticInfo syntax=host.explicit_argument(context,index);
            if (address_bits(syntax.value)==0u64) { host.fail(context,Problem::TypePosition,name,address(),0,0); return false; }
            if ((syntax.flags&6)!=0) { host.fail(context,Problem::TypeWrapped,name,address(),0,0); return false; }
            if (!host.explicit_type(context,index,plain_void(syntax) ? 1 : 0)) { return false; }
            if ((syntax.flags&1)!=0 && !host.explicit_dataset(context,index)) { return false; }
            QueryBool accepted=host.family_current(context,index);
            if (accepted.ready==0) { return false; }
            if (accepted.value==0) { host.fail(context,Problem::ExplicitFamily,name,address(),0,0); return false; }
        } else {
            u64 position=0u64;
            while (position<smaller(declaration.params.count,actual_count)) {
                ParamNode formal=row<ParamNode>(function.storage,item(function.storage,declaration.params,position));
                SignatureNode source=child(function,formal.ty.offset);
                if (type_mentions(source,name.bytes,name.length)) {
                    ActualArgument actual=host.actual(context,position);
                    if (address_bits(actual.ty.value)==0u64) { host.fail(context,Problem::Resource,name,address(),0,0); return false; }
                    // This immutable source was just queried; the recursive entry
                    // retains its query only for genuinely new child nodes.
                    if (!type_mentioned(host,context,source,actual.ty,index,name)) { return false; }
                }
                position+=1u64;
            }
            if (!host.has_type(context)) { host.fail(context,Problem::MissingType,name,address(),0,0); return false; }
        }
        host.push_type(context); index+=1u64;
    }
    index=0u64;
    while (index<declaration.values.count) {
        ValueParameterNode parameter=row<ValueParameterNode>(function.storage,item(function.storage,declaration.values,index));
        NameBytes name=text(function.storage,parameter.name);
        u32 value;
        if (explicit_count.present!=0) {
            SemanticInfo syntax=host.explicit_argument(context,first_value+index);
            if (!is_shape(syntax)) { host.fail(context,Problem::ConstShape,name,address(),0,0); return false; }
            Optional<u32> result=host.explicit_value(context,first_value+index);
            if (result.present==0) { return false; }
            value=result.value;
        } else {
            Optional<u32> inferred=Optional<u32>(0,0);
            u64 position=0u64;
            while (position<smaller(declaration.params.count,actual_count)) {
                ParamNode formal=row<ParamNode>(function.storage,item(function.storage,declaration.params,position));
                SignatureNode source=child(function,formal.ty.offset);
                ActualArgument actual=host.actual(context,position);
                if (actual.count.present!=0 && !infer_const_count(source,name.bytes,name.length,actual.count.value,inferred)) {
                    host.fail(context,Problem::RepeatCount,name,address(),0,0); return false;
                }
                if (address_bits(actual.ty.value)!=0u64 && !infer_value(host,context,source,actual.ty,name,inferred)) { return false; }
                position+=1u64;
            }
            if (inferred.present==0) { host.fail(context,Problem::MissingValue,name,address(),0,0); return false; }
            value=inferred.value;
        }
        if (!accepts_const(function.storage.nodes,parameter.choices,value)) { host.fail(context,Problem::ValueDomain,name,address(),value,0); return false; }
        host.push_value(context,value); index+=1u64;
    }
    return host.finish(context);
}

extern "C" bool type_mentioned(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    if (source.dataset!=0 && (actual.flags&1)!=0) {
        return host.type_dataset(context,formal,host.dataset_syntax(actual.value),index,name);
    }
    return type_after_dataset(host,context,formal,actual,index,name);
}

extern "C" bool type_after_dataset(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    TypeInfo current=actual;
    if (source.view!=0) {
        if ((current.flags&2)==0) { host.fail(context,Problem::ViewExpected,name,address(),0,0); return false; }
        current=host.view(current.value);
    }
    NameBytes source_name=text(formal.storage,source.name);
    if (same(source_name,name)) {
        if (source.arguments.count!=0u64) { host.fail(context,Problem::TypeParameterArguments,name,address(),0,0); return false; }
        QueryBool accepted=host.family_actual(context,index,current.value);
        if (accepted.ready==0) { return false; }
        if (accepted.value==0) { host.fail(context,Problem::InferredFamily,name,current.value,0,0); return false; }
        if (host.has_type(context)) {
            if (!host.same_type(context,current.value)) { host.fail(context,Problem::TypeMismatch,name,current.value,0,0); return false; }
        } else { host.keep_type(context,current.value); }
        return true;
    }
    if (word(source_name,"cfn")) {
        if ((current.flags&16)==0) { host.fail(context,Problem::CfnType,name,address(),0,0); return false; }
        if (source.arguments.count!=current.parameters+1u64) { host.fail(context,Problem::CfnArity,name,address(),0,0); return false; }
        u64 position=0u64;
        while (position<source.arguments.count) {
            ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
            TypeRef reference;
            match(argument) {
                ArgumentNode::Type(value) { reference=value; }
                ArgumentNode::Value(value) { host.fail(context,Problem::CfnTypeArgument,name,address(),0,0); return false; }
            }
            SignatureNode next=child(formal,reference.offset);
            CfnSlot slot=host.cfn_slot(current.value,position);
            if (position!=0u64) {
                CfnParameter projected=cfn_parameter(next);
                if (projected.access!=slot.access) { host.fail(context,Problem::CfnAccess,name,address(),0,0); return false; }
                if (projected.type.present==0) { host.fail(context,Problem::CfnPosition,name,address(),0,0); return false; }
                next=child(formal,projected.type.value.offset);
            }
            if (type_mentions(next,name.bytes,name.length) && !host.type_cfn(context,next,slot.ty,index,name,plain_void(slot.ty) ? 1 : 0)) { return false; }
            position+=1u64;
        }
        return true;
    }
    if (word(source_name,"result")) {
        if ((current.flags&8)==0 || source.arguments.count!=2u64) { host.fail(context,Problem::ResultType,name,address(),0,0); return false; }
        u64 position=0u64;
        while (position<smaller(source.arguments.count,current.arguments)) {
            ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
            match(argument) {
                ArgumentNode::Type(value) { if (!infer_type(host,context,child(formal,value.offset),host.child(current.value,position),index,name)) { return false; } }
                ArgumentNode::Value(value) { host.fail(context,Problem::ResultTypeArgument,name,address(),0,0); return false; }
            }
            position+=1u64;
        }
        return true;
    }
    if (word(source_name,"array")) {
        if ((current.flags&4)==0) { host.fail(context,Problem::ArrayType,name,address(),0,0); return false; }
        if (source.arguments.count==0u64) { host.fail(context,Problem::ArrayElement,name,address(),0,0); return false; }
        ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,0u64));
        TypeRef reference;
        match(argument) {
            ArgumentNode::Type(value) { reference=value; }
            ArgumentNode::Value(value) { host.fail(context,Problem::ArrayElement,name,address(),0,0); return false; }
        }
        if (source.arguments.count!=2u64) { host.fail(context,Problem::ArrayArity,name,address(),0,0); return false; }
        return infer_type(host,context,child(formal,reference.offset),host.element(current.value),index,name);
    }
    return host.record_type(context,formal,current,index,name);
}

extern "C" bool type_record(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name, NameBytes base) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    if (source.arguments.count!=actual.arguments+actual.value_count || !host.record_matches(actual.value,base)) {
        host.fail(context,Problem::RecordType,text(formal.storage,source.name),actual.value,0,0); return false;
    }
    u64 position=0u64;
    while (position<smaller(source.arguments.count,actual.arguments)) {
        ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
        match(argument) {
            ArgumentNode::Type(value) { if (!infer_type(host,context,child(formal,value.offset),host.child(actual.value,position),index,name)) { return false; } }
            ArgumentNode::Value(value) { host.fail(context,Problem::RecordTypeArgument,name,address(),0,0); return false; }
        }
        position+=1u64;
    }
    return true;
}

extern "C" bool infer_value(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    if (source.dataset!=0 && (actual.flags&1)!=0 && actual.value_count!=0u64) {
        u32 value=unsafe_position<u32>(actual.values);
        if (!infer_const_count(formal,name.bytes,name.length,value,inferred)) { host.fail(context,Problem::DatasetCount,name,address(),0,0); return false; }
    }
    if (source.dataset!=0 && (actual.flags&1)!=0) {
        return host.value_dataset(context,formal,host.dataset_syntax(actual.value),name,inferred);
    }
    return value_after_dataset(host,context,formal,actual,name,inferred);
}

extern "C" bool value_after_dataset(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    TypeInfo current=actual;
    if (source.view!=0) {
        if ((current.flags&2)==0) { return true; }
        current=host.view(current.value);
    }
    if (source.arguments.count==0u64) { return true; }
    NameBytes source_name=text(formal.storage,source.name);
    if (word(source_name,"cfn")) {
        if ((current.flags&16)==0) { host.fail(context,Problem::CfnValue,name,address(),0,0); return false; }
        if (source.arguments.count!=current.parameters+1u64) { host.fail(context,Problem::CfnValueArity,name,address(),0,0); return false; }
        u64 position=0u64;
        while (position<source.arguments.count) {
            ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
            TypeRef reference;
            match(argument) {
                ArgumentNode::Type(value) { reference=value; }
                ArgumentNode::Value(value) { host.fail(context,Problem::CfnTypeArgument,name,address(),0,0); return false; }
            }
            SignatureNode next=child(formal,reference.offset);
            CfnSlot slot=host.cfn_slot(current.value,position);
            if (position!=0u64) {
                CfnParameter projected=cfn_parameter(next);
                if (projected.access!=slot.access) { host.fail(context,Problem::CfnAccess,name,address(),0,0); return false; }
                if (projected.type.present==0) { host.fail(context,Problem::CfnPosition,name,address(),0,0); return false; }
                next=child(formal,projected.type.value.offset);
            }
            if (!plain_void(slot.ty) && slot.ty.never==0 && !host.value_cfn(context,next,slot.ty,name,inferred)) { return false; }
            position+=1u64;
        }
        return true;
    }
    if (word(source_name,"result")) {
        if ((current.flags&8)==0 || source.arguments.count!=2u64) { host.fail(context,Problem::ResultValue,name,address(),0,0); return false; }
        u64 position=0u64;
        while (position<smaller(source.arguments.count,current.arguments)) {
            ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
            match(argument) {
                ArgumentNode::Type(value) { if (!infer_value(host,context,child(formal,value.offset),host.child(current.value,position),name,inferred)) { return false; } }
                ArgumentNode::Value(value) { host.fail(context,Problem::ResultTypeArgument,name,address(),0,0); return false; }
            }
            position+=1u64;
        }
        return true;
    }
    if (word(source_name,"array")) {
        if ((current.flags&4)==0) { host.fail(context,Problem::ArrayValue,name,address(),0,0); return false; }
        if (source.arguments.count!=2u64) { host.fail(context,Problem::ArrayArity,name,address(),0,0); return false; }
        ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,0u64));
        match(argument) {
            ArgumentNode::Type(value) { if (!infer_value(host,context,child(formal,value.offset),host.element(current.value),name,inferred)) { return false; } }
            ArgumentNode::Value(value) { }
        }
        Optional<u32> conflict=infer_const_arguments(formal,name.bytes,name.length,1u64,current.values,current.value_count,inferred);
        if (conflict.present!=0) { host.fail(context,Problem::ArrayCount,name,address(),0,0); return false; }
        return true;
    }
    return host.record_value(context,formal,current,name,inferred);
}

extern "C" bool value_record(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, NameBytes base, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches) {
    TypeNode source=row<TypeNode>(formal.storage,formal.offset);
    if (source.arguments.count!=actual.arguments+actual.value_count || !host.record_matches(actual.value,base)) {
        host.fail(context,Problem::RecordValue,name,address(),0,0); return false;
    }
    u64 position=0u64;
    while (position<smaller(source.arguments.count,actual.arguments)) {
        ArgumentNode argument=row<ArgumentNode>(formal.storage,item(formal.storage,source.arguments,position));
        match(argument) {
            ArgumentNode::Type(value) { if (!infer_value(host,context,child(formal,value.offset),host.child(actual.value,position),name,inferred)) { return false; } }
            ArgumentNode::Value(value) { }
        }
        position+=1u64;
    }
    Optional<u32> conflict=infer_const_arguments(formal,name.bytes,name.length,actual.arguments,actual.values,actual.value_count,inferred);
    if (conflict.present!=0) { host.fail(context,Problem::RecordConstConflict,name,address(),inferred.value,conflict.value); return false; }
    return true;
}

extern "C" bool instantiate(SignatureNode function, u32 tracking, u64 type_count, u64 value_count, InstanceHost host, address context) on cpu
    requires nonzero(host.implementation) requires nonzero(host.external) requires nonzero(host.plain)
    requires nonzero(host.name) requires nonzero(host.type_argument) requires nonzero(host.value_argument)
    requires nonzero(host.constant) requires nonzero(host.nonzero_error) requires nonzero(host.uses)
    requires nonzero(host.construct) requires nonzero(host.c_abi) requires nonzero(host.interface) {
    FunctionNode declaration=row<FunctionNode>(function.storage,function.offset);
    if (tracking!=0) {
        if (declaration.kind!=2 && declaration.body.present!=0) { host.implementation(context); }
        if (declaration.external.present!=0) { host.external(context); }
    }
    if (declaration.types.count==0u64 && declaration.values.count==0u64) { host.plain(context); return true; }
    host.name(context);
    u64 index=0u64;
    while (index<smaller(declaration.types.count,type_count)) {
        TypeParameterNode parameter=row<TypeParameterNode>(function.storage,item(function.storage,declaration.types,index));
        host.type_argument(context,text(function.storage,parameter.name)); index+=1u64;
    }
    index=0u64;
    while (index<smaller(declaration.values.count,value_count)) {
        ValueParameterNode parameter=row<ValueParameterNode>(function.storage,item(function.storage,declaration.values,index));
        host.value_argument(context,text(function.storage,parameter.name),index); index+=1u64;
    }
    index=0u64;
    while (index<declaration.contracts.count) {
        ContractNode contract=row<ContractNode>(function.storage,item(function.storage,declaration.contracts,index));
        if (contract.kind==0) {
            PredicateNode predicate=row<PredicateNode>(function.storage,contract.predicate.offset);
            match(predicate) {
                PredicateNode::Nonzero(value) {
                    NameBytes name=text(function.storage,value);
                    Optional<u32> bound=host.constant(context,name);
                    if (bound.present!=0 && bound.value==0) { host.nonzero_error(context,index,name); return false; }
                }
                PredicateNode::Prefix(value) { }
                PredicateNode::Nonempty(value) { }
                PredicateNode::Index(value) { }
                PredicateNode::Contiguous(value) { }
                PredicateNode::Initialized(value) { }
                PredicateNode::SameLength(value) { }
                PredicateNode::Cleanup(value) { }
                PredicateNode::Memory(value) { }
                PredicateNode::Range(value) { }
            }
        }
        index+=1u64;
    }
    if (!host.uses(context)) { return false; }
    host.construct(context);
    if (declaration.c_abi!=0 && !host.c_abi(context)) { return false; }
    return host.interface(context);
}

extern "C" bool is_open(StorageView type_storage, Slice types, StorageView value_storage, Slice values) on cpu {
    u64 index=0u64;
    while (index<types.count) {
        TypeParameterNode parameter=row<TypeParameterNode>(type_storage,item(type_storage,types,index));
        if (parameter.family.present==0) { return true; }
        index+=1u64;
    }
    index=0u64;
    while (index<values.count) {
        ValueParameterNode parameter=row<ValueParameterNode>(value_storage,item(value_storage,values,index));
        if (parameter.choices.present==0) { return true; }
        index+=1u64;
    }
    return false;
}

