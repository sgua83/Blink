module bootstrap.call_execution;
extern "C" Phase invoke_start() on cpu {return Phase(0);}
extern "C" InvokeNeed invoke_step(inout Phase cursor,InvokeFacts facts) on cpu {
 if(cursor.phase==1){return facts.kernel==0 ? InvokeNeed::Native : InvokeNeed::Domain;}
 if(facts.external!=0){return InvokeNeed::External;}
 if(facts.compiled!=0&&facts.domain==0){
  if(facts.application==0&&facts.target!=CallTarget::Cpu){return InvokeNeed::InvalidCompiledTarget;}
  return InvokeNeed::Native;
 }
 if(facts.domain!=0){
  if(facts.entry==0){return InvokeNeed::InvalidDomain;}
  if(facts.callee_target==CallTarget::Cpu){cursor.phase=1;return InvokeNeed::Kernel;}
  return InvokeNeed::Domain;
 }
 if(facts.kind==CallKind::Cooperative){return InvokeNeed::InvalidCooperative;}
 if(facts.kind==CallKind::Constant){return InvokeNeed::InvalidConstant;}
 if((facts.callee_target==CallTarget::Cpu&&facts.target==CallTarget::Cpu)||(facts.entry!=0&&facts.callee_target!=CallTarget::Gpu)){return InvokeNeed::Native;}
 return InvokeNeed::Inline;
}
extern "C" Phase cooperate_start() on cpu {return Phase(0);}
extern "C" CooperateNeed cooperate_step(inout Phase cursor,CooperateFacts facts) on cpu {
 if(cursor.phase==0){
  if(facts.root==0){return CooperateNeed::InvalidRoot;}
  cursor.phase=1;return CooperateNeed::Resolve;
 }
 if(cursor.phase==1){
  if(facts.name==0){return CooperateNeed::Unknown;}
  if(facts.kind!=CallKind::Cooperative){return CooperateNeed::InvalidKind;}
  if(facts.arguments!=facts.parameters){return CooperateNeed::CountMismatch;}
  cursor.phase=2;return CooperateNeed::Arguments;
 }
 if(cursor.phase==2){
  if(facts.present==0){return CooperateNeed::Empty;}
  cursor.phase=3;
  if(facts.explicit!=0){return CooperateNeed::Explicit;}
 }
 if(cursor.phase==3){cursor.phase=4;return CooperateNeed::Specialize;}
 if(cursor.phase==4){cursor.phase=5;return CooperateNeed::Inline;}
 return CooperateNeed::Return;
}
extern "C" ParameterError parameter_error(ParameterFacts facts) on cpu {
 if(facts.owns!=0&&(facts.tile!=0||(facts.access!=CallAccess::Value&&facts.access!=CallAccess::Read&&facts.access!=CallAccess::Write&&facts.access!=CallAccess::InOut)||facts.kind!=CallKind::Value||facts.target!=CallTarget::Cpu)){return ParameterError::Owned;}
 if(facts.tile!=0&&facts.views!=0){return ParameterError::Tile;}
 if(facts.access==CallAccess::Atomic&&facts.tile==0&&facts.views!=0){return ParameterError::Atomic;}
 return ParameterError::None;
}
extern "C" u32 environment_plain(address iterator,u32 facts,EnvironmentHost host) on cpu requires nonzero(host.next) requires nonzero(host.borrows) requires nonzero(host.owns) requires nonzero(host.facts) {
 while(true){
  BindingItem item=host.next(iterator);
  if(item.present==0){return 1;}
  if(address_bits(item.value)==0u64){return 0;}
  if(host.borrows(item.value)!=0||host.owns(item.value)!=0){return 0;}
  if(facts!=0&&host.facts(item.value)!=0){return 0;}
 }
}
extern "C" u32 keep_position(address iterator,address position,PositionHost host) on cpu requires nonzero(host.next) {
 while(true){u32 item=host.next(iterator,position);if(item==0){return 1;}if(item==2){return 0;}}
}
extern "C" InlineCursor inline_start() on cpu {return InlineCursor(0u64,0u64,0,0,0);}
bool plain_result(InlineFacts facts) on cpu {return facts.result_borrows==0&&facts.result_owns==0;}
bool dataset_access(CallAccess wanted,CallAccess actual) on cpu {
 return (wanted==CallAccess::Read&&(actual==CallAccess::Read||actual==CallAccess::InOut))
  ||(wanted==CallAccess::Write&&(actual==CallAccess::Write||actual==CallAccess::InOut))
  ||(wanted==CallAccess::Atomic&&actual==CallAccess::Atomic)
  ||(wanted==CallAccess::InOut&&actual==CallAccess::InOut);
}
extern "C" InlineStep inline_step(InlineCursor state,InlineFacts facts) on cpu {
 InlineCursor cursor=state;
  if(cursor.phase==0){
   if(facts.callee_target!=CallTarget::Compute&&facts.callee_target!=facts.target){return InlineStep(cursor,InlineNeed::InvalidTarget);}
   cursor.phase=1;return InlineStep(cursor,InlineNeed::Recursion);
  }
  if(cursor.phase==1){
   if(facts.recursive!=0){return InlineStep(cursor,facts.target!=CallTarget::Gpu&&facts.kind==CallKind::Value ? InlineNeed::Native : InlineNeed::InvalidRecursion);}
   cursor.phase=2;return InlineStep(cursor,InlineNeed::ResultName);
  }
  if(cursor.phase==2){
   if(cursor.index>=facts.parameters||cursor.index>=facts.arguments){cursor.phase=20;return InlineStep(cursor,InlineNeed::Resolve);}
   cursor.phase=4;return InlineStep(cursor,InlineNeed::Parameter);
  }
  if(cursor.phase==4){
   cursor.phase=5;if(facts.argument==ArgumentKind::Scoped){return InlineStep(cursor,InlineNeed::UnwrapScoped);}
  }
  if(cursor.phase==5){
   cursor.phase=6;if(facts.argument==ArgumentKind::Value&&facts.access==CallAccess::Read){return InlineStep(cursor,InlineNeed::MaterializeRead);}
  }
  if(cursor.phase==6){
   if(facts.acceleration!=0){
    if(facts.argument!=ArgumentKind::Acceleration){return InlineStep(cursor,InlineNeed::InvalidAcceleration);}
    cursor.phase=98;return InlineStep(cursor,InlineNeed::Acceleration);
   }
   if(facts.texture!=0){
    if(facts.argument!=ArgumentKind::Texture){return InlineStep(cursor,InlineNeed::InvalidTexture);}
    cursor.phase=98;return InlineStep(cursor,InlineNeed::Texture);
   }
   cursor.phase=7;return InlineStep(cursor,InlineNeed::ParameterType);
  }
  if(cursor.phase==7){
   if(facts.dataset!=0&&facts.tile==0&&facts.argument==ArgumentKind::Dataset){cursor.phase=8;return InlineStep(cursor,InlineNeed::DatasetType);}
   if(facts.dataset==0&&facts.tile==0){
    if(facts.argument==ArgumentKind::Atomic&&facts.access==CallAccess::Atomic){cursor.phase=98;return InlineStep(cursor,InlineNeed::Atomic);}
    if(facts.argument==ArgumentKind::Reference&&(facts.access==CallAccess::Read||facts.access==CallAccess::Write||facts.access==CallAccess::InOut)){cursor.phase=11;return InlineStep(cursor,InlineNeed::ReferenceType);}
    if(facts.argument==ArgumentKind::Value&&facts.access==CallAccess::Value){cursor.phase=98;return InlineStep(cursor,InlineNeed::Value);}
   }
   if(facts.dataset==0&&facts.tile!=0&&facts.argument==ArgumentKind::Tile){cursor.phase=12;return InlineStep(cursor,InlineNeed::TileType);}
   return InlineStep(cursor,InlineNeed::InvalidArgument);
  }
  if(cursor.phase==8){
   if(!dataset_access(facts.access,facts.actual_access)){return InlineStep(cursor,InlineNeed::InvalidDatasetAccess);}
   cursor.phase=9;if(facts.access==CallAccess::Write){return InlineStep(cursor,InlineNeed::DatasetWrite);}
  }
  if(cursor.phase==9){cursor.phase=10;return InlineStep(cursor,InlineNeed::DatasetAccess);}
  if(cursor.phase==10){
   if(facts.counted!=0){cursor.phase=13;return InlineStep(cursor,InlineNeed::DatasetShape);}
   cursor.phase=98;return InlineStep(cursor,InlineNeed::Dataset);
  }
  if(cursor.phase==13){
   if(facts.expected_count.present!=facts.actual_count.present||(facts.expected_count.present!=0&&facts.expected_count.value!=facts.actual_count.value)){return InlineStep(cursor,InlineNeed::InvalidShape);}
   cursor.phase=98;return InlineStep(cursor,InlineNeed::Dataset);
  }
  if(cursor.phase==11){
   cursor.phase=98;
   if(facts.access==CallAccess::Read){return InlineStep(cursor,InlineNeed::ReferenceRead);}
   if(facts.access==CallAccess::Write){return InlineStep(cursor,InlineNeed::ReferenceWrite);}
   return InlineStep(cursor,InlineNeed::ReferenceInOut);
  }
  if(cursor.phase==12){
   if(facts.access!=facts.actual_access){return InlineStep(cursor,InlineNeed::InvalidTileAccess);}
   cursor.phase=98;return InlineStep(cursor,InlineNeed::Tile);
  }
  if(cursor.phase==98){cursor.index+=1u64;cursor.phase=2;}
  if(cursor.phase==20){cursor.phase=21;return InlineStep(cursor,InlineNeed::Prove);}
  if(cursor.phase==21){cursor.phase=22;return InlineStep(cursor,InlineNeed::Assume);}
  if(cursor.phase==22){cursor.phase=23;return InlineStep(cursor,InlineNeed::EnterEnvironment);}
  if(cursor.phase==23){cursor.phase=24;return InlineStep(cursor,InlineNeed::AssignedNames);}
  if(cursor.phase==24){cursor.phase=25;return InlineStep(cursor,facts.kind==CallKind::Cooperative ? InlineNeed::EnterCooperativeContext : InlineNeed::EnterContext);}
  if(cursor.phase==25){cursor.phase=26;return InlineStep(cursor,InlineNeed::InitializationInputs);}
  if(cursor.phase==26){
   cursor.initialization=facts.input_plain;
   if(cursor.initialization!=0&&facts.unit==0&&facts.never==0){
    if(facts.result_ready==0){cursor.phase=27;return InlineStep(cursor,InlineNeed::ResultProperties);}
    cursor.initialization=plain_result(facts) ? 1 : 0;
   }
   cursor.phase=28;
  }
  if(cursor.phase==27){cursor.initialization=plain_result(facts) ? 1 : 0;cursor.phase=28;}
  if(cursor.phase==28){cursor.phase=29;if(cursor.initialization!=0){return InlineStep(cursor,InlineNeed::IsolateInitialization);}}
  if(cursor.phase==29){cursor.phase=30;return InlineStep(cursor,InlineNeed::Materialize);}
  if(cursor.phase==30){cursor.phase=31;return InlineStep(cursor,InlineNeed::OutputInputs);}
  if(cursor.phase==31){cursor.phase=32;return InlineStep(cursor,InlineNeed::BoundsInputs);}
  if(cursor.phase==32){
   cursor.bounds=facts.input_plain;
   if(cursor.bounds!=0&&facts.unit==0&&facts.never==0){
    if(facts.result_ready==0){cursor.phase=33;return InlineStep(cursor,InlineNeed::ResultProperties);}
    cursor.bounds=plain_result(facts) ? 1 : 0;
    if(cursor.bounds!=0){cursor.phase=70;return InlineStep(cursor,InlineNeed::ResultFacts);}
   }
   cursor.phase=34;
  }
  if(cursor.phase==33){cursor.bounds=plain_result(facts) ? 1 : 0;if(cursor.bounds!=0){cursor.phase=70;return InlineStep(cursor,InlineNeed::ResultFacts);}cursor.phase=34;}
  if(cursor.phase==70){cursor.bounds=facts.result_facts==0 ? 1 : 0;cursor.phase=34;}
  if(cursor.phase==34){cursor.phase=35;return InlineStep(cursor,InlineNeed::EnterPath);}
  if(cursor.phase==35){cursor.phase=36;return InlineStep(cursor,InlineNeed::Body);}
  if(cursor.phase==36){cursor.phase=37;if(cursor.initialization!=0){return InlineStep(cursor,InlineNeed::RestoreInitialization);}}
  if(cursor.phase==37){cursor.phase=38;return InlineStep(cursor,InlineNeed::MarkResult);}
  if(cursor.phase==38){cursor.phase=39;return InlineStep(cursor,InlineNeed::PopCall);}
  if(cursor.phase==39){
   if(facts.continuing==0){cursor.phase=50;return InlineStep(cursor,InlineNeed::RestoreEnvironment);}
   cursor.phase=40;return InlineStep(cursor,cursor.bounds!=0 ? InlineNeed::RestorePath : InlineNeed::JoinPath);
  }
  if(cursor.phase==40){cursor.phase=41;cursor.index=0u64;return InlineStep(cursor,InlineNeed::ReturnedStorage);}
  if(cursor.phase==41){
   if(cursor.index<facts.items){cursor.phase=42;return InlineStep(cursor,InlineNeed::Update);}
   cursor.phase=43;return InlineStep(cursor,InlineNeed::RestoreEnvironment);
  }
  if(cursor.phase==42){cursor.index+=1u64;cursor.phase=41;}
  if(cursor.phase==43){cursor.index=0u64;cursor.phase=44;}
  if(cursor.phase==44){
   if(cursor.index>=facts.items){cursor.phase=51;return InlineStep(cursor,InlineNeed::FilterInitialization);}
   cursor.part=0u64;cursor.phase=45;return InlineStep(cursor,InlineNeed::InspectUpdate);
  }
  if(cursor.phase==45){
   if(facts.access==CallAccess::Write&&cursor.part<facts.positions){cursor.phase=46;return InlineStep(cursor,InlineNeed::InspectPosition);}
   cursor.index+=1u64;cursor.phase=44;
  }
  if(cursor.phase==46){
   if(facts.parameter_position!=0){cursor.phase=47;return InlineStep(cursor,InlineNeed::InsertScope);}
   cursor.part+=1u64;cursor.phase=45;
  }
  if(cursor.phase==47){cursor.part+=1u64;cursor.phase=45;}
  if(cursor.phase==51){cursor.index=0u64;cursor.phase=52;}
  if(cursor.phase==52){
   if(cursor.index>=facts.items){cursor.phase=56;cursor.index=0u64;return InlineStep(cursor,InlineNeed::BeginProofs);}
   cursor.phase=53;return InlineStep(cursor,InlineNeed::OutputDeclared);
  }
  if(cursor.phase==53){cursor.phase=54;if(facts.declared!=0){return InlineStep(cursor,InlineNeed::ForgetOutput);}}
  if(cursor.phase==54){cursor.phase=55;return InlineStep(cursor,InlineNeed::InitializeOutput);}
  if(cursor.phase==55){cursor.index+=1u64;cursor.phase=52;}
  if(cursor.phase==56){
   if(cursor.index<facts.items){cursor.phase=57;return InlineStep(cursor,InlineNeed::AssignProof);}
   cursor.phase=50;
  }
  if(cursor.phase==57){cursor.index+=1u64;cursor.phase=56;}
  if(cursor.phase==50){cursor.phase=60;cursor.index=0u64;return InlineStep(cursor,InlineNeed::RestoreContext);}
  if(cursor.phase==60){
   if(cursor.index<facts.held){cursor.phase=61;return InlineStep(cursor,InlineNeed::FinishScoped);}
   return InlineStep(cursor,InlineNeed::Complete);
  }
  if(cursor.phase==61){cursor.index+=1u64;cursor.phase=60;}
 return InlineStep(cursor,InlineNeed::Next);
}

bool domain_reads(CallAccess access) on cpu {return access==CallAccess::Read||access==CallAccess::InOut||access==CallAccess::Atomic;}
bool domain_writes(CallAccess access) on cpu {return access==CallAccess::Write||access==CallAccess::InOut||access==CallAccess::Atomic;}
extern "C" DomainParameterStep domain_parameter_step(DomainParameterCursor state,DomainParameterFacts facts) on cpu {
 DomainParameterCursor cursor=state;
 while(true){
  if(cursor.phase==0){
   if(cursor.index<facts.parameters){cursor.phase=1;return DomainParameterStep(cursor,DomainParameterNeed::Parameter);}
   return DomainParameterStep(cursor,cursor.alias_conflict!=0 ? DomainParameterNeed::InvalidAlias : DomainParameterNeed::Complete);
  }
  if(cursor.phase==1){
   if(facts.argument==ArgumentKind::Texture){if(facts.access!=CallAccess::Read){return DomainParameterStep(cursor,DomainParameterNeed::InvalidTexture);}cursor.phase=2;return DomainParameterStep(cursor,DomainParameterNeed::Texture);}
   if(facts.argument==ArgumentKind::Acceleration){if(facts.acceleration==0||facts.dataset!=0||facts.access!=CallAccess::Read){return DomainParameterStep(cursor,DomainParameterNeed::InvalidAcceleration);}cursor.phase=98;return DomainParameterStep(cursor,DomainParameterNeed::BindRoot);}
   if(facts.argument==ArgumentKind::Dataset){cursor.phase=3;return DomainParameterStep(cursor,DomainParameterNeed::DatasetType);}
   cursor.phase=6;return DomainParameterStep(cursor,DomainParameterNeed::ValueType);
  }
  if(cursor.phase==2){if(facts.present==0){return DomainParameterStep(cursor,DomainParameterNeed::InvalidTexture);}cursor.phase=98;return DomainParameterStep(cursor,DomainParameterNeed::BindRoot);}
  if(cursor.phase==3){if(facts.dataset==0||facts.same==0){return DomainParameterStep(cursor,DomainParameterNeed::InvalidDataset);}if(facts.counted!=0){cursor.phase=4;return DomainParameterStep(cursor,DomainParameterNeed::Shape);}cursor.phase=5;}
  if(cursor.phase==4){if(facts.expected_count.present!=facts.actual_count.present||(facts.expected_count.present!=0&&facts.expected_count.value!=facts.actual_count.value)){return DomainParameterStep(cursor,DomainParameterNeed::InvalidShape);}cursor.phase=5;}
  if(cursor.phase==5){if((domain_reads(facts.access)&&!domain_reads(facts.actual_access))||(domain_writes(facts.access)&&!domain_writes(facts.actual_access))){return DomainParameterStep(cursor,DomainParameterNeed::InvalidAccess);}cursor.phase=7;return DomainParameterStep(cursor,DomainParameterNeed::BindData);}
  if(cursor.phase==6){if(facts.dataset!=0||facts.same==0){return DomainParameterStep(cursor,DomainParameterNeed::InvalidValue);}cursor.phase=98;return DomainParameterStep(cursor,DomainParameterNeed::BindRoot);}
  if(cursor.phase==7){
   if(facts.present!=0&&(domain_writes(facts.previous)||domain_writes(facts.access))
      &&!(facts.previous==CallAccess::Atomic&&facts.access==CallAccess::Atomic)){
    cursor.alias_conflict=1;
   }
   cursor.phase=98;
  }
  if(cursor.phase==98){cursor.index+=1u64;cursor.phase=0;}
 }
}

extern "C" DomainCallStep domain_call_step(DomainCallCursor state,DomainCallFacts facts) on cpu {
 DomainCallCursor cursor=state;
 if(cursor.phase==0){
  if(facts.domain==0){return DomainCallStep(cursor,DomainCallNeed::InvalidDomain);}
  if(facts.arguments!=facts.parameters){return DomainCallStep(cursor,DomainCallNeed::InvalidCount);}
  if(facts.target!=facts.callee_target&&facts.callee_target!=CallTarget::Compute){return DomainCallStep(cursor,DomainCallNeed::InvalidTarget);}
  cursor.phase=1;
 }
 if(cursor.phase==2){cursor.index+=1u64;cursor.phase=1;}
 if(cursor.phase==1){if(cursor.index<facts.arguments){cursor.phase=2;return DomainCallStep(cursor,DomainCallNeed::Argument);}cursor.phase=3;}
 if(cursor.phase==3){cursor.phase=4;return DomainCallStep(cursor,DomainCallNeed::Specialize);}
 return DomainCallStep(cursor,DomainCallNeed::Bind);
}
extern "C" DomainInstanceNeed domain_instance_step(inout Phase cursor,u32 explicit) on cpu {
 if(cursor.phase==0){cursor.phase=1;return DomainInstanceNeed::Actuals;}
 if(cursor.phase==1){cursor.phase=2;if(explicit!=0){return DomainInstanceNeed::Explicit;}}
 if(cursor.phase==2){cursor.phase=3;return DomainInstanceNeed::Specialize;}
 return DomainInstanceNeed::Complete;
}
extern "C" void domain_actuals(u64 count,address context,cfn<void,address,u64> append) on cpu requires nonzero(append) {
 u64 index=0u64;while(index<count){append(context,index);index+=1u64;}
}

extern "C" DomainExecutionStep domain_execution_step(DomainExecutionCursor state,DomainExecutionFacts facts) on cpu {
 DomainExecutionCursor cursor=state;
 while(true){
  if(cursor.phase==0){cursor.phase=1;return DomainExecutionStep(cursor,DomainExecutionNeed::Contracts);}
  if(cursor.phase==1){cursor.phase=2;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Lengths);}
  if(cursor.phase==2){if(cursor.index<facts.items){cursor.phase=3;return DomainExecutionStep(cursor,DomainExecutionNeed::LengthPair);}cursor.phase=4;}
  if(cursor.phase==3){if(facts.valid==0){return DomainExecutionStep(cursor,DomainExecutionNeed::InvalidLength);}cursor.index+=1u64;cursor.phase=2;}
  if(cursor.phase==4){cursor.phase=5;return DomainExecutionStep(cursor,DomainExecutionNeed::Requirements);}
  if(cursor.phase==5){cursor.phase=6;return DomainExecutionStep(cursor,DomainExecutionNeed::Elements);}
  if(cursor.phase==6){cursor.phase=7;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::ProveElements);}
  if(cursor.phase==7){if(cursor.index<facts.items){cursor.phase=8;return DomainExecutionStep(cursor,DomainExecutionNeed::Prefix);}cursor.phase=9;}
  if(cursor.phase==8){cursor.index+=1u64;cursor.phase=7;}
  if(cursor.phase==9){cursor.phase=10;return DomainExecutionStep(cursor,DomainExecutionNeed::Source);}
  if(cursor.phase==10){if(facts.compiled!=0){cursor.phase=11;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Compiled);}cursor.phase=40;return DomainExecutionStep(cursor,DomainExecutionNeed::Enter);}
  if(cursor.phase==11){
   if(cursor.index<facts.items){cursor.index+=1u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Argument);}
   cursor.phase=15;return DomainExecutionStep(cursor,DomainExecutionNeed::Segment);
  }
  if(cursor.phase==15){cursor.phase=16;if(facts.segment_start!=facts.stage){return DomainExecutionStep(cursor,DomainExecutionNeed::SegmentFacts);}}
  if(cursor.phase==16){cursor.phase=17;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Writes);}
  if(cursor.phase==17){if(cursor.index<facts.items){cursor.phase=18;return DomainExecutionStep(cursor,DomainExecutionNeed::Write);}cursor.phase=19;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Outputs);}
  if(cursor.phase==18){cursor.index+=1u64;cursor.phase=17;}
  if(cursor.phase==19){if(cursor.index<facts.items){cursor.phase=20;return DomainExecutionStep(cursor,DomainExecutionNeed::Output);}cursor.phase=25;return DomainExecutionStep(cursor,DomainExecutionNeed::Publish);}
  if(cursor.phase==20){cursor.phase=21;if(facts.present!=0){return DomainExecutionStep(cursor,DomainExecutionNeed::OutputDomain);}}
  if(cursor.phase==21){cursor.phase=22;return DomainExecutionStep(cursor,DomainExecutionNeed::AppendOutput);}
  if(cursor.phase==22){cursor.index+=1u64;cursor.phase=19;}
  if(cursor.phase==25){cursor.phase=26;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Guarantees);}
  if(cursor.phase==26){if(cursor.index<facts.items){cursor.phase=27;return DomainExecutionStep(cursor,DomainExecutionNeed::Guarantee);}cursor.phase=44;}
  if(cursor.phase==27){if(facts.domain==facts.count){return DomainExecutionStep(cursor,DomainExecutionNeed::InvalidPrefix);}cursor.index+=1u64;cursor.phase=26;}
  if(cursor.phase==40){cursor.phase=41;return DomainExecutionStep(cursor,DomainExecutionNeed::Expand);}
  if(cursor.phase==41){cursor.phase=42;return DomainExecutionStep(cursor,DomainExecutionNeed::Proved);}
  if(cursor.phase==42){cursor.phase=44;return DomainExecutionStep(cursor,DomainExecutionNeed::Leave);}
  if(cursor.phase==44){cursor.phase=45;cursor.index=0u64;return DomainExecutionStep(cursor,DomainExecutionNeed::Grants);}
  if(cursor.phase==45){if(cursor.index<facts.items){cursor.phase=46;return DomainExecutionStep(cursor,DomainExecutionNeed::Grant);}return DomainExecutionStep(cursor,DomainExecutionNeed::Complete);}
  if(cursor.phase==46){cursor.index+=1u64;cursor.phase=45;}
 }
}
extern "C" DomainWriteStep domain_write_step(DomainExecutionCursor state,DomainWriteFacts facts) on cpu {
 DomainExecutionCursor cursor=state;
 if(cursor.phase==0){cursor.phase=1;if(facts.output!=0){return DomainWriteStep(cursor,DomainWriteNeed::Proof);}}
 if(cursor.phase==2){cursor.index+=1u64;cursor.phase=1;}
 if(cursor.index<facts.regions){cursor.phase=2;return DomainWriteStep(cursor,DomainWriteNeed::Region);}
 return DomainWriteStep(cursor,DomainWriteNeed::Complete);
}
