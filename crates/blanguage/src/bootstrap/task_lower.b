module bootstrap.task_lower;
extern "C" u32 bind(address context,u32 root,usize functions,BindHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.function) {
 if(root!=0 && host.root(context)!=0) {return 1;}
 usize index=0u64;
 while(index<functions) {
  if(host.function(context,index)!=0) {return 1;}
  index+=1u64;
 }
 return 0;
}
extern "C" u32 rewrite_block(address block,usize count,address context,RewriteHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(host.start) {
 usize index=0u64;
 while(index<count) {
  Instruction inst=host.inspect(block,index);
  if(inst.kind==Kind::Start) {
   if(host.start(context,block,index)!=0) {return 1;}
  } else {
   if(inst.kind==Kind::Scope || inst.kind==Kind::Pair) {
    if(host.child(context,inst.first)!=0) {return 1;}
    if(inst.kind==Kind::Pair) {
     if(host.child(context,inst.second)!=0) {return 1;}
    }
   }
  }
  index+=1u64;
 }
 return 0;
}
extern "C" BoundStart bind_start(address context,usize count,StartHost host) on cpu
 requires nonzero(host.matches) requires nonzero(host.create) {
 usize index=0u64;
 while(index<count && host.matches(context,index)==0){index+=1u64;}
 if(index==count){host.create(context);}
 return BoundStart(index,2u64,4u64);
}

extern "C" void bind_positions(address context,usize fields,usize parameters,usize first,PositionHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.owned) requires nonzero(host.publish) {
 usize field=0u64;
 usize parameter=0u64;
 usize position=first;
 while(field<fields && parameter<parameters){
  BindingParameter kind=host.parameter(context,parameter);
  if(kind!=BindingParameter::Frame){
   if(kind!=BindingParameter::Value && host.owned(context,field)!=0){
    host.publish(context,field,parameter,position,kind);
    position+=1u64;
   }
   field+=1u64;
  }
  parameter+=1u64;
 }
}

extern "C" EntryStatus emit_entry(address context,EntrySpec spec,EntryHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.field) requires nonzero(host.valid)
 requires nonzero(host.pointer) requires nonzero(host.reference) requires nonzero(host.load_value)
 requires nonzero(host.value) requires nonzero(host.call) requires nonzero(host.raise_error)
 requires nonzero(host.store) requires nonzero(host.selector) {
 usize index=0u64;
 usize argument=0u64;
 while(index<spec.parameters){
  EntryParameter parameter=host.parameter(context,index);
  if(parameter.role==Role::ReturnSource){
   if(spec.has_result==0){return EntryStatus::MissingResult;}
   host.reference(context,index,parameter.role,EntryAddress(4u64,5u64),spec.result_bytes);
  }else if(parameter.role==Role::Frame){
   if(parameter.size!=0){host.value(context,index,0u64,2u64);}
  }else{
   if(argument==spec.fields){
    return parameter.role==Role::Completion ? EntryStatus::MissingCompletion : EntryStatus::MissingArgument;
   }
   EntryField field=host.field(context,argument);
   argument+=1u64;
   if(parameter.role==Role::Completion){
    host.reference(context,index,parameter.role,EntryAddress(field.buffer,field.buffer+1u64),field.offset);
   }else if(parameter.reference!=0){
    if(host.valid(context,index,field.ty,parameter.reference)==0){return EntryStatus::BadAddress;}
    EntryAddress pointer=host.pointer(context,field);
    host.reference(context,index,parameter.role,pointer,0);
   }else{
    if(host.valid(context,index,field.ty,0)==0){return EntryStatus::BadType;}
    usize first=0u64;
    if(parameter.components!=0u64){
     first=host.load_value(context,index,0u64,field);
     usize part=1u64;
     while(part<parameter.components){
      host.load_value(context,index,part,field);
      part+=1u64;
     }
    }
    if(parameter.size!=0){host.value(context,index,first,parameter.components);}
   }
  }
  index+=1u64;
 }
 if(argument<spec.fields){return EntryStatus::Unused;}
 EntryCall call=host.call(context,spec.fallible);
 if(spec.fallible!=0){host.raise_error(context,call.error);}
 usize part=0u64;
 while(part<spec.results){
  host.store(context,part,call.result+part);
  part+=1u64;
 }
 if(spec.selector!=0){host.selector(context);}
 return EntryStatus::Complete;
}
