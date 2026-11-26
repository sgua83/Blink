module bootstrap.c_function;

bool word<const N:u32 in {4,5,6}>(Text text,read array<u8,N> name) on cpu {
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
                else if(word(value.name,"atomic")){access=Access::Atomic;}
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
    if(cursor.stage==0){
        if(cursor.index<cursor.count){
            cursor.stage=1;return give_signature(state,cursor,SignatureNeed::Parameter);
        }
        cursor.stage=2;return give_signature(state,cursor,SignatureNeed::ResultName);
    }
    bool plain=result.arguments==0u64&&result.wrappers==0;
    bool never=plain&&(result.decorations&3)==0&&word(result.name,"never");
    bool empty=(result.declaration!=0||(plain&&result.decorations==0))&&word(result.name,"void");
    cursor.never=never?1:0;
    if(!never&&!empty){cursor.stage=3;return give_signature(state,cursor,SignatureNeed::Result);}
    return give_signature(state,cursor,SignatureNeed::Done);
}

extern "C" NamedStep named_reference_step(NamedCursor cursor,NamedFacts facts) on cpu {
 NamedCursor c=cursor;
 if(c.phase==0){c.phase=1;return NamedStep(c,NamedNeed::Resolve);}
 if(c.phase==1){if(facts.present==0){return NamedStep(c,NamedNeed::Unknown);}c.phase=2;return NamedStep(c,NamedNeed::CAbi);}
 if(c.phase==2){if(facts.c_abi==0){return NamedStep(c,NamedNeed::InvalidAbi);}c.phase=3;return NamedStep(c,NamedNeed::Arguments);}
 if(c.phase==3){c.phase=4;return NamedStep(c,NamedNeed::Instance);}
 if(c.phase==4){c.phase=5;return NamedStep(c,NamedNeed::Contracts);}
 if(c.phase==5){c.phase=6;return NamedStep(c,NamedNeed::Result);}
 if(c.phase==7){c.index+=1u64;c.phase=6;}
 if(c.phase==6){if(c.index<facts.parameters){c.phase=7;return NamedStep(c,NamedNeed::Parameter);}c.phase=8;return NamedStep(c,NamedNeed::Bind);}
 return NamedStep(c,NamedNeed::Complete);
}
bool reference_same_text(Text left,Text right) on cpu {
 if(left.length!=right.length){return false;}u64 i=0u64;
 while(i<left.length){if(unsafe_position<u8>(unsafe_address(address_bits(left.bytes)+i))!=unsafe_position<u8>(unsafe_address(address_bits(right.bytes)+i))){return false;}i+=1u64;}
 return true;
}
void reference_name(address value,address context,u64 parameters,ReferenceHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.name) requires nonzero(host.rename) {
 Text name=host.name(value);u64 base=0u64;
 while(base<name.length){if(unsafe_position<u8>(unsafe_address(address_bits(name.bytes)+base))==u8(46)){break;}base+=1u64;}
 Text root=Text(name.bytes,base);u64 index=0u64;
 while(index<parameters){if(reference_same_text(root,host.parameter(context,index))){u64 end=base;if(base<name.length&&base+1u64==name.length){end=name.length;}host.rename(value,index,end);return;}index+=1u64;}
}
void reference_condition(address value,address context,u64 parameters,ReferenceHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.name) requires nonzero(host.rename) requires nonzero(host.condition) {
 ReferenceCondition node=host.condition(value);
 if(node.kind==1){reference_name(node.left,context,parameters,host);}
 else if(node.kind==2){reference_condition(node.left,context,parameters,host);}
 else if(node.kind==3){reference_condition(node.left,context,parameters,host);reference_condition(node.right,context,parameters,host);}
}
extern "C" void reference_contracts(address context,u64 parameters,ReferenceHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.append) requires nonzero(host.parameter)
 requires nonzero(host.name) requires nonzero(host.rename) requires nonzero(host.condition) {
 ReferenceContract value=host.next(context);
 while(value.present!=0){
  if(value.predicate==1&&value.kind==0){reference_name(value.name,context,parameters,host);host.append(context);}
  else if(value.predicate==2&&value.kind==1){host.append(context);}
  else if(value.predicate==3){reference_name(value.name,context,parameters,host);reference_condition(value.condition,context,parameters,host);host.append(context);}
  value=host.next(context);
 }
}
extern "C" AddressStep address_value_step(AddressCursor cursor,AddressFacts facts) on cpu {
 AddressCursor c=cursor;
 if(c.phase==0){if(facts.cpu==0){return AddressStep(c,AddressNeed::BadTarget);}c.phase=1;return AddressStep(c,AddressNeed::Application);}
 if(c.phase==1){if(facts.application==0){return AddressStep(c,AddressNeed::BadTarget);}c.phase=2;return AddressStep(c,AddressNeed::Signature);}
 if(c.phase==2){c.phase=3;return AddressStep(c,AddressNeed::ExternalKind);}
 if(c.phase==3){c.phase=4;return AddressStep(c,facts.external!=0?AddressNeed::ExternalTarget:AddressNeed::NativeTarget);}
 if(c.phase==4){c.phase=5;return AddressStep(c,AddressNeed::Register);}
 if(c.phase==5){c.phase=6;return AddressStep(c,AddressNeed::Emit);}
 if(c.phase==6){c.phase=7;return AddressStep(c,AddressNeed::Declaration);}
 if(c.phase==7){c.phase=8;if(facts.declaration!=0){return AddressStep(c,AddressNeed::Reference);}}
 return AddressStep(c,AddressNeed::Complete);
}
