module bootstrap.call_arguments;
import bootstrap.proof_facts;
import bootstrap.ir_dataflow;
import bootstrap.names;

extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
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
 return Optional<Text>(0,Text(0,bootstrap.ast_storage::Span(0u64,0u64)));
}
extern "C" ReadInputStep read_input_step(Phase cursor,ReadInputFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){c.phase=1;return ReadInputStep(c,ReadInputNeed::Reference);}
 if(c.phase==1){if(facts.present!=0){return ReadInputStep(c,ReadInputNeed::BorrowedData);}c.phase=2;return ReadInputStep(c,ReadInputNeed::Root);}
 if(c.phase==2){if(facts.bound!=0){c.phase=5;return ReadInputStep(c,ReadInputNeed::Type);}c.phase=3;return ReadInputStep(c,ReadInputNeed::IsPlace);}
 if(c.phase==3){if(facts.place!=0){c.phase=4;return ReadInputStep(c,ReadInputNeed::Temporary);}c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}
 if(c.phase==4){if(facts.temporary!=0){c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}c.phase=5;return ReadInputStep(c,ReadInputNeed::Type);}
 if(c.phase==5){if(facts.owns!=0){c.phase=9;return ReadInputStep(c,ReadInputNeed::Aggregate);}c.phase=8;return ReadInputStep(c,ReadInputNeed::Place);}
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
  bool atomic=facts.access==CallAccess::Atomic;
  if(facts.reference!=0&&((atomic&&facts.kind==CCallArgument::Atomic)||(!atomic&&facts.kind==CCallArgument::Reference))){
   c.phase=10;return CCallStep(c,CCallNeed::Reference,reads(facts.access) ? 1 : 0);
  }
  return CCallStep(c,CCallNeed::Invalid,0);
 }
 if(c.phase==5){c.phase=6;if(facts.contract!=0&&facts.capture_names!=0){return CCallStep(c,CCallNeed::CaptureName,0);}}
 if(c.phase==6){c.index+=1u64;c.phase=1;return CCallStep(c,facts.captured!=0 ? CCallNeed::CapturedValue : CCallNeed::PlainValue,0);}
 if(c.phase==10){c.phase=11;return CCallStep(c,CCallNeed::Address,0);}
 if(c.phase==11){c.index+=1u64;c.phase=1;return CCallStep(c,writes(facts.access)&&facts.access!=CallAccess::Atomic ? CCallNeed::WrittenReference : CCallNeed::PlainReference,0);}
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

extern "C" TakeAddressStep take_address_step(Phase cursor,TakeAddressFacts facts) on cpu {
 Phase c=cursor;CallAccess access=CallAccess::Read;
 if(c.phase==0){if(facts.cpu==0){return TakeAddressStep(c,TakeAddressNeed::BadTarget,access);}if(facts.arguments!=1u64){return TakeAddressStep(c,TakeAddressNeed::BadCount,access);}c.phase=3;return TakeAddressStep(c,TakeAddressNeed::Place,access);}
 if(c.phase==3){if(facts.present==0){return TakeAddressStep(c,TakeAddressNeed::Empty,access);}c.phase=4;return TakeAddressStep(c,TakeAddressNeed::Layout,access);}
 if(c.phase==4){if(facts.owns!=0||facts.views!=0||facts.empty!=0){return TakeAddressStep(c,TakeAddressNeed::BadLayout,access);}c.phase=5;return TakeAddressStep(c,TakeAddressNeed::Origin,access);}
 if(c.phase==5){c.phase=6;return TakeAddressStep(c,TakeAddressNeed::Reference,facts.writable!=0?CallAccess::InOut:CallAccess::Read);}
 if(c.phase==6){c.phase=7;return TakeAddressStep(c,TakeAddressNeed::Register,access);}
 if(c.phase==7){c.phase=8;return TakeAddressStep(c,TakeAddressNeed::Storage,access);}
 if(c.phase==8){if(facts.storage==1){c.phase=9;return TakeAddressStep(c,TakeAddressNeed::LocalOrigin,access);}if(facts.storage==2){c.phase=11;return TakeAddressStep(c,TakeAddressNeed::ReferenceIdentity,access);}c.phase=10;return TakeAddressStep(c,TakeAddressNeed::CopyOrigin,access);}
 if(c.phase==9){c.phase=10;return TakeAddressStep(c,TakeAddressNeed::Tail,access);}
 if(c.phase==10){c.phase=11;return TakeAddressStep(c,TakeAddressNeed::StorageIdentity,access);}
 if(c.phase==11){c.phase=12;return TakeAddressStep(c,TakeAddressNeed::Emit,access);}
 return TakeAddressStep(c,TakeAddressNeed::Complete,access);
}
extern "C" u32 walk_address_sources(u32 unavailable, AddressSourceWalk walk, AddressSourceHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.projection) {
    if (unavailable != 0) { return 0; }
    while (true) {
        let entry = host.next(host.context);
        if (entry.kind == AddressSourceKind::End) { return 1; }
        if (entry.possible == 0) { continue; }
        if (entry.kind == AddressSourceKind::Position) {
            if (host.projection(host.context) == 0) { return 0; }
        } else if (walk != AddressSourceWalk::Write || entry.kind != AddressSourceKind::Inactive) {
            return 0;
        }
    }
}

extern "C" u32 seed_memory_regions(RegionSeed seed, RegionSeedHost host) on cpu
    requires nonzero(host.part) requires nonzero(host.reserve) requires nonzero(host.emit) {
    if (seed.kind == RegionSeedKind::External) { return 0; }
    if (seed.materialize == 0) { return 1; }
    let count = min(seed.parts, seed.kind == RegionSeedKind::Resource ? seed.words : seed.positions);
    if (count == 0u64) { return 1; }
    host.reserve(host.context, count);
    usize slot = 0u64;
    while (slot < count) {
        let part = host.part(host.context, slot);
        host.emit(host.context, slot, RegionBytes(part.word, 0, part.size));
        slot += 1u64;
    }
    return 1;
}

extern "C" u32 project_memory_regions(address origin, usize parts, u32 parent_size, RegionSpan parent,
    RegionHost host) on cpu
    requires nonzero(host.part) requires nonzero(host.region) requires nonzero(host.emit) requires nonzero(host.finish) {
    usize slot=0u64;
    while (slot<parts) {
        RegionBytes part=host.part(origin,slot);
        u64 limit=u64(part.offset)+u64(part.size);
        if (limit>u64(parent_size)) { return 0; }
        usize at=parent.first;
        while (at<parent.end) {
            RegionBytes region=host.region(parent.regions,at);
            u32 first=part.offset>region.word ? part.offset : region.word;
            u32 region_end=region.word+region.size;
            u32 end=u32(limit)<region_end ? u32(limit) : region_end;
            if (first<end) {
                host.emit(parent.regions,at,RegionBytes(
                    part.word+(first-part.offset),
                    region.offset+(first-region.word),
                    end-first));
            }
            at+=1u64;
        }
        slot+=1u64;
    }
    if (parent.first < parent.end) { host.finish(parent); }
    return 1;
}

u8 scalar_write_mask(u32 width) on cpu {
    return u8((1 << width) - 1);
}

extern "C" u8 region_written_bytes(RegionWriteFacts facts, u32 width) on cpu {
    if (facts.kind == RegionWriteKind::Unrelated) { return u8(0); }
    if (facts.kind == RegionWriteKind::Selected) { return scalar_write_mask(width); }
    i64 offset = i64(facts.offset);
    if (facts.kind == RegionWriteKind::Resource) {
        offset += i64(facts.word) - i64(facts.target);
    }
    let first = max(offset, i64(0));
    let end = min(offset + i64(facts.size), i64(width));
    if (first >= end) { return u8(0); }
    let mask = (1 << u32(end - first)) - 1;
    return u8(mask << u32(first));
}

extern "C" u8 project_address_write_bytes(address position, u32 width, WriteProjectionHost host) on cpu
    requires nonzero(host.distinct_element) requires nonzero(host.position) requires nonzero(host.child)
    requires nonzero(host.regions) requires nonzero(host.region) {
    if (host.distinct_element(host.context, position) != 0) { return u8(0); }
    let view = host.position(host.context, position);
    if (view.kind == WritePositionKind::None) { return u8(0); }
    if (view.kind == WritePositionKind::All) { return scalar_write_mask(width); }
    u8 bytes = u8(0);
    if (view.kind == WritePositionKind::Composite) {
        usize index = 0u64;
        while (index < view.children) {
            bytes |= project_address_write_bytes(host.child(position, index), width, host);
            index += 1u64;
        }
    } else {
        let regions = host.regions(host.context);
        if (regions.present == 0) { return scalar_write_mask(width); }
        usize index = 0u64;
        while (index < regions.count) {
            bytes |= region_written_bytes(host.region(regions.data, position, index), width);
            index += 1u64;
        }
    }
    return bytes;
}

extern "C" u32 collect_memory_effect_sources(CallAccess request, EffectSourceHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.possible) requires nonzero(host.external)
    requires nonzero(host.record) {
    u32 external = 0;
    while (true) {
        let source = host.next(host.context);
        if (source.kind == EffectSourceKind::End) { return external; }
        if (source.kind != EffectSourceKind::Position &&
            (source.kind != EffectSourceKind::External || external != 0)) { continue; }
        if (host.possible(host.context) == 0) { continue; }
        if (external == 0) { external = host.external(host.context); }
        if (source.kind == EffectSourceKind::Position) {
            let access = source.writable != 0 ? request : CallAccess::Read;
            host.record(host.context, access, writes(access) ? 1 : 0);
        }
    }
}

extern "C" OptionalMemoryAccess merge_external_memory_access(OptionalMemoryAccess left, OptionalMemoryAccess right) on cpu {
    if (left.present == 0) { return right; }
    if (right.present != 0 && left.access != right.access) { return OptionalMemoryAccess(CallAccess::InOut, 1); }
    return left;
}

extern "C" OptionalMemoryAccess collect_ir_memory_sources(address known, OptionalMemoryAccess external, CallAccess request,
    IrMemorySourceHost host) on cpu requires nonzero(host.next) requires nonzero(host.external) requires nonzero(host.origin) {
    OptionalMemoryAccess result = external;
    let incoming = OptionalMemoryAccess(request, 1);
    while (true) {
        let source = host.next(host.context);
        if (source.kind == EffectSourceKind::End) { return result; }
        let merged = merge_external_memory_access(result, incoming);
        let needs_external = result.present == 0 || merged.access != result.access;
        if (source.kind != EffectSourceKind::Position &&
            (source.kind != EffectSourceKind::External || !needs_external)) { continue; }
        if (needs_external && host.external(host.context) != 0) { result = merged; }
        if (source.kind == EffectSourceKind::Position) {
            host.origin(host.context, known, source.writable != 0 ? request : CallAccess::Read);
        }
    }
}

extern "C" OptionalMemoryAccess collect_closed_ir_memory(address known, usize count, OptionalMemoryAccess external,
    OptionalMemoryAccess incoming, address context, cfn<void,address,address,usize> origin) on cpu requires nonzero(origin) {
    usize index = 0u64;
    while (index < count) {
        origin(context, known, index);
        index += 1u64;
    }
    return merge_external_memory_access(external, incoming);
}

extern "C" void append_ir_memory_aliases(address known, address aliases, usize count, CallAccess access,
    cfn<MemoryIrRow,address,usize,CallAccess> row) on cpu requires nonzero(row) {
    usize index = 0u64;
    while (index < count) {
        let record = row(aliases, index, access);
        push_memory_effect_record(known, size_of<MemoryIrRow>(), address_of(record));
        index += 1u64;
    }
}

MemoryIrRow recorded_ir_access(address known, usize at) on cpu {
    return unsafe_position<MemoryIrRow>(unsafe_address(address_bits(known) + at * size_of<MemoryIrRow>()));
}

extern "C" u32 contains_ir_memory_record(address known, usize count, MemoryIrRow sought) on cpu {
    usize at = 0u64;
    while (at < count) {
        let record = recorded_ir_access(known, at);
        if (record.kind == sought.kind && record.access == sought.access &&
            record.base == sought.base && record.length == sought.length) { return 1; }
        at += 1u64;
    }
    return 0;
}

MemoryIrRow memory_ir_row(MemoryIrRows rows, usize at, MemoryIrRowsHost host) on cpu
    requires nonzero(host.local) {
    if (at < rows.known_count) { return recorded_ir_access(rows.known, at); }
    let local = host.local(host.context);
    return MemoryIrRow(MemoryIrRowKind::Local, rows.external, local.end - local.length, local.length);
}

bool memory_alias_less(array<u64,3> left, array<u64,3> right) on cpu {
    if (left[0] != right[0]) { return left[0] < right[0]; }
    if (left[1] != right[1]) { return left[1] < right[1]; }
    return left[2] < right[2];
}

extern "C" bool memory_alias_before(address left, address right, address context) on cpu {
    return memory_alias_less(unsafe_position<array<u64,3>>(left), unsafe_position<array<u64,3>>(right));
}

extern "C" void materialize_ir_memory(MemoryIrRows rows, MemoryIrOutput format, address output, MemoryIrRowsHost host) on cpu
    requires nonzero(host.local) requires nonzero(host.emit) {
    let count = rows.known_count + (rows.external_present != 0 ? rows.locals : 0u64);
    if (format == MemoryIrOutput::Call || count < 2u64) {
        usize at = 0u64;
        while (at < count) {
            host.emit(output, memory_ir_row(rows, at, host));
            at += 1u64;
        }
        return;
    }

    // Sort the actual pointer keys in the original alias-type, allocation and length order.
    OrderedTree keys = OrderedTree(address(), 0u64, 0u64, 0u64);
    let width = size_of<array<u64,3>>();
    usize at = 0u64;
    while (at < count) {
        let row = memory_ir_row(rows, at, host);
        let key = array<u64,3>(row.kind == MemoryIrRowKind::Local ? 0u64 : 1u64, row.base, row.length);
        push_memory_effect_record(address_of(keys), width, address_of(key));
        at += 1u64;
    }
    sort_rows<array<u64,3>>(keys.records, keys.count, address(), c_function(memory_alias_before));
    array<u64,3> previous = unsafe_position<array<u64,3>>(keys.records);
    at = 0u64;
    while (at < keys.count) {
        let key = unsafe_position<array<u64,3>>(unsafe_address(address_bits(keys.records) + at * width));
        if (at == 0u64 || memory_alias_less(previous, key)) {
            let kind = key[0] == 0u64 ? MemoryIrRowKind::Local : MemoryIrRowKind::Data;
            host.emit(output, MemoryIrRow(kind, CallAccess::Value, key[1], key[2]));
            previous = key;
        }
        at += 1u64;
    }
    fact_tree_clear(address_of(keys));
}

address memory_ir_item(bootstrap.ir_dataflow::Span span, usize index) on cpu {
    return unsafe_address(address_bits(span.data) + index * span.stride);
}

void publish_pointer_memory(bootstrap.ir_dataflow::Node source, address owner, MemoryIrOwner kind,
    address context, MemoryIrHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.pointer) {
    let value = host.ir.inspect(source);
    if (value.tag == INDEXED_POINTER) {
        let register = unsafe_position<usize>(value.first.data);
        host.pointer(context, owner, kind, register);
    }
}

void publish_reference_memory(bootstrap.ir_dataflow::Node reference, address owner, MemoryIrOwner kind,
    address context, MemoryIrHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.pointer) {
    let value = host.ir.inspect(reference);
    let storage = host.ir.inspect(value.left);
    if (storage.tag == STORAGE_INDEXED) { publish_pointer_memory(storage.left, owner, kind, context, host); }
}

extern "C" void publish_ir_memory(bootstrap.ir_dataflow::Span code, address context, MemoryIrHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.call) requires nonzero(host.pointer) requires nonzero(host.child) {
    usize at = 0u64;
    while (at < code.count) {
        let owner = memory_ir_item(code, at);
        let instruction = host.ir.inspect(bootstrap.ir_dataflow::Node(owner, N_INSTRUCTION));
        let op = instruction.tag;
        if (op == OP_INVOKE) {
            let args = host.call(context, owner);
            usize index = 0u64;
            while (index < args.count) {
                let argument = memory_ir_item(args, index);
                let view = host.ir.inspect(bootstrap.ir_dataflow::Node(argument, N_ARGUMENT));
                if (view.tag == ARGUMENT_REFERENCE) {
                    publish_reference_memory(view.left, argument, MemoryIrOwner::Argument, context, host);
                }
                index += 1u64;
            }
        } else if (op == OP_LOAD_INDEXED || op == OP_STORE_INDEXED || op == OP_ATOMIC) {
            publish_pointer_memory(instruction.left, owner, MemoryIrOwner::Instruction, context, host);
        } else if (op == OP_POSITION_ADDRESS) {
            publish_reference_memory(instruction.left, owner, MemoryIrOwner::Instruction, context, host);
        } else if (op == OP_SCOPE || op == OP_CLEANUP) {
            publish_ir_memory(host.child(owner, 0), context, host);
        } else if (op == OP_BRANCH || op == OP_LOOP_VALUE) {
            publish_ir_memory(host.child(owner, 0), context, host);
            publish_ir_memory(host.child(owner, 1), context, host);
        }
        at += 1u64;
    }
}

extern "C" u32 same_position_resource(address left, address right, ResourceIdentityHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal) {
    address a = left;
    address b = right;
    while (true) {
        let x = host.inspect(a);
        let y = host.inspect(b);
        if (x.kind == ResourceIdentityKind::Unknown || x.kind != y.kind ||
            host.equal(x.kind, x.key, y.key) == 0) { return 0; }
        if (x.kind == ResourceIdentityKind::Input || x.kind == ResourceIdentityKind::Data) { return 1; }
        a = x.parent;
        b = y.parent;
    }
}

extern "C" u32 memory_access_conflicts(CallAccess access, AccessConflictHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.possible) requires nonzero(host.held_count)
    requires nonzero(host.held) requires nonzero(host.descendant) requires nonzero(host.region_count)
    requires nonzero(host.region) {
    while (true) {
        let source = host.next(host.context);
        if (source == EffectSourceKind::End) { return 0; }
        if (source != EffectSourceKind::Position || host.possible(host.context) == 0) { continue; }

        // An entire held dataset also covers its elements' owned resources.
        let held_count = host.held_count(host.context);
        usize held_at = 0u64;
        while (held_at < held_count) {
            let held = host.held(host.context, held_at);
            if (held.whole_dataset != 0 && (writes(access) || writes(held.access)) &&
                host.descendant(host.context, held_at) != 0) { return 1; }
            held_at += 1u64;
        }

        let regions = host.region_count(host.context);
        usize at = 0u64;
        while (at < regions) {
            let region = host.region(host.context, at);
            let bytes = u16(region.bytes) >> u16(region.offset);
            let mask = (u16(1) << u16(region.size)) - u16(1);
            if ((bytes & mask) != u16(0)) { return 1; }
            at += 1u64;
        }
    }
}

extern "C" void push_memory_effect_record(address records, usize width, address record) on cpu
    requires nonzero(width) {
    let destination = ordered_tree_prepare(records, width);
    RtlMoveMemory(destination, record, width);
    unsafe_position<OrderedTree>(records).count += 1u64;
}

extern "C" void append_memory_effect_records(address records, address source, usize width) on cpu
    requires nonzero(width) {
    let incoming = unsafe_position<OrderedTree>(source);
    if (incoming.count == 0u64) { return; }
    if (unsafe_position<OrderedTree>(records).count == 0u64) {
        unsafe_position<OrderedTree>(records) = incoming;
        unsafe_position<OrderedTree>(source) = OrderedTree(address(), 0u64, 0u64, 0u64);
        return;
    }
    usize at = 0u64;
    while (at < incoming.count) {
        let record = unsafe_address(address_bits(incoming.records) + at * width);
        push_memory_effect_record(records, width, record);
        at += 1u64;
    }
    fact_tree_clear(source);
}

extern "C" void drop_memory_effect_records(address records, cfn<void,address,usize> release) on cpu
    requires nonzero(release) {
    let current = unsafe_position<OrderedTree>(records);
    if (current.count != 0u64) { release(current.records, current.count); }
    fact_tree_clear(records);
}

extern "C" u8 memory_effect_bytes(MemoryEffectCoverage effect, MemoryEffectCoverageHost host) on cpu
    requires nonzero(host.access) requires nonzero(host.same_element) requires nonzero(host.project) {
    let selects_reads = writes(effect.request);
    if (effect.external_present != 0 && (selects_reads || writes(effect.external))) {
        return scalar_write_mask(effect.width);
    }
    u8 bytes = u8(0);
    usize at = 0u64;
    while (at < effect.count) {
        if ((selects_reads || writes(host.access(host.context, at))) &&
            (effect.outside_element == 0 || host.same_element(host.context, at) == 0)) {
            bytes |= host.project(host.context, at);
        }
        at += 1u64;
    }
    return bytes;
}

u32 exact_written_value(u32 width, u8 all, WrittenValueHost host) on cpu
    requires nonzero(host.regions) requires nonzero(host.region) requires nonzero(host.retain) {
    let count = host.regions(host.context);
    usize at = 0u64;
    u32 exact = 0;
    while (at < count) {
        let region = host.region(host.context, at);
        if (region.bytes != u8(0)) {
            if (region.offset != 0 || region.size != width || region.bytes != all ||
                host.retain(host.context, at) == 0) {
                return 0;
            }
            exact = 1;
        }
        at += 1u64;
    }
    return exact;
}

extern "C" u32 written_memory_value(WrittenValueFacts facts, WrittenValueHost host) on cpu
    requires nonzero(host.access) requires nonzero(host.project) requires nonzero(host.regions)
    requires nonzero(host.region) requires nonzero(host.retain) requires nonzero(host.merge) {
    if (writes(facts.external)) { return 1; }
    let all = scalar_write_mask(facts.width);
    usize at = 0u64;
    while (at < facts.count) {
        if (writes(host.access(host.context, at))) {
            let source = host.project(host.context, at);
            if (source.bytes != u8(0)) {
                u32 exact = 0;
                if (source.proof != 0) { exact = exact_written_value(facts.width, all, host); }
                u64 changed = 0u64;
                u64 kept = 0u64;
                u64 ordered = 0u64;
                u32 partial_integer = 0;
                if (exact == 0 && facts.integer != 0 && source.bytes != all) {
                    u32 byte = 0;
                    while (byte < facts.width) {
                        if ((u32(source.bytes) & (1 << byte)) != 0) {
                            changed |= 255u64 << u64(byte * 8);
                        }
                        byte += 1;
                    }
                    kept = facts.mask & ~changed;
                    ordered = kept ^ facts.bias;
                    partial_integer = 1;
                }
                host.merge(host.context, WrittenValueBits(changed, kept, ordered, exact, partial_integer));
            }
        }
        at += 1u64;
    }
    return 0;
}

extern "C" u32 address_tail(address context,u64 count,u32 size,u32 first,AddressTailHost host) on cpu
 requires nonzero(host.offset) requires nonzero(host.append) {
 u64 i=0u64;while(i<count){u32 at=host.offset(context,i);if(at>=first){host.append(context,i,at-first);}i+=1u64;}return size-first;
}
extern "C" ExternalPositionStep external_position_step(Phase cursor,ExternalPositionFacts facts) on cpu {
 Phase c=cursor;
 if(c.phase==0){if(facts.cpu==0){return ExternalPositionStep(c,ExternalPositionNeed::BadTarget);}if(facts.explicit_type==0||facts.arguments!=1u64){return ExternalPositionStep(c,ExternalPositionNeed::BadArguments);}if(facts.explicit_type==2){return ExternalPositionStep(c,ExternalPositionNeed::Owned);}c.phase=1;return ExternalPositionStep(c,ExternalPositionNeed::Type);}
 if(c.phase==1){if(facts.owns!=0){return ExternalPositionStep(c,ExternalPositionNeed::Owned);}if(facts.views!=0||facts.empty!=0){return ExternalPositionStep(c,ExternalPositionNeed::BadLayout);}c.phase=2;return ExternalPositionStep(c,ExternalPositionNeed::Argument);}
 if(c.phase==2){if(facts.present==0){return ExternalPositionStep(c,ExternalPositionNeed::Empty);}c.phase=3;return ExternalPositionStep(c,ExternalPositionNeed::RequireAddress);}
 if(c.phase==3){c.phase=4;return ExternalPositionStep(c,ExternalPositionNeed::Borrow);}
 if(c.phase==4){c.phase=5;return ExternalPositionStep(c,ExternalPositionNeed::Requirement);}
 if(c.phase==5){c.phase=6;return ExternalPositionStep(c,ExternalPositionNeed::BorrowKind);}
 if(c.phase==6){c.phase=7;return ExternalPositionStep(c,ExternalPositionNeed::Identity);}
 if(c.phase==7){if(facts.identity!=0){c.phase=10;return ExternalPositionStep(c,ExternalPositionNeed::Reword);}c.phase=8;return ExternalPositionStep(c,ExternalPositionNeed::KnownBits);}
 if(c.phase==8){c.phase=9;return ExternalPositionStep(c,facts.constant!=0?ExternalPositionNeed::ConstantIdentity:ExternalPositionNeed::DynamicIdentity);}
 if(c.phase==9){c.phase=10;return ExternalPositionStep(c,ExternalPositionNeed::Reword);}
 if(c.phase==10){c.phase=11;return ExternalPositionStep(c,ExternalPositionNeed::Low);}
 if(c.phase==11){c.phase=12;return ExternalPositionStep(c,ExternalPositionNeed::High);}
 return ExternalPositionStep(c,facts.external!=0?ExternalPositionNeed::ReadyExternal:ExternalPositionNeed::ReadyBorrowed);
}
extern "C" u32 data_external_sources(u32 pending,address context,cfn<u32,address> next) on cpu requires nonzero(next) {
 if(pending!=0){return 0;}u32 source=next(context);if(source==0){return 0;}
 while(source!=0){if(source!=1){return 0;}source=next(context);}return 1;
}

// Each field window repeats over the same rows with the same stride.
extern "C" void dataset_write_ranges(
    address regions, usize count, address output,
    cfn<DatasetWriteRegion,address> next, cfn<void,address,DatasetWriteRegion> emit
) on cpu requires nonzero(next) requires nonzero(emit) {
    if (count==0u64) { return; }
    DatasetWriteRegion current=next(regions);
    usize index=1u64;
    while (index<count) {
        DatasetWriteRegion part=next(regions);
        u64 end=u64(current.word)+u64(current.bytes);
        u64 part_end=u64(part.word)+u64(part.bytes);
        if (u64(part.word)<=end && part_end-u64(current.word)<=4294967295u64) {
            if (part_end>end) { current.bytes=u32(part_end-u64(current.word)); }
        } else {
            emit(output,current);
            current=part;
        }
        index+=1u64;
    }
    emit(output,current);
}
