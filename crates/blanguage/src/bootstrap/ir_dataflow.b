module bootstrap.ir_dataflow;

// These tags describe the existing borrowed Rust IR fields at this one
// private native handoff. They do not name a public IR format or registry.
const u32 N_CALL=1;const u32 N_TARGET=2;
const u32 N_DATASET_BORROW=4;const u32 N_REFERENCE=5;const u32 N_STORAGE=6;
const u32 N_INDEXED=7;const u32 N_STRIDE=8;const u32 N_TEXTURE=9;const u32 N_ATOMIC_OP=10;
const u32 N_DIVISION=13;
const u32 N_DIVISOR=14;const u32 N_WORDS3=15;
const u32 N_COMPONENT=16;
const u32 N_STAGE=17;const u32 N_RAY_RANGE=18;const u32 N_TILE=25;

Span empty_span() on cpu {return Span(unsafe_address(0u64),0u64,0u64);}
Node empty_node() on cpu {return Node(unsafe_address(0u64),0);}
Word absent() on cpu {return Word(0,0u64);}
Word found(usize value) on cpu {return Word(1,value);}
usize word_at(Span span,usize index) on cpu {
 return unsafe_position<usize>(unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride)));
}
Node node_at(Span span,usize index,u32 kind) on cpu {
 return Node(unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride)),kind);
}

extern "C" usize next_loop_edit(Span code,usize first,Host host) on cpu
 requires nonzero(host.inspect) {
 usize index=first;
 while(index<code.count){
  u32 kind=host.inspect(node_at(code,index,N_INSTRUCTION)).tag;
  if(kind==OP_BREAK_VALUE||kind==OP_CONTINUE_VALUE||kind==OP_BRANCH||kind==OP_SCOPE){return index;}
  index+=1u64;
 }
 return code.count;
}

enum TransferSearch:u32 { LoopLocal=0; LoopAndExit=1; FunctionExit=2; };

bool block_transfers(Span code,TransferSearch search,Host host) on cpu
 requires nonzero(host.inspect) {
 usize i=0u64;
 while(i<code.count){
  View instruction=host.inspect(node_at(code,i,N_INSTRUCTION));
  i+=1u64;
  u32 kind=instruction.tag;
  if(kind==OP_EXIT_VALUE && search!=TransferSearch::LoopLocal){return true;}
  if((kind==OP_BREAK_VALUE||kind==OP_CONTINUE_VALUE) && search!=TransferSearch::FunctionExit){return true;}
  if(kind==OP_BRANCH || (kind==OP_LOOP_VALUE && search==TransferSearch::FunctionExit)){
   if(block_transfers(instruction.first,search,host)||block_transfers(instruction.second,search,host)){return true;}
  }
  if(kind==OP_SCOPE && search!=TransferSearch::FunctionExit
     && block_transfers(instruction.first,TransferSearch::LoopLocal,host)){return true;}
 }
 return false;
}

extern "C" u32 has_function_exit(Span code,Host host) on cpu requires nonzero(host.inspect) {
 return u32(block_transfers(code,TransferSearch::FunctionExit,host));
}

extern "C" void loop_path_branch(Span yes_code,Span no_code,LoopPaths parent,
 LoopPaths yes,LoopPaths no,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope) {
 loop_exit_paths(yes_code,yes,host);
 loop_exit_paths(no_code,no,host);
 host.union_paths(parent.live,yes.live,no.live);
 host.union_paths(parent.exited,parent.exited,yes.exited);
 host.union_paths(parent.exited,parent.exited,no.exited);
}

extern "C" void loop_path_scope(Span code,LoopPaths parent,LoopPaths child,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope) {
 loop_exit_paths(code,child,host);
 host.union_paths(parent.live,child.live,child.exited);
}

extern "C" void loop_exit_paths(Span code,LoopPaths paths,LoopPathHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.record_break) requires nonzero(host.clear)
 requires nonzero(host.union_paths) requires nonzero(host.with_branch) requires nonzero(host.with_scope) {
 usize i=0u64;
 while(i<code.count){
  View instruction=host.ir.inspect(node_at(code,i,N_INSTRUCTION));
  i+=1u64;
  u32 kind=instruction.tag;
  if(kind==OP_BREAK_VALUE){host.record_break(host.context,paths.live);}
  else if(kind==OP_CONTINUE_VALUE){host.clear(paths.live);}
  else if(kind==OP_EXIT_VALUE){host.union_paths(paths.exited,paths.exited,paths.live);}
  else if(kind==OP_BRANCH&&(block_transfers(instruction.first,TransferSearch::LoopAndExit,host.ir)
                         ||block_transfers(instruction.second,TransferSearch::LoopAndExit,host.ir))){
   host.with_branch(instruction.a,instruction.first,instruction.second,paths,host);
  }else if(kind==OP_SCOPE&&block_transfers(instruction.first,TransferSearch::LoopLocal,host.ir)){
   host.with_scope(instruction.first,paths,host);
  }
 }
}


extern "C" void project_loop_borrow(address context, usize scope, LoopBorrowHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.source)
 requires nonzero(host.replace) requires nonzero(host.retain) {
 while (true) {
  LoopBorrowEdge edge = host.inspect(context);
  if (edge.present == 0) { return; }
  if (edge.level == scope) {
   address source = host.source(context, edge.input);
   if (source != unsafe_address(0u64)) {
    host.replace(context, source);
    continue;
   }
  }
  host.retain(context);
 }
}

void ordinary_results(address value,address output) on cpu {
 u32 kind=unsafe_position<View>(value).tag;
 if(kind==OP_INVOKE||kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH||kind==OP_TEXTURE_SIZE
     ||kind==OP_RAY_QUERY||kind==OP_TILE_COLLECTIVE){
  unsafe_position<Results>(output).first=unsafe_position<View>(value).first;
  if(kind==OP_INVOKE){unsafe_position<Results>(output).second=unsafe_position<View>(value).second;}
 }else if(kind==OP_ATOMIC){
  unsafe_position<Results>(output).first=unsafe_position<View>(value).first;
 }else if(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
     ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ||kind==OP_APPEND||kind==OP_CONSTANT
     ||kind==OP_FUNCTION_ADDRESS||kind==OP_INDEX_VALUE||kind==OP_UNIFORM||kind==OP_BUFFER_STRIDE
     ||kind==OP_LOAD_LOCAL||kind==OP_TILE_LENGTH||kind==OP_TILE_LANE||kind==OP_POSITION_ADDRESS
     ||kind==OP_LOAD_INDEXED||kind==OP_LOAD||kind==OP_LOAD_LOCAL_INDEXED||kind==OP_SUBGROUP
     ||kind==OP_PRINT||kind==OP_TILE_LOAD||kind==OP_VIEW_LOAD||kind==OP_UNARY||kind==OP_BINARY
     ||kind==OP_CAST||kind==OP_MATH||kind==OP_SELECT_VALUE){
  unsafe_position<Results>(output).a=unsafe_position<View>(value).a;unsafe_position<Results>(output).inline_count=1u64;
 }
}
// Both destinations are the result cursor allocated by the actual caller.
void division_results(usize a,usize b,u32 kind,address output) on cpu {
 unsafe_position<Results>(output).a=a;
 unsafe_position<Results>(output).b=b;
 unsafe_position<Results>(output).inline_count=kind==2 ? 2u64 : 1u64;
}
Results view_results(View value,Host host) on cpu requires nonzero(host.inspect) {
 Results output=Results(empty_span(),empty_span(),0u64,0u64,0u64,0u64);
 if(value.tag==OP_INTEGER_DIVIDE){
  View division=host.inspect(value.right);
  division_results(division.a,division.b,division.tag,address_of(output));
 }else{ordinary_results(address_of(value),address_of(output));}
 return output;
}
extern "C" Results results(Node source,Host host) on cpu requires nonzero(host.inspect) {
 return view_results(host.inspect(source),host);
}
extern "C" Word next_result(inout Results state) on cpu {
 usize index=state.index;
 if(index<state.first.count){state.index+=1u64;return found(word_at(state.first,index));}
 index-=state.first.count;
 if(index<state.second.count){state.index+=1u64;return found(word_at(state.second,index));}
 index-=state.second.count;
 if(index<state.inline_count){state.index+=1u64;return found(index==0u64 ? state.a : state.b);}
 return absent();
}

u32 visit_result_origins(address value,Host host,address context,
 cfn<u32,address,usize,usize,usize,ResultRole> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 Results output=Results(empty_span(),empty_span(),0u64,0u64,0u64,0u64);
 ResultRole role=ResultRole::Value;
 if(unsafe_position<View>(value).tag==OP_INTEGER_DIVIDE){
  View division=host.inspect(unsafe_position<View>(value).right);
  division_results(division.a,division.b,division.tag,address_of(output));
  role=division.tag==1 ? ResultRole::Remainder : ResultRole::Quotient;
 }else{ordinary_results(value,address_of(output));}
 usize count=output.first.count+output.second.count+output.inline_count;
 usize index=0u64;
 u32 attached=0;
 while(index<count){
  Word entry=next_result(output);
  attached|=visit(context,entry.value,index,count,role);
  if(role==ResultRole::Quotient){role=ResultRole::Remainder;}
  index+=1u64;
 }
 return attached;
}
extern "C" void result_origins(Node source,Host host,address context,
 cfn<u32,address,usize,usize,usize,ResultRole> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);
 visit_result_origins(address_of(value),host,context,visit);
}

u32 visit_span(Span values,address context,cfn<u32,address,usize> visit) on cpu requires nonzero(visit) {
 usize index=0u64;
 while(index<values.count){if(visit(context,word_at(values,index))==0){return 0;}index+=1u64;}
 return 1;
}
u32 visit_stride(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);
 return value.tag==1 ? visit_span(value.first,context,visit) : 1;
}
u32 visit_indexed(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);
 if(value.tag==0||value.tag==3){return visit(context,value.b);}
 if(visit_span(value.first,context,visit)==0){return 0;}
 if(value.tag==1&&visit_stride(value.left,host,context,visit)==0){return 0;}
 return visit(context,value.a);
}
u32 visit_reference(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);
 if(visit_span(value.first,context,visit)==0){return 0;}
 View storage=host.inspect(value.left);
 return storage.tag==1 ? visit_indexed(storage.left,host,context,visit) : 1;
}
u32 visit_call(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);View target=host.inspect(value.left);
 if(target.tag==2&&visit(context,target.a)==0){return 0;}
 usize index=0u64;
 while(index<value.first.count){
  View argument=host.inspect(node_at(value.first,index,N_ARGUMENT));
  if(argument.tag==0){
   if(visit_span(argument.first,context,visit)==0){return 0;}
   if(address_bits(argument.left.data)!=0u64){
    View borrow=host.inspect(argument.left);
    if(visit_span(borrow.first,context,visit)==0){return 0;}
    if(visit_span(borrow.second,context,visit)==0){return 0;}
   }
  }else if(visit_reference(argument.left,host,context,visit)==0){return 0;}
  index+=1u64;
 }
 return 1;
}
u32 visit_instruction(View value,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 u32 kind=value.tag;u32 continued=1;
 if(kind==OP_INVOKE){continued=visit_call(value.left,host,context,visit);}
 else if(kind==OP_RAISE){continued=visit(context,value.a);}
 else if(kind==OP_FUNCTION_ADDRESS){
  View target=host.inspect(value.left);continued=target.tag==2 ? visit(context,target.a) : 1;
 }else if(kind==OP_POSITION_ADDRESS){continued=visit_reference(value.left,host,context,visit);}
 else if(kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH){
  continued=visit_span(value.second,context,visit);
  if(continued!=0){continued=visit(context,value.a);}
 }else if(kind==OP_TEXTURE_READ){
  continued=visit_span(value.first,context,visit);
  if(continued!=0){continued=visit(context,value.b);}
 }else if(kind==OP_TEXTURE_SIZE){continued=visit(context,value.a);}
 else if(kind==OP_APPEND||kind==OP_TILE_PUBLISH){continued=visit_span(value.first,context,visit);}
 else if(kind==OP_ATOMIC){
  View op=host.inspect(value.right);
  if(op.tag!=0){
   continued=visit(context,op.a);
   if(continued!=0&&op.tag==5){continued=visit(context,op.b);}
  }
  if(continued!=0){continued=visit_indexed(value.left,host,context,visit);}
  if(continued!=0){continued=visit_span(value.second,context,visit);}
 }else if(kind==OP_RAY_QUERY){
  continued=visit_span(value.second,context,visit);
  if(continued!=0){View direction=host.inspect(value.left);continued=visit_span(direction.first,context,visit);}
  if(continued!=0){continued=visit(context,value.a);}
  if(continued!=0){continued=visit(context,value.b);}
  if(continued!=0&&value.c!=0u64){continued=visit(context,value.d);}
 }else if(kind==OP_LOAD_INDEXED||kind==OP_STORE_INDEXED){
  continued=visit_indexed(value.left,host,context,visit);
  if(continued!=0){continued=visit(context,kind==OP_LOAD_INDEXED ? value.b : value.a);}
  if(continued!=0&&kind==OP_STORE_INDEXED){continued=visit(context,value.b);}
 }else if(kind==OP_STORE_LOCAL||kind==OP_PRINT){continued=visit(context,value.b);}
 else if(kind==OP_LOAD_LOCAL_INDEXED){continued=visit(context,value.d);}
 else if(kind==OP_LOAD||kind==OP_TILE_LOAD){continued=visit(context,value.c);}
 else if(kind==OP_STORE_LOCAL_INDEXED||kind==OP_STORE){
  continued=visit(context,kind==OP_STORE_LOCAL_INDEXED ? value.c : value.b);
  if(continued!=0){continued=visit(context,value.d);}
 }else if(kind==OP_SUBGROUP){
  continued=visit_span(value.first,context,visit);
  if(continued!=0){continued=visit_span(value.second,context,visit);}
 }else if(kind==OP_VIEW_LOAD){
  continued=visit_span(value.first,context,visit);
  if(continued!=0){continued=visit_stride(value.left,host,context,visit);}
  if(continued!=0){continued=visit(context,value.b);}
 }else if(kind==OP_UNARY||kind==OP_CAST){continued=visit(context,value.b);}
 else if(kind==OP_INTEGER_DIVIDE){
  continued=visit(context,value.a);
  if(continued!=0){View divisor=host.inspect(value.left);if(divisor.tag==1){continued=visit(context,divisor.a);}}
 }else if(kind==OP_BINARY){
  continued=visit(context,value.b);
  if(continued!=0){continued=visit(context,value.c);}
 }else if(kind==OP_MATH){continued=visit_span(value.first,context,visit);}
 else if(kind==OP_SELECT_VALUE){
  continued=visit(context,value.b);
  if(continued!=0){continued=visit(context,value.c);}
  if(continued!=0){continued=visit(context,value.d);}
 }else if(kind==OP_BRANCH||kind==OP_LOOP_VALUE){continued=visit(context,value.a);}
 if(continued!=0&&(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH
    ||kind==OP_TEXTURE_SIZE||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
    ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ)){
  View texture=host.inspect(value.left);continued=visit_span(texture.first,context,visit);
  if(continued!=0){continued=visit_span(texture.second,context,visit);}
 }
 return continued;
}
extern "C" u32 operands(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 if(source.kind==N_CALL){return visit_call(source,host,context,visit);}
 if(source.kind==N_REFERENCE){return visit_reference(source,host,context,visit);}
 if(source.kind==N_INDEXED){return visit_indexed(source,host,context,visit);}
 if(source.kind==N_STRIDE){return visit_stride(source,host,context,visit);}
 return visit_instruction(host.inspect(source),host,context,visit);
}
extern "C" u32 parallel_frame_operand(address context,usize reg) on cpu {
 Span frame=unsafe_position<Span>(context);usize index=0u64;
 while(index<frame.count){if(word_at(frame,index)==reg){return 0;}index+=1u64;}
 return 1;
}
extern "C" u32 parallel_frame_used(Span code,Span frame,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(visit_instruction(value,host,address_of(frame),c_function(parallel_frame_operand))==0){return 1;}
  if(value.tag==OP_SCOPE||value.tag==OP_CLEANUP||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   if(parallel_frame_used(value.first,frame,host)!=0){return 1;}
   if(parallel_frame_used(value.second,frame,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
extern "C" u32 parallel_scope_required(Span code,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if((value.tag==OP_INVOKE||value.tag==OP_CLEANUP)&&value.a!=0u64){return 1;}
  if(value.tag==OP_SCOPE||value.tag==OP_CLEANUP||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   if(parallel_scope_required(value.first,host)!=0){return 1;}
   if(parallel_scope_required(value.second,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}

extern "C" bool accessed(usize access,u32 writing) on cpu {
 return writing!=0 ? access==2u64||access==3u64||access==4u64 : access==1u64||access==3u64||access==4u64;
}
struct RangePart { u32 more; u32 local; usize begin; usize end; };
RangePart no_range() on cpu {return RangePart(0,0,0u64,0u64);}
RangePart indexed_alias_range(Node source,usize index,Host host) on cpu requires nonzero(host.inspect) {
 View value=host.inspect(source);
 if(value.tag!=INDEXED_POINTER||index>=value.second.count){return no_range();}
 View alias=host.inspect(node_at(value.second,index,N_MEMORY_ALIAS));
 return RangePart(1,alias.tag==ALIAS_LOCAL ? 1 : 0,alias.a,alias.a+alias.b);
}
RangePart alias_range(Node source,usize index,Host host) on cpu requires nonzero(host.inspect) {
 if(source.kind==N_REFERENCE){
  View reference=host.inspect(source);View storage=host.inspect(reference.left);
  if(storage.tag==0){return index==0u64 ? RangePart(1,1,storage.a,storage.a+storage.b) : no_range();}
  return indexed_alias_range(storage.left,index,host);
 }
 return indexed_alias_range(source,index,host);
}
Locals view_locals(View value,u32 writing,Host host) on cpu requires nonzero(host.inspect) {
 Locals state=Locals(empty_node(),0,writing,0u64,0u64,0u64,0u64,0u64,0);
 u32 kind=value.tag;
 if((kind==OP_LOAD_LOCAL&&writing==0)||(kind==OP_STORE_LOCAL&&writing!=0)){
  state.mode=1;state.position=kind==OP_LOAD_LOCAL ? value.b : value.a;
 }else if((kind==OP_LOAD_LOCAL_INDEXED&&writing==0)||(kind==OP_STORE_LOCAL_INDEXED&&writing!=0)){
  state.mode=2;state.position=kind==OP_LOAD_LOCAL_INDEXED ? value.b : value.a;
  state.end=state.position+(kind==OP_LOAD_LOCAL_INDEXED ? value.c : value.b);
 }else if(kind==OP_INVOKE){state.mode=4;state.source=value.left;}
 else if((kind==OP_LOAD_INDEXED&&writing==0)||(kind==OP_STORE_INDEXED&&writing!=0)){
  state.mode=3;state.source=value.left;
 }else if(kind==OP_ATOMIC){
  View op=host.inspect(value.right);
  if(writing!=0 ? op.tag!=0 : op.tag!=1){state.mode=3;state.source=value.left;}
 }
 return state;
}
extern "C" Locals locals(Node source,u32 writing,Host host) on cpu requires nonzero(host.inspect) {
 return view_locals(host.inspect(source),writing,host);
}
extern "C" Word next_local(inout Locals state,Host host) on cpu requires nonzero(host.inspect) {
 if(state.mode==0){return absent();}
 if(state.mode==1){state.mode=0;return found(state.position);}
 if(state.mode==2){
  if(state.position>=state.end){state.mode=0;return absent();}
  usize value=state.position;state.position+=1u64;return found(value);
 }
 while(true){
  if(state.active!=0&&state.position<state.end){
   usize value=state.position;state.position+=1u64;return found(value);
  }
  state.active=0;
  if(state.mode==3){
   RangePart part=alias_range(state.source,state.alias,host);
   if(part.more==0){state.mode=0;return absent();}
   state.alias+=1u64;
   if(part.local!=0){state.position=part.begin;state.end=part.end;state.active=1;}
   continue;
  }
  View call_value=host.inspect(state.source);
  if(state.phase==0u64){
   if(state.entry>=call_value.first.count){state.phase=1u64;state.entry=0u64;state.alias=0u64;continue;}
   View parameter=host.inspect(node_at(call_value.first,state.entry,N_ARGUMENT));
   if(parameter.tag!=1||!accessed(parameter.a,state.writing)){
    state.entry+=1u64;state.alias=0u64;continue;
   }
   RangePart part=alias_range(parameter.left,state.alias,host);
   if(part.more==0){state.entry+=1u64;state.alias=0u64;continue;}
   state.alias+=1u64;
   if(part.local!=0){state.position=part.begin;state.end=part.end;state.active=1;}
   continue;
  }
  if(state.entry>=call_value.second.count){state.mode=0;return absent();}
  View effect=host.inspect(node_at(call_value.second,state.entry,N_MEMORY_EFFECT));state.entry+=1u64;
  if(accessed(effect.a,state.writing)){
   View alias=host.inspect(effect.left);
   if(alias.tag==0){state.position=alias.a;state.end=alias.a+alias.b;state.active=1;}
  }
 }
}
extern "C" void visit_local_writes(Span code,Host host,address context,cfn<void,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  Locals writes=view_locals(value,1,host);
  Word slot=next_local(writes,host);
  while(slot.present!=0){
   visit(context,slot.value);
   slot=next_local(writes,host);
  }
  if(value.tag==OP_SCOPE||value.tag==OP_CLEANUP){
   visit_local_writes(value.first,host,context,visit);
  }else if(value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   visit_local_writes(value.first,host,context,visit);
   visit_local_writes(value.second,host,context,visit);
  }
  index+=1u64;
 }
}
void number_used_slots(address slots,u64 count) on cpu {
 u64 used=0u64;u64 index=0u64;
 while(index<count){
  address at=unsafe_address(address_bits(slots)+index*size_of<u64>());
  if(unsafe_position<u64>(at)!=0u64){used+=1u64;unsafe_position<u64>(at)=used;}
  index+=1u64;
 }
}
address compact_at(address slots,usize index) on cpu {
 return unsafe_address(address_bits(slots)+u64(index)*size_of<usize>());
}
usize compact_index(address slots,usize index) on cpu {
 return unsafe_position<usize>(compact_at(slots,index));
}
extern "C" u32 compact_mark_register(address marks,usize reg) on cpu {
 unsafe_position<usize>(compact_at(marks,reg))=1u64;return 1;
}
extern "C" void compact_range(address marks,usize begin,usize end) on cpu {
 usize position=begin;
 while(position<end){unsafe_position<usize>(compact_at(marks,position))=1u64;position+=1u64;}
}
void visit_addressable_aliases(Node source,Host host,address context,
                               cfn<void,address,usize,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit) {
 usize index=0u64;
 while(true){
  RangePart part=alias_range(source,index,host);
  if(part.more==0){return;}
  if(part.local!=0 && part.begin!=part.end){emit(context,part.begin,part.end);}
  index+=1u64;
 }
}
void visit_addressable_locals(View value,Host host,address context,
                               cfn<void,address,usize,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit) {
 if(value.tag==OP_LOAD_LOCAL_INDEXED){emit(context,value.b,value.b+value.c);}
 else if(value.tag==OP_STORE_LOCAL_INDEXED){emit(context,value.a,value.a+value.b);}
 else if(value.tag==OP_INVOKE){
  View call=host.inspect(value.left);
  usize index=0u64;
  // Passing an address preserves the entire local range, including when
  // the call itself has no read or write effect on that range.
  while(index<call.first.count){
   View argument=host.inspect(node_at(call.first,index,N_ARGUMENT));
   if(argument.tag==1){visit_addressable_aliases(argument.left,host,context,emit);}
   index+=1u64;
  }
  index=0u64;
  while(index<call.second.count){
   View effect=host.inspect(node_at(call.second,index,N_MEMORY_EFFECT));
   View alias=host.inspect(effect.left);
   if(alias.tag==0 && alias.b!=0u64){emit(context,alias.a,alias.a+alias.b);}
   index+=1u64;
  }
 }else if(value.tag==OP_POSITION_ADDRESS||value.tag==OP_LOAD_INDEXED
     ||value.tag==OP_STORE_INDEXED||value.tag==OP_ATOMIC){
  visit_addressable_aliases(value.left,host,context,emit);
 }
}
void compact_locals(View value,address marks,Host host) on cpu requires nonzero(host.inspect) {
 if(value.tag==OP_LOAD_LOCAL){compact_range(marks,value.b,value.b+1u64);}
 else if(value.tag==OP_STORE_LOCAL){compact_range(marks,value.a,value.a+1u64);}
 else{visit_addressable_locals(value,host,marks,c_function(compact_range));}
}
void compact_marks(Span code,address regs,address locals,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count){
  Node node=node_at(code,index,N_INSTRUCTION);Results output=results(node,host);
  while(true){Word reg=next_result(output);if(reg.present==0){break;}compact_mark_register(regs,reg.value);}
  operands(node,host,regs,c_function(compact_mark_register));
  View value=host.inspect(node);compact_locals(value,locals,host);
  if(value.tag==OP_SCOPE||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   compact_marks(value.first,regs,locals,host);compact_marks(value.second,regs,locals,host);
  }
  index+=1u64;
 }
}
void compact_roots(Span roots,address marks,u32 rewriting) on cpu {
 usize index=0u64;
 while(index<roots.count){
  address at=unsafe_address(address_bits(roots.data)+u64(index)*u64(roots.stride));
  usize original=unsafe_position<usize>(at);
  if(rewriting!=0){unsafe_position<usize>(at)=compact_index(marks,original)-1u64;}
  else{compact_mark_register(marks,original);}
  index+=1u64;
 }
}
usize compact_registers(Span values,address marks) on cpu {
 usize input=0u64;usize output=0u64;
 while(input<values.count){
  if(compact_index(marks,input)!=0u64){
   usize byte=0u64;
   while(byte<values.stride){
    address from=unsafe_address(address_bits(values.data)+u64(input)*u64(values.stride)+u64(byte));
    address to=unsafe_address(address_bits(values.data)+u64(output)*u64(values.stride)+u64(byte));
    unsafe_position<u8>(to)=unsafe_position<u8>(from);byte+=1u64;
   }
   output+=1u64;
  }
  input+=1u64;
 }
 return output;
}
extern "C" usize compact_slots(CompactInput input,CompactHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.next)
 requires nonzero(host.whole) requires nonzero(host.scalar) requires nonzero(host.rewrite) {
 compact_roots(input.inputs,input.register_map,0);compact_roots(input.outputs,input.register_map,0);
 compact_marks(input.code,input.register_map,input.local_map,host.ir);
 number_used_slots(input.register_map,u64(input.registers.count));
 number_used_slots(input.local_map,u64(input.local_count));
 usize kept=compact_registers(input.registers,input.register_map);
 while(true){
  CompactDecl item=host.next(host.context);if(item.present==0){break;}
  if(item.count==0u64){continue;}
  usize begin=item.end-item.count;
  usize first=compact_index(input.local_map,begin);
  usize last=compact_index(input.local_map,item.end-1u64);
  if(first!=0u64&&last==first+item.count-1u64){host.whole(host.context,last);}
  else{
   usize index=0u64;
   while(index<item.count){
    usize position=compact_index(input.local_map,begin+index);
    if(position!=0u64){host.scalar(host.context,index,position);}
    index+=1u64;
   }
  }
 }
 // No read-only instruction view or original local declaration remains live.
 host.rewrite(host.context);
 compact_roots(input.inputs,input.register_map,1);compact_roots(input.outputs,input.register_map,1);
 return kept;
}


address reduction_byte(address uniform,usize reg) on cpu {return unsafe_address(address_bits(uniform)+u64(reg));}
extern "C" u32 reduction_uniform_operand(address uniform,usize reg) on cpu {
 return unsafe_position<u8>(reduction_byte(uniform,reg))!=u8(0)?1:0;
}
extern "C" ReductionExtract reduction_extract(Node source,address uniform,Host ir) on cpu requires nonzero(ir.inspect) {
 View value=ir.inspect(source);u32 operation;usize inputs=0u64;
 if(value.tag==OP_CONSTANT){operation=0;}
 else if(value.tag==OP_UNIFORM){operation=1;}
 else if(value.tag==OP_BUFFER_STRIDE){operation=2;}
 else if(value.tag==OP_UNARY){operation=3;inputs=1u64;}
 else if(value.tag==OP_BINARY){operation=4;inputs=2u64;}
 else if(value.tag==OP_CAST){operation=5;inputs=1u64;}
 else if(value.tag==OP_MATH){operation=6;inputs=value.first.count;}
 else if(value.tag==OP_SELECT_VALUE){operation=7;inputs=3u64;}
 else if(value.tag==OP_INTEGER_DIVIDE&&ir.inspect(value.left).tag==0){operation=8;inputs=1u64;}
 else{return ReductionExtract(0,0,0u64);}
 if(operands(source,ir,uniform,c_function(reduction_uniform_operand))==0){return ReductionExtract(0,0,0u64);}
 Results output=results(source,ir);Word next=next_result(output);
 while(next.present!=0){unsafe_position<u8>(reduction_byte(uniform,next.value))=u8(1);next=next_result(output);}
 return ReductionExtract(1,operation,inputs);
}
void reduction_copy_roots(Span values,usize offset,usize root,ReductionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.header) requires nonzero(host.clone) requires nonzero(host.remap) requires nonzero(host.push_root) requires nonzero(host.roots) requires nonzero(host.part) requires nonzero(host.uniform) requires nonzero(host.extract) requires nonzero(host.release_uniform) requires nonzero(host.entry) requires nonzero(host.seed) requires nonzero(host.body) requires nonzero(host.retain) requires nonzero(host.forward) requires nonzero(host.origin) requires nonzero(host.alias) requires nonzero(host.invariant) requires nonzero(host.boundaries) requires nonzero(host.append) requires nonzero(host.finish) {
 usize index=0u64;
 while(index<values.count){host.push_root(host.context,root,word_at(values,index)+offset);index+=1u64;}
}
extern "C" void reduction_kernel(ReductionInput input,ReductionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.header) requires nonzero(host.clone) requires nonzero(host.remap) requires nonzero(host.push_root) requires nonzero(host.roots) requires nonzero(host.part) requires nonzero(host.uniform) requires nonzero(host.extract) requires nonzero(host.release_uniform) requires nonzero(host.entry) requires nonzero(host.seed) requires nonzero(host.body) requires nonzero(host.retain) requires nonzero(host.forward) requires nonzero(host.origin) requires nonzero(host.alias) requires nonzero(host.invariant) requires nonzero(host.boundaries) requires nonzero(host.append) requires nonzero(host.finish) {
 View source=host.ir.inspect(input.source);
 if(input.identity!=0){
  host.clone(host.context,1u64,source.left,0u64);
  View identity=host.ir.inspect(source.left);
  reduction_copy_roots(host.ir.inspect(identity.right).first,0u64,2u64,host);
 }
 bool project=input.project!=0&&source.first.count!=0u64;
 if(project){
  Node value=node_at(source.first,0u64,22);ReductionHeader base=host.header(host.context);
  host.clone(host.context,2u64,value,base.locals);
  host.remap(host.context,2u64,base.registers,base.locals);
  View code=host.ir.inspect(value);
  reduction_copy_roots(host.ir.inspect(code.left).first,base.registers,0u64,host);
  reduction_copy_roots(host.ir.inspect(code.right).first,base.registers,1u64,host);
 }
 ReductionHeader base=host.header(host.context);
 host.clone(host.context,3u64,source.right,base.locals);
 host.remap(host.context,3u64,base.registers,base.locals);
 View combine=host.ir.inspect(source.right);Span inputs=host.ir.inspect(combine.left).first;
 usize index=0u64;
 while(index<inputs.count){
  usize reg=word_at(inputs,index)+base.registers;
  if(index<source.a){host.push_root(host.context,3u64,reg);}
  else{
   host.push_root(host.context,4u64,reg);
   if(!project){host.push_root(host.context,0u64,reg);host.push_root(host.context,1u64,reg);}
  }
  index+=1u64;
 }
 reduction_copy_roots(host.ir.inspect(combine.right).first,base.registers,5u64,host);
 address uniform=host.uniform(host.context,host.header(host.context).registers);
 usize partition=1u64;
 while(partition<4u64){host.extract(host.context,partition,uniform);partition+=1u64;}
 host.release_uniform(host.context);
 host.entry(host.context);host.seed(host.context);
 partition=1u64;
 while(partition<4u64){host.body(host.context,partition,partition==3u64?1:0);partition+=1u64;}
 partition=0u64;
 while(partition<4u64){host.retain(host.context,partition);partition+=1u64;}
 // Origin facts consume the same original roots before the binding rewrite.
 index=0u64;
 while(index<3u64){
  usize root=index==0u64?3u64:(index==1u64?4u64:0u64);Span values=host.roots(host.context,root);usize component=0u64;
  while(component<values.count){host.forward(host.context,root,component,word_at(values,component));component+=1u64;}
  index+=1u64;
 }
 host.origin(host.context);
 index=0u64;
 while(index<6u64){
  Span values=host.roots(host.context,index);usize component=0u64;
  while(component<values.count){
   address at=unsafe_address(address_bits(values.data)+u64(component)*u64(values.stride));
   usize rebound=host.alias(host.context,unsafe_position<usize>(at));unsafe_position<usize>(at)=rebound;component+=1u64;
  }
  index+=1u64;
 }
 Span entry=host.part(host.context,0u64);index=0u64;
 while(index<entry.count){
  Results result=results(node_at(entry,index,N_INSTRUCTION),host.ir);Word next=next_result(result);
  while(next.present!=0){host.invariant(host.context,next.value);next=next_result(result);}
  index+=1u64;
 }
 usize invariant_end=entry.count;usize initialize_end=invariant_end+host.part(host.context,1u64).count;
 usize project_end=initialize_end+host.part(host.context,2u64).count;
 host.boundaries(host.context,invariant_end,initialize_end,project_end);
 partition=1u64;while(partition<4u64){host.append(host.context,partition);partition+=1u64;}
 host.finish(host.context);
}


address selection_slot(address slots,usize index) on cpu {
 return unsafe_address(address_bits(slots)+u64(index)*size_of<SelectionSlot>());
}
address selection_byte(address bytes,usize index) on cpu {
 return unsafe_address(address_bits(bytes)+u64(index));
}
address selection_value(address values,usize index) on cpu {
 return unsafe_address(address_bits(values)+u64(index)*size_of<SelectionValue>());
}
void selection_indices(Span code,address indices,Host ir) on cpu requires nonzero(ir.inspect) {
 usize index=0u64;
 while(index<code.count){
  View value=ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_INDEX_VALUE){unsafe_position<u8>(selection_byte(indices,value.a))=u8(1);}
  if(value.tag==OP_SCOPE||value.tag==OP_CLEANUP){selection_indices(value.first,indices,ir);}
  else if(value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   selection_indices(value.first,indices,ir);selection_indices(value.second,indices,ir);
  }
  index+=1u64;
 }
}
bool selection_float(Span registers,usize reg,Host ir) on cpu requires nonzero(ir.inspect) {
 u32 scalar=ir.inspect(node_at(registers,reg,24)).tag;return scalar==0||scalar==6;
}
bool selection_allowed(View value,SelectionInput input,Host ir) on cpu requires nonzero(ir.inspect) {
 u32 kind=value.tag;
 if(kind==OP_LOAD_LOCAL){return unsafe_position<SelectionSlot>(selection_slot(input.slots,value.b)).yes==0u64;}
 if(kind==OP_LOAD){return value.b==input.domain&&unsafe_position<u8>(selection_byte(input.indices,value.c))!=u8(0);}
 if(kind==OP_BINARY){return !selection_float(input.registers,value.b,ir);}
 if(kind==OP_CAST){return !selection_float(input.registers,value.a,ir)&&!selection_float(input.registers,value.b,ir);}
 if(kind==OP_INTEGER_DIVIDE){return ir.inspect(value.left).tag==0;}
 if(kind==OP_MATH){return value.b==0u64;}
 return kind==OP_CONSTANT||kind==OP_INDEX_VALUE||kind==OP_UNIFORM||kind==OP_BUFFER_STRIDE
  ||kind==OP_SELECT_VALUE||kind==OP_UNARY||kind==OP_STORE_LOCAL;
}
SelectionPlan selection_plan(Span yes,Span no,Span following,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 usize capacity=yes.count<no.count?yes.count:no.count;
 capacity=capacity<following.count?capacity:following.count;
 if(capacity==0u64){return SelectionPlan(unsafe_address(0u64),0u64);}
 address values=host.scratch(host.context,capacity);
 usize index=0u64;
 while(index<input.local_count){unsafe_position<SelectionSlot>(selection_slot(input.slots,index))=SelectionSlot(0u64,0u64);index+=1u64;}
 usize count=0u64;index=0u64;
 while(index<yes.count){
  View value=host.ir.inspect(node_at(yes,index,N_INSTRUCTION));
  if(!selection_allowed(value,input,host.ir)){return SelectionPlan(values,0u64);}
  if(value.tag==OP_LOAD_LOCAL){unsafe_position<SelectionSlot>(selection_slot(input.slots,value.b))=SelectionSlot(0u64,1u64);}
  else if(value.tag==OP_STORE_LOCAL){
   address at=selection_slot(input.slots,value.a);SelectionSlot slot=unsafe_position<SelectionSlot>(at);
   if(slot.no!=0u64){return SelectionPlan(values,0u64);}
   if(slot.yes==0u64){count+=1u64;}
   unsafe_position<SelectionSlot>(at)=SelectionSlot(value.b+1u64,0u64);
  }
  index+=1u64;
 }
 if(count==0u64||count>no.count||count>following.count){return SelectionPlan(values,0u64);}
 usize matched=0u64;index=0u64;
 while(index<no.count){
  View value=host.ir.inspect(node_at(no,index,N_INSTRUCTION));
  if(!selection_allowed(value,input,host.ir)){return SelectionPlan(values,0u64);}
  if(value.tag==OP_STORE_LOCAL){
   address at=selection_slot(input.slots,value.a);SelectionSlot slot=unsafe_position<SelectionSlot>(at);
   if(slot.yes==0u64){return SelectionPlan(values,0u64);}
   if(slot.no==0u64){matched+=1u64;}
   unsafe_position<SelectionSlot>(at)=SelectionSlot(slot.yes,value.b+1u64);
  }
  index+=1u64;
 }
 if(count!=matched){return SelectionPlan(values,0u64);}
 index=0u64;
 while(index<count){
  View value=host.ir.inspect(node_at(following,index,N_INSTRUCTION));
  if(value.tag!=OP_LOAD_LOCAL){return SelectionPlan(values,0u64);}
  address at=selection_slot(input.slots,value.b);SelectionSlot slot=unsafe_position<SelectionSlot>(at);
  if(slot.yes==0u64){return SelectionPlan(values,0u64);}
  unsafe_position<SelectionValue>(selection_value(values,index))=SelectionValue(value.b,value.a,slot.yes-1u64,slot.no-1u64);
  unsafe_position<SelectionSlot>(at)=SelectionSlot(0u64,0u64);index+=1u64;
 }
 return SelectionPlan(values,count);
}
struct SelectionInspection { SelectionPlan plan; u32 changed; };
SelectionInspection selection_inspect(address instruction,Span following,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 View value=host.ir.inspect(Node(instruction,N_INSTRUCTION));u32 changed=0;
 if(value.tag==OP_SCOPE||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
  SelectionChildren children=host.children(instruction);
  changed=selection_block(children.first,input,host);
  if(value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){changed=selection_block(children.second,input,host)|changed;}
  if(value.tag==OP_BRANCH){
   SelectionPlan plan=selection_plan(host.block(children.first),host.block(children.second),following,input,host);
   return SelectionInspection(plan,changed);
  }
 }
 return SelectionInspection(SelectionPlan(unsafe_address(0u64),0u64),changed);
}
u32 selection_block(address owner,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 Span code=host.block(owner);usize index=0u64;u32 changed=0;
 while(index<code.count){
  address instruction=node_at(code,index,N_INSTRUCTION).data;
  Span following=Span(unsafe_address(address_bits(code.data)+u64(index+1u64)*u64(code.stride)),code.count-index-1u64,code.stride);
  SelectionInspection result=selection_inspect(instruction,following,input,host);changed=changed|result.changed;
  if(result.plan.count!=0u64){host.rebuild(host.context,owner,index,result.plan);return 1;}
  index+=1u64;
 }
 return changed;
}
void selection_emit(address cursor,address instruction,SelectionPlan plan,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 usize condition=host.ir.inspect(Node(instruction,N_INSTRUCTION)).a;
 SelectionChildren children=host.children(instruction);
 host.arm(cursor,children.first);host.arm(cursor,children.second);
 usize index=0u64;
 while(index<plan.count){
  SelectionValue value=unsafe_position<SelectionValue>(selection_value(plan.values,index));
  host.emit(cursor,condition,value);host.next(cursor);index+=1u64;
 }
}
extern "C" void selection_continue(address cursor,SelectionNext first,SelectionPlan plan,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 selection_emit(cursor,first.instruction,plan,host);
 SelectionNext current=host.next(cursor);
 while(address_bits(current.instruction)!=0u64){
  SelectionInspection result=selection_inspect(current.instruction,current.following,input,host);
  if(result.plan.count!=0u64){selection_emit(cursor,current.instruction,result.plan,host);}
  else{host.keep(cursor);}
  current=host.next(cursor);
 }
}
extern "C" void selection_move_arm(address cursor,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 SelectionNext current=host.next(cursor);
 while(address_bits(current.instruction)!=0u64){
  if(host.ir.inspect(Node(current.instruction,N_INSTRUCTION)).tag!=OP_STORE_LOCAL){host.keep(cursor);}
  current=host.next(cursor);
 }
}
extern "C" u32 selection_run(address code,SelectionInput input,SelectionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.children) requires nonzero(host.scratch) requires nonzero(host.rebuild) requires nonzero(host.next) requires nonzero(host.keep) requires nonzero(host.arm) requires nonzero(host.emit) {
 selection_indices(host.block(code),input.indices,host.ir);return selection_block(code,input,host);
}


bool pipeline_contains(Span span,inout usize at,usize binding) on cpu {
 while(at<span.count){
  usize current=word_at(span,at);
  if(current>=binding){return current==binding;}
  at+=1u64;
 }
 return false;
}
bool pipeline_cross(PipelineStage left,PipelineStage right) on cpu {
 usize lc=0u64;usize rc=0u64;usize lr=0u64;usize lw=0u64;usize rr=0u64;usize rw=0u64;
 while(lc<left.cross.count||rc<right.cross.count){
  usize binding;
  if(lc<left.cross.count){
   binding=word_at(left.cross,lc);
   if(rc<right.cross.count){usize other=word_at(right.cross,rc);if(other<binding){binding=other;}}
  }else{binding=word_at(right.cross,rc);}
  bool left_writes=pipeline_contains(left.writes,lw,binding);
  bool right_writes=pipeline_contains(right.writes,rw,binding);
  if((left_writes&&(right_writes||pipeline_contains(right.reads,rr,binding)))
     ||(right_writes&&pipeline_contains(left.reads,lr,binding))){return true;}
  if(lc<left.cross.count&&word_at(left.cross,lc)==binding){lc+=1u64;}
  if(rc<right.cross.count&&word_at(right.cross,rc)==binding){rc+=1u64;}
 }
 return false;
}
bool pipeline_ordered(Span code,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));u32 kind=value.tag;
  if(kind==OP_APPEND||kind==OP_SUBGROUP||kind==OP_PRINT||kind==OP_INVOKE
     ||kind==OP_TILE_LENGTH||kind==OP_TILE_LANE||kind==OP_TILE_PUBLISH||kind==OP_TILE_BARRIER
     ||kind==OP_TILE_COLLECTIVE||kind==OP_TILE_LOAD){return true;}
  if(kind==OP_LOAD_INDEXED||kind==OP_ATOMIC){
   View source=host.inspect(value.left);if(source.tag==3){return true;}
  }
  if(kind==OP_SCOPE||kind==OP_BRANCH||kind==OP_LOOP_VALUE){
   if(pipeline_ordered(value.first,host)||pipeline_ordered(value.second,host)){return true;}
  }
  index+=1u64;
 }
 return false;
}
bool pipeline_same_field(PipelineField left,PipelineField right) on cpu {
 return left.present==right.present&&left.binding==right.binding&&left.word==right.word;
}
bool pipeline_fusible(PipelineStage left,PipelineStage right,Host host) on cpu requires nonzero(host.inspect) {
 return left.operation==0&&right.operation==0&&left.domain==right.domain&&right.boundary==0
  &&pipeline_same_field(left.prefix,right.prefix)&&pipeline_same_field(left.selection,right.selection)
  &&!pipeline_cross(left,right)&&!pipeline_ordered(left.code,host)&&!pipeline_ordered(right.code,host);
}
extern "C" usize fusion_register(address offsets,usize value) on cpu {
 return value+unsafe_position<FusionOffsets>(offsets).registers;
}
extern "C" usize fusion_local(address offsets,usize value) on cpu {
 return value+unsafe_position<FusionOffsets>(offsets).locals;
}
void pipeline_fuse(PipelineFusion host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.next)
 requires nonzero(host.previous) requires nonzero(host.stage) requires nonzero(host.append)
 requires nonzero(host.rewrite.inspect) requires nonzero(host.layout)
 requires nonzero(host.origin) requires nonzero(host.retain) requires nonzero(host.commit) {
 while(true){
  address current=host.next(host.context);if(address_bits(current)==0u64){return;}
  address previous=host.previous(host.context);
  if(address_bits(previous)!=0u64&&pipeline_fusible(host.stage(previous),host.stage(current),host.ir)){
   host.origin(previous,current);
   FusionLayout layout=host.layout(previous,current);
   FusionOffsets offsets=layout.offsets;
   rewrite_code(layout.code,host.rewrite,RewriteMap(address_of(offsets),c_function(fusion_register),c_function(fusion_local)));
   host.retain(previous,current);
   host.commit(host.context,offsets.locals);
  }else{host.append(host.context);}
 }
}
extern "C" void optimize_program(PipelineProgram host) on cpu
 requires nonzero(host.fusion.ir.inspect) requires nonzero(host.fusion.begin)
 requires nonzero(host.fusion.next) requires nonzero(host.fusion.previous)
 requires nonzero(host.fusion.stage) requires nonzero(host.fusion.append)
 requires nonzero(host.fusion.rewrite.inspect) requires nonzero(host.fusion.layout)
 requires nonzero(host.fusion.origin) requires nonzero(host.fusion.retain) requires nonzero(host.fusion.commit)
 requires nonzero(host.batch) requires nonzero(host.remove) requires nonzero(host.origin) requires nonzero(host.finish) {
 pipeline_fuse(host.fusion);
 if(host.batch(host.fusion.context)==0){return;}
 if(host.remove(host.fusion.context)!=0u64){
  host.origin(host.fusion.context);
  host.fusion.begin(host.fusion.context);
  pipeline_fuse(host.fusion);
  if(host.batch(host.fusion.context)==0){return;}
 }
 host.finish(host.fusion.context);
}
extern "C" PipelineEmpty optimize_empty(PipelineStage stage,u32 carry) on cpu {
 if(stage.operation==0&&stage.code.count==0u64){return PipelineEmpty(0,stage.boundary,carry|stage.boundary);}
 return PipelineEmpty(1,stage.boundary|carry,0);
}
extern "C" void optimize_value(address value,PipelineValue host) on cpu
 requires nonzero(host.flow) requires nonzero(host.eliminate) requires nonzero(host.compact) {
 host.flow(value);host.eliminate(value);host.compact(value);
}
extern "C" void optimize_stage(PipelineStageHost host,u32 has_tiles) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.stage) requires nonzero(host.custom)
 requires nonzero(host.values.flow) requires nonzero(host.values.eliminate) requires nonzero(host.values.compact)
 requires nonzero(host.flow) requires nonzero(host.eliminate) requires nonzero(host.words)
 requires nonzero(host.prune) requires nonzero(host.select) requires nonzero(host.bindings)
 requires nonzero(host.release_words) requires nonzero(host.compact) {
 u32 operation=host.stage(host.context).operation;
 if(operation==2||operation==3||operation==4){
  PipelineValues values=host.custom(host.context);
  if(address_bits(values.project)!=0u64){optimize_value(values.project,host.values);}
  optimize_value(values.identity,host.values);optimize_value(values.combine,host.values);return;
 }
 host.flow(host.context);
 while(true){
  host.eliminate(host.context);
  u32 changed=0;
  if(has_tiles!=0){
   u32 preserve=has_subgroups(host.stage(host.context).code,host.ir);
   host.words(host.context);
   changed=host.prune(host.context,preserve);
  }
  if(host.select(host.context)!=0){changed=1;host.flow(host.context);}
  if(changed==0){break;}
  if(has_tiles!=0){host.release_words(host.context);}
 }
 if(operation==0){host.bindings(host.context);}
 if(has_tiles!=0){host.release_words(host.context);}
 host.compact(host.context);
}
extern "C" PipelineRootEdit optimize_root(address instruction,u32 preserve_scope,PipelineRead host,RewriteHost rewrite) on cpu
 requires nonzero(host.contains) requires nonzero(host.scalar_bytes) requires nonzero(rewrite.inspect) {
 RewriteView root=rewrite.inspect(RewriteNode(instruction,N_INSTRUCTION));
 if(root.tag!=OP_TILE_PUBLISH&&root.tag!=OP_TILE_COLLECTIVE){return PipelineRootEdit(PipelineRootAction::Keep,0u64);}
 usize count=root.first.count;
 if(root.tag==OP_TILE_PUBLISH){
  usize tile=unsafe_position<usize>(root.a);
  usize read=0u64;count=0u64;
  while(read<root.first.count){
   u32 word=unsafe_position<u32>(rewrite_word_at(root.second,read));
   usize reg=unsafe_position<usize>(rewrite_word_at(root.first,read));
   u32 bytes=host.scalar_bytes(host.context,reg);u32 part=0;bool keep=false;
   while(part<(bytes+3)/4){
    if(host.contains(host.context,0,tile,word+part*4)!=0){keep=true;break;}
    part+=1;
   }
   if(keep){
    if(count!=read){
     unsafe_position<u32>(rewrite_word_at(root.second,count))=word;
     unsafe_position<usize>(rewrite_word_at(root.first,count))=reg;
    }
    count+=1u64;
   }
   read+=1u64;
  }
 }
 if(count==0u64&&preserve_scope==0){return PipelineRootEdit(PipelineRootAction::Drop,0u64);}
 if(count!=root.first.count){return PipelineRootEdit(PipelineRootAction::Trim,count);}
 return PipelineRootEdit(PipelineRootAction::Keep,count);
}
address pipeline_flag(address flags,usize binding) on cpu {
 return unsafe_address(address_bits(flags)+u64(binding));
}
void pipeline_mark(address flags,usize binding,u8 mode) on cpu {
 address at=pipeline_flag(flags,binding);unsafe_position<u8>(at)=unsafe_position<u8>(at)|mode;
}
u8 pipeline_access(usize access) on cpu {
 return (accessed(access,0) ? u8(1) : u8(0))|(accessed(access,1) ? u8(2) : u8(0));
}
void pipeline_alias(Node source,address flags,u8 mode,Host host) on cpu requires nonzero(host.inspect) {
 View alias=host.inspect(source);if(alias.tag==1){pipeline_mark(flags,alias.a,mode);}
}
void pipeline_indexed(Node source,address flags,u8 mode,Host host) on cpu requires nonzero(host.inspect) {
 View value=host.inspect(source);
 if(value.tag==0){pipeline_mark(flags,value.a,mode);}
 else if(value.tag==2){
  usize index=0u64;
  while(index<value.second.count){pipeline_alias(node_at(value.second,index,N_MEMORY_ALIAS),flags,mode,host);index+=1u64;}
 }
}
void pipeline_reference(Node source,address flags,u8 mode,Host host) on cpu requires nonzero(host.inspect) {
 View reference=host.inspect(source);View storage=host.inspect(reference.left);
 if(storage.tag==1){pipeline_indexed(storage.left,flags,mode,host);}
}
void pipeline_code_access(Span code,address flags,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));u32 kind=value.tag;
  if(kind==OP_INVOKE){
   View call=host.inspect(value.left);usize argument=0u64;
   while(argument<call.first.count){
    View parameter=host.inspect(node_at(call.first,argument,N_ARGUMENT));
    if(parameter.tag==1){pipeline_reference(parameter.left,flags,pipeline_access(parameter.a),host);}
    argument+=1u64;
   }
   usize effect=0u64;
   while(effect<call.second.count){
    View memory=host.inspect(node_at(call.second,effect,N_MEMORY_EFFECT));
    pipeline_alias(memory.left,flags,pipeline_access(memory.a),host);effect+=1u64;
   }
  }else if(kind==OP_APPEND){pipeline_mark(flags,value.b,u8(3));pipeline_mark(flags,value.c,u8(2));}
  else if(kind==OP_ATOMIC){
   View op=host.inspect(value.right);
   u8 mode=(op.tag!=1 ? u8(1) : u8(0))|(op.tag!=0 ? u8(2) : u8(0));
   pipeline_indexed(value.left,flags,mode,host);
  }else if(kind==OP_LOAD_INDEXED||kind==OP_STORE_INDEXED){pipeline_indexed(value.left,flags,kind==OP_LOAD_INDEXED ? u8(1) : u8(2),host);}
  else if(kind==OP_LOAD){pipeline_mark(flags,value.b,u8(1));}
  else if(kind==OP_STORE){pipeline_mark(flags,value.a,u8(2));}
  if(kind==OP_SCOPE||kind==OP_BRANCH||kind==OP_LOOP_VALUE||kind==OP_CLEANUP){
   pipeline_code_access(value.first,flags,host);pipeline_code_access(value.second,flags,host);
  }
  index+=1u64;
 }
}
void pipeline_tile_access(Span tiles,address flags,Host ir,PipelineRead host) on cpu
 requires nonzero(ir.inspect) requires nonzero(host.contains) {
 usize index=0u64;
 while(index<tiles.count){
  View tile=ir.inspect(node_at(tiles,index,N_TILE));
  if(tile.tag==0){
   usize component=0u64;bool needed=false;
   while(component<tile.first.count&&!needed){
    View field=ir.inspect(node_at(tile.first,component,N_COMPONENT));usize part=0u64;
    while(part<(field.b+3u64)/4u64){
     if(host.contains(host.context,1,tile.a,u32(tile.b+field.a+part*4u64))!=0){needed=true;break;}
     part+=1u64;
    }
    component+=1u64;
   }
   if(needed){pipeline_mark(flags,tile.a,u8(1));}
  }
  index+=1u64;
 }
}
void pipeline_write_binding(address output,usize index,usize binding) on cpu {
 unsafe_position<usize>(unsafe_address(address_bits(output)+u64(index)*size_of<usize>()))=binding;
}
extern "C" PipelineAccessCounts optimize_bindings(PipelineMap input,Host ir,PipelineRead host) on cpu
 requires nonzero(ir.inspect) requires nonzero(host.contains) {
 pipeline_code_access(input.code,input.flags,ir);pipeline_tile_access(input.tiles,input.flags,ir,host);
 if(input.prefix.present!=0){pipeline_mark(input.flags,input.prefix.binding,u8(1));}
 if(input.selection.present!=0){pipeline_mark(input.flags,input.selection.binding,u8(1));}
 usize reads=0u64;usize writes=0u64;usize binding=0u64;
 while(binding<input.binding_count){
  u8 flag=unsafe_position<u8>(pipeline_flag(input.flags,binding));
  if((flag&u8(1))!=u8(0)){pipeline_write_binding(input.reads,reads,binding);reads+=1u64;}
  if((flag&u8(2))!=u8(0)){pipeline_write_binding(input.writes,writes,binding);writes+=1u64;}
  binding+=1u64;
 }
 usize cross=0u64;usize at=0u64;
 while(at<input.cross_count){
  binding=unsafe_position<usize>(unsafe_address(address_bits(input.cross)+u64(at)*size_of<usize>()));
  if(unsafe_position<u8>(pipeline_flag(input.flags,binding))!=u8(0)){pipeline_write_binding(input.cross,cross,binding);cross+=1u64;}
  at+=1u64;
 }
 return PipelineAccessCounts(reads,writes,cross);
}


// Field rewriting uses exclusive physical projections, not the read-only query
// views above. Mapping policies remain with their actual producers. No view is
// retained after this call and no container changes shape during the rewrite.
const u32 N_RAY_RESULT=16;
RewriteNode rewrite_node_at(RewriteSpan span,usize index,u32 kind) on cpu {
 return RewriteNode(unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride)),kind);
}
address rewrite_word_at(RewriteSpan span,usize index) on cpu {
 return unsafe_address(address_bits(span.data)+u64(index)*u64(span.stride));
}
void rewrite_word(address field,address context,cfn<usize,address,usize> map) on cpu requires nonzero(map) {
 unsafe_position<usize>(field)=map(context,unsafe_position<usize>(field));
}
void rewrite_optional(address field,address context,cfn<usize,address,usize> map) on cpu requires nonzero(map) {
 if(address_bits(field)!=0u64){rewrite_word(field,context,map);}
}
void rewrite_words(RewriteSpan words,address context,cfn<usize,address,usize> map) on cpu requires nonzero(map) {
 usize index=0u64;
 while(index<words.count){rewrite_word(rewrite_word_at(words,index),context,map);index+=1u64;}
}
void rewrite_stride(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {
 RewriteView value=host.inspect(source);
 if(value.tag==1){rewrite_words(value.first,mapping.context,mapping.reg);}
}
void rewrite_indexed_operands(RewriteView value,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {
 if(value.tag==1||value.tag==2){
  rewrite_words(value.first,mapping.context,mapping.reg);
  if(value.tag==1){rewrite_stride(value.left,host,mapping);}
 }
 rewrite_word(value.a,mapping.context,mapping.reg);
}
void rewrite_alias(RewriteNode source,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.slot) {
 RewriteView value=host.inspect(source);
 if(value.tag==0&&value.length!=0u64){rewrite_word(value.a,mapping.context,mapping.slot);}
}
void rewrite_indexed_locals(RewriteView value,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.slot) {
 if(value.tag==2){
  usize index=0u64;
  while(index<value.second.count){rewrite_alias(rewrite_node_at(value.second,index,N_MEMORY_ALIAS),host,mapping);index+=1u64;}
 }
}
void rewrite_reference_operands(RewriteView value,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {
 rewrite_optional(value.a,mapping.context,mapping.reg);
 RewriteView storage=host.inspect(value.left);
 if(storage.tag==1){rewrite_indexed_operands(host.inspect(storage.left),host,mapping);}
}
void rewrite_reference_locals(RewriteView value,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.slot) {
 RewriteView storage=host.inspect(value.left);
 if(storage.tag==0){
  if(storage.length!=0u64){rewrite_word(storage.a,mapping.context,mapping.slot);}
 }else{rewrite_indexed_locals(host.inspect(storage.left),host,mapping);}
}
void rewrite_call_inputs(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {
 RewriteView value=host.inspect(source);RewriteView target=host.inspect(value.left);
 if(target.tag==2){rewrite_word(target.a,mapping.context,mapping.reg);}
 usize index=0u64;
 while(index<value.first.count){
  RewriteView argument=host.inspect(rewrite_node_at(value.first,index,N_ARGUMENT));
  if(argument.tag==0){
   if(address_bits(argument.left.data)!=0u64){
    RewriteView borrow=host.inspect(argument.left);
    rewrite_words(borrow.first,mapping.context,mapping.reg);rewrite_word(borrow.a,mapping.context,mapping.reg);
   }
   rewrite_words(argument.first,mapping.context,mapping.reg);
  }else{rewrite_reference_operands(host.inspect(argument.left),host,mapping);}
  index+=1u64;
 }
}
void rewrite_call_outputs(RewriteNode source,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot) {
 RewriteView value=host.inspect(source);
 rewrite_words(value.third,mapping.context,mapping.reg);rewrite_optional(value.a,mapping.context,mapping.reg);
 usize index=0u64;
 while(index<value.first.count){
  RewriteView argument=host.inspect(rewrite_node_at(value.first,index,N_ARGUMENT));
  if(argument.tag==1){rewrite_reference_locals(host.inspect(argument.left),host,mapping);}
  index+=1u64;
 }
 index=0u64;
 while(index<value.second.count){
  RewriteView effect=host.inspect(rewrite_node_at(value.second,index,N_MEMORY_EFFECT));
  rewrite_alias(effect.left,host,mapping);index+=1u64;
 }
}
void rewrite_result(RewriteNode source,RewriteHost host,address context,cfn<usize,address,usize> map) on cpu
 requires nonzero(host.inspect) requires nonzero(map) {
 RewriteView value=host.inspect(source);
 if(source.kind==N_RAY_RESULT){rewrite_words(value.first,context,map);}
 else{rewrite_word(value.a,context,map);rewrite_optional(value.b,context,map);}
}
void rewrite_inputs(RewriteView value,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {
 u32 kind=value.tag;
 if(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH
    ||kind==OP_TEXTURE_SIZE||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
    ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ){
  RewriteView texture=host.inspect(value.left);
  rewrite_optional(texture.a,mapping.context,mapping.reg);rewrite_optional(texture.b,mapping.context,mapping.reg);
 }
 if(kind==OP_INVOKE){rewrite_call_inputs(value.left,host,mapping);}
 else if(kind==OP_POSITION_ADDRESS){rewrite_reference_operands(host.inspect(value.left),host,mapping);}
 else if(kind==OP_LOAD_INDEXED||kind==OP_STORE_INDEXED){
  rewrite_indexed_operands(host.inspect(value.left),host,mapping);
  rewrite_word(kind==OP_LOAD_INDEXED ? value.b : value.a,mapping.context,mapping.reg);
  if(kind==OP_STORE_INDEXED){rewrite_word(value.b,mapping.context,mapping.reg);}
 }else if(kind==OP_ATOMIC){
  rewrite_indexed_operands(host.inspect(value.left),host,mapping);rewrite_optional(value.b,mapping.context,mapping.reg);
  RewriteView op=host.inspect(value.right);
  if(op.tag!=0){rewrite_word(op.a,mapping.context,mapping.reg);}
  if(op.tag==2){rewrite_word(op.b,mapping.context,mapping.reg);}
 }else if(kind==OP_RAY_QUERY){
  rewrite_words(value.first,mapping.context,mapping.reg);rewrite_words(value.second,mapping.context,mapping.reg);
  rewrite_word(value.a,mapping.context,mapping.reg);rewrite_word(value.b,mapping.context,mapping.reg);rewrite_optional(value.c,mapping.context,mapping.reg);
 }else if(kind==OP_INTEGER_DIVIDE){
  rewrite_word(value.a,mapping.context,mapping.reg);RewriteView divisor=host.inspect(value.left);
  if(divisor.tag==1){rewrite_word(divisor.a,mapping.context,mapping.reg);}
 }else if(kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH){
  rewrite_words(value.second,mapping.context,mapping.reg);rewrite_word(value.a,mapping.context,mapping.reg);
 }else if(kind==OP_TEXTURE_READ){
  rewrite_words(value.first,mapping.context,mapping.reg);rewrite_word(value.b,mapping.context,mapping.reg);
 }else if(kind==OP_TEXTURE_SIZE){rewrite_word(value.a,mapping.context,mapping.reg);}
 else if(kind==OP_APPEND||kind==OP_TILE_PUBLISH){rewrite_words(value.first,mapping.context,mapping.reg);}
 else if(kind==OP_RAISE){rewrite_word(value.a,mapping.context,mapping.reg);}
 else if(kind==OP_FUNCTION_ADDRESS){RewriteView target=host.inspect(value.left);if(target.tag==2){rewrite_word(target.a,mapping.context,mapping.reg);}}
 else if(kind==OP_STORE_LOCAL||kind==OP_PRINT){rewrite_word(value.b,mapping.context,mapping.reg);}
 else if(kind==OP_LOAD_LOCAL_INDEXED){rewrite_word(value.d,mapping.context,mapping.reg);}
 else if(kind==OP_STORE_LOCAL_INDEXED||kind==OP_STORE){
  rewrite_word(kind==OP_STORE_LOCAL_INDEXED ? value.c : value.b,mapping.context,mapping.reg);rewrite_word(value.d,mapping.context,mapping.reg);
 }else if(kind==OP_SUBGROUP){rewrite_words(value.first,mapping.context,mapping.reg);rewrite_words(value.second,mapping.context,mapping.reg);}
 else if(kind==OP_VIEW_LOAD){
  rewrite_words(value.first,mapping.context,mapping.reg);rewrite_stride(value.left,host,mapping);rewrite_word(value.b,mapping.context,mapping.reg);
 }else if(kind==OP_LOAD||kind==OP_TILE_LOAD){rewrite_word(value.c,mapping.context,mapping.reg);}
 else if(kind==OP_UNARY||kind==OP_CAST){rewrite_word(value.b,mapping.context,mapping.reg);}
 else if(kind==OP_BINARY){rewrite_word(value.b,mapping.context,mapping.reg);rewrite_word(value.c,mapping.context,mapping.reg);}
 else if(kind==OP_MATH){rewrite_words(value.first,mapping.context,mapping.reg);}
 else if(kind==OP_SELECT_VALUE){rewrite_word(value.b,mapping.context,mapping.reg);rewrite_word(value.c,mapping.context,mapping.reg);rewrite_word(value.d,mapping.context,mapping.reg);}
 else if(kind==OP_BRANCH||kind==OP_LOOP_VALUE){rewrite_word(value.a,mapping.context,mapping.reg);}
}
void rewrite_outputs(RewriteView value,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot) {
 u32 kind=value.tag;
 if(kind==OP_INVOKE){rewrite_call_outputs(value.left,host,mapping);}
 else if(kind==OP_POSITION_ADDRESS){rewrite_word(value.a,mapping.context,mapping.reg);rewrite_reference_locals(host.inspect(value.left),host,mapping);}
 else if(kind==OP_LOAD_INDEXED||kind==OP_STORE_INDEXED){
  if(kind==OP_LOAD_INDEXED){rewrite_word(value.a,mapping.context,mapping.reg);}
  rewrite_indexed_locals(host.inspect(value.left),host,mapping);
 }else if(kind==OP_ATOMIC){rewrite_optional(value.a,mapping.context,mapping.reg);rewrite_indexed_locals(host.inspect(value.left),host,mapping);}
 else if(kind==OP_RAY_QUERY){rewrite_result(value.left,host,mapping.context,mapping.reg);}
 else if(kind==OP_INTEGER_DIVIDE){rewrite_result(value.right,host,mapping.context,mapping.reg);}
 else if(kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH||kind==OP_TEXTURE_SIZE||kind==OP_TILE_COLLECTIVE){rewrite_words(value.first,mapping.context,mapping.reg);}
 else if(kind==OP_LOAD_LOCAL||kind==OP_LOAD_LOCAL_INDEXED){rewrite_word(value.a,mapping.context,mapping.reg);rewrite_word(value.b,mapping.context,mapping.slot);}
 else if(kind==OP_STORE_LOCAL||kind==OP_STORE_LOCAL_INDEXED){rewrite_word(value.a,mapping.context,mapping.slot);}
 else if(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
    ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ||kind==OP_APPEND||kind==OP_CONSTANT
    ||kind==OP_FUNCTION_ADDRESS||kind==OP_INDEX_VALUE||kind==OP_UNIFORM||kind==OP_BUFFER_STRIDE
    ||kind==OP_TILE_LENGTH||kind==OP_TILE_LANE||kind==OP_LOAD||kind==OP_SUBGROUP||kind==OP_PRINT
    ||kind==OP_TILE_LOAD||kind==OP_VIEW_LOAD||kind==OP_UNARY||kind==OP_BINARY||kind==OP_CAST
    ||kind==OP_MATH||kind==OP_SELECT_VALUE){rewrite_word(value.a,mapping.context,mapping.reg);}
}
void rewrite_block(RewriteSpan code,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot) {
 usize entry=0u64;RewriteInputMap inputs=RewriteInputMap(mapping.context,mapping.reg);
 while(entry<code.count){
  RewriteView value=host.inspect(rewrite_node_at(code,entry,N_INSTRUCTION));
  rewrite_inputs(value,host,inputs);rewrite_outputs(value,host,mapping);
  if(value.tag==OP_SCOPE){rewrite_block(value.first,host,mapping);}
  else if(value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){rewrite_block(value.first,host,mapping);rewrite_block(value.second,host,mapping);}
  entry+=1u64;
 }
}
extern "C" void rewrite_code(RewriteSpan code,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot) {rewrite_block(code,host,mapping);}
extern "C" void rewrite_operands(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {rewrite_inputs(host.inspect(source),host,mapping);}

u32 retain_origin_block(RewriteSpan code,address context,RetainOriginHost host) on cpu
 requires nonzero(host.read.inspect) requires nonzero(host.write.inspect) requires nonzero(host.collect)
 requires nonzero(host.attach) requires nonzero(host.pending) {
 usize index=0u64;
 while(index<code.count){
  RewriteNode node=rewrite_node_at(code,index,N_INSTRUCTION);
  View value=host.read.inspect(Node(node.data,N_INSTRUCTION));
  u32 kinds=visit_result_origins(address_of(value),host.read,context,host.collect);
  if(kinds!=0){
   if((kinds&1)!=0){host.attach(context,node.data,ResultOriginKind::Expression);}
   if((kinds&2)!=0){host.attach(context,node.data,ResultOriginKind::LocalRead);}
   if((kinds&4)!=0){host.attach(context,node.data,ResultOriginKind::IndexedStoreRead);}
   if((kinds&8)!=0){host.attach(context,node.data,ResultOriginKind::IndexedRead);}
   if(host.pending(context)==0){return 0;}
  }
  if(value.tag==OP_SCOPE||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   RewriteView storage=host.write.inspect(node);
   if(retain_origin_block(storage.first,context,host)==0
     ||retain_origin_block(storage.second,context,host)==0){return 0;}
  }
  index+=1u64;
 }
 return 1;
}
extern "C" void retain_result_origins(RewriteSpan code,address context,RetainOriginHost host) on cpu
 requires nonzero(host.read.inspect) requires nonzero(host.write.inspect) requires nonzero(host.collect)
 requires nonzero(host.attach) requires nonzero(host.pending) {
 retain_origin_block(code,context,host);
}

extern "C" void seed_effect_flags(EffectSeedHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.remember) {
 while(true) {
  EffectSeed item=host.next(host.iterator);
  if(item.present==0){return;}
  if(item.enabled!=0){host.remember(host.known,item.name);}
 }
}

bool effect_body(Span code,EffectRead host,u32 explicit_errors) on cpu
 requires nonzero(host.inspect) requires nonzero(host.contains) {
 usize index=0u64;
 while(index<code.count) {
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  EffectNode item=host.inspect(instruction);
  if(explicit_errors!=0 && item.error!=0){return true;}
  if(address_bits(item.target)!=0u64 && host.contains(host.known,item.target)!=0){return true;}
  if(effect_body(item.first,host,explicit_errors)||effect_body(item.second,host,explicit_errors)){return true;}
  index+=1u64;
 }
 return false;
}

extern "C" u32 mark_effects(EffectHost host,u32 explicit_errors) on cpu
 requires nonzero(host.next) requires nonzero(host.inspect) requires nonzero(host.contains) {
 u32 changed=0;
 while(true) {
  EffectFunction item=host.next(host.iterator);
  if(item.present==0){return changed;}
  if(unsafe_position<u8>(item.flag)==u8(0) && effect_body(item.body,EffectRead(host.known,host.inspect,host.contains),explicit_errors)) {
   unsafe_position<u8>(item.flag)=u8(1);
   changed=1;
  }
 }
}

extern "C" void propagate_effects(address context,cfn<u32,address> round) on cpu
 requires nonzero(round) {
 while(round(context)!=0){}
}

bool failure_name(DependencyText left,DependencyText right) on cpu {
 if(left.length!=right.length){return false;}
 usize index=0u64;
 while(index<left.length){
  u8 a=unsafe_position<u8>(unsafe_address(address_bits(left.data)+u64(index)));
  u8 b=unsafe_position<u8>(unsafe_address(address_bits(right.data)+u64(index)));
  if(a!=b){return false;}
  index+=1u64;
 }
 return true;
}
bool failure_token<const N:u32 in {20}>(DependencyText text,read array<u8,N> expected) on cpu {
 if(text.length!=usize(N)){return false;}
 u32 index=0;
 while(index<N){
  if(unsafe_position<u8>(unsafe_address(address_bits(text.data)+u64(index)))!=expected[index]){return false;}
  index+=1;
 }
 return true;
}
void collect_status_reads(Span code,usize slot,FoldHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.insert) {
 usize index=0u64;
 while(index<code.count){
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  FoldNode node=host.inspect(instruction);
  if(node.kind==1 && node.slot==slot){host.insert(host.reads,node.reg);}
  collect_status_reads(node.structure.first,slot,host);
  collect_status_reads(node.structure.second,slot,host);
  index+=1u64;
 }
}
void replace_status_reads(Span code,FoldHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.contains) requires nonzero(host.replace) {
 usize index=0u64;
 while(index<code.count){
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  FoldNode node=host.inspect(instruction);
  if(node.kind==1 && host.contains(host.reads,node.reg)!=0){host.replace(instruction,node.reg,0);}
  else if(node.kind==2 && failure_name(node.library,host.runtime)
       && failure_token(node.symbol,"blink_error_consumed") && node.value_argument!=0
       && host.contains(host.reads,word_at(node.arguments,0u64))!=0){
   host.replace(instruction,word_at(node.outputs,0u64),1);
  }else{
   replace_status_reads(node.structure.first,host);
   replace_status_reads(node.structure.second,host);
  }
  index+=1u64;
 }
}
extern "C" void fold_empty_status(Span code,usize slot,FoldHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.insert) requires nonzero(host.contains) requires nonzero(host.replace) {
 collect_status_reads(code,slot,host);
 replace_status_reads(code,host);
}

LowerNeed lower_done(u32 flags) on cpu {
 return LowerNeed(0,flags,unsafe_address(0u64),Word(0,0u64),LowerBranch(0,0u64,0u64,0,0u64,0u64));
}
LowerNeed lower_give(address state,LowerCursor cursor,LowerNeed need) on cpu {
 unsafe_position<LowerCursor>(state)=cursor;return need;
}
LowerNeed lower_continue(address state,LowerCursor original,address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 LowerCursor cursor=original;
 while(true){
  if(cursor.phase==3){cursor.flags|=1;cursor.node=host.next(block);cursor.phase=0;}
  LowerNode node=cursor.node;
  if(node.present==0){return lower_done(cursor.flags);}
  if(cursor.phase==0){
   if(node.kind==1){
    if(!effect_body(node.body,host.effects,1)){
     host.fold(node.first,node.slot);host.cleanup_scope(block,output,node.first);
     cursor.flags|=1;cursor.node=host.next(block);continue;
    }
    host.capture(block,node.source,node.payload,1);
    usize zero=host.reg(block,0);usize error=host.reg(block,0);usize failed=host.reg(block,1);
    host.constant(block,output,zero,0u64);host.store(block,output,node.slot,zero);
    host.cleanup_body(block,output,node.first);host.load(block,output,error,node.slot);
    host.compare(block,output,failed,error,zero);
    u32 kind=node.continuation==2 ? 2 : 1;if(kind==2){cursor.flags|=2;}
    cursor.phase=3;
    return lower_give(state,cursor,LowerNeed(2,0,unsafe_address(0u64),Word(1,failed),LowerBranch(kind,error,node.slot,1,0u64,0u64)));
   }
   if(node.kind==3||node.kind==4||node.kind==5){
    cursor.phase=1;
    return lower_give(state,cursor,LowerNeed(1,0,node.first,Word(0,0u64),LowerBranch(0,0u64,0u64,0,0u64,0u64)));
   }
  }
  if(cursor.phase==1&&node.kind!=3){
   cursor.phase=2;
   return lower_give(state,cursor,LowerNeed(1,0,node.second,Word(0,0u64),LowerBranch(0,0u64,0u64,0,0u64,0u64)));
  }
  bool lower=node.kind==2&&node.continuation!=0&&(node.error.present!=0
      ||(address_bits(node.target)!=0u64&&host.effects.contains(host.effects.known,node.target)!=0));
  if(!lower){host.original(block,output);cursor.node=host.next(block);cursor.phase=0;continue;}
  usize error=node.error.value;
  if(node.error.present==0){error=host.reg(block,0);host.set_error(node.error_slot,error);}
  host.capture(block,node.source,node.payload,0);host.original(block,output);
  if(node.continuation==2){cursor.flags|=2;}
  cursor.phase=3;
  return lower_give(state,cursor,LowerNeed(2,0,unsafe_address(0u64),Word(0,error),LowerBranch(node.continuation,error,node.slot,0,0u64,0u64)));
 }
}
extern "C" LowerNeed lower_failure_start(address state,address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 return lower_continue(state,LowerCursor(host.next(block),0,0),block,output,host);
}
extern "C" LowerNeed lower_failure_child(address state,address block,address output,u32 nested,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 LowerCursor cursor=unsafe_position<LowerCursor>(state);cursor.flags|=nested;
 return lower_continue(state,cursor,block,output,host);
}
extern "C" LowerNeed lower_failure_branch(address state,address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 return lower_continue(state,unsafe_position<LowerCursor>(state),block,output,host);
}
extern "C" FailureBranchNeed failure_branch_prepare(address block,address yes,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.raise) requires nonzero(host.load) requires nonzero(host.constant) requires nonzero(host.compare) requires nonzero(host.join) requires nonzero(host.store) {
 if(branch.kind==1){host.raise(block,yes,branch.error);}
 else if(branch.kind==2){return FailureBranchNeed(1,Word(0,0u64),branch);}
 else if(branch.kind==3){
  usize previous=host.reg(block,0);usize joined=host.reg(block,0);
  usize zero=host.reg(block,0);usize present=host.reg(block,1);
  host.load(block,yes,previous,branch.slot);host.constant(block,yes,zero,0u64);
  host.compare(block,yes,present,previous,zero);
  return FailureBranchNeed(2,Word(1,present),LowerBranch(4,branch.error,branch.slot,0,previous,joined));
 }else if(branch.kind==4){host.join(block,yes,branch.joined,branch.previous,branch.error);host.store(block,yes,branch.slot,branch.joined);}
 return FailureBranchNeed(0,Word(0,0u64),branch);
}
extern "C" void failure_branch_finish(address block,address output,address yes,address no,Word test,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.constant) requires nonzero(host.compare) requires nonzero(host.store) requires nonzero(host.finish_if) {
 if(branch.kind==4){host.store(block,no,branch.slot,branch.error);}
 usize condition=test.value;
 if(test.present==0){usize zero=host.reg(block,0);condition=host.reg(block,1);host.constant(block,output,zero,0u64);host.compare(block,output,condition,test.value,zero);}
 host.finish_if(block,output,condition,yes,no);
}

extern "C" void collect_dependencies(Span code,DependencyHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.add) {
 usize index=0u64;
 while(index<code.count) {
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  DependencyNode item=host.inspect(instruction);
  if(address_bits(item.target)!=0u64){host.add(host.output,item.target);}
  collect_dependencies(item.first,host);
  collect_dependencies(item.second,host);
  index+=1u64;
 }
}

extern "C" void collect_program_dependencies(Span stages,
 cfn<DependencyStage,address> ensures memory(read) inspect,DependencyHost host) on cpu
 requires nonzero(inspect) requires nonzero(host.inspect) requires nonzero(host.add) {
 usize index=0u64;
 while(index<stages.count) {
  address stage=unsafe_address(address_bits(stages.data)+u64(index)*u64(stages.stride));
  DependencyStage item=inspect(stage);
  collect_dependencies(item.body,host);
  collect_dependencies(item.project,host);
  collect_dependencies(item.identity,host);
  collect_dependencies(item.combine,host);
  index+=1u64;
 }
}

bool dependency_local(DependencyUnit unit,DependencyOwner owner) on cpu {
 if(owner.present==0){return true;}
 if(unit.name.length!=owner.name.length){return false;}
 usize index=0u64;
 while(index<owner.name.length) {
  u8 left=unsafe_position<u8>(unsafe_address(address_bits(unit.name.data)+u64(index)));
  u8 right=unsafe_position<u8>(unsafe_address(address_bits(owner.name.data)+u64(index)));
  if(left!=right){return false;}
  index+=1u64;
 }
 return true;
}

extern "C" void seed_dependencies(DependencySeedHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.lookup) requires nonzero(host.push) {
 while(true) {
  address name=host.next(host.iterator);
  if(address_bits(name)==0u64){return;}
  DependencyUnit unit=host.lookup(host.units,name);
  if(unit.body.count!=0u64 && dependency_local(unit,host.owner)){host.push(host.pending,unit.body);}
 }
}

void expand_dependency_body(Span code,DependencyClosureHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.lookup) requires nonzero(host.insert)
 requires nonzero(host.push) requires nonzero(host.pop) {
 usize index=0u64;
 while(index<code.count) {
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  DependencyNode item=host.inspect(instruction);
  if(address_bits(item.target)!=0u64 && host.insert(host.required,item.target)!=0) {
   DependencyUnit unit=host.lookup(host.units,item.target);
   if(unit.body.count!=0u64 && dependency_local(unit,host.owner)){host.push(host.pending,unit.body);}
  }
  expand_dependency_body(item.first,host);
  expand_dependency_body(item.second,host);
  index+=1u64;
 }
}

extern "C" void expand_dependencies(DependencyClosureHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.lookup) requires nonzero(host.insert)
 requires nonzero(host.push) requires nonzero(host.pop) {
 while(true) {
  DependencyWork item=host.pop(host.pending);
  if(item.present==0){return;}
  expand_dependency_body(item.body,host);
 }
}

extern "C" void seed_frame_imports(FrameImportHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.remember) {
 while(true) {
  FrameMeta item=host.next(host.iterator);
  if(item.present==0){return;}
  if(item.has_body==0 && item.enabled==0){host.remember(host.output,item.name);}
 }
}

extern "C" u32 frame_other(address roots,usize reg) on cpu {
 usize first=unsafe_position<usize>(roots);
 usize second=unsafe_position<usize>(unsafe_address(address_bits(roots)+u64(size_of<usize>())));
 return reg!=first && reg!=second ? 1 : 0;
}

bool frame_code(Span code,Span roots,FrameSourceHost host) on cpu
 requires nonzero(host.data.inspect) requires nonzero(host.inspect)
 requires nonzero(host.arguments) requires nonzero(host.contains) {
 usize index=0u64;
 while(index<code.count) {
  address instruction=unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride));
  EffectNode item=host.inspect(instruction);
  if(address_bits(item.target)!=0u64) {
   Span arguments=host.arguments(instruction);
   usize argument=host.contains(host.imported,item.target)!=0 ? 0u64 : 1u64;
   while(argument<arguments.count) {
    View value=host.data.inspect(node_at(arguments,argument,N_ARGUMENT));
    u32 other=1;
    if(value.tag==0){other=visit_span(value.first,roots.data,c_function(frame_other));}
    else {other=visit_reference(value.left,host.data,roots.data,c_function(frame_other));}
    if(other==0){return true;}
    argument+=1u64;
   }
  }else if(operands(Node(instruction,N_INSTRUCTION),host.data,roots.data,c_function(frame_other))==0){return true;}
  if(frame_code(item.first,roots,host)||frame_code(item.second,roots,host)){return true;}
  index+=1u64;
 }
 return false;
}

extern "C" void mark_frame_sources(FrameSourceHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.data.inspect) requires nonzero(host.inspect)
 requires nonzero(host.arguments) requires nonzero(host.contains) {
 while(true) {
  FrameFunction item=host.next(host.iterator);
  if(item.present==0){return;}
  if(unsafe_position<u8>(item.flag)==u8(0)) {
   bool required=frame_code(item.code,item.roots,host);
   usize index=0u64;
   while(!required && index<item.outputs.count) {
    required=frame_other(item.roots.data,word_at(item.outputs,index))==0;
    index+=1u64;
   }
   if(required){unsafe_position<u8>(item.flag)=u8(1);}
  }
 }
}

u32 float_kind(u32 kind) on cpu { return u32(1)<<kind; }
u32 float_instruction_remaining(View value,Span registers,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.scalar) {
 u32 remaining=requested;
 if(value.tag==OP_MATH && value.b!=0u64) {
  remaining&=~float_kind(host.scalar(registers,value.a).kind);
 }else if(value.tag==OP_BINARY && value.d!=0u64) {
  remaining&=~float_kind(host.scalar(registers,value.b).kind);
 }else if(value.tag==OP_CAST) {
  RegisterFacts source=host.scalar(registers,value.b);
  if(source.floating!=0) {
   RegisterFacts target=host.scalar(registers,value.a);
   if(target.integer==0) {
    remaining&=~(float_kind(target.kind)^float_kind(source.kind));
   }
  }
 }else if(value.tag==OP_TILE_COLLECTIVE) {
  usize result=0u64;
  while(result<value.first.count && remaining!=0) {
   remaining&=~float_kind(host.scalar(registers,word_at(value.first,result)).kind);
   result+=1u64;
  }
 }
 return remaining;
}

u32 float_value_remaining(Node source,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 if(requested==0){return 0;}
 View value=host.ir.inspect(source);
 return code_requirements<0>(value.first,value.second,host,CodeRequirements(0,0,requested)).remaining_floats;
}

u32 float_operation_remaining(Node source,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 if(requested==0){return 0;}
 View operation=host.ir.inspect(source);
 u32 remaining=requested;
 if(operation.tag==1 || operation.tag==5 || operation.tag==6) {
  remaining&=~float_kind(host.ir.inspect(operation.left).tag);
 }else if(operation.tag==2 || operation.tag==3 || operation.tag==4) {
  View custom=host.ir.inspect(operation.left);
  remaining=float_value_remaining(custom.left,remaining,host);
  remaining=float_value_remaining(custom.right,remaining,host);
  if(custom.first.count!=0u64) {
   remaining=float_value_remaining(node_at(custom.first,0u64,N_VALUE_CODE),remaining,host);
  }
 }
 return remaining;
}

CodeRequirements stage_code_requirements<const Queries:u32>(
 View stage,RegisterHost host,CodeRequirements initial) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 u32 remaining=float_operation_remaining(stage.left,initial.remaining_floats,host);
 return code_requirements<Queries>(stage.first,stage.second,host,
  CodeRequirements(initial.subgroup,initial.atomic64,remaining));
}

extern "C" u32 float_preserve(Node source,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 let facts=stage_code_requirements<0>(host.ir.inspect(source),host,CodeRequirements(0,0,requested));
 return requested^facts.remaining_floats;
}

bool tile_phase_step(u32 tag) on cpu {
 return tag==OP_TILE_PUBLISH || tag==OP_TILE_COLLECTIVE || tag==OP_TILE_BARRIER;
}

extern "C" u32 has_tile_scope(Span code,u32 has_tiles,Host host) on cpu
 requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_TILE_LENGTH || value.tag==OP_TILE_LANE || value.tag==OP_TILE_LOAD
      || tile_phase_step(value.tag)
      || (has_tiles!=0 && (value.tag==OP_SUBGROUP || value.tag==OP_APPEND))){return 1;}
  if(value.tag==OP_ATOMIC || value.tag==OP_LOAD_INDEXED) {
   View source=host.inspect(value.left);
   if(source.tag==3){return 1;}
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(has_tile_scope(value.first,has_tiles,host)!=0){return 1;}
   if(has_tile_scope(value.second,has_tiles,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
extern "C" void private_spans(Span code,Host host,address context,
                              cfn<void,address,usize,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit) {
 usize index=0u64;
 while(index<code.count){
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  visit_addressable_locals(value,host,context,emit);
  if(value.tag==OP_SCOPE||value.tag==OP_BRANCH||value.tag==OP_LOOP_VALUE){
   private_spans(value.first,host,context,emit);
   private_spans(value.second,host,context,emit);
  }
  index+=1u64;
 }
}

extern "C" void storage_tiles(Span code,Host host,address context,
 cfn<void,address,usize> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_TILE_COLLECTIVE) {
   if(value.first.count!=0u64){emit(context,value.a);}
  }else if(value.tag==OP_TILE_LOAD) {
   emit(context,value.b);
  }else if(value.tag==OP_ATOMIC || value.tag==OP_LOAD_INDEXED) {
   View source=host.inspect(value.left);
   if(source.tag==3){emit(context,source.a);}
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   storage_tiles(value.first,host,context,emit);
   storage_tiles(value.second,host,context,emit);
  }
  index+=1u64;
 }
}

void shared_parts(usize tile,u32 byte,u32 bytes,i64 phase,StorageOutput output) on cpu
 requires nonzero(output.emit) {
 u32 part=0;
 while(part<(bytes+3)/4) {
  output.emit(output.context,StorageEvent::Read,tile,byte+part*4,phase);
  part+=1;
 }
}

void storage_visit(Span code,Span tiles,StorageHost host,i64 first_phase,bool root,StorageOutput output) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes) requires nonzero(output.emit) {
 i64 phase=first_phase;
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_ATOMIC || value.tag==OP_LOAD_INDEXED) {
   View source=host.ir.inspect(value.left);
   if(source.tag==3) {
    if(value.tag==OP_ATOMIC && value.second.count==0u64) {
     shared_parts(source.a,u32(value.a),u32(value.b),phase,output);
    }else{
     View tile=host.ir.inspect(node_at(tiles,source.a,N_TILE));
     usize component=0u64;
     while(component<tile.first.count) {
      View field=host.ir.inspect(node_at(tile.first,component,N_COMPONENT));
      shared_parts(source.a,u32(field.a),u32(field.b),phase,output);
      component+=1u64;
     }
     if(phase>=i64(0)) {
      u32 bias=tile.tag==0 ? u32(tile.b)%4 : 0;
      output.emit(output.context,StorageEvent::Group,source.a,u32((u64(tile.c)+u64(bias)+3u64)/4u64),phase);
     }
    }
   }
  }else if(value.tag==OP_TILE_LOAD) {
   shared_parts(value.b,u32(value.d),host.scalar_bytes(host.stage,value.a),phase,output);
  }else if(value.tag==OP_TILE_COLLECTIVE) {
   usize result=0u64;
   while(result<value.first.count) {
    // Read the original pair's u32 offset without reading the padding.
    u32 byte=unsafe_position<u32>(unsafe_address(address_bits(value.second.data)+u64(result)*u64(value.second.stride)));
    shared_parts(value.a,byte,host.scalar_bytes(host.stage,word_at(value.first,result)),root ? phase : -i64(1),output);
    result+=1u64;
   }
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   storage_visit(value.first,tiles,host,phase,false,output);
   storage_visit(value.second,tiles,host,phase,false,output);
  }
  if(root && phase>=i64(0)) {
   if(value.tag==OP_TILE_PUBLISH) {
    output.emit(output.context,StorageEvent::Birth,value.a,0,phase);
   }
   if(tile_phase_step(value.tag)) {
    phase+=i64(1);
   }
  }
  index+=1u64;
 }
}

extern "C" void storage_facts(Span code,Span tiles,StorageHost host,i64 phase,StorageOutput output) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes) requires nonzero(output.emit) {
 storage_visit(code,tiles,host,phase,true,output);
}

address tile_group_at(address groups,u64 index) on cpu {
 return unsafe_address(address_bits(groups)+index*size_of<TileGroup>());
}

extern "C" u64 coalesce_tile_groups(address groups,u64 count) on cpu {
 u64 used=0u64;
 u64 index=0u64;
 while(index<count) {
  TileGroup current=unsafe_position<TileGroup>(tile_group_at(groups,index));
  u64 kept=0u64;
  u64 read=0u64;
  while(read<used) {
   TileGroup prior=unsafe_position<TileGroup>(tile_group_at(groups,read));
   if(prior.storage==current.storage && prior.owner==current.owner
       && max(prior.first,current.first)<min(prior.end,current.end)) {
    current.first=min(prior.first,current.first);
    current.end=max(prior.end,current.end);
   }else{
    unsafe_position<TileGroup>(tile_group_at(groups,kept))=prior;
    kept+=1u64;
   }
   read+=1u64;
  }
  unsafe_position<TileGroup>(tile_group_at(groups,kept))=current;
  used=kept+1u64;
  index+=1u64;
 }
 return used;
}

extern "C" void tile_input_words(Span tiles,Host host,address used,
 cfn<u32,address,usize,u32> ensures memory(read) contains,address output,
 cfn<void,address,usize,u32,u32,u32> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(contains) requires nonzero(emit) {
 usize index=0u64;
 while(index<tiles.count) {
  View tile=host.inspect(node_at(tiles,index,N_TILE));
  if(tile.tag==0) {
   usize component=0u64;
   while(component<tile.first.count) {
    View field=host.inspect(node_at(tile.first,component,N_COMPONENT));
    u32 bytes=u32(field.b);
    u32 part=0;
    while(part<(bytes+3)/4) {
     u32 byte=u32(tile.b)+u32(field.a)+part*4;
     if(contains(used,tile.a,byte)!=0) {
      emit(output,tile.a,byte&0xfffffffcu,byte,bytes>4?4:bytes);
     }
     part+=1;
    }
    component+=1u64;
   }
  }
  index+=1u64;
 }
}

u32 collective_presence(Span code,Host host,bool include_subgroups) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_APPEND || (include_subgroups && value.tag==OP_SUBGROUP)){return 1;}
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(collective_presence(value.first,host,include_subgroups)!=0){return 1;}
   if(collective_presence(value.second,host,include_subgroups)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
extern "C" u32 has_subgroups(Span code,Host host) on cpu requires nonzero(host.inspect) {
 return collective_presence(code,host,true);
}
// Operation bits follow the existing SubgroupOp discriminants. The two
// following bits describe use of Append and non-32-bit subgroup results.
const u32 SUBGROUP_OPERATIONS=8388607;
const u32 SUBGROUP_APPEND=8388608;
const u32 SUBGROUP_EXTENDED=16777216;
const u32 SUBGROUP_BUILTINS=7;
const u32 SUBGROUP_VOTE=48;
const u32 SUBGROUP_BALLOT=448;
const u32 SUBGROUP_SHUFFLE=1536;
const u32 SUBGROUP_RELATIVE=6144;
const u32 SUBGROUP_ARITHMETIC=8380416;
const u32 APPEND_OPERATIONS=456; // Elect, Count, PrefixCount, BroadcastFirst.

const u32 CODE_SUBGROUPS=1;
const u32 CODE_SUBGROUP_WIDTHS=2;
const u32 CODE_ATOMICS=4;

struct CodeRequirements { u32 subgroup; u32 atomic64; u32 remaining_floats; };

CodeRequirements code_requirements<const Queries:u32>(Span code,Span registers,RegisterHost host,CodeRequirements initial) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 CodeRequirements facts=initial;
 usize index=0u64;
 while(index<code.count &&
       ((Queries&CODE_SUBGROUPS)!=0 || ((Queries&CODE_ATOMICS)!=0 && facts.atomic64!=3) || facts.remaining_floats!=0)) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   facts=code_requirements<Queries>(value.first,registers,host,facts);
   facts=code_requirements<Queries>(value.second,registers,host,facts);
  }else if(value.tag==OP_SUBGROUP) {
   if((Queries&CODE_SUBGROUPS)!=0){facts.subgroup|=1<<u32(value.c);}
   bool width=(Queries&CODE_SUBGROUP_WIDTHS)!=0 && (facts.subgroup&SUBGROUP_EXTENDED)==0;
   // Sum, ExclusiveSum, Min and Max share the result register used by the width query.
   bool floating=facts.remaining_floats!=0 && value.c>=13u64 && value.c<=16u64;
   if(width || floating) {
    RegisterFacts scalar=host.scalar(registers,value.a);
    if(width && scalar.bytes!=4){facts.subgroup|=SUBGROUP_EXTENDED;}
    if(floating){facts.remaining_floats&=~float_kind(scalar.kind);}
   }
  }else if((Queries&CODE_SUBGROUPS)!=0 && value.tag==OP_APPEND) {
   facts.subgroup|=SUBGROUP_APPEND|APPEND_OPERATIONS;
  }else if((Queries&CODE_ATOMICS)!=0 && facts.atomic64!=3 && value.tag==OP_ATOMIC && value.b==8u64) {
   View target=host.ir.inspect(value.left);
   if(target.tag==0){facts.atomic64|=1;}else if(target.tag==3){facts.atomic64|=2;}
  }else if(facts.remaining_floats!=0) {
   facts.remaining_floats=float_instruction_remaining(value,registers,facts.remaining_floats,host);
  }
  index+=1u64;
 }
 return facts;
}

SubgroupFlags subgroup_flags(u32 used) on cpu {
 return SubgroupFlags(
  u32((used&SUBGROUP_OPERATIONS)!=0),u32((used&SUBGROUP_VOTE)!=0),
  u32((used&SUBGROUP_ARITHMETIC)!=0),u32((used&SUBGROUP_BALLOT)!=0),
  u32((used&SUBGROUP_SHUFFLE)!=0),u32((used&SUBGROUP_RELATIVE)!=0),
  u32((used&SUBGROUP_EXTENDED)!=0));
}
u32 shader_subgroup_uses(Span code,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 return code_requirements<1>(code,empty_span(),host,CodeRequirements(0,0,0)).subgroup;
}
extern "C" u32 subgroup_operations(Span code,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 return shader_subgroup_uses(code,host)&SUBGROUP_OPERATIONS;
}
extern "C" ShaderCodeRequirements shader_code_requirements(
 Node source,u32 requested,bool subgroups,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 View stage=host.ir.inspect(source);
 CodeRequirements initial=CodeRequirements(0,0,requested);
 CodeRequirements facts=subgroups
  ? stage_code_requirements<5>(stage,host,initial)
  : stage_code_requirements<4>(stage,host,initial);
 u32 used=facts.subgroup;
 ShaderSubgroups uses=ShaderSubgroups(subgroup_flags(used),used&SUBGROUP_BUILTINS,
  u32((used&SUBGROUP_APPEND)!=0));
 return ShaderCodeRequirements(uses,facts.atomic64,requested^facts.remaining_floats);
}
extern "C" SubgroupFlags subgroup_from_operations(address context,cfn<Word,address> next) on cpu
 requires nonzero(next) {
 u32 used=0;
 while(true) {
  Word operation=next(context);
  if(operation.present==0){return subgroup_flags(used);}
  used|=1<<u32(operation.value);
 }
}
extern "C" SubgroupFlags subgroup_capabilities(Span stages,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 CodeRequirements facts=CodeRequirements(0,0,0);
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.ir.inspect(node_at(stages,index,N_STAGE));
  facts=code_requirements<3>(stage.first,stage.second,host,facts);
  index+=1u64;
 }
 return subgroup_flags(facts.subgroup);
}

extern "C" ProgramCodeRequirements program_code_requirements(Span stages,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 CodeRequirements facts=CodeRequirements(0,0,requested);
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.ir.inspect(node_at(stages,index,N_STAGE));
  facts=stage_code_requirements<7>(stage,host,facts);
  index+=1u64;
 }
 return ProgramCodeRequirements(subgroup_flags(facts.subgroup),facts.atomic64,
  requested^facts.remaining_floats,u32((facts.subgroup&SUBGROUP_APPEND)!=0));
}

void collect_ray_operands(Span code,RayHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  visit_instruction(value,host.ir,host.used,host.include);
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   collect_ray_operands(value.first,host);collect_ray_operands(value.second,host);
  }
  index+=1u64;
 }
}
extern "C" u32 scan_ray_queries(Span code,Host host,bool closest_only,address context,
 cfn<void,address,Span,usize> emit) on cpu requires nonzero(host.inspect) requires nonzero(emit) {
 u32 closest=0; usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_RAY_QUERY) {
   bool hit=value.first.count==39u64;
   if(hit || !closest_only){emit(context,value.first,word_at(value.first,0u64));}
   closest|=u32(hit);
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   closest|=scan_ray_queries(value.first,host,closest_only,context,emit);
   closest|=scan_ray_queries(value.second,host,closest_only,context,emit);
  }
  index+=1u64;
 }
 return closest;
}

extern "C" u32 ray_result_queries(Span code,Span queries,Span ranges,RayHost host,address context,
 cfn<u32,address,usize,usize> visit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) requires nonzero(host.contains)
 requires nonzero(host.fields) requires nonzero(visit) {
 collect_ray_operands(code,host);
 usize query=0u64;
 while(query<queries.count) {
  Span result=unsafe_position<RayQuery>(node_at(queries,query,N_INSTRUCTION).data).fields;
  if(result.count==39u64) {
   usize group=0u64;
   while(group<ranges.count) {
    View range=host.ir.inspect(node_at(ranges,group,N_RAY_RANGE));
    Span fields=host.fields(result,range.a,range.b);
    usize field=0u64; bool consumed=false;
    while(field<fields.count && !consumed) {
     consumed=host.contains(host.used,word_at(fields,field))!=0;field+=1u64;
    }
    if(consumed && visit(context,group,query)==0){return 0;}
    group+=1u64;
   }
  }
  query+=1u64;
 }
 return 1;
}

bool needs_intermediate(u32 operation,usize selection) on cpu {
 return selection!=0u64 || operation==3 || operation==4 || operation==5
  || operation==6 || operation==7 || operation==8;
}
extern "C" usize indirect_buffer(Span stages,usize buffers,Host host) on cpu
 requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.inspect(node_at(stages,index,N_STAGE));
  if(stage.b!=0u64){return buffers+1u64;}
  View operation=host.inspect(stage.left);
  if(needs_intermediate(operation.tag,0u64)){return buffers+1u64;}
  index+=1u64;
 }
 return buffers;
}
extern "C" ExecutionBuffers execution_buffers(Span stages,usize buffers,Host host) on cpu
 requires nonzero(host.inspect) {
 bool intermediate=false;bool prefix=false;bool selection=false;bool scan_alias=false;
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.inspect(node_at(stages,index,N_STAGE));View operation=host.inspect(stage.left);
  intermediate=intermediate || needs_intermediate(operation.tag,stage.b);
  prefix=prefix || stage.a!=0u64 || stage.b!=0u64;selection=selection || stage.b!=0u64;
  if(operation.tag==3 || operation.tag==4 || operation.tag==5 || operation.tag==6) {
   scan_alias=scan_alias || operation.a==operation.b;
  }
  index+=1u64;
 }
 usize indirect=buffers+(intermediate?1u64:0u64);
 usize auxiliary=(intermediate?1u64:0u64)+(prefix?1u64:0u64)+(selection?1u64:0u64);
 if(scan_alias && auxiliary<indirect+3u64-buffers){auxiliary=indirect+3u64-buffers;}
 return ExecutionBuffers(indirect,auxiliary);
}

const u32 N_BUFFER=20;const u32 N_UNIFORM=21;const u32 N_VALUE_CODE=22;const u32 N_SCALAR=24;
void numeric_scalar(inout NumericRequirements flags,u32 scalar,bool code) on cpu {
 if(code && scalar==6){flags.float64=1;}
 if(scalar==7 || scalar==8){flags.integer8=1;}
 if(scalar==9 || scalar==10){flags.integer16=1;}
}
void numeric_components(inout NumericRequirements flags,Span components,bool code,Host host) on cpu
 requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<components.count) {
  View component=host.inspect(node_at(components,index,N_COMPONENT));
  numeric_scalar(flags,u32(component.c),code);index+=1u64;
 }
}
extern "C" void numeric_local_scalar(address context,u32 scalar) on cpu {
 NumericRequirements flags=unsafe_position<NumericRequirements>(context);
 numeric_scalar(flags,scalar,true);
 unsafe_position<NumericRequirements>(context)=flags;
}
void numeric_code(inout NumericRequirements flags,Node source,NumericHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.locals) {
 View value=host.ir.inspect(source);usize index=0u64;
 while(index<value.second.count) {
  View scalar=host.ir.inspect(node_at(value.second,index,N_SCALAR));
  numeric_scalar(flags,scalar.tag,true);index+=1u64;
 }
 host.locals(unsafe_address(u64(value.c)),address_of(flags),c_function(numeric_local_scalar));
}
extern "C" NumericRequirements numeric_requirements(Span stages,Span buffers,Span uniforms,NumericHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.locals) {
 NumericRequirements flags=NumericRequirements(0,0,0);usize index=0u64;
 while(index<buffers.count) {
  View buffer=host.ir.inspect(node_at(buffers,index,N_BUFFER));numeric_components(flags,buffer.first,false,host.ir);index+=1u64;
 }
 index=0u64;
 while(index<uniforms.count) {
  View uniform=host.ir.inspect(node_at(uniforms,index,N_UNIFORM));numeric_components(flags,uniform.first,false,host.ir);index+=1u64;
 }
 index=0u64;
 while(index<stages.count) {
  Node source=node_at(stages,index,N_STAGE);numeric_code(flags,source,host);
  View stage=host.ir.inspect(source);View operation=host.ir.inspect(stage.left);
  if(operation.tag==1 || operation.tag==5 || operation.tag==6) {
   numeric_components(flags,operation.first,true,host.ir);
  }else if(operation.tag==2 || operation.tag==3 || operation.tag==4) {
   View custom=host.ir.inspect(operation.left);numeric_code(flags,custom.left,host);numeric_code(flags,custom.right,host);
   usize project=0u64;
   while(project<custom.first.count) {
    numeric_code(flags,node_at(custom.first,project,N_VALUE_CODE),host);project+=1u64;
   }
  }
  index+=1u64;
 }
 return flags;
}

extern "C" u32 maximal_reconvergence(Span code,Host host) on cpu requires nonzero(host.inspect) {
 return collective_presence(code,host,false);
}
extern "C" u32 program_maximal_reconvergence(Span stages,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.inspect(node_at(stages,index,N_STAGE));
  if(collective_presence(stage.first,host,false)!=0){return 1;}
  index+=1u64;
 }
 return 0;
}

const u32 N_PAGE_BUFFER=28;
const u32 N_PAGE_VARIANT=30;
const u32 N_PAGE_FIELD=31;

bool page_type_fits(Node source,u32 bytes,Host host) on cpu requires nonzero(host.inspect) {
 View value=host.inspect(source);
 Span children=value.first;
 u32 kind=N_PAGE_VARIANT;
 if(children.count==0u64) {
  if(value.tag==1){return value.a<=usize(bytes);}
  if(value.tag==2){return value.b==0u64 || page_type_fits(value.left,bytes,host);}
  children=value.second;
  kind=N_PAGE_FIELD;
 }
 usize index=0u64;
 while(index<children.count) {
  View child=host.inspect(node_at(children,index,kind));
  if(!page_type_fits(child.left,bytes,host)){return false;}
  index+=1u64;
 }
 return true;
}

extern "C" u32 shader_pages_fit(Span buffers,u32 bytes,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<buffers.count) {
  View buffer=host.inspect(node_at(buffers,index,N_PAGE_BUFFER));
  if(buffer.a>usize(bytes) || !page_type_fits(buffer.left,bytes,host)){return 0;}
  index+=1u64;
 }
 return 1;
}

void read_expand(address pending,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued)
 requires nonzero(host.push) requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.enqueue_operand) {
 Node node=host.queued(pending,id);
 if(address_bits(node.data)==0u64){return;}
 host.push(pending,ReadWork(id,ReadWorkKind::Emit));
 usize start=host.work_len(pending);
 operands(node,host.ir,pending,host.enqueue_operand);
 // The existing stack emits the earliest operand's producer first.
 host.reverse(pending,start);
}

extern "C" void read_demand(address pending,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.push)
 requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand) {
 read_expand(pending,id,host);
 while(true) {
  ReadWork work=host.pop(pending);
  if(work.kind==ReadWorkKind::Done){return;}
  if(work.kind==ReadWorkKind::Emit){host.emit(pending,work.id);}
  else{read_expand(pending,work.id,host);}
 }
}

void finish_shared_reads(address pending,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.push)
 requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand) requires nonzero(host.count) requires nonzero(host.read_at)
 requires nonzero(host.clear) {
 usize count=host.count(pending,ReadClear::Shared);
 usize index=0u64;
 while(index<count) {
  read_demand(pending,host.read_at(pending,index),host);
  index+=1u64;
 }
 host.clear(pending,ReadClear::Shared,0u64);
}

void finish_overlapping_reads(address pending,usize first,usize end,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.push)
 requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand) requires nonzero(host.count) requires nonzero(host.private_at)
 requires nonzero(host.retain) requires nonzero(host.clear) {
 usize count=host.count(pending,ReadClear::Local);
 usize kept=0u64;
 usize index=0u64;
 while(index<count) {
  ReadPrivate row=host.private_at(pending,index);
  if(row.pending) {
   if(row.first<end&&first<row.end){read_demand(pending,row.id,host);}
   else {
    host.retain(pending,index,kept);
    kept+=1u64;
   }
  }
  index+=1u64;
 }
 host.clear(pending,ReadClear::Local,kept);
}

void read_finish(address pending,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.push)
 requires nonzero(host.work_len) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand) requires nonzero(host.count) requires nonzero(host.clear) {
 usize count=host.count(pending,ReadClear::All);
 usize index=0u64;
 while(index<count) {
  read_demand(pending,index,host);
  index+=1u64;
 }
 host.clear(pending,ReadClear::All,0u64);
}

bool binding_written(Span writes,usize binding) on cpu {
 usize index=0u64;
 while(index<writes.count) {
  if(word_at(writes,index)==binding){return true;}
  index+=1u64;
 }
 return false;
}

extern "C" void place_reads(address input,address pending,Span writes,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.math_outgoing)
 requires nonzero(host.next) requires nonzero(host.sample) requires nonzero(host.child) requires nonzero(host.store)
 requires nonzero(host.queued) requires nonzero(host.enqueue_operand) requires nonzero(host.demand_operand) requires nonzero(host.work_len)
 requires nonzero(host.push) requires nonzero(host.reverse) requires nonzero(host.pop)
 requires nonzero(host.emit) requires nonzero(host.enqueue_operand) requires nonzero(host.count) requires nonzero(host.read_at)
 requires nonzero(host.private_at) requires nonzero(host.retain) requires nonzero(host.clear) {
 while(true) {
  Node node=host.next(input);
  if(address_bits(node.data)==0u64){break;}
  View inst=host.ir.inspect(node);
  u32 tag=inst.tag;
  if(tag==OP_TEXTURE_SAMPLE){host.sample(input);continue;}
  if(tag==OP_CONSTANT||tag==OP_INDEX_VALUE||tag==OP_UNARY||tag==OP_BINARY||tag==OP_INTEGER_DIVIDE||
     tag==OP_CAST||tag==OP_SELECT_VALUE||(tag==OP_MATH&&host.math_outgoing(u32(inst.b))==0u64)) {
   host.store(input,pending,ReadQueue::Value,0u64,0u64);
  } else if(tag==OP_LOAD_LOCAL||tag==OP_LOAD_LOCAL_INDEXED) {
   usize end=inst.b+1u64;
   if(tag==OP_LOAD_LOCAL_INDEXED){end=inst.b+inst.c;}
   host.store(input,pending,ReadQueue::Local,inst.b,end);
  } else if(tag==OP_VIEW_LOAD||(tag==OP_LOAD&&!binding_written(writes,inst.b))) {
   host.store(input,pending,ReadQueue::Shared,0u64,0u64);
  } else {
   if(tag==OP_STORE_LOCAL||tag==OP_STORE_LOCAL_INDEXED) {
    finish_shared_reads(pending,host);
    usize end=inst.a+1u64;
    if(tag==OP_STORE_LOCAL_INDEXED){end=inst.a+inst.b;}
    finish_overlapping_reads(pending,inst.a,end,host);
    visit_instruction(inst,host.ir,pending,host.demand_operand);
   } else if(tag==OP_STORE||tag==OP_STORE_INDEXED||tag==OP_LOAD_INDEXED||tag==OP_LOAD) {
    finish_shared_reads(pending,host);
    visit_instruction(inst,host.ir,pending,host.demand_operand);
   } else if(tag!=OP_UNIFORM) {
    // A full boundary leaves no pending producer to demand for this instruction.
    read_finish(pending,host);
    if(tag==OP_BRANCH||tag==OP_LOOP_VALUE) {
     host.child(input,0);
     host.child(input,1);
    } else if(tag==OP_SCOPE) {
     host.child(input,0);
    }
   }
   host.store(input,pending,ReadQueue::Immediate,0u64,0u64);
  }
 }
 read_finish(pending,host);
}

extern "C" CpuRangePlan cpu_range_plan(CpuRangePass pass,usize words,u32 has_head,u32 counted) on cpu {
 bool apply=pass==CpuRangePass::ScanAdd;
 bool finish=pass==CpuRangePass::SegmentFinish;
 bool prefix=pass==CpuRangePass::ScanPrefix || pass==CpuRangePass::SegmentPrefix;
 bool project=pass==CpuRangePass::Reduce || pass==CpuRangePass::Scan ||
  pass==CpuRangePass::SegmentReduce || finish;
 bool publish=pass==CpuRangePass::Reduce || pass==CpuRangePass::ReduceTail ||
  pass==CpuRangePass::Scan || pass==CpuRangePass::SegmentReduce;
 return CpuRangePlan(u32(project),u32(!apply),u32(apply),u32(finish),
  u32(prefix || pass==CpuRangePass::ReduceTail),u32(prefix),u32(publish),
  words+usize(has_head)+usize(counted));
}

void range_store_locals(Span values,usize carry,CpuRangeBlock part,address context,CpuRangeHost host) on cpu
 requires nonzero(host.put) {
 usize slot=0u64;
 while(slot<values.count) {
  host.put(context,part,CpuRangeOp::LocalStore,carry+slot,word_at(values,slot),0u64,0);
  slot+=1u64;
 }
}

void range_add_count(usize slot,usize value,CpuRangeBlock part,address context,CpuRangeHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.put) {
 usize before=host.reg(context,CpuRangeType::U32,0u64);
 usize after=host.reg(context,CpuRangeType::U32,0u64);
 host.put(context,part,CpuRangeOp::LocalLoad,before,slot,0u64,0);
 host.put(context,part,CpuRangeOp::Add,after,before,value,0);
 host.put(context,part,CpuRangeOp::LocalStore,slot,after,0u64,0);
}

void range_segment_tail(CpuRangeInput input,usize carry,usize index,usize length_slot,
 address context,CpuRangeHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.local) requires nonzero(host.offset)
 requires nonzero(host.put) requires nonzero(host.branch) {
 CpuRangeBlock body=CpuRangeBlock::Body;
 CpuRangeBlock yes=CpuRangeBlock::Yes;
 CpuRangeBlock no=CpuRangeBlock::No;
 usize one=host.reg(context,CpuRangeType::U32,0u64);
 usize next=host.reg(context,CpuRangeType::U32,0u64);
 usize length=host.reg(context,CpuRangeType::U32,0u64);
 usize last=host.reg(context,CpuRangeType::Bool,0u64);
 host.put(context,body,CpuRangeOp::Constant,one,1u64,0u64,0);
 host.put(context,body,CpuRangeOp::Add,next,index,one,0);
 host.put(context,body,CpuRangeOp::LocalLoad,length,length_slot,0u64,0);
 host.put(context,body,CpuRangeOp::Equal,last,next,length,0);
 usize tail_slot=host.local(context,CpuRangeType::Bool);
 usize is_last=host.reg(context,CpuRangeType::Bool,0u64);
 usize next_head=host.reg(context,CpuRangeType::Bool,0u64);
 host.put(context,yes,CpuRangeOp::Constant,is_last,1u64,0u64,0);
 host.put(context,yes,CpuRangeOp::LocalStore,tail_slot,is_last,0u64,0);
 host.put(context,no,CpuRangeOp::Load,next_head,input.source,next,input.head_word);
 host.put(context,no,CpuRangeOp::LocalStore,tail_slot,next_head,0u64,0);
 host.branch(context,last,no);

 usize tail=host.reg(context,CpuRangeType::Bool,0u64);
 host.put(context,body,CpuRangeOp::LocalLoad,tail,tail_slot,0u64,0);
 usize segments=host.reg(context,CpuRangeType::U32,0u64);
 usize target=host.reg(context,CpuRangeType::U32,0u64);
 host.put(context,yes,CpuRangeOp::LocalLoad,segments,carry+input.fields+1u64,0u64,0);
 host.put(context,yes,CpuRangeOp::Subtract,target,segments,one,0);
 usize tail_field=0u64;
 while(tail_field<input.fields) {
  usize value=host.reg(context,CpuRangeType::Value,tail_field);
  host.put(context,yes,CpuRangeOp::LocalLoad,value,carry+tail_field,0u64,0);
  host.put(context,yes,CpuRangeOp::Store,value,input.target,target,
   input.target_word+host.offset(context,tail_field,0));
  tail_field+=1u64;
 }
 host.branch(context,tail,no);
}

extern "C" CpuRangeBody cpu_range_body(CpuRangeInput input,CpuRangePlan plan,address context,CpuRangeHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.local) requires nonzero(host.offset) requires nonzero(host.put)
 requires nonzero(host.move_block) requires nonzero(host.branch) requires nonzero(host.size) {
 CpuRangeBlock body=CpuRangeBlock::Body;
 CpuRangeBlock yes=CpuRangeBlock::Yes;
 CpuRangeBlock no=CpuRangeBlock::No;
 bool apply=plan.apply!=0;
 bool finish=plan.finish!=0;
 bool packed=plan.packed!=0;
 bool prefix=plan.prefix!=0;
 bool has_head=input.has_head!=0;
 bool counted=input.counted!=0;
 bool segment_prefix=input.pass==CpuRangePass::SegmentPrefix;
 bool scan=input.pass==CpuRangePass::Scan;
 bool history=has_head && !apply && !finish && !segment_prefix;
 bool reset_on_head=has_head && (scan || input.pass==CpuRangePass::SegmentReduce || finish);
 usize carry=host.local(context,CpuRangeType::Value);
 usize flag_slot=carry+input.fields;
 if(has_head && !apply){host.local(context,CpuRangeType::Bool);}
 usize count_slot=counted ? host.local(context,CpuRangeType::U32) : 0u64;
 usize private_index=host.reg(context,CpuRangeType::U32,0u64);
 if(apply || finish){host.put(context,body,CpuRangeOp::Constant,private_index,0u64,0u64,0);}
 usize length_slot=0u64;
 if(finish) {
  length_slot=host.local(context,CpuRangeType::U32);
  usize length=host.reg(context,CpuRangeType::U32,0u64);
  host.put(context,body,CpuRangeOp::Load,length,input.temporary,private_index,input.payload_bytes);
  host.put(context,body,CpuRangeOp::LocalStore,length_slot,length,0u64,0);
 }
 if(apply || finish) {
  usize input_field=0u64;
  while(input_field<input.fields) {
   usize value=host.reg(context,CpuRangeType::Value,input_field);
   host.put(context,body,CpuRangeOp::Load,value,input.temporary,private_index,host.offset(context,input_field,0));
   host.put(context,body,CpuRangeOp::LocalStore,carry+input_field,value,0u64,0);
   input_field+=1u64;
  }
 }else{
  range_store_locals(input.initial,carry,body,context,host);
  if(history) {
   usize zero=host.reg(context,CpuRangeType::Bool,0u64);
   host.put(context,body,CpuRangeOp::Constant,zero,0u64,0u64,0);
   host.put(context,body,CpuRangeOp::LocalStore,flag_slot,zero,0u64,0);
  }
 }
 if(counted) {
  usize value=host.reg(context,CpuRangeType::U32,0u64);
  if(finish) {
   host.put(context,body,CpuRangeOp::Load,value,input.temporary,private_index,input.payload_bytes+4);
  }else{
   host.put(context,body,CpuRangeOp::Constant,value,0u64,0u64,0);
  }
  host.put(context,body,CpuRangeOp::LocalStore,count_slot,value,0u64,0);
 }
 usize initialization_end=host.size(context);
 usize index=host.reg(context,CpuRangeType::U32,0u64);
 host.put(context,body,CpuRangeOp::Index,index,0u64,0u64,0);
 usize head=0u64;
 if(has_head) {
  usize raw=host.reg(context,CpuRangeType::Bool,0u64);
  host.put(context,body,CpuRangeOp::Load,raw,input.source,index,packed ? input.payload_bytes : input.head_word);
  head=raw;
  if(counted && !packed) {
   usize zero=host.reg(context,CpuRangeType::U32,0u64);
   usize first=host.reg(context,CpuRangeType::Bool,0u64);
   head=host.reg(context,CpuRangeType::Bool,0u64);
   host.put(context,body,CpuRangeOp::Constant,zero,0u64,0u64,0);
   host.put(context,body,CpuRangeOp::Equal,first,index,zero,0);
   host.put(context,body,CpuRangeOp::Or,head,raw,first,0);
  }
  if(apply) {
   host.put(context,yes,CpuRangeOp::Break,0u64,0u64,0u64,0);
   host.branch(context,head,no);
  }else if(reset_on_head) {
   range_store_locals(input.initial,carry,yes,context,host);
   if(history) {
    usize flag=host.reg(context,CpuRangeType::Bool,0u64);
    host.put(context,yes,CpuRangeOp::Constant,flag,1u64,0u64,0);
    host.put(context,yes,CpuRangeOp::LocalStore,flag_slot,flag,0u64,0);
   }
   if(counted) {
    usize before=host.reg(context,CpuRangeType::U32,0u64);
    usize one=host.reg(context,CpuRangeType::U32,0u64);
    usize after=host.reg(context,CpuRangeType::U32,0u64);
    host.put(context,yes,CpuRangeOp::LocalLoad,before,count_slot,0u64,0);
    host.put(context,yes,CpuRangeOp::Constant,one,1u64,0u64,0);
    host.put(context,yes,CpuRangeOp::Add,after,before,one,0);
    host.put(context,yes,CpuRangeOp::LocalStore,count_slot,after,0u64,0);
   }
   host.branch(context,head,no);
  }
 }

 usize left_field=0u64;
 while(left_field<input.left.count) {
  host.put(context,body,CpuRangeOp::LocalLoad,word_at(input.left,left_field),carry+left_field,0u64,0);
  left_field+=1u64;
 }
 usize source_field=0u64;
 while(source_field<input.source_registers.count) {
  u32 offset=packed ? host.offset(context,source_field,0)
   : apply ? input.target_word+host.offset(context,source_field,0)
   : input.source_word+host.offset(context,source_field,1);
  host.put(context,body,CpuRangeOp::Load,word_at(input.source_registers,source_field),
   apply ? input.target : input.source,index,offset);
  source_field+=1u64;
 }
 usize right_count=0u64;
 if(counted && packed) {
  right_count=host.reg(context,CpuRangeType::U32,0u64);
  host.put(context,body,CpuRangeOp::Load,right_count,input.source,index,input.payload_bytes+4);
 }
 host.move_block(context,CpuRangeBlock::Project);
 if(scan || prefix) {
  usize scan_field=0u64;
  while(scan_field<input.left.count) {
   u32 offset=host.offset(context,scan_field,0)+(packed ? 0 : input.target_word);
   host.put(context,body,CpuRangeOp::Store,word_at(input.left,scan_field),input.target,index,offset);
   scan_field+=1u64;
  }
  if(input.pass==CpuRangePass::ScanPrefix && has_head) {
   usize flag=host.reg(context,CpuRangeType::Bool,0u64);
   host.put(context,body,CpuRangeOp::LocalLoad,flag,flag_slot,0u64,0);
   host.put(context,body,CpuRangeOp::Store,flag,input.target,index,input.payload_bytes);
  }
  if(segment_prefix) {
   usize length=host.reg(context,CpuRangeType::U32,0u64);
   usize segments=host.reg(context,CpuRangeType::U32,0u64);
   host.put(context,body,CpuRangeOp::Uniform,length,0u64,0u64,0);
   host.put(context,body,CpuRangeOp::LocalLoad,segments,count_slot,0u64,0);
   host.put(context,body,CpuRangeOp::Store,length,input.target,index,input.payload_bytes);
   host.put(context,body,CpuRangeOp::Store,segments,input.target,index,input.payload_bytes+4);
  }
 }
 CpuRangeBlock combine=CpuRangeBlock::Combine;
 if(apply) {
  usize result_field=0u64;
  while(result_field<input.result.count) {
   host.put(context,combine,CpuRangeOp::Store,word_at(input.result,result_field),input.target,
    index,input.target_word+host.offset(context,result_field,0));
   result_field+=1u64;
  }
 }else{
  range_store_locals(input.result,carry,combine,context,host);
 }
 if(has_head && prefix) {
  range_store_locals(input.projected,carry,yes,context,host);
  if(history) {
   usize flag=host.reg(context,CpuRangeType::Bool,0u64);
   host.put(context,yes,CpuRangeOp::Constant,flag,1u64,0u64,0);
   host.put(context,yes,CpuRangeOp::LocalStore,flag_slot,flag,0u64,0);
  }
  host.branch(context,head,combine);
 }else{
  host.move_block(context,combine);
 }
 if(counted && packed){range_add_count(count_slot,right_count,body,context,host);}
 if(finish){range_segment_tail(input,carry,index,length_slot,context,host);}
 usize loop_end=host.size(context);
 if(segment_prefix) {
  usize zero=host.reg(context,CpuRangeType::U32,0u64);
  usize total=host.reg(context,CpuRangeType::U32,0u64);
  host.put(context,body,CpuRangeOp::Constant,zero,0u64,0u64,0);
  host.put(context,body,CpuRangeOp::LocalLoad,total,count_slot,0u64,0);
  host.put(context,body,CpuRangeOp::Store,total,input.count_binding,zero,input.count_word);
 }
 return CpuRangeBody(initialization_end,loop_end,carry,count_slot,length_slot,
  u32(history),u32(reset_on_head),u32(apply && has_head));
}

