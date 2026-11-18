module bootstrap.c_function;

bool word<const N:u32 in {4,5}>(Text text,read array<u8,N> name) on cpu {
    if(text.length!=u64(N)){return false;}
    for(index in 0..N){
        if(unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+u64(index)))!=name[index]){return false;}
    }
    return true;
}
BindingCursor binding_start(address source,u64 count,u32 mode) on cpu {
    BindingCursor cursor;
    cursor.source=source;cursor.current=source;cursor.count=count;cursor.index=0u64;
    cursor.access=Access::Value;cursor.mode=mode;cursor.stage=0;
    return cursor;
}
extern "C" BindingCursor reference_start(address source,u64 count) on cpu {
    return binding_start(source,count,0);
}
extern "C" BindingCursor declaration_start(address result,u64 count) on cpu {
    return binding_start(result,count,1);
}
BindingNeed give_binding(address state,BindingCursor cursor,BindingNeed need) on cpu {
    unsafe_position<BindingCursor>(state)=cursor;return need;
}
extern "C" BindingNeed binding_step(address state,address context,BindingHost host) on cpu
    requires nonzero(host.argument) requires nonzero(host.reserve)
    requires nonzero(host.parameter) requires nonzero(host.fail) {
    BindingCursor cursor=unsafe_position<BindingCursor>(state);
    if(cursor.stage==0){
        if(cursor.mode==0){
            if(cursor.count==0u64){host.fail(context,Problem::MissingResult);return give_binding(state,cursor,BindingNeed::Done);}
            Type result=host.argument(cursor.source,0u64);
            cursor.current=result.reference;
            if(address_bits(cursor.current)==0u64){host.fail(context,Problem::TypeArgument);return give_binding(state,cursor,BindingNeed::Done);}
        }
        cursor.stage=1;
        return give_binding(state,cursor,BindingNeed::Result);
    }
    if(cursor.stage==1){
        if(cursor.mode==0){host.reserve(context,cursor.count-1u64);cursor.index=1u64;}
        cursor.stage=2;
    }
    if(cursor.stage==3){cursor.index+=1u64;cursor.stage=2;}
    if(cursor.index<cursor.count){
        Type value;
        Access access=Access::Value;
        if(cursor.mode==0){
            value=host.argument(cursor.source,cursor.index);
            if(address_bits(value.reference)==0u64){host.fail(context,Problem::TypeArgument);return give_binding(state,cursor,BindingNeed::Done);}
            if(value.wrappers==0){
                if(word(value.name,"read")){access=Access::Read;}
                else if(word(value.name,"write")){access=Access::Write;}
                else if(word(value.name,"inout")){access=Access::InOut;}
            }
            if(access!=Access::Value){
                if(value.arguments!=1u64){host.fail(context,Problem::Position);return give_binding(state,cursor,BindingNeed::Done);}
                value=host.argument(value.reference,0u64);
                if(address_bits(value.reference)==0u64){host.fail(context,Problem::Position);return give_binding(state,cursor,BindingNeed::Done);}
            }
        }else{
            Parameter parameter=host.parameter(context,cursor.index);
            value=parameter.value;access=parameter.access;
        }
        cursor.current=value.reference;cursor.access=access;cursor.stage=3;
        return give_binding(state,cursor,cursor.mode==0?BindingNeed::AppendParameter:BindingNeed::ReplaceParameter);
    }
    return give_binding(state,cursor,BindingNeed::Done);
}
bool ordinary(Layout value,address context,cfn<void,address,Problem> fail) on cpu requires nonzero(fail) {
    if(value.dataset!=0||value.owned!=0){fail(context,Problem::Resource);return false;}
    if(value.views!=0||value.empty!=0){fail(context,Problem::EmptyOrView);return false;}
    return true;
}
extern "C" SignatureCursor signature_start(u64 count) on cpu {
    SignatureCursor cursor;
    cursor.count=count;cursor.index=0u64;
    cursor.layout.dataset=0;cursor.layout.owned=0;cursor.layout.views=0;cursor.layout.empty=0;
    cursor.access=Access::Value;cursor.stage=0;cursor.never=0;
    return cursor;
}
SignatureNeed give_signature(address state,SignatureCursor cursor,SignatureNeed need) on cpu {
    unsafe_position<SignatureCursor>(state)=cursor;return need;
}
extern "C" SignatureNeed signature_step(address state,ResultShape result,address context,SignatureHost host) on cpu
    requires nonzero(host.append_value) requires nonzero(host.append_reference)
    requires nonzero(host.use_result) requires nonzero(host.fail) {
    SignatureCursor cursor=unsafe_position<SignatureCursor>(state);
    if(cursor.stage==1){
        if(!ordinary(cursor.layout,context,host.fail)){return give_signature(state,cursor,SignatureNeed::Done);}
        if(cursor.access==Access::Value){host.append_value(context);}
        else{host.append_reference(context,cursor.access);}
        cursor.index+=1u64;cursor.stage=0;
    }
    if(cursor.stage==3){
        if(!ordinary(cursor.layout,context,host.fail)){return give_signature(state,cursor,SignatureNeed::Done);}
        host.use_result(context);
        return give_signature(state,cursor,SignatureNeed::Done);
    }
    if(cursor.index<cursor.count){
        cursor.stage=1;return give_signature(state,cursor,SignatureNeed::Parameter);
    }
    bool plain=result.arguments==0u64&&result.wrappers==0;
    bool never=plain&&(result.decorations&3)==0&&word(result.name,"never");
    bool empty=plain&&result.decorations==0&&word(result.name,"void");
    cursor.never=never?1:0;
    if(!never&&!empty){cursor.stage=3;return give_signature(state,cursor,SignatureNeed::Result);}
    return give_signature(state,cursor,SignatureNeed::Done);
}
