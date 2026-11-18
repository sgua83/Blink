module bootstrap.dataset_drop;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" u32 index_maximum(u32 maximum) on cpu {return maximum==0 ? 0 : maximum-1;}
extern "C" Step<ParentNeed> parent_step(Cursor state,ParentFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){cursor.phase=1;return Step<ParentNeed>(cursor,ParentNeed::Borrow);}
 if(cursor.phase==1){cursor.phase=2;return Step<ParentNeed>(cursor,ParentNeed::Position);}
 if(cursor.phase==2){if(facts.borrowed!=0){return Step<ParentNeed>(cursor,ParentNeed::Borrowed);}cursor.phase=3;return Step<ParentNeed>(cursor,ParentNeed::Inspect);}
 if(cursor.phase==3){
  if(facts.children!=0&&facts.nonempty!=0){
   if(facts.placed!=0){cursor.phase=4;return Step<ParentNeed>(cursor,ParentNeed::Refresh);}
   cursor.phase=5;return Step<ParentNeed>(cursor,ParentNeed::Reference);
  }
  cursor.phase=7;if(facts.placed!=0){return Step<ParentNeed>(cursor,ParentNeed::StoredHandle);}return Step<ParentNeed>(cursor,ParentNeed::Handle);
 }
 if(cursor.phase==4){cursor.phase=5;return Step<ParentNeed>(cursor,ParentNeed::Reference);}
 if(cursor.phase==5){cursor.phase=6;return Step<ParentNeed>(cursor,ParentNeed::Elements);}
 if(cursor.phase==6){cursor.phase=7;return Step<ParentNeed>(cursor,ParentNeed::ReferenceHandle);}
 if(cursor.phase==7){cursor.phase=8;return Step<ParentNeed>(cursor,ParentNeed::Release);}
 return Step<ParentNeed>(cursor,ParentNeed::Complete);
}
extern "C" Step<ElementNeed> element_step(Cursor state,Model model) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){
  if(model==Model::Unknown||model==Model::Input||model==Model::Cleanup){cursor.phase=1;return Step<ElementNeed>(cursor,ElementNeed::Read);}
  cursor.phase=3;return Step<ElementNeed>(cursor,ElementNeed::KnownModel);
 }
 if(cursor.phase==1){cursor.phase=2;return Step<ElementNeed>(cursor,ElementNeed::NativeModel);}
 if(cursor.phase==2){return Step<ElementNeed>(cursor,ElementNeed::NativeDrop);}
 if(cursor.phase==3){cursor.phase=4;return Step<ElementNeed>(cursor,ElementNeed::Storage);}
 return Step<ElementNeed>(cursor,ElementNeed::KnownDrop);
}
extern "C" Step<LoopNeed> loop_step(Cursor state) on cpu {
 Cursor cursor=state;cursor.phase+=1;
 if(state.phase==0){return Step<LoopNeed>(cursor,LoopNeed::Cpu);}
 if(state.phase==1){return Step<LoopNeed>(cursor,LoopNeed::Initialize);}
 if(state.phase==2){return Step<LoopNeed>(cursor,LoopNeed::Outer);}
 if(state.phase==3){return Step<LoopNeed>(cursor,LoopNeed::Load);}
 if(state.phase==4){return Step<LoopNeed>(cursor,LoopNeed::Zero);}
 if(state.phase==5){return Step<LoopNeed>(cursor,LoopNeed::Positive);}
 if(state.phase==6){return Step<LoopNeed>(cursor,LoopNeed::Condition);}
 if(state.phase==7){return Step<LoopNeed>(cursor,LoopNeed::One);}
 if(state.phase==8){return Step<LoopNeed>(cursor,LoopNeed::Previous);}
 if(state.phase==9){return Step<LoopNeed>(cursor,LoopNeed::Place);}
 if(state.phase==10){return Step<LoopNeed>(cursor,LoopNeed::Drop);}
 if(state.phase==11){return Step<LoopNeed>(cursor,LoopNeed::Advance);}
 if(state.phase==12){return Step<LoopNeed>(cursor,LoopNeed::Loop);}
 return Step<LoopNeed>(cursor,LoopNeed::Complete);
}
extern "C" Step<NativeNeed> native_step(Cursor state,Kind kind) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){
  cursor.phase=1;
  if(kind==Kind::Data){return Step<NativeNeed>(cursor,NativeNeed::DataType);}
  if(kind==Kind::Owned){return Step<NativeNeed>(cursor,NativeNeed::OwnedType);}
  if(kind==Kind::Value){return Step<NativeNeed>(cursor,NativeNeed::ValueType);}
  return Step<NativeNeed>(cursor,NativeNeed::UnitType);
 }
 if(cursor.phase==1){cursor.phase=2;if(kind==Kind::Data){return Step<NativeNeed>(cursor,NativeNeed::MovableData);}if(kind==Kind::Owned){return Step<NativeNeed>(cursor,NativeNeed::MovableTree);}return Step<NativeNeed>(cursor,NativeNeed::Next);}
 if(cursor.phase==2){cursor.phase=3;return Step<NativeNeed>(cursor,NativeNeed::Declare);}
 return Step<NativeNeed>(cursor,NativeNeed::Invoke);
}
extern "C" Step<u32> resource_step(Cursor state,u64 count) on cpu {
 Cursor cursor=state;if(cursor.index>=count){return Step<u32>(cursor,0);}cursor.index+=1u64;return Step<u32>(cursor,1);
}
extern "C" Step<DeclNeed> declaration_step(Cursor state,DeclFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){cursor.phase=1;return Step<DeclNeed>(cursor,DeclNeed::Name);}
 if(cursor.phase==1){cursor.phase=2;return Step<DeclNeed>(cursor,DeclNeed::Header);}
 if(cursor.phase==2){cursor.phase=3;return Step<DeclNeed>(cursor,DeclNeed::Declare);}
 if(cursor.phase==3){cursor.phase=4;if(facts.made!=0){return Step<DeclNeed>(cursor,DeclNeed::Body);}return Step<DeclNeed>(cursor,DeclNeed::Next);}
 return Step<DeclNeed>(cursor,DeclNeed::Complete);
}
extern "C" Step<BodyNeed> body_step(Cursor state,BodyFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){cursor.phase=2;return Step<BodyNeed>(cursor,BodyNeed::Builder);}
 if(cursor.phase==2){cursor.phase=3;return Step<BodyNeed>(cursor,BodyNeed::Frame);}
 if(cursor.phase==3){cursor.phase=4;return Step<BodyNeed>(cursor,BodyNeed::Calls);}
 if(cursor.phase==4){cursor.phase=5;return Step<BodyNeed>(cursor,BodyNeed::Origin);}
 if(cursor.phase==5){if(facts.dataset!=0){cursor.phase=6;return Step<BodyNeed>(cursor,BodyNeed::DataParameter);}cursor.phase=20;return Step<BodyNeed>(cursor,BodyNeed::Registers);}
 if(cursor.phase==6){cursor.phase=7;return Step<BodyNeed>(cursor,BodyNeed::DataAbi);}
 if(cursor.phase==7){cursor.phase=10;return Step<BodyNeed>(cursor,BodyNeed::DataBind);}
 if(cursor.phase==20){cursor.phase=21;return Step<BodyNeed>(cursor,BodyNeed::ValueAbi);}
 if(cursor.phase==21){cursor.phase=22;return Step<BodyNeed>(cursor,BodyNeed::Value);}
 if(cursor.phase==22){cursor.phase=23;return Step<BodyNeed>(cursor,BodyNeed::Inspect);}
 if(cursor.phase==23){if(facts.owned!=0){cursor.phase=24;return Step<BodyNeed>(cursor,BodyNeed::Incoming);}cursor.phase=10;return Step<BodyNeed>(cursor,BodyNeed::ValueBind);}
 if(cursor.phase==24){cursor.phase=25;return Step<BodyNeed>(cursor,BodyNeed::Residency);}
 if(cursor.phase==25){cursor.phase=10;return Step<BodyNeed>(cursor,BodyNeed::OwnedBind);}
 if(cursor.phase==10){cursor.phase=11;return Step<BodyNeed>(cursor,BodyNeed::TakeElementsEnv);}
 if(cursor.phase==11){cursor.phase=12;return Step<BodyNeed>(cursor,BodyNeed::ElementPaths);}
 if(cursor.phase==12){cursor.phase=13;return Step<BodyNeed>(cursor,BodyNeed::PutElementsEnv);}
 if(cursor.phase==13){cursor.phase=14;return Step<BodyNeed>(cursor,BodyNeed::BindElements);}
 if(cursor.phase==14){cursor.phase=15;if(facts.dataset!=0){return Step<BodyNeed>(cursor,BodyNeed::ResourceFacts);}return Step<BodyNeed>(cursor,BodyNeed::Next);}
 if(cursor.phase==15){cursor.phase=16;return Step<BodyNeed>(cursor,BodyNeed::TakeContractEnv);}
 if(cursor.phase==16){cursor.phase=17;return Step<BodyNeed>(cursor,BodyNeed::Resolve);}
 if(cursor.phase==17){cursor.phase=18;return Step<BodyNeed>(cursor,BodyNeed::Assume);}
 if(cursor.phase==18){cursor.phase=19;return Step<BodyNeed>(cursor,BodyNeed::PutContractEnv);}
 if(cursor.phase==19){cursor.phase=30;return Step<BodyNeed>(cursor,BodyNeed::Arrays);}
 if(cursor.phase==30){cursor.phase=31;return Step<BodyNeed>(cursor,BodyNeed::Release);}
 if(cursor.phase==31){cursor.phase=32;return Step<BodyNeed>(cursor,BodyNeed::Initialization);}
 if(cursor.phase==32){cursor.phase=33;return Step<BodyNeed>(cursor,BodyNeed::Returns);}
 if(cursor.phase==33){cursor.phase=34;return Step<BodyNeed>(cursor,BodyNeed::Finish);}
 if(cursor.phase==34){cursor.phase=35;return Step<BodyNeed>(cursor,BodyNeed::Publish);}
 return Step<BodyNeed>(cursor,BodyNeed::Complete);
}
