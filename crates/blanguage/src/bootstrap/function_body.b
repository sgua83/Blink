module bootstrap.function_body;

extern "C" void loop_terminal_paths(address context,usize edges,LoopTerminalHost host) on cpu
    requires nonzero(host.bounded) requires nonzero(host.project) {
    usize edge=0u64;
    while(edge<edges) {
        host.project(context,edge,host.bounded(context,edge));
        edge+=1u64;
    }
}

extern "C" void loop_natural_dataset(address context,usize edges,u32 written,u32 terminal,NaturalDatasetHost host) on cpu
    requires nonzero(host.terminal) requires nonzero(host.repeated)
    requires nonzero(host.publish_terminal) requires nonzero(host.publish_repeated) {
    if(written!=0||terminal!=0) {
        usize edge=0u64;
        while(edge<edges) {
            if(terminal!=0){host.terminal(context,edge);}
            if(written!=0){host.repeated(context,edge);}
            edge+=1u64;
        }
        if(terminal!=0){host.publish_terminal(context);}
        if(written!=0&&edges!=0u64){host.publish_repeated(context,terminal);}
    }
}

extern "C" void loop_natural_scalar(address context,usize components,usize edges,NaturalScalarHost host) on cpu
    requires nonzero(host.header_identity) requires nonzero(host.header_write)
    requires nonzero(host.repeated) requires nonzero(host.publish) {
    usize component=0u64;
    while(component<components) {
        if(host.header_identity(context,component)==0&&host.header_write(context,component)==0) {
            u32 repeated=0;
            usize edge=0u64;
            while(edge<edges) {
                repeated|=host.repeated(context,component,edge);
                edge+=1u64;
            }
            if(repeated!=0){host.publish(context,component);}
        }
        component+=1u64;
    }
}

extern "C" u32 loop_exit_join(address context,usize exits,u32 natural,u32 terminal,LoopExitHost host) on cpu
    requires nonzero(host.terminal_reachable) requires nonzero(host.guards) requires nonzero(host.initialize)
    requires nonzero(host.adopt) requires nonzero(host.merge) requires nonzero(host.publish) {
    if(exits>1u64) {
        LoopExitGuard source=LoopExitGuard::Body;
        if(exits==2u64&&natural!=0&&terminal!=0&&host.terminal_reachable(context)==0) {
            source=LoopExitGuard::Terminal;
        }
        usize guard=host.guards(context,source,natural);
        host.initialize(context);
        host.adopt(context,0);
        usize remaining=exits-1u64;
        // The last exit supplies the seed. Earlier edges use their own guards.
        guard-=1u64;
        while(remaining!=0u64) {
            guard-=1u64;
            host.merge(context,guard);
            remaining-=1u64;
        }
    }else if(exits==1u64) {
        host.adopt(context,1);
    }
    u32 reachable=u32(exits!=0u64);
    host.publish(context,reachable);
    return reachable;
}

extern "C" CheckedDispatch checked_dispatch(SignatureNode declaration) on cpu {
    FunctionNode function = binding_node<FunctionNode>(declaration.storage.nodes,declaration.offset);
    if (function.kind == 2) { return CheckedDispatch::Constant; }
    if (function.kind == 0 && function.target == 1) { return CheckedDispatch::Native; }
    return CheckedDispatch::Body;
}

extern "C" CheckedProgramStep checked_program_step(u32 phase) on cpu {
    if (phase == 0) { return CheckedProgramStep(1,CheckedProgramNeed::Program); }
    if (phase == 1) { return CheckedProgramStep(2,CheckedProgramNeed::Prefixes); }
    return CheckedProgramStep(phase,CheckedProgramNeed::Builder);
}

extern "C" void checked_prefixes(SignatureNode declaration,address context,
    cfn<u32,address,PrefixPredicate,Span> bind) on cpu requires nonzero(bind) {
    address nodes = declaration.storage.nodes;
    Slice contracts = binding_node<FunctionNode>(nodes,declaration.offset).contracts;
    u64 index = 0u64;
    while (index < contracts.count) {
        ContractNode contract = binding_node<ContractNode>(nodes,binding_member(nodes,contracts,index));
        index += 1u64;
        if (contract.kind == 0) {
            match (binding_node<PredicateNode>(nodes,contract.predicate.offset)) {
                PredicateNode::Prefix(prefix) {
                    if (bind(context,prefix,contract.span) == 0) { return; }
                }
                PredicateNode::Nonzero(unused) {}
                PredicateNode::Nonempty(unused) {}
                PredicateNode::Index(unused) {}
                PredicateNode::Contiguous(unused) {}
                PredicateNode::Initialized(unused) {}
                PredicateNode::SameLength(unused) {}
                PredicateNode::Cleanup(unused) {}
                PredicateNode::Memory(unused) {}
                PredicateNode::Range(unused) {}
            }
        }
    }
}

bool checked_void_name(address name,u64 length) on cpu {
    return length == 4u64
        && u32(binding_node<u8>(name,0u64)) == 118 && u32(binding_node<u8>(name,1u64)) == 111
        && u32(binding_node<u8>(name,2u64)) == 105 && u32(binding_node<u8>(name,3u64)) == 100;
}

extern "C" CheckedBodyStep checked_body_step(u32 phase,CheckedBodyFacts facts) on cpu {
    if (phase == 0) { return CheckedBodyStep(1,CheckedBodyNeed::Context); }
    if (phase == 1 && facts.application != 0) { return CheckedBodyStep(2,CheckedBodyNeed::Views); }
    if (phase == 1 || phase == 2) { return CheckedBodyStep(3,CheckedBodyNeed::Parameters); }
    if (phase == 3) { return CheckedBodyStep(4,CheckedBodyNeed::Result); }
    if (phase == 4 && !checked_void_name(facts.result_name,facts.result_length) && facts.never == 0) {
        return CheckedBodyStep(5,CheckedBodyNeed::Layout);
    }
    if (phase == 4 || phase == 5) { return CheckedBodyStep(6,CheckedBodyNeed::ReturnInputs); }
    if (phase == 6) { return CheckedBodyStep(7,CheckedBodyNeed::Body); }
    return CheckedBodyStep(phase,CheckedBodyNeed::Finish);
}

extern "C" CheckedFinishStep checked_finish_step(u32 phase,usize index,u32 returned,
    usize written,u32 present) on cpu {
    if (phase == 0 && returned != 0) {
        return CheckedFinishStep(1,CheckedFinishNeed::NormalizeReturn,index);
    }
    if (phase == 2 && present != 0) {
        return CheckedFinishStep(3,CheckedFinishNeed::NormalizeWritten,index);
    }
    if (phase < 4) {
        usize next = phase >= 2 ? index+1u64 : index;
        if (next < written) { return CheckedFinishStep(2,CheckedFinishNeed::InspectWritten,next); }
        if (returned != 0) { return CheckedFinishStep(4,CheckedFinishNeed::Outputs,next); }
    }
    if (phase <= 4) { return CheckedFinishStep(5,CheckedFinishNeed::Canonical,index); }
    return CheckedFinishStep(phase,CheckedFinishNeed::Publish,index);
}

extern "C" void recursive_ranges(address context,SeekName function,usize models,usize exits,RecursiveHost host) on cpu
 requires nonzero(host.model) requires nonzero(host.remember_argument) requires nonzero(host.calls)
 requires nonzero(host.exit) requires nonzero(host.reg) requires nonzero(host.forwarded)
 requires nonzero(host.domains) requires nonzero(host.apply) {
 usize at=0u64;bool called=false;
 while(at<models){
  RecursiveModelCall call=host.model(context,at);at+=1u64;
  if(seek_same_name(function,call.function)){host.remember_argument(context,call.memory,call.arguments);called=true;}
 }
 if(!called){return;}
 host.calls(context);
 usize components=0u64;bool returned=false;at=0u64;
 while(at<exits){RecursiveValue value=host.exit(context,at);at+=1u64;if(address_bits(value.node)!=0u64){components=value.count;returned=true;break;}}
 if(!returned){return;}
 usize component=0u64;
 while(component<components){
  at=0u64;
  while(at<exits){
   RecursiveValue value=host.exit(context,at);at+=1u64;
   if(address_bits(value.node)!=0u64){
    RecursiveForward forward=host.forwarded(context,host.reg(value.node,component));
    if(address_bits(forward.arguments)==0u64||forward.component!=component){host.domains(context,value.node,component);break;}
   }
  }
  component+=1u64;
 }
 host.apply(context);
}
extern "C" void recursive_call(address context,SeekName function,RecursiveNativeCall call,RecursiveHost host) on cpu
 requires nonzero(host.arguments) requires nonzero(host.remember_forward) {
 if(call.defined==0||!seek_same_name(function,call.function)||call.memory_present==0){return;}
 address arguments=host.arguments(context,call.memory);if(address_bits(arguments)==0u64){return;}
 usize component=0u64;
 while(component<call.count){
  usize reg=unsafe_position<usize>(unsafe_address(address_bits(call.regs)+u64(component)*8u64));
  host.remember_forward(context,reg,component,arguments);component+=1u64;
 }
}
extern "C" void recursive_domain(address context,usize component,RecursiveDomain domain,usize exits,RecursiveHost host) on cpu
 requires nonzero(host.exit) requires nonzero(host.reg) requires nonzero(host.forwarded)
 requires nonzero(host.contains_domain) requires nonzero(host.origin) requires nonzero(host.same_origin) requires nonzero(host.inherit) {
 if(domain.kind!=1||domain.origin_kind!=1){return;}
 usize at=0u64;
 while(at<exits){
  RecursiveValue value=host.exit(context,at);at+=1u64;
  if(address_bits(value.node)!=0u64){
   RecursiveForward forward=host.forwarded(context,host.reg(value.node,component));
   if(address_bits(forward.arguments)!=0u64&&forward.component==component){
    usize index=0u64;bool preserved=false;
    while(true){
     address origin=host.origin(forward.arguments,domain.origin,index);if(address_bits(origin)==0u64){break;}
     if(host.same_origin(origin,domain.origin)!=0){preserved=true;break;}index+=1u64;
    }
    if(!preserved){return;}
   }else if(host.contains_domain(value.node,component,domain.original)==0){return;}
  }
 }
 at=0u64;
 while(at<exits){
  RecursiveValue value=host.exit(context,at);at+=1u64;
  if(address_bits(value.node)!=0u64){
   usize reg=host.reg(value.node,component);RecursiveForward forward=host.forwarded(context,reg);
   if(address_bits(forward.arguments)!=0u64&&forward.component==component){host.inherit(context,reg,domain.original);}
  }
 }
}
extern "C" void recursive_apply(address context,RecursiveValue value,RecursiveHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.inherited) requires nonzero(host.mark_domains) {
 usize component=0u64;
 while(component<value.count){
  address domains=host.inherited(context,host.reg(value.node,component));
  if(address_bits(domains)!=0u64){host.mark_domains(value.node,component,domains);}component+=1u64;
 }
}
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

extern "C" TryStep try_step(LocalCursor cursor,TryFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.arguments!=1u64){return TryStep(c,TryNeed::Arity);}c.phase=1;return TryStep(c,TryNeed::Evaluate);}
 if(c.phase==1){if(facts.continuing==0){return TryStep(c,TryNeed::Complete);}if(facts.present==0){return TryStep(c,TryNeed::BadSource);}c.phase=2;return TryStep(c,TryNeed::Inspect);}
 if(c.phase==2){if((facts.kind!=1&&facts.kind!=3)||facts.result==0){return TryStep(c,TryNeed::BadSource);}c.phase=3;return TryStep(c,TryNeed::Expected);}
 if(c.phase==3){if(facts.expected==0){return TryStep(c,TryNeed::BadFunction);}c.phase=4;return TryStep(c,TryNeed::ErrorType);}
 if(c.phase==4){c.phase=5;return TryStep(c,facts.kind==3 ? TryNeed::OwnedCondition : TryNeed::ValueCondition);}
 if(c.phase==5){c.phase=6;return TryStep(c,TryNeed::Branches);}
 if(facts.continuing==0){return TryStep(c,TryNeed::Complete);}return TryStep(c,facts.kind==3 ? TryNeed::OwnedSuccess : TryNeed::ValueSuccess);
}
extern "C" TryErrorStep try_error_step(LocalCursor cursor,TryErrorFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return TryErrorStep(c,facts.kind==3 ? TryErrorNeed::Owned : TryErrorNeed::Value);}
 if(c.phase==1){c.phase=2;return TryErrorStep(c,TryErrorNeed::Expected);}
 if(c.phase==2){c.phase=3;return TryErrorStep(c,facts.owns==0&&facts.item==1 ? TryErrorNeed::MoveValue : TryErrorNeed::Storage);}
 if(c.phase==3){c.phase=4;return TryErrorStep(c,TryErrorNeed::Build);}
 return TryErrorStep(c,facts.owns!=0 ? TryErrorNeed::OwnedReturn : TryErrorNeed::ValueReturn);
}

extern "C" ReadStep read_step(ReadCursor cursor,ReadFacts facts) on cpu {
 ReadCursor c=cursor;
 if(c.phase==0){c.phase=1;return ReadStep(c,ReadNeed::Inspect);}
 if(c.phase==1){if(facts.source==1){c.phase=2;return ReadStep(c,ReadNeed::Name);}c.phase=5;return ReadStep(c,ReadNeed::ElementType);}
 if(c.phase==2){if(facts.binding!=0){c.phase=3;return ReadStep(c,ReadNeed::NameStorage);}c.phase=5;return ReadStep(c,ReadNeed::ElementType);}
 if(c.phase==3){if(facts.application!=0){c.phase=4;return ReadStep(c,ReadNeed::NameType);}c.phase=5;return ReadStep(c,ReadNeed::ElementType);}
 if(c.phase==4){if(facts.owns!=0&&facts.dataset==0){c.phase=6;c.path=1;return ReadStep(c,ReadNeed::Application);}c.phase=5;return ReadStep(c,ReadNeed::ElementType);}
 if(c.phase==5){if(facts.owns!=0){c.phase=7;return ReadStep(c,ReadNeed::Place);}c.phase=9;return ReadStep(c,ReadNeed::Projection);}
 if(c.phase==6||c.phase==8||c.phase==10){if(facts.kind!=3){return ReadStep(c,ReadNeed::BadOwned);}c.phase=12;return ReadStep(c,ReadNeed::Available);}
 if(c.phase==7){if(facts.present==0){return ReadStep(c,ReadNeed::Complete);}c.phase=8;c.path=2;return ReadStep(c,ReadNeed::Model);}
 if(c.phase==9){if(facts.present!=0){c.phase=10;c.path=3;return ReadStep(c,ReadNeed::Projected);}c.phase=11;}
 if(c.phase==11){if(facts.source==2){return ReadStep(c,ReadNeed::Member);}if(facts.source==3){return ReadStep(c,ReadNeed::Index);}c.phase=20;return ReadStep(c,ReadNeed::Evaluate);}
 if(c.phase==12){if(facts.available==0){return ReadStep(c,ReadNeed::BadAvailable);}if(c.path==4){c.phase=16;return ReadStep(c,ReadNeed::HoldOwned);}if(c.path==1||c.path==2){c.phase=13;return ReadStep(c,ReadNeed::Prepare);}c.phase=14;return ReadStep(c,ReadNeed::Variants);}
 if(c.phase==13){if(c.path==1){c.phase=17;return ReadStep(c,ReadNeed::Borrow);}c.phase=14;return ReadStep(c,ReadNeed::Variants);}
 if(c.phase==14){if(facts.variants!=0){c.phase=16;return ReadStep(c,ReadNeed::Load);}if(c.path==2){c.phase=15;return ReadStep(c,ReadNeed::Initialize);}c.phase=18;return ReadStep(c,ReadNeed::ProjectionPlace);}
 if(c.phase==18){if(facts.place!=0){c.phase=15;return ReadStep(c,ReadNeed::Initialize);}c.phase=16;return ReadStep(c,ReadNeed::Load);}
 if(c.phase==15){c.phase=16;return ReadStep(c,ReadNeed::Load);}
 if(c.phase==16){c.phase=17;return ReadStep(c,ReadNeed::Borrow);}
 if(c.phase==17||c.phase==21){return ReadStep(c,ReadNeed::Return);}
 if(c.phase==20){if(facts.continuing==0){return ReadStep(c,ReadNeed::Complete);}if(facts.present==0){return ReadStep(c,ReadNeed::BadRead);}if(facts.kind==3){c.path=4;c.phase=12;return ReadStep(c,ReadNeed::Available);}if(facts.kind==2){c.phase=21;return ReadStep(c,ReadNeed::HoldData);}if(facts.kind==1){return ReadStep(c,ReadNeed::Return);}}
 return ReadStep(c,ReadNeed::BadRead);
}
extern "C" ReadMemberStep read_member_step(LocalCursor cursor,ReadMemberFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return ReadMemberStep(c,ReadMemberNeed::Parent);}
 if(c.phase==1){if(facts.present==0){return ReadMemberStep(c,ReadMemberNeed::Complete);}c.phase=3;if(facts.kind==3){c.phase=2;return ReadMemberStep(c,ReadMemberNeed::Find);}if(facts.kind==1){return ReadMemberStep(c,ReadMemberNeed::Value);}if(facts.kind==2&&local_spelling(facts.field.data,facts.field.length,"length")){return ReadMemberStep(c,ReadMemberNeed::Length);}return ReadMemberStep(c,ReadMemberNeed::BadField);}
 if(c.phase==2){if(facts.found==0){return ReadMemberStep(c,ReadMemberNeed::MissingField);}c.phase=3;return ReadMemberStep(c,ReadMemberNeed::Unpack);}
 if(c.phase==3){c.phase=4;return ReadMemberStep(c,ReadMemberNeed::Item);}
 if(c.phase==4){if(facts.kind!=1){return ReadMemberStep(c,ReadMemberNeed::Return);}c.phase=5;return ReadMemberStep(c,ReadMemberNeed::Retained);}
 if(c.phase==5){if(facts.empty!=0&&facts.owner!=0){c.phase=6;return ReadMemberStep(c,ReadMemberNeed::Release);}}
 return ReadMemberStep(c,ReadMemberNeed::Plain);
}
extern "C" u64 read_field(SeekName name,u64 count,address context,cfn<SeekName,address,u64> field) on cpu requires nonzero(field) {
 u64 index=0u64;while(index<count){if(seek_same_name(name,field(context,index))){return index;}index+=1u64;}return count;
}
extern "C" ReadIndexStep read_index_step(ReadIndexCursor cursor,ReadIndexFacts facts) on cpu {
 ReadIndexCursor c=cursor;
 if(c.phase==0){c.phase=1;return ReadIndexStep(c,ReadIndexNeed::Reference,0u64);}
 if(c.phase==1){if(facts.present!=0){c.phase=3;return ReadIndexStep(c,ReadIndexNeed::DataIndex,0u64);}c.phase=2;return ReadIndexStep(c,ReadIndexNeed::Parent,0u64);}
 if(c.phase==2){if(facts.present==0){return ReadIndexStep(c,ReadIndexNeed::Complete,0u64);}c.phase=4;return ReadIndexStep(c,ReadIndexNeed::Kind,0u64);}
 if(c.phase==4){if(facts.kind==1){if(facts.indexed!=0){c.phase=5;return ReadIndexStep(c,ReadIndexNeed::ArrayValue,0u64);}c.phase=6;return ReadIndexStep(c,ReadIndexNeed::ViewPlace,0u64);}if(facts.kind==2){c.phase=7;return ReadIndexStep(c,ReadIndexNeed::Data,0u64);}if(facts.kind==3){c.phase=10;return ReadIndexStep(c,ReadIndexNeed::ArrayType,0u64);}return ReadIndexStep(c,ReadIndexNeed::BadIndex,0u64);}
 if(c.phase==5){return ReadIndexStep(c,facts.present!=0 ? ReadIndexNeed::Return : ReadIndexNeed::Complete,0u64);}
 if(c.phase==6){if(facts.present==0){return ReadIndexStep(c,ReadIndexNeed::Complete,0u64);}c.phase=9;return ReadIndexStep(c,ReadIndexNeed::Plain,0u64);}
 if(c.phase==7){c.phase=3;return ReadIndexStep(c,ReadIndexNeed::DataIndex,0u64);}
 if(c.phase==3){if(facts.present==0){return ReadIndexStep(c,ReadIndexNeed::Complete,0u64);}c.phase=8;return ReadIndexStep(c,ReadIndexNeed::DataType,0u64);}
 if(c.phase==8){if(facts.owns!=0){c.phase=11;return ReadIndexStep(c,ReadIndexNeed::Storage,0u64);}c.phase=9;return ReadIndexStep(c,ReadIndexNeed::Plain,0u64);}
 if(c.phase==9||c.phase==13||c.phase==28){return ReadIndexStep(c,ReadIndexNeed::Return,0u64);}
 if(c.phase==11){c.phase=12;return ReadIndexStep(c,ReadIndexNeed::Materialize,0u64);}
 if(c.phase==12){c.phase=13;return ReadIndexStep(c,ReadIndexNeed::BorrowElement,0u64);}
 if(c.phase==10){if(facts.array==0){return ReadIndexStep(c,ReadIndexNeed::BadArray,0u64);}c.phase=14;return ReadIndexStep(c,ReadIndexNeed::Owner,0u64);}
 if(c.phase==14){if(facts.owner==0&&facts.place==0){c.phase=15;return ReadIndexStep(c,ReadIndexNeed::Hold,0u64);}c.phase=16;return ReadIndexStep(c,ReadIndexNeed::Expression,0u64);}
 if(c.phase==15){c.phase=16;return ReadIndexStep(c,ReadIndexNeed::Expression,0u64);}
 if(c.phase==16){if(facts.present==0){return ReadIndexStep(c,ReadIndexNeed::Complete,0u64);}c.phase=17;return ReadIndexStep(c,ReadIndexNeed::Type,0u64);}
 if(c.phase==17){c.phase=18;return ReadIndexStep(c,ReadIndexNeed::Bounds,0u64);}
 if(c.phase==18){if(facts.valid==0){return ReadIndexStep(c,ReadIndexNeed::BadBounds,0u64);}c.phase=19;return ReadIndexStep(c,ReadIndexNeed::Range,0u64);}
 if(c.phase==19){if(facts.minimum==facts.maximum){c.phase=26;return ReadIndexStep(c,ReadIndexNeed::Unpack,facts.minimum);}c.phase=20;c.index=0u64;c.model=0;}
 if(c.phase==20){while(c.index<facts.count){u64 at=c.index;c.index+=1u64;if(at>=facts.minimum&&at<=facts.maximum){u32 first=c.model;c.model=1;return ReadIndexStep(c,first==0 ? ReadIndexNeed::First : ReadIndexNeed::Join,at);}}c.phase=21;return ReadIndexStep(c,ReadIndexNeed::SourcePlace,0u64);}
 if(c.phase==21){if(facts.place==0){c.phase=22;return ReadIndexStep(c,ReadIndexNeed::MaterializeArray,0u64);}c.phase=23;return ReadIndexStep(c,ReadIndexNeed::ArrayPlace,0u64);}
 if(c.phase==22){c.phase=23;return ReadIndexStep(c,ReadIndexNeed::ArrayPlace,0u64);}
 if(c.phase==23){c.phase=24;c.index=0u64;}
 if(c.phase==24){if(c.index<facts.components){u64 at=c.index;c.index+=1u64;return ReadIndexStep(c,ReadIndexNeed::Component,at);}c.phase=26;return ReadIndexStep(c,ReadIndexNeed::Rebind,0u64);}
 if(c.phase==26){c.phase=27;return ReadIndexStep(c,ReadIndexNeed::ModelKind,0u64);}
 if(c.phase==29){c.phase=28;return ReadIndexStep(c,ReadIndexNeed::BorrowModel,0u64);}
 if(facts.kind==3&&c.phase==27){if(c.model!=0){c.phase=29;return ReadIndexStep(c,ReadIndexNeed::SetPlace,0u64);}c.phase=28;return ReadIndexStep(c,ReadIndexNeed::BorrowModel,0u64);}return ReadIndexStep(c,ReadIndexNeed::Return,0u64);
}

extern "C" VariantBuildStep variant_build_step(LocalCursor cursor,VariantBuildFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.owned!=0&&(facts.cpu==0||facts.application==0)){return VariantBuildStep(c,VariantBuildNeed::InvalidTarget);}c.phase=1;return VariantBuildStep(c,VariantBuildNeed::Payload);}
 if(c.phase==1){
  if(facts.unit!=0){if(facts.arguments!=0u64){return VariantBuildStep(c,VariantBuildNeed::VoidArguments);}c.phase=4;return VariantBuildStep(c,facts.owned!=0 ? VariantBuildNeed::Unit : VariantBuildNeed::Build);}
  if(facts.arguments!=1u64){return VariantBuildStep(c,VariantBuildNeed::PayloadArguments);}c.phase=2;return VariantBuildStep(c,VariantBuildNeed::Evaluate);
 }
 if(c.phase==2){if(facts.owned!=0&&facts.continuing==0){return VariantBuildStep(c,VariantBuildNeed::Complete);}if(facts.present==0){return VariantBuildStep(c,facts.owned!=0 ? VariantBuildNeed::Missing : VariantBuildNeed::Complete);}c.phase=3;return VariantBuildStep(c,VariantBuildNeed::Check);}
 if(c.phase==3||c.phase==4){c.phase=5;return VariantBuildStep(c,facts.owned!=0 ? VariantBuildNeed::Storage : VariantBuildNeed::Build);}
 return VariantBuildStep(c,VariantBuildNeed::Build);
}

extern "C" ReturnExprStep return_expr_step(LocalCursor cursor,SignatureNode source,ReturnExprFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){
  ExprNode expression=unsafe_position<ExprNode>(unsafe_address(address_bits(source.storage.nodes)+source.offset));
  match(expression.kind){
   ExprKind::Target(unused){return ReturnExprStep(c,ReturnExprNeed::Target);}
   ExprKind::Member(unused){return ReturnExprStep(c,ReturnExprNeed::Member);}
   ExprKind::Index(unused){c.phase=2;return ReturnExprStep(c,ReturnExprNeed::IndexType);}
   ExprKind::Construct(unused){c.phase=5;return ReturnExprStep(c,ReturnExprNeed::BindType);}
   ExprKind::Name(unused){c.phase=1;return ReturnExprStep(c,ReturnExprNeed::Binding);}
   ExprKind::Select(unused){return ReturnExprStep(c,ReturnExprNeed::Selection);}
   ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Call(unused){} ExprKind::Invoke(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){}
  }return ReturnExprStep(c,ReturnExprNeed::Value);
 }
 if(c.phase==1){return ReturnExprStep(c,facts.binding==1 ? ReturnExprNeed::Dataset : facts.binding==2 ? ReturnExprNeed::Owned : ReturnExprNeed::Value);}
 if(c.phase==2){if(facts.owns!=0){return ReturnExprStep(c,ReturnExprNeed::OwnedIndex);}c.phase=3;return ReturnExprStep(c,ReturnExprNeed::Temporary);}
 if(c.phase==3){if(facts.temporary!=0){return ReturnExprStep(c,ReturnExprNeed::OwnedIndex);}c.phase=4;return ReturnExprStep(c,ReturnExprNeed::ExpressionType);}
 if(c.phase==4){if(facts.owns!=0){c.phase=8;return ReturnExprStep(c,ReturnExprNeed::Place);}return ReturnExprStep(c,ReturnExprNeed::Value);}
 if(c.phase==5){c.phase=6;return ReturnExprStep(c,ReturnExprNeed::Storage);}
 if(c.phase==6){if(facts.task!=0){return ReturnExprStep(c,ReturnExprNeed::BadTask);}return ReturnExprStep(c,facts.owns!=0 ? ReturnExprNeed::OwnedRecord : ReturnExprNeed::Record);}
 return ReturnExprStep(c,facts.present!=0 ? ReturnExprNeed::Take : ReturnExprNeed::Complete);
}
extern "C" MemberStep member_step(LocalCursor cursor,MemberFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return MemberStep(c,MemberNeed::Type);}
 if(c.phase==1){if(facts.owns!=0){c.phase=2;return MemberStep(c,MemberNeed::Place);}c.phase=3;return MemberStep(c,MemberNeed::Projection);}
 if(c.phase==2){return MemberStep(c,facts.present!=0 ? MemberNeed::Take : MemberNeed::Complete);}
 if(c.phase==3){if(facts.present!=0){return MemberStep(c,MemberNeed::Projected);}c.phase=4;return MemberStep(c,MemberNeed::Reference);}
 if(c.phase==4){if(facts.present!=0){return MemberStep(c,local_spelling(facts.name.data,facts.name.length,"length") ? MemberNeed::ReferenceLength : MemberNeed::BadField);}c.phase=5;return MemberStep(c,MemberNeed::Variants);}
 if(c.phase==5){if(facts.variants!=0){return MemberStep(c,MemberNeed::BadVariant);}c.phase=6;return MemberStep(c,MemberNeed::Temporary);}
 if(c.phase==6){if(facts.temporary!=0){c.phase=10;return MemberStep(c,MemberNeed::Evaluate);}c.phase=7;return MemberStep(c,MemberNeed::IsPlace);}
 if(c.phase==7){if(facts.place!=0){c.phase=20;return MemberStep(c,MemberNeed::Binding);}c.phase=8;return MemberStep(c,MemberNeed::Resource);}
 if(c.phase==8){if(facts.resource!=0){c.phase=20;return MemberStep(c,MemberNeed::Binding);}c.phase=9;return MemberStep(c,MemberNeed::Binding);}
 if(c.phase==9){if(facts.binding!=0){c.phase=21;}else{c.phase=10;return MemberStep(c,MemberNeed::Evaluate);}}
 if(c.phase==20){c.phase=21;}
 if(c.phase==21){
  if(facts.binding==1){if(local_spelling(facts.name.data,facts.name.length,"length")){c.phase=22;return MemberStep(c,MemberNeed::TextureShape);}return MemberStep(c,MemberNeed::BadTexture);}
  if(facts.binding==2){if(local_spelling(facts.name.data,facts.name.length,"length")){return MemberStep(c,MemberNeed::TileLength);}if(local_spelling(facts.name.data,facts.name.length,"lane")){return MemberStep(c,MemberNeed::TileLane);}return MemberStep(c,MemberNeed::BadTile);}
  if(local_spelling(facts.name.data,facts.name.length,"length")){if(facts.resource!=0){return MemberStep(c,MemberNeed::DatasetLength);}c.phase=23;return MemberStep(c,MemberNeed::Resource);}
  c.phase=24;return MemberStep(c,MemberNeed::BasePlace);
 }
 if(c.phase==22){return MemberStep(c,facts.present!=0 ? MemberNeed::TextureLength : MemberNeed::BadTexture);}
 if(c.phase==23){if(facts.resource!=0){return MemberStep(c,MemberNeed::DatasetLength);}c.phase=24;return MemberStep(c,MemberNeed::BasePlace);}
 if(c.phase==24){if(facts.present!=0){c.phase=25;return MemberStep(c,MemberNeed::OrdinaryPlace);}c.phase=28;return MemberStep(c,MemberNeed::OrdinaryValue);}
 if(c.phase==25){if(facts.present==0){return MemberStep(c,MemberNeed::Complete);}c.phase=26;return MemberStep(c,MemberNeed::PlaceKind);}
 if(c.phase==26){
  if(facts.array!=0&&local_spelling(facts.name.data,facts.name.length,"length")){return MemberStep(c,MemberNeed::ArrayLength);}
  if(facts.view!=0){return MemberStep(c,local_spelling(facts.name.data,facts.name.length,"length") ? MemberNeed::ReadView : MemberNeed::BadView);}
  return MemberStep(c,MemberNeed::ReadField);
 }
 if(c.phase==28){return MemberStep(c,facts.present!=0 ? MemberNeed::OrdinaryMemberValue : MemberNeed::Complete);}
 if(c.phase==10){if(facts.continuing==0){return MemberStep(c,MemberNeed::Complete);}if(facts.kind==3){c.phase=11;return MemberStep(c,MemberNeed::Find);}if(facts.kind==2){if(!local_spelling(facts.name.data,facts.name.length,"length")){return MemberStep(c,MemberNeed::BadField);}c.phase=18;return MemberStep(c,MemberNeed::Length);}if(facts.kind==1){return MemberStep(c,MemberNeed::Value);}return MemberStep(c,MemberNeed::Missing);}
 if(c.phase==11){if(facts.found==0){return MemberStep(c,MemberNeed::MissingField);}c.phase=12;return MemberStep(c,MemberNeed::Unpack);}
 if(c.phase==12){c.phase=13;return MemberStep(c,MemberNeed::FieldType);}
 if(c.phase==13){if(facts.field_owns!=0){c.phase=14;return MemberStep(c,MemberNeed::ItemKind);}c.phase=16;return MemberStep(c,MemberNeed::Retained);}
 if(c.phase==14){c.phase=15;return MemberStep(c,facts.item_kind==2 ? MemberNeed::MoveData : MemberNeed::MoveOwner);}
 if(c.phase==15){c.phase=16;return MemberStep(c,MemberNeed::Retained);}
 if(c.phase==16){c.phase=17;return MemberStep(c,facts.empty!=0 ? MemberNeed::DropOwner : MemberNeed::HoldOwner);}
 if(c.phase==18){c.phase=17;return MemberStep(c,MemberNeed::DropData);}
 return MemberStep(c,MemberNeed::Return);
}
extern "C" OwnedIndexStep owned_index_step(LocalCursor cursor,OwnedIndexFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return OwnedIndexStep(c,OwnedIndexNeed::ExistingType);}
 if(c.phase==1){if(facts.defined!=0){c.phase=2;return OwnedIndexStep(c,OwnedIndexNeed::Place);}c.phase=3;return OwnedIndexStep(c,OwnedIndexNeed::Evaluate);}
 if(c.phase==2){return OwnedIndexStep(c,facts.present!=0 ? OwnedIndexNeed::TakePlace : OwnedIndexNeed::Complete);}
 if(c.phase==3){if(facts.continuing==0){return OwnedIndexStep(c,OwnedIndexNeed::Complete);}c.phase=4;return OwnedIndexStep(c,OwnedIndexNeed::Inspect);}
 if(c.phase==4){if(facts.kind==3&&facts.array!=0){c.phase=6;return OwnedIndexStep(c,OwnedIndexNeed::HoldOwner);}if(facts.kind==2){c.phase=11;return OwnedIndexStep(c,OwnedIndexNeed::HoldData);}if(facts.kind==1){if(facts.indexed!=0){return OwnedIndexStep(c,OwnedIndexNeed::ArrayValue);}c.phase=18;return OwnedIndexStep(c,OwnedIndexNeed::ViewPlace);}return OwnedIndexStep(c,OwnedIndexNeed::BadArray);}
 if(c.phase==6){c.phase=7;return OwnedIndexStep(c,OwnedIndexNeed::Index);}
 if(c.phase==7){if(facts.present==0){return OwnedIndexStep(c,OwnedIndexNeed::Complete);}c.phase=8;return OwnedIndexStep(c,OwnedIndexNeed::ArrayPlace);}
 if(c.phase==8){c.phase=9;return OwnedIndexStep(c,OwnedIndexNeed::TakeElement);}
 if(c.phase==9){c.phase=10;return OwnedIndexStep(c,OwnedIndexNeed::Release);}
 if(c.phase==11){c.phase=12;return OwnedIndexStep(c,OwnedIndexNeed::DataIndex);}
 if(c.phase==12){if(facts.present==0){return OwnedIndexStep(c,OwnedIndexNeed::Complete);}c.phase=13;return OwnedIndexStep(c,OwnedIndexNeed::DataType);}
 if(c.phase==13){c.phase=14;return OwnedIndexStep(c,facts.owns!=0 ? OwnedIndexNeed::TakeData : OwnedIndexNeed::ReadData);}
 if(c.phase==14){c.phase=15;return OwnedIndexStep(c,OwnedIndexNeed::Storage);}
 if(c.phase==15){if(facts.empty!=0){c.phase=10;return OwnedIndexStep(c,OwnedIndexNeed::Release);}return OwnedIndexStep(c,OwnedIndexNeed::Return);}
 if(c.phase==18){return OwnedIndexStep(c,facts.present!=0 ? OwnedIndexNeed::ReadView : OwnedIndexNeed::Complete);}
 return OwnedIndexStep(c,OwnedIndexNeed::Return);
}


extern "C" SourceValueStep source_value_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu {
 SourceValueStep result=SourceValueStep(cursor,SourceValueNeed::Empty,Text(0,Span(0u64,0u64)),0u64,0u64,0u64,Slice(0u64,0u64),0,0);
 if(cursor.phase==7){result.need=SourceValueNeed::FinishReturned;return result;}
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
  ExprKind::FunctionRef(unused){result.need=SourceValueNeed::BadFunction;}
  ExprKind::Cooperate(unused){result.need=SourceValueNeed::BadCooperate;}
  ExprKind::StaticU32(value){result.need=SourceValueNeed::StaticU32;result.bits=value;}
  ExprKind::Bool(value){result.need=SourceValueNeed::Bool;result.bits=value!=0 ? 1 : 0;}
  ExprKind::Number(value){result.need=SourceValueNeed::Number;result.spelling=value;}
  ExprKind::Name(value){result.need=SourceValueNeed::Name;result.spelling=value;}
  ExprKind::Specialize(unused){result.need=SourceValueNeed::Specialized;}
  ExprKind::Member(value){result.cursor.phase=7;result.need=SourceValueNeed::Member;result.first=value.base.offset;result.spelling=value.field;}
  ExprKind::Index(value){result.need=SourceValueNeed::Index;result.first=value.base.offset;result.second=value.index.offset;}
  ExprKind::Select(value){result.need=SourceValueNeed::Select;result.first=value.condition.offset;result.second=value.yes.offset;result.third=value.no.offset;}
  ExprKind::Target(value){result.cursor.phase=7;result.need=SourceValueNeed::Target;result.target=value.target;result.first=value.value.offset;}
  ExprKind::Construct(value){
   result.first=value.ty.offset;result.arguments=value.arguments;
   if(cursor.phase==0){result.cursor.phase=6;result.need=SourceValueNeed::ConstructType;}
   else{result.need=SourceValueNeed::Construct;}
  }
  ExprKind::Call(unused){result.cursor.phase=7;result.need=SourceValueNeed::Returned;}
  ExprKind::Invoke(unused){result.cursor.phase=7;result.need=SourceValueNeed::Returned;}
  ExprKind::Unary(value){
   result.spelling=value.operation;result.first=value.value.offset;
   if(cursor.phase==0){
    address bytes=binding_at(value.operation.kind==0 ? source.storage.source : source.storage.text,value.operation.range.start);
    if(local_spelling(bytes,value.operation.range.end-value.operation.range.start,"-")){
     ExprNode operand=binding_node<ExprNode>(source.storage.nodes,value.value.offset);
     match(operand.kind){
      ExprKind::Number(number){result.need=SourceValueNeed::Negative;result.spelling=number;return result;}
      ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Name(unused){} ExprKind::Member(unused){} ExprKind::Index(unused){} ExprKind::Call(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
     }
    }
    result.cursor.phase=1;result.need=SourceValueNeed::Operand;
   }else{result.need=present!=0 ? SourceValueNeed::Unary : SourceValueNeed::Empty;}
  }
  ExprKind::Binary(value){
   result.spelling=value.operation;result.first=value.left.offset;result.second=value.right.offset;
   if(cursor.phase==0){
    address bytes=binding_at(value.operation.kind==0 ? source.storage.source : source.storage.text,value.operation.range.start);
    u64 length=value.operation.range.end-value.operation.range.start;
    bool shorted=local_spelling(bytes,length,"&&")||local_spelling(bytes,length,"||");
    result.cursor.phase=shorted ? 4 : 2;result.need=shorted ? SourceValueNeed::Condition : SourceValueNeed::Left;
   }else if(present==0){result.need=SourceValueNeed::Empty;}
   else if(cursor.phase==2){result.cursor.phase=3;result.need=SourceValueNeed::Right;}
   else if(cursor.phase==3){result.need=SourceValueNeed::Binary;}
   else if(cursor.phase==4){result.cursor.phase=5;result.need=SourceValueNeed::RequireCondition;}
   else{result.need=SourceValueNeed::ShortCircuit;}
  }
 }
 return result;
}
extern "C" ReturnedValueStep returned_value_step(LocalCursor cursor,u32 continuing,u32 boolean) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(continuing==0){return ReturnedValueStep(c,ReturnedValueNeed::Empty);}c.phase=1;return ReturnedValueStep(c,ReturnedValueNeed::Ordinary);}
 if(c.phase==1){c.phase=2;return ReturnedValueStep(c,ReturnedValueNeed::Value);}
 if(c.phase==2){c.phase=3;if(boolean!=0){return ReturnedValueStep(c,ReturnedValueNeed::Bool);}}
 return ReturnedValueStep(c,ReturnedValueNeed::Complete);
}
extern "C" ShortCircuitPlan short_circuit_plan(SeekName operation) on cpu {
 return ShortCircuitPlan(local_spelling(operation.data,operation.length,"&&") ? ShortCircuitOrder::RightYes : ShortCircuitOrder::RightNo,
  local_spelling(operation.data,operation.length,"||") ? 1 : 0);
}


extern "C" SourcePlaceStep source_place_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu {
 SourcePlaceStep result=SourcePlaceStep(cursor,SourcePlaceNeed::Bad,Text(0,Span(0u64,0u64)),0u64,0u64,Slice(0u64,0u64),Slice(0u64,0u64));
 if(cursor.phase==2&&present==0){result.need=SourcePlaceNeed::Empty;return result;}
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
  ExprKind::Name(name){result.need=SourcePlaceNeed::Name;result.spelling=name;}
  ExprKind::Specialize(value){
   address bytes=binding_at(value.name.kind==0 ? source.storage.source : source.storage.text,value.name.range.start);
   if(local_spelling(bytes,value.name.range.end-value.name.range.start,"unsafe_position")){
    result.types=value.types;result.arguments=value.arguments;
    if(cursor.phase==0){result.cursor.phase=1;result.need=SourcePlaceNeed::Bind;}else{result.need=SourcePlaceNeed::External;}
   }
  }
  ExprKind::Member(value){result.first=value.base.offset;result.spelling=value.field;if(cursor.phase==0){result.cursor.phase=2;result.need=SourcePlaceNeed::Parent;}else{result.need=SourcePlaceNeed::Project;}}
  ExprKind::Index(value){result.need=SourcePlaceNeed::Index;result.first=value.base.offset;result.second=value.index.offset;}
  ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Call(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
 }
 return result;
}
extern "C" MemberPositionNeed member_position(u32 view) on cpu {return view!=0 ? MemberPositionNeed::BadView : MemberPositionNeed::Project;}
extern "C" NamePositionStep name_position_step(LocalCursor cursor,NamePositionFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){
  if(facts.kind==1){return NamePositionStep(c,NamePositionNeed::Atomic);}
  if(facts.kind==2){return NamePositionStep(c,NamePositionNeed::Owned);}
  if(facts.kind==5){return NamePositionStep(c,NamePositionNeed::ViewElement);}
  if(facts.kind==3){c.phase=1;return NamePositionStep(c,NamePositionNeed::InspectOwned);}
  if(facts.kind==4){c.phase=2;return NamePositionStep(c,NamePositionNeed::ArrayType);}
  return NamePositionStep(c,NamePositionNeed::Bad);
 }
 if(c.phase==1){return NamePositionStep(c,facts.present!=0 ? NamePositionNeed::ExistingOwned : NamePositionNeed::MaterializeOwned);}
 return NamePositionStep(c,facts.arrays!=0 ? NamePositionNeed::MaterializeArray : NamePositionNeed::Bad);
}
extern "C" IndexPositionStep index_position_step(LocalCursor cursor,SignatureNode source,IndexPositionFacts facts) on cpu {
 IndexPositionStep r=IndexPositionStep(cursor,IndexPositionNeed::Empty,Text(0,Span(0u64,0u64)),0);
 if(cursor.phase==0){r.cursor.phase=1;r.need=IndexPositionNeed::Application;return r;}
 if(cursor.phase==1){
  if(facts.present!=0){r.need=IndexPositionNeed::ApplicationIndex;return r;}
  ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
  match(expression.kind){
   ExprKind::Name(name){r.cursor.phase=2;r.need=IndexPositionNeed::TileBinding;r.name=name;return r;}
   ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Member(unused){} ExprKind::Index(unused){} ExprKind::Call(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
  }
  r.cursor.phase=3;r.need=IndexPositionNeed::Dataset;return r;
 }
 if(cursor.phase==2){if(facts.present!=0){r.need=IndexPositionNeed::TileIndex;return r;}r.cursor.phase=3;r.need=IndexPositionNeed::Dataset;return r;}
 if(cursor.phase==3){if(facts.present==0){r.need=IndexPositionNeed::View;return r;}r.cursor.phase=4;r.need=IndexPositionNeed::Index;return r;}
 if(cursor.phase==4){if(facts.present==0){return r;}r.cursor.phase=5;r.need=IndexPositionNeed::Type;return r;}
 if(cursor.phase==5){r.cursor.phase=6;r.need=IndexPositionNeed::Bounds;return r;}
 if(cursor.phase==6){
  if(facts.bounded==0&&!(facts.current!=0&&(facts.same_domain!=0||facts.same_shape!=0))){r.need=IndexPositionNeed::BadBounds;return r;}
  if(facts.access==2||facts.access==3){r.cursor.phase=7;r.need=IndexPositionNeed::Append;return r;}
 }
 if(cursor.phase==7){if(facts.appending!=0){r.need=IndexPositionNeed::BadAppend;return r;}r.cursor.phase=8;r.need=IndexPositionNeed::Domain;return r;}
 r.need=IndexPositionNeed::Element;r.writable=(facts.access==2||facts.access==3||facts.access==4)&&(facts.whole!=0||facts.current!=0) ? 1 : 0;return r;
}
extern "C" u32 position_append(u64 binding,address context,cfn<PositionAppend,address> next) on cpu requires nonzero(next) {
 while(true){PositionAppend item=next(context);if(item.present==0){return 0;}if(item.output==binding||item.count==binding){return 1;}}
}
extern "C" ViewPositionStep view_position_step(LocalCursor cursor,ViewPositionFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=facts.placed!=0 ? 1 : 5;return ViewPositionStep(c,facts.placed!=0 ? ViewPositionNeed::Parent : ViewPositionNeed::Returned);}
 if(c.phase==1){if(facts.present==0){return ViewPositionStep(c,ViewPositionNeed::Empty);}c.phase=2;return ViewPositionStep(c,ViewPositionNeed::DatasetType);}
 if(c.phase==2){if(facts.dataset!=0){c.phase=4;return ViewPositionStep(c,ViewPositionNeed::Reference);}c.phase=3;return ViewPositionStep(c,ViewPositionNeed::IndexedType);}
 if(c.phase==3){if(facts.indexed!=0){return ViewPositionStep(c,ViewPositionNeed::ArrayPlace);}c.phase=8;return ViewPositionStep(c,ViewPositionNeed::Read);}
 if(c.phase==4){return ViewPositionStep(c,ViewPositionNeed::ApplicationIndex);}
 if(c.phase==5){if(facts.continuing==0){return ViewPositionStep(c,ViewPositionNeed::Empty);}c.phase=6;return ViewPositionStep(c,ViewPositionNeed::Kind);}
 if(c.phase==6){
  if(facts.kind==1){c.phase=4;return ViewPositionStep(c,ViewPositionNeed::HoldDataset);}
  if(facts.kind==2){c.phase=7;return ViewPositionStep(c,ViewPositionNeed::OwnedArray);}
  c.phase=8;return ViewPositionStep(c,ViewPositionNeed::Ordinary);
 }
 if(c.phase==7){if(facts.array!=0){c.phase=9;return ViewPositionStep(c,ViewPositionNeed::HoldOwned);}c.phase=8;return ViewPositionStep(c,ViewPositionNeed::Ordinary);}
 if(c.phase==8){if(facts.present==0){return ViewPositionStep(c,ViewPositionNeed::Empty);}c.phase=10;return ViewPositionStep(c,ViewPositionNeed::ArrayType);}
 if(c.phase==9){return ViewPositionStep(c,ViewPositionNeed::ArrayPlace);}
 if(facts.array!=0){c.phase=9;return ViewPositionStep(c,ViewPositionNeed::Materialize);}
 return ViewPositionStep(c,ViewPositionNeed::View);
}


extern "C" LexicalStep lexical_block_step(Cursor cursor,u64 count,u32 continuing) on cpu {
 Cursor c=cursor;
 if(c.phase<10){
  SequenceStep sequence=sequence_step(c,count,1,continuing);c=sequence.cursor;
  if(sequence.need==SequenceNeed::StartNext||sequence.need==SequenceNeed::StartStop){return LexicalStep(c,LexicalNeed::Enter);}
  if(sequence.need==SequenceNeed::Statement){return LexicalStep(c,LexicalNeed::Statement);}
  if(sequence.need==SequenceNeed::After){return LexicalStep(c,LexicalNeed::After);}
  if(sequence.need==SequenceNeed::Complete){c.phase=10;return LexicalStep(c,LexicalNeed::Finish);}
  return LexicalStep(c,LexicalNeed::Next);
 }
 if(c.phase==10){if(continuing!=0){c.phase=11;return LexicalStep(c,LexicalNeed::Release);}c.phase=12;return LexicalStep(c,LexicalNeed::Retain);}
 if(c.phase==11){c.phase=12;return LexicalStep(c,LexicalNeed::Retain);}
 return LexicalStep(c,LexicalNeed::Complete);
}
extern "C" SourceStatementStep source_statement_step(LocalCursor cursor,SignatureNode source,SourceStatementFacts facts) on cpu {
 SourceStatementStep result=SourceStatementStep(cursor,SourceStatementNeed::Complete,Text(0,Span(0u64,0u64)),Text(0,Span(0u64,0u64)),0u64,0u64,Slice(0u64,0u64),Slice(0u64,0u64),0);
 StmtNode statement=binding_node<StmtNode>(source.storage.nodes,source.offset);
 match(statement.kind){
  StmtKind::Declare(value){result.name=value.name;result.first=value.ty.offset;if(cursor.phase==0){result.cursor.phase=1;result.need=SourceStatementNeed::Bind;}else{result.need=SourceStatementNeed::Declare;}}
  StmtKind::Let(value){result.name=value.name;result.first=value.annotation.value.offset;result.second=value.value.offset;
   if(cursor.phase==0&&value.annotation.present!=0){result.cursor.phase=1;result.need=SourceStatementNeed::Bind;}else{result.need=SourceStatementNeed::Let;}
  }
  StmtKind::Assign(value){result.need=SourceStatementNeed::Assign;result.operation=value.operation;result.first=value.target.offset;result.second=value.value.offset;}
  StmtKind::Expr(value){result.need=SourceStatementNeed::Expression;result.first=value.offset;}
  StmtKind::If(value){result.first=value.condition.offset;result.body=value.yes;result.other=value.no;
   if(cursor.phase==0){result.cursor.phase=1;result.need=SourceStatementNeed::Condition;}
   else if(cursor.phase==1){if(facts.present!=0){result.cursor.phase=2;result.need=SourceStatementNeed::RequireBool;}}
   else{result.need=SourceStatementNeed::Conditional;}
  }
  StmtKind::While(value){result.first=value.condition.offset;result.body=value.body;if(cursor.phase==0){result.cursor.phase=1;result.need=SourceStatementNeed::InitialLoop;}else{result.need=SourceStatementNeed::While;}}
  StmtKind::For(value){result.need=SourceStatementNeed::For;result.name=value.name;result.first=value.start.offset;result.second=value.end.value.offset;result.present=value.end.present;result.body=value.body;}
  StmtKind::Break(){result.need=SourceStatementNeed::LoopControl;}
  StmtKind::Continue(){result.need=SourceStatementNeed::LoopControl;}
  StmtKind::Match(value){result.need=SourceStatementNeed::Match;result.first=value.value.offset;result.body=value.arms;}
  StmtKind::Return(value){result.need=SourceStatementNeed::Return;result.first=value.value.offset;result.present=value.present;}
  StmtKind::Parallel(value){result.need=SourceStatementNeed::Parallel;result.name=value.name;result.first=value.domain.offset;result.body=value.body;}
  StmtKind::Phase(){if(cursor.phase==0){result.cursor.phase=1;result.need=SourceStatementNeed::Entry;}else if(facts.entry==0){result.need=SourceStatementNeed::BadPhase;}}
  StmtKind::Seek(value){result.need=SourceStatementNeed::Seek;result.name=value.name;result.first=value.value.offset;}
 }
 return result;
}


extern "C" ReadPlaceStep read_place_step(Cursor cursor,ReadPlaceFacts facts) on cpu {
 Cursor c=cursor;
 if(c.phase==0){c.phase=1;return ReadPlaceStep(c,ReadPlaceNeed::Access,0);}
 if(c.phase==1){c.phase=2;return ReadPlaceStep(c,ReadPlaceNeed::Owns,0);}
 if(c.phase==2){if(facts.owns!=0){return ReadPlaceStep(c,ReadPlaceNeed::OwnedError,0);}c.phase=3;return ReadPlaceStep(c,ReadPlaceNeed::Kind,0);}
 if(c.phase==3){
  if(facts.kind==8||((facts.kind==0||facts.kind==1)&&facts.access==4)){return ReadPlaceStep(c,ReadPlaceNeed::AtomicError,0);}
  if(facts.kind==2){return ReadPlaceStep(c,ReadPlaceNeed::Array,0);}
  if(facts.kind==0){c.phase=4;return ReadPlaceStep(c,ReadPlaceNeed::ReferenceMemory,0);}
  if(facts.kind==1){c.phase=21;return ReadPlaceStep(c,ReadPlaceNeed::Initialize,0);}
  c.index=0u64;
  if(facts.kind==4){if(facts.access==4){return ReadPlaceStep(c,ReadPlaceNeed::AtomicTileError,0);}c.phase=30;if(facts.input!=0){return ReadPlaceStep(c,ReadPlaceNeed::TileInput,0);}}
  else if(facts.kind==5){c.phase=30;return ReadPlaceStep(c,ReadPlaceNeed::Initialize,0);}
  else if(facts.kind==7){if(facts.access==4){return ReadPlaceStep(c,ReadPlaceNeed::AtomicDataError,0);}if(facts.access==2||facts.access==3){c.phase=41;return ReadPlaceStep(c,ReadPlaceNeed::Initialize,0);}c.phase=42;return ReadPlaceStep(c,ReadPlaceNeed::DataAccess,0);}
  else{c.phase=30;}
 }
 if(c.phase==4){if(facts.memory!=0){c.phase=5;return ReadPlaceStep(c,ReadPlaceNeed::Address,0);}c.phase=8;return ReadPlaceStep(c,ReadPlaceNeed::Initialize,0);}
 if(c.phase==5){if(facts.reference==1){c.phase=6;return ReadPlaceStep(c,ReadPlaceNeed::RememberRead,0);}c.phase=7;return ReadPlaceStep(c,ReadPlaceNeed::Memory,0);}
 if(c.phase==6){c.phase=7;return ReadPlaceStep(c,ReadPlaceNeed::Memory,0);}
 if(c.phase==7||c.phase==8){c.phase=9;return ReadPlaceStep(c,ReadPlaceNeed::Zero,0);}
 if(c.phase==9){c.index=0u64;c.phase=30;}
 if(c.phase==21){c.phase=22;return ReadPlaceStep(c,ReadPlaceNeed::DatasetEffect,0);}
 if(c.phase==22){c.phase=23;return ReadPlaceStep(c,ReadPlaceNeed::ApplicationRead,0);}
 if(c.phase==23){c.phase=24;return ReadPlaceStep(c,ReadPlaceNeed::ApplicationProof,0);}
 if(c.phase==24){return ReadPlaceStep(c,ReadPlaceNeed::FinishApplication,0);}
 if(c.phase==41){c.phase=42;return ReadPlaceStep(c,ReadPlaceNeed::DataAccess,0);}
 if(c.phase==42){c.phase=30;if(facts.current==0){return ReadPlaceStep(c,ReadPlaceNeed::DataCross,0);}}
 if(c.phase==31){c.index+=1u64;c.phase=30;}
 if(c.phase==30){
  if(c.index<facts.components&&c.index<facts.words){c.phase=31;ReadPlaceNeed load=facts.kind==0 ? ReadPlaceNeed::LoadReference : facts.kind==3 ? ReadPlaceNeed::LoadView : facts.kind==4 ? ReadPlaceNeed::LoadTile : facts.kind==7 ? ReadPlaceNeed::LoadData : ReadPlaceNeed::LoadLocal;return ReadPlaceStep(c,load,0);}
  c.phase=32;return ReadPlaceStep(c,ReadPlaceNeed::Proof,0);
 }
 if(c.phase==32&&facts.kind==0&&facts.reference==1&&facts.memory!=0&&facts.known==0){c.phase=33;return ReadPlaceStep(c,ReadPlaceNeed::Snapshot,0);}
 if(c.phase==33){c.part=0u64;c.phase=34;return ReadPlaceStep(c,ReadPlaceNeed::BorrowFields,0);}
 if(c.phase==35&&facts.field_address!=0){if(c.flags==0){c.flags=1;c.phase=37;return ReadPlaceStep(c,ReadPlaceNeed::ShareBorrow,0);}c.phase=36;return ReadPlaceStep(c,ReadPlaceNeed::SetBorrow,0);}
 if(c.phase==37){c.phase=36;return ReadPlaceStep(c,ReadPlaceNeed::SetBorrow,0);}
 if(c.phase==35||c.phase==36){c.part+=1u64;c.phase=34;}
 if(c.phase==34&&c.part<facts.fields){c.phase=35;return ReadPlaceStep(c,ReadPlaceNeed::BorrowField,0);}
 if(c.phase!=50&&facts.kind==0&&facts.reference==2){c.phase=50;return ReadPlaceStep(c,ReadPlaceNeed::ExternalProof,0);}
 u32 flags=facts.kind==5||facts.kind==6 ? 3 : facts.kind==7 ? 0 : 2;
 return ReadPlaceStep(c,ReadPlaceNeed::Finish,flags);
}
extern "C" NameValueStep name_value_step(LocalCursor cursor,NameValueFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){c.phase=1;return NameValueStep(c,NameValueNeed::Scope);}
 if(c.phase==1){if(facts.present!=0){return NameValueStep(c,NameValueNeed::ScopeValue);}c.phase=2;return NameValueStep(c,NameValueNeed::Binding);}
 if(c.phase==2){
  if(facts.kind==4){return NameValueStep(c,NameValueNeed::Value);}
  if(facts.kind==2){c.phase=3;return NameValueStep(c,NameValueNeed::OwnedPlace);}
  if(facts.kind==1||facts.kind==5){c.phase=3;return NameValueStep(c,NameValueNeed::NamedPlace);}
  if(facts.kind==0){c.phase=4;return NameValueStep(c,NameValueNeed::Resolve);}
  return NameValueStep(c,NameValueNeed::Unknown);
 }
 if(c.phase==3){return NameValueStep(c,NameValueNeed::Read);}
 if(c.phase==4){if(facts.present==0){return NameValueStep(c,NameValueNeed::Unknown);}c.phase=5;return NameValueStep(c,NameValueNeed::FunctionKind);}
 return NameValueStep(c,facts.constant!=0 ? NameValueNeed::ModuleConstant : NameValueNeed::Unknown);
}
extern "C" u32 source_is_place(SignatureNode source,SourcePlaceQueryHost host) on cpu requires nonzero(host.binding) {
 u64 offset=source.offset;
 while(true){ExprNode expression=binding_node<ExprNode>(source.storage.nodes,offset);
  match(expression.kind){
   ExprKind::Name(name){u32 kind=host.binding(host.context,name);return kind==1||kind==2||kind==3||kind==5 ? 1 : 0;}
   ExprKind::Member(value){offset=value.base.offset;}
   ExprKind::Index(unused){return 1;}
   ExprKind::Specialize(value){address bytes=binding_at(value.name.kind==0 ? source.storage.source : source.storage.text,value.name.range.start);return local_spelling(bytes,value.name.range.end-value.name.range.start,"unsafe_position") ? 1 : 0;}
   ExprKind::Number(unused){return 0;} ExprKind::StaticU32(unused){return 0;} ExprKind::Bool(unused){return 0;} ExprKind::Call(unused){return 0;} ExprKind::Invoke(unused){return 0;} ExprKind::Construct(unused){return 0;} ExprKind::FunctionRef(unused){return 0;} ExprKind::Cooperate(unused){return 0;} ExprKind::Target(unused){return 0;} ExprKind::Unary(unused){return 0;} ExprKind::Binary(unused){return 0;} ExprKind::Select(unused){return 0;}
  }
 }
}

extern "C" WritePlaceStep write_place_step(Cursor cursor,WritePlaceFacts facts) on cpu {
 Cursor c=cursor;
 if(c.phase==0){c.phase=1;return WritePlaceStep(c,WritePlaceNeed::Access);}
 if(c.phase==1){c.phase=2;return WritePlaceStep(c,WritePlaceNeed::ViewType);}
 if(c.phase==2){if(facts.views!=0&&facts.persistent!=0){return WritePlaceStep(c,WritePlaceNeed::ViewPayloadError);}c.phase=3;return WritePlaceStep(c,WritePlaceNeed::Owned);}
 if(c.phase==3){c.phase=4;return WritePlaceStep(c,WritePlaceNeed::ViewAvailable);}
 if(c.phase==4){c.phase=5;return WritePlaceStep(c,WritePlaceNeed::Same);}
 if(c.phase==5){c.phase=6;return WritePlaceStep(c,WritePlaceNeed::Kind);}
 if(c.phase==6){c.phase=7;if(facts.kind==0){return WritePlaceStep(c,WritePlaceNeed::NumericProof);}}
 if(c.phase==7){c.phase=8;return WritePlaceStep(c,WritePlaceNeed::ReferenceMemory);}
 if(c.phase==8){if(facts.memory!=0){c.phase=9;return WritePlaceStep(c,WritePlaceNeed::Pending);}c.phase=20;}
 if(c.phase==9){c.phase=10;if(facts.pending!=0){return WritePlaceStep(c,WritePlaceNeed::Resolved);}}
 if(c.phase==10){if(facts.pending!=0&&facts.resolved==0){c.phase=11;return WritePlaceStep(c,WritePlaceNeed::Snapshot);}if(facts.kind==2){c.phase=18;return WritePlaceStep(c,WritePlaceNeed::NumericProof);}c.phase=11;return WritePlaceStep(c,WritePlaceNeed::AddressWrite);}
 if(c.phase==18){c.phase=11;return WritePlaceStep(c,WritePlaceNeed::AddressWrite);}
 if(c.phase==11){c.part=0u64;c.phase=12;return WritePlaceStep(c,WritePlaceNeed::BorrowFields);}
 if(c.phase==13){if(facts.field_address!=0){c.phase=14;return WritePlaceStep(c,WritePlaceNeed::AddField);}c.part+=1u64;c.phase=12;}
 if(c.phase==14){c.part+=1u64;c.phase=12;}
 if(c.phase==12){if(c.part<facts.fields){c.phase=13;return WritePlaceStep(c,WritePlaceNeed::BorrowField);}c.phase=20;return WritePlaceStep(c,WritePlaceNeed::Memory);}
 if(c.phase==20){
  if(facts.kind==8||((facts.kind==0||facts.kind==1)&&facts.access==4)){return WritePlaceStep(c,WritePlaceNeed::AtomicError);}
  if(facts.kind==0){c.phase=21;return WritePlaceStep(c,WritePlaceNeed::Writable);}
  if(facts.kind==1){c.phase=40;return WritePlaceStep(c,WritePlaceNeed::Writable);}
  if(facts.kind==2){return WritePlaceStep(c,WritePlaceNeed::Array);}
  if(facts.kind==6){return WritePlaceStep(c,WritePlaceNeed::ReadOnlyError);}
  if(facts.kind==3){return WritePlaceStep(c,WritePlaceNeed::ViewError);}
  if(facts.kind==4){return WritePlaceStep(c,facts.access==4 ? WritePlaceNeed::AtomicTileError : WritePlaceNeed::TileError);}
  if(facts.kind==7){if(facts.access==4){return WritePlaceStep(c,WritePlaceNeed::AtomicDataError);}if(facts.writable==0||(facts.access!=2&&facts.access!=3&&facts.access!=4)){return WritePlaceStep(c,WritePlaceNeed::WriteError);}}
  c.part=0u64;c.phase=60;
 }
 if(c.phase==21){c.phase=22;return WritePlaceStep(c,WritePlaceNeed::AssignProof);}
 if(c.phase==22){c.phase=23;return WritePlaceStep(c,WritePlaceNeed::Initialize);}
 if(c.phase==23){c.phase=24;return WritePlaceStep(c,WritePlaceNeed::Zero);}
 if(c.phase==24){c.index=0u64;c.phase=70;}
 if(c.phase==40){c.phase=41;return WritePlaceStep(c,WritePlaceNeed::DatasetEffect);}
 if(c.phase==41){c.phase=42;return WritePlaceStep(c,WritePlaceNeed::CaptureProof);}
 if(c.phase==42){c.phase=43;if(facts.iterator!=0){return WritePlaceStep(c,WritePlaceNeed::Alias);}}
 if(c.phase==43){c.phase=44;return WritePlaceStep(c,WritePlaceNeed::ApplicationWrite);}
 if(c.phase==44){c.phase=45;return WritePlaceStep(c,WritePlaceNeed::Initialize);}
 if(c.phase==45){c.phase=46;return WritePlaceStep(c,WritePlaceNeed::AssignApplicationProof);}
 if(c.phase==46){c.phase=47;if(facts.loops_empty!=0){return WritePlaceStep(c,WritePlaceNeed::CompleteWrites);}}
 if(c.phase==47){return WritePlaceStep(c,WritePlaceNeed::Complete);}
 if(c.phase==61){if(facts.unique==0){return WritePlaceStep(c,WritePlaceNeed::DuplicateError);}c.part+=1u64;c.phase=60;}
 if(c.phase==60){if(c.part<facts.words){c.phase=61;return WritePlaceStep(c,WritePlaceNeed::Unique);}c.phase=63;if(facts.kind==7){return WritePlaceStep(c,WritePlaceNeed::DataAccess);}}
 if(c.phase==63){c.phase=64;return WritePlaceStep(c,WritePlaceNeed::AssignProof);}
 if(c.phase==64){c.phase=65;if(facts.kind==5){return WritePlaceStep(c,WritePlaceNeed::Initialize);}}
 if(c.phase==65){c.index=0u64;c.phase=70;}
 if(c.phase==71){c.index+=1u64;c.phase=70;}
 if(c.phase==70){
  if(c.index<facts.words&&c.index<facts.values){c.phase=71;return WritePlaceStep(c,facts.kind==0 ? WritePlaceNeed::StoreReference : facts.kind==5 ? WritePlaceNeed::StoreLocal : WritePlaceNeed::StoreData);}
  c.phase=72;if(facts.kind==7){return WritePlaceStep(c,WritePlaceNeed::Initialize);}
 }
 return WritePlaceStep(c,WritePlaceNeed::Complete);
}

extern "C" UnsafeReadStep unsafe_read_step(LocalCursor cursor,StorageView source,Slice arguments,UnsafeReadFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){
  if(arguments.count!=2u64){return UnsafeReadStep(c,UnsafeReadNeed::BadArguments);}
  ExprNode first=binding_node<ExprNode>(source.nodes,binding_member(source.nodes,arguments,0u64));
  match(first.kind){
   ExprKind::Name(unused){c.phase=1;return UnsafeReadStep(c,UnsafeReadNeed::Application);}
   ExprKind::Member(unused){c.phase=1;return UnsafeReadStep(c,UnsafeReadNeed::Application);}
   ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}
   ExprKind::Index(unused){}ExprKind::Call(unused){}ExprKind::Invoke(unused){}ExprKind::Construct(unused){}
   ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}ExprKind::Cooperate(unused){}ExprKind::Target(unused){}
   ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  c.phase=3;return UnsafeReadStep(c,UnsafeReadNeed::Dataset);
 }
 if(c.phase==1){
  if(facts.present!=0){c.phase=2;return UnsafeReadStep(c,UnsafeReadNeed::ApplicationCpu);}
  c.phase=3;return UnsafeReadStep(c,UnsafeReadNeed::Dataset);
 }
 if(c.phase==2){c.phase=10;return UnsafeReadStep(c,UnsafeReadNeed::Index);}
 if(c.phase==3){
  if(facts.present!=0){
   if(facts.access!=1&&facts.access!=3){return UnsafeReadStep(c,UnsafeReadNeed::BadAccess);}
   c.phase=11;return UnsafeReadStep(c,UnsafeReadNeed::Index);
  }
  c.phase=4;return UnsafeReadStep(c,UnsafeReadNeed::Value);
 }
 if(c.phase==4){return UnsafeReadStep(c,facts.present!=0 ? UnsafeReadNeed::ViewRead : UnsafeReadNeed::Empty);}
 if(c.phase==10||c.phase==11){
  if(facts.present==0){return UnsafeReadStep(c,UnsafeReadNeed::Empty);}
  c.phase=c.phase==10 ? 12 : 13;return UnsafeReadStep(c,UnsafeReadNeed::IndexType);
 }
 return UnsafeReadStep(c,c.phase==12 ? UnsafeReadNeed::ApplicationRead : UnsafeReadNeed::DatasetRead);
}

extern "C" UnsafeViewStep unsafe_view_step(LocalCursor cursor,UnsafeViewFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.view==0){return UnsafeViewStep(c,UnsafeViewNeed::BadView);}c.phase=1;return UnsafeViewStep(c,UnsafeViewNeed::Hold);}
 if(c.phase==1){c.phase=2;return UnsafeViewStep(c,UnsafeViewNeed::Index);}
 if(c.phase==2){c.phase=3;return UnsafeViewStep(c,UnsafeViewNeed::Release);}
 if(c.phase==3){c.phase=4;return UnsafeViewStep(c,UnsafeViewNeed::Result);}
 if(c.phase==4){if(facts.present==0){return UnsafeViewStep(c,UnsafeViewNeed::Empty);}c.phase=5;return UnsafeViewStep(c,UnsafeViewNeed::IndexType);}
 return UnsafeViewStep(c,UnsafeViewNeed::Read);
}

extern "C" MemberValueNeed member_value_need(u32 array,u32 view,address bytes,usize length) on cpu {
 if(local_spelling(bytes,length,"length")){
  if(array!=0){return MemberValueNeed::ArrayLength;}
  if(view!=0){return MemberValueNeed::ViewLength;}
 }
 return MemberValueNeed::Project;
}

extern "C" RayQueryStep ray_query_step(LocalCursor cursor,RayQueryFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.count!=5u64&&facts.count!=6u64){return RayQueryStep(c,RayQueryNeed::BadArguments);}c.phase=1;return RayQueryStep(c,RayQueryNeed::Name);}
 if(c.phase==1){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::MissingName);}c.phase=2;return RayQueryStep(c,RayQueryNeed::Binding);}
 if(c.phase==2){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::BadSource);}c.phase=3;return RayQueryStep(c,RayQueryNeed::Origin);}
 if(c.phase==3){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::Empty);}c.phase=4;return RayQueryStep(c,RayQueryNeed::Minimum);}
 if(c.phase==4){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::Empty);}c.phase=5;return RayQueryStep(c,RayQueryNeed::Direction);}
 if(c.phase==5){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::Empty);}c.phase=6;return RayQueryStep(c,RayQueryNeed::Maximum);}
 if(c.phase==6){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::Empty);}c.phase=7;return RayQueryStep(c,RayQueryNeed::OriginType);}
 if(c.phase==7){c.phase=8;return RayQueryStep(c,RayQueryNeed::MinimumType);}
 if(c.phase==8){c.phase=9;return RayQueryStep(c,RayQueryNeed::DirectionType);}
 if(c.phase==9){c.phase=10;return RayQueryStep(c,RayQueryNeed::MaximumType);}
 if(c.phase==10){if(facts.count==6u64){c.phase=11;return RayQueryStep(c,RayQueryNeed::Cull);}c.phase=13;}
 if(c.phase==11){if(facts.present==0){return RayQueryStep(c,RayQueryNeed::Empty);}c.phase=12;return RayQueryStep(c,RayQueryNeed::CullType);}
 if(c.phase==12){c.phase=13;}
 if(c.phase==13){c.phase=14;return RayQueryStep(c,facts.any!=0 ? RayQueryNeed::AnyType : RayQueryNeed::ClosestType);}
 if(c.phase==14){c.phase=15;return RayQueryStep(c,RayQueryNeed::Registers);}
 if(c.phase==15){c.phase=16;return RayQueryStep(c,facts.any!=0 ? RayQueryNeed::AnyResult : RayQueryNeed::ClosestResult);}
 if(c.phase==16){c.phase=17;return RayQueryStep(c,RayQueryNeed::Emit);}
 return RayQueryStep(c,RayQueryNeed::Value);
}

extern "C" TextureArgumentStep texture_argument_step(Cursor cursor,SignatureNode source,TextureArgumentFacts facts) on cpu {
 Cursor c=cursor;TextureArgumentStep result=TextureArgumentStep(c,TextureArgumentNeed::BadSource,Text(0,Span(0u64,0u64)));
 if(c.phase==0){
  ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
  match(expression.kind){
   ExprKind::Name(value){result.cursor.phase=1;result.need=TextureArgumentNeed::Binding;result.name=value;return result;}
   ExprKind::Index(value){c.flags=1;c.part=value.base.offset;c.index=value.index.offset;c.phase=2;result.cursor=c;result.need=TextureArgumentNeed::Base;return result;}
   ExprKind::Call(value){
    address spelling=binding_at(value.name.kind==0 ? source.storage.source : source.storage.text,value.name.range.start);
    if(local_spelling(spelling,value.name.range.end-value.name.range.start,"unsafe_read")){
     if(value.arguments.count!=2u64){result.need=TextureArgumentNeed::BadArguments;return result;}
     c.flags=0;c.part=binding_member(source.storage.nodes,value.arguments,0u64);c.index=binding_member(source.storage.nodes,value.arguments,1u64);c.phase=2;result.cursor=c;result.need=TextureArgumentNeed::Base;return result;
    }
   }
   ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}ExprKind::Member(unused){}
   ExprKind::Invoke(unused){}ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}
   ExprKind::Cooperate(unused){}ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  return result;
 }
 if(c.phase==1){result.need=facts.present!=0 ? TextureArgumentNeed::Resource : TextureArgumentNeed::BadSource;return result;}
 if(c.phase==2){if(facts.present==0){result.need=TextureArgumentNeed::Empty;return result;}c.phase=3;result.cursor=c;result.need=TextureArgumentNeed::Shape;return result;}
 if(c.phase==3){if(facts.array==0){result.need=TextureArgumentNeed::BadShape;return result;}c.phase=4;result.cursor=c;result.need=TextureArgumentNeed::Index;return result;}
 if(c.phase==4){if(facts.present==0){result.need=TextureArgumentNeed::Empty;return result;}c.phase=5;result.cursor=c;result.need=TextureArgumentNeed::IndexType;return result;}
 if(c.phase==5&&c.flags!=0){c.phase=6;result.cursor=c;result.need=TextureArgumentNeed::Bounds;return result;}
 if(c.phase==6&&facts.bounded==0){result.need=TextureArgumentNeed::BadBounds;return result;}
 result.need=TextureArgumentNeed::Select;return result;
}
extern "C" TextureSampleStep texture_sample_step(LocalCursor cursor,TextureSampleFacts facts) on cpu {
 LocalCursor c=cursor;
 if(c.phase==0){if(facts.count!=3u64){return TextureSampleStep(c,TextureSampleNeed::BadArguments,0);}c.phase=1;return TextureSampleStep(c,TextureSampleNeed::Texture,0);}
 if(c.phase==1){if(facts.present==0){return TextureSampleStep(c,TextureSampleNeed::Empty,0);}c.phase=2;return TextureSampleStep(c,TextureSampleNeed::Shape,0);}
 if(c.phase==2){if(facts.array!=0){return TextureSampleStep(c,TextureSampleNeed::BadArray,0);}c.phase=3;return TextureSampleStep(c,TextureSampleNeed::Coordinate,0);}
 if(c.phase==3){if(facts.present==0){return TextureSampleStep(c,TextureSampleNeed::Empty,0);}c.phase=4;return TextureSampleStep(c,TextureSampleNeed::Lod,0);}
 if(c.phase==4){if(facts.present==0){return TextureSampleStep(c,TextureSampleNeed::Empty,0);}c.phase=5;return TextureSampleStep(c,TextureSampleNeed::Binding,0);}
 if(c.phase==5){c.phase=6;return TextureSampleStep(c,TextureSampleNeed::CoordinateType,0);}
 if(c.phase==6){c.phase=7;return TextureSampleStep(c,TextureSampleNeed::LodType,0);}
 if(c.phase==7){c.phase=8;return TextureSampleStep(c,TextureSampleNeed::Proof,0);}
 if(c.phase==8){c.phase=9;return TextureSampleStep(c,TextureSampleNeed::Registers,0);}
 if(c.phase==9){c.phase=10;return TextureSampleStep(c,TextureSampleNeed::Emit,0);}
 return TextureSampleStep(c,TextureSampleNeed::Value,facts.texture_uniform!=0&&facts.coordinate_uniform!=0&&facts.lod_uniform!=0 ? 1 : 0);
}

extern "C" AppendStep append_step(Cursor cursor,StorageView source,Slice arguments,AppendFacts facts) on cpu {
 Cursor c=cursor;AppendStep result=AppendStep(c,AppendNeed::BadOutput,Text(0,Span(0u64,0u64)));
 if(c.phase==0){
  if(arguments.count!=3u64){result.need=AppendNeed::BadArguments;return result;}
  ExprNode first=binding_node<ExprNode>(source.nodes,binding_member(source.nodes,arguments,0u64));
  match(first.kind){
   ExprKind::Name(value){result.cursor.phase=1;result.need=AppendNeed::Binding;result.name=value;}
   ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}ExprKind::Member(unused){}ExprKind::Index(unused){}
   ExprKind::Call(unused){}ExprKind::Invoke(unused){}ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}
   ExprKind::Cooperate(unused){}ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  return result;
 }
 if(c.phase==1){if(facts.present==0){return result;}c.phase=2;result.cursor=c;result.need=AppendNeed::Count;return result;}
 if(c.phase==2){c.phase=3;result.cursor=c;result.need=AppendNeed::TargetAccess;return result;}
 if(c.phase==3){if(facts.access!=2&&facts.access!=3){result.need=AppendNeed::BadContract;return result;}c.phase=4;result.cursor=c;result.need=AppendNeed::CountAccess;return result;}
 if(c.phase==4){if(facts.access!=2&&facts.access!=3){result.need=AppendNeed::BadContract;return result;}if(facts.access==2){c.phase=5;result.cursor=c;result.need=AppendNeed::Initialized;return result;}c.phase=6;}
 if(c.phase==5){if(facts.present==0){result.need=AppendNeed::BadContract;return result;}c.phase=6;}
 if(c.phase==6){if(facts.target==facts.count){result.need=AppendNeed::BadContract;return result;}c.phase=7;result.cursor=c;result.need=AppendNeed::Shape;return result;}
 if(c.phase==7){if(facts.present==0){result.need=AppendNeed::BadContract;return result;}c.phase=8;result.cursor=c;result.need=AppendNeed::CountType;return result;}
 if(c.phase==8){if(facts.present==0){result.need=AppendNeed::BadContract;return result;}c.phase=9;result.cursor=c;result.need=AppendNeed::Value;return result;}
 if(c.phase==9){if(facts.present==0){result.need=AppendNeed::Empty;return result;}c.phase=10;result.cursor=c;result.need=AppendNeed::ValueType;return result;}
 if(c.phase==10){if(facts.views!=0){result.need=AppendNeed::BadViews;return result;}c.phase=11;result.cursor=c;result.need=AppendNeed::Domains;return result;}
 if(c.phase==11){if(facts.target_domain!=0||facts.count_domain!=0){result.need=AppendNeed::BadDomain;return result;}c.phase=12;result.cursor=c;result.need=AppendNeed::Pairs;return result;}
 if(c.phase==12){if(facts.present!=0){result.need=AppendNeed::BadPair;return result;}c.phase=13;result.cursor=c;result.need=AppendNeed::Pair;return result;}
 if(c.phase==13){c.phase=14;result.cursor=c;result.need=AppendNeed::Read;return result;}
 if(c.phase==14){c.phase=15;result.cursor=c;result.need=AppendNeed::Writes;return result;}
 if(c.phase==15){c.phase=16;result.cursor=c;result.need=AppendNeed::CountRegion;return result;}
 if(c.phase==16){c.index=0u64;c.phase=17;}
 if(c.phase==18){c.index+=1u64;c.phase=17;}
 if(c.phase==17){if(c.index<facts.target_components){c.phase=18;result.cursor=c;result.need=AppendNeed::MemoryField;return result;}c.phase=19;result.cursor=c;result.need=AppendNeed::Cross;return result;}
 if(c.phase==19){c.phase=20;result.cursor=c;result.need=AppendNeed::Register;return result;}
 if(c.phase==20){c.index=0u64;c.phase=21;}
 if(c.phase==22){c.index+=1u64;c.phase=21;}
 if(c.phase==21){if(c.index<facts.value_components&&c.index<facts.registers){c.phase=22;result.cursor=c;result.need=AppendNeed::ValueField;return result;}c.phase=23;result.cursor=c;result.need=AppendNeed::Emit;return result;}
 result.need=AppendNeed::Complete;return result;
}
extern "C" u32 append_pair_conflict(u64 target,u64 count,u32 word,address context,cfn<PositionAppend,address> next) on cpu requires nonzero(next) {
 while(true){PositionAppend item=next(context);if(item.present==0){return 0;}if((item.output==target||item.count==count)&&(item.output!=target||item.count!=count||item.word!=word)){return 1;}}
}

extern "C" ClearPrefixStep clear_prefix_step(LocalCursor cursor,StorageView source,Slice arguments,ClearPrefixFacts facts) on cpu {
 LocalCursor c=cursor;ClearPrefixStep result=ClearPrefixStep(c,ClearPrefixNeed::BadOutput,Text(0,Span(0u64,0u64)),0u64,0,0u64,0);
 if(c.phase==0){
  if(arguments.count!=2u64){result.need=ClearPrefixNeed::BadArguments;return result;}
  ExprNode first=binding_node<ExprNode>(source.nodes,binding_member(source.nodes,arguments,0u64));
  match(first.kind){
   ExprKind::Name(value){result.cursor.phase=1;result.need=ClearPrefixNeed::Binding;result.name=value;}
   ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}ExprKind::Member(unused){}ExprKind::Index(unused){}
   ExprKind::Call(unused){}ExprKind::Invoke(unused){}ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}
   ExprKind::Cooperate(unused){}ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  return result;
 }
 if(c.phase==1){if(facts.present==0){return result;}c.phase=2;result.cursor=c;result.need=ClearPrefixNeed::Count;return result;}
 if(c.phase==2){if((facts.target_access!=2&&facts.target_access!=3)||(facts.count_access!=2&&facts.count_access!=3)||facts.target==facts.count){result.need=ClearPrefixNeed::BadContract;return result;}c.phase=3;result.cursor=c;result.need=ClearPrefixNeed::Shape;return result;}
 if(c.phase==3){if(facts.single==0){result.need=ClearPrefixNeed::BadContract;return result;}c.phase=4;result.cursor=c;result.need=ClearPrefixNeed::Written;result.end=facts.word+4;return result;}
 if(c.phase==4){c.phase=5;result.cursor=c;result.need=ClearPrefixNeed::Relation;return result;}
 if(c.phase==5){c.phase=6;result.cursor=c;result.need=ClearPrefixNeed::Origin;return result;}
 if(c.phase==6){c.phase=7;result.cursor=c;result.need=ClearPrefixNeed::RegisterU32;return result;}
 if(c.phase==7){c.phase=8;result.cursor=c;result.need=ClearPrefixNeed::Constant;return result;}
 if(c.phase==8){c.phase=9;result.cursor=c;result.need=ClearPrefixNeed::Store;return result;}
 if(c.phase==9){c.phase=10;result.cursor=c;result.need=ClearPrefixNeed::WriteCount;return result;}
 if(c.phase==10){c.phase=11;result.cursor=c;result.need=ClearPrefixNeed::WriteRegion;result.end=facts.word+4;return result;}
 if(c.phase==11){c.phase=12;result.cursor=c;result.need=ClearPrefixNeed::StageMap;result.boundary=1;return result;}
 result.need=ClearPrefixNeed::Output;return result;
}

extern "C" OrdinalStep ordinal_step(LocalCursor cursor,StorageView source,Slice arguments,OrdinalFacts facts) on cpu {
 OrdinalStep result=OrdinalStep(cursor,OrdinalNeed::BadCursor,Text(0,Span(0u64,0u64)),0,0);
 if(cursor.phase==0){
  if(arguments.count!=1u64){result.need=OrdinalNeed::BadArguments;return result;}
  ExprNode argument=binding_node<ExprNode>(source.nodes,binding_member(source.nodes,arguments,0u64));
  result.need=OrdinalNeed::BadName;
  match(argument.kind){
   ExprKind::Name(value){result.cursor.phase=1;result.need=OrdinalNeed::Binding;result.name=value;}
   ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}ExprKind::Member(unused){}ExprKind::Index(unused){}
   ExprKind::Call(unused){}ExprKind::Invoke(unused){}ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}
   ExprKind::Cooperate(unused){}ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  return result;
 }
 if(facts.binding==2){
  if(facts.place==1){result.need=OrdinalNeed::Application;result.minimum=facts.minimum;result.maximum=facts.maximum;}
  if(facts.place==7){result.need=OrdinalNeed::Dataset;}
  if(facts.place==4){result.need=OrdinalNeed::Tile;}
 }
 if(facts.binding==5&&facts.place==3){result.need=OrdinalNeed::View;result.maximum=facts.maximum==0?0:facts.maximum-1;}
 return result;
}

u32 record_properties(u32 flags,u32 uniform,u32 constant) on cpu {u32 result=flags;if(uniform==0){result=result&2;}if(constant==0){result=result&1;}return result;}
extern "C" RecordConstructStep record_construct_step(Cursor cursor,RecordConstructFacts facts) on cpu {
 Cursor c=cursor;
 while(true){
  if(c.phase==0){
   if(c.index<facts.arguments){c.phase=1;return RecordConstructStep(c,RecordConstructNeed::Argument,facts.array!=0?1:(c.index<facts.fields?2:0));}
   c.phase=4;return RecordConstructStep(c,RecordConstructNeed::ReleaseBegin,0);
  }
  if(c.phase==1){if(facts.present==0){return RecordConstructStep(c,RecordConstructNeed::Empty,0);}if(facts.borrows!=0){c.phase=2;return RecordConstructStep(c,RecordConstructNeed::Hold,0);}c.phase=3;return RecordConstructStep(c,RecordConstructNeed::Push,0);}
  if(c.phase==2){c.phase=3;return RecordConstructStep(c,RecordConstructNeed::Push,0);}
  if(c.phase==3){c.index+=1u64;c.phase=0;}
  if(c.phase==4){c.index=0u64;c.phase=5;}
  if(c.phase==5){if(c.index<facts.held){c.phase=6;return RecordConstructStep(c,RecordConstructNeed::Release,0);}c.phase=7;}
  if(c.phase==6){c.index+=1u64;c.phase=5;}
  if(c.phase==7){
   if(facts.owns!=0){return RecordConstructStep(c,RecordConstructNeed::Owned,0);}
   if(facts.variants!=0){return RecordConstructStep(c,RecordConstructNeed::Variant,0);}
   if(facts.enumeration!=0){if(facts.arguments!=1u64){return RecordConstructStep(c,RecordConstructNeed::BadEnum,0);}c.phase=8;return RecordConstructStep(c,RecordConstructNeed::EnumValue,0);}
   if(facts.matrix!=0){return RecordConstructStep(c,RecordConstructNeed::Matrix,0);}
   if(facts.c_function!=0||facts.numeric!=0){return RecordConstructStep(c,RecordConstructNeed::Numeric,0);}
   if(facts.array==0){return RecordConstructStep(c,RecordConstructNeed::Record,0);}
   if(facts.arguments==1u64){c.phase=10;return RecordConstructStep(c,RecordConstructNeed::SameArray,0);}c.phase=11;c.index=0u64;c.flags=3;
  }
  if(c.phase==8){c.phase=9;return RecordConstructStep(c,facts.same!=0?RecordConstructNeed::EnumReturn:RecordConstructNeed::EnumType,0);}
  if(c.phase==9){return RecordConstructStep(c,RecordConstructNeed::EnumReturn,0);}
  if(c.phase==10){if(facts.same!=0){return RecordConstructStep(c,RecordConstructNeed::Single,0);}c.phase=11;c.index=0u64;c.flags=3;}
  if(c.phase==11){
   if(c.index<facts.arguments){c.phase=12;return RecordConstructStep(c,RecordConstructNeed::ArrayType,0);}
   if(facts.arguments!=1u64&&facts.arguments!=facts.elements){return RecordConstructStep(c,RecordConstructNeed::BadArray,0);}
   if(facts.empty!=0){if(facts.elements==0u64){c.flags=3;}return RecordConstructStep(c,RecordConstructNeed::EmptyArray,0);}
   c.phase=13;c.index=0u64;c.part=0u64;return RecordConstructStep(c,RecordConstructNeed::ArrayBegin,0);
  }
  if(c.phase==12){c.flags=record_properties(c.flags,facts.uniform,facts.constant);c.index+=1u64;c.phase=11;}
  if(c.phase==13){if(c.part<facts.elements){c.phase=14;return RecordConstructStep(c,RecordConstructNeed::ArrayElement,0);}return RecordConstructStep(c,RecordConstructNeed::ArrayValue,0);}
  if(c.phase==14){c.part+=1u64;c.index+=1u64;if(c.index==facts.arguments){c.index=0u64;}c.phase=13;}
 }
}
extern "C" RecordPackStep record_pack_step(Cursor cursor,RecordPackFacts facts) on cpu {
 Cursor c=cursor;
 if(c.phase==0){if(facts.values!=facts.fields){return RecordPackStep(c,RecordPackNeed::BadCount);}c.flags=3;c.phase=1;}
 if(c.phase==3){c.index+=1u64;c.phase=1;}
 if(c.phase==1){if(c.index<facts.values){c.phase=2;return RecordPackStep(c,RecordPackNeed::Field);}return RecordPackStep(c,RecordPackNeed::Value);}
 c.flags=record_properties(c.flags,facts.uniform,facts.constant);c.phase=3;return RecordPackStep(c,RecordPackNeed::Append);
}

extern "C" u32 dataset_layout(u64 count,address context,cfn<u32,address,u64> part) on cpu requires nonzero(part) {
 u32 offset=0;u64 index=0u64;while(index<count){offset+=part(context,index);index+=1u64;}return offset;
}
extern "C" DatasetProjection dataset_projection(SignatureNode source,DatasetProjectionUse usage,DatasetProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.declared) requires nonzero(host.field) requires nonzero(host.select) {
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 DatasetProjection result=DatasetProjection(usage==DatasetProjectionUse::Field?DatasetProjectionKind::BadRoot:DatasetProjectionKind::Missing,0,0u64,0u64,expression.span);
 match(expression.kind){
  ExprKind::Name(name){result=host.root(host.context,name,usage==DatasetProjectionUse::Reference?1:0);result.span=expression.span;if(result.kind==DatasetProjectionKind::Missing&&usage==DatasetProjectionUse::Field){result.kind=DatasetProjectionKind::BadRoot;}return result;}
  ExprKind::Member(member){
   result=dataset_projection(SignatureNode(source.storage,member.base.offset),usage,host);if(result.kind!=DatasetProjectionKind::Found){return result;}
   DatasetField field=DatasetField(0,0,0u64,SeekName(unsafe_address(0u64),0u64));u64 index=0u64;
   if(usage==DatasetProjectionUse::Field){field=host.field(host.context,member.field);}else{
    while(index<result.fields){field=host.declared(host.context,index);if(binding_name(source.storage.source,source.storage.text,member.field,field.name.data,field.name.length)){break;}field.present=0;index+=1u64;}
   }
   if(field.present==0){result.kind=DatasetProjectionKind::BadField;result.span=expression.span;return result;}
   if(usage!=DatasetProjectionUse::Field){host.select(host.context,index,usage==DatasetProjectionUse::Reference?1:0);}result.offset+=field.offset;result.fields=field.fields;return result;
  }
  ExprKind::Number(unused){}ExprKind::StaticU32(unused){}ExprKind::Bool(unused){}ExprKind::Index(unused){}ExprKind::Call(unused){}ExprKind::Invoke(unused){}ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}ExprKind::Cooperate(unused){}ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
 }return result;
}
extern "C" u32 dataset_element_words(u32 base,u64 count,u32 access,u32 requested,address context,cfn<u32,address,u64> ensures memory(read) component,cfn<void,address,u32> append) on cpu requires nonzero(component) requires nonzero(append) {
 u64 index=0u64;while(index<count){u32 word=component(context,index);append(context,base+word);index+=1u64;}return requested!=0&&(access==2||access==3||access==4)?1:0;
}

extern "C" StageRootStep stage_root_step(LocalCursor cursor,SignatureNode source,StageRootFacts facts) on cpu {
 StageRootStep result=StageRootStep(cursor,StageRootNeed::Unknown,Text(0,Span(0u64,0u64)),0);
 if(cursor.phase==0){
  ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
  result.need=StageRootNeed::Expression;
  match(expression.kind){
   ExprKind::Name(name){result.cursor.phase=1;result.need=StageRootNeed::Binding;result.spelling=name;}
   ExprKind::Number(number){result.need=StageRootNeed::Number;result.spelling=number;}
   ExprKind::StaticU32(bits){result.need=StageRootNeed::StaticU32;result.bits=bits;}
   ExprKind::Bool(unused){}ExprKind::Member(unused){}ExprKind::Index(unused){}ExprKind::Call(unused){}ExprKind::Invoke(unused){}
   ExprKind::Construct(unused){}ExprKind::Specialize(unused){}ExprKind::FunctionRef(unused){}ExprKind::Cooperate(unused){}
   ExprKind::Target(unused){}ExprKind::Unary(unused){}ExprKind::Binary(unused){}ExprKind::Select(unused){}
  }
  return result;
 }
 if(cursor.phase==1){result.cursor.phase=2;result.need=facts.present!=0?StageRootNeed::Bound:StageRootNeed::Scope;return result;}
 if(cursor.phase==2){result.cursor.phase=3;result.need=facts.present!=0?StageRootNeed::ScopeValue:StageRootNeed::Resolve;return result;}
 if(cursor.phase==3){if(facts.present!=0){result.cursor.phase=4;result.need=StageRootNeed::FunctionKind;}return result;}
 if(facts.constant!=0){result.need=StageRootNeed::Constant;}return result;
}

extern "C" StageControlStep stage_control_step(StageControlCursor state,SignatureNode source,StageControlFacts facts) on cpu {
 StageControlStep result=StageControlStep(state,StageControlNeed::Complete,Text(0,Span(0u64,0u64)),0u64,Slice(0u64,0u64));
 StmtNode statement=binding_node<StmtNode>(source.storage.nodes,source.offset);
 match(statement.kind){
  StmtKind::Let(value){
   result.name=value.name;
   if(state.phase==0){result.cursor.phase=1;result.need=StageControlNeed::Name;return result;}
   if(state.phase==1){if(facts.present!=0){result.need=StageControlNeed::BadLetName;return result;}if(value.annotation.present!=0){result.cursor.phase=2;result.expression=value.annotation.value.offset;result.need=StageControlNeed::Annotation;return result;}}
   if(state.phase==1||state.phase==2){result.cursor.phase=3;result.expression=value.value.offset;result.need=StageControlNeed::LetValue;return result;}
   if(state.phase==3){result.cursor.phase=4;result.need=StageControlNeed::Bind;return result;}
  }
  StmtKind::If(value){
   result.expression=value.condition.offset;
   if(state.phase==0){result.cursor.phase=10;result.need=StageControlNeed::Condition;return result;}
   if(state.phase==10){if(facts.literal==0){result.need=StageControlNeed::BadCondition;return result;}if(facts.same_type==0){result.need=StageControlNeed::BadBoolean;return result;}result.cursor.phase=11;result.need=StageControlNeed::ConditionValue;return result;}
   if(state.phase==11){result.cursor.phase=12;result.need=StageControlNeed::IfBody;result.body=facts.bits==0?value.no:value.yes;return result;}
   if(state.phase==12){result.cursor.phase=13;result.need=StageControlNeed::IfOrigin;return result;}
   if(facts.returned!=0){result.need=StageControlNeed::Return;}
  }
  StmtKind::For(value){
   result.name=value.name;result.body=value.body;result.expression=value.start.offset;
   if(value.end.present==0){result.need=StageControlNeed::Invalid;return result;}
   if(state.phase==0){result.cursor.phase=20;result.need=StageControlNeed::Name;return result;}
   if(state.phase==20){if(facts.present!=0){result.need=StageControlNeed::BadCounterName;return result;}result.cursor.phase=21;result.need=StageControlNeed::First;return result;}
   if(state.phase==21){if(facts.literal==0||facts.same_type==0){result.need=StageControlNeed::BadBound;return result;}result.cursor.phase=22;result.need=StageControlNeed::FirstValue;return result;}
   if(state.phase==22){result.cursor.index=facts.bits;result.cursor.phase=23;result.expression=value.end.value.offset;result.need=StageControlNeed::Last;return result;}
   if(state.phase==23||state.phase==24){result.expression=value.end.value.offset;}
   if(state.phase==23){if(facts.literal==0||facts.same_type==0){result.need=StageControlNeed::BadBound;return result;}result.cursor.phase=24;result.need=StageControlNeed::LastValue;return result;}
   if(state.phase==24){result.cursor.last=facts.bits;result.cursor.phase=25;result.need=StageControlNeed::BeginFor;return result;}
   if(state.phase==26){result.cursor.phase=27;result.need=StageControlNeed::IterationOrigin;return result;}
   if(state.phase==27){if(facts.returned!=0){result.cursor.phase=29;result.need=StageControlNeed::ForOrigin;return result;}result.cursor.index+=1;result.cursor.phase=25;}
   if(result.cursor.phase==25){if(result.cursor.index<result.cursor.last){result.cursor.phase=26;result.need=StageControlNeed::Iteration;return result;}result.cursor.phase=29;result.need=StageControlNeed::ForOrigin;return result;}
   if(facts.returned!=0){result.need=StageControlNeed::Return;}
  }
  StmtKind::Return(value){result.need=value.present==0?StageControlNeed::Return:StageControlNeed::Invalid;}
  StmtKind::Phase(){if(state.phase==0){result.cursor.phase=40;result.need=StageControlNeed::Boundary;}}
  StmtKind::Parallel(value){result.need=StageControlNeed::Parallel;result.name=value.name;result.expression=value.domain.offset;result.body=value.body;}
  StmtKind::Expr(value){result.need=StageControlNeed::Expression;result.expression=value.offset;}
  StmtKind::Declare(value){result.need=StageControlNeed::Invalid;}
  StmtKind::Assign(value){result.need=StageControlNeed::Invalid;}
  StmtKind::While(value){result.need=StageControlNeed::Invalid;}
  StmtKind::Break(){result.need=StageControlNeed::Invalid;}
  StmtKind::Continue(){result.need=StageControlNeed::Invalid;}
  StmtKind::Match(value){result.need=StageControlNeed::Invalid;}
  StmtKind::Seek(value){result.need=StageControlNeed::Invalid;}
 }
 return result;
}

extern "C" RootEnvironmentStep root_environment_step(EnvironmentCursor state,RootEnvironmentFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(cursor.index>=facts.roots){return RootEnvironmentStep(cursor,RootEnvironmentNeed::Complete);}cursor.phase=1;return RootEnvironmentStep(cursor,RootEnvironmentNeed::Entry);}
  if(cursor.phase==1){
   cursor.phase=9;
   if(facts.kind==0){return RootEnvironmentStep(cursor,RootEnvironmentNeed::Texture);}
   if(facts.kind==1){return RootEnvironmentStep(cursor,RootEnvironmentNeed::Acceleration);}
   if(facts.kind==2){return RootEnvironmentStep(cursor,RootEnvironmentNeed::Data);}
   if(facts.kind==4){return RootEnvironmentStep(cursor,RootEnvironmentNeed::Literal);}
   cursor.part=0u64;cursor.phase=2;return RootEnvironmentStep(cursor,RootEnvironmentNeed::BeginUniform);
  }
  if(cursor.phase==3){cursor.part+=1u64;cursor.phase=2;}
  if(cursor.phase==2){if(cursor.part<facts.components){cursor.phase=3;return RootEnvironmentStep(cursor,RootEnvironmentNeed::UniformComponent);}cursor.phase=9;return RootEnvironmentStep(cursor,RootEnvironmentNeed::BindUniform);}
  if(cursor.phase==9){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" ParallelPrepareStep parallel_prepare_step(LocalCursor state,SignatureNode source,ParallelPrepareFacts facts) on cpu {
 ParallelPrepareStep result=ParallelPrepareStep(state,ParallelPrepareNeed::Complete,Text(0,Span(0u64,0u64)),0u64);
 ParallelNode syntax=binding_node<ParallelNode>(source.storage.nodes,source.offset);result.spelling=syntax.dataset;
 if(state.phase==0){result.cursor.phase=1;result.need=ParallelPrepareNeed::Root;return result;}
 if(state.phase==1){if(facts.present==0){result.need=ParallelPrepareNeed::BadRoot;return result;}result.cursor.phase=2;if(syntax.count.present!=0){result.need=ParallelPrepareNeed::Prefix;result.expression=syntax.count.value.offset;return result;}}
 if(result.cursor.phase==2){if(syntax.selection.present!=0){result.cursor.phase=3;result.need=ParallelPrepareNeed::Selection;result.expression=syntax.selection.value.offset;return result;}result.cursor.phase=5;result.need=ParallelPrepareNeed::Builder;return result;}
 if(state.phase==3||state.phase==4||state.phase==41){result.expression=syntax.selection.value.offset;}
 if(state.phase==3){
  if(facts.access!=1&&facts.access!=3){if(facts.access==2){result.cursor.phase=4;result.need=ParallelPrepareNeed::Initialized;return result;}result.need=ParallelPrepareNeed::BadSelection;return result;}
  result.cursor.phase=41;result.need=ParallelPrepareNeed::Counts;return result;
 }
 if(state.phase==4){if(facts.initialized==0){result.need=ParallelPrepareNeed::BadSelection;return result;}result.cursor.phase=41;result.need=ParallelPrepareNeed::Counts;return result;}
 if(state.phase==41){if(facts.known_domain!=0&&facts.known_selection!=0&&facts.selection_count<facts.domain_count){result.need=ParallelPrepareNeed::BadCapacity;return result;}result.cursor.phase=5;result.need=ParallelPrepareNeed::Builder;return result;}
 if(state.phase==5){result.cursor.phase=6;if(facts.cpu!=0){result.need=ParallelPrepareNeed::Frame;return result;}}
 if(result.cursor.phase==6){result.cursor.phase=7;result.need=ParallelPrepareNeed::Context;return result;}
 if(state.phase==7){result.cursor.phase=8;result.need=ParallelPrepareNeed::Roots;return result;}
 if(state.phase==8){result.cursor.phase=9;result.need=ParallelPrepareNeed::Index;return result;}
 if(state.phase==9){result.cursor.phase=10;result.need=ParallelPrepareNeed::Refine;return result;}
 if(state.phase==10){result.cursor.phase=11;result.need=ParallelPrepareNeed::Name;return result;}
 if(state.phase==11){if(facts.present!=0){result.need=ParallelPrepareNeed::BadName;return result;}result.cursor.phase=12;result.need=ParallelPrepareNeed::Initialize;return result;}
 return result;
}
extern "C" ParallelInitializeStep parallel_initialize_step(EnvironmentCursor state,ParallelInitializeFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){cursor.phase=1;u32 flags=(facts.domain_access==2||facts.domain_access==3?1:0)+(facts.domain_access!=4?2:0);return ParallelInitializeStep(cursor,ParallelInitializeNeed::Element,flags);}
  if(cursor.phase==1){if(cursor.index<facts.views){cursor.phase=2;return ParallelInitializeStep(cursor,ParallelInitializeNeed::View,0);}cursor.phase=10;return ParallelInitializeStep(cursor,ParallelInitializeNeed::BindElement,0);}
  if(cursor.phase==2){cursor.part=0u64;cursor.phase=3;}
  if(cursor.phase==3){if(cursor.part<facts.components){cursor.phase=4;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Component,0);}cursor.index+=1u64;cursor.phase=1;}
  if(cursor.phase==4){if(facts.access==1||facts.access==3||facts.access==4){cursor.phase=6;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Mark,0);}cursor.phase=5;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Initialized,0);}
  if(cursor.phase==5){if(facts.present!=0){cursor.phase=6;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Mark,0);}cursor.part+=1u64;cursor.phase=3;}
  if(cursor.phase==6){cursor.part+=1u64;cursor.phase=3;}
  if(cursor.phase==10){cursor.phase=12;cursor.index=0u64;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Reset,0);}
  if(cursor.phase==12){if(cursor.index<facts.views){cursor.phase=13;return ParallelInitializeStep(cursor,ParallelInitializeNeed::AliasView,0);}return ParallelInitializeStep(cursor,ParallelInitializeNeed::Complete,0);}
  if(cursor.phase==13){if(facts.binding!=facts.domain&&(facts.access==2||facts.access==3)){cursor.phase=14;return ParallelInitializeStep(cursor,ParallelInitializeNeed::SameLength,0);}cursor.index+=1u64;cursor.phase=12;}
  if(cursor.phase==14){if(facts.present!=0){cursor.phase=15;return ParallelInitializeStep(cursor,ParallelInitializeNeed::Alias,3);}cursor.index+=1u64;cursor.phase=12;}
  if(cursor.phase==15){cursor.index+=1u64;cursor.phase=12;}
 }
}

extern "C" ParallelOutputStep parallel_output_step(EnvironmentCursor state,ParallelOutputFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){
   if(facts.access==4){cursor.phase=1;return ParallelOutputStep(cursor,ParallelOutputNeed::Old);}
   if(facts.prefix!=0||facts.selection!=0){return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}
   cursor.phase=10;return ParallelOutputStep(cursor,ParallelOutputNeed::Appends);
  }
  if(cursor.phase==1){if(facts.present==0){return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}cursor.phase=2;return ParallelOutputStep(cursor,ParallelOutputNeed::Atomic);}
  if(cursor.phase==2){if(cursor.index<facts.components){cursor.phase=3;return ParallelOutputStep(cursor,ParallelOutputNeed::Component);}return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}
  if(cursor.phase==3){cursor.part=0u64;cursor.phase=4;}
  if(cursor.phase==4){if(cursor.part<facts.regions){cursor.phase=5;return ParallelOutputStep(cursor,ParallelOutputNeed::Region);}cursor.phase=7;return ParallelOutputStep(cursor,ParallelOutputNeed::Assign);}
  if(cursor.phase==5){
   if(u64(facts.offset)<u64(facts.end)&&u64(facts.start)<u64(facts.offset)+u64(facts.bytes)){
    cursor.phase=6;
    return ParallelOutputStep(cursor,facts.start==facts.offset&&u64(facts.end)==u64(facts.offset)+u64(facts.bytes)&&facts.stored!=0?ParallelOutputNeed::Join:ParallelOutputNeed::Forget);
   }
   cursor.part+=1u64;cursor.phase=4;
  }
  if(cursor.phase==6){cursor.part+=1u64;cursor.phase=4;}
  if(cursor.phase==7){cursor.index+=1u64;cursor.phase=2;}
  if(cursor.phase==10){if(cursor.index<facts.items){cursor.phase=11;return ParallelOutputStep(cursor,ParallelOutputNeed::Append);}cursor.index=0u64;cursor.phase=20;return ParallelOutputStep(cursor,ParallelOutputNeed::Elements);}
  if(cursor.phase==11){if(facts.append_output==facts.binding||facts.append_count==facts.binding){return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}cursor.index+=1u64;cursor.phase=10;}
  if(cursor.phase==20){if(cursor.index<facts.items){cursor.phase=21;return ParallelOutputStep(cursor,ParallelOutputNeed::Element);}return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}
  if(cursor.phase==21){if(facts.present!=0&&facts.source_binding==facts.binding&&facts.source_index==facts.expected_index){cursor.phase=22;return ParallelOutputStep(cursor,ParallelOutputNeed::Type);}cursor.index+=1u64;cursor.phase=20;}
  if(cursor.phase==22){if(facts.same_type!=0){cursor.phase=23;return ParallelOutputStep(cursor,ParallelOutputNeed::Proof);}cursor.index+=1u64;cursor.phase=20;}
  if(cursor.phase==23){return ParallelOutputStep(cursor,ParallelOutputNeed::Complete);}
 }
}
extern "C" ParallelEffectsStep parallel_effects_step(EnvironmentCursor state,ParallelEffectsFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(cursor.index<facts.writes){cursor.phase=1;return ParallelEffectsStep(cursor,ParallelEffectsNeed::Binding);}cursor.index=0u64;cursor.phase=10;}
  if(cursor.phase==1){cursor.phase=2;return ParallelEffectsStep(cursor,ParallelEffectsNeed::Output);}
  if(cursor.phase==2){cursor.part=0u64;cursor.phase=3;}
  if(cursor.phase==3){if(cursor.part<facts.regions){cursor.phase=4;return ParallelEffectsStep(cursor,ParallelEffectsNeed::Written);}cursor.phase=6;if(facts.output!=0){return ParallelEffectsStep(cursor,ParallelEffectsNeed::Elements);}}
  if(cursor.phase==4){cursor.part+=1u64;cursor.phase=3;}
  if(cursor.phase==6){cursor.index+=1u64;cursor.phase=0;}
  if(cursor.phase==10){if(cursor.index<facts.guarantees){cursor.phase=11;return ParallelEffectsStep(cursor,ParallelEffectsNeed::Grant);}return ParallelEffectsStep(cursor,ParallelEffectsNeed::Complete);}
  if(cursor.phase==11){cursor.index+=1u64;cursor.phase=10;}
 }
}
extern "C" u32 parallel_written_overlap(u32 offset,u32 bytes,u64 count,address context,cfn<WrittenRegion,address> next) on cpu requires nonzero(next) {
 u64 index=0u64;
 while(index<count){WrittenRegion region=next(context);if(u64(offset)<u64(region.end)&&u64(region.start)<u64(offset)+u64(bytes)){return 1;}index+=1u64;}
 return 0;
}
extern "C" u32 stage_initialized(u32 known,u32 count,u64 binding,u32 word,u64 outputs,address context,cfn<StageKnownOutput,address,u64> output,cfn<u32,address,u64,u64> field,cfn<u32,address,u64,u64> length) on cpu requires nonzero(output) requires nonzero(field) requires nonzero(length) {
 if(known!=0&&count==0){return 1;}
 u64 index=0u64;
 while(index<outputs){
  StageKnownOutput item=output(context,index);
  if(item.binding==binding){
   u64 part=0u64;
   while(part<item.words){
    if(field(context,index,part)==word){
     if(item.has_domain!=0){if(length(context,item.domain,binding)!=0){return 1;}}
     else{if(known!=0&&count==1){return 1;}}
     break;
    }
    part+=1u64;
   }
  }
  index+=1u64;
 }
 return 0;
}
extern "C" PromoteStageStep promote_stage_step(EnvironmentCursor state,PromoteStageFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(cursor.index<facts.roots){cursor.phase=1;return PromoteStageStep(cursor,PromoteStageNeed::Root);}return PromoteStageStep(cursor,PromoteStageNeed::Complete);}
  if(cursor.phase==1){if(facts.access==2){cursor.part=0u64;cursor.phase=2;}else{cursor.index+=1u64;cursor.phase=0;}}
  if(cursor.phase==2){if(cursor.part<facts.components){cursor.phase=3;return PromoteStageStep(cursor,PromoteStageNeed::Component);}cursor.phase=4;return PromoteStageStep(cursor,PromoteStageNeed::Promote);}
  if(cursor.phase==3){if(facts.initialized!=0){cursor.part+=1u64;cursor.phase=2;}else{cursor.index+=1u64;cursor.phase=0;}}
  if(cursor.phase==4){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" StageWriteInitStep stage_write_init_step(EnvironmentCursor state,StageWriteInitFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(facts.prefix!=0||facts.selection!=0){return StageWriteInitStep(cursor,StageWriteInitNeed::Complete);}if(cursor.index<facts.writes){cursor.phase=1;return StageWriteInitStep(cursor,StageWriteInitNeed::Binding);}return StageWriteInitStep(cursor,StageWriteInitNeed::Complete);}
  if(cursor.phase==1){if(facts.access==2){cursor.part=0u64;cursor.phase=2;}else{cursor.index+=1u64;cursor.phase=0;}}
  if(cursor.phase==2){if(cursor.part<facts.appends){cursor.phase=3;return StageWriteInitStep(cursor,StageWriteInitNeed::Append);}cursor.part=0u64;cursor.phase=5;}
  if(cursor.phase==3){if(facts.append_output==facts.binding||facts.append_count==facts.binding){cursor.index+=1u64;cursor.phase=0;}else{cursor.part+=1u64;cursor.phase=2;}}
  if(cursor.phase==5){if(cursor.part<facts.components){cursor.phase=6;return StageWriteInitStep(cursor,StageWriteInitNeed::Component);}cursor.phase=9;if(facts.selected!=0){return StageWriteInitStep(cursor,StageWriteInitNeed::Emit);}}
  if(cursor.phase==6){if(facts.local!=0){cursor.phase=7;return StageWriteInitStep(cursor,StageWriteInitNeed::Known);}cursor.part+=1u64;cursor.phase=5;}
  if(cursor.phase==7){cursor.phase=8;if(facts.known==0){return StageWriteInitStep(cursor,StageWriteInitNeed::Word);}}
  if(cursor.phase==8){cursor.part+=1u64;cursor.phase=5;}
  if(cursor.phase==9){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" CollectiveInitStep collective_init_step(EnvironmentCursor state,CollectiveInitFacts facts) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(cursor.index<facts.stages){cursor.phase=1;return CollectiveInitStep(cursor,CollectiveInitNeed::Stage,0,0u64,0u64,0);}return CollectiveInitStep(cursor,CollectiveInitNeed::Complete,0,0u64,0u64,0);}
  if(cursor.phase==1){
   if(facts.kind==1||facts.kind==2||facts.kind==6||facts.kind==7||facts.kind==8||((facts.kind==3||facts.kind==4||facts.kind==5)&&facts.prefix==0)){cursor.phase=2;return CollectiveInitStep(cursor,CollectiveInitNeed::Descriptor,0,0u64,0u64,0);}
   cursor.index+=1u64;cursor.phase=0;
  }
  if(cursor.phase==2){if(facts.kind==6||facts.kind==7||facts.kind==8){cursor.phase=4;return CollectiveInitStep(cursor,CollectiveInitNeed::Word,0,0u64,0u64,facts.count_word);}cursor.part=0u64;cursor.phase=5;}
  if(cursor.phase==5){if(cursor.part<facts.components){cursor.phase=6;return CollectiveInitStep(cursor,CollectiveInitNeed::Component,0,0u64,0u64,0);}cursor.phase=4;}
  if(cursor.phase==6){cursor.phase=7;return CollectiveInitStep(cursor,CollectiveInitNeed::Word,0,0u64,0u64,facts.target_word+facts.offset);}
  if(cursor.phase==7){cursor.part+=1u64;cursor.phase=5;}
  if(cursor.phase==4){
   cursor.phase=8;
   if(facts.kind==6||facts.kind==7||facts.kind==8){return CollectiveInitStep(cursor,CollectiveInitNeed::Emit,0,facts.count,0u64,0);}
   if(facts.kind==3||facts.kind==4||facts.kind==5){return CollectiveInitStep(cursor,CollectiveInitNeed::Emit,1,facts.target,facts.source,0);}
   return CollectiveInitStep(cursor,CollectiveInitNeed::Emit,0,facts.target,0u64,0);
  }
  if(cursor.phase==8){cursor.index+=1u64;cursor.phase=0;}
 }
}
extern "C" ParallelBuildStep parallel_build_step(LocalCursor state,ParallelBuildFacts facts) on cpu {
 LocalCursor cursor=state;
 while(true){
  if(cursor.phase==0){cursor.phase=1;if(facts.prefix!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::Prefix,0);}}
  if(cursor.phase==1){cursor.phase=2;return ParallelBuildStep(cursor,ParallelBuildNeed::SetPrefix,0);}
  if(cursor.phase==2){cursor.phase=3;return ParallelBuildStep(cursor,ParallelBuildNeed::TileInputs,0);}
  if(cursor.phase==3){cursor.phase=4;return ParallelBuildStep(cursor,ParallelBuildNeed::Body,0);}
  if(cursor.phase==4){cursor.phase=5;return ParallelBuildStep(cursor,ParallelBuildNeed::Access,0);}
  if(cursor.phase==5){cursor.phase=6;return ParallelBuildStep(cursor,ParallelBuildNeed::Appends,0);}
  if(cursor.phase==6){cursor.phase=7;return ParallelBuildStep(cursor,ParallelBuildNeed::Initialize,0);}
  if(cursor.phase==7){cursor.phase=8;return ParallelBuildStep(cursor,ParallelBuildNeed::Tiles,0);}
  if(cursor.phase==8){
   if(facts.selection!=0&&facts.tiles!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::BadSparseTile,0);}
   cursor.phase=9;if(facts.selection!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::Selection,0);}
  }
  if(cursor.phase==9){cursor.phase=10;return ParallelBuildStep(cursor,ParallelBuildNeed::Frame,0);}
  if(cursor.phase==10){cursor.phase=11;if(facts.frame!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::FrameUse,0);}}
  if(cursor.phase==11){cursor.phase=12;if(facts.used!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::BindFrame,facts.has_native!=0?facts.native_frame:(facts.uniform_bytes+7)&4294967288);}}
  if(cursor.phase==12){cursor.phase=13;if(facts.used==0){return ParallelBuildStep(cursor,ParallelBuildNeed::Scope,0);}}
  if(cursor.phase==13){cursor.phase=20;return ParallelBuildStep(cursor,ParallelBuildNeed::Seal,0);}
  if(cursor.phase==20){cursor.phase=21;if(facts.used!=0||facts.scope!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::BindScope,0);}}
  if(cursor.phase==21){cursor.phase=22;if(facts.used!=0){return ParallelBuildStep(cursor,ParallelBuildNeed::CommitFrame,0);}}
  if(cursor.phase==22){cursor.phase=23;return ParallelBuildStep(cursor,ParallelBuildNeed::Facts,0);}
  if(cursor.phase==23){cursor.phase=24;return ParallelBuildStep(cursor,ParallelBuildNeed::Publish,0);}
  return ParallelBuildStep(cursor,ParallelBuildNeed::Complete,0);
 }
}
extern "C" ParallelRelationsStep parallel_relations_step(EnvironmentCursor state,u64 relations) on cpu {
 EnvironmentCursor cursor=state;
 while(true){
  if(cursor.phase==0){if(cursor.index<relations){cursor.phase=1;return ParallelRelationsStep(cursor,ParallelRelationsNeed::Relation);}cursor.index=0u64;cursor.phase=2;}
  if(cursor.phase==1){cursor.index+=1u64;cursor.phase=0;}
  if(cursor.phase==2){if(cursor.index<relations){cursor.phase=3;return ParallelRelationsStep(cursor,ParallelRelationsNeed::Prove);}return ParallelRelationsStep(cursor,ParallelRelationsNeed::Complete);}
  if(cursor.phase==3){cursor.index+=1u64;cursor.phase=2;}
 }
}
extern "C" StageSequenceStep stage_sequence_step(LocalCursor state,StageSequenceFacts facts) on cpu {
 LocalCursor cursor=state;
 while(true){
  if(cursor.phase==0){cursor.phase=1;return StageSequenceStep(cursor,StageSequenceNeed::Next);}
  if(cursor.phase==1){if(facts.present==0){return StageSequenceStep(cursor,StageSequenceNeed::Complete);}cursor.phase=2;return StageSequenceStep(cursor,StageSequenceNeed::Begin);}
  if(cursor.phase==2){cursor.phase=3;return StageSequenceStep(cursor,StageSequenceNeed::Dispatch);}
  if(cursor.phase==3){if(facts.returned!=0){return StageSequenceStep(cursor,StageSequenceNeed::Complete);}cursor.phase=4;return StageSequenceStep(cursor,StageSequenceNeed::Outputs);}
  if(cursor.phase==4){cursor.phase=0;}
 }
}
extern "C" StageExpressionStep stage_expression_step(StageExpressionCursor state,SignatureNode source,StageExpressionFacts facts) on cpu {
 StageExpressionStep result=StageExpressionStep(state,StageExpressionNeed::Complete,Text(0,Span(0u64,0u64)),Slice(0u64,0u64),Slice(0u64,0u64),CollectiveOperator::Sum);
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
  ExprKind::Call(value){result.name=value.name;result.arguments=value.arguments;}
  ExprKind::Specialize(value){result.name=value.name;result.arguments=value.arguments;result.types=value.types;result.cursor.flags=result.cursor.flags|1;}
  ExprKind::Number(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::StaticU32(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Bool(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Name(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Member(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Index(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Invoke(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Construct(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::FunctionRef(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Cooperate(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Target(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Unary(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Binary(unused){result.need=StageExpressionNeed::Invalid;return result;}
  ExprKind::Select(unused){result.need=StageExpressionNeed::Invalid;return result;}
 }
 if(state.phase==0){result.cursor.phase=1;result.need=StageExpressionNeed::Resolve;return result;}
 if(state.phase==1){
  if(facts.missing!=0&&(result.cursor.flags&1)==0){
   address bytes=binding_at(result.name.kind==0?source.storage.source:source.storage.text,result.name.range.start);u64 length=result.name.range.end-result.name.range.start;
   if(local_spelling(bytes,length,"clear_prefix")){result.cursor.phase=2;result.need=StageExpressionNeed::BeginSegment;return result;}
   if(local_spelling(bytes,length,"stable_sort_by_key")){result.cursor.phase=3;result.need=StageExpressionNeed::BeginSegment;return result;}
   if(local_spelling(bytes,length,"compact")){result.cursor.phase=4;result.need=StageExpressionNeed::BeginSegment;return result;}
   CollectiveBuiltin builtin=source_collective(bytes,usize(length));result.operation=builtin.operation;result.cursor.phase=20;
   if(builtin.form==CollectiveForm::Reduce){result.need=StageExpressionNeed::Reduce;return result;}
   if(builtin.form==CollectiveForm::SegmentedReduce){result.need=StageExpressionNeed::SegmentedReduce;return result;}
   if(builtin.form==CollectiveForm::Scan){result.need=StageExpressionNeed::Scan;return result;}
   if(builtin.form==CollectiveForm::SegmentedScan){result.need=StageExpressionNeed::SegmentedScan;return result;}
   u32 custom=source_custom_reduce(bytes,usize(length));
   if(custom!=0){result.cursor.custom=custom;result.cursor.phase=10;if(length>=4u64&&local_spelling(binding_at(bytes,length-4u64),4u64,"_map")){result.cursor.flags=result.cursor.flags|2;}result.need=StageExpressionNeed::BeginSegment;return result;}
  }
  if(facts.unique==0){result.need=StageExpressionNeed::Unknown;return result;}
  result.cursor.phase=30;result.need=StageExpressionNeed::CallPrepare;return result;
 }
 if(state.phase==2){result.cursor.phase=20;result.need=StageExpressionNeed::Clear;return result;}
 if(state.phase==3){result.cursor.phase=20;result.need=StageExpressionNeed::Sort;return result;}
 if(state.phase==4){result.cursor.phase=20;result.need=StageExpressionNeed::Compact;return result;}
 if(state.phase==10){
  result.cursor.phase=20;
  if(state.custom==1){result.need=StageExpressionNeed::CustomReduce;}
  if(state.custom==2){result.need=StageExpressionNeed::CustomScan;}
  if(state.custom==3){result.need=StageExpressionNeed::CustomSegmentedScan;}
  if(state.custom==4){result.need=StageExpressionNeed::CustomSegmentedReduce;}
  return result;
 }
 if(state.phase==20){result.cursor.phase=21;result.need=StageExpressionNeed::ResetBoundary;return result;}
 if(state.phase==30){result.cursor.phase=31;result.need=StageExpressionNeed::CallExecute;return result;}
 if(state.phase==31){result.cursor.phase=32;result.need=StageExpressionNeed::ResetFirst;return result;}
 return result;
}

extern "C" ValueKernelSource value_kernel_source(SignatureNode source) on cpu {
 ValueKernelSource result=ValueKernelSource(ValueKernelError::Valid,0,source.offset,Text(0,Span(0u64,0u64)),Slice(0u64,0u64),Slice(0u64,0u64));
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
  ExprKind::Call(value){
   address bytes=binding_at(value.name.kind==0?source.storage.source:source.storage.text,value.name.range.start);
   if(local_spelling(bytes,value.name.range.end-value.name.range.start,"bind")){
    if(value.arguments.count==0u64){result.error=ValueKernelError::Missing;return result;}
    result.reference=binding_member(source.storage.nodes,value.arguments,0u64);
    result.captures=Slice(value.arguments.offset+8u64,value.arguments.count-1u64);
   }
  }
  ExprKind::Number(unused){}
  ExprKind::StaticU32(unused){}
  ExprKind::Bool(unused){}
  ExprKind::Name(unused){}
  ExprKind::Member(unused){}
  ExprKind::Index(unused){}
  ExprKind::Invoke(unused){}
  ExprKind::Construct(unused){}
  ExprKind::Specialize(unused){}
  ExprKind::FunctionRef(unused){}
  ExprKind::Cooperate(unused){}
  ExprKind::Target(unused){}
  ExprKind::Unary(unused){}
  ExprKind::Binary(unused){}
  ExprKind::Select(unused){}

 }
 ExprNode reference=binding_node<ExprNode>(source.storage.nodes,result.reference);
 match(reference.kind){
  ExprKind::Name(value){result.name=value;}
  ExprKind::FunctionRef(value){result.name=value.name;result.types=value.types;result.explicit=1;}
  ExprKind::Number(unused){result.error=ValueKernelError::Reference;}
  ExprKind::StaticU32(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Bool(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Member(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Index(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Call(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Invoke(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Construct(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Specialize(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Cooperate(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Target(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Unary(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Binary(unused){result.error=ValueKernelError::Reference;}
  ExprKind::Select(unused){result.error=ValueKernelError::Reference;}

 }
 return result;
}
extern "C" u32 value_kernel_signature(SignatureNode declaration,u32 invocation,u64 parameters,u64 captures) on cpu {
 FunctionNode function=binding_node<FunctionNode>(declaration.storage.nodes,declaration.offset);
 if(invocation==0||function.kind!=0||function.target!=0||function.params.count!=parameters+captures){return 0;}
 u64 i=0u64;while(i<function.params.count){
  ParamNode parameter=binding_node<ParamNode>(declaration.storage.nodes,binding_member(declaration.storage.nodes,function.params,i));
  TypeNode type=binding_node<TypeNode>(declaration.storage.nodes,parameter.ty.offset);
  if(parameter.access!=0||type.dataset!=0||type.tile!=0||type.view!=0){return 0;}
  i+=1u64;
 }
 return 1;
}
extern "C" ValueKernelStep value_kernel_step(ValueCursor state,ValueKernelFacts facts) on cpu {
 ValueKernelStep result=ValueKernelStep(state,ValueKernelNeed::Publish,state.index);
 if(state.phase==0){result.cursor.phase=1;result.need=ValueKernelNeed::Resolve;return result;}
 if(state.phase==1){if(facts.unique==0){result.need=ValueKernelNeed::Unknown;return result;}result.cursor.phase=2;result.need=ValueKernelNeed::Signature;return result;}
 if(state.phase==2){if(facts.valid==0){result.need=ValueKernelNeed::InvalidSignature;return result;}result.cursor.phase=3;result.cursor.index=0u64;result.need=ValueKernelNeed::Begin;return result;}
 if(state.phase==3){if(state.index<facts.parameters){result.cursor.index+=1u64;result.need=ValueKernelNeed::Parameter;return result;}result.cursor.phase=4;result.cursor.index=0u64;result.need=ValueKernelNeed::BeginCaptures;return result;}
 if(state.phase==4){if(state.index<facts.captures){result.cursor.phase=5;result.need=ValueKernelNeed::Capture;return result;}result.cursor.phase=6;result.need=ValueKernelNeed::EndCaptures;return result;}
 if(state.phase==5){if(facts.uniform==0||facts.views!=0||facts.reads!=0||facts.writes!=0){result.need=ValueKernelNeed::InvalidCapture;return result;}result.cursor.phase=4;result.cursor.index+=1u64;result.need=ValueKernelNeed::PushCapture;return result;}
 if(state.phase==6){if(facts.explicit!=0){result.cursor.phase=7;result.need=ValueKernelNeed::Explicit;return result;}result.cursor.phase=8;result.need=ValueKernelNeed::Specialize;return result;}
 if(state.phase==7){result.cursor.phase=8;result.need=ValueKernelNeed::Specialize;return result;}
 if(state.phase==8){result.cursor.phase=9;result.need=ValueKernelNeed::Origin;return result;}
 if(state.phase==9){result.cursor.phase=10;result.need=ValueKernelNeed::Invoke;return result;}
 if(state.phase==10){result.cursor.phase=11;result.need=ValueKernelNeed::Ordinary;return result;}
 if(state.phase==11){result.cursor.phase=12;result.need=ValueKernelNeed::Value;return result;}
 if(state.phase==12){result.cursor.phase=13;result.need=ValueKernelNeed::Same;return result;}
 return result;
}

extern "C" CollectiveFieldSyntax collective_field_syntax(SignatureNode source) on cpu {
 CollectiveFieldSyntax result=CollectiveFieldSyntax(0,0,Text(0,Span(0u64,0u64)));
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
 ExprKind::Name(value){result.named=1;result.name=value;}
 ExprKind::Member(value){result.member=1;}
 ExprKind::Number(unused){}
 ExprKind::StaticU32(unused){}
 ExprKind::Bool(unused){}
 ExprKind::Index(unused){}
 ExprKind::Call(unused){}
 ExprKind::Invoke(unused){}
 ExprKind::Construct(unused){}
 ExprKind::Specialize(unused){}
 ExprKind::FunctionRef(unused){}
 ExprKind::Cooperate(unused){}
 ExprKind::Target(unused){}
 ExprKind::Unary(unused){}
 ExprKind::Binary(unused){}
 ExprKind::Select(unused){}

 }
 return result;
}
extern "C" u32 collective_project(SignatureNode source,address context,CollectiveProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.field) {
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
 ExprKind::Member(value){if(collective_project(SignatureNode(source.storage,value.base.offset),context,host)==0){return 0;}return host.field(context,value.field);}
 ExprKind::Number(unused){return host.root(context,source.offset);}
 ExprKind::StaticU32(unused){return host.root(context,source.offset);}
 ExprKind::Bool(unused){return host.root(context,source.offset);}
 ExprKind::Name(unused){return host.root(context,source.offset);}
 ExprKind::Index(unused){return host.root(context,source.offset);}
 ExprKind::Call(unused){return host.root(context,source.offset);}
 ExprKind::Invoke(unused){return host.root(context,source.offset);}
 ExprKind::Construct(unused){return host.root(context,source.offset);}
 ExprKind::Specialize(unused){return host.root(context,source.offset);}
 ExprKind::FunctionRef(unused){return host.root(context,source.offset);}
 ExprKind::Cooperate(unused){return host.root(context,source.offset);}
 ExprKind::Target(unused){return host.root(context,source.offset);}
 ExprKind::Unary(unused){return host.root(context,source.offset);}
 ExprKind::Binary(unused){return host.root(context,source.offset);}
 ExprKind::Select(unused){return host.root(context,source.offset);}

 }
}
extern "C" CollectivePrefixInput collective_prefix_input(SignatureNode source) on cpu {
 CollectivePrefixInput result=CollectivePrefixInput(CollectivePrefixKind::Plain,source.offset,0u64,Text(0,Span(0u64,0u64)));
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
 ExprKind::Call(value){
  address bytes=binding_at(value.name.kind==0?source.storage.source:source.storage.text,value.name.range.start);
  if(local_spelling(bytes,value.name.range.end-value.name.range.start,"prefix")){
   if(value.arguments.count!=2u64){result.kind=CollectivePrefixKind::Arity;return result;}
   result.root=binding_member(source.storage.nodes,value.arguments,0u64);result.count=binding_member(source.storage.nodes,value.arguments,1u64);
   CollectiveFieldSyntax root=collective_field_syntax(SignatureNode(source.storage,result.root));
   result.kind=CollectivePrefixKind::WrongName;
   if(root.named!=0){result.kind=CollectivePrefixKind::Prefix;result.name=root.name;}
  }
 }
 ExprKind::Number(unused){}
 ExprKind::StaticU32(unused){}
 ExprKind::Bool(unused){}
 ExprKind::Name(unused){}
 ExprKind::Member(unused){}
 ExprKind::Index(unused){}
 ExprKind::Invoke(unused){}
 ExprKind::Construct(unused){}
 ExprKind::Specialize(unused){}
 ExprKind::FunctionRef(unused){}
 ExprKind::Cooperate(unused){}
 ExprKind::Target(unused){}
 ExprKind::Unary(unused){}
 ExprKind::Binary(unused){}
 ExprKind::Select(unused){}

 }
 return result;
}
extern "C" u32 collective_read_access(u32 access,u64 components,address context,cfn<u32,address,u64> initialized) on cpu requires nonzero(initialized) {
 if(access!=2){return access;}
 u64 i=0u64;while(i<components){if(initialized(context,i)==0){return access;}i+=1u64;}
 return 3;
}


bool numeric_readable(u32 access) on cpu {return access==1||access==3;}
bool numeric_writable(u32 access) on cpu {return access==2||access==3||access==4;}
bool numeric_scan(ValueKind kind) on cpu {return kind==ValueKind::Scan||kind==ValueKind::SegmentedScan;}
bool numeric_head_mismatch(NumericFacts facts) on cpu {return facts.heads!=facts.source||!numeric_readable(facts.head_access)||(facts.has_head_prefix!=0&&(facts.has_prefix==0||facts.head_count!=facts.prefix_count||facts.head_count_word!=facts.prefix_word));}
extern "C" NumericStep numeric_step(ValueKind kind,CollectiveOperator operation,LocalCursor cursor,NumericFacts facts) on cpu {
 NumericStep result=NumericStep(cursor,NumericNeed::Error,NumericError::ReduceArity,ValueOperation::Reduce,0u64);
 if(cursor.phase==0){
  u64 expected=kind==ValueKind::SegmentedReduce?4u64:(kind==ValueKind::SegmentedScan?3u64:2u64);
  if(facts.arguments!=expected){if(numeric_scan(kind)){result.error=NumericError::ScanArity;}if(kind==ValueKind::SegmentedReduce){result.error=NumericError::SegmentedArity;}return result;}
  result.cursor.phase=1;result.need=NumericNeed::Source;return result;
 }
 if(cursor.phase==1){
  if(kind==ValueKind::SegmentedReduce){result.cursor.phase=2;result.need=NumericNeed::Head;result.argument=1u64;return result;}
  result.cursor.phase=4;result.need=NumericNeed::Target;result.argument=kind==ValueKind::SegmentedScan?2u64:1u64;return result;
 }
 if(cursor.phase==2){result.cursor.phase=3;result.need=NumericNeed::Target;result.argument=2u64;return result;}
 if(cursor.phase==3){result.cursor.phase=4;result.need=NumericNeed::Count;result.argument=3u64;return result;}
 if(cursor.phase==4){result.cursor.phase=5;result.need=NumericNeed::Types;return result;}
 if(cursor.phase==5){
  if(!collective_accepts(operation,facts.numeric!=0,facts.integer!=0)||facts.same==0){result.error=NumericError::ReduceType;if(numeric_scan(kind)){result.error=NumericError::ScanType;}if(kind==ValueKind::SegmentedReduce){result.error=NumericError::SegmentedType;}return result;}
  if(kind==ValueKind::SegmentedScan){result.cursor.phase=6;result.need=NumericNeed::Head;result.argument=1u64;return result;}
  result.cursor.phase=8;result.need=NumericNeed::Access;return result;
 }
 if(cursor.phase==6){if(numeric_head_mismatch(facts)){result.error=NumericError::HeadAccess;return result;}result.cursor.phase=8;result.need=NumericNeed::Access;return result;}
 if(cursor.phase==8){
  bool bad=facts.source_overlap!=0||facts.head_overlap!=0||!numeric_readable(facts.source_access)||!numeric_writable(facts.target_access)||(facts.has_prefix!=0&&facts.prefix_count==facts.target);
  if(kind==ValueKind::SegmentedReduce){bad=bad||numeric_head_mismatch(facts)||!numeric_writable(facts.count_access)||facts.source==facts.count||facts.target==facts.count||(facts.has_prefix!=0&&facts.prefix_count==facts.count);}
  if(bad){result.error=NumericError::ReduceAccess;if(numeric_scan(kind)){result.error=NumericError::ScanAccess;}if(kind==ValueKind::SegmentedReduce){result.error=NumericError::SegmentedAccess;}return result;}
  if(kind==ValueKind::Reduce){result.cursor.phase=9;result.need=NumericNeed::TargetShape;return result;}
  if(kind==ValueKind::SegmentedReduce){result.cursor.phase=10;result.need=NumericNeed::CountShape;return result;}
  result.cursor.phase=11;result.need=NumericNeed::Capacity;return result;
 }
 if(cursor.phase==9){if(facts.target_known==0||facts.target_size!=1){result.error=NumericError::ReduceShape;return result;}result.cursor.phase=12;result.need=NumericNeed::Fields;return result;}
 if(cursor.phase==10){if(facts.count_known==0||facts.count_size!=1){result.error=NumericError::CountShape;return result;}result.cursor.phase=11;result.need=NumericNeed::Capacity;return result;}
 if(cursor.phase==11){
  if(facts.source_known!=0&&facts.target_known!=0){
   if(kind==ValueKind::SegmentedReduce&&facts.target_size<facts.source_size){result.error=NumericError::SegmentedCapacity;return result;}
   if(numeric_scan(kind)&&((facts.has_prefix==0&&facts.source_size!=facts.target_size)||(facts.has_prefix!=0&&facts.target_size<facts.source_size))){result.error=NumericError::ScanCapacity;return result;}
  }
  result.cursor.phase=12;result.need=NumericNeed::Fields;return result;
 }
 if(cursor.phase==12){
  if(kind==ValueKind::SegmentedReduce){result.cursor.phase=13;result.need=NumericNeed::CountWrite;return result;}
  if(numeric_scan(kind)&&facts.has_prefix!=0){result.cursor.phase=14;result.need=NumericNeed::GrantInput;return result;}
  result.cursor.phase=15;result.need=NumericNeed::Reads;return result;
 }
 if(cursor.phase==13){result.cursor.phase=14;result.need=NumericNeed::GrantOutput;return result;}
 if(cursor.phase==14){result.cursor.phase=15;result.need=NumericNeed::Reads;return result;}
 result.cursor.phase=16;result.need=NumericNeed::Publish;
 if(numeric_scan(kind)){result.operation=ValueOperation::Scan;}
 if(kind==ValueKind::SegmentedReduce){result.operation=ValueOperation::SegmentedReduce;}
 return result;
}
extern "C" void collective_write_fields(address type,address context,u32 base,u64 count,cfn<CollectiveComponent,address,u64> component,cfn<void,address,u32,u32> write) on cpu requires nonzero(component) requires nonzero(write) {
 u64 i=0u64;while(i<count){CollectiveComponent field=component(type,i);u32 start=base+field.offset;write(context,start,start+field.size);i+=1u64;}
}
extern "C" void collective_write_count(address context,u32 word,cfn<void,address,u32,u32> write) on cpu requires nonzero(write) {write(context,word,word+4);}
extern "C" CollectiveReads collective_reads(u64 source,u32 second_present,u64 second,u32 third_present,u64 third) on cpu {
 CollectiveReads result=CollectiveReads(source,0u64,0u64,1u64);
 if(second_present!=0&&second!=source){if(second<source){result.first=second;result.second=source;}else{result.second=second;}result.count=2u64;}
 if(third_present!=0&&third!=result.first&&(result.count==1u64||third!=result.second)){
  if(third<result.first){result.third=result.second;result.second=result.first;result.first=third;}
  else if(result.count==1u64||third<result.second){result.third=result.second;result.second=third;}
  else{result.third=third;}
  result.count+=1u64;
 }
 return result;
}

struct RecordKey {u64 expression;u32 order;u32 valid;};
RecordKey record_key(SignatureNode source) on cpu {
 RecordKey result=RecordKey(source.offset,0,1);
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
 ExprKind::Call(value){
  address bytes=binding_at(value.name.kind==0?source.storage.source:source.storage.text,value.name.range.start);
  if(local_spelling(bytes,value.name.range.end-value.name.range.start,"descending")){
   if(value.arguments.count!=1u64){result.valid=0;return result;}
   result.expression=binding_member(source.storage.nodes,value.arguments,0u64);result.order=1;
  }
 }
 ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Name(unused){} ExprKind::Member(unused){} ExprKind::Index(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
 }
 return result;
}
RecordStep record_compact_step(RecordCursor cursor,Slice arguments,RecordFacts facts) on cpu {
 RecordCursor c=cursor;RecordStep result=RecordStep(c,RecordNeed::Error,RecordError::CompactArity,0u64,0u64,0,0);
 if(c.phase==0){if(arguments.count!=4u64){return result;}result.cursor.phase=1;result.need=RecordNeed::Source;return result;}
 if(c.phase==1){result.cursor.phase=2;result.need=RecordNeed::Predicate;result.argument=1u64;return result;}
 if(c.phase==2){result.cursor.phase=3;result.need=RecordNeed::Target;result.argument=2u64;return result;}
 if(c.phase==3){result.cursor.phase=4;result.need=RecordNeed::Views;return result;}
 if(c.phase==4){if(facts.views!=0){result.error=RecordError::CompactViews;return result;}result.cursor.phase=5;result.need=RecordNeed::Count;result.argument=3u64;return result;}
 if(c.phase==5){
  result.error=RecordError::CompactAccess;
  if(!numeric_readable(facts.source_access)){return result;}
  if(facts.predicate_access==2){result.cursor.phase=6;result.need=RecordNeed::FlagInitialized;return result;}
  if(!numeric_readable(facts.predicate_access)){return result;}
  c.phase=6;
 }
 if(c.phase==6){
  result.error=RecordError::CompactAccess;
  if((facts.predicate_access==2&&facts.initialized==0)||!numeric_writable(facts.target_access)||!numeric_writable(facts.count_access)||facts.target==facts.count||(facts.has_prefix!=0&&(facts.prefix_count==facts.target||facts.prefix_count==facts.count))||facts.source==facts.target||facts.source==facts.count||facts.predicate==facts.target||facts.predicate==facts.count){return result;}
  result.cursor.phase=7;result.need=RecordNeed::Types;return result;
 }
 if(c.phase==7){if(facts.same_type==0){result.error=RecordError::CompactType;return result;}result.cursor.phase=8;result.need=RecordNeed::CountShape;return result;}
 if(c.phase==8){if(facts.count_known==0||facts.count_size!=1){result.error=RecordError::CountShape;return result;}result.cursor.phase=9;result.need=RecordNeed::PredicateShape;return result;}
 if(c.phase==9){if(facts.source_known!=0&&facts.predicate_known!=0&&facts.source_size!=facts.predicate_size){result.error=RecordError::PredicateShape;return result;}result.cursor.phase=10;result.need=RecordNeed::Capacity;return result;}
 if(c.phase==10){if(facts.source_known!=0&&facts.target_known!=0&&facts.target_size<facts.source_size){result.error=RecordError::CompactCapacity;return result;}result.cursor.phase=11;result.need=RecordNeed::Fields;return result;}
 if(c.phase==11){result.cursor.phase=12;result.need=RecordNeed::CountWrite;return result;}
 if(c.phase==12){result.cursor.phase=13;result.need=RecordNeed::GrantOutput;return result;}
 if(c.phase==13){result.cursor.phase=14;result.need=RecordNeed::Reads;result.argument=1u64;return result;}
 result.cursor.phase=15;result.need=RecordNeed::Publish;return result;
}
RecordStep record_sort_step(RecordCursor cursor,StorageView source,Slice arguments,RecordFacts facts) on cpu {
 RecordCursor c=cursor;RecordStep result=RecordStep(c,RecordNeed::Error,RecordError::SortArity,c.index,0u64,0,0);
 if(c.phase==0){if(arguments.count<2u64){return result;}c.index=0u64;c.phase=1;}
 if(c.phase==4){c.index+=1u64;c.phase=1;}
 if(c.phase==1){
  if(c.index>=arguments.count-1u64){result.cursor=c;result.cursor.phase=5;result.need=RecordNeed::Target;result.argument=arguments.count-1u64;return result;}
  RecordKey key=record_key(SignatureNode(source,binding_member(source.nodes,arguments,c.index)));
  result.argument=c.index;
  if(key.valid==0){result.error=RecordError::Descending;return result;}
  result.cursor=c;result.cursor.phase=2;result.need=RecordNeed::Key;result.expression=key.expression;result.order=key.order;result.set_source=c.index==0u64?1:0;return result;
 }
 if(c.phase==2){
  if(facts.key_binding!=facts.source||facts.key_same_prefix==0||!numeric_readable(facts.key_access)){result.error=RecordError::KeyAccess;return result;}
  if(c.index==0u64){result.cursor.phase=3;result.need=RecordNeed::RootRead;return result;}
  c.phase=3;
 }
 if(c.phase==3){
  if(facts.root_read==0){result.error=RecordError::KeyAccess;return result;}
  if(facts.key_numeric==0){result.error=RecordError::KeyType;return result;}
  result.cursor.phase=4;result.need=RecordNeed::KeyComponents;return result;
 }
 if(c.phase==5){result.cursor.phase=6;result.need=RecordNeed::Views;return result;}
 if(c.phase==6){
  if(facts.views!=0){result.error=RecordError::SortViews;return result;}
  if(!numeric_writable(facts.target_access)||facts.source==facts.target||(facts.has_prefix!=0&&facts.prefix_count==facts.target)){result.error=RecordError::SortAccess;return result;}
  result.cursor.phase=7;result.need=RecordNeed::Types;return result;
 }
 if(c.phase==7){if(facts.same_type==0){result.error=RecordError::SortType;return result;}result.cursor.phase=8;result.need=RecordNeed::Capacity;return result;}
 if(c.phase==8){if(facts.source_known!=0&&facts.target_known!=0&&facts.target_size<facts.source_size){result.error=RecordError::SortCapacity;return result;}result.cursor.phase=9;result.need=RecordNeed::SourceFacts;return result;}
 if(c.phase==9){if(facts.source_proof==0){result.cursor.phase=13;result.need=RecordNeed::Fields;return result;}result.cursor.phase=10;result.need=RecordNeed::Elements;return result;}
 if(c.phase==10){if(facts.has_prefix!=0){result.cursor.phase=11;result.need=RecordNeed::JoinElements;return result;}result.cursor.phase=17;result.need=RecordNeed::SameLength;return result;}
 if(c.phase==17){if(facts.same_length==0){result.cursor.phase=11;result.need=RecordNeed::JoinElements;return result;}result.cursor.phase=12;result.need=RecordNeed::Guarantees;return result;}
 if(c.phase==11){result.cursor.phase=12;result.need=RecordNeed::Guarantees;return result;}
 if(c.phase==12){result.cursor.phase=13;result.need=RecordNeed::Fields;return result;}
 if(c.phase==13){if(facts.has_prefix!=0){result.cursor.phase=14;result.need=RecordNeed::GrantInput;return result;}result.cursor.phase=15;result.need=RecordNeed::Reads;result.argument=0u64;return result;}
 if(c.phase==14){result.cursor.phase=15;result.need=RecordNeed::Reads;result.argument=0u64;return result;}
 result.cursor.phase=16;result.need=RecordNeed::Publish;return result;
}
extern "C" RecordStep record_step(RecordKind kind,RecordCursor cursor,StorageView source,Slice arguments,RecordFacts facts) on cpu {
 if(kind==RecordKind::Compact){return record_compact_step(cursor,arguments,facts);}
 return record_sort_step(cursor,source,arguments,facts);
}
extern "C" void record_sort_components(address type,address keys,u32 word,u32 order,u64 count,cfn<RecordComponent,address,u64> component,cfn<void,address,u32,u32,u32> push) on cpu requires nonzero(component) requires nonzero(push) {
 u64 i=0u64;while(i<count){RecordComponent value=component(type,i);u32 offset=word+value.offset;
  if(value.size==8){u32 high=value.scalar==6?0:(value.scalar==5?2:1);push(keys,offset+4,high,order);}
  push(keys,offset,value.scalar,order);i+=1u64;
 }
}

extern "C" CustomStep custom_collection_step(ValueKind kind,u32 map,LocalCursor cursor,CustomFacts facts) on cpu {
 bool scan=numeric_scan(kind);bool counted=kind==ValueKind::SegmentedReduce;bool segmented=kind==ValueKind::SegmentedScan||counted;
 u64 offset=segmented?1u64:0u64;u64 functions=2u64+offset+(counted?1u64:0u64);u64 mapping=map!=0?1u64:0u64;
 CustomStep result=CustomStep(cursor,CustomNeed::Error,CustomError::Arity,ValueOperation::Reduce,0u64);
 if(cursor.phase==0){if(facts.arguments!=functions+2u64+mapping){return result;}result.cursor.phase=1;result.need=CustomNeed::Source;return result;}
 if(cursor.phase==1){if(segmented){result.cursor.phase=2;result.need=CustomNeed::Head;result.argument=1u64;return result;}result.cursor.phase=3;result.need=CustomNeed::Target;result.argument=1u64;return result;}
 if(cursor.phase==2){
  if(facts.heads!=facts.source||!numeric_readable(facts.head_access)||(facts.has_head_prefix!=0&&(facts.has_prefix==0||facts.head_count!=facts.prefix_count||facts.head_count_word!=facts.prefix_word))){result.error=CustomError::Head;return result;}
  result.cursor.phase=3;result.need=CustomNeed::Target;result.argument=2u64;return result;
 }
 if(cursor.phase==3){result.cursor.phase=4;result.need=CustomNeed::Types;return result;}
 if(cursor.phase==4){if((map==0&&facts.same==0)||facts.source_views!=0||facts.target_views!=0){result.error=CustomError::Type;return result;}result.cursor.phase=5;result.need=CustomNeed::Access;return result;}
 if(cursor.phase==5){
  if(facts.source_overlap!=0||facts.head_overlap!=0||!numeric_readable(facts.source_access)||!numeric_writable(facts.target_access)||(facts.has_prefix!=0&&facts.prefix_count==facts.target)){result.error=CustomError::Access;return result;}
  if(counted){result.cursor.phase=6;result.need=CustomNeed::Count;result.argument=3u64;return result;}
  result.cursor.phase=9;result.need=scan?CustomNeed::Capacity:CustomNeed::TargetShape;return result;
 }
 if(cursor.phase==6){if(!numeric_writable(facts.count_access)||facts.count==facts.source||facts.count==facts.target||(facts.has_prefix!=0&&facts.prefix_count==facts.count)){result.error=CustomError::CountAccess;return result;}result.cursor.phase=7;result.need=CustomNeed::CountShape;return result;}
 if(cursor.phase==7){if(facts.count_known==0||facts.count_size!=1){result.error=CustomError::CountShape;return result;}result.cursor.phase=9;result.need=CustomNeed::Capacity;return result;}
 if(cursor.phase==9){
  if(scan||counted){if(facts.source_known!=0&&facts.target_known!=0&&facts.target_size<facts.source_size){result.error=CustomError::Capacity;return result;}}
  else if(facts.target_known==0||facts.target_size!=1){result.error=CustomError::TargetShape;return result;}
  if(map!=0){result.cursor.phase=10;result.need=CustomNeed::Project;result.argument=functions;return result;}
  result.cursor.phase=11;result.need=CustomNeed::Identity;result.argument=functions;return result;
 }
 if(cursor.phase==10){result.cursor.phase=11;result.need=CustomNeed::Identity;result.argument=functions+1u64;return result;}
 if(cursor.phase==11){result.cursor.phase=12;result.need=CustomNeed::Combine;result.argument=functions+1u64+mapping;return result;}
 if(cursor.phase==12){result.cursor.phase=13;result.need=CustomNeed::Fields;return result;}
 if(cursor.phase==13){
  if(counted){result.cursor.phase=14;result.need=CustomNeed::CountWrite;return result;}
  if(scan&&facts.has_prefix!=0){result.cursor.phase=15;result.need=CustomNeed::GrantInput;return result;}
  result.cursor.phase=16;result.need=CustomNeed::Reads;return result;
 }
 if(cursor.phase==14){result.cursor.phase=15;result.need=CustomNeed::GrantOutput;return result;}
 if(cursor.phase==15){result.cursor.phase=16;result.need=CustomNeed::Reads;return result;}
 result.cursor.phase=17;result.need=CustomNeed::Publish;
 if(scan){result.operation=ValueOperation::Scan;}if(counted){result.operation=ValueOperation::SegmentedReduce;}
 return result;
}

extern "C" TileInputStep tile_input_step(TileCursor cursor,StorageView source,Slice body,TileInputFacts facts) on cpu {
 TileCursor c=cursor;TileInputStep result=TileInputStep(c,TileInputNeed::Complete,Text(0,Span(0u64,0u64)),0u64,0u64);
 if(c.phase==8){c.index+=1u64;c.phase=0;}
 if(c.phase==0){
  result.cursor=c;if(c.index>=body.count){return result;}
  StmtNode statement=binding_node<StmtNode>(source.nodes,binding_member(source.nodes,body,c.index));
  match(statement.kind){
  StmtKind::Let(value){
   ExprNode expression=binding_node<ExprNode>(source.nodes,value.value.offset);
   match(expression.kind){
   ExprKind::Call(call){
    address name=binding_at(call.name.kind==0?source.source:source.text,call.name.range.start);
    if(!local_spelling(name,call.name.range.end-call.name.range.start,"tile_input")){return result;}
    if(value.annotation.present!=0||call.arguments.count!=2u64){result.need=TileInputNeed::BadSyntax;return result;}
    result.cursor.phase=1;result.need=TileInputNeed::Name;result.name=value.name;
    result.field=binding_member(source.nodes,call.arguments,0u64);result.width=binding_member(source.nodes,call.arguments,1u64);return result;
   }
   ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Name(unused){} ExprKind::Member(unused){} ExprKind::Index(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
   }
  }
  StmtKind::Declare(unused){} StmtKind::Assign(unused){} StmtKind::Expr(unused){} StmtKind::If(unused){} StmtKind::Match(unused){} StmtKind::While(unused){} StmtKind::For(unused){} StmtKind::Parallel(unused){} StmtKind::Return(unused){} StmtKind::Break(){} StmtKind::Continue(){} StmtKind::Phase(){} StmtKind::Seek(unused){}
  }
  return result;
 }
 result.cursor=c;
 if(c.phase==1){if(facts.name_exists!=0){result.need=TileInputNeed::Duplicate;return result;}result.cursor.phase=2;result.need=TileInputNeed::Width;return result;}
 if(c.phase==2){result.cursor.phase=3;result.need=TileInputNeed::Field;return result;}
 if(c.phase==3){if(!numeric_readable(facts.access)||facts.views!=0||facts.nonempty==0){result.need=TileInputNeed::BadType;return result;}result.cursor.phase=4;result.need=TileInputNeed::Capacity;return result;}
 if(c.phase==4){if(facts.source_known!=0&&facts.domain_known!=0&&facts.source_size<facts.domain_size){result.need=TileInputNeed::BadCapacity;return result;}if(facts.access==3){result.cursor.phase=5;result.need=TileInputNeed::ElementDomain;return result;}c.phase=5;}
 if(c.phase==5){result.cursor.phase=6;result.need=TileInputNeed::Find;return result;}
 if(c.phase==6){if(facts.found==0){result.cursor.phase=7;result.need=TileInputNeed::Push;return result;}c.phase=7;}
 result.cursor.phase=8;result.need=TileInputNeed::Bind;return result;
}
extern "C" u64 tile_find_input(u64 count,u64 binding,u32 word,address context,cfn<TileCandidate,address,u64> candidate,cfn<u32,address,u64> same_type) on cpu requires nonzero(candidate) requires nonzero(same_type) {
 u64 i=0u64;while(i<count){TileCandidate item=candidate(context,i);if(item.input!=0&&item.binding==binding&&item.word==word&&same_type(context,i)!=0){return i;}i+=1u64;}return count;
}
extern "C" TileWidthStep tile_width_step(LocalCursor cursor,TileWidthFacts facts) on cpu {
 if(cursor.phase==0){if(facts.constant==0){return TileWidthStep(cursor,TileWidthNeed::ConstantError);}if(facts.scalar==0){return TileWidthStep(cursor,TileWidthNeed::WidthError);}return TileWidthStep(LocalCursor(1),TileWidthNeed::Read);}
 if(facts.width==0||(facts.first.present!=0&&(facts.first.fixed==0||facts.first.width!=facts.width))){return TileWidthStep(cursor,TileWidthNeed::WidthError);}
 return TileWidthStep(cursor,TileWidthNeed::Complete);
}
extern "C" TilePublishStep tile_publish_step(LocalCursor cursor,TilePublishFacts facts) on cpu {
 TilePublishStep result=TilePublishStep(cursor,TilePublishNeed::Complete,facts.atomic!=0?4:0);
 if(cursor.phase==0){
  if(facts.annotated!=0||facts.root==0||(facts.arguments!=1u64&&facts.arguments!=2u64)){result.need=TilePublishNeed::BadRoot;return result;}
  result.cursor.phase=1;
  if(facts.arguments==2u64){if(facts.cooperative!=0){result.need=TilePublishNeed::BadFixed;return result;}result.need=TilePublishNeed::FixedWidth;return result;}
  if(facts.has_tile==0){result.need=TilePublishNeed::MissingWidth;return result;}result.need=TilePublishNeed::InheritedWidth;return result;
 }
 if(cursor.phase==1){result.cursor.phase=2;result.need=TilePublishNeed::Value;return result;}
 if(cursor.phase==2){if(facts.present==0){return result;}if(facts.views!=0||facts.nonempty==0){result.need=TilePublishNeed::BadValue;return result;}result.cursor.phase=3;result.need=TilePublishNeed::Publish;return result;}
 if(cursor.phase==3){result.cursor.phase=4;result.need=TilePublishNeed::Bind;return result;}
 return result;
}
extern "C" TileCollectiveStep tile_collective_step(CollectiveOperator operation,LocalCursor cursor,TileCollectiveFacts facts) on cpu {
 LocalCursor c=cursor;TileCollectiveStep result=TileCollectiveStep(c,TileCollectiveNeed::Publish);
 if(c.phase==0){if(facts.root==0||facts.arguments!=1u64){result.need=TileCollectiveNeed::BadRoot;return result;}result.cursor.phase=1;result.need=TileCollectiveNeed::View;return result;}
 if(c.phase==1){if(facts.atomic!=0){result.need=TileCollectiveNeed::BadAtomic;return result;}if(facts.input!=0){result.cursor.phase=2;result.need=TileCollectiveNeed::Read;return result;}c.phase=2;}
 if(c.phase==2){result.cursor.phase=3;result.need=TileCollectiveNeed::Type;return result;}
 if(c.phase==3){if(!collective_accepts(operation,facts.numeric!=0,facts.integer!=0)){result.need=TileCollectiveNeed::BadType;return result;}if(facts.sum!=0){result.cursor.phase=4;result.need=TileCollectiveNeed::CommonProof;return result;}return result;}
 if(c.phase==4&&facts.proof!=0){result.cursor.phase=5;result.need=TileCollectiveNeed::SumProof;return result;}
 return result;
}
extern "C" u32 tile_project(SignatureNode source,address context,TileProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.member) requires nonzero(host.invalid) {
 ExprNode expression=binding_node<ExprNode>(source.storage.nodes,source.offset);
 match(expression.kind){
 ExprKind::Name(value){return host.root(context,source.offset,value);}
 ExprKind::Member(value){if(tile_project(SignatureNode(source.storage,value.base.offset),context,host)==0){return 0;}return host.member(context,source.offset,value.field);}
 ExprKind::Number(unused){} ExprKind::StaticU32(unused){} ExprKind::Bool(unused){} ExprKind::Call(unused){} ExprKind::Index(unused){} ExprKind::Invoke(unused){} ExprKind::Construct(unused){} ExprKind::Specialize(unused){} ExprKind::FunctionRef(unused){} ExprKind::Cooperate(unused){} ExprKind::Target(unused){} ExprKind::Unary(unused){} ExprKind::Binary(unused){} ExprKind::Select(unused){}
 }
 return host.invalid(context,source.offset);
}
extern "C" void tile_words(u64 count,address context,cfn<u32,address,u64> get,cfn<void,address,u32> push) on cpu requires nonzero(get) requires nonzero(push) {
 u64 i=0u64;while(i<count){push(context,get(context,i));i+=1u64;}
}
extern "C" void tile_publication(u64 count,address context,cfn<TilePair,address,u64> get,cfn<void,address,TilePair> push) on cpu requires nonzero(get) requires nonzero(push) {
 u64 i=0u64;while(i<count){push(context,get(context,i));i+=1u64;}
}
extern "C" void tile_results(u64 count,address context,TileResultHost host) on cpu requires nonzero(host.get) requires nonzero(host.allocate) requires nonzero(host.push) {
 u64 i=0u64;while(i<count){TileComponent field=host.get(context,i);u64 reg=host.allocate(context,field.scalar);host.push(context,TilePair(field.word,reg));i+=1u64;}
}
extern "C" u32 tile_root(u32 function,u32 cooperative,u64 root,u64 depth) on cpu {return (function!=0?(cooperative!=0&&root==depth):depth==1u64)?1:0;}
extern "C" u32 tile_barrier_allowed(u64 arguments,u32 root,u64 tiles) on cpu {return arguments==0u64&&root!=0&&tiles!=0u64?1:0;}
extern "C" TileLowerStep tile_lower_step(LocalCursor cursor,TileLowerFacts facts) on cpu {
 if(cursor.phase==0||cursor.phase==2){return TileLowerStep(LocalCursor(1),TileLowerNeed::Next);}
 if(facts.present==0){return TileLowerStep(cursor,TileLowerNeed::Complete);}
 if(facts.actual==0||facts.fixed==0){return TileLowerStep(cursor,TileLowerNeed::Invalid);}
 return TileLowerStep(LocalCursor(2),TileLowerNeed::Publish);
}
extern "C" u32 tile_width_bound(TileRange range) on cpu {return range.fixed!=0?range.width:4294967295;}
extern "C" TileLanePlan tile_lane_plan(TileRange range) on cpu {return TileLanePlan(tile_width_bound(range)-1,range.fixed,range.width);}
extern "C" u32 tile_index_minimum(u32 width,u32 count) on cpu requires nonzero(width) {
 u32 tail=count%width;return tail==0?width:tail;
}
extern "C" TilePlaceStep tile_place_step(LocalCursor cursor,TilePlaceFacts facts) on cpu {
 if(cursor.phase==0){return TilePlaceStep(LocalCursor(1),facts.present!=0?TilePlaceNeed::Type:TilePlaceNeed::Complete);}
 if(cursor.phase==1){return TilePlaceStep(LocalCursor(2),TilePlaceNeed::Range);}
 if(cursor.phase==2&&facts.fixed!=0&&facts.prefix==0){return TilePlaceStep(LocalCursor(3),TilePlaceNeed::Count);}
 if(cursor.phase==3&&facts.known!=0){return TilePlaceStep(LocalCursor(4),TilePlaceNeed::Minimum);}
 if(cursor.phase==2||cursor.phase==3||cursor.phase==4){return TilePlaceStep(LocalCursor(5),TilePlaceNeed::Bound);}
 return TilePlaceStep(cursor,facts.bounded!=0?TilePlaceNeed::Publish:TilePlaceNeed::Invalid);
}

enum LoopProgress:u32 { Unchanged=0; Added=1; Lost=2; };

LoopProgress compare_loop_fact(u32 entry, u32 current) on cpu {
    if (entry == current) { return LoopProgress::Unchanged; }
    return entry == 0 ? LoopProgress::Added : LoopProgress::Lost;
}

LoopProgress initial_loop_resources(LoopResource entry, LoopResource current, address context, LoopProgressHost host) on cpu
    requires nonzero(host.payload) {
    LoopProgress progress = compare_loop_fact(entry.owned, current.owned);
    if (progress == LoopProgress::Lost) { return progress; }

    usize count = entry.count > current.count ? entry.count : current.count;
    usize index = 0u64;
    while (index < count) {
        LoopPayload before = LoopPayload(0, LoopResource(unsafe_address(0u64), 0u64, 0, 0));
        LoopPayload after = before;
        if (index < entry.count) { before = host.payload(context, entry.children, index); }
        if (index < current.count) { after = host.payload(context, current.children, index); }
        index += 1u64;
        LoopProgress payload = compare_loop_fact(entry.variant & before.present, current.variant & after.present);
        if (payload == LoopProgress::Lost) { return payload; }
        if (payload == LoopProgress::Added) { progress = payload; }
        LoopProgress child = initial_loop_resources(before.resource, after.resource, context, host);
        if (child == LoopProgress::Lost) { return child; }
        if (child == LoopProgress::Added) { progress = child; }
    }
    return progress;
}

extern "C" u32 initial_loop_progress(address context, LoopProgressHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.boolean_at) requires nonzero(host.payload) {
    u32 added = 0;
    LoopProgressBinding binding = host.next(context);
    while (binding.present != 0) {
        usize component = 0u64;
        while (component < binding.components) {
            LoopFactPair value = host.boolean_at(context, component);
            component += 1u64;
            LoopProgress boolean = compare_loop_fact(value.entry, value.current);
            if (boolean == LoopProgress::Lost) { return 0; }
            if (boolean == LoopProgress::Added) { added = 1; }
        }
        LoopProgress resources = initial_loop_resources(binding.entry, binding.current, context, host);
        if (resources == LoopProgress::Lost) { return 0; }
        if (resources == LoopProgress::Added) { added = 1; }
        binding = host.next(context);
    }
    return added;
}
