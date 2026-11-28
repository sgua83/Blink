module bootstrap.call_arguments;
import bootstrap.proof_facts;
import bootstrap.clause_storage;
import bootstrap.output_bindings;
import bootstrap.ir_dataflow;
import bootstrap.names;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" never abort() on cpu from "ucrtbase.dll";

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

extern "C" void normalize_address_origin(u32 derived, u32 pending, OriginNormalizationHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.emit) {
    if (derived == 0) {
        host.emit(host.context, OriginSourceAction::Direct);
        return;
    }
    while (true) {
        let source = host.next(host.context);
        if (source.kind == OriginSourceKind::End) { break; }
        OriginSourceAction action = OriginSourceAction::Retain;
        if (source.kind == OriginSourceKind::Position && source.layout_matches != 0) {
            action = OriginSourceAction::Project;
        } else if (source.kind == OriginSourceKind::Inactive) {
            action = OriginSourceAction::Inactive;
        } else if (source.kind == OriginSourceKind::External) {
            action = OriginSourceAction::External;
        }
        host.emit(host.context, action);
    }
    if (pending != 0) { host.emit(host.context, OriginSourceAction::Pending); }
}

usize find_address_component(u32 word, usize parts, address context,
    cfn<u32,address,usize> ensures memory(read) offset) on cpu requires nonzero(offset) {
    usize index = 0u64;
    while (index < parts && offset(context, index) != word) { index += 1u64; }
    return index;
}

extern "C" OriginProjectionResult project_address_origin(usize selected, usize parent,
    u32 selected_writable, u32 parent_writable, OriginProjectionHost host) on cpu
    requires nonzero(host.selected) requires nonzero(host.parent_offset)
    requires nonzero(host.parent) requires nonzero(host.emit) {
    usize slot = 0u64;
    while (slot < selected) {
        let field = host.selected(host.context, slot);
        let index = find_address_component(field.word, parent, host.context, host.parent_offset);
        if (index == parent) { return OriginProjectionResult::Missing; }
        let candidate = host.parent(host.context, index);
        host.emit(host.context, index, candidate.word, candidate.initialized | field.initialized);
        slot += 1u64;
    }
    return selected_writable != 0 && parent_writable != 0
        ? OriginProjectionResult::Writable : OriginProjectionResult::ReadOnly;
}

extern "C" u32 address_position_at(u32 word, usize parts, u32 stored, AddressPositionRoot root,
    AddressPositionHost host) on cpu requires nonzero(host.offset) requires nonzero(host.initialized)
    requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.resolve)
    requires nonzero(host.same) requires nonzero(host.accept) requires nonzero(host.pending) {
    let slot = find_address_component(word, parts, host.context, host.offset);
    if (slot == parts) { return 0; }
    if (stored != 0) {
        host.emit(host.context, slot, AddressPositionValue::Stored);
        return 1;
    }
    if (root != AddressPositionRoot::Dataset && host.initialized(host.context, slot) != 0) {
        host.emit(host.context, slot, AddressPositionValue::Initialized);
        return 1;
    }
    if (root == AddressPositionRoot::Other) { return 0; }

    u32 agreed = 0;
    while (true) {
        let source = host.next(host.context);
        if (source.kind == AddressSourceKind::End) { break; }
        if (source.possible == 0) { continue; }
        if (source.kind != AddressSourceKind::Position || host.resolve(host.context, slot) == 0) { return 0; }
        if (agreed != 0 && host.same(host.context) == 0) { return 0; }
        host.accept(host.context);
        agreed = 1;
    }
    if (root == AddressPositionRoot::Dataset && host.pending(host.context) != 0) { return 0; }
    return agreed;
}

extern "C" void borrowed_address_positions(usize fields, BorrowedPositionHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.remember) requires nonzero(host.compare)
    requires nonzero(host.forget) requires nonzero(host.publish) {
    usize remaining = fields;
    u32 retain = 0;
    while (remaining != 0u64) {
        let source = host.next(host.context);
        if (source.kind == AddressSourceKind::End) { break; }
        if (source.possible == 0) { continue; }
        if (source.kind != AddressSourceKind::Position) {
            retain = 0;
            break;
        }
        usize field = 0u64;
        while (field < fields) {
            if (retain == 0) {
                if (host.remember(host.context, field) == 0) { remaining -= 1u64; }
            } else if (host.compare(host.context, field) == BorrowedPositionMatch::Different) {
                host.forget(host.context, field);
                remaining -= 1u64;
            }
            field += 1u64;
        }
        retain = 1;
    }
    usize field = 0u64;
    while (field < fields) {
        host.publish(host.context, field, retain);
        field += 1u64;
    }
}

extern "C" u32 address_layout_valid(AddressLayoutHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.validate) {
    while (true) {
        let source = host.next(host.context);
        if (source.kind == AddressLayoutKind::End) { return 1; }
        if (source.kind == AddressLayoutKind::Trusted) { continue; }
        if (source.kind == AddressLayoutKind::Unknown) { return 0; }
        if (source.kind == AddressLayoutKind::Parent && source.fits == 0) { return 0; }
        if (host.validate(host.context) == 0) { return 0; }
    }
}

extern "C" u32 query_address_origin(AddressOriginRoot root, AddressOriginQuery query, AddressOriginHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.follow)
    requires nonzero(host.symbolic_relation) requires nonzero(host.possible) {
    if (query == AddressOriginQuery::Storage) {
        if (root == AddressOriginRoot::External) { return 0; }
        if (root != AddressOriginRoot::Derived) { return host.possible(host.context); }
    } else if (root != AddressOriginRoot::Derived && root != AddressOriginRoot::Dataset) {
        return 0;
    }
    while (true) {
        let source = host.next(host.context);
        if (source == AddressOriginSource::End) { return 0; }
        if (source == AddressOriginSource::Position) {
            if (host.follow(host.context) != 0) { return 1; }
        } else if (query == AddressOriginQuery::Symbolic) {
            if (source == AddressOriginSource::Symbolic) { return 1; }
            if (source == AddressOriginSource::Relation && host.symbolic_relation(host.context) != 0) { return 1; }
        }
    }
}

extern "C" void bind_address_origin(AddressOriginRoot root, AddressOriginBindingMode mode, AddressOriginBindingHost host) on cpu
    requires nonzero(host.parameter) requires nonzero(host.unresolved_parameter)
    requires nonzero(host.bind_borrow) requires nonzero(host.external)
    requires nonzero(host.clear_positions) requires nonzero(host.index) requires nonzero(host.unresolved_index) {
    if (root == AddressOriginRoot::Stored) { return; }
    if (root == AddressOriginRoot::Parameter) {
        if (mode == AddressOriginBindingMode::Outputs) { return; }
        if (host.parameter(host.context) != 0) {
            host.clear_positions(host.context);
        } else if (mode == AddressOriginBindingMode::Call) {
            host.unresolved_parameter(host.context);
            host.clear_positions(host.context);
        }
        return;
    }
    if (root == AddressOriginRoot::External) {
        host.external(host.context);
    } else if (root != AddressOriginRoot::Dataset || mode != AddressOriginBindingMode::Substitute) {
        host.bind_borrow(host.context);
    }
    if (mode == AddressOriginBindingMode::Call) { host.clear_positions(host.context); }
    if (root == AddressOriginRoot::Dataset || root == AddressOriginRoot::Resource) {
        if (host.index(host.context) == 0) { host.unresolved_index(host.context); }
    }
}

extern "C" AddressIdentityResult bind_address_identity(AddressIdentityKind kind, usize count,
    AddressOriginBindingMode mode, AddressIdentityHost host) on cpu
    requires nonzero(host.address_at) requires nonzero(host.sources)
    requires nonzero(host.append_source) requires nonzero(host.append_original) requires nonzero(host.bind_storage) {
    if (kind == AddressIdentityKind::Constant) { return AddressIdentityResult::Original; }
    if (kind == AddressIdentityKind::Storage) {
        host.bind_storage(host.context);
        return AddressIdentityResult::Bound;
    }
    if (mode != AddressOriginBindingMode::Outputs) {
        usize slot = 0u64;
        while (slot < count) {
            if (host.address_at(host.context, slot) != 0) { return AddressIdentityResult::Bound; }
            slot += 1u64;
        }
    }
    usize slot = 0u64;
    while (slot < count) {
        let sources = host.sources(host.context, slot);
        if (sources.count != 0u64) {
            let width = mode == AddressOriginBindingMode::Substitute ? sources.count : 1u64;
            usize child = 0u64;
            while (child < width) {
                host.append_source(host.context, child);
                child += 1u64;
            }
        } else if (mode == AddressOriginBindingMode::Outputs ||
                   (mode == AddressOriginBindingMode::Substitute && sources.present == 0)) {
            host.append_original(host.context, slot);
        }
        slot += 1u64;
    }
    return AddressIdentityResult::Values;
}

extern "C" void bind_external_address_storage(AddressStorageHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.local) requires nonzero(host.emit) {
    while (true) {
        let source = host.next(host.context);
        if (source == AddressSourceKind::End) { return; }
        let external = source == AddressSourceKind::Position && host.local(host.context) != 0;
        host.emit(host.context, external ? 1 : 0);
    }
}

extern "C" void join_address_identities(usize width, usize left, usize right, u32 conditional, AddressJoinHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.inspect) requires nonzero(host.emit) {
    if (width == 0u64 || (left == 0u64 && right == 0u64)) { return; }
    host.reserve(host.context, width);
    usize component = 0u64;
    while (component < width) {
        let entry = host.inspect(host.context, component);
        AddressJoinAction action = AddressJoinAction::Keep;
        if (entry.equal == 0) {
            if (entry.storage == 0) { action = AddressJoinAction::Unknown; }
            else if (conditional != 0) { action = AddressJoinAction::Conditional; }
            else { action = AddressJoinAction::Union; }
        }
        host.emit(host.context, component, action);
        component += 1u64;
    }
}

extern "C" void join_borrow_components(usize width, u32 conditional, BorrowJoinHost host) on cpu
    requires nonzero(host.equal) requires nonzero(host.branch_paths) requires nonzero(host.restrict_left)
    requires nonzero(host.merge_right) requires nonzero(host.union_right) {
    usize component = 0u64;
    if (conditional == 0) {
        while (component < width) {
            host.union_right(host.context, component);
            component += 1u64;
        }
        return;
    }
    bool prepared = false;
    while (component < width) {
        if (host.equal(host.context, component) == 0) {
            if (!prepared) {
                host.branch_paths(host.context);
                prepared = true;
            }
            host.restrict_left(host.context, component);
            host.merge_right(host.context, component);
        }
        component += 1u64;
    }
}

TaskBorrowAction task_borrow_action(TaskBorrowEdge edge, TaskBorrowProjection projection) on cpu {
    if (edge.kind == TaskBorrowSource::Task) {
        if (projection == TaskBorrowProjection::Returned) { return TaskBorrowAction::TakeReturned; }
        if (projection == TaskBorrowProjection::Initialized) { return TaskBorrowAction::TakeInitialized; }
    }
    if (edge.kind == TaskBorrowSource::Inactive && projection == TaskBorrowProjection::Initialized) {
        return TaskBorrowAction::Omit;
    }
    if (projection == TaskBorrowProjection::Writes || projection == TaskBorrowProjection::Places) {
        if (edge.kind == TaskBorrowSource::Task) { return TaskBorrowAction::ProjectCaptures; }
        if (edge.kind == TaskBorrowSource::Position) {
            return projection == TaskBorrowProjection::Places || edge.writable != 0
                ? TaskBorrowAction::Keep : TaskBorrowAction::Omit;
        }
        if (edge.kind == TaskBorrowSource::TaskOutput) {
            // Until completion is received, a memory effect covers all returned owners.
            if (edge.held == TaskBorrowProjection::ReturnedWithCompletion) { return TaskBorrowAction::ProjectReturnedOwners; }
            if (edge.held == TaskBorrowProjection::Writes) { return TaskBorrowAction::ProjectWrites; }
            if (edge.held == TaskBorrowProjection::Places) { return TaskBorrowAction::ProjectRequested; }
        }
        if (edge.kind == TaskBorrowSource::Inactive || edge.kind == TaskBorrowSource::Storage) {
            return TaskBorrowAction::Omit;
        }
    }
    if (edge.kind == TaskBorrowSource::Inactive || edge.kind == TaskBorrowSource::Unresolved) {
        return TaskBorrowAction::Keep;
    }
    return TaskBorrowAction::Defer;
}

extern "C" void project_task_borrows(usize count, TaskBorrowProjection projection, u32 pending, TaskBorrowHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.emit) requires nonzero(host.emit_pending) {
    usize source = 0u64;
    while (source < count) {
        TaskBorrowEdge edge = host.next(host.context);
        TaskBorrowAction action = task_borrow_action(edge, projection);
        if (action != TaskBorrowAction::Omit) { host.emit(host.context, edge, action); }
        source += 1u64;
    }
    if (pending != 0) { host.emit_pending(host.context); }
}

extern "C" void project_borrow_fields(BorrowFieldSpan fields, usize count, BorrowFieldKind kind, u32 pending, BorrowFieldHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.reserve) requires nonzero(host.emit) requires nonzero(host.emit_pending) {
    usize source = 0u64;
    while (source < count) {
        let projected = host.next(host.context);
        if (fields.count != 0u64) {
            let tail = host.reserve(host.context, fields.count);
            usize field = 0u64;
            while (field < fields.count) {
                let offset = field * size_of<usize>();
                let from = unsafe_address(address_bits(fields.data) + offset);
                let to = unsafe_address(address_bits(tail) + offset);
                unsafe_position<usize>(to) = unsafe_position<usize>(from);
                field += 1u64;
            }
        }
        BorrowFieldAction action = BorrowFieldAction::Field;
        if (kind == BorrowFieldKind::Element) {
            action = projected != 0 ? BorrowFieldAction::ExpandElement : BorrowFieldAction::Element;
        }
        host.emit(host.context, action, fields.count);
        source += 1u64;
    }
    if (pending != 0) { host.emit_pending(host.context); }
}

void visit_call_source_task(CallSourceShape source,address context,cfn<void,address,CallSourceTaskPart,usize> visit) on cpu
    requires nonzero(visit) {
    visit(context, CallSourceTaskPart::Captures, 0u64);
    visit(context, CallSourceTaskPart::Initialized, 0u64);
    usize output = 0u64;
    while (output < source.outputs) {
        visit(context, CallSourceTaskPart::Output, output);
        output += 1u64;
    }
    usize completion = 0u64;
    while (completion < source.completions) {
        visit(context, CallSourceTaskPart::Completion, completion);
        completion += 1u64;
    }
    usize guard = 0u64;
    while (guard < source.guards) {
        visit(context, CallSourceTaskPart::Guard, guard);
        guard += 1u64;
    }
}

extern "C" void bind_call_borrow_source(CallSourceShape source, CallSourceHost host) on cpu
    requires nonzero(host.lookup) requires nonzero(host.borrow) requires nonzero(host.identity)
    requires nonzero(host.index) requires nonzero(host.task) requires nonzero(host.finish) {
    if (source.kind == CallSourceKind::Data || source.kind == CallSourceKind::Parameter) {
        if (host.lookup(host.context) == 0) {
            host.finish(host.context, CallSourceKind::Missing);
            return;
        }
        if (source.kind == CallSourceKind::Parameter) {
            usize index = 0u64;
            while (index < source.indices) {
                host.index(host.context, index);
                index += 1u64;
            }
            host.borrow(host.context);
            host.finish(host.context, CallSourceKind::Parameter);
        } else if (host.borrow(host.context) == 0 && host.identity(host.context) == 0) {
            host.finish(host.context, CallSourceKind::Missing);
        }
        return;
    }
    if (source.kind == CallSourceKind::Task) {
        visit_call_source_task(source,host.context,host.task);
    }
    host.finish(host.context, source.kind);
}

void rename_output_source(address bindings,OutputSourceHost host,OutputSourcePart part,usize index) on cpu
    requires nonzero(host.identity) requires nonzero(host.rename) {
    let original = host.identity(host.context,part,index);
    if (original == address()) { return; }
    let replacement = find_output_binding(bindings,original);
    if (replacement != address()) { host.rename(host.context,part,index,replacement); }
}

extern "C" void bind_output_borrow_source(CallSourceShape source,address bindings,OutputSourceHost host) on cpu
    requires nonzero(host.identity) requires nonzero(host.rename) requires nonzero(host.child) requires nonzero(host.task) {
    if (source.kind == CallSourceKind::Keep) { return; }
    if (source.kind == CallSourceKind::Task) {
        visit_call_source_task(source,host.context,host.task);
        return;
    }
    if (source.kind == CallSourceKind::Data || source.kind == CallSourceKind::Parameter) {
        rename_output_source(bindings,host,OutputSourcePart::Root,0u64);
        usize index = 0u64;
        while (index < source.indices) {
            rename_output_source(bindings,host,OutputSourcePart::Index,index);
            index += 1u64;
        }
        return;
    }
    host.child(host.context,source.kind);
    if (source.kind == CallSourceKind::Element) {
        rename_output_source(bindings,host,OutputSourcePart::Index,0u64);
    } else if (source.kind == CallSourceKind::TaskOutput) {
        usize result = 0u64;
        while (result < source.indices) {
            rename_output_source(bindings,host,OutputSourcePart::TaskResult,result);
            result += 1u64;
        }
    }
}

address borrow_edge_condition(BorrowEdgeGroup group,address edge) on cpu {
    return unsafe_address(address_bits(edge)+group.condition);
}

u32 write_borrow_edge(BorrowEdgeGroup group,OrderedPlace place,address key,address incoming,BorrowEdgeMode mode,
    BorrowEdgeMerge host) on cpu requires nonzero(group.stride)
    requires nonzero(host.key)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized) {
    address value = incoming;
    if (place.entry != address()) {
        address current = borrow_edge_condition(group,place.entry);
        address combined = host.clauses.either(unsafe_position<address>(current),value);
        clause_release(value);
        value = combined;
    }
    if (mode == BorrowEdgeMode::Canonical && place.entry != address()) {
        address result = host.clauses.canonical(value);
        clause_release(value);
        value = result;
    }
    if (mode == BorrowEdgeMode::Normalized) {
        address result = host.clauses.normalized(host.normal,value);
        clause_release(value);
        value = result;
    }
    if (place.entry != address()) {
        address current = borrow_edge_condition(group,place.entry);
        clause_release(unsafe_position<address>(current));
        unsafe_position<address>(current) = value;
        return 0;
    }
    address slot = ordered_tree_insert_slot(group.tree,group.stride,place.parent,place.order);
    host.key(key,slot);
    unsafe_position<address>(borrow_edge_condition(group,slot)) = value;
    return 1;
}

extern "C" u32 merge_borrow_edge(BorrowEdgeGroup group,address key,address when,BorrowEdgeMode mode,BorrowEdgeMerge host) on cpu
    requires nonzero(group.stride)
    requires nonzero(host.compare) requires nonzero(host.key)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized) {
    address incoming = unsafe_position<address>(when);
    clause_retain(incoming);
    let place = ordered_tree_seek(group.tree,group.stride,key,host.compare);
    return write_borrow_edge(group,place,key,incoming,mode,host);
}

extern "C" void union_borrow_edges(BorrowEdgeGroup group,BorrowEdgeGroup source,address path,BorrowEdgeMerge host) on cpu
    requires nonzero(group.stride)
    requires nonzero(host.compare) requires nonzero(host.key) requires nonzero(host.clauses.both)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized) {
    FactCursor cursor = ordered_tree_cursor(source.tree,source.stride);
    u64 position = 0u64;
    while (true) {
        address edge = ordered_tree_step(source.tree,source.stride,address_of(cursor),0);
        if (edge == address()) { return; }
        address incoming = unsafe_position<address>(borrow_edge_condition(source,edge));
        if (path == address()) {
            clause_retain(incoming);
        } else {
            incoming = host.clauses.both(incoming,unsafe_position<address>(path));
        }
        OrderedPlace place = ordered_tree_seek_next(group.tree,group.stride,edge,position,host.compare);
        write_borrow_edge(group,place,edge,incoming,BorrowEdgeMode::Union,host);
    }
}

extern "C" void merge_borrow_output(inout address remaining, ReadWritePaths paths,
    BorrowOutputShape output, BorrowOutputShape input, BorrowOutputMerge host,
    cfn<u32,address> conflict) on cpu
    requires nonzero(output.sources.stride) requires nonzero(output.inputs.stride)
    requires nonzero(output.calls.stride) requires nonzero(output.reads.stride)
    requires nonzero(host.sources.compare) requires nonzero(host.sources.copy)
    requires nonzero(host.inputs.compare) requires nonzero(host.inputs.copy)
    requires nonzero(host.calls.compare) requires nonzero(host.calls.copy)
    requires nonzero(host.reads.compare) requires nonzero(host.reads.copy)
    requires nonzero(host.clauses.both) requires nonzero(host.clauses.either)
    requires nonzero(host.clauses.canonical) requires nonzero(host.clauses.normalized)
    requires nonzero(conflict)
{
    address visible = paths.written;
    if (remaining != address()) {
        visible = paths.written == address() ? remaining
            : combine_clause_both(remaining,paths.written,conflict);
    }
    address path = visible == address() ? address() : address_of(visible);
    union_borrow_edges(output.sources,input.sources,path,
        BorrowEdgeMerge(host.clauses,host.sources.compare,host.sources.copy,address()));
    union_borrow_edges(output.inputs,input.inputs,path,
        BorrowEdgeMerge(host.clauses,host.inputs.compare,host.inputs.copy,address()));
    union_borrow_edges(output.calls,input.calls,path,
        BorrowEdgeMerge(host.clauses,host.calls.compare,host.calls.copy,address()));
    union_borrow_edges(output.reads,input.reads,path,
        BorrowEdgeMerge(host.clauses,host.reads.compare,host.reads.copy,address()));
    if (remaining != address() && paths.written != address()) {
        clause_release(visible);
    }
    if (paths.unwritten == address()) { return; }
    if (remaining == address()) {
        clause_retain(paths.unwritten);
        remaining = paths.unwritten;
    } else {
        address earlier = combine_clause_both(remaining,paths.unwritten,conflict);
        clause_release(remaining);
        remaining = earlier;
    }
}

void merge_owned_borrow_sources(BorrowEdgeGroup output,address owner,address path,
    BorrowEdgeMerge host,cfn<void,address> release_key) on cpu
    requires nonzero(output.stride)
    requires nonzero(host.compare) requires nonzero(host.key) requires nonzero(host.clauses.both)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized) requires nonzero(release_key)
{
    FactCursor rows = ordered_tree_cursor(owner,output.stride);
    u64 position = 0u64;
    while (true) {
        address edge = ordered_tree_step(owner,output.stride,address_of(rows),0);
        if (edge == address()) { break; }
        address found = unsafe_position<address>(borrow_edge_condition(output,edge));
        address matched = host.clauses.both(found,unsafe_position<address>(path));
        clause_release(found);
        OrderedPlace place = ordered_tree_seek_next(output.tree,output.stride,edge,position,host.compare);
        if (write_borrow_edge(output,place,edge,matched,BorrowEdgeMode::Union,host) == 0) {
            release_key(edge);
        }
    }
    fact_tree_clear(owner);
}

extern "C" void merge_evaluated_borrow_edges(BorrowEdgeGroup output,usize count,address context,
    BorrowEdgeMerge host,cfn<EvaluatedBorrowEdge,address,usize> evaluate,cfn<void,address,address> move_key,
    cfn<void,address> release_key) on cpu
    requires nonzero(output.stride)
    requires nonzero(host.compare) requires nonzero(host.key) requires nonzero(host.clauses.both)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized)
    requires nonzero(evaluate) requires nonzero(move_key) requires nonzero(release_key)
{
    BorrowEdgeMerge move_sources = BorrowEdgeMerge(host.clauses,host.compare,move_key,host.normal);
    usize index = 0u64;
    while (index < count) {
        EvaluatedBorrowEdge edge = evaluate(context,index);
        if (edge.borrowed == address()) {
            merge_owned_borrow_sources(output,address_of(edge.owned),edge.path,move_sources,release_key);
        } else {
            BorrowEdgeGroup source = BorrowEdgeGroup(edge.borrowed,output.stride,output.condition);
            union_borrow_edges(output,source,edge.path,host);
        }
        index += 1u64;
    }
}

extern "C" void replay_borrow_sources(BorrowEdgeGroup output,BorrowEdgeGroup source,address context,
    BorrowEdgeMerge host,cfn<OrderedTree,address,address,address> read,cfn<void,address> release_key) on cpu
    requires nonzero(output.stride)
    requires nonzero(host.compare) requires nonzero(host.key) requires nonzero(host.clauses.both)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized)
    requires nonzero(read) requires nonzero(release_key)
{
    FactCursor input = ordered_tree_cursor(source.tree,source.stride);
    while (true) {
        address origin = ordered_tree_step(source.tree,source.stride,address_of(input),0);
        if (origin == address()) { return; }
        address path = borrow_edge_condition(source,origin);
        OrderedTree value = read(context,origin,path);
        merge_owned_borrow_sources(output,address_of(value),path,host,release_key);
    }
}

void restrict_borrow_edges(BorrowEdgeGroup group,address condition,cfn<address,address,address> both) on cpu
    requires nonzero(both) {
    FactCursor cursor = ordered_tree_cursor(group.tree,group.stride);
    while (true) {
        address edge = ordered_tree_step(group.tree,group.stride,address_of(cursor),0);
        if (edge == address()) { return; }
        address current = borrow_edge_condition(group,edge);
        address previous = unsafe_position<address>(current);
        unsafe_position<address>(current) = both(previous,condition);
        clause_release(previous);
    }
}

extern "C" void restrict_borrow_output(BorrowOutputShape output,address path,cfn<address,address,address> both) on cpu
    requires nonzero(both) {
    address condition = unsafe_position<address>(path);
    restrict_borrow_edges(output.sources,condition,both);
    restrict_borrow_edges(output.inputs,condition,both);
    restrict_borrow_edges(output.calls,condition,both);
    restrict_borrow_edges(output.reads,condition,both);
}

extern "C" void finish_borrow_history(inout address earlier,
    BorrowOutputShape output, BorrowOutputShape input, BorrowOutputMerge host,
    cfn<u32,address> conflict) on cpu
    requires nonzero(output.sources.stride) requires nonzero(output.inputs.stride)
    requires nonzero(output.calls.stride) requires nonzero(output.reads.stride)
    requires nonzero(host.sources.compare) requires nonzero(host.sources.copy)
    requires nonzero(host.inputs.compare) requires nonzero(host.inputs.copy)
    requires nonzero(host.calls.compare) requires nonzero(host.calls.copy)
    requires nonzero(host.reads.compare) requires nonzero(host.reads.copy)
    requires nonzero(host.clauses.both) requires nonzero(host.clauses.either)
    requires nonzero(host.clauses.canonical) requires nonzero(host.clauses.normalized)
    requires nonzero(conflict)
{
    if (earlier != address()) {
        restrict_borrow_output(output,address_of(earlier),host.clauses.both);
        clause_release(earlier);
        earlier = address();
    }
    merge_borrow_output(earlier,ReadWritePaths(address(),address()),output,input,host,conflict);
}

enum BorrowReachWorkKind:u32 { Group=0; Children=1; LeavePath=2; };

struct BorrowReachWork {
    BorrowReachWorkKind kind;
    BorrowInquiry inquiry;
    BorrowEdgeGroup group;
    FactCursor cursor;
    address source;
    usize index;
    usize limit;
    u32 sources;
};

struct BorrowReachBuffer { address records; usize count; usize capacity; };
struct BorrowReachPath { address when; address value; };

struct BorrowReachWalk {
    BorrowReachBuffer work;
    BorrowReachBuffer paths;
    address root;
    u32 root_ready;
    usize resolved;
};

address borrow_reach_slot(BorrowReachBuffer buffer,usize index,usize stride) on cpu {
    return unsafe_address(address_bits(buffer.records)+index*stride);
}

address append_borrow_reach_slot(inout BorrowReachBuffer buffer,usize stride) on cpu requires nonzero(stride) {
    if(buffer.count==buffer.capacity){
        if(buffer.capacity>9223372036854775807u64/stride/2u64){abort();}
        usize capacity=buffer.capacity==0u64?8u64:buffer.capacity*2u64;
        let heap=GetProcessHeap();
        address records=buffer.records==address()?HeapAlloc(heap,0,capacity*stride):
            HeapReAlloc(heap,0,buffer.records,capacity*stride);
        if(records==address()){abort();}
        buffer.records=records;
        buffer.capacity=capacity;
    }
    let slot=borrow_reach_slot(buffer,buffer.count,stride);
    buffer.count+=1u64;
    return slot;
}

void push_borrow_reach_work(inout BorrowReachWalk walk,BorrowReachWork value) on cpu {
    let slot=append_borrow_reach_slot(walk.work,size_of<BorrowReachWork>());
    unsafe_position<BorrowReachWork>(slot)=value;
}

BorrowReachWork plain_borrow_reach_work(BorrowReachWorkKind kind,BorrowInquiry inquiry,address source) on cpu {
    return BorrowReachWork(kind,inquiry,BorrowEdgeGroup(address(),0u64,0u64),FactCursor(0u64,0u64),source,0u64,0u64,0);
}

void push_borrow_reach_path(inout BorrowReachWalk walk,address when) on cpu {
    let slot=append_borrow_reach_slot(walk.paths,size_of<BorrowReachPath>());
    unsafe_position<BorrowReachPath>(slot)=BorrowReachPath(when,address());
}

void pop_borrow_reach_path(inout BorrowReachWalk walk) on cpu {
    walk.paths.count-=1u64;
    if(walk.resolved>walk.paths.count){
        walk.resolved=walk.paths.count;
        let slot=borrow_reach_slot(walk.paths,walk.paths.count,size_of<BorrowReachPath>());
        clause_release(unsafe_position<BorrowReachPath>(slot).value);
    }
}

address resolve_borrow_reach_path(inout BorrowReachWalk walk,BorrowReachHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.both) requires nonzero(host.possible) {
    if(walk.root_ready==0){
        let slot=host.initial(host.context);
        if(slot!=address()){walk.root=unsafe_position<address>(slot);}
        walk.root_ready=1;
    }
    address value=walk.root;
    if(walk.resolved!=0u64){
        let previous=borrow_reach_slot(walk.paths,walk.resolved-1u64,size_of<BorrowReachPath>());
        value=unsafe_position<BorrowReachPath>(previous).value;
    }
    while(walk.resolved<walk.paths.count){
        let slot=borrow_reach_slot(walk.paths,walk.resolved,size_of<BorrowReachPath>());
        if(value!=address()){
            value=host.both(value,unsafe_position<BorrowReachPath>(slot).when);
            if(host.possible(value)==0){clause_release(value);value=address();}
        }
        unsafe_position<BorrowReachPath>(slot).value=value;
        walk.resolved+=1u64;
    }
    return value;
}

void queue_borrow_reach_group(inout BorrowReachWalk walk,BorrowEdgeGroup group,BorrowInquiry inquiry,u32 sources) on cpu {
    if(unsafe_position<OrderedTree>(group.tree).count==0u64){return;}
    let cursor=ordered_tree_cursor(group.tree,group.stride);
    push_borrow_reach_work(walk,BorrowReachWork(BorrowReachWorkKind::Group,inquiry,group,cursor,address(),0u64,0u64,sources));
}

bool queue_borrow_reach_groups(inout BorrowReachWalk walk,BorrowOutputShape shape,BorrowInquiry inquiry) on cpu {
    if(inquiry==BorrowInquiry::Pending){
        if(unsafe_position<OrderedTree>(shape.inputs.tree).count!=0u64||
            unsafe_position<OrderedTree>(shape.calls.tree).count!=0u64||
            unsafe_position<OrderedTree>(shape.reads.tree).count!=0u64){return true;}
    }
    queue_borrow_reach_group(walk,shape.sources,inquiry,1);
    if(inquiry==BorrowInquiry::PendingInPath){
        queue_borrow_reach_group(walk,shape.reads,inquiry,0);
        queue_borrow_reach_group(walk,shape.calls,inquiry,0);
        queue_borrow_reach_group(walk,shape.inputs,inquiry,0);
    }
    return false;
}

bool inspect_borrow_reach_source(
    inout BorrowReachWalk walk,address source,BorrowInquiry inquiry,BorrowReachHost host
) on cpu
    requires nonzero(host.initial) requires nonzero(host.both) requires nonzero(host.possible)
    requires nonzero(host.describe) requires nonzero(host.child) requires nonzero(host.leaf) {
    let shape=host.describe(source);
    if(shape.kind==BorrowReachSourceKind::Unresolved){
        return inquiry==BorrowInquiry::Unresolved&&resolve_borrow_reach_path(walk,host)!=address();
    }
    if(shape.kind==BorrowReachSourceKind::Relation){
        if(inquiry==BorrowInquiry::Pending){return host.leaf(source,BorrowReachLeaf::RelationPending)!=0;}
        if(inquiry==BorrowInquiry::Content){return host.leaf(source,BorrowReachLeaf::RelationContent)!=0;}
        let leaf=inquiry==BorrowInquiry::Local?BorrowReachLeaf::RelationLocal:BorrowReachLeaf::RelationUnresolved;
        return host.leaf(source,leaf)!=0&&resolve_borrow_reach_path(walk,host)!=address();
    }
    if(shape.kind==BorrowReachSourceKind::Contents){
        if(inquiry==BorrowInquiry::Pending){return host.leaf(source,BorrowReachLeaf::OriginPending)!=0;}
        if(inquiry==BorrowInquiry::Content){return true;}
        if(inquiry==BorrowInquiry::Unresolved){
            return (host.leaf(source,BorrowReachLeaf::OriginLocal)!=0||
                host.leaf(source,BorrowReachLeaf::OriginUnresolved)!=0)&&resolve_borrow_reach_path(walk,host)!=address();
        }
        return false;
    }
    if(shape.kind==BorrowReachSourceKind::Position){
        if(inquiry==BorrowInquiry::Pending){return host.leaf(source,BorrowReachLeaf::OriginPending)!=0;}
        if(inquiry==BorrowInquiry::Content){return host.leaf(source,BorrowReachLeaf::OriginContent)!=0;}
        if(shape.root==BorrowReachRoot::Dataset||shape.root==BorrowReachRoot::Derived||
            (inquiry==BorrowInquiry::Unresolved&&shape.root==BorrowReachRoot::Resource)){
            return queue_borrow_reach_groups(walk,host.child(source,BorrowReachChild::Single,0u64),inquiry);
        }
        return inquiry==BorrowInquiry::Local&&host.leaf(source,BorrowReachLeaf::OriginLocal)!=0&&
            resolve_borrow_reach_path(walk,host)!=address();
    }
    if(shape.kind==BorrowReachSourceKind::Task){
        BorrowReachWork children=plain_borrow_reach_work(BorrowReachWorkKind::Children,inquiry,source);
        children.limit=shape.outputs+2u64;
        push_borrow_reach_work(walk,children);
        return false;
    }
    if(shape.kind==BorrowReachSourceKind::TaskOutput){
        if(inquiry==BorrowInquiry::Pending&&shape.returned==0){return true;}
        return queue_borrow_reach_groups(walk,host.child(source,BorrowReachChild::Single,0u64),inquiry);
    }
    if(shape.kind==BorrowReachSourceKind::Child){
        return queue_borrow_reach_groups(walk,host.child(source,BorrowReachChild::Single,0u64),inquiry);
    }
    return false;
}

bool run_borrow_reach_walk(inout BorrowReachWalk walk,BorrowReachHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.both) requires nonzero(host.possible)
    requires nonzero(host.describe) requires nonzero(host.child) requires nonzero(host.leaf) {
    while(walk.work.count!=0u64){
        walk.work.count-=1u64;
        let slot=borrow_reach_slot(walk.work,walk.work.count,size_of<BorrowReachWork>());
        BorrowReachWork pending=unsafe_position<BorrowReachWork>(slot);
        if(pending.kind==BorrowReachWorkKind::LeavePath){
            pop_borrow_reach_path(walk);
            continue;
        }
        if(pending.kind==BorrowReachWorkKind::Children){
            let part=pending.index==0u64?BorrowReachChild::Captures:
                (pending.index==1u64?BorrowReachChild::Initialized:BorrowReachChild::Output);
            let index=pending.index<2u64?0u64:pending.index-2u64;
            pending.index+=1u64;
            if(pending.index<pending.limit){push_borrow_reach_work(walk,pending);}
            if(queue_borrow_reach_groups(walk,host.child(pending.source,part,index),pending.inquiry)){return true;}
            continue;
        }

        let edge=ordered_tree_step(pending.group.tree,pending.group.stride,address_of(pending.cursor),0);
        if(edge==address()){continue;}
        push_borrow_reach_work(walk,pending);
        if(pending.inquiry==BorrowInquiry::Pending||pending.inquiry==BorrowInquiry::Content){
            if(inspect_borrow_reach_source(walk,edge,pending.inquiry,host)){return true;}
            continue;
        }

        let when=unsafe_position<address>(borrow_edge_condition(pending.group,edge));
        push_borrow_reach_path(walk,when);
        if(pending.inquiry==BorrowInquiry::PendingInPath){
            let found=resolve_borrow_reach_path(walk,host)!=address();
            pop_borrow_reach_path(walk);
            if(found&&(pending.sources==0||inspect_borrow_reach_source(walk,edge,BorrowInquiry::Pending,host))){
                return true;
            }
        }else{
            push_borrow_reach_work(walk,plain_borrow_reach_work(BorrowReachWorkKind::LeavePath,pending.inquiry,address()));
            if(inspect_borrow_reach_source(walk,edge,pending.inquiry,host)){return true;}
        }
    }
    return false;
}

bool active_borrow_reach_group(BorrowEdgeGroup group,cfn<u32,address> possible) on cpu requires nonzero(possible) {
    FactCursor cursor=ordered_tree_cursor(group.tree,group.stride);
    while(true){
        let edge=ordered_tree_step(group.tree,group.stride,address_of(cursor),0);
        if(edge==address()){return false;}
        if(possible(unsafe_position<address>(borrow_edge_condition(group,edge)))!=0){return true;}
    }
}

void release_borrow_reach_walk(inout BorrowReachWalk walk) on cpu {
    while(walk.paths.count!=0u64){pop_borrow_reach_path(walk);}
    let heap=GetProcessHeap();
    if(walk.paths.records!=address()){HeapFree(heap,0,walk.paths.records);}
    if(walk.work.records!=address()){HeapFree(heap,0,walk.work.records);}
}

extern "C" u32 query_borrow_reachability(BorrowOutputShape shape,BorrowInquiry inquiry,BorrowReachHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.both) requires nonzero(host.possible)
    requires nonzero(host.describe) requires nonzero(host.child) requires nonzero(host.leaf) {
    if(inquiry==BorrowInquiry::Active){
        return active_borrow_reach_group(shape.sources,host.possible)||
            active_borrow_reach_group(shape.inputs,host.possible)||
            active_borrow_reach_group(shape.calls,host.possible)||
            active_borrow_reach_group(shape.reads,host.possible)?1:0;
    }
    let empty=BorrowReachBuffer(address(),0u64,0u64);
    BorrowReachWalk walk=BorrowReachWalk(empty,empty,address(),0,0u64);
    let found=queue_borrow_reach_groups(walk,shape,inquiry)||run_borrow_reach_walk(walk,host);
    release_borrow_reach_walk(walk);
    return found?1:0;
}

extern "C" u32 query_borrow_source(address source,BorrowInquiry inquiry,BorrowReachHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.both) requires nonzero(host.possible)
    requires nonzero(host.describe) requires nonzero(host.child) requires nonzero(host.leaf) {
    let empty=BorrowReachBuffer(address(),0u64,0u64);
    BorrowReachWalk walk=BorrowReachWalk(empty,empty,address(),0,0u64);
    let found=inspect_borrow_reach_source(walk,source,inquiry,host)||run_borrow_reach_walk(walk,host);
    release_borrow_reach_walk(walk);
    return found?1:0;
}

void visit_borrow_output_group(BorrowEdgeGroup group,BorrowCallPart part,usize offset,address context,
    cfn<void,address,BorrowCallPart,address> visit) on cpu requires nonzero(visit) {
    FactCursor cursor = ordered_tree_cursor(group.tree,group.stride);
    while (true) {
        address edge = ordered_tree_step(group.tree,group.stride,address_of(cursor),0);
        if (edge == address()) { return; }
        visit(context,part,unsafe_address(address_bits(edge)+offset));
    }
}

extern "C" void bind_borrow_output(BorrowOutputShape shape,u32 rename_sources,BorrowOutputHost host) on cpu
    requires nonzero(host.collect) requires nonzero(host.project)
    requires nonzero(host.restore) requires nonzero(host.rename_source) {
    visit_borrow_output_group(shape.inputs,BorrowCallPart::Inputs,shape.inputs.condition,host.context,host.collect);
    visit_borrow_output_group(shape.calls,BorrowCallPart::Calls,shape.calls.condition,host.context,host.collect);
    visit_borrow_output_group(shape.reads,BorrowCallPart::Reads,shape.reads.condition,host.context,host.collect);
    visit_borrow_output_group(shape.sources,BorrowCallPart::Sources,shape.sources.condition,host.context,host.collect);
    host.project(host.context);
    visit_borrow_output_group(shape.inputs,BorrowCallPart::Inputs,0u64,host.context,host.restore);
    visit_borrow_output_group(shape.calls,BorrowCallPart::Calls,0u64,host.context,host.restore);
    visit_borrow_output_group(shape.reads,BorrowCallPart::Reads,0u64,host.context,host.restore);
    if (rename_sources == 0) {
        visit_borrow_output_group(shape.sources,BorrowCallPart::Sources,0u64,host.context,host.restore);
    } else {
        visit_borrow_output_group(shape.sources,BorrowCallPart::Sources,0u64,host.context,host.rename_source);
    }
}

extern "C" void map_call_borrows(BorrowCallCounts counts, BorrowCallHost host) on cpu
    requires nonzero(host.collect) requires nonzero(host.project)
    requires nonzero(host.restore) requires nonzero(host.map_source) {
    if (counts.inputs != 0u64) { host.collect(host.context, BorrowCallPart::Inputs); }
    if (counts.calls != 0u64) { host.collect(host.context, BorrowCallPart::Calls); }
    if (counts.reads != 0u64) { host.collect(host.context, BorrowCallPart::Reads); }
    if (counts.sources != 0u64) { host.collect(host.context, BorrowCallPart::Sources); }
    host.project(host.context);
    if (counts.inputs != 0u64) { host.restore(host.context, BorrowCallPart::Inputs); }
    if (counts.calls != 0u64) { host.restore(host.context, BorrowCallPart::Calls); }
    if (counts.reads != 0u64) { host.restore(host.context, BorrowCallPart::Reads); }
    usize source = 0u64;
    while (source < counts.sources) {
        host.map_source(host.context);
        source += 1u64;
    }
}

extern "C" AddressViolation validate_address_requirements(AddressRequirements required, AddressRequirementHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.valid_layout) requires nonzero(host.blink_storage)
    requires nonzero(host.writable) requires nonzero(host.initialized) requires nonzero(host.possible)
    requires nonzero(host.applies) {
    while (true) {
        let source = host.next(host.context);
        if (source == AddressSourceKind::End) { return AddressViolation::None; }
        if (source != AddressSourceKind::Position) { continue; }
        AddressViolation failure = AddressViolation::None;
        if (host.valid_layout(host.context) == 0) { failure = AddressViolation::Layout; }
        else if (required.atomic != 0 && host.blink_storage(host.context) != 0) { failure = AddressViolation::Atomic; }
        else if (required.writable != 0 && host.writable(host.context) == 0) { failure = AddressViolation::ReadOnly; }
        else if (required.initialized != 0 && host.initialized(host.context) == 0) { failure = AddressViolation::Uninitialized; }
        if (failure == AddressViolation::None || host.possible(host.context) == 0) { continue; }
        return host.applies(host.context) != 0 ? failure : AddressViolation::Inactive;
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

struct MemoryAliasCell { OrderedNode links; array<u64,3> key; };
struct MemoryAliasParent { address keys; address parent; };
struct MemoryAliasSearch { usize found; usize parent; i32 order; };

address memory_alias_cell(OrderedTree tree, usize at) on cpu {
    return unsafe_address(address_bits(tree.records) + (at - 1u64) * size_of<MemoryAliasCell>());
}

MemoryAliasSearch find_memory_alias(OrderedTree tree, array<u64,3> key) on cpu {
    usize at = tree.root;
    usize parent = 0u64;
    i32 order = i32(0);
    while (at != 0u64) {
        let cell = unsafe_position<MemoryAliasCell>(memory_alias_cell(tree, at));
        parent = at;
        if (memory_alias_less(key, cell.key)) {
            at = cell.links.left;
            order = i32(-1);
        } else if (memory_alias_less(cell.key, key)) {
            at = cell.links.right;
            order = i32(1);
        } else {
            return MemoryAliasSearch(at, parent, i32(0));
        }
    }
    return MemoryAliasSearch(0u64, parent, order);
}

void insert_memory_alias(address keys, array<u64,3> key) on cpu {
    let place = find_memory_alias(unsafe_position<OrderedTree>(keys), key);
    if (place.found != 0u64) { return; }
    let width = size_of<MemoryAliasCell>();
    let row = ordered_tree_prepare(keys, width);
    unsafe_position<MemoryAliasCell>(row).key = key;
    ordered_tree_publish(keys, width, place.parent, place.order);
}

bool needed_memory_alias(MemoryAliasCollector collector, MemoryIrRow row, array<u64,3> key) on cpu
    requires nonzero(collector.needed) {
    if (find_memory_alias(unsafe_position<OrderedTree>(collector.keys), key).found != 0u64) { return false; }
    address parent = collector.parent;
    while (parent != address()) {
        let state = unsafe_position<MemoryAliasParent>(parent);
        if (find_memory_alias(unsafe_position<OrderedTree>(state.keys), key).found != 0u64) { return false; }
        parent = state.parent;
    }
    return collector.needed(collector.context, row) != 0;
}

void merge_memory_aliases(address output, address incoming) on cpu {
    let tree = unsafe_position<OrderedTree>(incoming);
    if (unsafe_position<OrderedTree>(output).count == 0u64) {
        unsafe_position<OrderedTree>(output) = tree;
        unsafe_position<OrderedTree>(incoming) = OrderedTree(address(), 0u64, 0u64, 0u64);
        return;
    }
    let width = size_of<MemoryAliasCell>();
    usize at = ordered_tree_first(tree, width);
    while (at != 0u64) {
        insert_memory_alias(output, unsafe_position<MemoryAliasCell>(memory_alias_cell(tree, at)).key);
        at = ordered_tree_next(tree, width, at);
    }
}

extern "C" void collect_memory_origin_aliases(MemoryAliasOrigin origin, MemoryAliasCollector collector, MemoryAliasOriginHost host) on cpu
    requires nonzero(collector.needed) requires nonzero(host.possible) requires nonzero(host.borrow) {
    if (origin.kind == MemoryAliasRoot::Other) { return; }
    if (origin.kind != MemoryAliasRoot::Borrow) {
        let data = origin.kind == MemoryAliasRoot::Data;
        let row = MemoryIrRow(data ? MemoryIrRowKind::Data : MemoryIrRowKind::Local, CallAccess::Value, origin.base, origin.length);
        let key = array<u64,3>(data ? 1u64 : 0u64, row.base, row.length);
        if (needed_memory_alias(collector, row, key) && host.possible(host.context) != 0) {
            insert_memory_alias(collector.keys, key);
        }
        return;
    }

    OrderedTree nested = OrderedTree(address(), 0u64, 0u64, 0u64);
    let parent = MemoryAliasParent(collector.keys, collector.parent);
    host.borrow(origin.borrow, MemoryAliasCollector(address_of(nested), address_of(parent), collector.context, collector.needed));
    if (nested.count != 0u64 && host.possible(host.context) != 0) {
        merge_memory_aliases(collector.keys, address_of(nested));
    }
    fact_tree_clear(address_of(nested));
}

extern "C" void collect_memory_borrow_aliases(MemoryAliasCollector collector, MemoryAliasBorrowHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.position) {
    while (true) {
        let kind = host.next(host.context);
        if (kind == EffectSourceKind::End) { return; }
        if (kind == EffectSourceKind::Position) { host.position(host.context, collector); }
    }
}

extern "C" void project_memory_aliases(MemoryAliasOrigin origin, MemoryAliasOriginHost host, address context,
    cfn<u32,address,MemoryIrRow> needed, address output, cfn<void,address,MemoryIrRow> emit) on cpu
    requires nonzero(host.possible) requires nonzero(host.borrow) requires nonzero(needed) requires nonzero(emit) {
    OrderedTree keys = OrderedTree(address(), 0u64, 0u64, 0u64);
    collect_memory_origin_aliases(origin, MemoryAliasCollector(address_of(keys), address(), context, needed), host);
    let width = size_of<MemoryAliasCell>();
    usize at = ordered_tree_first(keys, width);
    while (at != 0u64) {
        let key = unsafe_position<MemoryAliasCell>(memory_alias_cell(keys, at)).key;
        let kind = key[0] == 0u64 ? MemoryIrRowKind::Local : MemoryIrRowKind::Data;
        emit(output, MemoryIrRow(kind, CallAccess::Value, key[1], key[2]));
        at = ordered_tree_next(keys, width, at);
    }
    fact_tree_clear(address_of(keys));
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
