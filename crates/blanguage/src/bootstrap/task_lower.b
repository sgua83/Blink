module bootstrap.task_lower;
extern "C" Cursor start() on cpu {return Cursor(0,0u64,0u64);}
extern "C" Step<BindNeed> bind_step(Cursor state,BindFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){if(facts.resources==0){return Step<BindNeed>(c,BindNeed::Empty);}c.phase=1;return Step<BindNeed>(c,BindNeed::Start);}
 if(c.phase==1){c.phase=2;if(facts.root!=0){return Step<BindNeed>(c,BindNeed::Root);}return Step<BindNeed>(c,BindNeed::Next);}
 if(c.phase==2){if(c.index<facts.functions){c.phase=3;return Step<BindNeed>(c,BindNeed::Function);}c.phase=4;return Step<BindNeed>(c,BindNeed::Entries);}
 if(c.phase==3){c.index+=1u64;c.phase=2;return Step<BindNeed>(c,BindNeed::Next);}
 if(c.phase==4){c.index=0u64;c.phase=5;return Step<BindNeed>(c,BindNeed::Next);}
 if(c.phase==5){if(c.index<facts.bindings){c.phase=6;return Step<BindNeed>(c,BindNeed::Entry);}if(facts.bindings==0u64){return Step<BindNeed>(c,BindNeed::Empty);}return Step<BindNeed>(c,BindNeed::Present);}
 c.index+=1u64;c.phase=5;return Step<BindNeed>(c,BindNeed::Next);
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
extern "C" Step<EntryNeed> entry_step(Cursor state,EntryFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<EntryNeed>(c,EntryNeed::Body);}
 if(c.phase==1){c.phase=2;return Step<EntryNeed>(c,EntryNeed::Zero);}
 if(c.phase==2){if(c.index<facts.parameters){c.part=0u64;c.phase=3;return Step<EntryNeed>(c,EntryNeed::Parameter);}c.phase=30;return Step<EntryNeed>(c,EntryNeed::Extra);}
 if(c.phase==3){
  if(facts.role==Role::ReturnSource){if(facts.result==0){return Step<EntryNeed>(c,EntryNeed::MissingResult);}c.phase=20;return Step<EntryNeed>(c,EntryNeed::ReturnSource);}
  if(facts.role==Role::Completion){c.phase=4;return Step<EntryNeed>(c,EntryNeed::CompletionField);}
  if(facts.role==Role::Frame){c.phase=10;return Step<EntryNeed>(c,EntryNeed::Frame);}
  c.phase=5;return Step<EntryNeed>(c,EntryNeed::Field);
 }
 if(c.phase==4){if(facts.field==0){return Step<EntryNeed>(c,EntryNeed::MissingCompletion);}c.phase=20;return Step<EntryNeed>(c,EntryNeed::CompletionReference);}
 if(c.phase==5){if(facts.field==0){return Step<EntryNeed>(c,EntryNeed::MissingArgument);}if(facts.reference!=0){c.phase=6;return Step<EntryNeed>(c,EntryNeed::ValidateAddress);}c.phase=9;return Step<EntryNeed>(c,EntryNeed::ValidateType);}
 if(c.phase==6){
  if(facts.address==0){return Step<EntryNeed>(c,EntryNeed::BadAddress);}c.phase=7;return Step<EntryNeed>(c,EntryNeed::AddressLow);
 }
 if(c.phase==7){c.phase=8;return Step<EntryNeed>(c,EntryNeed::AddressHigh);}
 if(c.phase==8){c.phase=20;return Step<EntryNeed>(c,EntryNeed::Reference);}
 if(c.phase==9){if(facts.same==0){return Step<EntryNeed>(c,EntryNeed::BadType);}c.part=0u64;c.phase=11;return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==11){if(c.part<facts.components){c.phase=12;return Step<EntryNeed>(c,EntryNeed::LoadValue);}c.phase=20;if(facts.size!=0){return Step<EntryNeed>(c,EntryNeed::Value);}return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==12){c.part+=1u64;c.phase=11;return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==10){c.phase=20;if(facts.size!=0){return Step<EntryNeed>(c,EntryNeed::Value);}return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==20){c.index+=1u64;c.phase=2;return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==30){if(facts.extra!=0){return Step<EntryNeed>(c,EntryNeed::Unused);}c.part=0u64;c.phase=31;return Step<EntryNeed>(c,EntryNeed::ResultBase);}
 if(c.phase==31){if(c.part<facts.result_components){c.phase=32;return Step<EntryNeed>(c,EntryNeed::ResultReg);}c.phase=33;if(facts.fallible!=0){return Step<EntryNeed>(c,EntryNeed::ErrorReg);}return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==32){c.part+=1u64;c.phase=31;return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==33){c.phase=34;return Step<EntryNeed>(c,EntryNeed::Call);}
 if(c.phase==34){if(facts.fallible!=0){c.phase=35;return Step<EntryNeed>(c,EntryNeed::ErrorZero);}c.part=0u64;c.phase=38;return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==35){c.phase=36;return Step<EntryNeed>(c,EntryNeed::ErrorCompare);}
 if(c.phase==36){c.part=0u64;c.phase=38;return Step<EntryNeed>(c,EntryNeed::ErrorIf);}
 if(c.phase==38){if(c.part<facts.result_components){c.phase=39;return Step<EntryNeed>(c,EntryNeed::Store);}c.phase=40;if(facts.selector!=0){return Step<EntryNeed>(c,EntryNeed::Selector);}return Step<EntryNeed>(c,EntryNeed::Next);}
 if(c.phase==39){c.part+=1u64;c.phase=38;return Step<EntryNeed>(c,EntryNeed::Next);}
 return Step<EntryNeed>(c,EntryNeed::Finish);
}
