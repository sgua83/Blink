module bootstrap.task_lower;
extern "C" Cursor start() on cpu {return Cursor(0,0u64,0u64);}
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
extern "C" Step<LookupNeed> lookup_step(Cursor state,LookupFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){if(c.index<facts.count){c.phase=1;return Step<LookupNeed>(c,LookupNeed::Worker);}return Step<LookupNeed>(c,LookupNeed::Missing);}
 if(c.phase==1){if(facts.equal!=0){c.phase=2;return Step<LookupNeed>(c,LookupNeed::Input);}c.phase=5;return Step<LookupNeed>(c,LookupNeed::Next);}
 if(c.phase==2){if(facts.equal!=0){c.phase=3;return Step<LookupNeed>(c,LookupNeed::Start);}c.phase=5;return Step<LookupNeed>(c,LookupNeed::Next);}
 if(c.phase==3){if(facts.equal!=0){c.phase=4;return Step<LookupNeed>(c,LookupNeed::Selector);}c.phase=5;return Step<LookupNeed>(c,LookupNeed::Next);}
 if(c.phase==4){if(facts.equal!=0){return Step<LookupNeed>(c,LookupNeed::Found);}c.phase=5;return Step<LookupNeed>(c,LookupNeed::Next);}
 c.index+=1u64;c.phase=0;return Step<LookupNeed>(c,LookupNeed::Next);
}
extern "C" Step<NewNeed> new_step(Cursor state,NewFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;if(facts.has_result!=0){return Step<NewNeed>(c,NewNeed::CompletionResult);}return Step<NewNeed>(c,NewNeed::CompletionUnit);}
 if(c.phase==1){c.phase=2;return Step<NewNeed>(c,NewNeed::Output);}
 if(c.phase==2){c.phase=3;return Step<NewNeed>(c,NewNeed::Index);}
 if(c.phase==3){c.phase=4;return Step<NewNeed>(c,NewNeed::Layout);}
 if(c.phase==4){if(c.index<facts.fields&&c.part<facts.parameters){c.phase=5;return Step<NewNeed>(c,NewNeed::Parameter);}c.phase=10;return Step<NewNeed>(c,NewNeed::Name);}
 if(c.phase==5){if(facts.frame!=0){c.part+=1u64;c.phase=4;return Step<NewNeed>(c,NewNeed::Next);}c.phase=6;return Step<NewNeed>(c,NewNeed::Field);}
 if(c.phase==6){if(facts.owned!=0&&facts.reference!=0){c.phase=7;if(facts.inout!=0){return Step<NewNeed>(c,NewNeed::RenameInOut);}return Step<NewNeed>(c,NewNeed::RenameWrite);}c.phase=8;return Step<NewNeed>(c,NewNeed::Next);}
 if(c.phase==7){c.phase=8;return Step<NewNeed>(c,NewNeed::PositionType);}
 if(c.phase==8){c.index+=1u64;c.part+=1u64;c.phase=4;return Step<NewNeed>(c,NewNeed::Next);}
 if(c.phase==10){c.phase=11;return Step<NewNeed>(c,NewNeed::Entry);}
 if(c.phase==11){c.phase=12;return Step<NewNeed>(c,NewNeed::Types);}
 if(c.phase==12){c.phase=13;return Step<NewNeed>(c,NewNeed::Positions);}
 if(c.phase==13){c.phase=14;return Step<NewNeed>(c,NewNeed::Binding);}
 return Step<NewNeed>(c,NewNeed::Complete);
}
extern "C" Step<RewriteNeed> rewrite_step(Cursor state) on cpu {
 Cursor c=state;c.phase+=1;
 if(state.phase==0){return Step<RewriteNeed>(c,RewriteNeed::Registers);}
 if(state.phase==1){return Step<RewriteNeed>(c,RewriteNeed::Address);}
 if(state.phase==2){return Step<RewriteNeed>(c,RewriteNeed::Index);}
 if(state.phase==3){return Step<RewriteNeed>(c,RewriteNeed::Worker);}
 if(state.phase==4){return Step<RewriteNeed>(c,RewriteNeed::Trim);}
 if(state.phase==5){return Step<RewriteNeed>(c,RewriteNeed::Argument);}
 if(state.phase==6){return Step<RewriteNeed>(c,RewriteNeed::Symbol);}
 return Step<RewriteNeed>(c,RewriteNeed::Complete);
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
