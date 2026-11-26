module bootstrap.type_reference;

extern "C" Cursor start(Reference source) on cpu {
    Cursor cursor;
    cursor.source=source;cursor.types=0u64;cursor.count=0u64;cursor.index=0u64;
    cursor.code=Code::Ordinary;cursor.canonical=0;cursor.stage=0;
    cursor.reference=unsafe_address(0u64);
    return cursor;
}
Need give(address state,Cursor cursor,Need need) on cpu {
    unsafe_position<Cursor>(state)=cursor;
    return need;
}
extern "C" Need step(address state,address context,Host host) on cpu
    requires nonzero(host.resource) requires nonzero(host.record) requires nonzero(host.reserve)
    requires nonzero(host.argument) requires nonzero(host.append_value) requires nonzero(host.append_release)
    requires nonzero(host.fail) requires nonzero(host.finish) {
    Cursor cursor=unsafe_position<Cursor>(state);
    if(cursor.stage==0){
        Reference source=cursor.source;
        if(source.kind==BuiltinKind::CFunction&&source.wrappers==0){return give(state,cursor,Need::Function);}
        if(source.resource!=0){host.resource(context);return give(state,cursor,Need::Done);}
        if(source.kind==BuiltinKind::Array){cursor.types=1u64;cursor.count=2u64;cursor.code=Code::Array;}
        else if(source.kind==BuiltinKind::Result){cursor.types=2u64;cursor.count=2u64;cursor.code=Code::Result;}
        else if(source.kind==BuiltinKind::Owned){cursor.types=1u64;cursor.count=2u64;cursor.code=Code::Owned;}
        else if(source.kind==BuiltinKind::Task||source.kind==BuiltinKind::TaskStartError){cursor.types=1u64;cursor.count=1u64;cursor.code=Code::Owned;}
        else if(source.kind==BuiltinKind::Void||source.kind==BuiltinKind::Never){cursor.code=Code::Void;}
        else if(source.kind==BuiltinKind::Missing||source.kind==BuiltinKind::CFunction){
            Parameters parameters=host.record(context);
            if(parameters.present==0){return give(state,cursor,Need::Done);}
            cursor.types=parameters.types;cursor.count=parameters.count;cursor.canonical=1;
        }
        if(source.arguments!=cursor.count){
            host.fail(context,cursor.code,Problem::Arity,cursor.count);
            return give(state,cursor,Need::Done);
        }
        host.reserve(context,cursor.count);
        cursor.stage=1;
    }
    if(cursor.stage==3){cursor.index+=1u64;cursor.stage=1;}
    if(cursor.stage==2){
        host.finish(context,cursor.canonical,cursor.source.name);
        return give(state,cursor,Need::Done);
    }
    while(cursor.index<cursor.count){
        if(cursor.source.kind==BuiltinKind::Owned&&cursor.index==1u64){
            Argument release=host.argument(context,cursor.index);
            if(address_bits(release.reference)==0u64){host.fail(context,Code::Owned,Problem::ReleaseType,0u64);return give(state,cursor,Need::Done);}
            if(release.wrappers!=0||release.arguments!=0u64){host.fail(context,Code::Owned,Problem::ReleaseForm,0u64);return give(state,cursor,Need::Done);}
            if(!host.append_release(context,release.reference)){return give(state,cursor,Need::Done);}
            cursor.index+=1u64;
        }else if(cursor.index<cursor.types){
            Argument value=host.argument(context,cursor.index);
            if(address_bits(value.reference)==0u64){host.fail(context,cursor.code,Problem::Type,0u64);return give(state,cursor,Need::Done);}
            cursor.reference=value.reference;cursor.stage=3;
            return give(state,cursor,Need::Type);
        }else{
            Argument value=host.argument(context,cursor.index);
            if(value.literal.present==0){
                cursor.stage=3;
                return give(state,cursor,Need::Value);
            }
            host.append_value(context,value.literal.value);
            cursor.index+=1u64;
        }
    }
    cursor.stage=2;
    // An unresolved count is evaluated after the actual type and const arguments.
    // An already known count is held by the same result owner.
    if(cursor.source.pending_count!=0){return give(state,cursor,Need::Count);}
    host.finish(context,cursor.canonical,cursor.source.name);
    return give(state,cursor,Need::Done);
}

bool member_is<const N:u32 in {2,3,6}>(Name name,read array<u8,N> expected) on cpu {
    if (name.length!=u64(N)) { return false; }
    for (index in 0..N) {
        u8 actual=unsafe_position<u8>(unsafe_address(address_bits(name.bytes)+u64(index)));
        if (actual!=expected[index]) { return false; }
    }
    return true;
}
extern "C" PathStart contract_path_start(BuiltinKind kind,u32 wrappers,u64 remaining) on cpu {
    if (kind==BuiltinKind::CFunction && wrappers==0) {
        return remaining==0u64 ? PathStart::FunctionNumeric : PathStart::FunctionField;
    }
    return PathStart::Bind;
}

PathAction variant_path(bool tail) on cpu {
    return tail ? PathAction::VariantTail : PathAction::Variant;
}

extern "C" PathAction contract_path_next(address path,PathInput input,PathHost host) on cpu
    requires nonzero(host.member) requires nonzero(host.ordinary_record) {
    ContractPath cursor=unsafe_position<ContractPath>(path);
    if (cursor.index==cursor.count) {
        return host.ordinary_record(host.context)!=0 ? PathAction::RecordNumeric : PathAction::Layout;
    }

    bool tail=cursor.index+1u64<cursor.count;
    if (((input.wrappers&1)!=0 || input.kind==BuiltinKind::Array) && member_is(input.field,"length")) {
        return tail ? PathAction::LengthTail : PathAction::Length;
    }
    if (input.kind==BuiltinKind::Result && (member_is(input.field,"Ok") || member_is(input.field,"Err"))) {
        return variant_path(tail);
    }
    if (input.builtin!=0) { return PathAction::Builtin; }

    PathMember member=host.member(host.context,input.field);
    if (member==PathMember::Variant) { return variant_path(tail); }
    if (member==PathMember::MissingType) { return PathAction::MissingType; }
    if (member==PathMember::MissingField) { return PathAction::MissingField; }
    unsafe_position<ContractPath>(path).index=cursor.index+1u64;
    return PathAction::Specialize;
}
