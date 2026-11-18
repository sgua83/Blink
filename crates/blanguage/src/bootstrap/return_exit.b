module bootstrap.return_exit;
extern "C" Cursor start() on cpu {return Cursor(0,0,0u64,0u64);}
extern "C" StatementStep statement_step(Cursor state,StatementFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(facts.context==0){return StatementStep(c,StatementNeed::NoContext);}
  if(facts.expected==0){return StatementStep(c,facts.expression!=0 ? StatementNeed::VoidValue : StatementNeed::Finish);}
  if(facts.expected==1){
   if(facts.expression==0){return StatementStep(c,StatementNeed::BadNever);}
   c.phase=5;return StatementStep(c,StatementNeed::EvaluateNever);
  }
  if(facts.expression==0){return StatementStep(c,StatementNeed::EmptyValue);}
  if(facts.expected==2){c.phase=1;return StatementStep(c,StatementNeed::EvaluateValue);}
  c.phase=3;return StatementStep(c,StatementNeed::EvaluateDataset);
 }
 if(c.phase==1){
  if(facts.continuing==0){return StatementStep(c,StatementNeed::Complete);}
  if(facts.present==0){return StatementStep(c,StatementNeed::MissingValue);}
  c.phase=2;return StatementStep(c,StatementNeed::CheckValue);
 }
 if(c.phase==3){
  if(facts.continuing==0){return StatementStep(c,StatementNeed::Complete);}
  if(facts.present==0||facts.kind!=2){return StatementStep(c,StatementNeed::MissingDataset);}
  c.phase=2;return StatementStep(c,StatementNeed::CheckDataset);
 }
 if(c.phase==2){return StatementStep(c,StatementNeed::Finish);}
 if(c.phase==5){return StatementStep(c,facts.continuing==0 ? StatementNeed::Complete : StatementNeed::BadNever);}
 return StatementStep(c,StatementNeed::Next);
}
extern "C" CollectStep collect_step(Cursor state,CollectFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(c.index>=f.parameters){return CollectStep(c,CollectNeed::Complete);}
  c.phase=1;return CollectStep(c,CollectNeed::Parameter);
 }
 if(c.phase==1){
  if(f.dataset!=0&&f.writable!=0&&f.binding==1){c.phase=2;return CollectStep(c,CollectNeed::Data);}
  if(f.dataset!=0&&f.write!=0&&f.binding==2){c.phase=10;return CollectStep(c,CollectNeed::View);}
  if(f.writable!=0&&f.dataset==0&&f.tile==0){
   if(f.declared!=0){c.phase=20;return CollectStep(c,CollectNeed::Place);}
   c.phase=22;return CollectStep(c,CollectNeed::Written);
  }
  c.index+=1u64;c.phase=0;return CollectStep(c,CollectNeed::Next);
 }
 if(c.phase==2){
  if(f.declared!=0){
   if(f.snapshot==0){return CollectStep(c,CollectNeed::Snapshot);}
   c.phase=3;return CollectStep(c,CollectNeed::DataGuards);
  }
  c.phase=4;return CollectStep(c,CollectNeed::DataInitialized);
 }
 if(c.phase==3){c.phase=7;return CollectStep(c,CollectNeed::DataCheck);}
 if(c.phase==4){
  if(f.initialized==0){c.phase=5;return CollectStep(c,CollectNeed::DataDefer);}
  c.phase=8;return CollectStep(c,CollectNeed::InsertData);
 }
 if(c.phase==5){
  if(f.deferred==0){return CollectStep(c,CollectNeed::BadData);}
  c.phase=8;return CollectStep(c,CollectNeed::InsertData);
 }
 if(c.phase==7){c.phase=8;return CollectStep(c,CollectNeed::InsertData);}
 if(c.phase==8){c.index+=1u64;c.phase=0;return CollectStep(c,CollectNeed::Next);}
 if(c.phase==10){
  if(f.snapshot==0){return CollectStep(c,CollectNeed::Snapshot);}
  c.phase=11;return CollectStep(c,CollectNeed::ViewGuards);
 }
 if(c.phase==11){c.part=0u64;c.phase=12;return CollectStep(c,CollectNeed::ViewPath);}
 if(c.phase==12){
  if(c.part>=f.guards){c.phase=18;return CollectStep(c,CollectNeed::RestorePath);}
  c.phase=13;return CollectStep(c,CollectNeed::GuardPath);
 }
 if(c.phase==13){
  if(f.possible!=0){c.phase=14;return CollectStep(c,CollectNeed::GuardInitialized);}
  c.part+=1u64;c.phase=12;return CollectStep(c,CollectNeed::Next);
 }
 if(c.phase==14){
  if(f.initialized==0){c.phase=15;return CollectStep(c,CollectNeed::GuardDefer);}
  c.part+=1u64;c.phase=12;return CollectStep(c,CollectNeed::Next);
 }
 if(c.phase==15){
  if(f.deferred==0){return CollectStep(c,CollectNeed::BadGuard);}
  c.part+=1u64;c.phase=12;return CollectStep(c,CollectNeed::Next);
 }
 if(c.phase==18){c.index+=1u64;c.phase=0;return CollectStep(c,CollectNeed::Next);}
 if(c.phase==20){
  if(f.snapshot==0){return CollectStep(c,CollectNeed::Snapshot);}
  c.phase=21;return CollectStep(c,CollectNeed::ValueGuards);
 }
 if(c.phase==21){c.phase=22;return CollectStep(c,CollectNeed::Conditional);}
 if(c.phase==22){c.phase=23;return CollectStep(c,CollectNeed::Normalize);}
 if(c.phase==23){c.phase=24;return CollectStep(c,CollectNeed::InsertValue);}
 if(c.phase==24){c.index+=1u64;c.phase=0;return CollectStep(c,CollectNeed::Next);}
 return CollectStep(c,CollectNeed::Next);
}
extern "C" CheckStep check_step(Cursor state,CheckFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return CheckStep(c,CheckNeed::Output);}
 if(c.phase==1){
  if(f.present==0){c.phase=4;return CheckStep(c,CheckNeed::Reset);}
  c.phase=0;
  if(f.kind==2){return CheckStep(c,CheckNeed::OwnData);}
  if(f.kind==3){return CheckStep(c,CheckNeed::OwnValue);}
  return CheckStep(c,CheckNeed::Next);
 }
 if(c.phase==4){c.phase=5;return CheckStep(c,CheckNeed::Output);}
 if(c.phase==5){
  if(f.present==0){c.phase=8;return CheckStep(c,CheckNeed::Reset);}
  c.phase=4;return CheckStep(c,CheckNeed::Elements);
 }
 if(c.phase==8){c.phase=9;return CheckStep(c,CheckNeed::Output);}
 if(c.phase==9){
  if(f.present==0){return CheckStep(c,CheckNeed::Complete);}
  c.phase=8;
  if(f.kind==1||f.kind==3){return CheckStep(c,CheckNeed::View);}
  return CheckStep(c,CheckNeed::Next);
 }
 return CheckStep(c,CheckNeed::Next);
}
extern "C" FinishStep finish_step(Cursor state,FinishFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return FinishStep(c,f.present!=0 ? FinishNeed::Initialized : FinishNeed::Next);}
 if(c.phase==1){
  c.phase=2;
  if(f.present!=0&&f.kind==1){return FinishStep(c,FinishNeed::Value);}
  if(f.present!=0&&f.kind==3){return FinishStep(c,FinishNeed::Owned);}
  return FinishStep(c,FinishNeed::Next);
 }
 if(c.phase==2){c.phase=3;return FinishStep(c,FinishNeed::Borrowed);}
 if(c.phase==3){c.phase=4;return FinishStep(c,FinishNeed::Outputs);}
 if(c.phase==4){c.phase=5;return FinishStep(c,FinishNeed::Environment);}
 if(c.phase==5){c.phase=6;return FinishStep(c,FinishNeed::ViewUses);}
 if(c.phase==6){c.phase=7;return FinishStep(c,f.present!=0&&(f.kind==2||f.kind==3) ? FinishNeed::Hold : FinishNeed::Next);}
 if(c.phase==7){c.phase=8;return FinishStep(c,FinishNeed::Release);}
 if(c.phase==8){c.phase=9;return FinishStep(c,f.pending!=0 ? FinishNeed::Remove : FinishNeed::Next);}
 if(c.phase==9){c.phase=10;return FinishStep(c,FinishNeed::Initialization);}
 if(c.phase==10){c.phase=11;return FinishStep(c,FinishNeed::Borrow);}
 if(c.phase==11){c.phase=12;return FinishStep(c,f.task!=0&&f.view!=0 ? FinishNeed::Publish : FinishNeed::Next);}
 if(c.phase==12){c.phase=13;c.index=0u64;return FinishStep(c,FinishNeed::Context);}
 if(c.phase==13){
  if(f.scoped!=0){
   if(f.present!=0&&c.index<f.slots&&c.index<f.registers){c.phase=14;return FinishStep(c,FinishNeed::Store);}
   c.phase=15;return FinishStep(c,FinishNeed::Exit);
  }
  c.phase=16;return FinishStep(c,FinishNeed::Push);
 }
 if(c.phase==14){c.index+=1u64;c.phase=13;return FinishStep(c,FinishNeed::Next);}
 if(c.phase==15){c.phase=16;return FinishStep(c,FinishNeed::Push);}
 if(c.phase==16){c.phase=17;return FinishStep(c,FinishNeed::Flow);}
 if(c.phase==17){return FinishStep(c,FinishNeed::Complete);}
 return FinishStep(c,FinishNeed::Next);
}
extern "C" InitStep init_step(Cursor state,InitFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(f.invocation==0||c.index>=f.parameters){return InitStep(c,InitNeed::Effects);}
  c.phase=1;return InitStep(c,InitNeed::Parameter);
 }
 if(c.phase==1){
  if(f.dataset!=0&&f.write!=0&&f.data!=0){c.phase=2;return InitStep(c,InitNeed::Project);}
  c.index+=1u64;c.phase=0;return InitStep(c,InitNeed::Next);
 }
 if(c.phase==2){c.index+=1u64;c.phase=0;return InitStep(c,InitNeed::Next);}
 return InitStep(c,InitNeed::Next);
}
extern "C" u32 keep_temporary(u32 temporary,u32 owner,u32 may_own) on cpu {
 return temporary==0||(owner!=0&&may_own!=0) ? 1 : 0;
}
