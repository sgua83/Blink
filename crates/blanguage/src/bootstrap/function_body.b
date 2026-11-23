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

extern "C" WhileStep while_step(WhileCursor cursor,WhileFacts facts) on cpu {
 WhileCursor c=cursor;
 if(c.phase==0){c.phase=1;return WhileStep(c,WhileNeed::Enter,0,0);}
 if(c.phase==1){c.phase=2;return WhileStep(c,WhileNeed::Writes,0,0);}
 if(c.phase==2){c.phase=3;return WhileStep(c,WhileNeed::Condition,0,0);}
 if(c.phase==3){if(facts.present==0){return WhileStep(c,WhileNeed::Empty,0,0);}c.phase=4;return WhileStep(c,WhileNeed::Inspect,0,0);}
 if(c.phase==4){if(facts.truth==1){return WhileStep(c,WhileNeed::Impossible,0,0);}c.phase=5;return WhileStep(c,WhileNeed::Refine,0,0);}
 if(c.phase==5){if(facts.initial==0){c.phase=6;return WhileStep(c,WhileNeed::Bounds,0,0);}c.phase=10;return WhileStep(c,WhileNeed::SaveBody,0,0);}
 if(c.phase==6){c.phase=7;return WhileStep(c,WhileNeed::NextBound,0,0);}
 if(c.phase==7){if(facts.present!=0){c.phase=6;return WhileStep(c,WhileNeed::Bound,0,0);}c.phase=8;return WhileStep(c,WhileNeed::Retain,0,0);}
 if(c.phase==8){if(facts.possible==0){return WhileStep(c,WhileNeed::Impossible,0,0);}c.phase=10;return WhileStep(c,WhileNeed::SaveBody,0,0);}
 if(c.phase==10){c.phase=11;if(facts.truth!=2){return WhileStep(c,WhileNeed::Natural,0,0);}}
 if(c.phase==11){c.phase=12;return WhileStep(c,WhileNeed::RestoreBody,0,0);}
 if(c.phase==12){c.phase=13;return WhileStep(c,WhileNeed::EnterBody,facts.outer!=0&&facts.condition!=0 ? 1 : 0,0);}
 if(c.phase==13){c.phase=14;return WhileStep(c,WhileNeed::Body,0,0);}
 if(c.phase==14){c.phase=15;if(facts.continuing!=0){return WhileStep(c,WhileNeed::CheckBackedge,0,0);}}
 if(c.phase==15){c.phase=16;return WhileStep(c,WhileNeed::Split,0,0);}
 if(c.phase==16){c.phase=17;if(facts.initial!=0){return WhileStep(c,WhileNeed::Expand,0,0);}}
 if(c.phase==17){c.phase=18;return WhileStep(c,WhileNeed::Leave,0,0);}
 return WhileStep(c,WhileNeed::Emit,0,(facts.flow&2)|(facts.next!=0 ? 1 : 0));
}

extern "C" ForStep for_step(ForCursor cursor,ForFacts facts) on cpu {
 ForCursor c=cursor;
 if(c.phase==0){if(facts.minimum>=facts.maximum||facts.same!=0){c.phase=1;return ForStep(c,ForNeed::EmptyBegin,0,0);}c.phase=4;return ForStep(c,ForNeed::Seeks,0,0);}
 if(c.phase==1){c.phase=2;return ForStep(c,ForNeed::EmptyNext,0,0);}
 if(c.phase==2){if(facts.present==0){return ForStep(c,ForNeed::EmptyDone,0,0);}c.phase=1;return ForStep(c,ForNeed::EmptyInitialize,0,0);}
 if(c.phase==4){c.initial=facts.initial!=0||(facts.seeks==0&&facts.minimum+1==facts.maximum) ? 1 : 0;c.phase=5;if(facts.seeks!=0){return ForStep(c,ForNeed::ClearInitialization,0,0);}}
 if(c.phase==5){c.phase=6;return ForStep(c,ForNeed::Enter,0,0);}
 if(c.phase==6){c.phase=7;return ForStep(c,ForNeed::Cursor,facts.begin_uniform!=0&&facts.limit_uniform!=0 ? 1 : 0,0);}
 if(c.phase==7){c.phase=8;return ForStep(c,ForNeed::Test,0,0);}
 if(c.phase==8){c.phase=9;return ForStep(c,ForNeed::BodyCursor,0,0);}
 if(c.phase==9){c.phase=10;if(facts.seeks==0){return ForStep(c,ForNeed::RefineStart,0,0);}}
 if(c.phase==10){
  if(facts.domain==1){c.phase=11;return ForStep(c,ForNeed::Application,0,0);}
  if(facts.domain==2){c.phase=16;return ForStep(c,ForNeed::DatasetBinding,0,0);}
  if(facts.domain==3){c.phase=13;return ForStep(c,ForNeed::TileBinding,0,0);}
  if(facts.domain==4){c.phase=13;return ForStep(c,ForNeed::ViewBinding,0,0);}
  c.phase=16;return ForStep(c,ForNeed::IndexBinding,0,0);
 }
 if(c.phase==11){c.phase=12;if(c.initial==0&&facts.seeks==0){return ForStep(c,ForNeed::ApplicationInput,0,0);}}
 if(c.phase==12){c.phase=16;return ForStep(c,ForNeed::ApplicationBinding,0,0);}
 if(c.phase==13){c.phase=14;return ForStep(c,ForNeed::NextComponent,0,0);}
 if(c.phase==14){if(facts.present!=0){c.phase=13;return ForStep(c,ForNeed::Component,0,0);}c.phase=16;return ForStep(c,facts.domain==4 ? ForNeed::FinishView : ForNeed::FinishOwned,0,0);}
 if(c.phase==16){c.phase=17;return ForStep(c,ForNeed::Install,0,0);}
 if(c.phase==17){c.phase=18;return ForStep(c,ForNeed::Writes,0,0);}
 if(c.phase==18){c.phase=19;return ForStep(c,ForNeed::Capture,0,0);}
 if(c.phase==19){c.phase=20;return ForStep(c,ForNeed::Natural,0,0);}
 if(c.phase==20){c.phase=21;if(c.initial==0){return ForStep(c,ForNeed::Retain,0,0);}}
 if(c.phase==21){c.phase=22;return ForStep(c,ForNeed::EnterBody,facts.outer!=0&&facts.begin_uniform!=0&&facts.limit_uniform!=0 ? 1 : 0,0);}
 if(c.phase==22){c.phase=23;return ForStep(c,ForNeed::Body,0,0);}
 if(c.phase==23){c.phase=24;if((facts.flow&1)!=0){return ForStep(c,ForNeed::Advance,0,0);}}
 if(c.phase==24){c.phase=25;if((facts.flow&1)!=0){return ForStep(c,ForNeed::CheckBackedge,0,0);}}
 if(c.phase==25){c.phase=26;return ForStep(c,ForNeed::EndBody,0,0);}
 if(c.phase==26){c.phase=27;if(c.initial==0&&facts.seeks==0&&(facts.domain==0||facts.domain==1)){return ForStep(c,ForNeed::IterationElements,0,0);}}
 if(c.phase==27){c.phase=28;return ForStep(c,ForNeed::ExitInitialization,0,0);}
 if(c.phase==28){c.phase=29;if((facts.flow&9)==0){return ForStep(c,ForNeed::TestEntry,0,0);}}
 if(c.phase==29){c.phase=30;if(facts.nonempty!=0&&(c.initial!=0||(facts.flow&9)==0)){return ForStep(c,ForNeed::DropNatural,0,0);}}
 if(c.phase==30){c.phase=31;if(c.initial!=0){return ForStep(c,ForNeed::Expand,0,0);}}
 if(c.phase==31){c.phase=32;return ForStep(c,ForNeed::Leave,0,0);}
 return ForStep(c,ForNeed::Emit,0,(facts.flow&2)|(facts.next!=0 ? 1 : 0));
}

extern "C" ForInputStep for_input_step(ForInputCursor cursor,ForInputFacts facts) on cpu {
 ForInputCursor c=cursor;
 if(c.phase==0){if(facts.collision!=0){return ForInputStep(c,ForInputNeed::Collision);}if(facts.range!=0){c.phase=1;return ForInputStep(c,ForInputNeed::RangeBegin);}c.phase=5;return ForInputStep(c,ForInputNeed::ApplicationReference);}
 if(c.phase==1){if(facts.present==0){return ForInputStep(c,ForInputNeed::Complete);}c.phase=2;return ForInputStep(c,ForInputNeed::RangeEnd);}
 if(c.phase==2){if(facts.present==0){return ForInputStep(c,ForInputNeed::Complete);}c.phase=3;return ForInputStep(c,ForInputNeed::BeginType);}
 if(c.phase==3){c.phase=4;return ForInputStep(c,ForInputNeed::EndType);}
 if(c.phase==4){c.phase=21;return ForInputStep(c,ForInputNeed::Bounds);}
 if(c.phase==5){if(facts.present!=0){c.phase=8;}else{c.phase=6;return ForInputStep(c,ForInputNeed::SourceKind);}}
 if(c.phase==6){c.phase=8;return ForInputStep(c,facts.name!=0 ? ForInputNeed::NameBinding : ForInputNeed::DatasetReference);}
 if(c.phase==8){c.phase=9;return ForInputStep(c,ForInputNeed::Zero);}
 if(c.phase==9){c.phase=10;return ForInputStep(c,ForInputNeed::Classify);}
 if(c.phase==10){
  c.phase=4;
  if(facts.kind==1){return ForInputStep(c,ForInputNeed::Application);}
  if(facts.kind==2){return ForInputStep(c,facts.access==1||facts.access==3||facts.access==4 ? ForInputNeed::Dataset : ForInputNeed::InvalidAccess);}
  if(facts.kind==3){return ForInputStep(c,ForInputNeed::Tile);}
  c.phase=11;return ForInputStep(c,ForInputNeed::ViewExpression);
 }
 if(c.phase==11){if(facts.present==0){return ForInputStep(c,ForInputNeed::Complete);}if(facts.view==0){return ForInputStep(c,ForInputNeed::InvalidSource);}c.phase=4;return ForInputStep(c,ForInputNeed::View);}
 if(c.phase==21){if(facts.begin_min>=facts.limit_max){return ForInputStep(c,ForInputNeed::Complete);}c.nonempty=facts.begin_max<facts.limit_min ? 1 : 0;if(facts.kind==1){c.phase=22;return ForInputStep(c,ForInputNeed::Borrowed);}c.phase=23;return ForInputStep(c,ForInputNeed::Initial);}
 if(c.phase==22){c.phase=23;return ForInputStep(c,ForInputNeed::Initial);}
 if(c.phase==23){c.phase=24;return ForInputStep(c,ForInputNeed::Slot);}
 if(c.phase==24){c.phase=25;return ForInputStep(c,ForInputNeed::Initialization);}
 if(c.phase==25){if(facts.kind==1){c.phase=26;return ForInputStep(c,ForInputNeed::BindApplication);}c.phase=27;return ForInputStep(c,ForInputNeed::Loop);}
 if(c.phase==26){c.phase=27;return ForInputStep(c,ForInputNeed::Loop);}
 if(c.phase==27){c.phase=28;if(facts.continuing!=0){return ForInputStep(c,ForInputNeed::CompleteWrites);}}
 return ForInputStep(c,ForInputNeed::Complete);
}
extern "C" u32 for_written(u32 unknown,u64 count,ForWrittenHost host) on cpu
 requires nonzero(host.storage) requires nonzero(host.locals) requires nonzero(host.local) {
 if(unknown!=0){return 1;}u64 index=0u64;
 while(index<count){if(host.storage(host.context,index)!=0){return 1;}index+=1u64;}
 u64 locals=host.locals(host.context);index=0u64;
 while(index<locals){if(host.local(host.context,index)!=0){return 1;}index+=1u64;}return 0;
}

bool local_spelling<const N:u32>(address bytes,u64 length,read array<u8,N> expected) on cpu {
 if(length!=u64(N)){return false;}for(i in 0..N){if(unsafe_position<u8>(unsafe_address(address_bits(bytes)+u64(i)))!=expected[i]){return false;}}return true;
}
extern "C" u32 local_publish(address bytes,u64 length) on cpu {if(local_spelling(bytes,length,"tile_publish")){return 1;}if(local_spelling(bytes,length,"tile_atomic")){return 2;}return 0;}
extern "C" LetStep let_step(LocalCursor cursor,LetFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.collision!=0){return LetStep(c,LetNeed::Collision,0);}c.phase=1;return LetStep(c,LetNeed::Inspect,0);}
 if(c.phase==1){if(facts.publish!=0){return LetStep(c,LetNeed::Publish,facts.publish==2 ? 1 : 0);}c.phase=2;if(facts.annotation!=0){return LetStep(c,LetNeed::Expected,0);}}
 if(c.phase==2){c.phase=4;if(facts.kind==2){return LetStep(c,LetNeed::Cooperate,0);}if(facts.kind==1){c.phase=3;return LetStep(c,LetNeed::Collective,0);}return LetStep(c,LetNeed::Evaluate,0);}
 if(c.phase==3){c.phase=4;return LetStep(c,facts.tile!=0 ? LetNeed::Tile : LetNeed::Evaluate,0);}
 if(c.phase==4){if(facts.continuing==0){return LetStep(c,LetNeed::Complete,0);}if(facts.present==0){return LetStep(c,LetNeed::Missing,0);}c.phase=5;if(facts.annotation!=0){return LetStep(c,LetNeed::CheckType,0);}}
 return LetStep(c,LetNeed::Bind,0);
}
extern "C" DeclareStep declare_step(LocalCursor cursor,DeclareFacts facts) on cpu {
 LocalCursor c=cursor;if(c.phase==0){if(facts.collision!=0){return DeclareStep(c,DeclareNeed::Collision);}if(facts.dataset!=0){return DeclareStep(c,DeclareNeed::Dataset);}c.phase=1;return DeclareStep(c,DeclareNeed::Type);}return DeclareStep(c,facts.owns!=0 ? DeclareNeed::Owned : DeclareNeed::Plain);
}
extern "C" ValueStep value_step(ValueCursor cursor,ValueFacts facts) on cpu {
 ValueCursor c=cursor;u32 uniform=facts.uniform!=0&&facts.control!=0 ? 1 : 0;
 if(c.phase==0){c.phase=1;return ValueStep(c,ValueNeed::View,0,0u64);}
 if(c.phase==1){if(facts.collision!=0){return ValueStep(c,ValueNeed::Collision,0,0u64);}c.phase=2;return ValueStep(c,ValueNeed::Mutability,0,0u64);}
 if(c.phase==2){if(facts.mutable!=0){c.phase=6;return ValueStep(c,ValueNeed::Slots,0,0u64);}c.phase=3;return ValueStep(c,ValueNeed::Arrays,0,0u64);}
 if(c.phase==3){if(facts.arrays==0){return ValueStep(c,ValueNeed::Immutable,uniform,0u64);}c.phase=4;return ValueStep(c,ValueNeed::Array,0,0u64);}
 if(c.phase==4){return ValueStep(c,ValueNeed::ArrayFinish,uniform,0u64);}
 if(c.index<facts.slots&&c.index<facts.regs){u64 at=c.index;c.index+=1u64;return ValueStep(c,ValueNeed::Store,0,at);}return ValueStep(c,ValueNeed::Finish,uniform,0u64);
}
extern "C" OwnedStep owned_step(LocalCursor cursor,OwnedFacts facts) on cpu {
 LocalCursor c=cursor;if(c.phase==0){if(facts.collision!=0){return OwnedStep(c,OwnedNeed::Collision,0);}c.phase=1;return OwnedStep(c,OwnedNeed::Rebound,0);}if(c.phase==1){if(facts.rebound!=0){return OwnedStep(c,OwnedNeed::Bind,1);}c.phase=2;return OwnedStep(c,OwnedNeed::Variants,0);}return OwnedStep(c,OwnedNeed::Bind,facts.variants==0 ? 1 : 0);
}
extern "C" ReturnedStep returned_step(LocalCursor cursor,ReturnedFacts facts) on cpu {
 LocalCursor c=cursor;if(c.phase==0){if(facts.kind==0){return ReturnedStep(c,ReturnedNeed::Value,0);}if(facts.kind==1){return ReturnedStep(c,ReturnedNeed::Owned,0);}if(facts.kind==3){return ReturnedStep(c,ReturnedNeed::Unit,0);}c.phase=1;return ReturnedStep(c,ReturnedNeed::Name,0);}if(c.phase==1){if(facts.collision!=0){return ReturnedStep(c,ReturnedNeed::Collision,0);}c.phase=2;return ReturnedStep(c,ReturnedNeed::Rebound,0);}return ReturnedStep(c,ReturnedNeed::Dataset,facts.rebound!=0 ? 1 : 0);
}
address binding_at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
T binding_node<T>(address nodes,u64 offset) on cpu {return unsafe_position<T>(binding_at(nodes,offset));}
u64 binding_member(address nodes,Slice list,u64 index) on cpu {return binding_node<u64>(nodes,list.offset+index*8u64);}
bool binding_name(address source,address stored,Text text,address name,u64 length) on cpu {
 if(text.range.end-text.range.start!=length){return false;}address bytes=binding_at(text.kind==0 ? source : stored,text.range.start);u64 i=0u64;
 while(i<length){if(unsafe_position<u8>(binding_at(bytes,i))!=unsafe_position<u8>(binding_at(name,i))){return false;}i+=1u64;}return true;
}
bool binding_root(address nodes,address source,address stored,ExprRef reference,address name,u64 length) on cpu {
 ExprNode expression=binding_node<ExprNode>(nodes,reference.offset);
 match(expression.kind){ExprKind::Name(value){return binding_name(source,stored,value,name,length);}ExprKind::Member(value){return binding_root(nodes,source,stored,value.base,name,length);}ExprKind::Number(unused){return false;}ExprKind::StaticU32(unused){return false;}ExprKind::Bool(unused){return false;}ExprKind::Index(unused){return false;}ExprKind::Call(unused){return false;}ExprKind::Invoke(unused){return false;}ExprKind::Construct(unused){return false;}ExprKind::Specialize(unused){return false;}ExprKind::FunctionRef(unused){return false;}ExprKind::Cooperate(unused){return false;}ExprKind::Target(unused){return false;}ExprKind::Unary(unused){return false;}ExprKind::Binary(unused){return false;}ExprKind::Select(unused){return false;}}
}
bool rebound_body(address nodes,address source,address stored,Slice body,address name,u64 length) on cpu {
 u64 i=0u64;while(i<body.count){StmtNode statement=binding_node<StmtNode>(nodes,binding_member(nodes,body,i));
  match(statement.kind){
   StmtKind::Assign(value){if(binding_root(nodes,source,stored,value.target,name,length)){return true;}}
   StmtKind::If(value){if(rebound_body(nodes,source,stored,value.yes,name,length)||rebound_body(nodes,source,stored,value.no,name,length)){return true;}}
   StmtKind::Match(value){u64 arm=0u64;while(arm<value.arms.count){ArmNode item=binding_node<ArmNode>(nodes,binding_member(nodes,value.arms,arm));if(rebound_body(nodes,source,stored,item.body,name,length)){return true;}arm+=1u64;}}
   StmtKind::For(value){if(rebound_body(nodes,source,stored,value.body,name,length)){return true;}}
   StmtKind::While(value){if(rebound_body(nodes,source,stored,value.body,name,length)){return true;}}
   StmtKind::Declare(unused){}   StmtKind::Let(unused){}   StmtKind::Expr(unused){}   StmtKind::Parallel(unused){}   StmtKind::Return(unused){}   StmtKind::Break(){}   StmtKind::Continue(){}   StmtKind::Phase(){}   StmtKind::Seek(unused){}
  }i+=1u64;
 }return false;
}
extern "C" u32 binding_rebound(address nodes,address source,address stored,Slice body,address name,u64 length) on cpu {return rebound_body(nodes,source,stored,body,name,length) ? 1 : 0;}

extern "C" AssignmentStep assignment_step(AssignmentCursor cursor,AssignmentFacts facts) on cpu {
 AssignmentCursor c=cursor;
 if(c.phase==0){c.compound=facts.operation.length!=1u64||unsafe_position<u8>(facts.operation.data)!=61 ? 1 : 0;c.phase=1;return AssignmentStep(c,AssignmentNeed::Type);}
 if(c.phase==1){if(facts.owns!=0){c.phase=2;return AssignmentStep(c,AssignmentNeed::Projection);}c.phase=10;return AssignmentStep(c,AssignmentNeed::Name);}
 if(c.phase==2){c.phase=3;return AssignmentStep(c,AssignmentNeed::Inspect);}
 if(c.phase==3){if(facts.kind==1){c.phase=4;return AssignmentStep(c,AssignmentNeed::BaseType);}if(facts.kind==2){c.phase=2;return AssignmentStep(c,AssignmentNeed::Project);}c.phase=10;return AssignmentStep(c,AssignmentNeed::Name);}
 if(c.phase==4){if(facts.dataset!=0){c.phase=5;return AssignmentStep(c,AssignmentNeed::ElementType);}c.phase=2;return AssignmentStep(c,AssignmentNeed::Project);}
 if(c.phase==5){if(facts.owns!=0){c.phase=6;return AssignmentStep(c,AssignmentNeed::Reference);}c.phase=2;return AssignmentStep(c,AssignmentNeed::Project);}
 if(c.phase==6){if(facts.present==0){return AssignmentStep(c,AssignmentNeed::Complete);}c.phase=7;return AssignmentStep(c,AssignmentNeed::Reverse);}
 if(c.phase==7){return AssignmentStep(c,AssignmentNeed::Element);}
 if(c.phase==10){if(facts.name!=0){c.phase=11;return AssignmentStep(c,AssignmentNeed::NamedData);}c.phase=12;return AssignmentStep(c,AssignmentNeed::Place);}
 if(c.phase==11){if(facts.present!=0){return AssignmentStep(c,AssignmentNeed::Application);}c.phase=12;return AssignmentStep(c,AssignmentNeed::Place);}
 if(c.phase==12){if(facts.present==0){return AssignmentStep(c,AssignmentNeed::Complete);}c.phase=13;return AssignmentStep(c,AssignmentNeed::PlaceType);}
 if(c.phase==13){if(facts.owns!=0){return AssignmentStep(c,AssignmentNeed::Owned);}c.phase=14;return AssignmentStep(c,AssignmentNeed::BorrowKind);}
 if(c.phase==14){if(facts.borrowed!=0){c.phase=15;return AssignmentStep(c,AssignmentNeed::Hold);}c.phase=16;return AssignmentStep(c,AssignmentNeed::Right);}
 if(c.phase==15){c.phase=16;return AssignmentStep(c,AssignmentNeed::Right);}
 if(c.phase==16){if(facts.present==0){return AssignmentStep(c,AssignmentNeed::Complete);}c.phase=17;if(facts.borrowed!=0){return AssignmentStep(c,AssignmentNeed::Release);}}
 if(c.phase==17){if(c.compound!=0){c.phase=18;return AssignmentStep(c,AssignmentNeed::Read);}return AssignmentStep(c,AssignmentNeed::Write);}
 if(c.phase==18){c.phase=19;return AssignmentStep(c,AssignmentNeed::Combine);}
 return AssignmentStep(c,AssignmentNeed::Write);
}
extern "C" void reverse_projection(u64 count,address context,cfn<void,address,u64,u64> exchange) on cpu requires nonzero(exchange) {
 u64 index=0u64;while(index<count/2u64){exchange(context,index,count-1u64-index);index+=1u64;}
}
extern "C" u32 statement_barrier(address bytes,u64 length) on cpu {return local_spelling(bytes,length,"tile_barrier") ? 1 : 0;}
extern "C" ExpressionStep expression_step(LocalCursor cursor,ExpressionFacts facts) on cpu {
 LocalCursor c=cursor;if(c.phase==0){c.phase=1;return ExpressionStep(c,ExpressionNeed::Inspect);}
 if(c.phase==1){if(facts.kind==1){c.phase=2;return ExpressionStep(c,ExpressionNeed::AtomicQuery);}if(facts.kind==2){return ExpressionStep(c,ExpressionNeed::Cooperate);}c.phase=4;return ExpressionStep(c,ExpressionNeed::Evaluate);}
 if(c.phase==2){if(facts.atomic!=0){return ExpressionStep(c,ExpressionNeed::Atomic);}c.phase=3;return ExpressionStep(c,ExpressionNeed::BarrierQuery);}
 if(c.phase==3){if(facts.barrier!=0){return ExpressionStep(c,ExpressionNeed::Barrier);}c.phase=4;return ExpressionStep(c,ExpressionNeed::Evaluate);}
 return ExpressionStep(c,facts.present!=0 ? ExpressionNeed::Drop : ExpressionNeed::Complete);
}

extern "C" JumpStep jump_step(LocalCursor cursor,JumpFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.local==0){return JumpStep(c,JumpNeed::Outside);}c.phase=1;return JumpStep(c,JumpNeed::Release);}
 if(c.phase==1){if(facts.breaking!=0){return JumpStep(c,JumpNeed::Break);}c.phase=2;return JumpStep(c,JumpNeed::Iterator);}
 if(c.phase==2){c.phase=3;if(facts.iterator!=0){return JumpStep(c,JumpNeed::Advance);}}
 if(c.phase==3){c.phase=4;return JumpStep(c,JumpNeed::Backedge);}
 return JumpStep(c,JumpNeed::Continue);
}
bool seek_same_name(SeekName requested,SeekName current) on cpu {
 if(requested.length!=current.length){return false;}u64 index=0u64;
 while(index<requested.length){if(unsafe_position<u8>(unsafe_address(address_bits(requested.data)+index))!=unsafe_position<u8>(unsafe_address(address_bits(current.data)+index))){return false;}index+=1u64;}return true;
}
extern "C" SeekStep seek_step(LocalCursor cursor,SeekFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return SeekStep(c,SeekNeed::Iterator);}
 if(c.phase==1){if(facts.iterator==0){return SeekStep(c,SeekNeed::Missing);}if(!seek_same_name(facts.requested,facts.current)){return SeekStep(c,SeekNeed::WrongName);}c.phase=2;return SeekStep(c,SeekNeed::Position);}
 if(c.phase==2){if(facts.present==0){return SeekStep(c,SeekNeed::Complete);}c.phase=3;return SeekStep(c,SeekNeed::Type);}
 if(c.phase==3){c.phase=4;return SeekStep(c,SeekNeed::Release);}
 if(c.phase==4){c.phase=5;return SeekStep(c,SeekNeed::Compare);}
 if(c.phase==5){if(facts.outside!=0){return SeekStep(c,SeekNeed::Break);}c.phase=6;return SeekStep(c,SeekNeed::Store);}
 if(c.phase==6){c.phase=7;return SeekStep(c,SeekNeed::Backedge);}
 return SeekStep(c,SeekNeed::Continue);
}

extern "C" MatchStep match_step(LocalCursor cursor,MatchFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return MatchStep(c,MatchNeed::Type);}
 if(c.phase==1){if(facts.owns!=0){c.phase=2;return MatchStep(c,MatchNeed::Root);}c.phase=5;return MatchStep(c,MatchNeed::Evaluate);}
 if(c.phase==2){c.phase=3;return MatchStep(c,MatchNeed::RootKind);}
 if(c.phase==3){if(facts.root==1||facts.root==2){c.phase=2;return MatchStep(c,MatchNeed::Parent);}if(facts.root==3){c.phase=4;return MatchStep(c,MatchNeed::Borrowed);}c.phase=5;return MatchStep(c,MatchNeed::Evaluate);}
 if(c.phase==4){c.phase=5;return MatchStep(c,facts.borrowed!=0 ? MatchNeed::Read : MatchNeed::Evaluate);}
 if(c.phase==5){if(facts.continuing==0){return MatchStep(c,MatchNeed::Complete);}if(facts.present==0){return MatchStep(c,MatchNeed::Missing);}if(facts.kind!=1&&facts.kind!=3){return MatchStep(c,MatchNeed::InvalidType);}c.phase=6;return MatchStep(c,MatchNeed::Patterns);}
 if(c.phase==6){c.phase=7;return MatchStep(c,MatchNeed::Arms);}
 return MatchStep(c,MatchNeed::Complete);
}
extern "C" MatchBranch match_branch(u64 index,u64 count,u32 kind) on cpu {if(index+1u64==count){return MatchBranch::Last;}return kind==3 ? MatchBranch::Owned : MatchBranch::Value;}
extern "C" u32 match_binding(u32 present,address data,u64 length) on cpu {if(present==0){return 0;}return length==1u64&&unsafe_position<u8>(data)==95 ? 0 : 1;}
extern "C" MatchArmStep match_arm_step(LocalCursor cursor,MatchArmFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return MatchArmStep(c,facts.owned!=0 ? MatchArmNeed::Unpack : MatchArmNeed::Payload);}
 if(c.phase==1){c.phase=2;return MatchArmStep(c,MatchArmNeed::Name);}
 if(c.phase==2){if(facts.named!=0){c.phase=3;return MatchArmStep(c,facts.owned!=0 ? MatchArmNeed::BindReturned : MatchArmNeed::BindValue);}if(facts.owned!=0){c.phase=7;return MatchArmStep(c,MatchArmNeed::DropValue);}c.phase=4;return MatchArmStep(c,MatchArmNeed::Body);}
 if(c.phase==3||c.phase==7){c.phase=4;return MatchArmStep(c,MatchArmNeed::Body);}
 if(c.phase==4){if(facts.named==0){return MatchArmStep(c,MatchArmNeed::Complete);}if(facts.owned!=0&&facts.continuing!=0){c.phase=5;return MatchArmStep(c,MatchArmNeed::Lookup);}return MatchArmStep(c,MatchArmNeed::Remove);}
 if(c.phase==5){c.phase=6;if(facts.present!=0){return MatchArmStep(c,MatchArmNeed::DropBinding);}}
 return MatchArmStep(c,MatchArmNeed::Remove);
}
extern "C" PatternLabel pattern_label(SeekName spelling) on cpu {
 u64 index=spelling.length;while(index>1u64){
  if(unsafe_position<u8>(unsafe_address(address_bits(spelling.data)+index-2u64))==58&&unsafe_position<u8>(unsafe_address(address_bits(spelling.data)+index-1u64))==58){return PatternLabel(1,index-2u64,index);}index-=1u64;
 }return PatternLabel(0,0u64,0u64);
}
extern "C" u64 pattern_find(SeekName name,u64 count,address context,cfn<SeekName,address,u64> variant) on cpu requires nonzero(variant) {
 u64 index=0u64;while(index<count){if(seek_same_name(name,variant(context,index))){return index;}index+=1u64;}return count;
}
extern "C" PatternStep pattern_step(ValueCursor cursor,PatternFacts facts) on cpu {
 ValueCursor c=cursor;
 if(c.phase==0){if(facts.eligible==0){return PatternStep(c,PatternNeed::InvalidType);}c.phase=1;}
 while(true){
  if(c.phase==1){if(c.index>=facts.arms){c.index=0u64;c.phase=7;}else{c.phase=2;return PatternStep(c,PatternNeed::Arm);}}
  if(c.phase==2){if(facts.owner!=0){c.phase=3;return PatternStep(c,PatternNeed::Owner);}c.phase=4;return PatternStep(c,PatternNeed::Find);}
  if(c.phase==3){c.phase=4;return PatternStep(c,PatternNeed::Find);}
  if(c.phase==4){if(facts.found==0){return PatternStep(c,PatternNeed::Unknown);}c.phase=5;return PatternStep(c,PatternNeed::Select);}
  if(c.phase==5){if(facts.duplicate!=0){return PatternStep(c,PatternNeed::Duplicate);}c.phase=6;return PatternStep(c,PatternNeed::Inspect);}
  if(c.phase==6){if(facts.binding==0&&facts.unit==0){return PatternStep(c,PatternNeed::MissingBinding);}if(facts.unit!=0&&facts.named!=0){return PatternStep(c,PatternNeed::UnitBinding);}c.index+=1u64;c.phase=1;continue;}
  if(c.phase==7){if(c.index>=facts.variants){return PatternStep(c,PatternNeed::Complete);}c.phase=8;return PatternStep(c,PatternNeed::Coverage);}
  if(facts.present==0){return PatternStep(c,PatternNeed::Incomplete);}c.index+=1u64;c.phase=7;
 }
}
