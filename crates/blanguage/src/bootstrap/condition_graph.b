module bootstrap.condition_graph;
import codec.scalar_ranges;

extern "C" address next_fact_block(address walk,FactWalkHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.push) requires nonzero(host.pop) {
 while(true){
  address clause=host.pop(walk);
  if(address_bits(clause)==0u64){return address();}
  SourceNode source=host.inspect(walk,clause);
  if(source.kind==1){return source.left;}
  if(source.kind>=2){
   address right=source.kind==4&&source.truth==0 ? source.no : source.right;
   host.push(walk,right);
   host.push(walk,source.left);
  }
 }
}

extern "C" u32 condition_inputs(address walk,address input,ConditionInputHost host) on cpu
 requires nonzero(host.graph.inspect) requires nonzero(host.graph.push) requires nonzero(host.graph.pop)
 requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.predicate)
 requires nonzero(host.accept) {
 while(true){
  address block=next_fact_block(walk,host.graph);
  if(address_bits(block)==0u64){return 0;}
  host.begin(input,block);
  while(true){
   FactView fact=host.next(input);
   if(fact.present==0){break;}
   if(fact.kind==FactKind::Predicate){
    InternalPredicate predicate=host.predicate(fact.payload);
    if(predicate.left.is_constant==0 && host.accept(input,predicate.left.identity)!=0){return 1;}
    if(predicate.right.is_constant==0 && host.accept(input,predicate.right.identity)!=0){return 1;}
   }else{
    if(host.accept(input,fact.first)!=0){return 1;}
    if((fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual||fact.kind==FactKind::Below)
       && host.accept(input,fact.second)!=0){return 1;}
   }
  }
 }
}

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

u32 query_search(usize first,usize second,u32 right_truth,address context,Graph graph,Host host) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative) {
 address values=graph.values;usize at=0u64;
 host.pending_push(context,first,1);host.pending_push(context,second,right_truth);
 usize consumed=0u64;usize negative=0u64;usize choice=0u64;
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
    // A false leaf already constrains this path before any further gate choice.
    u32 found=remaining;
    if(found==0){
     while(negative<trail.count){
      usize id=word_at(trail,negative);negative+=1u64;
      Gate node=node_at(graph,id);
      if(node.kind==1&&value_at(values,id)==1){
       host.begin_facts(context,id);found=1;break;
      }
     }
    }
    if(found!=0){
     while(true){
      address fact=host.next_fact(context);
      if(address_bits(fact)==0u64){break;}
      FactView negated=negated_fact(inspect_fact(fact),host.facts);
      if(negated.present!=0){
       host.set_next_fact(context,negated);
       if(host.remaining(context)!=0u64){
        host.push_fact(context,fact,Checkpoint(trail.count,host.additions(context),negative,choice));
       }
       chosen=1;break;
      }
     }
     if(chosen!=0){continue;}
    }else{
     // Assignments already scanned remain fixed until a rollback.
     at=choice;
     while(at<trail.count){
      usize id=word_at(trail,at);Gate node=node_at(graph,id);u32 either=node.kind==3 ? 1 : 0;
      if(node.kind>=2&&value_at(values,id)==either+1
         &&value_at(values,node.left)==0&&value_at(values,node.right)==0){
       Checkpoint checkpoint=Checkpoint(trail.count,host.additions(context),negative,at);
       host.push_clause(context,node.left,1-either,checkpoint);
       host.pending_push(context,node.left,either);chosen=1;break;
      }
      at+=1u64;
     }
     choice=at+u64(chosen);
     if(chosen==0){return 1;}
     continue;
    }
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
  consumed=alternative.checkpoint.nodes;negative=alternative.checkpoint.negative;choice=alternative.checkpoint.choice;
  if(alternative.kind==1){host.pending_push(context,alternative.node,alternative.truth);}
 }
}

extern "C" u32 overlap(usize first,usize second,u32 right_truth,address context,Graph graph,Host host) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative) {
 return query_search(first,second,right_truth,context,graph,host);
}

const usize NO_QUERY_NODE=0xffffffffffffffffu64;
const usize LEFT_TERMS=0u64;
const usize RIGHT_TERMS=1u64;
const usize REQUIREMENTS=2u64;
const usize DELTA=3u64;
const usize COMMON=4u64;
const usize SUFFIX=5u64;
const usize PATHS=6u64;

void set_word(Span span,usize index,usize value) on cpu {
 unsafe_position<usize>(unsafe_address(address_bits(span.data)+u64(index)*8u64))=value;
}

void query_terms(address plan,EquivalenceHost host,usize root,u32 kind,usize list) on cpu
 requires nonzero(host.node) requires nonzero(host.list_clear) requires nonzero(host.seen_clear)
 requires nonzero(host.visit) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.list_push) requires nonzero(host.list_sort) {
 host.list_clear(plan,list);host.seen_clear(plan);host.push(plan,root);
 while(true){
  usize id=host.pop(plan);
  if(id==NO_QUERY_NODE){break;}
  if(host.visit(plan,id)==0){continue;}
  QueryNode node=host.node(plan,id);
  if(node.kind==kind){
   host.push(plan,node.left);host.push(plan,node.right);
  }else{
   host.list_push(plan,list,id);
  }
 }
 host.list_sort(plan,list);
}

void query_atoms(address plan,EquivalenceHost host,usize root,usize side) on cpu
 requires nonzero(host.node) requires nonzero(host.list_clear) requires nonzero(host.seen_clear)
 requires nonzero(host.visit) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.list_push) requires nonzero(host.list_sort) requires nonzero(host.list)
 requires nonzero(host.atoms_clear) requires nonzero(host.atom_insert) {
 query_terms(plan,host,root,2,side);
 host.atoms_clear(plan,side);
 Span terms=host.list(plan,side);
 usize term_at=0u64;
 while(term_at<terms.count){
  usize i=term_at;term_at+=1u64;host.atom_insert(plan,side,word_at(terms,i));}
}

u32 query_contains(Span terms,usize id) on cpu {
 usize first=0u64;usize end=terms.count;
 while(first<end){
  usize middle=first+(end-first)/2u64;
  usize current=word_at(terms,middle);
  if(current==id){return 1;}
  if(current<id){first=middle+1u64;}else{end=middle;}
 }
 return 0;
}

u32 query_subset(Span left,Span right) on cpu {
 if(left.count>right.count){return 0;}
 usize term_at=0u64;
 while(term_at<left.count){
  usize i=term_at;term_at+=1u64;
  if(query_contains(right,word_at(left,i))==0){return 0;}
 }
 return 1;
}

QueryNode query_gate_key(QueryNode node) on cpu {
 if(node.left>node.right){return QueryNode(node.right,node.left,node.kind);}
 return node;
}

usize query_join(address plan,EquivalenceHost host,u32 kind,usize left,usize right) on cpu
 requires nonzero(host.append) requires nonzero(host.gate_get) requires nonzero(host.gate_put) {
 if(left==right){return left;}
 QueryNode node=QueryNode(left,right,kind);QueryNode key=query_gate_key(node);
 usize id=host.gate_get(plan,key);
 if(id==NO_QUERY_NODE){id=host.append(plan,node);host.gate_put(plan,key,id);}
 return id;
}

usize query_join_terms(address plan,EquivalenceHost host,u32 kind,Span terms) on cpu
 requires nonzero(host.append) requires nonzero(host.gate_get) requires nonzero(host.gate_put) {
 if(terms.count==0u64){return NO_QUERY_NODE;}
 usize root=word_at(terms,0u64);
 usize term_at=1u64;
 while(term_at<terms.count){
  usize i=term_at;term_at+=1u64;
  root=query_join(plan,host,kind,root,word_at(terms,i));
 }
 return root;
}

struct QueryRoots {usize first;usize second;u32 directions;};

QueryRoots query_comparison(address plan,EquivalenceHost host,usize first,usize second,usize count) on cpu
 requires nonzero(host.node) requires nonzero(host.list_clear) requires nonzero(host.seen_clear)
 requires nonzero(host.visit) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.list_push) requires nonzero(host.list_sort) requires nonzero(host.list)
 requires nonzero(host.lists) requires nonzero(host.append)
 requires nonzero(host.gate_get) requires nonzero(host.gate_put) {
 query_terms(plan,host,first,3,LEFT_TERMS);
 query_terms(plan,host,second,3,RIGHT_TERMS);
 Span older=host.list(plan,LEFT_TERMS);Span newer=host.list(plan,RIGHT_TERMS);
 usize narrower=first;
 if(query_subset(older,newer)==0){
  if(query_subset(newer,older)==0){return QueryRoots(first,second,2);}
  Span swap=older;older=newer;newer=swap;narrower=second;
 }
 // All old disjuncts must be false in the only remaining direction.
 host.lists(plan,PATHS+older.count);
 usize old_at=0u64;
 while(old_at<older.count){
  usize i=old_at;old_at+=1u64;
  query_terms(plan,host,word_at(older,i),2,PATHS+i);
 }
 usize new_at=0u64;
 while(new_at<newer.count){
  usize i=new_at;new_at+=1u64;
  usize id=word_at(newer,i);
  if(query_contains(older,id)!=0){continue;}
  query_terms(plan,host,id,2,REQUIREMENTS);
  Span requirements=host.list(plan,REQUIREMENTS);
  if(query_contains(requirements,narrower)!=0){continue;}
  u32 covered=0;
  usize path_at=0u64;
  while(path_at<older.count){
   usize p=path_at;path_at+=1u64;
   if(query_subset(host.list(plan,PATHS+p),requirements)!=0){covered=1;break;}
  }
  if(covered==0){host.list_push(plan,DELTA,id);}
 }
 Span difference=host.list(plan,DELTA);
 if(difference.count==0u64){return QueryRoots(0u64,0u64,0);}
 usize gate_at=0u64;
 while(gate_at<count){
  usize i=gate_at;gate_at+=1u64;
  QueryNode node=host.node(plan,i);
  if(node.kind>=2){host.gate_put(plan,query_gate_key(node),i);}
 }
 usize delta=query_join_terms(plan,host,3,difference);
 if(older.count==1u64){return QueryRoots(delta,narrower,1);}

 Span first_path=host.list(plan,PATHS);
 usize first_at=0u64;
 while(first_at<first_path.count){
  usize i=first_at;first_at+=1u64;
  usize id=word_at(first_path,i);u32 common=1;
  usize path_at=1u64;
  while(path_at<older.count){
   usize p=path_at;path_at+=1u64;
   if(query_contains(host.list(plan,PATHS+p),id)==0){common=0;break;}
  }
  if(common!=0){host.list_push(plan,COMMON,id);}
 }
 Span common=host.list(plan,COMMON);
 if(common.count==0u64){return QueryRoots(delta,narrower,1);}
 // A false shared prefix rules out every old disjunct.
 usize prefix=query_join_terms(plan,host,2,common);
 usize suffix_at=0u64;
 while(suffix_at<older.count){
  usize p=suffix_at;suffix_at+=1u64;
  Span path=host.list(plan,PATHS+p);
  usize residual=NO_QUERY_NODE;usize at=0u64;
  while(at<path.count){
   usize id=word_at(path,at);at+=1u64;
   if(query_contains(common,id)!=0){continue;}
   residual=residual==NO_QUERY_NODE ? id : query_join(plan,host,2,residual,id);
  }
  if(residual==NO_QUERY_NODE){return QueryRoots(delta,prefix,1);}
  host.list_push(plan,SUFFIX,residual);
 }
 usize suffix=query_join_terms(plan,host,3,host.list(plan,SUFFIX));
 return QueryRoots(delta,query_join(plan,host,2,prefix,suffix),1);
}

QueryRoots query_normalize(address plan,EquivalenceHost host,usize first,usize second,usize count) on cpu
 requires nonzero(host.node) requires nonzero(host.write) requires nonzero(host.indices)
 requires nonzero(host.list_clear) requires nonzero(host.seen_clear) requires nonzero(host.visit)
 requires nonzero(host.push) requires nonzero(host.pop) requires nonzero(host.list_push)
 requires nonzero(host.list_sort) requires nonzero(host.list) requires nonzero(host.lists)
 requires nonzero(host.atoms_clear) requires nonzero(host.atom_insert) requires nonzero(host.atoms_subset)
 requires nonzero(host.append) requires nonzero(host.gate_get) requires nonzero(host.gate_put) {
 Span indices=host.indices(plan,count);
 host.lists(plan,PATHS);
 usize source_at=0u64;
 while(source_at<count){
  usize i=source_at;source_at+=1u64;
  QueryNode node=host.node(plan,i);
  if(node.kind>=2){
   node.left=word_at(indices,node.left);node.right=word_at(indices,node.right);
   if(node.left==node.right){set_word(indices,i,node.left);continue;}
   if(node.kind==3){
    if(host.node(plan,node.left).kind==2||host.node(plan,node.right).kind==2){
     query_atoms(plan,host,node.left,LEFT_TERMS);
     query_atoms(plan,host,node.right,RIGHT_TERMS);
     if(host.atoms_subset(plan,LEFT_TERMS,RIGHT_TERMS)!=0){set_word(indices,i,node.left);continue;}
     if(host.atoms_subset(plan,RIGHT_TERMS,LEFT_TERMS)!=0){set_word(indices,i,node.right);continue;}
    }
   }
   host.write(plan,i,i,node);
  }
  set_word(indices,i,i);
 }
 usize left=word_at(indices,first);usize right=word_at(indices,second);
 if(left==right){return QueryRoots(0u64,0u64,0);}
 return query_comparison(plan,host,left,right,count);
}

struct PackedQuery {usize first;usize second;usize count;};

PackedQuery query_pack(address plan,EquivalenceHost host,QueryRoots roots) on cpu
 requires nonzero(host.node) requires nonzero(host.write)
 requires nonzero(host.size) requires nonzero(host.indices) {
 usize count=host.size(plan);Span indices=host.indices(plan,count);
 set_word(indices,roots.first,1u64);set_word(indices,roots.second,1u64);
 usize remaining=count;
 while(remaining!=0u64){
  remaining-=1u64;
  if(word_at(indices,remaining)==0u64){continue;}
  QueryNode node=host.node(plan,remaining);
  if(node.kind>=2){set_word(indices,node.left,1u64);set_word(indices,node.right,1u64);}
 }
 // Parents have larger indices than their children, so the same slots can hold the final remap.
 usize destination=0u64;
 usize source_at=0u64;
 while(source_at<count){
  usize i=source_at;source_at+=1u64;
  if(word_at(indices,i)==0u64){continue;}
  QueryNode node=host.node(plan,i);
  if(node.kind>=2){node.left=word_at(indices,node.left);node.right=word_at(indices,node.right);}
  host.write(plan,i,destination,node);
  set_word(indices,i,destination);destination+=1u64;
 }
 return PackedQuery(word_at(indices,roots.first),word_at(indices,roots.second),destination);
}

extern "C" u32 equivalent(usize first,usize second,address plan,address context,usize count,EquivalenceHost preparation,Host host,cfn<void,address> reset) on cpu
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.set_next_fact)
 requires nonzero(host.facts.compare) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative)
 requires nonzero(reset)
 requires nonzero(preparation.node) requires nonzero(preparation.write) requires nonzero(preparation.indices)
 requires nonzero(preparation.list_clear) requires nonzero(preparation.seen_clear) requires nonzero(preparation.visit)
 requires nonzero(preparation.push) requires nonzero(preparation.pop) requires nonzero(preparation.list_push)
 requires nonzero(preparation.list_sort) requires nonzero(preparation.list) requires nonzero(preparation.lists)
 requires nonzero(preparation.atoms_clear) requires nonzero(preparation.atom_insert) requires nonzero(preparation.atoms_subset)
 requires nonzero(preparation.append) requires nonzero(preparation.gate_get) requires nonzero(preparation.gate_put)
 requires nonzero(preparation.size) requires nonzero(preparation.build) {
 if(first==second){return 1;}
 QueryRoots roots=query_normalize(plan,preparation,first,second,count);
 if(roots.directions==0){return 1;}
 PackedQuery packed=query_pack(plan,preparation,roots);
 Graph graph=preparation.build(plan,context,packed.count);
 if(query_search(packed.first,packed.second,0,context,graph,host)!=0){return 0;}
 if(roots.directions==1){return 1;}
 Span assigned=host.trail(context);usize at=assigned.count;
 while(at>0u64){at-=1u64;set_value(graph.values,word_at(assigned,at),0);}
 reset(context);
 return query_search(packed.second,packed.first,0,context,graph,host)==0 ? 1 : 0;
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
 requires nonzero(host.status) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.pending_count) requires nonzero(host.impossible)
 requires nonzero(host.leaf) requires nonzero(host.union) requires nonzero(host.common)
 requires nonzero(host.valid_additions) requires nonzero(host.release) {
 usize at=0u64;
 while(at<graph.count){
  RequiredNode node=required_node(graph,at);
  required_remaining(graph,at,node.users);at+=1u64;
 }
 host.push(context,graph.root);
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
 required_remaining(graph,graph.root,required_node(graph,graph.root).remaining-1u64);
}

FactView fact_equal(FactView value,i32 order) on cpu {
 FactView key=value;key.kind=FactKind::Equal;
 if(order>i32(0)){address first=key.first;key.first=key.second;key.second=first;}
 return key;
}
extern "C" u32 cross_conflict(address context,CrossScalars scalars,CrossHost host,u32 may_share) on cpu
 requires nonzero(host.count) requires nonzero(host.begin) requires nonzero(host.next)
 requires nonzero(host.contains) requires nonzero(host.range) requires nonzero(host.compare) {
 u32 side=host.count(context,0)<host.count(context,1) ? 0 : 1;
 u32 other=1-side;host.begin(context,side);
 while(true){
  FactView fact=host.next(context);if(fact.present==0){return 0;}
  if(may_share!=0&&host.contains(context,other,fact)!=0){continue;}
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


extern "C" void project_call_graph(address context,CallProjectionHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.impossible) requires nonzero(host.facts)
 requires nonzero(host.both) requires nonzero(host.either) {
 while(true){
  CallProjectionNode next=host.next(context);
  if(address_bits(next.original)==0u64){return;}
  SourceNode source=next.node;
  if(source.kind==0){host.impossible(context,next.original);}
  else if(source.kind==1){
   host.facts(context,next.original,source.left);
  }else if(source.kind==3){
   host.either(context,next.original,source.left,source.right);
  }else{
   address right=source.kind==4&&source.truth==0 ? source.no : source.right;
   host.both(context,next.original,source.left,right);
  }
 }
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
bool internal_same_limit(FactView fact,FactKind kind,address identity,address scalar,InternalHost host) on cpu
 requires nonzero(host.compare) requires nonzero(host.same_scalar) {
 return fact.present!=0&&fact.kind==kind
    &&host.compare(fact.first,identity)==i32(0)&&host.same_scalar(fact.scalar,scalar)!=0;
}
FactView internal_limit(address context,FactView key,u32 preceding,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.compare) requires nonzero(host.same_scalar) {
 FactView found=host.select(context,key,preceding);
 if(!internal_same_limit(found,key.kind,key.first,key.scalar,host)){found.present=0;}
 return found;
}
BitRange internal_clip(address context,address identity,address scalar,InternalScalar info,BitRange bounds,InternalHost host) on cpu
 requires nonzero(host.contains) {
 u64 lower=bounds.lower;u64 upper=bounds.upper;
 if(info.kind==0&&host.contains(context,internal_key(FactKind::NonZero,identity,scalar,0u64))!=0){lower=max(lower,1u64);}
 if(host.contains(context,internal_key(FactKind::Zero,identity,scalar,0u64))!=0){lower=max(lower,info.bias);upper=min(upper,info.bias);}
 return BitRange(lower,upper);
}
BitRange internal_bounds(address context,address identity,address scalar,InternalScalar info,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.contains) requires nonzero(host.compare)
 requires nonzero(host.same_scalar) {
 FactView lo=internal_limit(context,internal_key(FactKind::Minimum,identity,scalar,info.mask),1,host);
 FactView hi=internal_limit(context,internal_key(FactKind::Maximum,identity,scalar,0u64),0,host);
 BitRange bounds=BitRange(lo.present!=0 ? lo.value : 0u64,hi.present!=0 ? hi.value : info.mask);
 return internal_clip(context,identity,scalar,info,bounds,host);
}
BitRange internal_operand(address context,InternalOperand operand,address scalar,InternalScalar info,InternalHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.contains) requires nonzero(host.compare)
 requires nonzero(host.same_scalar) {
 if(operand.is_constant!=0){
  u64 value=info.kind==2 ? float_order(info.width,operand.bits) : operand.bits^info.bias;
  return BitRange(value,value);
 }
 return internal_bounds(context,operand.identity,scalar,info,host);
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
  BitRange left=left_zero!=0 ? BitRange(info.bias,info.bias) : internal_bounds(context,fact.first,scalar,info,host);
  if(left.lower!=left.upper){continue;}
  BitRange right=right_zero!=0 ? BitRange(info.bias,info.bias) : internal_bounds(context,fact.second,scalar,info,host);
  if(right.lower==right.upper&&left.lower==right.lower){return 1;}
 }
 return 0;
}
extern "C" u32 internal_conflict(address context,CrossScalars integers,InternalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.select) requires nonzero(host.contains)
 requires nonzero(host.compare) requires nonzero(host.same_scalar) requires nonzero(host.scalar)
 requires nonzero(host.predicate) requires nonzero(host.upper) {
 FactView fact=host.next(context);
 while(fact.present!=0){
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
  }else if(fact.kind==FactKind::Outside){
   InternalUpper upper=host.upper(fact.payload);
   if(upper.present!=0&&upper.distance==0&&host.compare(fact.first,upper.identity)==i32(0)){return 1;}
   key.kind=FactKind::Index;
   if(host.contains(context,key)!=0){return 1;}
  }else if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){
   FactView first=fact;
   InternalScalar info=host.scalar(first.scalar);
   u64 lower=0u64;
   while(true){
    if(first.kind==FactKind::Minimum&&fact.value<=info.mask){lower=fact.value;}
    fact=host.next(context);
    if(!internal_same_limit(fact,first.kind,first.first,first.scalar,host)){break;}
   }
   u64 upper=first.value;
   if(first.kind==FactKind::Minimum){
    FactView hi=internal_limit(context,internal_key(FactKind::Maximum,first.first,first.scalar,0u64),0,host);
    upper=hi.present!=0 ? hi.value : info.mask;
   }else{
    // The Minimum segment precedes Maximum in both original ordered inputs.
    FactView lo=internal_limit(context,internal_key(FactKind::Minimum,first.first,first.scalar,info.mask),1,host);
    if(lo.present!=0){continue;}
   }
   if(lower>upper){return 1;}
   if(info.bias<lower||info.bias>upper){
    if(host.contains(context,internal_key(FactKind::Zero,first.first,first.scalar,0u64))!=0){return 1;}
   }else if(lower==upper&&host.contains(context,internal_key(FactKind::NonZero,first.first,first.scalar,0u64))!=0){
    return 1;
   }
   continue;
  }else if(fact.kind==FactKind::NotEqual){
   if(internal_not_equal(context,fact,integers,host)!=0){return 1;}
  }else if(fact.kind==FactKind::Below){
   if(fact.flag!=0&&host.compare(fact.first,fact.second)==i32(0)){return 1;}
   key.first=fact.second;key.second=fact.first;key.flag=1;
   if(host.contains(context,key)!=0){return 1;}
   if(fact.flag!=0){key.flag=0;if(host.contains(context,key)!=0){return 1;}}
   InternalScalar info=host.scalar(fact.scalar);
   BitRange left=internal_bounds(context,fact.first,fact.scalar,info,host);
   BitRange right=internal_bounds(context,fact.second,fact.scalar,info,host);
   if(left.lower>right.upper||(fact.flag!=0&&left.lower==right.upper)){return 1;}
  }
  fact=host.next(context);
 }
 return 0;
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
 cfn<FactView,address,u32> bound_next,
 cfn<i32,address,address> ensures memory(read) compare,
 cfn<i32,address,address> ensures memory(read) scalar_compare) on cpu
 requires nonzero(bound_next) requires nonzero(compare) requires nonzero(scalar_compare) {
 if(carry.present==0){return CommonLimit(carry,carry);}
 FactView value=carry;
 FactView next=bound_next(context,side);
 while(next.present!=0&&common_limit_order(value,next,compare,scalar_compare)==i32(0)){
  value.value=value.kind==FactKind::Minimum ? max(value.value,next.value) : min(value.value,next.value);
  next=bound_next(context,side);
 }
 return CommonLimit(value,next);
}
extern "C" void common_facts(address context,CommonHost host) on cpu
 requires nonzero(host.intersection) requires nonzero(host.root_next) requires nonzero(host.representative)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.known_next)
 requires nonzero(host.known_bits) requires nonzero(host.contains_known)
 requires nonzero(host.emit_known) requires nonzero(host.bound_next) requires nonzero(host.range)
 requires nonzero(host.compare) requires nonzero(host.scalar_compare) requires nonzero(host.scalar) {
 host.intersection(context);
 CommonRoot left_root=host.root_next(context,0);
 CommonRoot right_root=host.root_next(context,1);
 while(left_root.identity!=address()&&right_root.identity!=address()){
  i32 order=host.compare(left_root.identity,right_root.identity);
  if(order<i32(0)){left_root=host.root_next(context,0);continue;}
  if(order>i32(0)){right_root=host.root_next(context,1);continue;}
  address identity=left_root.identity;
  address left=left_root.root;address right=right_root.root;
  left_root=host.root_next(context,0);right_root=host.root_next(context,1);
  address representative=left;
  if(host.compare(left,right)==i32(0)){
   if(host.compare(identity,left)==i32(0)){continue;}
  }else{
   representative=host.representative(context,left,right,identity);
   if(address_bits(representative)==0u64){continue;}
  }
  FactView fact=internal_key(FactKind::Equal,representative,unsafe_address(0u64),0u64);fact.second=identity;
  fact=fact_equal(fact,host.compare(fact.first,fact.second));
  if(host.contains(context,fact)==0){host.emit(context,fact);}
 }
 address previous_identity=unsafe_address(0u64);address previous_scalar=unsafe_address(0u64);u32 have_previous=0;
 while(true){
  CommonKnown known=host.known_next(context);if(known.present==0){break;}
  if(have_previous!=0&&host.compare(previous_identity,known.identity)==i32(0)&&host.scalar_compare(previous_scalar,known.scalar)==i32(0)){continue;}
  previous_identity=known.identity;previous_scalar=known.scalar;have_previous=1;
  CommonBits left=host.known_bits(context,0,known.identity,known.scalar);
  if(left.present==0){continue;}
  CommonBits right=host.known_bits(context,1,known.identity,known.scalar);
  if(right.present==0){continue;}
  InternalScalar scalar=host.scalar(known.scalar);
  BitPattern bits=known_common(scalar.mask,BitPattern(left.mask,left.value),scalar.mask,scalar.bias,BitPattern(right.mask,right.value));
  if(bits.mask!=0u64&&host.contains_known(context,known.identity,known.scalar,bits.mask,bits.value)==0){host.emit_known(context,known.identity,known.scalar,bits.mask,bits.value);}
 }
 CommonLimit left=common_limit_next(context,0,host.bound_next(context,0),host.bound_next,host.compare,host.scalar_compare);
 if(left.value.present==0){return;}
 CommonLimit right=common_limit_next(context,1,host.bound_next(context,1),host.bound_next,host.compare,host.scalar_compare);
 while(left.value.present!=0&&right.value.present!=0){
  i32 order=common_limit_order(left.value,right.value,host.compare,host.scalar_compare);
  if(order<i32(0)){left=common_limit_next(context,0,left.next,host.bound_next,host.compare,host.scalar_compare);
  }else if(order>i32(0)){right=common_limit_next(context,1,right.next,host.bound_next,host.compare,host.scalar_compare);
  }else{
   FactView fact=left.value;FactView lower=fact;FactView upper=fact;
   if(fact.kind==FactKind::Maximum){fact.value=max(fact.value,right.value.value);lower.value=0u64;upper.value=fact.value;
   }else{fact.value=min(fact.value,right.value.value);lower.value=fact.value;upper.value=fact.mask;}
   if(host.range(context,lower,upper)==0){host.emit(context,fact);}
   if(left.next.present==0||right.next.present==0){return;}
   left=common_limit_next(context,0,left.next,host.bound_next,host.compare,host.scalar_compare);right=common_limit_next(context,1,right.next,host.bound_next,host.compare,host.scalar_compare);
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
  if(order==0){
   if(left.present==0){host.insert(context,fact.first,left.root);}
   continue;
  }
  address root=order<0 ? left.root : right.root;
  EqualityLookup merged=order<0 ? right : left;
  if(merged.present!=0){host.merge(context,merged.root,root);}
  changed=1;
  if(left.present==0){host.insert(context,fact.first,root);}
  if(right.present==0){host.insert(context,fact.second,root);}
 }
}
extern "C" void computed_equalities(address context,ComputedEqualityHost host) on cpu
 requires nonzero(host.begin) requires nonzero(host.step) requires nonzero(host.produced)
 requires nonzero(host.remember) requires nonzero(host.equate) requires nonzero(host.has_equalities)
 requires nonzero(host.extend) requires nonzero(host.compare) {
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
extern "C" void alias_facts(address context,u32 inherited,AliasFactHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.previous)
 requires nonzero(host.group) requires nonzero(host.member) requires nonzero(host.emit)
 requires nonzero(host.compare) {
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
    host.emit(context,fact,member);
   }
  }
  side+=1;
 }
}

extern "C" u32 canonical_term_overlap(address context,CanonicalTermHost host) on cpu
 requires nonzero(host.same) requires nonzero(host.conflict) requires nonzero(host.next)
 requires nonzero(host.insert) requires nonzero(host.remove) requires nonzero(host.remaining)
 requires nonzero(host.facts.compare) {
 if(host.same(context)!=0||host.conflict(context,0)!=0){return 0;}
 if(host.conflict(context,1)!=0){return 1;}
 while(true){
  FactView fact=host.next(context);if(fact.present==0){return 0;}
  FactView opposite=negated_fact(fact,host.facts);if(opposite.present==0){continue;}
  if(host.insert(context,opposite)==0||host.conflict(context,0)==0){return 1;}
  host.remove(context,opposite);
  // The search resumes with the rejected alternative's positive prefix.
  if(host.remaining(context)==0){return 0;}
  if(host.insert(context,fact)!=0&&host.conflict(context,0)!=0){return 0;}
 }
}

extern "C" CanonicalInsertion canonical_insert(address context,address candidate,u32 empty,CanonicalInsertHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.subset) requires nonzero(host.reset)
 requires nonzero(host.overlap) {
 while(true){
  CanonicalTerm old=host.next(context);if(old.present==0){break;}
  if(host.subset(old.value,candidate)!=0){return CanonicalInsertion::Reject;}
 }
 if(empty!=0){return CanonicalInsertion::Insert;}
 host.reset(context);
 while(true){
  CanonicalTerm old=host.next(context);if(old.present==0){break;}
  if(host.overlap(candidate,old.value)==0){return CanonicalInsertion::Reject;}
 }
 return CanonicalInsertion::RetainInsert;
}
extern "C" u32 canonical_keep(address old,address candidate,CanonicalKeepHost host) on cpu
 requires nonzero(host.subset) requires nonzero(host.overlap) {
 if(host.subset(candidate,old)!=0){return 0;}
 return host.overlap(old,candidate);
}
extern "C" void canonical_normalize(address context,CanonicalNormalHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reset) requires nonzero(host.bound)
 requires nonzero(host.contains) requires nonzero(host.emit) requires nonzero(host.compare)
 requires nonzero(host.scalar) requires nonzero(host.upper) {
 FactView lower=internal_key(FactKind::Minimum,address(),address(),0u64);
 bool lower_started=false;
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
   if(!lower_started){lower=host.bound(context,0);lower_started=true;}
   bool redundant=false;
   while(lower.present!=0){
    i32 order=host.compare(lower.first,fact.first);
    if(order>i32(0)){break;}
    if(order==i32(0)&&lower.value>1u64&&host.scalar(lower.scalar).kind==0){redundant=true;break;}
    lower=host.bound(context,0);
   }
   if(redundant){continue;}
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
   bound=fact_numeric_alias(bound,scalar.kind);
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
 if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){fact=fact_numeric_alias(fact,host.scalar(fact.scalar).kind);}
 if(fact.kind==FactKind::Minimum){return contract_minimum(context,fact.first,fact.scalar,fact.value,host);}
 if(fact.kind==FactKind::Maximum){return contract_cut(context,fact.first,fact.scalar,fact.value,host);}
 if(fact.kind==FactKind::Zero||fact.kind==FactKind::NonZero){
  address scalar=host.integer(context,fact.first,0);
  if(scalar!=address()){usize zero=contract_zero(context,fact.first,scalar,host);return fact.kind==FactKind::Zero ? zero : host.invert(context,zero);}
  if(host.boolean(context,fact.first)!=0){FactView truth=fact;truth.kind=FactKind::Boolean;truth.flag=1;usize value=host.atom(context,truth);return fact.kind==FactKind::NonZero ? value : host.invert(context,value);}
 }
 if(fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual){
  if(host.compare(fact.first,fact.second)==0){return fact.kind==FactKind::Equal ? 1u64 : 0u64;}
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
     host.guard(context,begin,cell.end);
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

bool relation_wide_equal(IntegerWide a,IntegerWide b) on cpu {return a.low==b.low&&a.high==b.high;}
bool relation_wide_nonzero(IntegerWide a) on cpu {return a.low!=0u64||a.high!=0u64;}
IntegerWide relation_zero() on cpu {return IntegerWide(0u64,0u64);}
IntegerWide relation_one() on cpu {return IntegerWide(1u64,0u64);}
IntegerWide relation_minus_one() on cpu {return IntegerWide(18446744073709551615u64,18446744073709551615u64);}
IntegerWide relation_number(InternalScalar scalar,u64 bits) on cpu {
 u64 low=bits&scalar.mask;u64 high=0u64;
 if(scalar.kind==1&&(low&scalar.bias)!=0u64){low|=~scalar.mask;high=18446744073709551615u64;}
 return IntegerWide(low,high);
}
RelationRange relation_scalar_bounds(InternalScalar scalar) on cpu {return RelationRange(1,relation_number(scalar,scalar.bias),relation_number(scalar,scalar.mask^scalar.bias));}
RelationAffine relation_none() on cpu {return RelationAffine(0,address(),relation_zero(),relation_zero());}
RelationAffine relation_constant(IntegerWide value) on cpu {return RelationAffine(1,address(),relation_zero(),value);}
RelationAffine relation_scaled(RelationAffine input,IntegerWide factor) on cpu {
 if(input.present==0){return relation_none();}IntegerWideChecked scale=integer_wide_checked_multiply(input.scale,factor);if(scale.present==0){return relation_none();}
 IntegerWideChecked offset=integer_wide_checked_multiply(input.offset,factor);if(offset.present==0){return relation_none();}
 return RelationAffine(1,relation_wide_nonzero(scale.value) ? input.source : address(),scale.value,offset.value);
}
RelationAffine relation_plus(RelationAffine a,RelationAffine b,cfn<i32,address,address> ensures memory(read) compare) on cpu requires nonzero(compare) {
 if(a.present==0||b.present==0){return relation_none();}
 if(a.source!=address()&&b.source!=address()&&compare(a.source,b.source)!=0){return relation_none();}
 address source=a.source!=address() ? a.source : b.source;
 IntegerWideChecked scale=integer_wide_checked_add(a.scale,b.scale);if(scale.present==0){return relation_none();}
 IntegerWideChecked offset=integer_wide_checked_add(a.offset,b.offset);if(offset.present==0){return relation_none();}
 return RelationAffine(1,relation_wide_nonzero(scale.value) ? source : address(),scale.value,offset.value);
}
bool relation_affine_equal(RelationAffine a,RelationAffine b,cfn<i32,address,address> ensures memory(read) compare) on cpu requires nonzero(compare) {
 if((a.source==address())!=(b.source==address())){return false;}
 if(a.source!=address()&&compare(a.source,b.source)!=0){return false;}
 return relation_wide_equal(a.scale,b.scale)&&relation_wide_equal(a.offset,b.offset);
}
extern "C" RelationRange relation_interval(address context,RelationAffine value,RelationIntervalHost host) on cpu
 requires nonzero(host.bounds) {
 if(value.source==address()){return RelationRange(1,value.offset,value.offset);}
 RelationRange bounds=host.bounds(context,value.source);
 IntegerWideChecked a=integer_wide_checked_multiply(bounds.lower,value.scale);if(a.present==0){return RelationRange(0,relation_zero(),relation_zero());}
 a=integer_wide_checked_add(a.value,value.offset);if(a.present==0){return RelationRange(0,relation_zero(),relation_zero());}
 IntegerWideChecked b=integer_wide_checked_multiply(bounds.upper,value.scale);if(b.present==0){return RelationRange(0,relation_zero(),relation_zero());}
 b=integer_wide_checked_add(b.value,value.offset);if(b.present==0){return RelationRange(0,relation_zero(),relation_zero());}
 return RelationRange(1,range_min(a.value,b.value),range_max(a.value,b.value));
}

extern "C" RelationAffine relation_operand(address context,InternalOperand operand,InternalScalar info,RelationOperandHost host) on cpu
 requires nonzero(host.cache) requires nonzero(host.root) requires nonzero(host.enter) requires nonzero(host.bounds) requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.computed) requires nonzero(host.expression) requires nonzero(host.end) requires nonzero(host.leave) requires nonzero(host.remember) requires nonzero(host.compare) {
 if(operand.is_constant!=0){return relation_constant(relation_number(info,operand.bits));}
 address id=operand.identity;RelationAffine cached=host.cache(context,id);if(cached.present!=0){return cached;}
 RelationAffine symbol=RelationAffine(1,host.root(context,id),relation_one(),relation_zero());
 if(host.enter(context,id)==0){return symbol;}
 RelationRange bounds=host.bounds(context,id);RelationAffine value=relation_wide_equal(bounds.lower,bounds.upper) ? relation_constant(bounds.lower) : symbol;
 if(relation_affine_equal(value,symbol,host.compare)){
  host.begin(context,id);
  while(true){FactView fact=host.next(context);if(fact.present==0||fact.kind!=FactKind::Index){break;}if(host.compare(fact.first,id)!=0){break;}
   RelationExpression expression=host.computed(fact.payload);if(expression.present==0){continue;}
   RelationAffine candidate=host.expression(context,expression.definition);if(candidate.present!=0&&!relation_affine_equal(candidate,symbol,host.compare)){value=candidate;break;}
  }host.end(context);
 }host.leave(context,id);host.remember(context,id,value);return value;
}

extern "C" RelationAffine relation_expression(address context,RelationExpression expression,address scalar,InternalScalar info,RelationExpressionHost host) on cpu
 requires nonzero(host.operand) requires nonzero(host.interval) requires nonzero(host.same_scalar) requires nonzero(host.compare) {
 if(host.same_scalar(expression.scalar,scalar)==0||expression.operation>2){return relation_none();}
 RelationAffine a=host.operand(context,0);RelationAffine b=host.operand(context,1);RelationAffine value=relation_none();
 if(expression.operation==0){value=relation_plus(a,b,host.compare);}
 else if(expression.operation==1){value=relation_plus(a,relation_scaled(b,relation_minus_one()),host.compare);}
 else if(a.source==address()){value=relation_scaled(b,a.offset);}
 else if(b.source==address()){value=relation_scaled(a,b.offset);}
 if(value.present==0){return value;}RelationRange range=host.interval(context,value);if(range.present==0){return relation_none();}
 RelationRange bounds=relation_scalar_bounds(info);
 if(range_less(range.lower,bounds.lower)||range_less(range.upper,range.lower)||range_less(bounds.upper,range.upper)){return relation_none();}return value;
}

extern "C" u32 relation_retain(address context,IntegerWide lower,IntegerWide upper,InternalScalar info,RelationRetainHost host) on cpu
 requires nonzero(host.bounds) requires nonzero(host.emit) {
 RelationRange before=host.bounds(context);
 if(range_less(before.upper,lower)||range_less(upper,before.lower)||range_less(upper,lower)){return 0;}
 if(range_less(before.lower,lower)){host.emit(context,1,(lower.low&info.mask)^info.bias);}
 if(range_less(upper,before.upper)){host.emit(context,0,(upper.low&info.mask)^info.bias);}return 1;
}

extern "C" u32 relation_quotient(address context,address id,InternalOperand numerator,InternalOperand divisor,InternalScalar info,RelationQuotientHost host) on cpu
 requires nonzero(host.bounds) requires nonzero(host.operand_bounds) requires nonzero(host.retain) {
 if(numerator.is_constant!=0){return 1;}
 RelationRange q=host.bounds(context,id);RelationRange d=host.operand_bounds(context,divisor);RelationRange scalar=relation_scalar_bounds(info);
 if(info.kind!=1){
  IntegerWide dl=range_max(d.lower,relation_one());if(integer_wide_unsigned_less(d.upper,dl)){return 0;}
  IntegerWide lower=range_product(q.lower.low,dl.low);IntegerWide upper=range_subtract(range_add(range_product(q.upper.low,d.upper.low),d.upper),relation_one());
  if(integer_wide_unsigned_less(scalar.upper,lower)){return 0;}
  if(integer_wide_unsigned_less(scalar.upper,upper)){upper=scalar.upper;}
  return host.retain(context,numerator.identity,lower,upper);
 }
 RelationRange result=RelationRange(0,relation_zero(),relation_zero());u32 side=0;
 while(side<2){
  IntegerWide a=q.lower;IntegerWide b=q.upper;IntegerWide low=range_max(d.lower,relation_one());IntegerWide high=d.upper;
  if(side!=0){a=range_negate(q.upper);b=range_negate(q.lower);low=range_max(range_negate(d.upper),relation_one());high=range_negate(d.lower);}side+=1;
  if(range_less(high,low)){continue;}
  IntegerWide first=integer_wide_checked_multiply(a,low).value;IntegerWide last=integer_wide_checked_multiply(a,high).value;
  if(!range_less(relation_zero(),a)){first=range_subtract(first,range_subtract(low,relation_one()));last=range_subtract(last,range_subtract(high,relation_one()));}
  IntegerWide lower=range_max(range_min(first,last),scalar.lower);
  first=integer_wide_checked_multiply(b,low).value;last=integer_wide_checked_multiply(b,high).value;
  if(!range_less(b,relation_zero())){first=range_add(first,range_subtract(low,relation_one()));last=range_add(last,range_subtract(high,relation_one()));}
  IntegerWide upper=range_min(range_max(first,last),scalar.upper);
  if(!range_less(upper,lower)){
   if(result.present==0){result=RelationRange(1,lower,upper);}else{result.lower=range_min(result.lower,lower);result.upper=range_max(result.upper,upper);}
  }
 }
 IntegerWide minus_one=relation_minus_one();
 if(!range_less(scalar.lower,q.lower)&&!range_less(q.upper,scalar.lower)&&!range_less(minus_one,d.lower)&&!range_less(d.upper,minus_one)){
  if(result.present!=0){result.lower=scalar.lower;}else{result=RelationRange(1,scalar.lower,scalar.lower);}
 }
 if(result.present==0){return 0;}return host.retain(context,numerator.identity,result.lower,result.upper);
}

extern "C" u32 relation_at_most_zero(address context,RelationAffine value,InternalScalar info,RelationAtMostHost host) on cpu
 requires nonzero(host.retain) {
 if(value.source==address()){return !range_less(relation_zero(),value.offset) ? 1 : 0;}
 RelationRange bounds=relation_scalar_bounds(info);IntegerWide magnitude=(value.offset.high>>63u64)!=0u64 ? range_negate(value.offset) : value.offset;
 IntegerWide divisor=(value.scale.high>>63u64)!=0u64 ? range_negate(value.scale) : value.scale;
 IntegerWideDivision divided=integer_wide_unsigned_divide(magnitude,divisor);IntegerWide quotient=divided.quotient;
 if(range_less(relation_zero(),value.scale)){
  IntegerWide upper;
  if(!range_less(relation_zero(),value.offset)){
   if(!integer_wide_unsigned_less(quotient,bounds.upper)){return 1;}upper=quotient;
  }else{
   if(relation_wide_nonzero(divided.remainder)){quotient=range_add(quotient,relation_one());}
   IntegerWide limit=range_negate(bounds.lower);if(integer_wide_unsigned_less(limit,quotient)){return 0;}upper=range_negate(quotient);
  }return host.retain(context,value.source,bounds.lower,upper);
 }
 IntegerWide lower;
 if(!range_less(value.offset,relation_zero())){
  if(relation_wide_nonzero(divided.remainder)){quotient=range_add(quotient,relation_one());}
  if(integer_wide_unsigned_less(bounds.upper,quotient)){return 0;}lower=quotient;
 }else{
  IntegerWide limit=range_negate(bounds.lower);if(!integer_wide_unsigned_less(quotient,limit)){return 1;}lower=range_negate(quotient);
 }return host.retain(context,value.source,lower,bounds.upper);
}

extern "C" u32 relation_facts(address context,RelationFactsHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.scalar) requires nonzero(host.computed) requires nonzero(host.operand) requires nonzero(host.expression) requires nonzero(host.at_most) requires nonzero(host.quotient) requires nonzero(host.root) requires nonzero(host.interval) requires nonzero(host.retain) requires nonzero(host.compare) requires nonzero(host.finish) {
 while(true){FactView fact=host.next(context);if(fact.present==0){break;}
  if(fact.kind==FactKind::Below&&host.scalar(fact.scalar).kind<2){
   RelationAffine a=host.operand(context,0,InternalOperand(fact.first,0u64,address(),0),fact.scalar);
   RelationAffine b=host.operand(context,1,InternalOperand(fact.second,0u64,address(),0),fact.scalar);
   RelationAffine difference=relation_plus(a,relation_scaled(b,relation_minus_one()),host.compare);
   if(difference.present!=0){IntegerWideChecked offset=integer_wide_checked_add(difference.offset,IntegerWide(u64(fact.flag),0u64));
    if(offset.present!=0){difference.offset=offset.value;if(host.at_most(context,fact.scalar,difference)==0){return 0;}}
   }
  }else if(fact.kind==FactKind::Index){
   RelationExpression expression=host.computed(fact.payload);if(expression.present==0||host.scalar(expression.scalar).kind>=2){continue;}
   if(expression.operation==3&&host.quotient(context,fact.first,expression.scalar,expression.left,expression.right)==0){return 0;}
   RelationAffine value=host.expression(context,expression.definition,expression.scalar);if(value.present==0){continue;}
   address id=host.root(context,fact.first);
   if(value.source!=address()&&host.compare(value.source,id)==0){
    RelationAffine difference=relation_plus(value,RelationAffine(1,id,relation_minus_one(),relation_zero()),host.compare);
    if(difference.present!=0){
     if(host.at_most(context,expression.scalar,difference)==0){return 0;}
     RelationAffine reverse=relation_scaled(difference,relation_minus_one());if(reverse.present!=0&&host.at_most(context,expression.scalar,reverse)==0){return 0;}
    }
   }else{
    RelationRange range=host.interval(context,value,expression.scalar);if(range.present!=0&&host.retain(context,id,expression.scalar,range.lower,range.upper)==0){return 0;}
   }
  }
 }host.finish(context);return 1;
}


// One endpoint rule for the original source-path and public-contract consumers.
FactView fact_numeric_alias(FactView input,u32 scalar_kind) on cpu {
 FactView fact=input;
 if(scalar_kind==0){
  if(fact.kind==FactKind::Minimum&&fact.value==1u64){fact.kind=FactKind::NonZero;}
  else if(fact.kind==FactKind::Maximum&&fact.value==0u64){fact.kind=FactKind::Zero;}
 }return fact;
}

extern "C" u32 relation_source_plan(address context,RelationSourcePlanHost host) on cpu
 requires nonzero(host.pop) requires nonzero(host.push) requires nonzero(host.bounds) requires nonzero(host.enter) requires nonzero(host.computed) requires nonzero(host.root) requires nonzero(host.select) requires nonzero(host.emit) requires nonzero(host.clear) requires nonzero(host.compare) {
 address source=address();u32 known=1;
 while(true){
  RelationSourceWork work=host.pop(context);if(work.present==0){break;}
  address id=work.operand.identity;RelationRange bounds=host.bounds(context,id);
  if(relation_wide_equal(bounds.lower,bounds.upper)){continue;}
  if(work.ready!=0){host.emit(context,id);continue;}
  if(host.enter(context,id)==0){continue;}
  RelationExpression expression=host.computed(context,id);
  if(expression.present!=0){
   host.push(context,work.operand,1);
   if(expression.left.is_constant==0){host.push(context,expression.left,0);}
   if(expression.right.is_constant==0){host.push(context,expression.right,0);}
  }else{
   address root=host.root(context,id);
   if(source!=address()&&host.compare(source,root)!=0){known=0;break;}
   if(source==address()){source=host.select(context,root);}
  }
 }
 host.clear(context);return known!=0&&source!=address() ? 1 : 0;
}
RelationRange source_operand_range(address context,InternalOperand operand,address scalar,InternalScalar info,
 cfn<RelationRange,address,address> ensures memory(read) bounds,
 cfn<u32,address,address> ensures memory(read) same_scalar) on cpu requires nonzero(bounds) requires nonzero(same_scalar) {
 if(operand.is_constant!=0){
  if(same_scalar(operand.scalar,scalar)==0){return RelationRange(0,relation_zero(),relation_zero());}
  IntegerWide at=IntegerWide(operand.bits^info.bias,0u64);return RelationRange(1,at,at);
 }
 RelationRange value=bounds(context,operand.identity);
 return RelationRange(1,IntegerWide((value.lower.low&info.mask)^info.bias,0u64),IntegerWide((value.upper.low&info.mask)^info.bias,0u64));
}
extern "C" u32 relation_source_below(address context,address left,address right,address source,u64 lower,u64 upper,address scalar,InternalScalar info,RelationSourceBelowHost host) on cpu
 requires nonzero(host.seed) requires nonzero(host.next) requires nonzero(host.bounds) requires nonzero(host.computed) requires nonzero(host.same_scalar) requires nonzero(host.store) {
 host.seed(context,source,lower,upper);
 ComputedIntegerType encoding=ComputedIntegerType(info.width,info.kind==1 ? 1 : 0,0,info.mask,info.bias);
 while(true){
  address id=host.next(context);if(id==address()){break;}
  RelationRange bounds=host.bounds(context,id);
  if(range_less(bounds.upper,bounds.lower)){return 2;}
  if(relation_wide_equal(bounds.lower,bounds.upper)){continue;}
  RelationExpression expression=host.computed(context,id);if(expression.present==0){return 0;}
  RelationRange a=source_operand_range(context,expression.left,scalar,info,host.bounds,host.same_scalar);
  if(a.present==0){return 0;}
  RelationRange b=source_operand_range(context,expression.right,scalar,info,host.bounds,host.same_scalar);
  if(b.present==0){return 0;}
  if((expression.operation==3||expression.operation==4)&&b.lower.low<=info.bias&&info.bias<=b.upper.low){return 0;}
  BitRange value=computed_range_hull(expression.operation,encoding,BitRange(a.lower.low,a.upper.low),
   BitRange(b.lower.low,b.upper.low),BitRange((bounds.lower.low&info.mask)^info.bias,(bounds.upper.low&info.mask)^info.bias));
  if(value.lower>value.upper){return 2;}
  host.store(context,id,value.lower,value.upper);
 }
 RelationRange a=host.bounds(context,left);RelationRange b=host.bounds(context,right);
 return range_less(a.upper,a.lower)||range_less(b.upper,b.lower)||range_less(a.upper,b.lower) ? 2 : 1;
}

struct IndexProjection {u32 present;IndexOutput output;IndexFields fields;};
IndexProjection index_projection(IndexFields input,u32 strict) on cpu {
 IndexFields fields=input;
 if(input.kind==1||input.kind==2||input.kind==3||input.kind==4||input.kind==10){return IndexProjection(1,IndexOutput::Original,fields);}
 if(input.kind>=5&&input.kind<=8){
  if(strict==0){return IndexProjection(0,IndexOutput::Original,fields);}
  if(input.kind==5){return IndexProjection(1,IndexOutput::Data,fields);}
  if(input.kind==6){return IndexProjection(1,IndexOutput::Tile,fields);}
  if(input.kind==7){return IndexProjection(1,IndexOutput::View,fields);}
  return IndexProjection(1,IndexOutput::ApplicationData,fields);
 }
 if(input.kind==9||input.kind==11){
  fields.distance+=u64(strict);
  if(fields.distance<=4294967295u64){return IndexProjection(1,input.kind==9 ? IndexOutput::Upper : IndexOutput::Shifted,fields);}
 }
 return IndexProjection(0,IndexOutput::Original,fields);
}
extern "C" void index_domains_below(address context,u32 strict,IndexBelowHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.emit) {
 while(true){IndexFields fields=host.next(context);if(fields.present==0){break;}
  IndexProjection projected=index_projection(fields,strict);if(projected.present!=0){host.emit(context,projected.output,projected.fields);}
 }
}
void arithmetic_index_project(address context,u32 side,u32 strict,
 cfn<void,address,u32> select,cfn<IndexFields,address> next,cfn<void,address,IndexOutput,IndexFields> emit) on cpu
 requires nonzero(select) requires nonzero(next) requires nonzero(emit) {
 select(context,side);index_domains_below(context,strict,IndexBelowHost(next,emit));
}
void arithmetic_index_copy(address context,u32 side,
 cfn<void,address,u32> select,cfn<IndexFields,address> next,cfn<void,address,IndexOutput,IndexFields> emit) on cpu
 requires nonzero(select) requires nonzero(next) requires nonzero(emit) {
 select(context,side);while(true){IndexFields fields=next(context);if(fields.present==0){break;}emit(context,IndexOutput::Original,fields);}
}
extern "C" void arithmetic_index_domains(address context,u32 operation,ArithmeticIndexHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.next) requires nonzero(host.emit) requires nonzero(host.same) requires nonzero(host.bounds) requires nonzero(host.constant) requires nonzero(host.nonzero) requires nonzero(host.shift_zero) requires nonzero(host.identity) requires nonzero(host.room) requires nonzero(host.scratch) requires nonzero(host.contains) {
 if(operation==10||operation==11){
  if(operation==11){host.scratch(context,1);}
  arithmetic_index_project(context,0,0,host.select,host.next,host.emit);
  if(operation==10){arithmetic_index_project(context,1,0,host.select,host.next,host.emit);}
  else{
   host.scratch(context,0);host.select(context,1);
   while(true){IndexFields fields=host.next(context);if(fields.present==0){break;}IndexProjection p=index_projection(fields,0);
    if(p.present!=0&&host.contains(context,p.output,p.fields)!=0){host.emit(context,p.output,p.fields);}
   }
  }return;
 }
 if((operation==5||operation==6)&&host.same(context)!=0){
  arithmetic_index_copy(context,0,host.select,host.next,host.emit);arithmetic_index_copy(context,1,host.select,host.next,host.emit);return;
 }
 if(operation==5){
  arithmetic_index_project(context,0,0,host.select,host.next,host.emit);arithmetic_index_project(context,1,0,host.select,host.next,host.emit);return;
 }
 if(operation==4||operation==3){
  IndexBounds right=host.bounds(context,1);if(right.lower==0u64){return;}
  u32 strict=operation==3&&host.nonzero(context,0)!=0&&right.lower>1u64 ? 1 : 0;
  arithmetic_index_project(context,0,strict,host.select,host.next,host.emit);
  if(operation==4){arithmetic_index_project(context,1,1,host.select,host.next,host.emit);}return;
 }
 if(operation==9){
  u32 strict=host.nonzero(context,0)!=0&&host.shift_zero(context)==0 ? 1 : 0;
  arithmetic_index_project(context,0,strict,host.select,host.next,host.emit);return;
 }
 if(operation!=0&&operation!=1){return;}
 IndexBounds left=host.bounds(context,0);IndexBounds right=host.bounds(context,1);IndexConstant second=host.constant(context,1);
 if(operation==1){
  if(second.present!=0&&left.lower>=second.bits){arithmetic_index_project(context,0,second.bits!=0u64 ? 1 : 0,host.select,host.next,host.emit);}
  if(left.lower<right.upper){return;}
  address a=host.identity(context,0);address b=host.identity(context,1);
  host.emit(context,IndexOutput::Upper,IndexFields(1,0,address(),address(),a,address(),u64(u32(right.lower))));
  IndexConstant first=host.constant(context,0);
  if(first.present!=0&&first.bits==4294967295u64){host.emit(context,IndexOutput::Room,IndexFields(1,0,address(),address(),b,address(),0u64));}
  if(second.present!=0){
   u64 amount=u64(u32(second.bits));host.select(context,0);
   while(true){IndexFields input=host.next(context);if(input.present==0){break;}
    if(input.kind==9||input.kind==11){IndexFields fields=input;fields.distance+=amount;
     if(fields.distance<=4294967295u64){host.emit(context,input.kind==9 ? IndexOutput::Upper : IndexOutput::Shifted,fields);}
    }
   }
  }return;
 }
 address a=host.identity(context,0);address b=host.identity(context,1);IndexConstant first=host.constant(context,0);
 u32 side=0;
 while(side<2){address base=side==0 ? a : b;host.select(context,1-side);
  while(true){IndexFields input=host.next(context);if(input.present==0){break;}
   if(input.kind==9){IndexFields fields=input;fields.second=input.first;fields.first=base;host.emit(context,IndexOutput::Shifted,fields);}
  }side+=1;
 }
 if(left.upper+right.upper<=4294967295u64||host.room(context,0,b)!=0||host.room(context,1,a)!=0){
  host.emit(context,IndexOutput::Sum,IndexFields(1,0,address(),address(),a,b,0u64));side=0;
  while(side<2){IndexConstant constant=side==0 ? second : first;address base=side==0 ? a : b;
   if(constant.present!=0){u64 amount=u64(u32(constant.bits));if(amount!=0u64){
    host.emit(context,IndexOutput::Offset,IndexFields(1,0,address(),address(),base,address(),amount));host.select(context,side);
    while(true){IndexFields input=host.next(context);if(input.present==0){break;}
     if(input.kind==12){IndexFields fields=input;fields.distance+=amount;if(fields.distance<=4294967295u64){host.emit(context,IndexOutput::Offset,fields);}}
    }
   }}side+=1;
  }
 }
 side=0;
 while(side<2){IndexConstant constant=side==0 ? second : first;
  if(constant.present!=0){u64 amount=u64(u32(constant.bits));host.select(context,side);
   while(true){IndexFields input=host.next(context);if(input.present==0){break;}
    if((input.kind==9||input.kind==11)&&input.distance>=amount){IndexFields fields=input;fields.distance-=amount;host.emit(context,input.kind==9 ? IndexOutput::Upper : IndexOutput::Shifted,fields);}
   }
  }side+=1;
 }
}

void comparison_constraint_ends(address context,u32 strict,InternalScalar info,u64 left_min,u64 right_max,ComparisonConstraintHost host) on cpu
 requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.domain) {
 host.emit(context,ComparisonConstraintOutput::Below,u64(strict));
 if(right_max>=u64(strict)){u64 upper=right_max-u64(strict);if(upper!=info.mask){host.emit(context,ComparisonConstraintOutput::Maximum,upper);}}
 if(info.kind!=1){host.emit(context,ComparisonConstraintOutput::Upper,u64(strict));}
 if(info.kind==0&&info.width==32){index_domains_below(context,strict,IndexBelowHost(host.next,host.domain));}
 u64 lower=left_min+u64(strict);
 if(lower>=left_min&&lower!=0u64&&lower<=info.mask){host.emit(context,ComparisonConstraintOutput::Minimum,lower);}
}

extern "C" void comparison_constraint(address context,u32 strict,InternalScalar info,IndexBounds left,IndexBounds right,ComparisonConstraintHost host) on cpu
 requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.domain) {
 comparison_constraint_ends(context,strict,info,left.lower,right.upper,host);
}

bool call_comparison_overlaps(CallComparisonRanges ranges,BitRange first_left,BitRange first_right,u64 mask) on cpu
 requires nonzero(ranges.get) {
 BitRange left=first_left;
 BitRange right=first_right;
 u64 left_at=0u64;
 u64 right_at=0u64;
 while(true){
  u64 lower=max(left.lower,right.lower);
  if(lower>mask){return false;}
  if(lower<=min(left.upper,right.upper)){return true;}
  if(left.upper<right.upper){
   left_at+=1u64;
   if(left_at==ranges.left_count){return false;}
   left=ranges.get(ranges.left,left_at);
  }else{
   right_at+=1u64;
   if(right_at==ranges.right_count){return false;}
   right=ranges.get(ranges.right,right_at);
  }
 }
}

u64 call_comparison_right_max(CallComparisonRanges ranges,BitRange first,u64 mask) on cpu
 requires nonzero(ranges.get) {
 u64 accepted=0u64;
 u64 end=ranges.right_count;
 u64 upper=first.upper;
 while(end-accepted>1u64){
  u64 middle=accepted+(end-accepted)/2u64;
  BitRange range=ranges.get(ranges.right,middle);
  if(range.lower<=mask){
   accepted=middle;
   upper=range.upper;
  }else{
   end=middle;
  }
 }
 return min(upper,mask);
}

extern "C" bool comparison_call_below(address context,u32 strict,u32 same,InternalScalar info,
 CallComparisonRanges ranges,ComparisonConstraintHost host) on cpu
 requires nonzero(ranges.get) requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.domain) {
 if(same!=0&&strict!=0){return false;}
 if(ranges.left_count==0u64||ranges.right_count==0u64){return false;}
 BitRange left=ranges.get(ranges.left,0u64);
 BitRange right=ranges.get(ranges.right,0u64);
 if(left.lower>info.mask||right.lower>info.mask){return false;}
 if(same!=0&&!call_comparison_overlaps(ranges,left,right,info.mask)){return false;}
 u64 upper=call_comparison_right_max(ranges,right,info.mask);
 if(same==0&&(left.lower>upper||(strict!=0&&left.lower==upper))){return false;}
 comparison_constraint_ends(context,strict,info,left.lower,upper,host);
 return true;
}
void comparison_domains(address state,u32 truth,u32 source,u32 target,u32 common_only,ComparisonCaseHost host) on cpu
 requires nonzero(host.select) requires nonzero(host.next) requires nonzero(host.domain) {
 host.select(state,source);
 while(true){ComparisonDomain domain=host.next(state);if(domain.present==0){return;}
  if(common_only==0||domain.common!=0){host.domain(state,truth,target,domain.original);}
 }
}
u32 comparison_complement(u32 operation) on cpu {
 if(operation==10){return 11;}if(operation==11){return 10;}
 if(operation==12){return 15;}if(operation==13){return 14;}
 return operation==14 ? 13 : 12;
}
u32 comparison_possible(u32 operation,IndexBounds left,IndexBounds right,u32 same) on cpu {
 if(same!=0){return max(left.lower,right.lower)<=min(left.upper,right.upper)&&(operation==10||operation==14||operation==15) ? 1 : 0;}
 if(operation==12){return left.lower<right.upper ? 1 : 0;}
 if(operation==13){return left.upper>right.lower ? 1 : 0;}
 if(operation==14){return left.lower<=right.upper ? 1 : 0;}
 if(operation==15){return left.upper>=right.lower ? 1 : 0;}
 if(operation==10){return max(left.lower,right.lower)<=min(left.upper,right.upper) ? 1 : 0;}
 return left.lower!=right.upper||left.upper!=right.lower ? 1 : 0;
}
extern "C" void comparison_integer_case(address state,address input,u32 operation,u32 truth,ComparisonIntegerInfo info,ComparisonIntegerHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.range) requires nonzero(host.constrain) {
 u32 op=truth!=0 ? operation : comparison_complement(operation);u32 possible=0;u64 a=0u64;
 while(a<info.left_count&&possible==0){IndexBounds left=host.range(input,0,a);u64 b=0u64;
  while(b<info.right_count&&possible==0){possible=comparison_possible(op,left,host.range(input,1,b),info.same);b+=1u64;}a+=1u64;
 }
 if(possible!=0){
  if(op>=12){host.constrain(state,input,truth,op==13||op==15 ? 1 : 0,op==12||op==13 ? 1 : 0);}
  else if(op==10){
   host.cases.emit(state,truth,ComparisonOutput::Equal,0,0u64);
   comparison_domains(state,truth,1,0,0,host.cases);comparison_domains(state,truth,0,1,0,host.cases);
   u32 side=0;while(side<2){IndexBounds other=side==0 ? info.right : info.left;u32 nonzero=side==0 ? info.right_nonzero : info.left_nonzero;
    if(other.lower!=0u64){host.cases.emit(state,truth,ComparisonOutput::Minimum,side,other.lower);}
    if(other.upper!=info.scalar.mask){host.cases.emit(state,truth,ComparisonOutput::Maximum,side,other.upper);}
    if(nonzero!=0){host.cases.emit(state,truth,ComparisonOutput::NonZero,side,0u64);}side+=1;
   }
  }else{
   host.cases.emit(state,truth,ComparisonOutput::NotEqual,0,0u64);u32 side=0;
   while(side<2){IndexBounds other=side==0 ? info.right : info.left;
    if(other.lower==info.scalar.bias&&other.upper==info.scalar.bias){host.cases.emit(state,truth,info.scalar.kind==1 ? ComparisonOutput::NonZero : ComparisonOutput::Minimum,side,1u64);}side+=1;
   }
  }
 }
 host.cases.finish(state,truth,possible);
}
extern "C" u32 comparison_integer(address state,address input,u32 operation,ComparisonIntegerInfo info,ComparisonIntegerHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.range) requires nonzero(host.constrain) requires nonzero(host.ordered) requires nonzero(host.refine) {
 u32 op=operation==11 ? 10 : operation;
 comparison_integer_case(state,input,op,0,info,host);comparison_integer_case(state,input,op,1,info,host);
 if(op==10&&!(info.left.lower==info.scalar.bias&&info.left.upper==info.scalar.bias)&&!(info.right.lower==info.scalar.bias&&info.right.upper==info.scalar.bias)){
  host.ordered(state,input,0,12,info);host.ordered(state,input,1,13,info);host.refine(state);
 }
 return operation==11 ? 1 : 0;
}
extern "C" u32 comparison_float(address state,u32 operation,IndexBounds finite,ComparisonFloatHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.select) requires nonzero(host.cases.next) requires nonzero(host.cases.domain) requires nonzero(host.cases.finish) requires nonzero(host.predicate) {
 BinaryOperation op=BinaryOperation(operation==11 ? 10 : operation);u32 truth=0;
 while(truth<2){host.predicate(state,truth,op);u32 side=0;
  while(side<2){comparison_domains(state,truth,side,side,1,host.cases);
   if(truth!=0){host.cases.emit(state,truth,ComparisonOutput::Minimum,side,finite.lower);host.cases.emit(state,truth,ComparisonOutput::Maximum,side,finite.upper);}side+=1;
  }host.cases.finish(state,truth,1);truth+=1;
 }return operation==11 ? 1 : 0;
}
extern "C" u32 comparison_boolean(address state,u32 operation,ComparisonBooleanHost host) on cpu
 requires nonzero(host.pair) requires nonzero(host.finish) {
 host.pair(state,0,0,1);host.pair(state,1,1,0);host.finish(state,0);
 host.pair(state,0,0,0);host.pair(state,1,1,1);host.finish(state,1);
 return operation==11 ? 1 : 0;
}

extern "C" void value_condition(address context,ValueConditionInput input,ValueConditionHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.domain) requires nonzero(host.bounds) requires nonzero(host.emit) requires nonzero(host.finish) {
 u32 live=input.base_cases;
 if(input.constant.present!=0){live=live&(input.constant.bits==0u64 ? 1 : 2);}
 if(input.scalar.kind!=2&&live!=0){
  while(true){ComparisonDomain domain=host.next(context);if(domain.present==0){break;}
   if(domain.common!=0){host.domain(context,live,domain.original);}
  }
  if(input.scalar.kind==0||input.scalar.kind==1){
   IndexBounds bounds=host.bounds(context);
   if(bounds.lower!=0u64){host.emit(context,live,ValueConditionOutput::Minimum,bounds.lower);}
   if(bounds.upper!=input.scalar.mask){host.emit(context,live,ValueConditionOutput::Maximum,bounds.upper);}
  }
 }
 u32 truth=0;
 while(truth<2){u32 mask=1<<truth;
  if((live&mask)==0){host.finish(context,truth,ValueConditionFinish::Impossible);}
  else if(input.scalar.kind==2){host.finish(context,truth,ValueConditionFinish::Base);}
  else{
   ValueConditionOutput output=input.scalar.kind==3 ? (truth==0 ? ValueConditionOutput::BooleanFalse : ValueConditionOutput::BooleanTrue) : (truth==0 ? ValueConditionOutput::Zero : ValueConditionOutput::NonZero);
   host.emit(context,mask,output,0u64);host.finish(context,truth,ValueConditionFinish::Facts);
  }truth+=1;
 }
}

void index_condition_constant(address state,u32 value,cfn<void,address,u32,IndexConditionFinish> finish) on cpu
 requires nonzero(finish) {
 finish(state,0,value==0 ? IndexConditionFinish::Empty : IndexConditionFinish::Impossible);
 finish(state,1,value!=0 ? IndexConditionFinish::Empty : IndexConditionFinish::Impossible);
}
extern "C" void index_condition_equal(address state,IndexRelationOperand left,IndexRelationOperand right,IndexRelationEqualHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.finish) requires nonzero(host.predicate) requires nonzero(host.guard) {
 if(left.kind==2&&right.kind==2){index_condition_constant(state,left.bits==right.bits ? 1 : 0,host.cases.finish);return;}
 if(left.kind==1&&right.kind==1){
  host.predicate(state,left.identity,right.identity);
  host.cases.emit(state,0,IndexConditionOutput::NotEqual,left.identity,right.identity,0u64);
  host.cases.emit(state,1,IndexConditionOutput::Equal,left.identity,right.identity,0u64);
  host.cases.finish(state,0,IndexConditionFinish::Facts);host.cases.finish(state,1,IndexConditionFinish::Facts);return;
 }
 IndexRelationOperand value=left.kind==1 ? left : right;IndexRelationOperand constant=left.kind==2 ? left : right;
 host.cases.emit(state,1,IndexConditionOutput::Minimum,value.identity,address(),constant.bits);
 host.cases.emit(state,1,IndexConditionOutput::Maximum,value.identity,address(),constant.bits);host.guard(state);
}
extern "C" void index_condition_below(address state,address input,IndexRelationInput values,u32 original_strict,IndexRelationBelowHost host) on cpu
 requires nonzero(host.cases.emit) requires nonzero(host.cases.finish) requires nonzero(host.bounds) requires nonzero(host.constrain) requires nonzero(host.compare) {
 if(values.left.kind==2&&values.right.kind==2){
  u32 value=original_strict!=0 ? (values.left.bits<values.right.bits ? 1 : 0) : (values.left.bits<=values.right.bits ? 1 : 0);
  index_condition_constant(state,value,host.cases.finish);return;
 }
 u32 invert=(values.left.kind==0||values.left.kind==2)&&values.right.kind==1 ? 1 : 0;
 u32 first=invert;u32 second=1-first;u32 strict=invert!=0 ? 1-original_strict : original_strict;
 IndexRelationOperand a=invert==0 ? values.left : values.right;IndexRelationOperand b=invert==0 ? values.right : values.left;
 if(a.kind==1&&b.kind==1&&host.compare(a.identity,b.identity)==0){index_condition_constant(state,(1-strict)^invert,host.cases.finish);return;}
 IndexBounds raw_a=host.bounds(input,first);IndexBounds raw_b=host.bounds(input,second);
 u32 nonzero_a=invert==0 ? values.left_nonzero : values.right_nonzero;u32 nonzero_b=invert==0 ? values.right_nonzero : values.left_nonzero;
 IndexBounds a_range=IndexBounds(u64(u32(max(raw_a.lower,u64(nonzero_a)))),u64(u32(raw_a.upper)));
 IndexBounds b_range=IndexBounds(u64(u32(max(raw_b.lower,u64(nonzero_b)))),u64(u32(raw_b.upper)));
 u32 possible_false=strict!=0 ? (a_range.upper>=b_range.lower ? 1 : 0) : (a_range.upper>b_range.lower ? 1 : 0);
 u32 possible_true=strict!=0 ? (a_range.lower<b_range.upper ? 1 : 0) : (a_range.lower<=b_range.upper ? 1 : 0);
 if(a.kind==1&&(b.kind==0||b.kind==2)){
  u64 minimum=b.kind==2 ? u64(u32(b.bits)) : b_range.lower;u64 maximum=b.kind==2 ? u64(u32(b.bits)) : b_range.upper;
  u64 lower=minimum+u64(1-strict);
  if(lower>4294967295u64){possible_false=0;}else if(possible_false!=0){host.cases.emit(state,invert,IndexConditionOutput::Minimum,a.identity,address(),lower);}
  if(maximum<u64(strict)){possible_true=0;}else if(possible_true!=0){host.cases.emit(state,1-invert,IndexConditionOutput::Maximum,a.identity,address(),maximum-u64(strict));}
 }else if(a.kind==1&&b.kind==1){
  if(possible_false!=0){host.constrain(state,input,invert,first,b.identity,a.identity,raw_b,raw_a,1-strict);}
  if(possible_true!=0){host.constrain(state,input,1-invert,second,a.identity,b.identity,raw_a,raw_b,strict);}
 }
 host.cases.finish(state,invert,possible_false!=0 ? IndexConditionFinish::Facts : IndexConditionFinish::Impossible);
 host.cases.finish(state,1-invert,possible_true!=0 ? IndexConditionFinish::Facts : IndexConditionFinish::Impossible);
}

extern "C" void numeric_bounds(address context,NumericBoundsHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.root) requires nonzero(host.lookup) requires nonzero(host.emit) {
 while(true){FactView fact=host.next(context);if(fact.present==0){return;}
  if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){
   NumericBoundSide side=fact.kind==FactKind::Minimum ? NumericBoundSide::Lower : NumericBoundSide::Upper;
   address id=host.root(context,fact.first);NumericBoundPrevious previous=host.lookup(context,side,id,fact.scalar);
   u64 value=fact.value;
   if(previous.present!=0){value=side==NumericBoundSide::Lower ? max(previous.value,value) : min(previous.value,value);}
   host.emit(context,side,id,fact.scalar,value);
  }
 }
}

extern "C" void execution_condition(address root,address context,ExecutionHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.push) requires nonzero(host.pop)
 requires nonzero(host.has_result) requires nonzero(host.impossible) requires nonzero(host.guard) requires nonzero(host.compose) {
 host.push(context,root,0);
 while(true){
  NegatePending pending=host.pop(context);if(pending.present==0){return;}
  if(host.has_result(context,pending.source)!=0){continue;}
  SourceNode node=host.inspect(pending.source);
  if(pending.ready==0&&node.kind>=2){
   host.push(context,pending.source,1);
   if(node.kind!=4){host.push(context,node.right,0);}
   host.push(context,node.left,0);continue;
  }
  if(node.kind==0){host.impossible(context,pending.source);
  }else if(node.kind==1){host.guard(context,pending.source);
  }else{
   ExecutionCase left_no=ExecutionCase(node.left,0,0);ExecutionCase left_yes=ExecutionCase(node.left,1,0);
   ExecutionCase right_no=ExecutionCase(node.right,0,0);ExecutionCase right_yes=ExecutionCase(node.right,1,0);
   u32 no_any=node.kind==3 ? 0 : 1;u32 yes_any=node.kind==3 ? 1 : 0;
   if(node.kind==4){right_no=ExecutionCase(node.condition,1-node.truth,1);right_yes=ExecutionCase(node.condition,node.truth,1);}
   host.compose(context,pending.source,ExecutionJoin(left_no,right_no,no_any),ExecutionJoin(left_yes,right_yes,yes_any));
  }
 }
}

extern "C" void proof_facts(address context,ProofFactsInput input,ProofFactsHost host) on cpu
 requires nonzero(host.origin) requires nonzero(host.range) requires nonzero(host.rewind) requires nonzero(host.next)
 requires nonzero(host.boolean_cases) requires nonzero(host.emit) requires nonzero(host.compare) {
 if(input.scalar.kind==2||(input.scalar.kind==3&&input.numeric_only!=0)){return;}
 usize extra=input.mode==2||input.mode==3||(input.mode==1&&input.origins==0u64) ? 1u64 : 0u64;
 if(input.origins+extra==0u64){return;}
 u64 lower=1u64;u64 upper=0u64;u32 have=0;u32 contains_zero=0;u32 nonzero=input.nonzero;u32 impossible=0;
 if(input.scalar.kind==3){impossible=host.boolean_cases(context);
 }else{
  usize index=0u64;
  while(index<input.ranges){IndexBounds interval=host.range(context,index);index+=1u64;
   u64 lo=interval.lower;u64 hi=min(interval.upper,input.scalar.mask);if(lo>hi){continue;}
   if(input.mode==1&&input.constant!=0){u64 value=input.bits^input.scalar.bias;lo=max(lo,value);hi=min(hi,value);if(lo>hi){continue;}}
   if(input.mode==1&&input.nonzero!=0){
    if(lo==input.scalar.bias){if(lo==hi){continue;}lo+=1u64;}
    if(hi==input.scalar.bias){hi-=1u64;}
   }
   if(have==0){lower=lo;have=1;}upper=hi;
   if(lo<=input.scalar.bias&&input.scalar.bias<=hi){contains_zero=1;}
  }
  if(input.mode==1){nonzero=nonzero!=0||contains_zero==0||(input.constant!=0&&input.bits!=0u64) ? 1 : 0;}
 }
 address previous=address();usize at=0u64;bool have_target=false;
 while(at<input.origins+extra){
  usize index=at;at+=1u64;
  address identity=address();
  if(input.mode==3){
   if(index==input.origins){
    if(have_target){break;}
    identity=input.primary;
   }else{
    identity=host.origin(context,index);
    i32 order=host.compare(identity,input.primary);
    if(order==i32(0)){have_target=true;}
    else{
     FactView fact=internal_key(FactKind::Equal,identity,address(),0u64);
     fact.second=input.primary;
     host.emit(context,fact_equal(fact,order));
    }
   }
  }else{
   identity=extra!=0u64&&index==0u64 ? input.primary : host.origin(context,index-extra);
   if(input.mode==2&&index!=0u64&&host.compare(identity,input.primary)==i32(0)){continue;}
   if(input.numeric_only==0&&address_bits(previous)!=0u64){
    FactView fact=internal_key(FactKind::Equal,previous,address(),0u64);
    fact.second=identity;
    host.emit(context,fact_equal(fact,host.compare(previous,identity)));
   }
   previous=identity;
  }
  if(input.scalar.kind==3){
   u32 truth=0;while(truth<2){if((impossible&(1<<(1-truth)))!=0){FactView fact=internal_key(FactKind::Boolean,identity,address(),0u64);fact.flag=truth;host.emit(context,fact);}truth+=1;}
   continue;
  }
  if(nonzero!=0){host.emit(context,internal_key(FactKind::NonZero,identity,address(),0u64));}
  if(lower==input.scalar.bias&&upper==input.scalar.bias){host.emit(context,internal_key(FactKind::Zero,identity,address(),0u64));}
  if(lower!=0u64){host.emit(context,internal_key(FactKind::Minimum,identity,input.scalar_ref,lower));}
  if(upper!=input.scalar.mask){host.emit(context,internal_key(FactKind::Maximum,identity,input.scalar_ref,upper));}
  if(input.scalar.kind==0){host.rewind(context);
   while(true){ProofFactDomain domain=host.next(context);IndexFields fields=domain.fields;if(fields.present==0){break;}
    if(upper==0u64&&fields.kind==9&&fields.distance==0u64){continue;}
    if(input.numeric_only!=0&&domain.known_bits==0){continue;}
    FactView fact=internal_key(FactKind::Index,identity,address(),0u64);fact.payload=fields.original;host.emit(context,fact);
    if(input.numeric_only==0&&(fields.kind==12||fields.kind==13||fields.kind==9)){
     FactView below=internal_key(FactKind::Below,fields.kind==9 ? identity : fields.first,input.scalar_ref,0u64);below.second=fields.kind==9 ? fields.first : identity;below.flag=fields.distance!=0u64 ? 1 : 0;host.emit(context,below);
    }
   }
  }
 }
}

extern "C" void output_aliases(address context,usize components,usize conditions,address primary,OutputAliasHost host,ProofFactsHost facts) on cpu
 requires nonzero(host.input) requires nonzero(host.finish_values)
 requires nonzero(host.case_identity) requires nonzero(host.join_cases)
 requires nonzero(facts.origin) requires nonzero(facts.range) requires nonzero(facts.rewind) requires nonzero(facts.next)
 requires nonzero(facts.boolean_cases) requires nonzero(facts.emit) requires nonzero(facts.compare) {
 usize component=0u64;
 while(component<components){
  ProofFactsInput input=host.input(context,component,primary);
  proof_facts(context,input,facts);
  component+=1u64;
 }
 host.finish_values(context);
 usize index=0u64;
 while(index<conditions){
  FactView no=internal_key(FactKind::Boolean,host.case_identity(context,index),address(),0u64);
  FactView yes=no;yes.flag=1;
  host.join_cases(context,index,no,yes);
  index+=1u64;
 }
}

bool has_call_input(CallFactInput input) on cpu {
 return input.original!=address()||input.identity!=address();
}

void call_fact_domains(address context,CallFactHost host,CallFactInput input,address target,FactView key,u32 mode,CallFactCommon common) on cpu
 requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.cast_domain) requires nonzero(host.emit) {
 if(input.original==address()){return;}
 host.begin(context,input.original);ComparisonDomain domain=host.next(context);
 while(domain.present!=0){
  if(mode!=2||domain.common!=0){
   address value=domain.original;
   if(mode==1){value=host.cast_domain(context,value,common);}
   if(address_bits(value)!=0){FactView output=key;output.kind=FactKind::Index;output.first=target;output.payload=value;host.emit(context,output);}
  }
  domain=host.next(context);
 }
}
extern "C" void call_fact(address context,FactView fact,CallFactHost host) on cpu
 requires nonzero(host.input) requires nonzero(host.range) requires nonzero(host.boolean)
 requires nonzero(host.select_case) requires nonzero(host.include) requires nonzero(host.include_cases)
 requires nonzero(host.impossible) requires nonzero(host.emit) requires nonzero(host.commit) requires nonzero(host.count)
 requires nonzero(host.below) requires nonzero(host.mapped_domain) requires nonzero(host.common) requires nonzero(host.converted)
 requires nonzero(host.begin) requires nonzero(host.next) requires nonzero(host.cast_domain) requires nonzero(host.origin)
 requires nonzero(host.emit_common) requires nonzero(host.predicate) requires nonzero(host.comparison)
 requires nonzero(host.inspect_predicate) requires nonzero(host.scalar) requires nonzero(host.compare) {
 if(fact.kind==FactKind::Boolean||fact.kind==FactKind::Zero||fact.kind==FactKind::NonZero){
  u32 truth=0;if(fact.kind==FactKind::Boolean){truth=fact.flag;}if(fact.kind==FactKind::NonZero){truth=1;}
  CallFactInput input=host.input(context,fact.first);if(!has_call_input(input)){return;}
  if(truth==0&&input.nonzero!=0){host.impossible(context);return;}
  CallFactCase selected=host.select_case(input.original,truth);
  if(address_bits(selected.original)!=0&&selected.empty==0){host.include(context,selected.original);}
  if(address_bits(input.identity)!=0){FactView output=fact;output.first=input.identity;host.emit(context,output);host.commit(context);}
  return;
 }
 if(fact.kind==FactKind::Below){
  CallFactInput left=host.input(context,fact.first);CallFactInput right=host.input(context,fact.second);
  if(!has_call_input(left)||!has_call_input(right)){return;}
  if(address_bits(left.identity)!=0&&address_bits(right.identity)!=0){host.below(context,left,right,fact.flag,fact.scalar);return;}
  address mapped=host.comparison(context,fact.scalar,fact.first,fact.second,fact.flag);
  if(address_bits(mapped)!=0){FactView output=fact;output.kind=FactKind::Predicate;output.payload=mapped;output.flag=1;host.emit(context,output);host.commit(context);}
  return;
 }
 if(fact.kind==FactKind::Equal||fact.kind==FactKind::NotEqual){
  CallFactInput left=host.input(context,fact.first);CallFactInput right=host.input(context,fact.second);
  if(fact.kind==FactKind::Equal){
   if(!has_call_input(left)||!has_call_input(right)){return;}
   CallFactRange a=host.range(left.original,18446744073709551615u64,0);CallFactRange b=host.range(right.original,18446744073709551615u64,0);
   if(a.upper<b.lower||b.upper<a.lower){host.impossible(context);return;}
   if(address_bits(left.identity)==0||address_bits(right.identity)==0){return;}
   FactView output=fact;output.first=left.identity;output.second=right.identity;
   if(host.compare(output.first,output.second)>0){output.first=right.identity;output.second=left.identity;}
   host.emit(context,output);
   call_fact_domains(context,host,left,right.identity,fact,0,CallFactCommon(address(),address(),address(),address(),address()));
   call_fact_domains(context,host,right,left.identity,fact,0,CallFactCommon(address(),address(),address(),address(),address()));
   host.commit(context);host.include_cases(context,left.original);host.include_cases(context,right.original);return;
  }
  if(has_call_input(left)&&has_call_input(right)){
   u32 same=0;if(address_bits(left.identity)!=0&&address_bits(right.identity)!=0&&host.compare(left.identity,right.identity)==0){same=1;}
   u32 a_truth=host.boolean(left.original);u32 b_truth=host.boolean(right.original);
   CallFactRange a=host.range(left.original,18446744073709551615u64,0);CallFactRange b=host.range(right.original,18446744073709551615u64,0);
   if(same!=0||(a_truth==0&&b_truth==0)||(a.lower==a.upper&&b.lower==b.upper&&a.lower==b.lower)){host.impossible(context);return;}
   if((a_truth<2&&b_truth<2&&a_truth!=b_truth)||a.upper<b.lower||b.upper<a.lower){return;}
  }
  if(address_bits(left.identity)!=0&&address_bits(right.identity)!=0){
   FactView output=fact;output.first=left.identity;output.second=right.identity;
   if(host.compare(output.first,output.second)>0){output.first=right.identity;output.second=left.identity;}
   host.emit(context,output);host.commit(context);
  }
  return;
 }
 if(fact.kind==FactKind::Index||fact.kind==FactKind::Outside){
  CallFactInput target=host.input(context,fact.first);
  if(address_bits(target.identity)==0){return;}
  if(fact.kind==FactKind::Index){
   CallFactCommon common=host.common(fact.payload);
   if(address_bits(common.source)!=0){
    CallFactInput input=host.input(context,common.source);if(!has_call_input(input)){return;}
    CallFactConverted value=host.converted(input.original,common);InternalScalar scalar=host.scalar(common.to);
    if(scalar.kind==3){u32 truth=host.boolean(input.original);if(truth<2){FactView output=fact;output.kind=FactKind::Boolean;output.first=target.identity;output.flag=truth;host.emit(context,output);}}
    u32 nonzero=input.nonzero;
    if(scalar.kind==0||scalar.kind==1){nonzero=0;if(value.contains_zero==0||(value.known!=0&&value.value!=0u64)){nonzero=1;}}
    if(nonzero!=0){FactView output=fact;output.kind=FactKind::NonZero;output.first=target.identity;host.emit(context,output);}
    if(scalar.kind!=3){
     FactView output=fact;output.first=target.identity;output.scalar=common.to;
     if(value.lower!=0u64){output.kind=FactKind::Minimum;output.value=value.lower;host.emit(context,output);}
     if(value.upper!=scalar.mask){output.kind=FactKind::Maximum;output.value=value.upper;host.emit(context,output);}
    }
    call_fact_domains(context,host,input,target.identity,fact,1,common);
    usize at=0u64;CallFactOrigin source=host.origin(input,at);
    while(address_bits(source.original)!=0){if(source.element==0){host.emit_common(context,target.identity,source.original,common);}at+=1u64;source=host.origin(input,at);}
    if(host.count(context)!=0u64){host.commit(context);}return;
   }
  }
  address mapped=host.mapped_domain(context,fact.payload);
  if(address_bits(mapped)!=0){FactView output=fact;output.first=target.identity;output.payload=mapped;host.emit(context,output);host.commit(context);}return;
 }
 if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum){
  CallFactInput input=host.input(context,fact.first);
  if(has_call_input(input)){
   CallFactRange value=host.range(input.original,fact.mask,1);
   if(fact.kind==FactKind::Minimum){if(value.lower>=fact.value){return;}if(value.upper<fact.value){host.impossible(context);return;}}
   else{if(value.upper<=fact.value){return;}if(value.lower>fact.value){host.impossible(context);return;}}
  }
  if(address_bits(input.identity)!=0){FactView output=fact;output.first=input.identity;host.emit(context,output);host.commit(context);}return;
 }
 if(fact.kind==FactKind::Predicate){
  address mapped=host.predicate(context,fact.payload);if(address_bits(mapped)==0){return;}
  FactView output=fact;output.payload=mapped;host.emit(context,output);
  InternalPredicate original=host.inspect_predicate(fact.payload);u32 side=0;
  while(side<2){
   InternalOperand operand=original.left;if(side!=0){operand=original.right;}
   if(operand.is_constant==0){CallFactInput input=host.input(context,operand.identity);if(address_bits(input.identity)!=0){call_fact_domains(context,host,input,input.identity,fact,2,CallFactCommon(address(),address(),address(),address(),address()));}}
   side+=1;
  }
  host.commit(context);
 }
}

extern "C" void rename_output_graph(address context,OutputRenameHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.inspect) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.status) requires nonzero(host.begin_facts)
 requires nonzero(host.map_fact) requires nonzero(host.same_facts)
 requires nonzero(host.build) requires nonzero(host.emit) {
 while(true){
  address root=host.root(context);
  if(address_bits(root)==0u64){return;}
  host.push(context,root,0);
  while(true){
   NegatePending pending=host.pop(context);
   if(pending.present==0){break;}
   if(host.status(context,pending.source)!=OutputStatus::Pending){continue;}
   SourceNode node=host.inspect(pending.source);
   if(pending.ready==0&&node.kind>=2){
    host.push(context,pending.source,1);
    host.push(context,node.right,0);
    if(node.kind==4){host.push(context,node.no,0);}
    host.push(context,node.left,0);
    continue;
   }
   OutputBuild build=OutputBuild::Original;
   if(node.kind==1&&node.empty==0){
    host.begin_facts(context,node.left);
    while(host.map_fact(context)!=0){}
    if(host.same_facts(context,node.left)==0){build=OutputBuild::Facts;}
   }else if(node.kind>=2){
    bool left_same=host.status(context,node.left)==OutputStatus::Original;
    bool right_same=host.status(context,node.right)==OutputStatus::Original;
    if(node.kind==4){
     bool cases_same=right_same&&host.status(context,node.no)==OutputStatus::Original;
     if(!cases_same){build=OutputBuild::MappedBranch;}
     else if(!left_same){build=OutputBuild::Branch;}
    }else if(!left_same||!right_same){
     build=node.kind==2 ? OutputBuild::Both : OutputBuild::Either;
    }
   }
   host.build(context,pending.source,node,build);
  }
  host.emit(context,root);
 }
}

extern "C" void select_aggregate_proof(address context,usize choices,usize components,u32 publish,AggregateProofHost host) on cpu
 requires nonzero(host.project) requires nonzero(host.relate) requires nonzero(host.required)
 requires nonzero(host.refine) requires nonzero(host.join) {
 usize choice=0u64;
 while(choice<choices) {
  host.project(context,choice);
  // The result registers refer to the complete aggregate alternative, even
  // where its clause is infeasible under the current incoming path.
  if(publish!=0){host.relate(context);}
  if(host.required(context)!=0) {
   usize component=0u64;
   while(component<components) {
    host.refine(context,component);
    component+=1u64;
   }
   host.join(context);
  }
  choice+=1u64;
 }
}


extern "C" void bind_proof_path(address context,usize count,PathProofHost host) on cpu
 requires nonzero(host.component) requires nonzero(host.identity) requires nonzero(host.merge)
 requires nonzero(host.share) requires nonzero(host.boolean) requires nonzero(host.integer) {
 usize index=0u64;
 while(index<count) {
  PathProofComponent component=host.component(context,index);
  if(component.existing!=0){host.merge(context,index);}
  else if(component.boolean!=0||component.integer!=0) {
   address identity=host.identity(context,index);
   if(identity==address()){host.share(context,index);}
   else if(component.boolean!=0){host.boolean(context,index,identity);}
   else{host.integer(context,index,identity);}
  }
  index+=1u64;
 }
}


void apply_proof_domain(address context,address domain,ProofDomainView value,ProofDomainHost host) on cpu
 requires nonzero(host.nonzero) requires nonzero(host.bounded) requires nonzero(host.retain) {
 if(value.kind==ProofDomainKind::KnownBits&&value.value!=0u64){host.nonzero(context);}
 if(value.kind==ProofDomainKind::FixedLength){host.bounded(context,value.value);}
 else if(value.kind==ProofDomainKind::ResourceLength){host.bounded(context,4294967295u64);}
 host.retain(context,domain);
}

extern "C" void bind_proof_domain(address context,address domain,ProofDomainHost host) on cpu
 requires nonzero(host.describe) requires nonzero(host.nonzero) requires nonzero(host.bounded) requires nonzero(host.retain) {
 apply_proof_domain(context,domain,host.describe(domain),host);
}

extern "C" void refine_proof_fact(address context,ScalarEncoding scalar,u64 zero,FactView fact,ProofRefinementHost host) on cpu
 requires nonzero(host.domains.describe) requires nonzero(host.domains.nonzero)
 requires nonzero(host.domains.bounded) requires nonzero(host.domains.retain)
 requires nonzero(host.scalar) requires nonzero(host.deny) requires nonzero(host.range) {
 if(fact.kind==FactKind::Boolean&&scalar.kind==ScalarKind::Boolean) {
  host.deny(context,fact.flag==0?1:0);
 }else if(fact.kind==FactKind::Index&&scalar.kind==ScalarKind::Unsigned) {
  ProofDomainView value=host.domains.describe(fact.payload);
  if(scalar.width==32||value.kind==ProofDomainKind::Numeric||value.kind==ProofDomainKind::KnownBits) {
   apply_proof_domain(context,fact.payload,value,host.domains);
  }
 }else if(fact.kind==FactKind::Minimum||fact.kind==FactKind::Maximum) {
  ScalarEncoding expected=host.scalar(fact.scalar);
  if(scalar.kind==expected.kind&&scalar.width==expected.width) {
   if(fact.kind==FactKind::Minimum) {
    host.range(context,fact.value,fact.mask);
    if(scalar.kind!=ScalarKind::Signed&&fact.value!=0u64){host.domains.nonzero(context);}
   }else{host.range(context,0u64,fact.value);}
  }
 }else if(scalar.kind==ScalarKind::Unsigned||scalar.kind==ScalarKind::Signed) {
  if(fact.kind==FactKind::Zero){host.range(context,zero,zero);}
  else if(fact.kind==FactKind::NonZero){host.domains.nonzero(context);}
 }
}


extern "C" void join_proof_conditions(address context,usize width,u32 selected,ConditionJoinHost host) on cpu
 requires nonzero(host.present) requires nonzero(host.empty) requires nonzero(host.common) requires nonzero(host.selected) {
 usize index=0u64;
 while(index<width) {
  if(host.present(context,index)==0){host.empty(context);}
  else if(selected!=0){host.selected(context,index);}
  else{host.common(context,index);}
  index+=1u64;
 }
}

struct UpperGroup { UpperRow limit; UpperRow following; };

UpperGroup upper_group(address context,u32 side,UpperRow first,DomainJoinHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.compare) {
 u32 strongest=first.distance;
 UpperRow following=host.next(context,side);
 while(following.identity!=address()&&host.compare(first.identity,following.identity)==0) {
  // Each typed set orders equal-identity rows by ascending distance.
  strongest=following.distance;
  following=host.next(context,side);
 }
 return UpperGroup(UpperRow(first.identity,strongest),following);
}

extern "C" u32 join_proof_domains(address context,u32 bits,DomainJoinHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.compare) requires nonzero(host.origin)
 requires nonzero(host.upper) requires nonzero(host.bits) {
 UpperRow left=host.next(context,0);
 UpperRow right=host.next(context,1);
 while(left.identity!=address()||right.identity!=address()) {
  i32 order=i32(0);
  if(left.identity==address()){order=i32(1);}
  else if(right.identity==address()){order=-i32(1);}
  else{order=host.compare(left.identity,right.identity);}
  if(order<0) {
   UpperGroup a=upper_group(context,0,left,host);
   if(host.origin(context,1,a.limit.identity)!=0){host.upper(context,a.limit.identity,0);}
   left=a.following;
  }else if(order>0) {
   UpperGroup b=upper_group(context,1,right,host);
   if(host.origin(context,0,b.limit.identity)!=0){host.upper(context,b.limit.identity,0);}
   right=b.following;
  }else{
   UpperGroup a=upper_group(context,0,left,host);
   UpperGroup b=upper_group(context,1,right,host);
   u32 common=a.limit.distance<b.limit.distance?a.limit.distance:b.limit.distance;
   host.upper(context,a.limit.identity,common);
   left=a.following;
   right=b.following;
  }
 }
 return bits!=0?host.bits(context):u32(0);
}


extern "C" void join_proof_numeric(address context,usize width,NumericJoinHost host) on cpu
 requires nonzero(host.domains) requires nonzero(host.store) {
 usize component=0u64;
 while(component<width){
  NumericJoinFacts facts=host.domains(context,component);
  u32 nonzero=facts.bits_nonzero;
  if(facts.left_nonzero!=0&&facts.right_nonzero!=0){nonzero=1;}
  host.store(context,component,nonzero);
  component+=1u64;
 }
}

extern "C" u32 proof_length_relation(address context,u32 strict,LengthRelationHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.origin) requires nonzero(host.resource)
 requires nonzero(host.range) requires nonzero(host.sum) {
 u64 margin=u64(strict);
 IndexFields index=host.next(context,0);
 while(index.present!=0) {
  if((index.kind==1||index.kind==2||index.kind==3)&&host.resource(context,index)!=0){return 1;}
  if(index.distance>=margin) {
   if(index.kind==9&&host.origin(context,1,index.first)!=0){return 1;}
   if(index.kind==11&&(host.sum(context,index.first,index.second)!=0||host.sum(context,index.second,index.first)!=0)){return 1;}
  }
  index=host.next(context,0);
 }
 IndexFields length=host.next(context,1);
 while(length.present!=0) {
  if(length.kind==12) {
   if(length.distance>=margin&&(host.origin(context,0,length.first)!=0
    ||host.range(context,IndexOutput::Offset,length,0,u32(length.distance-margin))!=0)){return 1;}
   u32 minimum=length.distance>=margin?0:1;
   if(host.range(context,IndexOutput::Upper,length,minimum,4294967295)!=0){return 1;}
  }
  length=host.next(context,1);
 }
 return 0;
}


extern "C" void project_call_proof(address context,ProofCallWidths widths,ProofCallHost host) on cpu
 requires nonzero(host.begin) requires nonzero(host.input) requires nonzero(host.merge_input) requires nonzero(host.keep_origin)
 requires nonzero(host.domain) requires nonzero(host.direct) requires nonzero(host.common) requires nonzero(host.derived)
 requires nonzero(host.false_case) requires nonzero(host.possible) requires nonzero(host.nonzero) {
 usize component=0u64;
 while(component<widths.origins||component<widths.domains||component<widths.conditions) {
  bool has_origins=component<widths.origins;
  bool has_domains=component<widths.domains;
  host.begin(context,component,has_origins?1:0,(has_origins||has_domains)?1:0);
  if(has_origins) {
   CallFactInput input=host.input(context);
   while(has_call_input(input)) {
    if(input.original!=address()){host.merge_input(context,input.original);}
    else{host.keep_origin(context,input.identity);}
    input=host.input(context);
   }
  }
  if(has_domains) {
   ProofCallDomain next=host.domain(context);
   while(next.original!=address()) {
    if(next.kind==1){host.common(context,next.original);}
    else if(next.kind==2){host.derived(context,next.original);}
    else{host.direct(context,next.original);}
    next=host.domain(context);
   }
  }
  address clause=host.false_case(context);
  if(clause!=address()&&host.possible(clause)==0){host.nonzero(context);}
  component+=1u64;
 }
}

extern "C" void capture_selected_proof(address context,address indices,usize count,
 usize width,usize registers,CaptureProofHost host) on cpu
 requires nonzero(host.all) requires nonzero(host.component) {
 Span selected=Span(indices,count);
 if(count==width) {
  usize index=0u64;
  while(index<count&&word_at(selected,index)==index) { index+=1u64; }
  if(index==count) { host.all(context); return; }
 }
 usize index=0u64;
 while(index<count) {
  usize component=word_at(selected,index);
  host.component(context,component,component<registers?1:0);
  index+=1u64;
 }
}

extern "C" void bind_output_proof(address context,ProofOutputShape shape,ProofOutputHost host) on cpu
 requires nonzero(host.bind_address) requires nonzero(host.bind_view)
 requires nonzero(host.origins) requires nonzero(host.domains) requires nonzero(host.conditions) {
 usize index=0u64;
 while(index<shape.addresses){host.bind_address(context,index);index+=1u64;}
 index=0u64;
 while(index<shape.views){host.bind_view(context,index);index+=1u64;}
 index=0u64;
 while(index<shape.origins){host.origins(context,index);index+=1u64;}
 index=0u64;
 while(index<shape.domains){host.domains(context,index);index+=1u64;}
 host.conditions(context);
}
