module bootstrap.written_output;
extern "C" Cursor start() on cpu {return Cursor(0,0,0u64,0u64);}
extern "C" WrittenStep written_step(Cursor state,WrittenFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return WrittenStep(c,WrittenNeed::Binding);}
 if(c.phase==1){
  if(f.kind==1){c.phase=5;return WrittenStep(c,WrittenNeed::Place);}
  c.phase=2;return WrittenStep(c,WrittenNeed::Resolve);
 }
 if(c.phase==2){c.phase=3;return WrittenStep(c,WrittenNeed::Prepare);}
 if(c.phase==3){c.phase=4;return WrittenStep(c,WrittenNeed::Available);}
 if(c.phase==4){
  if(f.available==0){return WrittenStep(c,WrittenNeed::BadAvailable);}
  c.phase=5;return WrittenStep(c,WrittenNeed::Place);
 }
 if(c.phase==5){
  if(f.kind==1||f.variants==0){c.phase=6;return WrittenStep(c,WrittenNeed::Initialized);}
  c.phase=10;return WrittenStep(c,WrittenNeed::Load);
 }
 if(c.phase==6){
  if(f.initialized==0){c.phase=7;return WrittenStep(c,WrittenNeed::Defer);}
  c.phase=8;return WrittenStep(c,WrittenNeed::Next);
 }
 if(c.phase==7){
  if(f.deferred==0){return WrittenStep(c,WrittenNeed::BadInit);}
  c.phase=8;return WrittenStep(c,WrittenNeed::Next);
 }
 if(c.phase==8){
  if(f.kind==1){c.phase=12;c.index=0u64;return WrittenStep(c,WrittenNeed::Next);}
  c.phase=10;return WrittenStep(c,WrittenNeed::Load);
 }
 if(c.phase==10){c.phase=11;return WrittenStep(c,WrittenNeed::ReturnedInitialized);}
 if(c.phase==11){return WrittenStep(c,WrittenNeed::Complete);}
 if(c.phase==12){
  if(c.index<f.fields){c.phase=13;return WrittenStep(c,WrittenNeed::Register);}
  c.phase=14;return WrittenStep(c,WrittenNeed::Plain);
 }
 if(c.phase==13){c.index+=1u64;c.phase=12;return WrittenStep(c,WrittenNeed::Next);}
 if(c.phase==14){return WrittenStep(c,WrittenNeed::Complete);}
 return WrittenStep(c,WrittenNeed::Next);
}
extern "C" ConditionalStep conditional_step(Cursor state,ConditionalFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return ConditionalStep(c,ConditionalNeed::Binding);}
 if(c.phase==1){c.phase=2;return ConditionalStep(c,ConditionalNeed::Place);}
 if(c.phase==2){c.phase=3;c.index=0u64;return ConditionalStep(c,ConditionalNeed::Path);}
 if(c.phase==3){
  if(c.index>=f.guards){c.phase=20;return ConditionalStep(c,ConditionalNeed::RestorePath);}
  c.phase=4;return ConditionalStep(c,ConditionalNeed::Guard);
 }
 if(c.phase==4){
  if(f.possible==0){c.phase=10;return ConditionalStep(c,ConditionalNeed::Next);}
  if(f.kind==2){
   if(f.snapshot==0){return ConditionalStep(c,ConditionalNeed::Snapshot);}
   c.phase=5;return ConditionalStep(c,ConditionalNeed::Prepare);
  }
  c.phase=6;return ConditionalStep(c,ConditionalNeed::Selected);
 }
 if(c.phase==5){c.phase=6;return ConditionalStep(c,ConditionalNeed::Selected);}
 if(c.phase==6){c.phase=7;return ConditionalStep(c,ConditionalNeed::Initialized);}
 if(c.phase==7){
  if(f.initialized==0){c.phase=8;return ConditionalStep(c,ConditionalNeed::Defer);}
  c.phase=10;return ConditionalStep(c,ConditionalNeed::Next);
 }
 if(c.phase==8){
  if(f.deferred==0){return ConditionalStep(c,ConditionalNeed::BadInit);}
  c.phase=10;return ConditionalStep(c,ConditionalNeed::Next);
 }
 if(c.phase==10){
  if(f.kind==2&&f.possible!=0){c.phase=11;return ConditionalStep(c,ConditionalNeed::RestoreInitialization);}
  c.index+=1u64;c.phase=3;return ConditionalStep(c,ConditionalNeed::Next);
 }
 if(c.phase==11){c.index+=1u64;c.phase=3;return ConditionalStep(c,ConditionalNeed::Next);}
 if(c.phase==20){
  if(f.kind==2){c.phase=21;return ConditionalStep(c,ConditionalNeed::Model);}
  c.phase=25;c.part=0u64;return ConditionalStep(c,ConditionalNeed::Next);
 }
 if(c.phase==21){c.phase=22;return ConditionalStep(c,ConditionalNeed::Resources);}
 if(c.phase==22){c.phase=23;return ConditionalStep(c,ConditionalNeed::RestorePath);}
 if(c.phase==23){c.phase=25;c.part=0u64;return ConditionalStep(c,ConditionalNeed::Next);}
 if(c.phase==25){
  if(c.part<f.fields){c.phase=26;return ConditionalStep(c,ConditionalNeed::Register);}
  c.phase=27;return ConditionalStep(c,f.kind==1 ? ConditionalNeed::Plain : ConditionalNeed::Rebind);
 }
 if(c.phase==26){c.part+=1u64;c.phase=25;return ConditionalStep(c,ConditionalNeed::Next);}
 if(c.phase==27){return ConditionalStep(c,ConditionalNeed::Complete);}
 return ConditionalStep(c,ConditionalNeed::Next);
}
extern "C" DataStep data_step(Cursor state,DataFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return DataStep(c,DataNeed::Path);}
 if(c.phase==1){
  if(c.index<f.fields&&c.index<f.guards){c.phase=2;return DataStep(c,DataNeed::FieldPath);}
  if(f.resources!=0&&f.nonzero!=0){c.phase=5;return DataStep(c,DataNeed::Elements);}
  c.phase=7;return DataStep(c,DataNeed::Restore);
 }
 if(c.phase==2){
  if(f.possible!=0){c.phase=3;return DataStep(c,DataNeed::Initialized);}
  c.index+=1u64;c.phase=1;return DataStep(c,DataNeed::Next);
 }
 if(c.phase==3){
  if(f.initialized==0){c.phase=4;return DataStep(c,DataNeed::Defer);}
  c.index+=1u64;c.phase=1;return DataStep(c,DataNeed::Next);
 }
 if(c.phase==4){
  if(f.deferred==0){return DataStep(c,DataNeed::BadInit);}
  c.index+=1u64;c.phase=1;return DataStep(c,DataNeed::Next);
 }
 if(c.phase==5){c.phase=6;return DataStep(c,DataNeed::Resources);}
 if(c.phase==6){c.phase=7;return DataStep(c,DataNeed::Restore);}
 if(c.phase==7){return DataStep(c,DataNeed::Complete);}
 return DataStep(c,DataNeed::Next);
}
extern "C" ResourceStep resource_step(Cursor state,ResourceFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return ResourceStep(c,ResourceNeed::Inspect);}
 if(c.phase==1){
  if(f.kind==2){c.phase=2;return ResourceStep(c,ResourceNeed::Guard);}
  if(f.kind==3){
   if(f.cleanup!=0){c.phase=20;return ResourceStep(c,ResourceNeed::Guard);}
   c.phase=30;return ResourceStep(c,ResourceNeed::Next);
  }
  return ResourceStep(c,ResourceNeed::Complete);
 }
 if(c.phase==2){c.phase=3;return ResourceStep(c,ResourceNeed::TruePath);}
 if(c.phase==3){
  if(f.possible!=0){c.phase=4;return ResourceStep(c,ResourceNeed::ResolveData);}
  c.phase=7;return ResourceStep(c,ResourceNeed::FalsePath);
 }
 if(c.phase==4){c.phase=5;return ResourceStep(c,ResourceNeed::AvailableData);}
 if(c.phase==5){
  if(f.available==0){return ResourceStep(c,ResourceNeed::BadData);}
  c.phase=6;return ResourceStep(c,ResourceNeed::RequireData);
 }
 if(c.phase==6){c.phase=7;return ResourceStep(c,ResourceNeed::FalsePath);}
 if(c.phase==7){
  if(f.possible!=0){c.phase=8;return ResourceStep(c,ResourceNeed::MayOwnData);}
  c.phase=9;return ResourceStep(c,ResourceNeed::Next);
 }
 if(c.phase==8){
  if(f.may_own!=0){return ResourceStep(c,ResourceNeed::BadDataAbsent);}
  c.phase=9;return ResourceStep(c,ResourceNeed::Next);
 }
 if(c.phase==9){
  if(f.moved!=0){c.phase=10;return ResourceStep(c,ResourceNeed::MarkAbsent);}
  return ResourceStep(c,ResourceNeed::Complete);
 }
 if(c.phase==10){return ResourceStep(c,ResourceNeed::Complete);}
 if(c.phase==20){c.phase=21;return ResourceStep(c,ResourceNeed::TruePath);}
 if(c.phase==21){
  if(f.possible!=0){c.phase=22;return ResourceStep(c,ResourceNeed::AvailableValue);}
  c.phase=23;return ResourceStep(c,ResourceNeed::FalsePath);
 }
 if(c.phase==22){
  if(f.available==0){return ResourceStep(c,ResourceNeed::BadValue);}
  c.phase=23;return ResourceStep(c,ResourceNeed::FalsePath);
 }
 if(c.phase==23){
  if(f.possible!=0){c.phase=24;return ResourceStep(c,ResourceNeed::MayOwnValue);}
  c.phase=30;return ResourceStep(c,ResourceNeed::Next);
 }
 if(c.phase==24){
  if(f.may_own!=0){return ResourceStep(c,ResourceNeed::BadValueAbsent);}
  c.phase=30;return ResourceStep(c,ResourceNeed::Next);
 }
 if(c.phase==30){
  if(c.index>=f.children){return ResourceStep(c,ResourceNeed::Complete);}
  c.phase=31;return ResourceStep(c,ResourceNeed::Child);
 }
 if(c.phase==31){
  if(f.present!=0){c.phase=32;return ResourceStep(c,ResourceNeed::Recurse);}
  c.index+=1u64;c.phase=30;return ResourceStep(c,ResourceNeed::Next);
 }
 if(c.phase==32){c.index+=1u64;c.phase=30;return ResourceStep(c,ResourceNeed::Next);}
 return ResourceStep(c,ResourceNeed::Next);
}
