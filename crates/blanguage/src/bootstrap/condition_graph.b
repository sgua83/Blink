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
 requires nonzero(host.next_fact) requires nonzero(host.negate) requires nonzero(host.remaining)
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
     if(host.negate(context,fact)!=0){
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
