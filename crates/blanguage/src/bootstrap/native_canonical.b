module bootstrap.native_canonical;
extern "C" Cursor start() on cpu {return Cursor(0,0u64,0u64);}
extern "C" Step<CanonicalNeed> canonical_step(Cursor state) on cpu {
 Cursor c=state;c.phase+=1;
 if(state.phase==0){return Step<CanonicalNeed>(c,CanonicalNeed::Seed);}
 if(state.phase==1){return Step<CanonicalNeed>(c,CanonicalNeed::Parameters);}
 if(state.phase==2){return Step<CanonicalNeed>(c,CanonicalNeed::Values);}
 if(state.phase==3){return Step<CanonicalNeed>(c,CanonicalNeed::Elements);}
 if(state.phase==4){return Step<CanonicalNeed>(c,CanonicalNeed::Sources);}
 if(state.phase==5){return Step<CanonicalNeed>(c,CanonicalNeed::BindSources);}
 if(state.phase==6){return Step<CanonicalNeed>(c,CanonicalNeed::Resources);}
 if(state.phase==7){return Step<CanonicalNeed>(c,CanonicalNeed::Memory);}
 if(state.phase==8){return Step<CanonicalNeed>(c,CanonicalNeed::Models);}
 if(state.phase==9){return Step<CanonicalNeed>(c,CanonicalNeed::Names);}
 if(state.phase==10){return Step<CanonicalNeed>(c,CanonicalNeed::Initialization);}
 if(state.phase==11){return Step<CanonicalNeed>(c,CanonicalNeed::PublishNames);}
 if(state.phase==12){return Step<CanonicalNeed>(c,CanonicalNeed::PrefixBindings);}
 if(state.phase==13){return Step<CanonicalNeed>(c,CanonicalNeed::Translate);}
 if(state.phase==14){return Step<CanonicalNeed>(c,CanonicalNeed::Complete);}
 return Step<CanonicalNeed>(c,CanonicalNeed::Complete);
}
extern "C" Step<OutputNeed> output_step(Cursor state,OutputMode mode,OutputFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){if(c.index<facts.count){c.phase=1;return Step<OutputNeed>(c,OutputNeed::Inspect);}return Step<OutputNeed>(c,OutputNeed::Complete);}
 if(c.phase==1){
  c.phase=3;
  if(mode==OutputMode::Models){if(facts.present!=0){return Step<OutputNeed>(c,OutputNeed::Model);}return Step<OutputNeed>(c,OutputNeed::Empty);}
  if(mode==OutputMode::Values&&facts.view!=0){return Step<OutputNeed>(c,OutputNeed::Value);}
  if(facts.present!=0){
   if(mode==OutputMode::Sources){return Step<OutputNeed>(c,OutputNeed::Sources);}
   if(mode==OutputMode::Resources){return Step<OutputNeed>(c,OutputNeed::Resources);}
   if(mode==OutputMode::Translate){c.phase=2;return Step<OutputNeed>(c,OutputNeed::Translate);}
  }
  return Step<OutputNeed>(c,OutputNeed::Continue);
 }
 if(c.phase==2){c.phase=3;return Step<OutputNeed>(c,OutputNeed::Prefix);}
 c.index+=1u64;c.phase=0;return Step<OutputNeed>(c,OutputNeed::Continue);
}
extern "C" Step<SourceNeed> source_step(Cursor state,SourceFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<SourceNeed>(c,SourceNeed::Next);}
 if(c.phase==1){if(facts.present==0){return Step<SourceNeed>(c,SourceNeed::Complete);}c.phase=2;return Step<SourceNeed>(c,SourceNeed::Bound);}
 if(c.phase==2){c.phase=3;if(facts.access!=SourceAccess::Value&&facts.dataset==0&&facts.tile==0){return Step<SourceNeed>(c,SourceNeed::Storage);}return Step<SourceNeed>(c,SourceNeed::Continue);}
 if(c.phase==3){c.part=0u64;c.phase=4;if(facts.access==SourceAccess::Read||facts.access==SourceAccess::InOut){return Step<SourceNeed>(c,SourceNeed::InspectContent);}c.phase=6;return Step<SourceNeed>(c,SourceNeed::Continue);}
 if(c.phase==4){if(c.part<facts.content){c.phase=5;return Step<SourceNeed>(c,SourceNeed::Content);}c.phase=6;return Step<SourceNeed>(c,SourceNeed::Continue);}
 if(c.phase==5){c.part+=1u64;c.phase=4;return Step<SourceNeed>(c,SourceNeed::Continue);}
 if(c.phase==6){c.phase=7;c.part=0u64;return Step<SourceNeed>(c,SourceNeed::InspectResources);}
 if(c.phase==7){
  u64 count=0u64;
  if(facts.resource==ResourceKind::Owned){count=facts.resources;}
  if(facts.resource==ResourceKind::Application&&facts.access==SourceAccess::Value){count=1u64;}
  if(c.part<count){c.phase=8;return Step<SourceNeed>(c,SourceNeed::Resource);}
  c.index+=1u64;c.phase=0;return Step<SourceNeed>(c,SourceNeed::Continue);
 }
 c.part+=1u64;c.phase=7;return Step<SourceNeed>(c,SourceNeed::Continue);
}
extern "C" Step<MemoryNeed> memory_step(Cursor state,MemoryFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<MemoryNeed>(c,MemoryNeed::Next);}
 if(c.phase==1){if(facts.present==0){return Step<MemoryNeed>(c,MemoryNeed::Complete);}c.phase=2;c.part=0u64;return Step<MemoryNeed>(c,MemoryNeed::Continue);}
 if(c.phase==2){
  u64 count=0u64;if(facts.kind==MemoryKind::Snapshot){count=facts.width;}if(facts.kind==MemoryKind::Dataset){count=1u64;}
  if(c.part<count){c.phase=3;return Step<MemoryNeed>(c,MemoryNeed::Bind);}
  c.index+=1u64;c.phase=0;return Step<MemoryNeed>(c,MemoryNeed::Continue);
 }
 c.part+=1u64;c.phase=2;return Step<MemoryNeed>(c,MemoryNeed::Continue);
}
extern "C" Step<ModelNeed> model_step(Cursor state,ModelFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){if(facts.present==0){c.part=facts.base;return Step<ModelNeed>(c,ModelNeed::Complete);}c.phase=1;return Step<ModelNeed>(c,ModelNeed::Capture);}
 if(c.phase==1){c.phase=2;c.part=facts.base;return Step<ModelNeed>(c,ModelNeed::Rebind);}
 if(c.phase==2){if(c.part<facts.base+facts.width){c.phase=3;return Step<ModelNeed>(c,ModelNeed::Register);}c.phase=5;return Step<ModelNeed>(c,ModelNeed::Paths);}
 if(c.phase==3){c.phase=4;if(facts.known!=0){return Step<ModelNeed>(c,ModelNeed::Identity);}return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==4){c.part+=1u64;c.phase=2;return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==5){c.phase=6;c.index=0u64;return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==6){if(c.index<facts.paths){c.phase=7;c.part=0u64;return Step<ModelNeed>(c,ModelNeed::Root);}c.part=facts.base+facts.width;return Step<ModelNeed>(c,ModelNeed::Complete);}
 if(c.phase==7){
  c.phase=8;c.part=facts.depth;
  if(facts.root==0){c.phase=10;return Step<ModelNeed>(c,ModelNeed::Continue);}
  if(facts.borrowed!=0){return Step<ModelNeed>(c,ModelNeed::BorrowedName);}
  return Step<ModelNeed>(c,ModelNeed::OwnedName);
 }
 if(c.phase==8){if(c.part!=0u64){c.part-=1u64;c.phase=9;return Step<ModelNeed>(c,ModelNeed::Element);}c.phase=10;return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==9){c.phase=8;return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==10){
  c.phase=12;
  if(facts.depth!=0u64){c.phase=11;return Step<ModelNeed>(c,ModelNeed::InspectNested);}
  if(facts.root!=0){return Step<ModelNeed>(c,ModelNeed::RootIdentities);}
  return Step<ModelNeed>(c,ModelNeed::Continue);
 }
 if(c.phase==11){c.phase=12;if(facts.nested!=0){return Step<ModelNeed>(c,ModelNeed::Nested);}return Step<ModelNeed>(c,ModelNeed::Continue);}
 if(c.phase==12){
  c.phase=13;
  if(facts.root!=0){if(facts.model_index==0u64){return Step<ModelNeed>(c,ModelNeed::ReturnedName);}c.part=facts.model_index-1u64;return Step<ModelNeed>(c,ModelNeed::WrittenName);}
  return Step<ModelNeed>(c,ModelNeed::Continue);
 }
 c.index+=1u64;c.phase=6;return Step<ModelNeed>(c,ModelNeed::Continue);
}
extern "C" Step<InitialNeed> initial_step(Cursor state,InitialFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<InitialNeed>(c,InitialNeed::Next);}
 if(c.phase==1){if(facts.present==0){return Step<InitialNeed>(c,InitialNeed::Complete);}if(facts.position==0||facts.pending!=0){return Step<InitialNeed>(c,InitialNeed::Incomplete);}c.phase=2;return Step<InitialNeed>(c,InitialNeed::Keep);}
 c.phase=1;return Step<InitialNeed>(c,InitialNeed::Next);
}
extern "C" Step<BoundNeed> bound_step(Cursor state,BoundFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<BoundNeed>(c,BoundNeed::Next);}
 if(c.phase==1){if(facts.present==0){return Step<BoundNeed>(c,BoundNeed::Complete);}if(facts.local!=0){c.phase=0;return Step<BoundNeed>(c,BoundNeed::Continue);}c.phase=2;return Step<BoundNeed>(c,BoundNeed::When);}
 if(c.phase==2){c.phase=3;return Step<BoundNeed>(c,BoundNeed::Bind);}
 if(c.phase==3){c.phase=0;return Step<BoundNeed>(c,BoundNeed::Yield);}
 return Step<BoundNeed>(c,BoundNeed::Complete);
}
extern "C" Step<PrefixParameterNeed> prefix_parameter_step(Cursor state,PrefixParameterFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Next);}
 if(c.phase==1){if(facts.present==0){return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Complete);}c.phase=3;if(facts.origin!=0){c.phase=2;return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Bound);}return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Continue);}
 if(c.phase==2){c.phase=3;if(facts.dataset!=0){return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Bind);}return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Continue);}
 if(facts.reference!=0){c.part+=2u64;}else{c.part+=facts.width;}c.index+=1u64;c.phase=0;return Step<PrefixParameterNeed>(c,PrefixParameterNeed::Continue);
}
extern "C" Step<PrefixNeed> prefix_step(Cursor state,PrefixFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){if(facts.kind==Kind::Data){c.phase=1;return Step<PrefixNeed>(c,PrefixNeed::Facts);}if(facts.kind==Kind::Owned){c.phase=2;return Step<PrefixNeed>(c,PrefixNeed::Continue);}return Step<PrefixNeed>(c,PrefixNeed::Complete);}
 if(c.phase==1){c.phase=9;return Step<PrefixNeed>(c,PrefixNeed::Elements);}
 if(c.phase==2){if(c.index<facts.payloads){c.phase=3;return Step<PrefixNeed>(c,PrefixNeed::Inspect);}return Step<PrefixNeed>(c,PrefixNeed::Complete);}
 if(c.phase==3){c.phase=4;if(facts.present!=0){return Step<PrefixNeed>(c,PrefixNeed::Payload);}return Step<PrefixNeed>(c,PrefixNeed::Continue);}
 if(c.phase==4){c.index+=1u64;c.phase=2;return Step<PrefixNeed>(c,PrefixNeed::Continue);}
 return Step<PrefixNeed>(c,PrefixNeed::Complete);
}
extern "C" void seed(address context,u32 task_source,u32 returned_view,SeedHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.input) requires nonzero(host.prepare_returns)
 requires nonzero(host.next_return) requires nonzero(host.next_completion)
 requires nonzero(host.input_binding) requires nonzero(host.return_binding) requires nonzero(host.completion_binding) {
 u64 count=host.count(context);u64 index=0u64;
 while(index<count){host.input_binding(context,host.input(context,index),index);index+=1u64;}
 if(task_source!=0&&returned_view!=0){
  host.prepare_returns(context);Index item=host.next_return(context);
  while(item.present!=0){host.return_binding(context,item.value);item=host.next_return(context);}
 }
 Completion completion=host.next_completion(context);
 while(completion.present!=0){
  u64 input=0u64;while(host.input(context,input)!=completion.address){input+=1u64;}
  u64 component=0u64;
  while(component<completion.components+completion.tasks){host.completion_binding(context,completion.address,component,input);component+=1u64;}
  completion=host.next_completion(context);
 }
}
extern "C" void apply_known(address context,BindingHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.apply) {
 Known item=host.next(context);while(item.present!=0){if(item.known!=0){host.apply(context);}item=host.next(context);}
}

extern "C" Step<SourceModelNeed> element_sources_step(Cursor state,SourceModelFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;if(facts.view!=0){return Step<SourceModelNeed>(c,SourceModelNeed::View);}return Step<SourceModelNeed>(c,SourceModelNeed::Continue);}
 if(c.phase==1){
  if(facts.kind==Kind::Data){c.phase=2;return Step<SourceModelNeed>(c,SourceModelNeed::InspectElement);}
  if(facts.kind==Kind::Owned){c.phase=3;return Step<SourceModelNeed>(c,SourceModelNeed::Continue);}
  return Step<SourceModelNeed>(c,SourceModelNeed::Complete);
 }
 if(c.phase==2){c.phase=9;if(facts.present!=0){return Step<SourceModelNeed>(c,SourceModelNeed::Element);}return Step<SourceModelNeed>(c,SourceModelNeed::Complete);}
 if(c.phase==3){if(c.index<facts.payloads){c.phase=4;return Step<SourceModelNeed>(c,SourceModelNeed::InspectPayload);}return Step<SourceModelNeed>(c,SourceModelNeed::Complete);}
 if(c.phase==4){c.phase=5;if(facts.present!=0){return Step<SourceModelNeed>(c,SourceModelNeed::Payload);}return Step<SourceModelNeed>(c,SourceModelNeed::Continue);}
 if(c.phase==5){c.index+=1u64;c.phase=3;return Step<SourceModelNeed>(c,SourceModelNeed::Continue);}
 return Step<SourceModelNeed>(c,SourceModelNeed::Complete);
}
