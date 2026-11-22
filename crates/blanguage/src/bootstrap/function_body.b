module bootstrap.function_body;
extern "C" Cursor start() on cpu {return Cursor(0,0,0u64,0u64);}
extern "C" BeginStep begin_step(Cursor state,BeginFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(facts.void_name!=0&&facts.dataset==0&&facts.tile==0&&facts.view==0){c.phase=3;return BeginStep(c,BeginNeed::Void);}
  if(facts.never!=0){c.phase=3;return BeginStep(c,BeginNeed::Never);}
  c.phase=1;
  if(facts.layout_ready==0){return BeginStep(c,BeginNeed::Type);}
 }
 if(c.phase==1){
  if(facts.owns!=0&&(facts.value_function==0||facts.cpu==0)){return BeginStep(c,BeginNeed::InvalidOwned);}
  if(facts.dataset!=0){
   c.phase=2;
   if(facts.layout_ready==0){return BeginStep(c,BeginNeed::Shape);}
  }else{
   c.phase=3;return BeginStep(c,BeginNeed::Value);
  }
 }
 if(c.phase==2){c.phase=3;return BeginStep(c,BeginNeed::Dataset);}
 if(c.phase==3){c.phase=4;return BeginStep(c,BeginNeed::Early);}
 if(c.phase==4){
  if(facts.early!=0&&facts.cooperative!=0){return BeginStep(c,BeginNeed::InvalidCooperative);}
  c.phase=5;c.index=0u64;
 }
 if(c.phase==5){
  if(facts.early!=0&&c.index<facts.scalars){c.phase=6;return BeginStep(c,BeginNeed::Slot);}
  c.phase=7;
 }
 if(c.phase==6){c.index+=1u64;c.phase=5;return BeginStep(c,BeginNeed::Next);}
 if(c.phase==7){c.phase=8;return BeginStep(c,BeginNeed::Initial);}
 if(c.phase==8){c.phase=9;return BeginStep(c,BeginNeed::Inputs);}
 if(c.phase==9){c.phase=10;return BeginStep(c,BeginNeed::Context);}
 if(c.phase==10){return BeginStep(c,BeginNeed::Complete);}
 return BeginStep(c,BeginNeed::Next);
}
extern "C" SequenceStep sequence_step(Cursor state,u64 count,u32 possible,u32 continuing) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return SequenceStep(c,possible!=0 ? SequenceNeed::StartNext : SequenceNeed::StartStop);}
 if(c.phase==1){
  if(c.index>=count||continuing==0){return SequenceStep(c,SequenceNeed::Complete);}
  c.phase=2;return SequenceStep(c,SequenceNeed::Statement);
 }
 if(c.phase==2){c.phase=3;return SequenceStep(c,SequenceNeed::After);}
 if(c.phase==3){c.index+=1u64;c.phase=1;}
 return SequenceStep(c,SequenceNeed::Next);
}
extern "C" FinishStep finish_step(Cursor state,FinishFacts facts) on cpu {
 Cursor c=state;
 if(c.phase==0){
  if(facts.expected==1&&facts.continuing!=0){return FinishStep(c,FinishNeed::InvalidNever);}
  if(facts.continuing!=0){
   if(facts.expected==2||facts.expected==3){return FinishStep(c,facts.empty!=0 ? FinishNeed::MissingEmpty : FinishNeed::MissingReturn);}
   c.phase=1;return FinishStep(c,FinishNeed::ImplicitReturn);
  }
  c.phase=1;
 }
 if(c.phase==1){c.phase=2;return FinishStep(c,FinishNeed::Context);}
 if(c.phase==2){c.phase=3;return FinishStep(c,FinishNeed::Code);}
 if(c.phase==3){
  if(facts.scoped!=0&&facts.last_exit!=0){c.phase=4;return FinishStep(c,FinishNeed::OtherExits);}
  c.phase=5;
 }
 if(c.phase==4){
  c.phase=5;
  if(facts.other_exits==0){c.flags=1;return FinishStep(c,FinishNeed::PopExit);}
 }
 if(c.phase==5){c.phase=6;return FinishStep(c,facts.scoped!=0&&c.flags==0 ? FinishNeed::Scope : FinishNeed::Append);}
 if(c.phase==6){c.phase=7;return FinishStep(c,FinishNeed::Initial);}
 if(c.phase==7){c.phase=8;return FinishStep(c,FinishNeed::First);}
 if(c.phase==8){
  if(facts.present==0){return FinishStep(c,FinishNeed::NoExit);}
  c.phase=9;return FinishStep(c,FinishNeed::FirstModel);
 }
 if(c.phase==9){
  c.index=0u64;
  if(facts.scoped!=0){c.phase=10;}
  else {c.phase=12;return FinishStep(c,facts.result!=0 ? FinishNeed::Registers : FinishNeed::EmptyRegisters);}
 }
 if(c.phase==10){
  if(c.index<facts.slots&&c.index<facts.scalars){c.phase=11;return FinishStep(c,FinishNeed::Slot);}
  c.phase=12;
 }
 if(c.phase==11){c.index+=1u64;c.phase=10;return FinishStep(c,FinishNeed::Next);}
 if(c.phase==12){c.phase=13;return FinishStep(c,FinishNeed::Written);}
 if(c.phase==13){c.phase=14;return FinishStep(c,FinishNeed::BindFirst);}
 if(c.phase==14){c.phase=15;return FinishStep(c,FinishNeed::NextExit);}
 if(c.phase==15){
  if(facts.present!=0){c.phase=16;return FinishStep(c,FinishNeed::BindOther);}
  c.phase=20;
 }
 if(c.phase==16){c.phase=17;return FinishStep(c,FinishNeed::MergePath);}
 if(c.phase==17){
  c.phase=18;
  if(facts.result!=0&&facts.other_result!=0){return FinishStep(c,FinishNeed::JoinResult);}
 }
 if(c.phase==18){c.phase=14;return FinishStep(c,FinishNeed::MergeBorrowed);}
 if(c.phase==20){c.phase=21;return FinishStep(c,FinishNeed::RebindWritten);}
 if(c.phase==21){c.phase=22;return FinishStep(c,FinishNeed::Snapshot);}
 if(c.phase==22){c.phase=23;if(facts.return_consumer!=0){return FinishStep(c,FinishNeed::OutputStorage);}}
 if(c.phase==23){c.phase=24;return FinishStep(c,FinishNeed::NextModel);}
 if(c.phase==24){
  if(facts.present==0){c.phase=40;}
  else {
   if(facts.model==2){c.phase=37;return FinishStep(c,FinishNeed::Dataset);}
   c.phase=25;return FinishStep(c,FinishNeed::Parameter);
  }
 }
 if(c.phase==25){c.phase=26;return FinishStep(c,FinishNeed::Place);}
 if(c.phase==26){
  c.flags=facts.declared!=0&&facts.owns==0&&facts.top!=0&&facts.cpu!=0 ? 1 : 0;
  c.phase=27;return FinishStep(c,c.flags!=0 ? FinishNeed::DirectGuards : FinishNeed::Guards);
 }
 if(c.phase==27){
  if(facts.declared!=0&&facts.owns!=0){c.phase=34;return FinishStep(c,FinishNeed::Ownership);}
  if(facts.declared!=0&&c.flags!=0){c.part=0u64;c.phase=28;}
  else {c.phase=34;}
 }
 if(c.phase==28){
  if(c.part<facts.guards){c.phase=29;return FinishStep(c,FinishNeed::Guard);}
  c.phase=34;
 }
 if(c.phase==29){
  if(facts.active!=0){c.phase=30;return FinishStep(c,FinishNeed::Initialization);}
  c.part+=1u64;c.phase=28;return FinishStep(c,FinishNeed::Next);
 }
 if(c.phase==30){c.part+=1u64;c.phase=28;return FinishStep(c,FinishNeed::Next);}
 if(c.phase==34){
  c.phase=37;
  if(facts.model==1){return FinishStep(c,FinishNeed::Value);}
  if(facts.model==3){return FinishStep(c,FinishNeed::Owned);}
  return FinishStep(c,FinishNeed::InvalidModel);
 }
 if(c.phase==37){c.phase=38;return FinishStep(c,FinishNeed::Insert);}
 if(c.phase==38){c.phase=23;return FinishStep(c,FinishNeed::Next);}
 if(c.phase==40){c.phase=41;if(facts.scoped!=0&&facts.result!=0){return FinishStep(c,FinishNeed::RebindResult);}}
 if(c.phase==41){c.phase=42;if(facts.top!=0){return FinishStep(c,FinishNeed::Publish);}}
 if(c.phase==42){return FinishStep(c,FinishNeed::Complete);}
 return FinishStep(c,FinishNeed::Next);
}
extern "C" u32 owns_exit(CodeSpan code,ExitHost host) on cpu requires nonzero(host.inspect) {
 u64 i=0u64;while(i<code.count){ExitNode node=host.inspect(code.data,i);
  if(node.kind==1){return 1;}
  if(node.kind==2&&(owns_exit(node.first,host)!=0||owns_exit(node.second,host)!=0)){return 1;}
  i+=1u64;
 }return 0;
}
extern "C" u32 allocate_written(address context,WrittenHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.allocate) requires nonzero(host.finish) {
 u32 consumer=0;
 while(true){
  WrittenItem item=host.next(context);if(item.present==0){return consumer;}
  if(item.kind!=2){consumer=1;}
  u64 index=0u64;while(index<item.fields){host.allocate(context,index);index+=1u64;}
  host.finish(context);
 }
}
extern "C" void models(address context,ModelsHost host) on cpu requires nonzero(host.next) requires nonzero(host.visit) {
 while(host.next(context)!=0){host.visit(context);}
}
extern "C" void snapshot(u64 count,address context,cfn<void,address,u64> append) on cpu requires nonzero(append) {
 u64 index=0u64;while(index<count){append(context,index);index+=1u64;}
}
extern "C" u64 parameter(u64 count,address context,cfn<u32,address,u64> equal) on cpu requires nonzero(equal) {
 u64 index=0u64;while(index<count){if(equal(context,index)!=0){return index;}index+=1u64;}return count;
}


extern "C" BranchStep branch_step(BranchCursor cursor,BranchFacts facts) on cpu {
    BranchCursor c=cursor;
    if (c.phase==0) {
        if (facts.truth!=0) { c.phase=3;return BranchStep(c,facts.truth==2 ? BranchNeed::KnownYes : BranchNeed::KnownNo,0); }
        c.phase=1;
        return BranchStep(c,BranchNeed::DynamicYes,facts.outer_control!=0 && facts.condition_uniform!=0 ? 1 : 0);
    }
    if (c.phase==1) { c.phase=2;return BranchStep(c,BranchNeed::DynamicNo,0); }
    if (c.phase==2) {
        c.phase=3;BranchNeed need;
        if (facts.yes_live!=0 && facts.no_live==0) { need=BranchNeed::OnlyYes; }
        else if (facts.yes_live==0 && facts.no_live!=0) { need=BranchNeed::OnlyNo; }
        else if (facts.yes_live!=0 && facts.no_live!=0) { need=BranchNeed::Both; }
        else { need=BranchNeed::Neither; }
        u32 uniform=facts.outer_control!=0 && (facts.condition_uniform!=0 || (facts.yes_live!=0 && facts.no_live!=0)) ? 1 : 0;
        return BranchStep(c,need,uniform);
    }
    return BranchStep(c,BranchNeed::Complete,0);
}

extern "C" void selection_result(SelectionResultInput input,SelectionHost host) on cpu
 requires nonzero(host.storage) requires nonzero(host.layout) requires nonzero(host.action) {
 if(input.truth!=0){host.action(host.context,SelectionAction::Known,input.truth==2 ? 0 : 1,0u64);return;}
 SelectionStorage values=host.storage(host.context);
 if(values.yes==0&&values.no==0){host.action(host.context,SelectionAction::Emit,0,0u64);return;}
 SelectionLayout layout=host.layout(host.context);u64 index=0u64;
 while(values.yes!=0&&index<layout.slots&&index<values.yes_regs){host.action(host.context,SelectionAction::Store,0,index);index+=1u64;}
 index=0u64;
 while(values.no!=0&&index<layout.slots&&index<values.no_regs){host.action(host.context,SelectionAction::Store,1,index);index+=1u64;}
 host.action(host.context,SelectionAction::Emit,0,0u64);index=0u64;
 while(index<layout.slots&&index<layout.components){host.action(host.context,SelectionAction::Load,0,index);index+=1u64;}
 host.action(host.context,SelectionAction::BeginPaths,0,0u64);
 if(values.yes!=0){host.action(host.context,SelectionAction::Aliases,0,0u64);}
 if(values.no!=0){host.action(host.context,SelectionAction::Aliases,1,0u64);}
 host.action(host.context,SelectionAction::Path,0,0u64);
 host.action(host.context,values.yes!=0&&values.no!=0 ? SelectionAction::CommonProof : SelectionAction::CaptureProof,0,0u64);
 if(values.yes!=0&&values.no!=0){host.action(host.context,SelectionAction::Join,0,0u64);}
 else {host.action(host.context,SelectionAction::Take,values.yes!=0 ? 0 : 1,0u64);}
 u32 uniform=input.uniform!=0&&(values.yes==0||values.yes_uniform!=0)&&(values.no==0||values.no_uniform!=0) ? 1 : 0;
 host.action(host.context,SelectionAction::Finish,uniform,0u64);
}

extern "C" void selection_merge(SelectionMergeInput input,SelectionMergeHost host) on cpu
 requires nonzero(host.next_region) requires nonzero(host.next_write) requires nonzero(host.owner_possible) requires nonzero(host.next_owner) requires nonzero(host.action) {
 bool yes=input.yes_live!=0;bool no=input.no_live!=0;
 if(input.truth==2||(input.truth==0&&yes&&!no)){host.action(host.context,SelectionMergeAction::RememberYes,0);}
 else if(input.truth==1||(input.truth==0&&!yes&&no)){host.action(host.context,SelectionMergeAction::RememberNo,0);}
 host.action(host.context,SelectionMergeAction::Initialize,0);host.action(host.context,SelectionMergeAction::Split,0);
 u32 side=0;while(side<2){
  bool retain=side==0 ? input.truth!=1 : input.truth!=2;
  if(retain){
   host.action(host.context,SelectionMergeAction::BeginRegions,side);
   while(host.next_region(host.context)!=0){
    host.action(host.context,SelectionMergeAction::Region,0);
    while(true){SelectionWriteFacts write=host.next_write(host.context);if(write.present==0){break;}
     SelectionMergeAction action=write.occupied==0 ? SelectionMergeAction::InsertWrite : (write.stored!=0&&write.value!=0 ? SelectionMergeAction::JoinWrite : SelectionMergeAction::ClearWrite);
     host.action(host.context,action,0);
    }
   }
  }
  host.action(host.context,SelectionMergeAction::EndRegions,side);side+=1;
 }
 host.action(host.context,SelectionMergeAction::Environment,0);
 if(input.truth==2){host.action(host.context,SelectionMergeAction::YesEffects,0);}
 else if(input.truth==1){host.action(host.context,SelectionMergeAction::NoEnvironment,0);}
 else {host.action(host.context,SelectionMergeAction::MergeEffects,0);}
 if(input.truth!=0){
  host.action(host.context,input.truth==2 ? SelectionMergeAction::YesPath : SelectionMergeAction::NoPath,0);
  if(input.returns!=0){host.action(host.context,SelectionMergeAction::SelectExits,input.truth==2 ? 1 : 0);}
  host.action(host.context,input.truth==2 ? SelectionMergeAction::YesFlow : SelectionMergeAction::NoFlow,0);
  host.action(host.context,SelectionMergeAction::RestoreInit,0);
 }else{
  host.action(host.context,yes ? (no ? SelectionMergeAction::EitherPath : SelectionMergeAction::YesPath) : (no ? SelectionMergeAction::NoPath : SelectionMergeAction::ImpossiblePath),0);
  if(yes&&no){host.action(host.context,SelectionMergeAction::JoinEnvironment,0);}
  if(yes&&!no){host.action(host.context,SelectionMergeAction::RestoreInit,0);}
  else if(!yes&&no){host.action(host.context,SelectionMergeAction::NoEnvironment,0);host.action(host.context,SelectionMergeAction::RestoreInit,0);}
  host.action(host.context,SelectionMergeAction::UnionFlow,0);
 }
 host.action(host.context,SelectionMergeAction::Control,input.uniform);host.action(host.context,SelectionMergeAction::PrepareOwners,0);
 side=0;while(side<2){
  if(host.owner_possible(host.context,side)!=0){host.action(host.context,SelectionMergeAction::BeginOwners,side);while(host.next_owner(host.context)!=0){host.action(host.context,SelectionMergeAction::HoldOwner,0);}}
  host.action(host.context,SelectionMergeAction::EndOwners,side);side+=1;
 }
}

extern "C" SelectionStep selection_step(SelectionCursor cursor,SelectionFacts facts) on cpu {
 SelectionCursor c=cursor;
 if(c.phase==0){c.phase=1;return SelectionStep(c,SelectionNeed::Prepare,0);}
 if(c.phase==1){c.phase=2;return SelectionStep(c,SelectionNeed::Modified,0);}
 if(c.phase==2){c.phase=3;return SelectionStep(c,SelectionNeed::EnterYes,facts.outer!=0&&facts.condition!=0 ? 1 : 0);}
 if(c.phase==3){c.phase=4;return SelectionStep(c,SelectionNeed::Yes,0);}
 if(c.phase==4){
  if(facts.continuing!=0&&facts.present==0){return SelectionStep(c,SelectionNeed::Missing,0);}
  c.phase=5;return SelectionStep(c,SelectionNeed::EnterNo,0);
 }
 if(c.phase==5){c.phase=6;return SelectionStep(c,SelectionNeed::No,0);}
 if(c.phase==6){
  if(facts.continuing!=0&&facts.present==0){return SelectionStep(c,SelectionNeed::Missing,0);}
  c.phase=7;if(facts.present!=0&&facts.other!=0){return SelectionStep(c,SelectionNeed::Same,0);}
 }
 return SelectionStep(c,SelectionNeed::Finish,0);
}

bool temporary_owner_name(OwnerName name) on cpu {
 if(name.length<10u64){return false;}u64 index=0u64;
 while(index<10u64){u64 part=index<8u64 ? 7954884667833999616u64 : 14964u64;u64 shift=(index<8u64 ? index : index-8u64)*8u64;
  if(unsafe_position<u8>(unsafe_address(address_bits(name.data)+index))!=u8(part>>shift)){return false;}index+=1u64;
 }return true;
}
extern "C" OwnerStep owner_step(OwnerCursor cursor,OwnerFacts facts) on cpu {
 OwnerCursor c=cursor;
 if(c.phase==0){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}
 if(c.phase==1){if(facts.present==0){c.phase=15;return OwnerStep(c,OwnerNeed::Complete);}if(!temporary_owner_name(facts.name)){return OwnerStep(c,OwnerNeed::NextBinding);}c.phase=2;return OwnerStep(c,OwnerNeed::Original);}
 if(c.phase==2){if(facts.answer!=0){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}c.phase=3;return OwnerStep(c,OwnerNeed::Kind);}
 if(c.phase==3){if(facts.kind!=1&&facts.kind!=2){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}c.phase=4;return OwnerStep(c,OwnerNeed::MayOwn);}
 if(c.phase==4){if(facts.answer==0){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}if(facts.kind==1){c.phase=5;return OwnerStep(c,OwnerNeed::Reference);}c.phase=6;return OwnerStep(c,OwnerNeed::Borrow);}
 if(c.phase==5){if(facts.answer!=0){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}c.phase=6;return OwnerStep(c,OwnerNeed::Borrow);}
 if(c.phase==6){c.phase=7;return OwnerStep(c,OwnerNeed::NextResource);}
 if(c.phase==7){if(facts.present!=0){c.phase=6;return OwnerStep(c,OwnerNeed::UnionResource);}c.phase=8;return OwnerStep(c,OwnerNeed::Restrict);}
 if(c.phase==8){c.phase=9;return OwnerStep(c,OwnerNeed::BorrowPending);}
 if(c.phase==9){if(facts.answer!=0){c.phase=14;return OwnerStep(c,OwnerNeed::Keep);}c.phase=10;return OwnerStep(c,OwnerNeed::ReferencesPending);}
 if(c.phase==10){if(facts.answer!=0){c.phase=14;return OwnerStep(c,OwnerNeed::Keep);}c.phase=11;return OwnerStep(c,OwnerNeed::StartSources);}
 if(c.phase==11){c.phase=12;return OwnerStep(c,OwnerNeed::NextSource);}
 if(c.phase==12){if(facts.present==0){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}c.phase=13;return OwnerStep(c,OwnerNeed::Overlap);}
 if(c.phase==13){if(facts.answer!=0){c.phase=14;return OwnerStep(c,OwnerNeed::Keep);}c.phase=12;return OwnerStep(c,OwnerNeed::NextSource);}
 if(c.phase==14){c.phase=1;return OwnerStep(c,OwnerNeed::NextBinding);}
 return OwnerStep(c,OwnerNeed::Complete);
}
u64 owner_ordinal(OwnerName name) on cpu {u64 index=10u64;u64 value=0u64;while(index<name.length){value=value*10u64+u64(unsafe_position<u8>(unsafe_address(address_bits(name.data)+index)))-48u64;index+=1u64;}return value;}
void sift_owner_names(u64 root,u64 end,OwnerNamesHost host) on cpu
 requires nonzero(host.name) requires nonzero(host.exchange) {
 u64 at=root;while(at<end/2u64){u64 child=at*2u64+1u64;
  if(child+1u64<end&&owner_ordinal(host.name(host.context,child))<owner_ordinal(host.name(host.context,child+1u64))){child+=1u64;}
  if(owner_ordinal(host.name(host.context,at))>=owner_ordinal(host.name(host.context,child))){break;}
  host.exchange(host.context,at,child);at=child;
 }
}
extern "C" void move_owners(u64 count,OwnerNamesHost host) on cpu
 requires nonzero(host.name) requires nonzero(host.exchange) requires nonzero(host.begin) requires nonzero(host.take) {
 u64 root=count/2u64;while(root>0u64){root-=1u64;sift_owner_names(root,count,host);}
 u64 end=count;while(end>1u64){end-=1u64;host.exchange(host.context,0u64,end);sift_owner_names(0u64,end,host);}
 host.begin(host.context);u64 index=0u64;while(index<count){host.take(host.context);index+=1u64;}
}
