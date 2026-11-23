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

extern "C" Optional<Text> read_input_root(SignatureNode source) on cpu {
 u64 offset=source.offset;
 while(true){
  ExprNode expression=unsafe_position<ExprNode>(unsafe_address(address_bits(source.storage.nodes)+offset));
  match(expression.kind){
   ExprKind::Name(name){return Optional<Text>(1,name);}
   ExprKind::Member(value){offset=value.base.offset;}
   ExprKind::Index(value){offset=value.base.offset;}
   ExprKind::Number(unused){break;} ExprKind::StaticU32(unused){break;} ExprKind::Bool(unused){break;} ExprKind::Call(unused){break;} ExprKind::Invoke(unused){break;} ExprKind::Construct(unused){break;} ExprKind::Specialize(unused){break;} ExprKind::FunctionRef(unused){break;} ExprKind::Cooperate(unused){break;} ExprKind::Target(unused){break;} ExprKind::Unary(unused){break;} ExprKind::Binary(unused){break;} ExprKind::Select(unused){break;}
  }
 }
 return Optional<Text>(0,Text(0,Span(0u64,0u64)));
}
extern "C" ReadInputStep read_input_step(Phase cursor,ReadInputFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){c.phase=1;return ReadInputStep(c,ReadInputNeed::Reference);}
 if(c.phase==1){if(facts.present!=0){return ReadInputStep(c,ReadInputNeed::BorrowedData);}c.phase=2;return ReadInputStep(c,ReadInputNeed::Root);}
 if(c.phase==2){if(facts.bound!=0){c.phase=5;return ReadInputStep(c,ReadInputNeed::Type);}c.phase=3;return ReadInputStep(c,ReadInputNeed::IsPlace);}
 if(c.phase==3){if(facts.place!=0){c.phase=4;return ReadInputStep(c,ReadInputNeed::Temporary);}c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}
 if(c.phase==4){if(facts.temporary!=0){c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}c.phase=5;return ReadInputStep(c,ReadInputNeed::Type);}
 if(c.phase==5){if(facts.owns!=0){c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}c.phase=6;return ReadInputStep(c,ReadInputNeed::Binding);}
 if(c.phase==6){if(facts.value!=0){c.phase=7;return ReadInputStep(c,ReadInputNeed::Materialize);}c.phase=8;return ReadInputStep(c,ReadInputNeed::Place);}
 if(c.phase==7){c.phase=8;return ReadInputStep(c,ReadInputNeed::Place);}
 if(c.phase==8){if(facts.present==0){return ReadInputStep(c,ReadInputNeed::Empty);}c.phase=10;return ReadInputStep(c,ReadInputNeed::Storage);}
 if(c.phase==10){if(facts.storage==1){c.phase=12;return ReadInputStep(c,ReadInputNeed::Initialize);}if(facts.storage==2){c.phase=11;return ReadInputStep(c,ReadInputNeed::Writable);}return ReadInputStep(c,ReadInputNeed::Borrow);}
 if(c.phase==11){if(facts.writable!=0){c.phase=12;return ReadInputStep(c,ReadInputNeed::Initialize);}return ReadInputStep(c,ReadInputNeed::Borrow);}
 if(c.phase==12){return ReadInputStep(c,ReadInputNeed::Borrow);}
 if(c.phase==9){if(facts.present==0){return ReadInputStep(c,ReadInputNeed::Empty);}if(facts.owner!=0){return ReadInputStep(c,ReadInputNeed::Scoped);}if(facts.kind==3){c.phase=14;return ReadInputStep(c,ReadInputNeed::Application);}return ReadInputStep(c,ReadInputNeed::Resource);}
 return ReadInputStep(c,facts.application!=0 ? ReadInputNeed::OwnedReference : ReadInputNeed::Resource);
}

extern "C" MutableInputStep mutable_input_step(Phase cursor,MutableInputFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){c.phase=1;return MutableInputStep(c,MutableInputNeed::Reference);}
 if(c.phase==1){if(facts.present!=0){return MutableInputStep(c,MutableInputNeed::Borrowed);}c.phase=2;return MutableInputStep(c,MutableInputNeed::IsPlace);}
 if(c.phase==2){if(facts.place!=0){c.phase=3;return MutableInputStep(c,MutableInputNeed::Place);}c.phase=5;return MutableInputStep(c,MutableInputNeed::Evaluate);}
 if(c.phase==3){if(facts.present==0){return MutableInputStep(c,MutableInputNeed::Empty);}c.phase=4;return MutableInputStep(c,MutableInputNeed::Dataset);}
 if(c.phase==4){if(facts.dataset!=0){return MutableInputStep(c,MutableInputNeed::Application);}c.phase=6;return MutableInputStep(c,MutableInputNeed::Owns);}
 if(c.phase==6){return MutableInputStep(c,facts.owns!=0 ? MutableInputNeed::Owned : MutableInputNeed::Plain);}
 return MutableInputStep(c,facts.continuing!=0 ? MutableInputNeed::Returned : MutableInputNeed::Empty);
}
extern "C" MutableValueStep mutable_value_step(Phase cursor,u32 kind) on cpu {
 Phase c=cursor;
 if(c.phase==0){if(kind==2){c.phase=1;return MutableValueStep(c,MutableValueNeed::HoldData);}if(kind==3){c.phase=3;return MutableValueStep(c,MutableValueNeed::BindOwner);}return MutableValueStep(c,MutableValueNeed::Invalid);}
 if(c.phase==1){c.phase=2;return MutableValueStep(c,MutableValueNeed::BorrowData);}
 if(c.phase==2){return MutableValueStep(c,MutableValueNeed::Scoped);}
 if(c.phase==3){c.phase=4;return MutableValueStep(c,MutableValueNeed::OwnerPlace);}
 if(c.phase==4){c.phase=5;return MutableValueStep(c,MutableValueNeed::HoldOwner);}
 return MutableValueStep(c,MutableValueNeed::Reference);
}
extern "C" OwnedInputStep owned_input_step(Phase cursor,OwnedInputFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){c.phase=1;return OwnedInputStep(c,facts.application!=0 ? OwnedInputNeed::Application : OwnedInputNeed::Model,reads(facts.access) ? 1 : 0);}
 if(c.phase==1){if(facts.borrowed!=0){return OwnedInputStep(c,OwnedInputNeed::Borrowed,0);}c.phase=2;return OwnedInputStep(c,OwnedInputNeed::Prepare,0);}
 if(c.phase==2){if(facts.access==CallAccess::InOut){c.phase=3;return OwnedInputStep(c,OwnedInputNeed::Available,0);}return OwnedInputStep(c,OwnedInputNeed::Reference,0);}
 if(c.phase==3){if(facts.available==0){return OwnedInputStep(c,OwnedInputNeed::Unavailable,0);}c.phase=4;return OwnedInputStep(c,OwnedInputNeed::Load,0);}
 return OwnedInputStep(c,OwnedInputNeed::Reference,0);
}

extern "C" CallReferenceStep call_reference_step(Phase cursor,CallReferenceFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){if(facts.atomic!=0&&facts.access!=CallAccess::Atomic){return CallReferenceStep(c,CallReferenceNeed::BadAtomic);}c.phase=1;return CallReferenceStep(c,CallReferenceNeed::Kind);}
 if(c.phase==1){
  if(facts.kind==ReferenceKind::Indexed){c.phase=2;return CallReferenceStep(c,CallReferenceNeed::Base);}
  if(facts.kind==ReferenceKind::Element){c.phase=3;return CallReferenceStep(c,CallReferenceNeed::ElementFacts);}
  c.phase=10;
  if(facts.kind==ReferenceKind::Reference){return CallReferenceStep(c,CallReferenceNeed::Reference);}if(facts.kind==ReferenceKind::Local){return CallReferenceStep(c,CallReferenceNeed::Local);}if(facts.kind==ReferenceKind::Application){return CallReferenceStep(c,CallReferenceNeed::Application);}if(facts.kind==ReferenceKind::View){return CallReferenceStep(c,CallReferenceNeed::View);}return CallReferenceStep(c,CallReferenceNeed::BadStorage);
 }
 if(c.phase==2){
  if(facts.base==ReferenceKind::Element){c.phase=3;return CallReferenceStep(c,CallReferenceNeed::ElementFacts);}c.phase=10;
  if(facts.base==ReferenceKind::Reference){return CallReferenceStep(c,CallReferenceNeed::IndexedReference);}if(facts.base==ReferenceKind::Local){return CallReferenceStep(c,CallReferenceNeed::IndexedLocal);}if(facts.base==ReferenceKind::Application){return CallReferenceStep(c,CallReferenceNeed::IndexedApplication);}if(facts.base==ReferenceKind::View){return CallReferenceStep(c,CallReferenceNeed::IndexedView);}return CallReferenceStep(c,CallReferenceNeed::BadStorage);
 }
 if(c.phase==3){c.phase=4;if(reads(facts.access)){return CallReferenceStep(c,CallReferenceNeed::Read);}}
 if(c.phase==4){c.phase=5;if(writes(facts.access)){return CallReferenceStep(c,CallReferenceNeed::Write);}}
 if(c.phase==5){c.phase=6;if(facts.cross!=0){return CallReferenceStep(c,CallReferenceNeed::Cross);}}
 if(c.phase==6){c.phase=10;return CallReferenceStep(c,facts.kind==ReferenceKind::Indexed ? CallReferenceNeed::IndexedElement : CallReferenceNeed::Element);}
 if(c.phase==10){c.phase=11;return CallReferenceStep(c,CallReferenceNeed::DataAccess);}
 return CallReferenceStep(c,CallReferenceNeed::Complete);
}

extern "C" CCallStep c_call_step(CCallCursor cursor,CCallFacts facts,u64 arguments,u64 written,u64 held) on cpu {
 CCallCursor c=cursor;
 if(c.phase==0){if(facts.cpu==0){return CCallStep(c,CCallNeed::BadTarget,0);}c.phase=1;return CCallStep(c,CCallNeed::Inputs,0);}
 if(c.phase==1){if(c.index<arguments){c.phase=2;return CCallStep(c,CCallNeed::Inspect,0);}c.phase=20;if(facts.contract!=0){return CCallStep(c,CCallNeed::Prove,0);}}
 if(c.phase==2){c.phase=3;if(facts.kind==CCallArgument::Scoped){return CCallStep(c,CCallNeed::Scoped,0);}}
 if(c.phase==3){c.phase=4;if(facts.reference!=0&&facts.access==CallAccess::Read&&facts.kind==CCallArgument::Value){return CCallStep(c,CCallNeed::Materialize,0);}}
 if(c.phase==4){
  if(facts.reference==0&&facts.kind==CCallArgument::Value){c.phase=5;return CCallStep(c,CCallNeed::Value,0);}
  if(facts.reference!=0&&facts.kind==CCallArgument::Reference){c.phase=10;return CCallStep(c,CCallNeed::Reference,reads(facts.access) ? 1 : 0);}
  return CCallStep(c,CCallNeed::Invalid,0);
 }
 if(c.phase==5){c.phase=6;if(facts.contract!=0&&facts.capture_names!=0){return CCallStep(c,CCallNeed::CaptureName,0);}}
 if(c.phase==6){c.index+=1u64;c.phase=1;return CCallStep(c,facts.captured!=0 ? CCallNeed::CapturedValue : CCallNeed::PlainValue,0);}
 if(c.phase==10){c.phase=11;return CCallStep(c,CCallNeed::Address,0);}
 if(c.phase==11){c.index+=1u64;c.phase=1;return CCallStep(c,writes(facts.access) ? CCallNeed::WrittenReference : CCallNeed::PlainReference,0);}
 if(c.phase==20){c.phase=21;return CCallStep(c,CCallNeed::Registers,0);}
 if(c.phase==21){c.phase=22;return CCallStep(c,CCallNeed::Memory,0);}
 if(c.phase==22){c.phase=23;return CCallStep(c,CCallNeed::Emit,0);}
 if(c.phase==23){if(facts.never!=0){return CCallStep(c,CCallNeed::Stop,0);}c.index=0u64;c.phase=24;return CCallStep(c,CCallNeed::Returned,0);}
 if(c.phase==24){if(c.index<written){c.phase=25;return CCallStep(c,CCallNeed::WrittenInspect,0);}c.index=0u64;c.phase=32;}
 if(c.phase==25){if(facts.contract!=0){c.phase=26;return CCallStep(c,CCallNeed::Guards,0);}c.phase=28;return CCallStep(c,CCallNeed::AllGuards,0);}
 if(c.phase==26){c.phase=28;if(facts.declared!=0){return CCallStep(c,CCallNeed::Forget,0);}}
 if(c.phase==28){c.phase=29;return CCallStep(c,CCallNeed::Assign,0);}
 if(c.phase==29){c.index+=1u64;c.phase=24;return CCallStep(c,CCallNeed::Initialize,0);}
 if(c.index<held){c.index+=1u64;return CCallStep(c,CCallNeed::Finish,0);}
 return CCallStep(c,CCallNeed::Complete,0);
}

extern "C" NativeReferenceStep native_reference_step(CCallCursor cursor,NativeReferenceFacts facts,u64 count) on cpu {
 CCallCursor c=cursor;u32 expose=writes(facts.access)&&facts.expose_writes!=0 ? 1 : 0;
 if(c.phase==0){if(facts.c_abi!=0){return NativeReferenceStep(c,NativeReferenceNeed::CAbi,expose);}c.phase=1;return NativeReferenceStep(c,NativeReferenceNeed::Layout,expose);}
 if(c.phase==1){if(facts.form==0){return NativeReferenceStep(c,NativeReferenceNeed::Empty,expose);}if(facts.form==1){return NativeReferenceStep(c,NativeReferenceNeed::Whole,expose);}c.phase=2;return NativeReferenceStep(c,NativeReferenceNeed::Components,expose);}
 if(c.phase==2){if(facts.components!=0){c.phase=3;}else{c.phase=4;return NativeReferenceStep(c,NativeReferenceNeed::Words,expose);}}
 if(c.phase==3){if(c.index<count){c.index+=1u64;return NativeReferenceStep(c,NativeReferenceNeed::Part,expose);}return NativeReferenceStep(c,NativeReferenceNeed::Complete,expose);}
 if(c.phase==4){if(facts.present==0){return NativeReferenceStep(c,NativeReferenceNeed::Invalid,expose);}c.phase=5;return NativeReferenceStep(c,NativeReferenceNeed::Base,expose);}
 if(c.index<count){c.index+=1u64;return NativeReferenceStep(c,NativeReferenceNeed::Word,expose);}
 return NativeReferenceStep(c,NativeReferenceNeed::Complete,expose);
}
extern "C" ContiguousStep contiguous_step(Phase cursor,u32 symbolic,u64 components,u32 present) on cpu {
 Phase c=cursor;if(c.phase==0){if(symbolic!=0){return ContiguousStep(c,components==1u64 ? ContiguousNeed::True : ContiguousNeed::Assumed);}c.phase=1;return ContiguousStep(c,ContiguousNeed::Words);}
 return ContiguousStep(c,present!=0 ? ContiguousNeed::Physical : ContiguousNeed::Invalid);
}
extern "C" u32 physical_contiguous(u32 first,u32 alignment,u64 count,address context,cfn<ReferenceOffset,address> next) on cpu
 requires nonzero(alignment) requires nonzero(next) {
 if(first%alignment!=0){return 0;}
 u64 i=0u64;while(i<count){ReferenceOffset part=next(context);if(part.word<first||part.word-first!=part.offset){return 0;}i+=1u64;}
 return 1;
}

extern "C" FunctionValueStep function_value_step(Phase cursor,FunctionValueFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){if(facts.cpu==0){return FunctionValueStep(c,FunctionValueNeed::BadTarget);}c.phase=1;return FunctionValueStep(c,FunctionValueNeed::Lower);}
 if(c.phase==1){if(facts.present==0){return FunctionValueStep(c,FunctionValueNeed::Empty);}c.phase=2;return FunctionValueStep(c,FunctionValueNeed::Kind);}
 if(c.phase==2){if(facts.signature==0){return FunctionValueStep(c,FunctionValueNeed::BadKind);}c.phase=3;return FunctionValueStep(c,FunctionValueNeed::Nonzero);}
 if(c.phase==3){if(facts.nonzero==0){return FunctionValueStep(c,FunctionValueNeed::Zero);}c.phase=4;return FunctionValueStep(c,FunctionValueNeed::Signature);}
 if(c.phase==4){c.phase=5;return FunctionValueStep(c,FunctionValueNeed::Arguments);}
 if(c.phase==5){if(facts.present==0){return FunctionValueStep(c,FunctionValueNeed::Empty);}c.phase=6;return FunctionValueStep(c,FunctionValueNeed::Contracts);}
 return FunctionValueStep(c,facts.contracts!=0 ? FunctionValueNeed::ReadyContract : FunctionValueNeed::ReadyPlain);
}
