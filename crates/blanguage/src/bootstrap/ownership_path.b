module bootstrap.ownership_path;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" u32 ownership(u32 kind,u32 truth) on cpu {
 if(kind==3&&truth!=2){return truth!=0 ? 1 : 0;}return kind;
}
extern "C" u32 possible(u32 truth) on cpu {return truth!=0 ? 1 : 0;}
extern "C" PositionStep position_step(Cursor state,PositionFacts f) on cpu {
 Cursor c=state;
 if(c.phase==0){c.phase=1;return PositionStep(c,PositionNeed::Inspect);}
 if(c.phase==1){
  if(f.kind==1){c.phase=2;return PositionStep(c,PositionNeed::Query);}
  if(f.kind==2){c.phase=10;return PositionStep(c,PositionNeed::Left);}
  if(f.kind==3){c.phase=20;c.index=0u64;return PositionStep(c,PositionNeed::Next);}
  c.phase=8;return PositionStep(c,PositionNeed::Copy);
 }
 if(c.phase==2){
  if(f.truth==1){c.phase=8;return PositionStep(c,PositionNeed::Yes);}
  if(f.truth==0){c.phase=8;return PositionStep(c,PositionNeed::No);}
  c.phase=4;return PositionStep(c,PositionNeed::AssumeYes);
 }
 if(c.phase==4){c.phase=5;return PositionStep(c,PositionNeed::YesUnder);}
 if(c.phase==5){c.phase=6;return PositionStep(c,PositionNeed::AssumeNo);}
 if(c.phase==6){c.phase=7;return PositionStep(c,PositionNeed::NoUnder);}
 if(c.phase==7){c.phase=8;return PositionStep(c,PositionNeed::Conditional);}
 if(c.phase==10){c.phase=11;return PositionStep(c,PositionNeed::Right);}
 if(c.phase==11){c.phase=8;return PositionStep(c,PositionNeed::Both);}
 if(c.phase==20){
  if(c.index<f.count){c.phase=21;return PositionStep(c,PositionNeed::QueryChoice);}
  c.phase=23;c.index=0u64;return PositionStep(c,PositionNeed::Next);
 }
 if(c.phase==21){
  if(f.truth==1){c.phase=8;return PositionStep(c,PositionNeed::Chosen);}
  c.phase=22;return PositionStep(c,f.truth==0 ? PositionNeed::RecordAbsent : PositionNeed::RecordPossible);
 }
 if(c.phase==22){c.index+=1u64;c.phase=20;return PositionStep(c,PositionNeed::Next);}
 if(c.phase==23){
  if(c.index<f.count){c.phase=24;return PositionStep(c,PositionNeed::ReadChoice);}
  c.phase=30;c.index=1u64;return PositionStep(c,PositionNeed::Next);
 }
 if(c.phase==24){
  if(f.truth==0){c.phase=25;return PositionStep(c,PositionNeed::Absent);}
  c.phase=26;return PositionStep(c,PositionNeed::AssumeChoice);
 }
 if(c.phase==26){c.phase=25;return PositionStep(c,PositionNeed::ChoiceUnder);}
 if(c.phase==25){c.index+=1u64;c.phase=23;return PositionStep(c,PositionNeed::Next);}
 if(c.phase==30){
  if(c.index>=f.count){c.phase=8;return PositionStep(c,PositionNeed::First);}
  c.phase=31;return PositionStep(c,PositionNeed::Compare);
 }
 if(c.phase==31){
  if(f.equal==0){c.phase=8;return PositionStep(c,PositionNeed::Indexed);}
  c.index+=1u64;c.phase=30;return PositionStep(c,PositionNeed::Next);
 }
 if(c.phase==8){return PositionStep(c,PositionNeed::Complete);}
 return PositionStep(c,PositionNeed::Next);
}
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
    requires nonzero(host.consume)
    requires nonzero(host.advance) {
    ResidencyQueryStep step=ResidencyQueryStep::Entered;
    while(step!=ResidencyQueryStep::Complete){
        if(step==ResidencyQueryStep::Entered && host.consume(context)==0){return;}
        step=host.advance(context);
    }
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
