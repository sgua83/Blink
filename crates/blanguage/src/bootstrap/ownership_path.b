module bootstrap.ownership_path;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" u32 ownership(u32 kind,u32 truth) on cpu {
 if(kind==3&&truth!=2){return truth!=0 ? 1 : 0;}return kind;
}
extern "C" u32 possible(u32 truth) on cpu {return truth!=0 ? 1 : 0;}
extern "C" ResidencyStep residency_step(Cursor state,ResidencyFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return ResidencyStep(c,ResidencyNeed::Inspect);}
 if(c.phase==1){
  if(f.selected!=0){c.phase=2;return ResidencyStep(c,ResidencyNeed::Query);}
  c.phase=8;return ResidencyStep(c,ResidencyNeed::Copy);
 }
 if(c.phase==2){
  if(f.truth==1){c.phase=8;return ResidencyStep(c,ResidencyNeed::Yes);}
  if(f.truth==0){c.phase=8;return ResidencyStep(c,ResidencyNeed::No);}
  c.phase=3;return ResidencyStep(c,ResidencyNeed::Left);
 }
 if(c.phase==3){c.phase=4;return ResidencyStep(c,ResidencyNeed::Right);}
 if(c.phase==4){c.phase=8;return ResidencyStep(c,ResidencyNeed::Conditional);}
 if(c.phase==8){return ResidencyStep(c,ResidencyNeed::Complete);}
 return ResidencyStep(c,ResidencyNeed::Next);
}
extern "C" ModelStep model_step(Cursor state,ModelFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return ModelStep(c,ModelNeed::Inspect);}
 if(c.phase==1){
  if(f.kind==2){c.phase=2;return ModelStep(c,ModelNeed::Ownership);}
  if(f.kind==3){c.phase=10;return ModelStep(c,ModelNeed::Ownership);}
  return ModelStep(c,ModelNeed::Complete);
 }
 if(c.phase==2){c.phase=3;return ModelStep(c,ModelNeed::Residency);}
 if(c.phase==3){
  c.phase=8;return ModelStep(c,f.initialization!=0 ? ModelNeed::Initialization : ModelNeed::Next);
 }
 if(c.phase==10){c.phase=11;c.index=0u64;return ModelStep(c,ModelNeed::Next);}
 if(c.phase==11){
  if(c.index>=f.count){return ModelStep(c,ModelNeed::Complete);}
  c.phase=12;return ModelStep(c,ModelNeed::Child);
 }
 if(c.phase==12){
  if(f.present!=0){c.phase=13;return ModelStep(c,ModelNeed::Recurse);}
  c.index+=1u64;c.phase=11;return ModelStep(c,ModelNeed::Next);
 }
 if(c.phase==13){c.index+=1u64;c.phase=11;return ModelStep(c,ModelNeed::Next);}
 if(c.phase==8){return ModelStep(c,ModelNeed::Complete);}
 return ModelStep(c,ModelNeed::Next);
}
extern "C" MetadataStep metadata_step(Cursor state,MetadataFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return MetadataStep(c,MetadataNeed::Inspect);}
 if(c.phase==1){
  if(f.kind==2||(f.kind==3&&f.presence!=0)){
   if(f.forget==0){c.phase=2;return MetadataStep(c,f.resolved!=0 ? MetadataNeed::ResolvedAvailability : MetadataNeed::Availability);}
   c.phase=3;return MetadataStep(c,MetadataNeed::Next);
  }
  if(f.kind==3){c.phase=10;c.index=0u64;return MetadataStep(c,MetadataNeed::Next);}
  return MetadataStep(c,MetadataNeed::Complete);
 }
 if(c.phase==2){
  if(f.available==0){return MetadataStep(c,MetadataNeed::Complete);}
  c.phase=3;return MetadataStep(c,MetadataNeed::Next);
 }
 if(c.phase==3){
  if(f.place!=0){c.phase=8;return MetadataStep(c,f.forget!=0 ? MetadataNeed::Forget : MetadataNeed::Initialize);}
  return MetadataStep(c,MetadataNeed::Complete);
 }
 if(c.phase==10){
  if(c.index>=f.count){return MetadataStep(c,MetadataNeed::Complete);}
  c.phase=11;return MetadataStep(c,MetadataNeed::Child);
 }
 if(c.phase==11){
  if(f.owns!=0){
   if(f.place!=0){c.phase=12;return MetadataStep(c,MetadataNeed::Project);}
   c.phase=13;return MetadataStep(c,MetadataNeed::Recurse);
  }
  c.index+=1u64;c.phase=10;return MetadataStep(c,MetadataNeed::Next);
 }
 if(c.phase==12){c.phase=13;return MetadataStep(c,MetadataNeed::Recurse);}
 if(c.phase==13){c.index+=1u64;c.phase=10;return MetadataStep(c,MetadataNeed::Next);}
 if(c.phase==8){return MetadataStep(c,MetadataNeed::Complete);}
 return MetadataStep(c,MetadataNeed::Next);
}
extern "C" IntersectStep intersect_step(Cursor state,IntersectFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return IntersectStep(c,IntersectNeed::Contains);}
 if(c.phase==1){
  if(f.hit!=0){return IntersectStep(c,IntersectNeed::Yes);}
  if(f.indexed==0){return IntersectStep(c,IntersectNeed::No);}
  c.phase=2;c.index=0u64;return IntersectStep(c,IntersectNeed::Next);
 }
 if(c.phase==2){
  if(c.index>=f.count){return IntersectStep(c,IntersectNeed::No);}
  c.phase=3;return IntersectStep(c,IntersectNeed::Child);
 }
 if(c.phase==3){
  if(f.child!=0){return IntersectStep(c,IntersectNeed::Yes);}
  c.index+=1u64;c.phase=2;return IntersectStep(c,IntersectNeed::Next);
 }
 return IntersectStep(c,IntersectNeed::Next);
}

extern "C" u32 compare_residency(address context,ResidencyPair pair,ResidencyCompareHost host) on cpu
    requires nonzero(host.possible)
    requires nonzero(host.equivalent)
    requires nonzero(host.selected)
    requires nonzero(host.project)
    requires nonzero(host.leaf) {
    if(host.possible(pair.path)==0 || address_bits(pair.left)==address_bits(pair.right)){return 1;}
    if(address_bits(host.selected(pair.left).condition)!=0u64 ||
       address_bits(host.selected(pair.right).condition)!=0u64){
        return host.project(context,pair);
    }
    if(host.equivalent(pair.left,pair.right)!=0){return 1;}
    return host.leaf(context,pair);
}

extern "C" u32 compare_projected_residency(address context,ResidencyPair pair,ResidencyCompareHost host) on cpu
    requires nonzero(host.equivalent)
    requires nonzero(host.selected)
    requires nonzero(host.branch)
    requires nonzero(host.leaf) {
    if(host.equivalent(pair.left,pair.right)!=0){return 1;}
    ResidencyBranch left=host.selected(pair.left);
    if(address_bits(left.condition)!=0u64){
        if(host.branch(context,ResidencyPair(left.yes,pair.right,pair.path),left.condition,1)==0){return 0;}
        return host.branch(context,ResidencyPair(left.no,pair.right,pair.path),left.condition,0);
    }
    ResidencyBranch right=host.selected(pair.right);
    if(address_bits(right.condition)!=0u64){
        if(host.branch(context,ResidencyPair(pair.left,right.yes,pair.path),right.condition,1)==0){return 0;}
        return host.branch(context,ResidencyPair(pair.left,right.no,pair.path),right.condition,0);
    }
    return host.leaf(context,pair);
}

extern "C" ResidencyVisitResult check_residency(address context,ResidencyVisit visit,ResidencyCheckHost host) on cpu
    requires nonzero(host.possible) requires nonzero(host.inspect)
    requires nonzero(host.branch) requires nonzero(host.origins) {
    if(visit.mode!=ResidencyCheckMode::Query && host.possible(visit.path)==0){
        return ResidencyVisitResult(1,visit.deferred);
    }
    let shape=host.inspect(visit.value);
    if(address_bits(shape.branch.condition)!=0u64){
        ResidencyVisit branch=visit;
        branch.value=shape.branch.yes;
        let yes=host.branch(context,branch,shape.branch.condition,1);
        if(yes.accepted==0){return yes;}
        branch.value=shape.branch.no;
        branch.deferred=yes.deferred;
        return host.branch(context,branch,shape.branch.condition,0);
    }
    if(address_bits(shape.origins)==0u64){return ResidencyVisitResult(1,visit.deferred);}
    return host.origins(context,visit,shape.origins);
}

extern "C" ResidencyVisitResult check_residency_origins(address context,ResidencyVisit visit,address iterator,ResidencyOriginsHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.overlaps)
    requires nonzero(host.same_input) requires nonzero(host.insert) {
    u32 deferred=visit.deferred;
    bool query=visit.mode==ResidencyCheckMode::Query;
    ResidencyOriginItem origin=host.next(iterator);
    if(origin.kind==ResidencyOriginKind::End){
        return ResidencyVisitResult(u32(query && host.overlaps(visit.path)==0),deferred);
    }
    while(origin.kind!=ResidencyOriginKind::End){
        u32 accepted=1;
        if(origin.kind==ResidencyOriginKind::Cpu || origin.kind==ResidencyOriginKind::Gpu){
            u32 target=origin.kind==ResidencyOriginKind::Cpu ? 0 : 1;
            accepted=u32(visit.target==target || (query && host.overlaps(visit.path)==0));
        }else if(origin.kind==ResidencyOriginKind::Output){
            accepted=u32(visit.mode==ResidencyCheckMode::Known || (query && host.overlaps(visit.path)==0));
        }else if(visit.mode==ResidencyCheckMode::Require){
            accepted=host.insert(context,origin.input,visit.target,visit.path);
        }else if(query){
            bool wanted=address_bits(visit.wanted)!=0u64;
            bool matches=wanted && host.same_input(visit.wanted,origin.input)!=0;
            if((deferred==0 || matches) && (wanted || host.overlaps(visit.path)!=0)){
                deferred=1;
                if(matches){accepted=host.insert(context,origin.input,visit.target,visit.path);}
            }
        }
        if(accepted==0){return ResidencyVisitResult(0,deferred);}
        origin=host.next(iterator);
    }
    return ResidencyVisitResult(1,deferred);
}

extern "C" void bind_argument(address context,u64 prefix,ArgumentBindingHost host) on cpu
    requires nonzero(host.bind_root)
    requires nonzero(host.next_descendant)
    requires nonzero(host.declared_at)
    requires nonzero(host.bind_child) {
    host.bind_root(context);
    ArgumentDescendant next=host.next_descendant(context);
    while(next.fields!=0u64){
        if(next.bound==0){
            u64 fields=next.fields;
            while(fields>prefix && host.declared_at(context,fields)==0){
                fields-=1u64;
            }
            if(fields==prefix){host.bind_child(context);}
        }
        next=host.next_descendant(context);
    }
}

extern "C" void traverse_residency_query(address context,ResidencyQueryHost host) on cpu
    requires nonzero(host.consume) requires nonzero(host.next_child)
    requires nonzero(host.leave) requires nonzero(host.project) {
    if(host.consume(context)==0){return;}
    while(true){
        let child=host.next_child(context);
        if(address_bits(child)==0u64){
            if(host.leave(context)==0){return;}
        }else if(host.project(context,child)!=0 && host.consume(context)==0){
            return;
        }
    }
}

extern "C" u32 enter_residency_query_child(address context,address child,address path,ResidencyQueryChildHost host) on cpu
    requires nonzero(host.possible) requires nonzero(host.enter) requires nonzero(host.release) {
    if(host.possible(path)==0){
        host.release(path);
        return 0;
    }
    return host.enter(context,child,path);
}

extern "C" u32 resolve_residency_source(address context,SourceVisit visit,ResidencySourceHost host) on cpu
    requires nonzero(host.possible)
    requires nonzero(host.selected)
    requires nonzero(host.truth)
    requires nonzero(host.under)
    requires nonzero(host.both)
    requires nonzero(host.origins) {
    if(host.possible(visit.path)==0){return 0;}
    return resolve_viable_source(context,visit,host);
}

extern "C" u32 resolve_viable_source(address context,SourceVisit visit,ResidencySourceHost host) on cpu
    requires nonzero(host.selected)
    requires nonzero(host.truth)
    requires nonzero(host.under)
    requires nonzero(host.both)
    requires nonzero(host.origins) {
    ResidencyBranch branch=host.selected(visit.value);
    if(address_bits(branch.condition)==0u64){
        return host.origins(context,visit);
    }
    if(address_bits(visit.result)!=0u64){
        u32 truth=host.truth(branch.condition,visit.path);
        if(truth!=2){
            address child=truth==1 ? branch.yes : branch.no;
            return resolve_viable_source(context,SourceVisit(child,visit.path,visit.result),host);
        }
        host.both(context,visit,branch);
        return 0;
    }
    SourceVisit positive=SourceVisit(branch.yes,visit.path,visit.result);
    if(host.under(context,positive,branch.condition,1)!=0){return 1;}
    return host.under(context,SourceVisit(branch.no,visit.path,visit.result),branch.condition,0);
}

extern "C" u32 resolve_resource_origins(address cursor,u32 collect,ResourceOriginHost host) on cpu
    requires nonzero(host.next)
    requires nonzero(host.append)
    requires nonzero(host.output) {
    ResourceOrigin origin=host.next(cursor);
    while(origin.kind!=ResourceOriginKind::End){
        if(origin.kind==ResourceOriginKind::Concrete){
            if(collect==0){return 1;}
            host.append(cursor,origin.value);
        }else if(host.output(cursor,origin.value)!=0 && collect==0){return 1;}
        origin=host.next(cursor);
    }
    return 0;
}

void walk_residency_dependency(address names,address value,DependencyHost host) on cpu
    requires nonzero(host.residency) requires nonzero(host.walk) {
    ResidencyCheckNode shape=host.residency(value);
    if(address_bits(shape.branch.condition)!=0u64){
        walk_residency_dependency(names,shape.branch.yes,host);
        walk_residency_dependency(names,shape.branch.no,host);
    }else if(address_bits(shape.origins)!=0u64){
        host.walk(names,DependencyList::Origins,shape.origins);
    }
}

void walk_dependency_model(address names,address value,DependencyHost host) on cpu
    requires nonzero(host.model) requires nonzero(host.residency) requires nonzero(host.walk) {
    DependencyModel model=host.model(value);
    if(address_bits(model.residency)!=0u64){
        walk_residency_dependency(names,model.residency,host);
        if(model.maximum!=0u64 && address_bits(model.elements)!=0u64){
            walk_dependency_model(names,model.elements,host);
        }
    }else if(address_bits(model.payloads)!=0u64){
        host.walk(names,DependencyList::Payloads,model.payloads);
    }
}

extern "C" void collect_residency_dependencies(address names,address cursor,DependencyHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.insert)
    requires nonzero(host.model) requires nonzero(host.residency)
    requires nonzero(host.walk) {
    DependencyItem item=host.next(cursor);
    while(item.kind!=DependencyItemKind::End){
        if(item.kind==DependencyItemKind::Reference){
            host.insert(names,item.value);
            host.walk(names,DependencyList::Arguments,item.arguments);
        }else if(item.kind==DependencyItemKind::Model){
            walk_dependency_model(names,item.value,host);
        }else if(item.kind==DependencyItemKind::Residency){
            walk_residency_dependency(names,item.value,host);
        }
        item=host.next(cursor);
    }
}

bool same_residency_condition(address a,address b,ResidencyAlgebraHost host) on cpu
    requires nonzero(host.condition_equal) {
    return address_bits(a)!=0u64 && host.condition_equal(a,b)!=0;
}

extern "C" ChoicePlan choose_residency(address when,address yes,address no,ResidencyAlgebraHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal)
    requires nonzero(host.condition_equal) requires nonzero(host.order) requires nonzero(host.facts) requires nonzero(host.exclusive) {
    let plan=ChoicePlan(ChoiceAction::Select,0u64,0u64,0,0);
    ChoiceFacts facts=host.facts(when);
    if(facts.impossible!=0){plan.action=ChoiceAction::Absent;return plan;}
    if(facts.literal==1){plan.action=ChoiceAction::Yes;return plan;}
    if(facts.literal==0){plan.action=ChoiceAction::No;return plan;}
    if(facts.unconstrained!=0){plan.action=ChoiceAction::Join;return plan;}

    address left_value=yes;
    address right_value=no;
    ResidencyBranch left=host.inspect(left_value).branch;
    ResidencyBranch right=host.inspect(right_value).branch;
    // Projected conditions can overlap. Absorb a repeated condition only
    // when the matching arm already supplies the same source.
    while(true){
        if(same_residency_condition(right.condition,when,host) && host.equal(left_value,right.yes)!=0){
            right_value=right.no;
            plan.no_depth+=1u64;
            right=host.inspect(right_value).branch;
        }else if(same_residency_condition(left.condition,when,host) && host.equal(right_value,left.no)!=0){
            left_value=left.yes;
            plan.yes_depth+=1u64;
            left=host.inspect(left_value).branch;
        }else{
            break;
        }
    }
    bool exclusive=host.exclusive(when)!=0;
    if(exclusive){
        if(same_residency_condition(left.condition,when,host)){
            left_value=left.yes;
            plan.yes_depth+=1u64;
            left=host.inspect(left_value).branch;
        }
        if(same_residency_condition(right.condition,when,host)){
            right_value=right.no;
            plan.no_depth+=1u64;
            right=host.inspect(right_value).branch;
        }
    }
    if(host.equal(left_value,right_value)!=0){
        plan.action=ChoiceAction::Yes;
        return plan;
    }
    if(!exclusive){return plan;}

    // Only exclusive branches can move ahead of the current condition.
    address first=unsafe_address(0u64);
    if(address_bits(left.condition)!=0u64 && host.order(left.condition,when)<0 && host.exclusive(left.condition)!=0){
        first=left.condition;
    }
    if(address_bits(right.condition)!=0u64 && host.order(right.condition,when)<0
        && (address_bits(first)==0u64 || host.order(right.condition,first)<0)
        && host.exclusive(right.condition)!=0){
        first=right.condition;
    }
    if(address_bits(first)!=0u64){
        plan.split_yes=u32(same_residency_condition(left.condition,first,host));
        plan.split_no=u32(same_residency_condition(right.condition,first,host));
    }
    return plan;
}

extern "C" JoinPlan join_residency(address left,address right,ResidencyAlgebraHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal)
    requires nonzero(host.condition_equal) requires nonzero(host.order) requires nonzero(host.exclusive) requires nonzero(host.empty) {
    let null=unsafe_address(0u64);
    let plan=JoinPlan(JoinAction::Keep,0,null,right,right);
    if(host.equal(left,right)!=0){return plan;}
    ResidencyCheckNode a=host.inspect(left);
    ResidencyCheckNode b=host.inspect(right);
    bool left_selected=address_bits(a.branch.condition)!=0u64;
    bool right_selected=address_bits(b.branch.condition)!=0u64;
    bool left_origins=address_bits(a.origins)!=0u64;
    bool right_origins=address_bits(b.origins)!=0u64;
    if(!right_selected && !right_origins){return plan;}
    if(!left_selected && !left_origins){plan.action=JoinAction::Copy;return plan;}
    if(left_origins && right_origins){
        plan.action=host.empty(a.origins)!=0 || host.empty(b.origins)!=0 ? JoinAction::Clear : JoinAction::Extend;
        return plan;
    }

    plan.action=JoinAction::Select;
    if(left_selected && (!right_selected || host.order(a.branch.condition,b.branch.condition)<=0)){
        plan.split_left=1;
        plan.condition=a.branch.condition;
        // An overlapping right selection remains a whole value in both arms.
        if(same_residency_condition(b.branch.condition,plan.condition,host) && host.exclusive(plan.condition)!=0){
            plan.right_yes=b.branch.yes;
            plan.right_no=b.branch.no;
        }
    }else{
        plan.condition=b.branch.condition;
        plan.right_yes=b.branch.yes;
        plan.right_no=b.branch.no;
    }
    return plan;
}
