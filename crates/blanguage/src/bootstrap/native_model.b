module bootstrap.native_model;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" Step<RebindNeed> rebind_step(Cursor state,RebindFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(facts.kind==Kind::Unit){return Step<RebindNeed>(c,RebindNeed::Complete);}
  if(facts.kind==Kind::Data){c.phase=1;return Step<RebindNeed>(c,RebindNeed::Root);}
  if(facts.kind==Kind::Value){c.phase=9;return Step<RebindNeed>(c,RebindNeed::Value);}
  c.phase=3;return Step<RebindNeed>(c,RebindNeed::Owned);
 }
 if(c.phase==1){c.phase=2;return Step<RebindNeed>(c,RebindNeed::Length);}
 if(c.phase==2){c.phase=9;return Step<RebindNeed>(c,RebindNeed::DataPlace);}
 if(c.phase==3){c.phase=4;return Step<RebindNeed>(c,RebindNeed::OwnedPlace);}
 if(c.phase==4){if(c.index<facts.payloads){c.phase=5;return Step<RebindNeed>(c,RebindNeed::Inspect);}return Step<RebindNeed>(c,RebindNeed::Complete);}
 if(c.phase==5){
  c.phase=6;
  if(facts.present==0){return Step<RebindNeed>(c,RebindNeed::Next);}
  if(facts.variant!=0){return Step<RebindNeed>(c,RebindNeed::Variant);}
  return Step<RebindNeed>(c,RebindNeed::Aggregate);
 }
 if(c.phase==6){c.index+=1u64;c.phase=4;return Step<RebindNeed>(c,RebindNeed::Next);}
 return Step<RebindNeed>(c,RebindNeed::Complete);
}
extern "C" Step<ValueNeed> value_step(Cursor state,u32 constant) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return Step<ValueNeed>(c,ValueNeed::Proof);}
 if(c.phase==1){c.phase=2;return Step<ValueNeed>(c,ValueNeed::Constant);}
 if(c.phase==2){if(constant==0){c.phase=3;return Step<ValueNeed>(c,ValueNeed::Known);}c.phase=4;return Step<ValueNeed>(c,ValueNeed::Uniform);}
 if(c.phase==3){c.phase=4;return Step<ValueNeed>(c,ValueNeed::Uniform);}
 return Step<ValueNeed>(c,ValueNeed::Complete);
}
extern "C" Step<TranslateNeed> translate_step(Cursor state,TranslateFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(facts.kind!=facts.model||facts.kind==Kind::Unit){return Step<TranslateNeed>(c,TranslateNeed::Complete);}
  if(facts.kind==Kind::Data){c.phase=10;return Step<TranslateNeed>(c,TranslateNeed::Root);}
  if(facts.kind==Kind::Value){c.phase=99;return Step<TranslateNeed>(c,TranslateNeed::Scalar);}
  c.phase=1;return Step<TranslateNeed>(c,TranslateNeed::Scalar);
 }
 if(c.phase==1){if(c.index<facts.payloads&&c.index<facts.model_payloads){c.phase=2;return Step<TranslateNeed>(c,TranslateNeed::Inspect);}return Step<TranslateNeed>(c,TranslateNeed::Complete);}
 if(c.phase==2){c.phase=3;if(facts.present!=0&&facts.model_present!=0){return Step<TranslateNeed>(c,TranslateNeed::Payload);}return Step<TranslateNeed>(c,TranslateNeed::Clear);}
 if(c.phase==3){c.index+=1u64;c.phase=1;return Step<TranslateNeed>(c,TranslateNeed::Next);}
 if(c.phase==10){c.phase=11;return Step<TranslateNeed>(c,TranslateNeed::Length);}
 if(c.phase==11){c.phase=12;return Step<TranslateNeed>(c,TranslateNeed::RootProof);}
 if(c.phase==12){c.phase=13;return Step<TranslateNeed>(c,TranslateNeed::LengthProof);}
 if(c.phase==13){c.phase=14;return Step<TranslateNeed>(c,TranslateNeed::Residency);}
 if(c.phase==14){c.phase=15;return Step<TranslateNeed>(c,TranslateNeed::Prefixes);}
 if(c.phase==15){c.phase=16;return Step<TranslateNeed>(c,TranslateNeed::Count);}
 if(c.phase==16){c.phase=99;return Step<TranslateNeed>(c,TranslateNeed::Elements);}
 return Step<TranslateNeed>(c,TranslateNeed::Complete);
}
extern "C" void project(address parent,address child,address value,u32 offset,address output,ProjectionHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.component) requires nonzero(host.reg) requires nonzero(host.append) {
 u64 count=host.count(child);u64 parents=host.count(parent);
 u64 index=0u64;
 while(index<count){
  Component field=host.component(child,index);u32 word=offset+field.word;u64 selected=0u64;bool found=false;
  while(selected<parents&&!found){
   Component part=host.component(parent,selected);
   if(part.word<=word&&word<part.word+part.size){found=true;}
   else{selected+=1u64;}
  }
  host.append(output,host.reg(value,selected));index+=1u64;
 }
}
