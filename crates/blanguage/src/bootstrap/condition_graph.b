module bootstrap.condition_graph;

usize word_at(Span span,usize index) on cpu {
 return unsafe_position<usize>(unsafe_address(address_bits(span.data)+u64(index)*8u64));
}
Gate node_at(Graph graph,usize index) on compute {
 return unsafe_position<Gate>(unsafe_address(address_bits(graph.nodes)+u64(index)*u64(graph.stride)));
}
u32 value_at(address values,usize index) on compute {
 return u32(unsafe_position<u8>(unsafe_address(address_bits(values)+u64(index))));
}
void set_value(address values,usize index,u32 value) on compute {
 unsafe_position<u8>(unsafe_address(address_bits(values)+u64(index)))=u8(value);
}

u32 propagate(address context,Graph graph,Host host) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop)
 requires nonzero(host.trail_push) {
 address values=graph.values;
 while(true){
  Assignment assignment=host.pending_pop(context);
  if(assignment.present==0){return 1;}
  usize id=assignment.node;u32 truth=assignment.truth+1;
  u32 old=value_at(values,id);
  if(old!=0){if(old!=truth){return 0;}continue;}
  set_value(values,id,truth);host.trail_push(context,id);
  Gate node=node_at(graph,id);
  usize own=node.kind>=2 ? 1u64 : 0u64;
  usize at=0u64;
  while(at<own+node.parents.count){
   usize parent=at<own ? id : word_at(node.parents,at-own);
   Gate gate=node_at(graph,parent);u32 either=gate.kind==3 ? 2 : 1;
   u32 left=value_at(values,gate.left);u32 right=value_at(values,gate.right);
   u32 result=0;
   if(left==either||right==either){result=either;}
   else if(left!=0&&right!=0){result=3-either;}
   u32 required=value_at(values,parent);
   if(required!=0&&result!=0){if(required!=result){return 0;}}
   else if(required==0&&result!=0){host.pending_push(context,parent,result-1);}
   else if(required!=0){
    if(left==0&&(required!=either||right!=0)){host.pending_push(context,gate.left,required-1);}
    if(right==0&&(required!=either||left!=0)){host.pending_push(context,gate.right,required-1);}
   }
   at+=1u64;
  }
 }
}

extern "C" u32 overlap(address left,address right,u32 right_truth,address context,Host host) on cpu
 requires nonzero(host.index) requires nonzero(host.node_count) requires nonzero(host.inspect)
 requires nonzero(host.define) requires nonzero(host.parent_push) requires nonzero(host.prepare)
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.inspect_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative) {
 usize first=host.index(context,left);usize second=host.index(context,right);
 usize at=0u64;
 while(at<host.node_count(context)){
  SourceNode source=host.inspect(context,at);
  if(source.kind==0){host.pending_push(context,at,0);}
  else if(source.kind==1&&source.empty!=0){host.pending_push(context,at,1);}
  if(source.kind>=2){
   address child=source.right;
   if(source.kind==4&&source.truth==0){child=source.no;}
   usize a=host.index(context,source.left);usize b=host.index(context,child);
   host.define(context,at,source.kind==3 ? 3 : 2,a,b);
   host.parent_push(context,a,at);host.parent_push(context,b,at);
  }else{host.define(context,at,source.kind,0u64,0u64);}
  at+=1u64;
 }
 Graph graph=host.prepare(context);address values=graph.values;
 host.pending_push(context,first,1);host.pending_push(context,second,right_truth);
 usize consumed=0u64;usize negative=0u64;
 while(true){
  if(propagate(context,graph,host)!=0){
   usize before=host.additions(context);Span trail=host.trail(context);
   at=consumed;
   while(at<trail.count){
    usize id=word_at(trail,at);Gate node=node_at(graph,id);
    if(value_at(values,id)==2&&node.kind==1){host.add_leaf(context,id);}
    at+=1u64;
   }
   u32 remaining=host.add_next(context);consumed=trail.count;
   if(host.additions(context)==before||host.contradictory(context)==0){
    u32 chosen=0;
    at=0u64;
    while(at<trail.count){
     usize id=word_at(trail,at);Gate node=node_at(graph,id);u32 either=node.kind==3 ? 1 : 0;
     if(node.kind>=2&&value_at(values,id)==either+1
        &&value_at(values,node.left)==0&&value_at(values,node.right)==0){
      Checkpoint checkpoint=Checkpoint(trail.count,host.additions(context),negative);
      host.push_clause(context,node.left,1-either,checkpoint);
      host.pending_push(context,node.left,either);chosen=1;break;
     }
     at+=1u64;
    }
    if(chosen!=0){continue;}
    u32 found=remaining;
    if(found==0){
     at=negative;
     while(at<trail.count){
      usize id=word_at(trail,at);Gate node=node_at(graph,id);
      if(node.kind==1&&value_at(values,id)==1){
       negative=at+1u64;host.begin_facts(context,id);found=1;break;
      }
      at+=1u64;
     }
    }
    if(found==0){return 1;}
    while(true){
     address fact=host.next_fact(context);
     if(address_bits(fact)==0u64){break;}
     FactView negated=negated_fact(host.inspect_fact(fact),host.facts);
     if(negated.present!=0){
      host.set_next_fact(context,negated);
      if(host.remaining(context)!=0u64){
       host.push_fact(context,fact,Checkpoint(trail.count,host.additions(context),negative));
      }
      chosen=1;break;
     }
    }
    if(chosen!=0){continue;}
   }
  }
  Alternative alternative=host.pop_alternative(context);
  if(alternative.kind==0){return 0;}
  host.pending_clear(context);
  Span trail=host.trail(context);usize count=trail.count;
  while(count>alternative.checkpoint.nodes){
   usize id=host.trail_pop(context);set_value(values,id,0);count-=1u64;
  }
  count=host.additions(context);
  while(count>alternative.checkpoint.facts){host.undo_addition(context);count-=1u64;}
  consumed=alternative.checkpoint.nodes;negative=alternative.checkpoint.negative;
  if(alternative.kind==1){host.pending_push(context,alternative.node,alternative.truth);}
 }
}

extern "C" void required_build(address root,address context,RequiredBuildHost host) on cpu
 requires nonzero(host.index) requires nonzero(host.count)
 requires nonzero(host.inspect) requires nonzero(host.define) {
 usize first=host.index(context,root);
 usize at=first;
 while(at<host.count(context)){
  SourceNode source=host.inspect(context,at);
  if(source.kind>=2){
   address child=source.right;
   if(source.kind==4&&source.truth==0){child=source.no;}
   usize a=host.index(context,source.left);usize b=host.index(context,child);
   host.define(context,at,source.kind==3 ? 3 : 2,a,b);
  }else{host.define(context,at,source.kind,0u64,0u64);}
  at+=1u64;
 }
}
RequiredNode required_node(RequiredGraph graph,usize index) on compute {
 return unsafe_position<RequiredNode>(unsafe_address(address_bits(graph.nodes)+u64(index)*u64(graph.stride)));
}
void required_remaining(RequiredGraph graph,usize index,usize count) on compute {
 unsafe_position<RequiredNode>(unsafe_address(address_bits(graph.nodes)+u64(index)*u64(graph.stride))).remaining=count;
}
u32 required_consume(RequiredGraph graph,usize index) on compute {
 usize remaining=required_node(graph,index).remaining-1u64;
 required_remaining(graph,index,remaining);
 return remaining==0u64 ? 1 : 0;
}
void required_abandon(RequiredGraph graph,address context,RequiredHost host,usize index) on cpu
 requires nonzero(host.push) requires nonzero(host.pop) requires nonzero(host.pending_count)
 requires nonzero(host.release) {
 usize before=host.pending_count(context);
 host.push(context,index);
 while(host.pending_count(context)>before){
  RequiredPending pending=host.pop(context);usize id=pending.index;
  RequiredNode node=required_node(graph,id);u32 last=required_consume(graph,id);
  u32 computed=host.release(context,id,last);
  if(last!=0&&computed==0&&node.kind>=2){host.push(context,node.left);host.push(context,node.right);}
 }
}
extern "C" void required_evaluate(RequiredGraph graph,address context,RequiredHost host) on cpu
 requires nonzero(host.reset) requires nonzero(host.status) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.pending_count) requires nonzero(host.impossible)
 requires nonzero(host.leaf) requires nonzero(host.union) requires nonzero(host.common)
 requires nonzero(host.valid_additions) requires nonzero(host.release) {
 usize at=0u64;
 while(at<graph.count){
  RequiredNode node=required_node(graph,at);
  required_remaining(graph,at,node.users);host.reset(context,at);at+=1u64;
 }
 host.push(context,0u64);
 while(true){
  RequiredPending pending=host.pop(context);
  if(pending.present==0){break;}
  usize id=pending.index;
  if(host.status(context,id)!=0){continue;}
  RequiredNode node=required_node(graph,id);
  if(node.kind==0){host.impossible(context,id);continue;}
  if(node.kind==1){host.leaf(context,id);continue;}
  u32 a=host.status(context,node.left);
  if(a==0){host.push(context,id);host.push(context,node.left);continue;}
  if(node.kind!=3&&a==1){
   u32 last=required_consume(graph,node.left);
   u32 computed=host.release(context,node.left,last);
   required_abandon(graph,context,host,node.right);
   host.impossible(context,id);continue;
  }
  u32 b=host.status(context,node.right);
  if(b==0){host.push(context,id);host.push(context,node.right);continue;}
  u32 valid=1;
  if(node.kind!=3){
   if(a!=2||b!=2){valid=0;}
   else if(required_node(graph,node.left).kind==3&&host.valid_additions(context,node.left)==0){valid=0;}
   else if(required_node(graph,node.right).kind==3&&host.valid_additions(context,node.right)==0){valid=0;}
  }
  u32 left_last=required_consume(graph,node.left);u32 right_last=required_consume(graph,node.right);
  if(node.kind==3){host.common(context,id,left_last,right_last);}
  else if(valid!=0){host.union(context,id,left_last,right_last);}
  else{
   u32 left_computed=host.release(context,node.left,left_last);
   u32 right_computed=host.release(context,node.right,right_last);
   host.impossible(context,id);
  }
 }
 required_remaining(graph,0u64,required_node(graph,0u64).remaining-1u64);
}

FactView fact_equal(FactView value,i32 order) on cpu {
 FactView key=value;key.kind=FactKind::Equal;
 if(order>i32(0)){address first=key.first;key.first=key.second;key.second=first;}
 return key;
}
extern "C" u32 cross_conflict(address context,CrossScalars scalars,CrossHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.begin) requires nonzero(host.next)
 requires nonzero(host.contains) requires nonzero(host.range) requires nonzero(host.compare) {
 u32 side=host.count(context,0)<host.count(context,1) ? 0 : 1;
 u32 other=1-side;host.begin(context,side);
 while(true){
  FactView fact=host.next(context);if(fact.present==0){return 0;}
  FactView key=fact;
  if(fact.kind==FactKind::Boolean||fact.kind==FactKind::Predicate){
   key.flag=1-fact.flag;if(host.contains(context,other,key)!=0){return 1;}
  }else if(fact.kind==FactKind::NonZero||fact.kind==FactKind::Zero){
   key.kind=fact.kind==FactKind::NonZero ? FactKind::Zero : FactKind::NonZero;
   if(host.contains(context,other,key)!=0){return 1;}
  }else if(fact.kind==FactKind::Index||fact.kind==FactKind::Outside){
   key.kind=fact.kind==FactKind::Index ? FactKind::Outside : FactKind::Index;
   if(host.contains(context,other,key)!=0){return 1;}
  }else if(fact.kind==FactKind::Below){
   key.first=fact.second;key.second=fact.first;key.flag=1;
   if(host.contains(context,other,key)!=0){return 1;}
   if(fact.flag!=0){
    key.flag=0;if(host.contains(context,other,key)!=0){return 1;}
    if(host.contains(context,other,fact_equal(fact,host.compare(fact.first,fact.second)))!=0){return 1;}
   }
  }else if(fact.kind==FactKind::Equal){
   key.kind=FactKind::NotEqual;if(host.contains(context,other,key)!=0){return 1;}
   if(host.compare(fact.first,fact.second)<=i32(0)){
    for(i in 0..4){
     key=fact;key.kind=FactKind::Below;key.flag=1;key.scalar=scalars.values[i];
     if(host.contains(context,other,key)!=0){return 1;}
     key.first=fact.second;key.second=fact.first;
     if(host.contains(context,other,key)!=0){return 1;}
    }
   }
  }else if(fact.kind==FactKind::NotEqual){
   if(host.contains(context,other,fact_equal(fact,host.compare(fact.first,fact.second)))!=0){return 1;}
  }else if(fact.kind==FactKind::Minimum){
   key.kind=FactKind::Maximum;FactView lower=key;lower.value=0u64;
   if(host.range(context,other,lower,key,0,0)!=0){return 1;}
  }else if(fact.kind==FactKind::Maximum){
   key.kind=FactKind::Minimum;FactView upper=key;upper.value=fact.mask;
   if(host.range(context,other,key,upper,1,1)!=0){return 1;}
  }
 }
}

extern "C" FactView negated_fact(FactView value,FactHost host) on cpu
 requires nonzero(host.compare) {
 FactView fact=value;
 if(fact.kind==FactKind::Minimum){
  if(fact.value==0u64){fact.present=0;}else{fact.kind=FactKind::Maximum;fact.value-=1u64;}
 }else if(fact.kind==FactKind::Maximum){
  if(fact.value==fact.mask){fact.present=0;}else{fact.kind=FactKind::Minimum;fact.value+=1u64;}
 }else if(fact.kind==FactKind::NonZero){fact.kind=FactKind::Zero;
 }else if(fact.kind==FactKind::Zero){fact.kind=FactKind::NonZero;
 }else if(fact.kind==FactKind::Boolean||fact.kind==FactKind::Predicate){fact.flag=1-fact.flag;
 }else if(fact.kind==FactKind::Equal){fact.kind=FactKind::NotEqual;
 }else if(fact.kind==FactKind::NotEqual){fact=fact_equal(fact,host.compare(fact.first,fact.second));
 }else if(fact.kind==FactKind::Below){
  address first=fact.first;fact.first=fact.second;fact.second=first;fact.flag=1-fact.flag;
 }else if(fact.kind==FactKind::Index){
  if(fact.fixed!=0){fact.kind=FactKind::Minimum;fact.scalar=host.unsigned32;}else{fact.kind=FactKind::Outside;}
 }else if(fact.kind==FactKind::Outside){fact.kind=FactKind::Index;}
 return fact;
}


extern "C" void negate_graph(address root,address context,NegateHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.has_result) requires nonzero(host.unconstrained) requires nonzero(host.impossible)
 requires nonzero(host.begin_facts) requires nonzero(host.next_fact) requires nonzero(host.either_fact)
 requires nonzero(host.either) requires nonzero(host.both) requires nonzero(host.facts.compare) {
 host.push(context,root,0);
 while(true){
  NegatePending pending=host.pop(context);if(pending.present==0){return;}
  if(host.has_result(context,pending.source)!=0){continue;}
  SourceNode source=host.inspect(pending.source);address right=source.right;
  if(source.kind==4&&source.truth==0){right=source.no;}
  if(pending.ready==0&&source.kind>=2){
   host.push(context,pending.source,1);host.push(context,right,0);host.push(context,source.left,0);continue;
  }
  if(source.kind==0){host.unconstrained(context,pending.source);
  }else if(source.kind==1){
   host.impossible(context,pending.source);host.begin_facts(context,pending.source);
   while(true){
    FactView fact=host.next_fact(context);if(fact.present==0){break;}
    FactView opposite=negated_fact(fact,host.facts);
    if(opposite.present!=0){host.either_fact(context,pending.source,opposite);}
   }
  }else if(source.kind==3){host.both(context,pending.source,source.left,right);
  }else{host.either(context,pending.source,source.left,right);}
 }
}
