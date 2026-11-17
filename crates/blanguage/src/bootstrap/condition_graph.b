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

FactView internal_key(FactKind kind,address identity,address scalar,u64 value) on cpu {
 address nil=unsafe_address(0u64);
 return FactView(1,kind,identity,nil,nil,scalar,value,0u64,0,0);
}
BitRange internal_bounds(address context,address identity,address scalar,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.contains) requires nonzero(host.compare)
 requires nonzero(host.same_scalar) requires nonzero(host.scalar) {
 InternalScalar info=host.scalar(scalar);
 FactView lower_fact=host.select(context,internal_key(FactKind::Minimum,identity,scalar,info.mask),1);
 FactView upper_fact=host.select(context,internal_key(FactKind::Maximum,identity,scalar,0u64),0);
 u64 lower=0u64;u64 upper=info.mask;
 if(lower_fact.present!=0&&lower_fact.kind==FactKind::Minimum
    &&host.compare(lower_fact.first,identity)==i32(0)&&host.same_scalar(lower_fact.scalar,scalar)!=0){lower=lower_fact.value;}
 if(upper_fact.present!=0&&upper_fact.kind==FactKind::Maximum
    &&host.compare(upper_fact.first,identity)==i32(0)&&host.same_scalar(upper_fact.scalar,scalar)!=0){upper=upper_fact.value;}
 if(info.kind==0&&host.contains(context,internal_key(FactKind::NonZero,identity,scalar,0u64))!=0){lower=max(lower,1u64);}
 if(host.contains(context,internal_key(FactKind::Zero,identity,scalar,0u64))!=0){lower=max(lower,info.bias);upper=min(upper,info.bias);}
 return BitRange(lower,upper);
}
BitRange internal_operand(address context,InternalOperand operand,address scalar,InternalScalar info,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.contains) requires nonzero(host.compare)
 requires nonzero(host.same_scalar) requires nonzero(host.scalar) {
 if(operand.is_constant!=0){
  u64 value=info.kind==2 ? float_order(info.width,operand.bits) : operand.bits^info.bias;
  return BitRange(value,value);
 }
 return internal_bounds(context,operand.identity,scalar,host);
}
u32 internal_not_equal(address context,FactView fact,CrossScalars integers,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.contains) requires nonzero(host.compare)
 requires nonzero(host.same_scalar) requires nonzero(host.scalar) {
 if(host.compare(fact.first,fact.second)==i32(0)){return 1;}
 address nil=unsafe_address(0u64);
 u32 left_zero=host.contains(context,internal_key(FactKind::Zero,fact.first,nil,0u64));
 u32 right_zero=host.contains(context,internal_key(FactKind::Zero,fact.second,nil,0u64));
 if(left_zero!=0&&right_zero!=0){return 1;}
 for(truth in 0..2){
  FactView left=internal_key(FactKind::Boolean,fact.first,nil,0u64);left.flag=truth;
  FactView right=left;right.first=fact.second;
  if(host.contains(context,left)!=0&&host.contains(context,right)!=0){return 1;}
 }
 for(i in 0..4){
  address scalar=integers.values[i];InternalScalar info=host.scalar(scalar);
  BitRange left=left_zero!=0 ? BitRange(info.bias,info.bias) : internal_bounds(context,fact.first,scalar,host);
  BitRange right=right_zero!=0 ? BitRange(info.bias,info.bias) : internal_bounds(context,fact.second,scalar,host);
  if(left.lower==left.upper&&right.lower==right.upper&&left.lower==right.lower){return 1;}
 }
 return 0;
}
extern "C" u32 internal_conflict(address context,CrossScalars integers,InternalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.select) requires nonzero(host.contains)
 requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar)
 requires nonzero(host.predicate) requires nonzero(host.upper) {
 while(true){
  FactView fact=host.next(context);if(fact.present==0){return 0;}
  FactView key=fact;
  if(fact.kind==FactKind::Boolean||fact.kind==FactKind::Predicate){
   key.flag=1-fact.flag;if(host.contains(context,key)!=0){return 1;}
   if(fact.kind==FactKind::Predicate){
    InternalPredicate predicate=host.predicate(fact.payload);InternalScalar info=host.scalar(predicate.scalar);
    BitRange left=internal_operand(context,predicate.left,predicate.scalar,info,host);
    BitRange right=internal_operand(context,predicate.right,predicate.scalar,info,host);
    if(info.kind==2){
     if(float_comparison_possible(predicate.operation,info.width,left,right,fact.flag)==0){return 1;}
    }else if(left.lower==left.upper&&right.lower==right.upper){
     u64 result=binary(predicate.operation,info.width,info.kind==1 ? 1 : 0,left.lower^info.bias,right.lower^info.bias);
     if(result!=u64(fact.flag)){return 1;}
    }
   }
  }else if(fact.kind==FactKind::Zero||fact.kind==FactKind::NonZero){
   key.kind=fact.kind==FactKind::Zero ? FactKind::NonZero : FactKind::Zero;
   if(host.contains(context,key)!=0){return 1;}
  }else if(fact.kind==FactKind::Index||fact.kind==FactKind::Outside){
   if(fact.kind==FactKind::Outside){
    InternalUpper upper=host.upper(fact.payload);
    if(upper.present!=0&&upper.distance==0&&host.compare(fact.first,upper.identity)==i32(0)){return 1;}
   }
   key.kind=fact.kind==FactKind::Index ? FactKind::Outside : FactKind::Index;
   if(host.contains(context,key)!=0){return 1;}
  }else if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){
   BitRange bounds=internal_bounds(context,fact.first,fact.scalar,host);InternalScalar info=host.scalar(fact.scalar);
   if(bounds.lower>bounds.upper){return 1;}
   key.kind=FactKind::Zero;
   if(host.contains(context,key)!=0&&!(bounds.lower<=info.bias&&info.bias<=bounds.upper)){return 1;}
   key.kind=FactKind::NonZero;
   if(host.contains(context,key)!=0&&bounds.lower==bounds.upper&&bounds.lower==info.bias){return 1;}
  }else if(fact.kind==FactKind::NotEqual){
   if(internal_not_equal(context,fact,integers,host)!=0){return 1;}
  }else if(fact.kind==FactKind::Below){
   if(fact.flag!=0&&host.compare(fact.first,fact.second)==i32(0)){return 1;}
   key.first=fact.second;key.second=fact.first;key.flag=1;
   if(host.contains(context,key)!=0){return 1;}
   if(fact.flag!=0){key.flag=0;if(host.contains(context,key)!=0){return 1;}}
   BitRange left=internal_bounds(context,fact.first,fact.scalar,host);
   BitRange right=internal_bounds(context,fact.second,fact.scalar,host);
   if(left.lower>right.upper||(fact.flag!=0&&left.lower==right.upper)){return 1;}
  }
 }
}

i32 common_limit_order(FactView left,FactView right,
 cfn<i32,address,address> ensures memory(read) compare,
 cfn<i32,address,address> ensures memory(read) scalar_compare) on cpu
 requires nonzero(compare) requires nonzero(scalar_compare) {
 if(left.kind!=right.kind){return left.kind==FactKind::Minimum ? i32(-1) : i32(1);}
 i32 order=compare(left.first,right.first);
 return order!=i32(0) ? order : scalar_compare(left.scalar,right.scalar);
}
CommonLimit common_limit_next(address context,u32 side,FactView carry,
 cfn<FactView,address,u32> side_next,
 cfn<i32,address,address> ensures memory(read) compare,
 cfn<i32,address,address> ensures memory(read) scalar_compare) on cpu
 requires nonzero(side_next) requires nonzero(compare) requires nonzero(scalar_compare) {
 FactView value=carry;
 while(value.present!=0&&value.kind!=FactKind::Minimum&&value.kind!=FactKind::Maximum){value=side_next(context,side);}
 if(value.present==0){return CommonLimit(value,value);}
 FactView next=side_next(context,side);
 while(next.present!=0){
  if(next.kind!=FactKind::Minimum&&next.kind!=FactKind::Maximum){next=side_next(context,side);continue;}
  if(common_limit_order(value,next,compare,scalar_compare)!=i32(0)){break;}
  value.value=value.kind==FactKind::Minimum ? max(value.value,next.value) : min(value.value,next.value);
  next=side_next(context,side);
 }
 return CommonLimit(value,next);
}
extern "C" void common_facts(address context,CommonHost host) on cpu
 requires nonzero(host.intersection) requires nonzero(host.root_next) requires nonzero(host.representative)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.union_next)
 requires nonzero(host.known) requires nonzero(host.known_bits) requires nonzero(host.contains_known)
 requires nonzero(host.emit_known) requires nonzero(host.side_next) requires nonzero(host.range)
 requires nonzero(host.compare) requires nonzero(host.scalar_compare) requires nonzero(host.scalar) {
 host.intersection(context);
 while(true){
  CommonRoot root=host.root_next(context);if(root.present==0){break;}
  if(root.matched==0){continue;}
  address representative=root.left;
  if(host.compare(root.left,root.right)==i32(0)){
   if(host.compare(root.identity,root.left)==i32(0)){continue;}
  }else{
   representative=host.representative(context,root.left,root.right,root.identity);
   if(address_bits(representative)==0u64){continue;}
  }
  FactView fact=internal_key(FactKind::Equal,representative,unsafe_address(0u64),0u64);fact.second=root.identity;
  fact=fact_equal(fact,host.compare(fact.first,fact.second));
  if(host.contains(context,fact)==0){host.emit(context,fact);}
 }
 address previous_identity=unsafe_address(0u64);address previous_scalar=unsafe_address(0u64);u32 have_previous=0;
 while(true){
  FactView fact=host.union_next(context);if(fact.present==0){break;}
  if(fact.kind!=FactKind::Index){continue;}
  CommonKnown known=host.known(fact.payload);if(known.present==0){continue;}
  if(have_previous!=0&&host.compare(previous_identity,fact.first)==i32(0)&&host.scalar_compare(previous_scalar,known.scalar)==i32(0)){continue;}
  previous_identity=fact.first;previous_scalar=known.scalar;have_previous=1;
  CommonBits left=host.known_bits(context,0,fact.first,known.scalar);
  if(left.present==0){continue;}
  CommonBits right=host.known_bits(context,1,fact.first,known.scalar);
  if(right.present==0){continue;}
  InternalScalar scalar=host.scalar(known.scalar);
  BitPattern bits=known_common(scalar.mask,BitPattern(left.mask,left.value),scalar.mask,scalar.bias,BitPattern(right.mask,right.value));
  if(bits.mask!=0u64&&host.contains_known(context,fact.first,known.scalar,bits.mask,bits.value)==0){host.emit_known(context,fact.first,known.scalar,bits.mask,bits.value);}
 }
 CommonLimit left=common_limit_next(context,0,host.side_next(context,0),host.side_next,host.compare,host.scalar_compare);
 if(left.value.present==0){return;}
 CommonLimit right=common_limit_next(context,1,host.side_next(context,1),host.side_next,host.compare,host.scalar_compare);
 while(left.value.present!=0&&right.value.present!=0){
  i32 order=common_limit_order(left.value,right.value,host.compare,host.scalar_compare);
  if(order<i32(0)){left=common_limit_next(context,0,left.next,host.side_next,host.compare,host.scalar_compare);
  }else if(order>i32(0)){right=common_limit_next(context,1,right.next,host.side_next,host.compare,host.scalar_compare);
  }else{
   FactView fact=left.value;FactView lower=fact;FactView upper=fact;
   if(fact.kind==FactKind::Maximum){fact.value=max(fact.value,right.value.value);lower.value=0u64;upper.value=fact.value;
   }else{fact.value=min(fact.value,right.value.value);lower.value=fact.value;upper.value=fact.mask;}
   if(host.range(context,lower,upper)==0){host.emit(context,fact);}
   if(left.next.present==0||right.next.present==0){return;}
   left=common_limit_next(context,0,left.next,host.side_next,host.compare,host.scalar_compare);right=common_limit_next(context,1,right.next,host.side_next,host.compare,host.scalar_compare);
  }
 }
}

extern "C" void task_return_sources(address context,TaskSourceHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.inspect) requires nonzero(host.begin)
 requires nonzero(host.next) requires nonzero(host.contains) requires nonzero(host.lookup)
 requires nonzero(host.unknown) requires nonzero(host.empty) requires nonzero(host.single)
 requires nonzero(host.copy) requires nonzero(host.intersection) requires nonzero(host.union)
 requires nonzero(host.same_scalar) requires nonzero(host.facts.compare) {
 usize count=host.count(context);
 usize at=0u64;
 while(at<count){
  usize index=at;at+=1u64;
  SourceNode node=host.inspect(context,index);
  if(node.kind==0){host.empty(context,index);continue;}
  if(node.kind==1){
   host.begin(context,index);u32 found=0;u32 value=0;
   while(true){
    FactView fact=host.next(context);
    if(fact.present==0){break;}
    if(fact.kind!=FactKind::Minimum){continue;}
    if(host.facts.compare(fact.first,host.identity)!=0){continue;}
    if(host.same_scalar(fact.scalar,host.facts.unsigned32)==0){continue;}
    FactView maximum=fact;maximum.kind=FactKind::Maximum;
    if(host.contains(context,maximum)!=0){found=1;value=u32(fact.value);break;}
   }
   if(found!=0){host.single(context,index,value);}else{host.unknown(context,index);}
   continue;
  }
  address right=node.right;
  if(node.kind==4&&node.truth==0){right=node.no;}
  TaskSourceSet left=host.lookup(context,node.left);
  TaskSourceSet other=host.lookup(context,right);
  if(node.kind==3){
   if(left.known!=0&&other.known!=0){host.union(context,index,left.values,other.values);}
   else{host.unknown(context,index);}
  }else{
   if(left.known!=0&&other.known!=0){host.intersection(context,index,left.values,other.values);}
   else if(left.known!=0){host.copy(context,index,left.values);}
   else if(other.known!=0){host.copy(context,index,other.values);}
   else{host.unknown(context,index);}
  }
 }
}

bool clause_direct(SourceNode source,address other,cfn<u32,address,address> ensures memory(read) same) on cpu
 requires nonzero(same) {
 if(source.kind!=2&&source.kind!=4){return false;}
 address right=source.right;if(source.kind==4&&source.truth==0){right=source.no;}
 return same(source.left,other)!=0||same(right,other)!=0;
}
address clause_joined(address left,address right,
 cfn<SourceNode,address> ensures memory(read) inspect,
 cfn<u32,address,address> ensures memory(read) same,
 cfn<u32,address,address> ensures memory(read) same_condition) on cpu
 requires nonzero(inspect) requires nonzero(same) requires nonzero(same_condition) {
 SourceNode a=inspect(left);SourceNode b=inspect(right);
 if(a.kind==4&&b.kind==4&&a.truth!=b.truth
    &&same(a.left,b.left)!=0&&same_condition(a.condition,b.condition)!=0){return a.left;}
 return address();
}
extern "C" void clause_facts(address facts,address output,ClauseLeafHost host) on cpu
 requires nonzero(host.conflict) requires nonzero(host.impossible) requires nonzero(host.make) {
 if(host.conflict(facts)!=0){host.impossible(output);}else{host.make(output,facts);}
}
extern "C" void clause_both(address left,address right,address output,ClauseBothHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.same) requires nonzero(host.unite)
 requires nonzero(host.impossible) requires nonzero(host.copy) requires nonzero(host.make) {
 SourceNode a=host.inspect(left);SourceNode b=host.inspect(right);
 if(a.kind==0||b.kind==0){host.impossible(output);return;}
 if(a.kind==1&&a.empty!=0){host.copy(output,right);return;}
 if(b.kind==1&&b.empty!=0){host.copy(output,left);return;}
 if(a.kind==1&&b.kind==1){host.unite(output,a.left,b.left);return;}
 if(host.same(left,right)!=0||clause_direct(a,right,host.same)){host.copy(output,left);return;}
 if(clause_direct(b,left,host.same)){host.copy(output,right);return;}
 host.make(output,left,right);
}
extern "C" void clause_either(address left,address right,address output,ClauseEitherHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.same) requires nonzero(host.same_condition)
 requires nonzero(host.subset) requires nonzero(host.copy) requires nonzero(host.make) requires nonzero(host.factor) {
 address parent=clause_joined(left,right,host.inspect,host.same,host.same_condition);
 if(parent!=address()){host.copy(output,parent);return;}
 SourceNode a=host.inspect(left);SourceNode b=host.inspect(right);
 if(a.kind==3){
  parent=clause_joined(a.right,right,host.inspect,host.same,host.same_condition);
  if(parent!=address()){clause_either(a.left,parent,output,host);return;}
  parent=clause_joined(a.left,right,host.inspect,host.same,host.same_condition);
  if(parent!=address()){clause_either(parent,a.right,output,host);return;}
 }
 if(b.kind==3){
  parent=clause_joined(left,b.left,host.inspect,host.same,host.same_condition);
  if(parent!=address()){clause_either(parent,b.right,output,host);return;}
  parent=clause_joined(left,b.right,host.inspect,host.same,host.same_condition);
  if(parent!=address()){clause_either(b.left,parent,output,host);return;}
 }
 if(a.kind==0){host.copy(output,right);return;}
 if(b.kind==0){host.copy(output,left);return;}
 if(a.kind==1&&a.empty!=0){host.copy(output,left);return;}
 if(b.kind==1&&b.empty!=0){host.copy(output,right);return;}
 if(a.kind==1&&b.kind==1){
  if(host.subset(a.left,b.left)!=0){host.copy(output,left);return;}
  if(host.subset(b.left,a.left)!=0){host.copy(output,right);return;}
 }
 if(host.same(left,right)!=0){host.copy(output,left);return;}
 if(clause_direct(a,right,host.same)){host.copy(output,right);return;}
 if(clause_direct(b,left,host.same)){host.copy(output,left);return;}
 if(a.kind==2&&b.kind==2){
  if(host.same(a.left,b.left)!=0){host.factor(output,a.left,a.right,b.right);return;}
  if(host.same(a.left,b.right)!=0){host.factor(output,a.left,a.right,b.left);return;}
  if(host.same(a.right,b.left)!=0){host.factor(output,a.right,a.left,b.right);return;}
  if(host.same(a.right,b.right)!=0){host.factor(output,a.right,a.left,b.left);return;}
 }
 host.make(output,left,right);
}
extern "C" void clause_branch(address parent,address condition,u32 truth,address output,ClauseBranchHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.case) requires nonzero(host.impossible) requires nonzero(host.make) {
 if(host.inspect(parent).kind==0||host.inspect(host.case(condition,truth)).kind==0){host.impossible(output);return;}
 host.make(output,parent,condition,truth);
}

extern "C" void equality_merge_value(address current,address merged,address root,EqualityIdentityHost host) on cpu
 requires nonzero(host.compare) requires nonzero(host.assign) {
 if(host.compare(current,merged)==0){host.assign(current,root);}
}
extern "C" u32 equality_extend(address context,EqualityHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.compare)
 requires nonzero(host.merge) requires nonzero(host.insert) {
 u32 changed=0;
 while(true){
  FactView fact=host.next(context);
  if(fact.present==0||fact.kind!=FactKind::Equal){return changed;}
  EqualityLookup left=host.lookup(context,0,fact.first);
  EqualityLookup right=host.lookup(context,1,fact.second);
  i32 order=host.compare(left.root,right.root);
  address root=order<=0 ? left.root : right.root;
  if(order!=0){
   address merged=order<=0 ? right.root : left.root;
   changed=1;host.merge(context,merged,root);
  }
  if(left.present==0){host.insert(context,fact.first,root);}
  if(right.present==0){host.insert(context,fact.second,root);}
 }
}
extern "C" void computed_equalities(address context,ComputedEqualityHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.expression) requires nonzero(host.collect)
 requires nonzero(host.begin) requires nonzero(host.step) requires nonzero(host.produced)
 requires nonzero(host.remember) requires nonzero(host.equate) requires nonzero(host.has_equalities)
 requires nonzero(host.extend) requires nonzero(host.compare) {
 while(true){
  FactView fact=host.next(context);if(fact.present==0){break;}
  if(fact.kind!=FactKind::Index){continue;}
  address expression=host.expression(fact.payload);
  if(expression!=address()){host.collect(context,fact.first,expression);}
 }
 while(true){
  host.begin(context);
  while(true){
   ComputedEqualityStep step=host.step(context);if(step.present==0){break;}
   address previous=host.produced(context);
   if(previous==address()){host.remember(context);}
   else if(host.compare(previous,step.result)!=0){host.equate(context,previous,step.result);}
  }
  if(host.has_equalities(context)==0){return;}
  host.extend(context);
 }
}
extern "C" void additional_equalities(address context,AdditionalEqualityHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.expression)
 requires nonzero(host.compare) requires nonzero(host.own) requires nonzero(host.extend)
 requires nonzero(host.computed) requires nonzero(host.unchanged) requires nonzero(host.borrow) {
 u32 changes=0;
 while(true){
  FactView fact=host.next(context);if(fact.present==0){break;}
  if(fact.kind==FactKind::Equal){
   address left=host.lookup(context,fact.first);address right=host.lookup(context,fact.second);
   if(left==address()||right==address()){changes=1;break;}
   if(host.compare(left,right)!=0){changes=1;break;}
  }else if(fact.kind==FactKind::Index&&host.expression(fact.payload)!=address()){changes=1;break;}
 }
 if(changes==0){return;}
 host.own(context);host.extend(context);host.computed(context);
 if(host.unchanged(context)!=0){host.borrow(context);}
}
extern "C" u32 alias_facts(address context,usize members,u32 inherited,AliasFactHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.previous)
 requires nonzero(host.group) requires nonzero(host.member) requires nonzero(host.key)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.has_inferred)
 requires nonzero(host.compare) {
 if(members==0u64){return 0;}
 u32 side=0;
 while(side<1+inherited){
  while(true){
   FactView fact=host.next(context,side);if(fact.present==0){break;}
   if(fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual
      ||fact.kind==FactKind::Below||fact.kind==FactKind::Predicate){continue;}
   address identity=fact.first;address root=host.root(context,identity);if(root==address()){continue;}
   address previous=address();if(side!=0){previous=host.previous(context,identity);}
   if(previous!=address()&&host.compare(previous,identity)!=0){continue;}
   host.group(context,root);
   while(true){
    address member=host.member(context);if(member==address()){break;}
    if(host.compare(member,identity)==0){continue;}
    if(previous!=address()){
     address old=host.previous(context,member);
     if(old!=address()&&host.compare(old,previous)==0){continue;}
    }
    address key=host.key(context,fact,member);
    if(host.contains(context,0,key)==0&&host.contains(context,1,key)==0){host.emit(context,key);}
   }
  }
  side+=1;
 }
 return host.has_inferred(context);
}

extern "C" CanonicalInsertion canonical_insert(address context,address candidate,u32 empty,CanonicalInsertHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.subset) requires nonzero(host.prepare)
 requires nonzero(host.reset) requires nonzero(host.forward) {
 while(true){
  CanonicalTerm old=host.next(context);if(old.present==0){break;}
  if(host.subset(old.value,candidate)!=0){return CanonicalInsertion::Reject;}
 }
 if(empty!=0){return CanonicalInsertion::Insert;}
 host.prepare(context);host.reset(context);
 while(true){
  CanonicalTerm old=host.next(context);if(old.present==0){break;}
  if(host.forward(context,old.value)==0){return CanonicalInsertion::RejectPrepared;}
 }
 return CanonicalInsertion::RetainInsert;
}
extern "C" u32 canonical_keep(address old,address candidate,address incoming,CanonicalKeepHost host) on cpu
 requires nonzero(host.subset) requires nonzero(host.reverse) {
 if(host.subset(candidate,old)!=0){return 0;}
 return host.reverse(old,incoming);
}
extern "C" void canonical_normalize(address context,CanonicalNormalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reset) requires nonzero(host.bound)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.compare)
 requires nonzero(host.scalar) requires nonzero(host.upper) {
 while(true){
  FactView fact=host.next(context);if(fact.present==0){break;}
  if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){continue;}
  if(fact.kind==FactKind::Below&&fact.flag==0&&host.compare(fact.first,fact.second)==0){
   if(host.scalar(fact.scalar).kind<2){continue;}
  }
  if(fact.kind==FactKind::Index){
   InternalUpper upper=host.upper(fact.payload);
   if(upper.present!=0&&upper.distance==0&&host.compare(fact.first,upper.identity)==0){continue;}
  }
  if(fact.kind==FactKind::NonZero){
   host.reset(context,0);u32 redundant=0;
   while(true){
    FactView bound=host.bound(context,0);if(bound.present==0){break;}
    if(bound.value>1u64&&host.compare(fact.first,bound.first)==0&&host.scalar(bound.scalar).kind==0){redundant=1;break;}
   }
   if(redundant!=0){continue;}
  }
  host.emit(context,fact);
 }
 u32 side=0;
 while(side<2){
  host.reset(context,side);
  while(true){
   FactView bound=host.bound(context,side);if(bound.present==0){break;}
   if(side==0&&bound.value==0u64){continue;}
   if(side==1&&bound.value==bound.mask){continue;}
   InternalScalar scalar=host.scalar(bound.scalar);
   FactView zero=bound;zero.kind=FactKind::Zero;
   if(scalar.kind<2&&host.contains(context,zero)!=0){continue;}
   if(scalar.kind==0){
    if(side==0&&bound.value==1u64){bound.kind=FactKind::NonZero;}
    if(side==1&&bound.value==0u64){bound.kind=FactKind::Zero;}
   }
   host.emit(context,bound);
  }
  side+=1;
 }
}
extern "C" void canonical_accept(address source,address target,CanonicalAcceptHost host) on cpu
 requires nonzero(host.conflict) requires nonzero(host.normalize) {
 if(host.conflict(source)==0){host.normalize(source,target);}
}
extern "C" void canonical_either_terms(address context,CanonicalEitherHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.insert) {
 while(true){CanonicalTerm term=host.next(context);if(term.present==0){return;}host.insert(context,term.value);}
}
extern "C" void canonical_product(address context,CanonicalProductHost host) on cpu
 requires nonzero(host.left) requires nonzero(host.reset) requires nonzero(host.right) requires nonzero(host.union) {
 while(true){
  CanonicalTerm left=host.left(context);if(left.present==0){return;}
  host.reset(context);
  while(true){CanonicalTerm right=host.right(context);if(right.present==0){break;}host.union(context,left.value,right.value);}
 }
}
extern "C" void canonical_graph(address context,CanonicalGraphHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.inspect) requires nonzero(host.lookup)
 requires nonzero(host.empty) requires nonzero(host.leaf) requires nonzero(host.either) requires nonzero(host.both) {
 usize count=host.count(context);
 usize at=0u64;
 while(at<count){
  usize index=at;at+=1u64;
  SourceNode node=host.inspect(context,index);
  if(node.kind==0){host.empty(context,index);continue;}
  if(node.kind==1){host.leaf(context,index,node.left);continue;}
  address right=node.right;if(node.kind==4&&node.truth==0){right=node.no;}
  address left=host.lookup(context,node.left);address other=host.lookup(context,right);
  if(node.kind==3){host.either(context,index,left,other);}else{host.both(context,index,left,other);}
 }
}
extern "C" void canonical_consensus(address context,CanonicalConsensusHost host) on cpu
 requires nonzero(host.left) requires nonzero(host.right_begin) requires nonzero(host.right)
 requires nonzero(host.facts_begin) requires nonzero(host.fact) requires nonzero(host.contains)
 requires nonzero(host.resolve) requires nonzero(host.facts.compare) {
 while(true){
  CanonicalTerm left=host.left(context);if(left.present==0){return;}
  host.right_begin(context,left.value);
  while(true){
   CanonicalTerm right=host.right(context);if(right.present==0){break;}
   host.facts_begin(context,left.value);
   while(true){
    FactView fact=host.fact(context);if(fact.present==0){break;}
    FactView other=negated_fact(fact,host.facts);
    if(other.present!=0&&host.contains(right.value,other)!=0){host.resolve(context,left.value,right.value,fact,other);}
   }
  }
 }
}
extern "C" u32 canonical_merge_additions(address context,CanonicalMergeHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.insert) {
 u32 changed=0;while(host.next(context)!=0){changed|=host.insert(context);}return changed;
}
extern "C" void canonical_closure(address context,CanonicalClosureHost host) on cpu
 requires nonzero(host.round) {
 while(host.round(context)!=0){}
}
extern "C" void canonical_fold(address context,CanonicalFoldHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.append) {
 while(host.next(context)!=0){host.append(context);}
}

bool contract_end_equal(ContractEnd a,ContractEnd b) on cpu {return a.high==b.high&&a.low==b.low;}
bool contract_end_less(ContractEnd a,ContractEnd b) on cpu {return a.high<b.high||(a.high==b.high&&a.low<b.low);}
extern "C" usize contract_node(address context,ContractNodeHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.last) requires nonzero(host.extend) requires nonzero(host.push) requires nonzero(host.count) requires nonzero(host.intern) {
 while(true){
  ContractCell next=host.next(context);if(next.present==0){break;}
  ContractCell previous=host.last(context);
  if(previous.present!=0&&previous.value==next.value){host.extend(context,next.end);}else{host.push(context,next);}
 }
 if(host.count(context)==1u64){return host.last(context).value;}
 return host.intern(context);
}

extern "C" u32 contract_pairs(address context,usize left,usize right,ContractPairHost host) on cpu
 requires nonzero(host.compare) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.push) {
 i32 order=host.compare(context);usize count=host.count(context,order<=0 ? 0 : 1);
 if(order!=0){
  usize at=0u64;while(at<count){ContractCell cell=host.cell(context,order<0 ? 0 : 1,at);at+=1u64;
   host.push(context,cell.end,order<0 ? cell.value : left,order<0 ? right : cell.value);
  }return order<0 ? 0 : 1;
 }
 usize a=0u64;usize b=0u64;usize right_count=host.count(context,1);
 while(a<count&&b<right_count){
  ContractCell x=host.cell(context,0,a);ContractCell y=host.cell(context,1,b);
  ContractEnd end=contract_end_less(x.end,y.end) ? x.end : y.end;host.push(context,end,x.value,y.value);
  if(contract_end_equal(x.end,end)){a+=1u64;}if(contract_end_equal(y.end,end)){b+=1u64;}
 }return 0;
}

ContractOperation contract_key(usize a,usize b,u32 either,u32 ready) on cpu {
 return ContractOperation(1,ready,either,a<b ? a : b,a<b ? b : a);
}
extern "C" usize contract_operation(address context,usize left,usize right,u32 either,ContractOperationHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.lookup) requires nonzero(host.remember) requires nonzero(host.prepare) requires nonzero(host.count) requires nonzero(host.pair) requires nonzero(host.cell) requires nonzero(host.make) requires nonzero(host.release) {
 ContractOperation root=contract_key(left,right,either,0);host.push(context,root);
 while(true){
  ContractOperation work=host.pop(context);if(work.present==0){break;}
  if(host.lookup(context,work.either,work.left,work.right).present!=0){continue;}
  usize a=work.left;usize b=work.right;u32 simple=1;usize value=0u64;
  if(a==b){value=a;}else if(work.either!=0){
   if(a==1u64||b==1u64){value=1u64;}else if(a==0u64){value=b;}else{simple=0;}
  }else if(a==0u64){value=0u64;}else if(a==1u64){value=b;}else{simple=0;}
  if(simple!=0){host.remember(context,work.either,a,b,value);continue;}
  host.prepare(context,a,b);usize count=host.count(context);
  if(work.ready==0){
   work.ready=1;host.push(context,work);
   while(count!=0u64){count-=1u64;ContractPair pair=host.pair(context,count);host.push(context,contract_key(pair.left,pair.right,work.either,0));}
  }else{
   usize at=0u64;while(at<count){ContractPair pair=host.pair(context,at);at+=1u64;
    ContractOperation key=contract_key(pair.left,pair.right,work.either,0);host.cell(context,pair.end,host.lookup(context,key.either,key.left,key.right).value);
   }value=host.make(context);host.remember(context,work.either,a,b,value);
  }host.release(context);
 }return host.lookup(context,root.either,root.left,root.right).value;
}

extern "C" usize contract_not(address context,usize root,ContractNotHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.lookup) requires nonzero(host.remember) requires nonzero(host.load) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.emit) requires nonzero(host.make) requires nonzero(host.release) {
 host.push(context,root,0);
 while(true){
  ContractPending work=host.pop(context);if(work.present==0){break;}
  if(host.lookup(context,work.index).present!=0){continue;}
  host.load(context,work.index);usize count=host.count(context);
  if(work.ready==0){
   host.push(context,work.index,1);
   while(count!=0u64){count-=1u64;host.push(context,host.cell(context,count).value,0);}
  }else{
   usize at=0u64;while(at<count){ContractCell cell=host.cell(context,at);at+=1u64;host.emit(context,cell.end,host.lookup(context,cell.value).value);}
   usize inverse=host.make(context);host.remember(context,work.index,inverse);host.remember(context,inverse,work.index);
  }host.release(context);
 }return host.lookup(context,root).value;
}

// Semantic helpers share the original typed normalizer and actual scalar fields.
usize contract_cut(address context,address identity,address scalar,u64 upper,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 if(upper>=host.scalar(scalar).mask){return 1u64;}return host.range(context,identity,scalar,upper);
}
usize contract_minimum(address context,address identity,address scalar,u64 lower,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 if(lower==0u64){return 1u64;}return host.invert(context,contract_cut(context,identity,scalar,lower-1u64,host));
}
usize contract_zero(address context,address identity,address scalar,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 u64 zero=host.scalar(scalar).bias;usize upper=contract_cut(context,identity,scalar,zero,host);
 usize lower=contract_minimum(context,identity,scalar,zero,host);return host.operation(context,lower,upper,0);
}
usize contract_less(address context,address a,address b,address scalar,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 if(host.compare(a,b)==0){return 0u64;}
 FactView fact=FactView(1,FactKind::Below,a,b,address(),scalar,0u64,0u64,1,0);
 usize relation=host.atom(context,fact);usize before=contract_cut(context,a,scalar,host.scalar(scalar).mask-1u64,host);
 usize after=contract_minimum(context,b,scalar,1u64,host);relation=host.operation(context,relation,before,0);return host.operation(context,relation,after,0);
}
ContractIndex contract_comparison_less(address context,InternalOperand a,InternalOperand b,address scalar,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 ContractIndex result=ContractIndex(1,0u64);InternalScalar info=host.scalar(scalar);
 if(a.is_constant==0&&b.is_constant==0){result.value=contract_less(context,a.identity,b.identity,scalar,host);return result;}
 if(a.is_constant==0&&b.is_constant!=0&&host.same_scalar(b.scalar,scalar)!=0){u64 order=(b.bits&info.mask)^info.bias;
  if(order!=0u64){result.value=contract_cut(context,a.identity,scalar,order-1u64,host);}return result;
 }
 if(a.is_constant!=0&&b.is_constant==0&&host.same_scalar(a.scalar,scalar)!=0){u64 order=(a.bits&info.mask)^info.bias;
  if(order!=info.mask){result.value=contract_minimum(context,b.identity,scalar,order+1u64,host);}return result;
 }
 if(a.is_constant!=0&&b.is_constant!=0&&host.same_scalar(a.scalar,scalar)!=0&&host.same_scalar(b.scalar,scalar)!=0){
  result.value=((a.bits&info.mask)^info.bias)<((b.bits&info.mask)^info.bias) ? 1u64 : 0u64;return result;
 }result.present=0;return result;
}
ContractIndex contract_compare(address context,BinaryOperation operation,address scalar,InternalOperand a,InternalOperand b,ContractSemanticHost host) on cpu requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 ContractIndex result=ContractIndex(0,0u64);if(host.scalar(scalar).kind>=2){return result;}
 if(operation==BinaryOperation::Less){return contract_comparison_less(context,a,b,scalar,host);}
 if(operation==BinaryOperation::Greater){return contract_comparison_less(context,b,a,scalar,host);}
 if(operation==BinaryOperation::LessEqual||operation==BinaryOperation::GreaterEqual){
  result=operation==BinaryOperation::LessEqual ? contract_comparison_less(context,b,a,scalar,host) : contract_comparison_less(context,a,b,scalar,host);
  if(result.present!=0){result.value=host.invert(context,result.value);}return result;
 }
 result=contract_comparison_less(context,a,b,scalar,host);if(result.present==0){return result;}
 ContractIndex right=contract_comparison_less(context,b,a,scalar,host);if(right.present==0){return right;}
 result.value=host.operation(context,result.value,right.value,1);
 if(operation==BinaryOperation::Equal){result.value=host.invert(context,result.value);}return result;
}
InternalOperand contract_value(address identity) on cpu {return InternalOperand(identity,0u64,address(),0);}
extern "C" usize contract_fact(address context,FactView input,ContractSemanticHost host) on cpu
 requires nonzero(host.atom) requires nonzero(host.range) requires nonzero(host.operation) requires nonzero(host.invert) requires nonzero(host.integer) requires nonzero(host.boolean) requires nonzero(host.length) requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar) requires nonzero(host.predicate) requires nonzero(host.domain) {
 FactView fact=input;
 if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){
  if(host.scalar(fact.scalar).kind==0){
   if(fact.kind==FactKind::Minimum&&fact.value==1u64){fact.kind=FactKind::NonZero;}
   else if(fact.kind==FactKind::Maximum&&fact.value==0u64){fact.kind=FactKind::Zero;}
  }
 }
 if(fact.kind==FactKind::Minimum){return contract_minimum(context,fact.first,fact.scalar,fact.value,host);}
 if(fact.kind==FactKind::Maximum){return contract_cut(context,fact.first,fact.scalar,fact.value,host);}
 if(fact.kind==FactKind::Zero||fact.kind==FactKind::NonZero){
  address scalar=host.integer(context,fact.first,0);
  if(scalar!=address()){usize zero=contract_zero(context,fact.first,scalar,host);return fact.kind==FactKind::Zero ? zero : host.invert(context,zero);}
  if(host.boolean(context,fact.first)!=0){FactView truth=fact;truth.kind=FactKind::Boolean;truth.flag=1;usize value=host.atom(context,truth);return fact.kind==FactKind::NonZero ? value : host.invert(context,value);}
 }
 if(fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual){
  if(host.compare(fact.first,fact.second)==0){return fact.kind==FactKind::Equal ? 1u64 : 0u64;}
  address scalar=host.integer(context,fact.first,0);address other=host.integer(context,fact.second,1);
  if(scalar!=address()&&other!=address()&&host.same_scalar(scalar,other)!=0){
   return contract_compare(context,fact.kind==FactKind::Equal ? BinaryOperation::Equal : BinaryOperation::NotEqual,scalar,contract_value(fact.first),contract_value(fact.second),host).value;
  }
 }
 if(fact.kind==FactKind::Below&&host.scalar(fact.scalar).kind<2){
  return contract_compare(context,fact.flag!=0 ? BinaryOperation::Less : BinaryOperation::LessEqual,fact.scalar,contract_value(fact.first),contract_value(fact.second),host).value;
 }
 if(fact.kind==FactKind::Predicate){
  InternalPredicate predicate=host.predicate(fact.payload);ContractIndex value=contract_compare(context,predicate.operation,predicate.scalar,predicate.left,predicate.right,host);
  if(value.present!=0){return fact.flag!=0 ? value.value : host.invert(context,value.value);}
 }
 if(fact.kind==FactKind::Index||fact.kind==FactKind::Outside){
  ContractDomain domain=host.domain(fact.payload);ContractIndex relation=ContractIndex(0,0u64);
  if(domain.kind==1){relation.present=1;if(domain.count!=0){relation.value=contract_cut(context,fact.first,host.unsigned32,u64(domain.count)-1u64,host);}}
  else if(domain.kind==2&&domain.distance<=1){
   address scalar=host.integer(context,fact.first,0);address other=host.integer(context,domain.identity,1);
   if(scalar!=address()&&other!=address()&&host.scalar(scalar).kind==0&&host.same_scalar(scalar,other)!=0){
    relation=contract_compare(context,domain.distance==0 ? BinaryOperation::LessEqual : BinaryOperation::Less,scalar,contract_value(fact.first),contract_value(domain.identity),host);
   }
  }else if(domain.kind==3||domain.kind==4){
   address length=host.length(context,domain.identity);
   if(length!=address()){
    address scalar=host.integer(context,fact.first,0);address other=host.integer(context,length,1);
    if(scalar!=address()&&other!=address()&&host.scalar(scalar).kind==0&&host.same_scalar(scalar,other)!=0){
     relation=contract_compare(context,domain.kind==3 ? BinaryOperation::Less : BinaryOperation::LessEqual,scalar,contract_value(fact.first),contract_value(length),host);
    }
   }
  }if(relation.present!=0){return fact.kind==FactKind::Index ? relation.value : host.invert(context,relation.value);}
 }
 u32 truth=1;
 if(fact.kind==FactKind::Boolean){truth=fact.flag;fact.flag=1;}
 else if(fact.kind==FactKind::NonZero){fact.kind=FactKind::Zero;truth=0;}
 else if(fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual){truth=fact.kind==FactKind::Equal ? 1 : 0;fact.kind=FactKind::Equal;
  if(host.compare(fact.first,fact.second)>0){address swap=fact.first;fact.first=fact.second;fact.second=swap;}
 }else if(fact.kind==FactKind::Outside){fact.kind=FactKind::Index;truth=0;}
 else if(fact.kind==FactKind::Predicate){truth=fact.flag;fact.flag=1;}
 usize value=host.atom(context,fact);return truth!=0 ? value : host.invert(context,value);
}

extern "C" u32 contract_unary(FactView fact,cfn<InternalPredicate,address> ensures memory(read) predicate,cfn<ContractDomain,address> ensures memory(read) domain) on cpu
 requires nonzero(predicate) requires nonzero(domain) {
 if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum||fact.kind==FactKind::Zero||fact.kind==FactKind::NonZero||fact.kind==FactKind::Boolean){return 1;}
 if(fact.kind==FactKind::Index||fact.kind==FactKind::Outside){return domain(fact.payload).kind==1 ? 1 : 0;}
 if(fact.kind==FactKind::Predicate){InternalPredicate p=predicate(fact.payload);return p.left.is_constant!=0||p.right.is_constant!=0 ? 1 : 0;}return 0;
}

extern "C" u32 contract_all_unary(address context,cfn<FactView,address> next,cfn<InternalPredicate,address> ensures memory(read) predicate,cfn<ContractDomain,address> ensures memory(read) domain) on cpu
 requires nonzero(next) requires nonzero(predicate) requires nonzero(domain) {
 while(true){FactView fact=next(context);if(fact.present==0){return 1;}if(contract_unary(fact,predicate,domain)==0){return 0;}}
}

extern "C" u32 contract_priority(address source,ContractPriorityHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.all_unary) {
 SourceNode node=host.inspect(source);if(node.kind==0){return 1;}if(node.kind==1){return host.all_unary(node.left);}return 0;
}

extern "C" void contract_operands(address context,u32 either,ContractOperandsHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.seen) requires nonzero(host.imported) requires nonzero(host.inspect) requires nonzero(host.emit) {
 while(true){address clause=host.pop(context);if(clause==address()){return;}if(host.seen(context,clause)==0){continue;}
  if(host.imported(context,clause)!=0){host.emit(context,clause);continue;}SourceNode node=host.inspect(clause);
  if((node.kind==2&&either==0)||(node.kind==3&&either!=0)){host.push(context,node.right);host.push(context,node.left);}
  else if(node.kind==4&&either==0){host.push(context,node.truth!=0 ? node.right : node.no);host.push(context,node.left);}
  else{host.emit(context,clause);}
 }
}

extern "C" usize contract_import(address context,address root,ContractImportHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.visit) requires nonzero(host.start) requires nonzero(host.resume) requires nonzero(host.lookup) requires nonzero(host.inspect) requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.predicate) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.operation) requires nonzero(host.operand) requires nonzero(host.remember) {
 host.visit(context,root);
 while(true){ContractImport work=host.pop(context);if(work.present==0){break;}
  if(work.fold==0){
   if(host.lookup(context,work.clause).present!=0){continue;}SourceNode node=host.inspect(work.clause);usize result=0u64;
   if(node.kind==1){result=1u64;u32 phase=0;while(phase<2){host.begin(context,node.left);
    while(true){FactView fact=host.next(context);if(fact.present==0){break;}
     if(contract_unary(fact,host.predicate,host.domain)==(phase==0 ? 1 : 0)){result=host.operation(context,result,host.fact(context,fact),0);}
    }phase+=1;
   }}else if(node.kind!=0){u32 either=node.kind==3 ? 1 : 0;host.start(context,work.clause,either,either!=0 ? 0u64 : 1u64);continue;}
   host.remember(context,work.clause,result);
  }else{
   usize next=work.next;usize result=work.result;
   while(next<work.count&&result!=usize(work.either)){
    address operand=host.operand(context,next);ContractIndex value=host.lookup(context,operand);if(value.present==0){break;}
    result=host.operation(context,result,value.value,work.either);next+=1u64;
   }
   if(next<work.count&&result!=usize(work.either)){address operand=host.operand(context,next);host.resume(context,work.clause,work.either,next,result);host.visit(context,operand);}
   else{host.remember(context,work.clause,result);}
  }
 }return host.lookup(context,root).value;
}

extern "C" void contract_emit(address context,usize root,ContractEmitHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.known) requires nonzero(host.load) requires nonzero(host.variable) requires nonzero(host.count) requires nonzero(host.cell) requires nonzero(host.bound) requires nonzero(host.guard) requires nonzero(host.atom) requires nonzero(host.negate) requires nonzero(host.guard_node) requires nonzero(host.child) requires nonzero(host.both) requires nonzero(host.first) requires nonzero(host.either) requires nonzero(host.remember) requires nonzero(host.release) {
 host.push(context,root,0);
 while(true){ContractPending work=host.pop(context);if(work.present==0){return;}if(host.known(context,work.index)!=0){continue;}
  host.load(context,work.index);usize count=host.count(context);
  if(work.ready==0){host.push(context,work.index,1);while(count!=0u64){count-=1u64;host.push(context,host.cell(context,count).value,0);}host.release(context);continue;}
  ContractVariable variable=host.variable(context);ContractEnd begin=ContractEnd(0u64,0u64);u32 found=0;usize at=0u64;
  while(at<count){ContractCell cell=host.cell(context,at);at+=1u64;
   if(cell.value!=0u64){
    if(variable.kind==0){
     if(begin.low!=0u64||begin.high!=0u64){FactView lower=variable.fact;lower.kind=FactKind::Minimum;lower.value=begin.low;host.bound(context,lower);}
     if(!contract_end_equal(cell.end,variable.end)){FactView upper=variable.fact;upper.kind=FactKind::Maximum;upper.value=cell.end.low-1u64;host.bound(context,upper);}
     host.guard(context);
    }else{host.atom(context,variable.fact);if(begin.low==0u64&&begin.high==0u64){host.negate(context);}}
    if(cell.value!=1u64){SourceNode guard=host.guard_node(context);
     if(guard.kind==1&&guard.empty!=0){host.child(context,cell.value);}else{host.both(context,cell.value);}
    }
    if(found==0){host.first(context);found=1;}else{host.either(context);}
   }begin=cell.end;
  }host.remember(context,work.index,found);host.release(context);
 }
}

extern "C" void contract_union(address context,ContractUnionHost host) on cpu
 requires nonzero(host.case) requires nonzero(host.import) requires nonzero(host.operation) requires nonzero(host.emit) {
 u32 truth=0;while(truth<2){usize a=host.import(context,host.case(context,0,truth));usize b=host.import(context,host.case(context,1,truth));host.emit(context,truth,host.operation(context,a,b,truth));truth+=1;}
}

