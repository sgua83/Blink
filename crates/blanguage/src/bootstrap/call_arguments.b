module bootstrap.call_arguments;
bool reads(CallAccess access) on cpu {return access==CallAccess::Read||access==CallAccess::InOut||access==CallAccess::Atomic;}
bool writes(CallAccess access) on cpu {return access==CallAccess::Write||access==CallAccess::InOut||access==CallAccess::Atomic;}
bool retained_consumer(CallAccess access,ArgumentKind kind,u32 scoped_dataset) on cpu {
 if(access==CallAccess::Value){return kind==ArgumentKind::Data||kind==ArgumentKind::OwnedValue;}
 return writes(access)&&(kind==ArgumentKind::OwnedReference||kind==ArgumentKind::BorrowedData
  ||kind==ArgumentKind::Data||scoped_dataset!=0);
}
extern "C" Phase source_start() on cpu {return Phase(0);}
extern "C" SourceNeed source_step(inout Phase cursor,Parameter parameter,SourceFacts facts) on cpu {
 if(cursor.phase==0){
  if(parameter.form==ParameterForm::Acceleration){cursor.phase=8;return SourceNeed::Acceleration;}
  if(parameter.form==ParameterForm::Texture){return SourceNeed::Texture;}
  if(parameter.form==ParameterForm::Dataset){
   if(parameter.access==CallAccess::Value){cursor.phase=1;return SourceNeed::EvaluateDataset;}
   cursor.phase=2;return SourceNeed::ApplicationReference;
  }
  if(parameter.form==ParameterForm::Tile){return SourceNeed::Tile;}
  if(parameter.access==CallAccess::Atomic){return SourceNeed::Atomic;}
  if(parameter.access==CallAccess::Write||parameter.access==CallAccess::InOut){return SourceNeed::Mutable;}
  cursor.phase=4;return SourceNeed::Expected;
 }
 if(cursor.phase==8){return facts.present!=0 ? SourceNeed::ReturnAcceleration : SourceNeed::InvalidAcceleration;}
 if(cursor.phase==1){
  if(facts.continuing==0){return SourceNeed::Empty;}
  return facts.dataset!=0 ? SourceNeed::ReturnDataset : SourceNeed::MissingDataset;
 }
 if(cursor.phase==2){
  if(facts.present!=0){return SourceNeed::ReturnApplicationReference;}
  cursor.phase=3;return SourceNeed::DatasetReference;
 }
 if(cursor.phase==3){return facts.present!=0 ? SourceNeed::ReturnDatasetReference : SourceNeed::Scoped;}
 if(cursor.phase==4){
  if(parameter.access==CallAccess::Read){cursor.phase=5;return SourceNeed::Read;}
  cursor.phase=6;return SourceNeed::Value;
 }
 if(facts.continuing==0){return SourceNeed::Empty;}
 if(facts.present==0){return SourceNeed::MissingValue;}
 return cursor.phase==5 ? SourceNeed::ReturnRead : SourceNeed::ReturnValue;
}
extern "C" BuildCursor build_start(u64 count,u64 parameters) on cpu {return BuildCursor(0u64,count==parameters ? 0 : 99,0);}
extern "C" BuildNeed build_step(inout BuildCursor cursor,u64 count,BuildFacts facts) on cpu {
 while(true){
  if(cursor.phase==99){return BuildNeed::CountMismatch;}
  if(cursor.phase==0){
   if(cursor.index>=count){cursor.phase=10;return BuildNeed::Finish;}
   cursor.phase=1;return BuildNeed::Inspect;
  }
  if(cursor.phase==1){cursor.phase=2;return BuildNeed::Before;}
  if(cursor.phase==2){cursor.phase=3;return BuildNeed::Lower;}
  if(cursor.phase==3){
   if(facts.continuing==0||facts.present==0){cursor.phase=90;return BuildNeed::Abandon;}
   if(retained_consumer(facts.parameter.access,facts.kind,facts.scoped_dataset)){cursor.retained=1;}
   cursor.phase=4;return BuildNeed::Append;
  }
  if(cursor.phase==4){
   if(facts.parameter.form==ParameterForm::Dataset&&facts.kind==ArgumentKind::BorrowedData){
    if(facts.parameter.access==CallAccess::Write){cursor.phase=6;return BuildNeed::PrepareOutput;}
    if(facts.parameter.access==CallAccess::InOut){cursor.phase=5;return BuildNeed::OutputDeclared;}
   }
   cursor.phase=6;
  }
  if(cursor.phase==5){
   cursor.phase=6;if(facts.declared!=0){return BuildNeed::PrepareOutput;}
  }
  if(cursor.phase==6){cursor.phase=7;return BuildNeed::Hold;}
  if(cursor.phase==7){
   cursor.phase=8;if(facts.held!=0){return BuildNeed::Remember;}
  }
  if(cursor.phase==8){cursor.index+=1u64;cursor.phase=0;}
  if(cursor.phase==10){return cursor.retained!=0 ? BuildNeed::ReadyRetained : BuildNeed::Ready;}
  if(cursor.phase==90){return BuildNeed::Empty;}
 }
}
extern "C" Phase hold_start() on cpu {return Phase(0);}
extern "C" HoldNeed hold_step(inout Phase cursor,CallAccess access,HoldFacts facts) on cpu {
 if(cursor.phase==0){
  cursor.phase=1;
  if(facts.kind==ArgumentKind::Reference){return reads(access) ? HoldNeed::ReadReference : HoldNeed::ClearReference;}
 }
 if(facts.kind==ArgumentKind::BorrowedData){return HoldNeed::DataReference;}
 if(facts.kind==ArgumentKind::Reference){return HoldNeed::Reference;}
 if(facts.kind==ArgumentKind::Atomic&&facts.application_element!=0){return HoldNeed::Atomic;}
 if(facts.kind==ArgumentKind::Value&&facts.borrows!=0){return HoldNeed::Value;}
 if(facts.kind==ArgumentKind::OwnedValue){return HoldNeed::OwnedValue;}
 if(facts.kind==ArgumentKind::OwnedReference){
  if(access==CallAccess::Read){return HoldNeed::ReadOwnedReference;}
  return facts.owner!=0 ? HoldNeed::Existing : HoldNeed::BorrowOwnedReference;
 }
 if(facts.kind==ArgumentKind::Data&&access==CallAccess::Value){return HoldNeed::Data;}
 if(facts.kind==ArgumentKind::Scoped){return HoldNeed::Existing;}
 return HoldNeed::None;
}
extern "C" Cursor finish_start() on cpu {return Cursor(0u64,0);}
extern "C" FinishNeed finish_step(inout Cursor cursor,u64 count,FinishFacts facts) on cpu {
 while(true){
  if(cursor.phase==0){
   if(cursor.index>=count){return FinishNeed::Complete;}
   cursor.phase=1;return FinishNeed::Inspect;
  }
  if(cursor.phase==1){
   if(facts.kind==ArgumentKind::Scoped||(facts.kind==ArgumentKind::OwnedReference&&facts.owner!=0)){
    cursor.index+=1u64;cursor.phase=0;
   }else{cursor.phase=2;return FinishNeed::Remove;}
  }
  if(cursor.phase==2){
   cursor.phase=3;
   if(facts.kind==ArgumentKind::OwnedReference||facts.binding==BindingKind::DataReference||facts.binding==BindingKind::Atomic){return FinishNeed::Discard;}
   if(facts.binding==BindingKind::Owned){return FinishNeed::Reference;}
   if(facts.binding==BindingKind::Data){return FinishNeed::Data;}
   if(facts.binding==BindingKind::OwnedValue){return FinishNeed::OwnedValue;}
   if(facts.binding==BindingKind::Value){return FinishNeed::Value;}
   return FinishNeed::Invalid;
  }
  if(cursor.phase==3){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" Cursor collect_start() on cpu {return Cursor(0u64,0);}
extern "C" CollectNeed collect_step(inout Cursor cursor,u64 count,CollectFacts facts) on cpu {
 while(true){
  if(cursor.phase==0){
   if(cursor.index>=count){return CollectNeed::Complete;}
   cursor.phase=1;return CollectNeed::Inspect;
  }
  if(cursor.phase==1){
   cursor.phase=2;
   if(facts.kind==ArgumentKind::Scoped||(facts.kind==ArgumentKind::OwnedReference&&facts.owner!=0)){return CollectNeed::Scoped;}
  }
  if(cursor.phase==2){
   cursor.phase=3;
   if(writes(facts.access)){
    if(facts.kind==ArgumentKind::OwnedReference){return CollectNeed::TransferOwned;}
    if(facts.kind==ArgumentKind::BorrowedData){return CollectNeed::TransferData;}
   }
  }
  if(cursor.phase==3){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" Cursor retain_start() on cpu {return Cursor(0u64,0);}
extern "C" RetainNeed retain_step(inout Cursor cursor,u64 count,RetainFacts facts) on cpu {
 while(true){
  if(cursor.phase==0){cursor.phase=1;return RetainNeed::NextEnvironment;}
  if(cursor.phase==1){
   if(facts.present==0){cursor.index=0u64;cursor.phase=2;}
   else{cursor.phase=0;if(facts.scoped==0){return RetainNeed::Environment;}}
  }
  if(cursor.phase==2){
   if(cursor.index>=count){return RetainNeed::Complete;}
   cursor.phase=3;return RetainNeed::InspectArgument;
  }
  if(cursor.phase==3){
   if(facts.kind==ArgumentKind::Reference){cursor.phase=4;return RetainNeed::Reference;}
   cursor.index+=1u64;cursor.phase=2;
  }
  if(cursor.phase==4){cursor.index+=1u64;cursor.phase=2;}
 }
}
extern "C" CheckCursor checks_start() on cpu {return CheckCursor(0u64,0u64,0);}
extern "C" CheckNeed checks_step(inout CheckCursor cursor,u64 count,CheckFacts facts) on cpu {
 while(true){
  if(cursor.phase==0){
   if(cursor.index>=count){cursor.index=0u64;cursor.phase=4;return CheckNeed::Invocation;}
   cursor.phase=1;return CheckNeed::Inspect;
  }
  if(cursor.phase==1){
   if(facts.access==CallAccess::Value){
    if(facts.argument.kind==ArgumentKind::Data){cursor.phase=2;return CheckNeed::OwnedData;}
    if(facts.argument.kind==ArgumentKind::OwnedValue){cursor.part=0u64;cursor.phase=3;return CheckNeed::OwnedResources;}
   }
   cursor.index+=1u64;cursor.phase=0;
  }
  if(cursor.phase==2){cursor.index+=1u64;cursor.phase=0;}
  if(cursor.phase==3){
   if(cursor.part<facts.resources){cursor.phase=20;return CheckNeed::OwnedResource;}
   cursor.index+=1u64;cursor.phase=0;
  }
  if(cursor.phase==20){cursor.part+=1u64;cursor.phase=3;}
  if(cursor.phase==4){
   if(cursor.index>=count){cursor.phase=90;return CheckNeed::Aliases;}
   cursor.phase=5;return CheckNeed::Inspect;
  }
  if(cursor.phase==5){
   cursor.phase=7;
   if(facts.argument.kind==ArgumentKind::OwnedReference&&writes(facts.access)){cursor.part=0u64;cursor.phase=6;return CheckNeed::MutableResources;}
  }
  if(cursor.phase==6){
   if(cursor.part<facts.resources){cursor.phase=21;return CheckNeed::MutableResource;}
   cursor.phase=7;
  }
  if(cursor.phase==21){cursor.part+=1u64;cursor.phase=6;}
  if(cursor.phase==7){
   if(facts.argument.kind==ArgumentKind::BorrowedData){cursor.phase=8;return CheckNeed::Tasks;}
   cursor.phase=12;
  }
  if(cursor.phase==8){
   cursor.phase=9;if(facts.tasks!=0){return CheckNeed::TaskPosition;}
  }
  if(cursor.phase==9){
   if(writes(facts.access)){cursor.phase=10;return CheckNeed::DataSources;}
   cursor.part=0u64;cursor.phase=11;
  }
  if(cursor.phase==10){cursor.part=0u64;cursor.phase=30;return CheckNeed::ResourcePaths;}
  if(cursor.phase==30){
   if(cursor.part<facts.paths){cursor.phase=31;return CheckNeed::FieldResource;}
   cursor.part=0u64;cursor.phase=11;
  }
  if(cursor.phase==31){cursor.part+=1u64;cursor.phase=30;}
  if(cursor.phase==11){
   if(cursor.part<facts.argument.sources){cursor.phase=32;return CheckNeed::ReferenceSource;}
   cursor.phase=12;
  }
  if(cursor.phase==32){cursor.part+=1u64;cursor.phase=11;}
  if(cursor.phase==12){
   if(facts.argument.kind==ArgumentKind::Reference||facts.argument.kind==ArgumentKind::Atomic){cursor.phase=13;return CheckNeed::PlaceAccess;}
   cursor.phase=15;
  }
  if(cursor.phase==13){
   if(facts.argument.application_element!=0){cursor.part=0u64;cursor.phase=14;return CheckNeed::PlaceIdentity;}
   cursor.phase=15;
  }
  if(cursor.phase==14){
   if(cursor.part<facts.argument.sources){cursor.phase=33;return CheckNeed::PlaceSource;}
   cursor.phase=15;
  }
  if(cursor.phase==33){cursor.part+=1u64;cursor.phase=14;}
  if(cursor.phase==15){
   if(facts.access!=CallAccess::Value&&(facts.argument.kind==ArgumentKind::Data||facts.argument.scoped_dataset!=0)){
    cursor.phase=16;if(writes(facts.access)){return CheckNeed::HandleExclusion;}
   }else{cursor.index+=1u64;cursor.phase=4;}
  }
  if(cursor.phase==16){cursor.phase=17;return CheckNeed::HandleAvailable;}
  if(cursor.phase==17){cursor.index+=1u64;cursor.phase=4;}
  if(cursor.phase==90){return CheckNeed::Complete;}
 }
}
