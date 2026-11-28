module bootstrap.ir_dataflow;
import codec.builtin_type;
import codec.type_layout;

// These tags describe the existing borrowed Rust IR fields at this one
// private native handoff. They do not name a public IR format or registry.
const u32 N_CALL=1;const u32 N_TARGET=2;
const u32 N_DATASET_BORROW=4;const u32 N_REFERENCE=5;const u32 N_STORAGE=6;
const u32 N_INDEXED=7;const u32 N_STRIDE=8;const u32 N_TEXTURE=9;const u32 N_ATOMIC_OP=10;
const u32 N_DIVISION=13;
const u32 N_DIVISOR=14;const u32 N_WORDS3=15;
const u32 N_STAGE=17;const u32 N_RAY_RANGE=18;

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

extern "C" InitialLoopEdit next_initial_loop_edit(InitialLoopEdit start,usize inserted,InitialLoopEditor host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.block) requires nonzero(host.bodies)
 requires nonzero(host.push) requires nonzero(host.pop) requires nonzero(host.record_break) {
 InitialLoopEdit cursor=InitialLoopEdit(start.block,start.index+inserted);
 while(cursor.block!=address()){
  Span code=host.block(cursor.block);
  usize index=cursor.index;
  while(index<code.count){
   u32 kind=host.ir.inspect(node_at(code,index,N_INSTRUCTION)).tag;
   if(kind==OP_CONTINUE_VALUE){return InitialLoopEdit(cursor.block,index);}
   if(kind==OP_BREAK_VALUE){host.record_break(host.context);}
   if(kind==OP_BRANCH||kind==OP_SCOPE){
    InitialLoopBodies bodies=host.bodies(cursor.block,index);
    host.push(host.context,InitialLoopEdit(cursor.block,index+1u64));
    if(kind==OP_BRANCH){host.push(host.context,InitialLoopEdit(bodies.second,0u64));}
    cursor=InitialLoopEdit(bodies.first,0u64);
    break;
   }
   index+=1u64;
  }
  if(index==code.count){cursor=host.pop(host.context);}
 }
 return cursor;
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

address stored_call_position(Span arguments,u32 word,u32 scalar,address context,StoredPositionHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.zero_offset) requires nonzero(host.matches_position) {
 usize index=0u64;
 while(index<arguments.count){
  View argument=host.ir.inspect(node_at(arguments,index,N_ARGUMENT));
  index+=1u64;
  if(argument.tag!=ARGUMENT_REFERENCE||!accessed(argument.a,1)){continue;}
  View reference=host.ir.inspect(argument.left);
  if(reference.first.count!=0u64&&host.zero_offset(context,word_at(reference.first,0u64))==0){continue;}
  View storage=host.ir.inspect(reference.left);
  if(storage.tag!=STORAGE_INDEXED){continue;}
  usize part=0u64;
  while(part<argument.second.count){
   View field=host.ir.inspect(node_at(argument.second,part,N_COMPONENT));
   part+=1u64;
   if(field.c==u64(scalar)&&u32(reference.a)+u32(field.a)==word){
    if(host.matches_position(context,storage.left.data)!=0){return storage.left.data;}
    break;
   }
  }
 }
 return unsafe_address(0u64);
}

extern "C" address stored_position(Span code,Span registers,u32 word,u32 scalar,address context,StoredPositionHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.zero_offset) requires nonzero(host.matches_position) {
 usize index=0u64;
 while(index<code.count){
  View instruction=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  index+=1u64;
  u32 kind=instruction.tag;
  if(kind==OP_STORE_INDEXED){
   if(instruction.c==u64(word)
      &&host.ir.inspect(node_at(registers,instruction.b,N_SCALAR)).tag==scalar
      &&host.zero_offset(context,instruction.a)!=0
      &&host.matches_position(context,instruction.left.data)!=0){
    return instruction.left.data;
   }
  }else if(kind==OP_INVOKE){
   View call=host.ir.inspect(instruction.left);
   address found=stored_call_position(call.first,word,scalar,context,host);
   if(found!=unsafe_address(0u64)){return found;}
  }else if(kind==OP_BRANCH||kind==OP_LOOP_VALUE||kind==OP_SCOPE){
   address found=stored_position(instruction.first,registers,word,scalar,context,host);
   if(found!=unsafe_address(0u64)){return found;}
   if(kind!=OP_SCOPE){
    found=stored_position(instruction.second,registers,word,scalar,context,host);
    if(found!=unsafe_address(0u64)){return found;}
   }
  }
 }
 return unsafe_address(0u64);
}

extern "C" void stored_write_paths(Span code,address context,StoredWriteHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.active) requires nonzero(host.transfer)
 requires nonzero(host.branch) requires nonzero(host.scope) requires nonzero(host.write) {
 usize index=0u64;
 while(index<code.count&&host.active(context)!=0){
  View instruction=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  u32 kind=instruction.tag;
  if(kind==OP_CONTINUE_VALUE){host.transfer(context,StoredTransfer::Continue);return;}
  if(kind==OP_EXIT_VALUE){host.transfer(context,StoredTransfer::Exit);return;}
  if(kind==OP_BREAK_VALUE){host.transfer(context,StoredTransfer::Break);return;}
  if(kind==OP_BRANCH){host.branch(context,instruction.a,instruction.first,instruction.second);}
  else if(kind==OP_SCOPE){host.scope(context,instruction.first);}
  else{host.write(context,index);}
  index+=1u64;
 }
}

extern "C" void stored_arms(address context,StoredTruth truth,cfn<void,address,u32,u32> arm) on cpu
 requires nonzero(arm) {
 if(truth!=StoredTruth::No){arm(context,1,u32(truth==StoredTruth::Both));}
 if(truth!=StoredTruth::Yes){arm(context,0,0);}
}

extern "C" void iteration_interference(Span code,address context,InterferenceHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.branch) requires nonzero(host.nested) requires nonzero(host.write) {
 usize index=0u64;
 while(index<code.count){
  View instruction=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  u32 kind=instruction.tag;
  if(kind==OP_BRANCH){host.branch(context,instruction.a,instruction.first,instruction.second);}
  else if(kind==OP_SCOPE||kind==OP_LOOP_VALUE){host.nested(context,instruction.first,instruction.second);}
  else if(kind==OP_STORE||kind==OP_STORE_INDEXED||kind==OP_INVOKE||kind==OP_ATOMIC||kind==OP_APPEND){
   host.write(context,index);
  }
  index+=1u64;
 }
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

Results ordinary_results(read View value) on cpu {
 Results output=Results(empty_span(),empty_span(),0u64,0u64,0u64,0u64);
 u32 kind=value.tag;
 if(kind==OP_INVOKE||kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH||kind==OP_TEXTURE_SIZE
     ||kind==OP_RAY_QUERY||kind==OP_TILE_COLLECTIVE){
  output.first=value.first;
  if(kind==OP_INVOKE){output.second=value.second;}
 }else if(kind==OP_ATOMIC){
  output.first=value.first;
 }else if(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
     ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ||kind==OP_APPEND||kind==OP_CONSTANT
     ||kind==OP_FUNCTION_ADDRESS||kind==OP_INDEX_VALUE||kind==OP_UNIFORM||kind==OP_BUFFER_STRIDE
     ||kind==OP_LOAD_LOCAL||kind==OP_TILE_LENGTH||kind==OP_TILE_LANE||kind==OP_POSITION_ADDRESS
     ||kind==OP_LOAD_INDEXED||kind==OP_LOAD||kind==OP_LOAD_LOCAL_INDEXED||kind==OP_SUBGROUP
     ||kind==OP_PRINT||kind==OP_TILE_LOAD||kind==OP_VIEW_LOAD||kind==OP_UNARY||kind==OP_BINARY
     ||kind==OP_CAST||kind==OP_MATH||kind==OP_SELECT_VALUE){
  output.a=value.a;
  output.inline_count=1u64;
 }
 return output;
}
Results division_results(usize a,usize b,u32 kind) on cpu {
 return Results(empty_span(),empty_span(),a,b,kind==2 ? 2u64 : 1u64,0u64);
}
Results view_results(View value,Host host) on cpu requires nonzero(host.inspect) {
 if(value.tag==OP_INTEGER_DIVIDE){
  View division=host.inspect(value.right);
  return division_results(division.a,division.b,division.tag);
 }
 return ordinary_results(value);
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

RegisterDefinition find_register_definition(Span code,usize reg,bool newest,Host host) on cpu
 requires nonzero(host.inspect) {
 address missing=unsafe_address(0u64);
 usize cursor=0u64;
 while(cursor<code.count){
  usize index=newest ? code.count-cursor-1u64 : cursor;
  Node node=node_at(code,index,N_INSTRUCTION);
  View instruction=host.inspect(node);
  u32 kind=instruction.tag;
  if(kind==OP_BRANCH||kind==OP_LOOP_VALUE||kind==OP_SCOPE){
   address found=find_register_definition(instruction.first,reg,false,host).instruction;
   if(found==missing&&kind!=OP_SCOPE){
    found=find_register_definition(instruction.second,reg,false,host).instruction;
   }
   if(found!=missing){return RegisterDefinition(found,index);}
  }else{
   Results outputs=view_results(instruction,host);
   Word result=next_result(outputs);
   while(result.present!=0){
    if(result.value==reg){return RegisterDefinition(node.data,index);}
    result=next_result(outputs);
   }
  }
  cursor+=1u64;
 }
 return RegisterDefinition(missing,0u64);
}
extern "C" RegisterDefinition register_definition(Span code,usize reg,Host host) on cpu
 requires nonzero(host.inspect) {
 return find_register_definition(code,reg,true,host);
}

OriginResults origin_results(read View value,Host host) on cpu requires nonzero(host.inspect) {
 Results output;
 ResultRole role=ResultRole::Value;
 if(value.tag==OP_INTEGER_DIVIDE){
  View division=host.inspect(value.right);
  output=division_results(division.a,division.b,division.tag);
  role=division.tag==1 ? ResultRole::Remainder : ResultRole::Quotient;
 }else{output=ordinary_results(value);}
 return OriginResults(output,output.first.count+output.second.count+output.inline_count,role);
}
OriginResult next_origin_result(inout OriginResults state) on cpu {
 Word entry=next_result(state.outputs);
 ResultRole role=state.role;
 if(role==ResultRole::Quotient){state.role=ResultRole::Remainder;}
 return OriginResult(entry.value,role);
}
extern "C" void result_origins(Node source,Host host,address context,
 cfn<u32,address,usize,usize,usize,ResultRole> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);
 OriginResults output=origin_results(value,host);
 usize index=0u64;
 while(index<output.count){
  OriginResult entry=next_origin_result(output);
  visit(context,entry.reg,index,output.count,entry.role);
  index+=1u64;
 }
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
  if(continued!=0&&value.d!=0u64){continued=visit(context,value.d-1u64);}
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
 requires nonzero(host.whole) requires nonzero(host.scalar) requires nonzero(host.rewrite.inspect) {
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
 rewrite_compact(RewriteSpan(input.code.data,input.code.count,input.code.stride),host.rewrite,
  input.register_map,input.local_map);
 compact_roots(input.inputs,input.register_map,1);compact_roots(input.outputs,input.register_map,1);
 return kept;
}


address reduction_byte(address uniform,usize reg) on cpu {return unsafe_address(address_bits(uniform)+u64(reg));}
extern "C" u32 reduction_uniform_operand(address uniform,usize reg) on cpu {
 return unsafe_position<u8>(reduction_byte(uniform,reg))!=u8(0)?1:0;
}
struct ReductionExtract { u32 eligible; u32 operation; usize inputs; };
ReductionExtract reduction_extract(Node source,address uniform,Host ir) on cpu requires nonzero(ir.inspect) {
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
void reduction_extract_part(usize partition,address uniform,ReductionHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.open_part) requires nonzero(host.hoist)
 requires nonzero(host.move_instruction) requires nonzero(host.close_part) {
 Span code=host.open_part(host.context,partition);
 usize read=0u64;
 usize kept=0u64;
 while(read<code.count){
  Node inst=node_at(code,read,N_INSTRUCTION);
  ReductionExtract decision=reduction_extract(inst,uniform,host.ir);
  if(decision.eligible!=0){
   host.hoist(host.context,partition,inst.data,decision.operation,decision.inputs);
  }else{
   if(kept!=read){host.move_instruction(node_at(code,kept,N_INSTRUCTION).data,inst.data);}
   kept+=1u64;
  }
  read+=1u64;
 }
 host.close_part(host.context,partition,kept);
}
void reduction_copy_roots(Span values,usize offset,usize root,ReductionHost host) on cpu
 requires nonzero(host.push_root) {
 usize index=0u64;
 while(index<values.count){host.push_root(host.context,root,word_at(values,index)+offset);index+=1u64;}
}
extern "C" void reduction_prefix_definitions(Span code,Host ir,address definitions) on cpu
 requires nonzero(ir.inspect) {
 usize index=0u64;
 while(index<code.count){
  Results result=results(node_at(code,index,N_INSTRUCTION),ir);Word next=next_result(result);
  while(next.present!=0){
   unsafe_position<u8>(reduction_byte(definitions,next.value))=u8(1);
   next=next_result(result);
  }
  index+=1u64;
 }
}

extern "C" void reduction_kernel(ReductionInput input,ReductionHost host) on cpu requires nonzero(host.ir.inspect) requires nonzero(host.header) requires nonzero(host.clone) requires nonzero(host.rewrite.inspect) requires nonzero(host.push_root) requires nonzero(host.alloc_uniform) requires nonzero(host.release_uniform) requires nonzero(host.open_part) requires nonzero(host.hoist) requires nonzero(host.move_instruction) requires nonzero(host.close_part) requires nonzero(host.value_flow) {
 View source=host.ir.inspect(input.source);
 if(input.identity!=0){
  host.clone(host.context,1u64,source.left,0u64);
  View identity=host.ir.inspect(source.left);
  reduction_copy_roots(host.ir.inspect(identity.right).first,0u64,2u64,host);
 }
 bool project=input.project!=0&&source.first.count!=0u64;
 if(project){
  Node value=node_at(source.first,0u64,22);ReductionHeader base=host.header(host.context);
  RewriteSpan part=host.clone(host.context,2u64,value,base.locals);
  rewrite_offsets(part,host.rewrite,RewriteOffsets(base.registers,base.locals));
  View code=host.ir.inspect(value);
  reduction_copy_roots(host.ir.inspect(code.left).first,base.registers,0u64,host);
  reduction_copy_roots(host.ir.inspect(code.right).first,base.registers,1u64,host);
 }
 ReductionHeader base=host.header(host.context);
 RewriteSpan part=host.clone(host.context,3u64,source.right,base.locals);
 rewrite_offsets(part,host.rewrite,RewriteOffsets(base.registers,base.locals));
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
 usize register_count=host.header(host.context).registers;
 address uniform=host.alloc_uniform(register_count);
 usize partition=1u64;
 while(partition<4u64){reduction_extract_part(partition,uniform,host);partition+=1u64;}
 host.release_uniform(uniform,register_count);
 host.value_flow(host.context);
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
bool pipeline_cross(PipelineAccess left,PipelineAccess right) on cpu {
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
enum StageOrder:u32 { Unknown=0; Unordered=1; Ordered=2; };
StageOrder pipeline_order(Span code,Host host) on cpu requires nonzero(host.inspect) {
 return pipeline_ordered(code,host) ? StageOrder::Ordered : StageOrder::Unordered;
}
bool pipeline_fusible(address previous,address current,inout StageOrder previous_order,
 inout StageOrder current_order,PipelineFusion host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.access) {
 if(previous_order==StageOrder::Ordered){return false;}
 View left_ir=host.ir.inspect(Node(previous,N_STAGE));
 if(host.ir.inspect(left_ir.left).tag!=0){return false;}
 View right_ir=host.ir.inspect(Node(current,N_STAGE));
 if(host.ir.inspect(right_ir.left).tag!=0){return false;}
 PipelineAccess left=host.access(previous);
 PipelineAccess right=host.access(current);
 if(left.domain!=right.domain||right.boundary!=0
    ||!pipeline_same_field(left.prefix,right.prefix)||!pipeline_same_field(left.selection,right.selection)
    ||pipeline_cross(left,right)){return false;}
 if(previous_order==StageOrder::Unknown){
  previous_order=pipeline_order(left_ir.first,host.ir);
  if(previous_order==StageOrder::Ordered){return false;}
 }
 current_order=pipeline_order(right_ir.first,host.ir);
 return current_order==StageOrder::Unordered;
}
usize pipeline_fuse(usize count,u32 record,PipelineFusion host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.at) requires nonzero(host.access)
 requires nonzero(host.swap) requires nonzero(host.truncate) requires nonzero(host.rewrite.inspect)
 requires nonzero(host.layout) requires nonzero(host.origin) requires nonzero(host.retain)
 requires nonzero(host.commit) requires nonzero(host.joined) {
 usize index=0u64;
 usize kept=0u64;
 usize last_join=0u64;
 StageOrder previous_order=StageOrder::Unknown;
 while(index<count){
  bool merged=false;
  StageOrder current_order=StageOrder::Unknown;
  if(kept!=0u64){
   usize target=kept-1u64;
   address previous=host.at(host.context,target);
   address current=host.at(host.context,index);
   if(pipeline_fusible(previous,current,previous_order,current_order,host)){
    host.origin(previous,current);
    FusionLayout layout=host.layout(previous,current);
    RewriteOffsets offsets=layout.offsets;
    rewrite_offsets(layout.code,host.rewrite,offsets);
    host.retain(previous,current);
    host.commit(host.context,target,index,offsets.locals);
    // The fused body consists only of the two bodies already classified as unordered.
    if(record!=0&&last_join!=kept){
     host.joined(host.context,target);
     last_join=kept;
    }
    merged=true;
   }
  }
  if(!merged){
   if(index!=kept){host.swap(host.context,kept,index);}
   kept+=1u64;
   previous_order=current_order;
  }
  index+=1u64;
 }
 host.truncate(host.context,kept);
 return kept;
}
usize pipeline_remove_empty(usize count,PipelineProgram host) on cpu
 requires nonzero(host.fusion.at) requires nonzero(host.fusion.ir.inspect)
 requires nonzero(host.fusion.access) requires nonzero(host.fusion.swap)
 requires nonzero(host.fusion.truncate) requires nonzero(host.removed) requires nonzero(host.boundary) {
 PipelineFusion fusion=host.fusion;
 usize index=0u64;
 usize kept=0u64;
 u32 carry=0;
 while(index<count){
  address owner=fusion.at(fusion.context,index);
  View stage=fusion.ir.inspect(Node(owner,N_STAGE));
  if(stage.first.count==0u64&&fusion.ir.inspect(stage.left).tag==0){
   carry=carry|fusion.access(owner).boundary;
   host.removed(fusion.context,index);
  }else{
   if(carry!=0&&fusion.access(owner).boundary==0){host.boundary(fusion.context,index);}
   carry=0;
   if(index!=kept){fusion.swap(fusion.context,kept,index);}
   kept+=1u64;
  }
  index+=1u64;
 }
 fusion.truncate(fusion.context,kept);
 return kept;
}
extern "C" void optimize_program(usize count,PipelineProgram host) on cpu
 requires nonzero(host.fusion.ir.inspect) requires nonzero(host.fusion.at)
 requires nonzero(host.fusion.access) requires nonzero(host.fusion.swap) requires nonzero(host.fusion.truncate)
 requires nonzero(host.fusion.rewrite.inspect) requires nonzero(host.fusion.layout)
 requires nonzero(host.fusion.origin) requires nonzero(host.fusion.retain) requires nonzero(host.fusion.commit)
 requires nonzero(host.fusion.joined) requires nonzero(host.batch) requires nonzero(host.removed)
 requires nonzero(host.boundary) requires nonzero(host.origin) {
 usize fused=pipeline_fuse(count,0,host.fusion);
 if(host.batch(host.fusion.context,0)==0){return;}
 usize retained=pipeline_remove_empty(fused,host);
 if(retained!=fused){
  host.origin(host.fusion.context);
  pipeline_fuse(retained,1,host.fusion);
  host.batch(host.fusion.context,1);
 }
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
 View stage=host.ir.inspect(Node(host.stage(host.context),N_STAGE));
 u32 operation=host.ir.inspect(stage.left).tag;
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
   u32 preserve=host.words(host.context);
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
 requires nonzero(host.scalar_bytes) requires nonzero(rewrite.inspect) {
 RewriteView root=rewrite.inspect(RewriteNode(instruction,N_INSTRUCTION));
 if(root.tag!=OP_TILE_PUBLISH&&root.tag!=OP_TILE_COLLECTIVE){return PipelineRootEdit(PipelineRootAction::Keep,0u64);}
 usize count=root.first.count;
 if(root.tag==OP_TILE_PUBLISH){
  usize tile=unsafe_position<usize>(root.a);
  usize read=0u64;count=0u64;
  while(read<root.first.count){
   u32 word=unsafe_position<u32>(rewrite_word_at(root.second,read));
   usize reg=unsafe_position<usize>(rewrite_word_at(root.first,read));
   u32 bytes=host.scalar_bytes(host.registers,reg);u32 part=0;bool keep=false;
   while(part<(bytes+3)/4){
    if(tile_word_read(host.words,TileKey(1u64,tile,word+part*4))){keep=true;break;}
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
const u64 PIPELINE_BINDINGS_PER_WORD=32u64;
address pipeline_flag_word(address flags,usize word) on cpu {
 return unsafe_address(address_bits(flags)+u64(word)*size_of<u64>());
}
u64 pipeline_flag_shift(usize binding) on cpu {
 return (binding%PIPELINE_BINDINGS_PER_WORD)*2u64;
}
void pipeline_mark(address flags,usize binding,u8 mode) on cpu {
 address at=pipeline_flag_word(flags,binding/PIPELINE_BINDINGS_PER_WORD);
 unsafe_position<u64>(at)=unsafe_position<u64>(at)|(u64(mode)<<pipeline_flag_shift(binding));
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
void pipeline_tile_access(Span tiles,address flags,Host ir,Span words) on cpu
 requires nonzero(ir.inspect) {
 usize index=0u64;
 while(index<tiles.count){
  View tile=ir.inspect(node_at(tiles,index,N_TILE));
  if(tile.tag==0){
   usize component=0u64;bool needed=false;
   while(component<tile.first.count&&!needed){
    View field=ir.inspect(node_at(tile.first,component,N_COMPONENT));usize part=0u64;
    while(part<(field.b+3u64)/4u64){
     if(tile_word_read(words,TileKey(0u64,tile.a,u32(tile.b+field.a+part*4u64)))){needed=true;break;}
     part+=1u64;
    }
    component+=1u64;
   }
   if(needed){pipeline_mark(flags,tile.a,u8(1));}
  }
  index+=1u64;
 }
}
extern "C" usize optimize_bindings(PipelineMap input,Host ir,Span words) on cpu
 requires nonzero(ir.inspect) requires nonzero(input.append) {
 pipeline_code_access(input.code,input.flags,ir);pipeline_tile_access(input.tiles,input.flags,ir,words);
 if(input.prefix.present!=0){pipeline_mark(input.flags,input.prefix.binding,u8(1));}
 if(input.selection.present!=0){pipeline_mark(input.flags,input.selection.binding,u8(1));}
 usize word=0u64;
 while(word<input.flag_words){
  u64 flags=unsafe_position<u64>(pipeline_flag_word(input.flags,word));
  usize binding=word*PIPELINE_BINDINGS_PER_WORD;
  while(flags!=0u64){
   if((flags&1u64)!=0u64){input.append(input.output,binding,false);}
   if((flags&2u64)!=0u64){input.append(input.output,binding,true);}
   flags=flags>>2;
   binding+=1u64;
  }
  word+=1u64;
 }
 usize cross=0u64;usize at=0u64;
 while(at<input.cross_count){
  usize binding=unsafe_position<usize>(unsafe_address(address_bits(input.cross)+u64(at)*size_of<usize>()));
  u64 flags=unsafe_position<u64>(pipeline_flag_word(input.flags,binding/PIPELINE_BINDINGS_PER_WORD));
  if(((flags>>pipeline_flag_shift(binding))&3u64)!=0u64){
   unsafe_position<usize>(unsafe_address(address_bits(input.cross)+u64(cross)*size_of<usize>()))=binding;
   cross+=1u64;
  }
  at+=1u64;
 }
 return cross;
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
struct RewriteMap {
 address context;
 cfn<usize,address,usize> reg;
 cfn<usize,address,usize> slot;
};

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
extern "C" usize offset_register(address context,usize value) on cpu {
 return value+unsafe_position<RewriteOffsets>(context).registers;
}
extern "C" usize offset_local(address context,usize value) on cpu {
 return value+unsafe_position<RewriteOffsets>(context).locals;
}
void rewrite_offsets(RewriteSpan code,RewriteHost host,RewriteOffsets offsets) on cpu requires nonzero(host.inspect) {
 rewrite_block(code,host,RewriteMap(address_of(offsets),c_function(offset_register),c_function(offset_local)));
}

struct CompactNumbers { address registers; address locals; };
extern "C" usize compact_register_number(address context,usize value) on cpu {
 return compact_index(unsafe_position<CompactNumbers>(context).registers,value)-1u64;
}
extern "C" usize compact_local_number(address context,usize value) on cpu {
 return compact_index(unsafe_position<CompactNumbers>(context).locals,value)-1u64;
}
void rewrite_compact(RewriteSpan code,RewriteHost host,address registers,address locals) on cpu requires nonzero(host.inspect) {
 CompactNumbers numbers=CompactNumbers(registers,locals);
 rewrite_block(code,host,RewriteMap(address_of(numbers),c_function(compact_register_number),c_function(compact_local_number)));
}

extern "C" usize bound_register(address registers,usize value) on cpu {
 usize bound=compact_index(registers,value);
 return bound==0u64 ? value : bound-1u64;
}
extern "C" usize binding_local(address context,usize value) on cpu { return value; }
extern "C" void bind_reduction_arguments(RewriteSpan code,RewriteSpan result,Span right,Span projected,
 address registers,RewriteHost host) on cpu requires nonzero(host.inspect) {
 usize count=right.count<projected.count ? right.count : projected.count;
 usize index=0u64;
 while(index<count){
  // The last pair for a right-hand input owns that binding. Zero is unbound.
  address slot=unsafe_address(address_bits(registers)+word_at(right,index)*size_of<usize>());
  unsafe_position<usize>(slot)=word_at(projected,index)+1u64;
  index+=1u64;
 }
 rewrite_block(code,host,RewriteMap(registers,c_function(bound_register),c_function(binding_local)));
 rewrite_words(result,registers,c_function(bound_register));
}

extern "C" void rewrite_operands(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) {rewrite_inputs(host.inspect(source),host,mapping);}

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
extern "C" void cleanup_success_facts(Span code,usize slot,address facts,CleanupFactsHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.error) requires nonzero(host.zero) {
 usize index=0u64;
 while(index<code.count){
  View inst=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(inst.tag==OP_LOAD_LOCAL && inst.b==slot){
   host.zero(facts,inst.a);
  }else if(inst.tag==OP_INVOKE){
   CleanupError error=host.error(inst.left.data);
   if(error.present!=0 && error.slot==slot){host.zero(facts,error.reg);}
  }else if(inst.tag==OP_SCOPE || inst.tag==OP_CLEANUP){
   cleanup_success_facts(inst.first,slot,facts,host);
  }else if(inst.tag==OP_BRANCH || inst.tag==OP_LOOP_VALUE){
   cleanup_success_facts(inst.first,slot,facts,host);
   cleanup_success_facts(inst.second,slot,facts,host);
  }
  index+=1u64;
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
 if(value.tag==OP_MATH && value.b!=0u64 && value.d!=0u64) {
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
 Node operation,Span code,Span registers,RegisterHost host,CodeRequirements initial) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 u32 remaining=float_operation_remaining(operation,initial.remaining_floats,host);
 return code_requirements<Queries>(code,registers,host,
  CodeRequirements(initial.subgroup,initial.atomic64,remaining));
}

extern "C" u32 float_preserve(Node source,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 View stage=host.ir.inspect(source);
 let facts=stage_code_requirements<0>(stage.left,stage.first,stage.second,host,CodeRequirements(0,0,requested));
 return requested^facts.remaining_floats;
}

bool tile_phase_step(u32 tag) on cpu {
 return tag==OP_TILE_PUBLISH || tag==OP_TILE_COLLECTIVE || tag==OP_TILE_BARRIER;
}

void private_spans(Span code,Host host,address context,
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

struct TileGroupBuffer { address data; usize count; usize capacity; };

address tile_group_at(address groups,usize index) on cpu {
 return unsafe_address(address_bits(groups)+u64(index)*size_of<TileGroup>());
}

void add_tile_group(inout TileGroupBuffer groups,TileGroup next,StorageOutput output) on cpu
 requires nonzero(output.reserve_groups) {
 TileGroup current=next;
 usize kept=0u64;
 usize read=0u64;
 while(read<groups.count) {
  TileGroup prior=unsafe_position<TileGroup>(tile_group_at(groups.data,read));
  if(prior.storage==current.storage && prior.owner==current.owner
      && max(prior.first,current.first)<min(prior.end,current.end)) {
   current.first=min(prior.first,current.first);
   current.end=max(prior.end,current.end);
  }else{
   unsafe_position<TileGroup>(tile_group_at(groups.data,kept))=prior;
   kept+=1u64;
  }
  read+=1u64;
 }
 if(kept==groups.capacity) {
  groups.capacity=max(8u64,groups.capacity*2u64);
  groups.data=output.reserve_groups(output.group_storage,groups.capacity);
 }
 unsafe_position<TileGroup>(tile_group_at(groups.data,kept))=current;
 groups.count=kept+1u64;
}

void shared_parts(usize tile,u32 byte,u32 bytes,i64 phase,StorageOutput output) on cpu
 requires nonzero(output.read) {
 u32 part=0;
 while(part<(bytes+3)/4) {
  output.read(output.context,tile,byte+part*4,phase);
  part+=1;
 }
}

u32 storage_visit(Span code,Span tiles,StorageHost host,i64 first_phase,bool root,
 inout TileGroupBuffer groups,StorageOutput output) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes)
 requires nonzero(output.read) requires nonzero(output.reserve_groups) {
 i64 phase=first_phase;
 u32 preserve=0;
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
      TileKey origin=tile_key_for_view(tile,source.a,0);
      u32 first=origin.byte/4;
      u32 count=u32((u64(tile.c)+u64(origin.byte%4)+3u64)/4u64);
      add_tile_group(groups,TileGroup(origin.storage,origin.owner,first,first+count),output);
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
   preserve|=storage_visit(value.first,tiles,host,phase,false,groups,output);
   preserve|=storage_visit(value.second,tiles,host,phase,false,groups,output);
  }else if(value.tag==OP_APPEND || value.tag==OP_SUBGROUP) {
   preserve=1;
  }
  if(root && phase>=i64(0)) {
   if(value.tag==OP_TILE_PUBLISH) {
    unsafe_position<i64>(node_at(output.publication_phases,value.a,0).data)=phase;
   }
   if(tile_phase_step(value.tag)) {
    phase+=i64(1);
   }
  }
  index+=1u64;
 }
 return preserve;
}

extern "C" StorageFacts storage_facts(Span code,Span tiles,StorageHost host,i64 phase,StorageOutput output) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes)
 requires nonzero(output.read) requires nonzero(output.reserve_groups) {
 TileGroupBuffer groups=TileGroupBuffer(address(),0u64,0u64);
 u32 preserve=storage_visit(code,tiles,host,phase,true,groups,output);
 return StorageFacts(preserve,groups.count);
}

bool tile_key_less(TileKey a, TileKey b) on cpu {
    if (a.storage != b.storage) { return a.storage < b.storage; }
    if (a.owner != b.owner) { return a.owner < b.owner; }
    return a.byte < b.byte;
}

address tile_key_row(Span rows, TileKey key) on cpu {
    usize low = 0u64;
    usize high = rows.count;
    while (low < high) {
        usize middle = low + (high - low) / 2u64;
        address row = node_at(rows, middle, 0).data;
        if (tile_key_less(unsafe_position<TileKey>(row), key)) {
            low = middle + 1u64;
        } else {
            high = middle;
        }
    }
    if (low == rows.count) { return address(); }
    address row = node_at(rows, low, 0).data;
    return tile_key_less(key, unsafe_position<TileKey>(row)) ? address() : row;
}

bool tile_word_read(Span uses, TileKey key) on cpu {
    u32 bit = u32(1) << (key.byte % 4);
    address row = tile_key_row(uses, TileKey(key.storage, key.owner, key.byte & 0xfffffffc));
    return row != address() && (unsafe_position<TileUse>(row).read_mask & bit) != 0;
}

TileInputFragment tile_input_fragment(Span fragments, usize index) on cpu {
 return unsafe_position<TileInputFragment>(node_at(fragments,index,0).data);
}

extern "C" void tile_input_words(Span tiles,Host host,Span uses,address output,
 cfn<void,address,TileInputFragment> emit) on cpu
 requires nonzero(host.inspect) requires nonzero(emit) {
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
     if(tile_word_read(uses,TileKey(0u64,tile.a,byte))) {
      BuiltinScalar unit = bytes == 1 ? BuiltinScalar::U8 :
          (bytes == 2 ? BuiltinScalar::U16 : BuiltinScalar::U32);
      emit(output,TileInputFragment(tile.a,byte,unit));
     }
     part+=1;
    }
    component+=1u64;
   }
  }
  index+=1u64;
 }
}

u32 has_append(Span code,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_APPEND){return 1;}
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(has_append(value.first,host)!=0){return 1;}
   if(has_append(value.second,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
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

extern "C" ShaderCodeRequirements shader_code_requirements(
 Node operation,Span code,Span registers,u32 requested,bool subgroups,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 CodeRequirements initial=CodeRequirements(0,0,requested);
 CodeRequirements facts=subgroups
  ? stage_code_requirements<5>(operation,code,registers,host,initial)
  : stage_code_requirements<4>(operation,code,registers,host,initial);
 u32 used=facts.subgroup;
 ShaderSubgroups uses=ShaderSubgroups(subgroup_flags(used),used&SUBGROUP_BUILTINS,
  u32((used&SUBGROUP_APPEND)!=0));
 return ShaderCodeRequirements(uses,facts.atomic64,requested^facts.remaining_floats);
}
extern "C" ProgramCodeRequirements program_code_requirements(Span stages,u32 requested,RegisterHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 CodeRequirements facts=CodeRequirements(0,0,requested);
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.ir.inspect(node_at(stages,index,N_STAGE));
  facts=stage_code_requirements<7>(stage.left,stage.first,stage.second,host,facts);
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

bool ray_fields_used(Span result,usize begin,usize end,RayHost host) on cpu
 requires nonzero(host.fields) requires nonzero(host.contains) {
 Span fields=host.fields(result,begin,end);
 usize field=0u64;
 while(field<fields.count) {
  if(host.contains(host.used,word_at(fields,field))!=0){return true;}
  field+=1u64;
 }
 return false;
}

extern "C" u32 shader_ray_usage(Span code,Span queries,bool closest,RayHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) requires nonzero(host.contains)
 requires nonzero(host.fields) {
 if(closest){collect_ray_operands(code,host);}
 u32 used=0;
 usize ordinal=0u64;
 while(ordinal<queries.count) {
  address row=node_at(queries,ordinal,N_INSTRUCTION).data;
  RayQuery query=unsafe_position<RayQuery>(row);
  query.ordinal=u32(ordinal);
  if(query.fields.count==39u64) {
   if(ray_fields_used(query.fields,6u64,15u64,host)){query.used|=RAY_POSITIONS;}
   if(ray_fields_used(query.fields,15u64,27u64,host)){query.used|=RAY_OBJECT_TO_WORLD;}
   if(ray_fields_used(query.fields,27u64,39u64,host)){query.used|=RAY_WORLD_TO_OBJECT;}
  }
  used|=query.used;
  unsafe_position<RayQuery>(row)=query;
  ordinal+=1u64;
 }
 return used & RAY_POSITIONS;
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
    if(ray_fields_used(result,range.a,range.b,host) && visit(context,group,query)==0){return 0;}
    group+=1u64;
   }
  }
  query+=1u64;
 }
 return 1;
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
 return has_append(code,host);
}
extern "C" u32 program_maximal_reconvergence(Span stages,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<stages.count) {
  View stage=host.inspect(node_at(stages,index,N_STAGE));
  if(has_append(stage.first,host)!=0){return 1;}
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

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";

const u32 READ_HEAP_EXCEPTIONS=4;
const u32 READ_HEAP_ZERO=8;
struct ReadStorage {address data;usize capacity;};
struct ReadList {ReadStorage storage;usize count;};
enum ReadWorkKind:u32 {Expand=0;Emit=1;Demand=2;};
struct ReadWork {usize id;ReadWorkKind kind;};
struct ReadPrivate {usize id;usize first;usize end;};
struct ReadScratch {
 ReadStorage producers;
 ReadList work;
 ReadList shared;
 ReadList locals;
 usize pending;
};

void reserve_reads(inout ReadStorage storage,usize needed,usize stride,u32 flags) on cpu {
 if(needed<=storage.capacity){return;}
 usize grown=storage.capacity==0u64 ? 4u64 : storage.capacity*2u64;
 usize capacity=grown<needed ? needed : grown;
 handle heap=GetProcessHeap();
 if(storage.capacity==0u64){
  storage.data=HeapAlloc(heap,flags|READ_HEAP_EXCEPTIONS,capacity*stride);
 }else{
  storage.data=HeapReAlloc(heap,flags|READ_HEAP_EXCEPTIONS,storage.data,capacity*stride);
 }
 storage.capacity=capacity;
}

address read_row<T>(ReadStorage storage,usize index) on cpu {
 return unsafe_address(address_bits(storage.data)+u64(index)*size_of<T>());
}

void push_read<T>(inout ReadList rows,T value) on cpu {
 reserve_reads(rows.storage,rows.count+1u64,size_of<T>(),0);
 unsafe_position<T>(read_row<T>(rows.storage,rows.count))=value;
 rows.count+=1u64;
}

void release_reads(ReadStorage storage) on cpu {
 if(storage.capacity!=0u64){HeapFree(GetProcessHeap(),0,storage.data);}
}

usize read_producer(ReadStorage producers,usize reg) on cpu {
 return unsafe_position<usize>(read_row<usize>(producers,reg));
}

struct ReadOperands {address scratch;bool deferred;};
extern "C" u32 read_operand(address context,usize reg) on cpu {
 ReadOperands operands=unsafe_position<ReadOperands>(context);
 if(operands.deferred){
  push_read<ReadWork>(unsafe_position<ReadScratch>(operands.scratch).work,ReadWork(reg,ReadWorkKind::Demand));
 }else{
  usize producer=read_producer(unsafe_position<ReadScratch>(operands.scratch).producers,reg);
  if(producer!=0u64){
   push_read<ReadWork>(unsafe_position<ReadScratch>(operands.scratch).work,ReadWork(producer-1u64,ReadWorkKind::Expand));
  }
 }
 return 1;
}

void append_read_operands(View inst,address scratch,bool deferred,Host host) on cpu
 requires nonzero(host.inspect) {
 ReadOperands context=ReadOperands(scratch,deferred);
 usize first=unsafe_position<ReadScratch>(scratch).work.count;
 visit_instruction(inst,host,address_of(context),c_function(read_operand));
 ReadList work=unsafe_position<ReadScratch>(scratch).work;
 usize end=work.count;
 while(first+1u64<end){
  end-=1u64;
  address left=read_row<ReadWork>(work.storage,first);
  address right=read_row<ReadWork>(work.storage,end);
  ReadWork previous=unsafe_position<ReadWork>(left);
  unsafe_position<ReadWork>(left)=unsafe_position<ReadWork>(right);
  unsafe_position<ReadWork>(right)=previous;
  first+=1u64;
 }
}

void read_expand(address pending,address scratch,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) {
 Node node=host.queued(pending,id);
 if(address_bits(node.data)==0u64){return;}
 push_read<ReadWork>(unsafe_position<ReadScratch>(scratch).work,ReadWork(id,ReadWorkKind::Emit));
 append_read_operands(host.ir.inspect(node),scratch,false,host.ir);
}

void read_emit(address pending,address scratch,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.emit) {
 Node node=host.queued(pending,id);
 if(address_bits(node.data)==0u64){return;}
 Results outputs=results(node,host.ir);
 ReadStorage producers=unsafe_position<ReadScratch>(scratch).producers;
 Word reg=next_result(outputs);
 while(reg.present!=0){
  unsafe_position<usize>(read_row<usize>(producers,reg.value))=0u64;
  reg=next_result(outputs);
 }
 host.emit(pending,id);
}

void drain_reads(address pending,address scratch,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.emit) {
 while(unsafe_position<ReadScratch>(scratch).work.count!=0u64){
  unsafe_position<ReadScratch>(scratch).work.count-=1u64;
  ReadList work=unsafe_position<ReadScratch>(scratch).work;
  ReadWork task=unsafe_position<ReadWork>(read_row<ReadWork>(work.storage,work.count));
  if(task.kind==ReadWorkKind::Emit){read_emit(pending,scratch,task.id,host);}
  else if(task.kind==ReadWorkKind::Demand){
   // The preceding operand can retire this definition before it is needed.
   usize producer=read_producer(unsafe_position<ReadScratch>(scratch).producers,task.id);
   if(producer!=0u64){read_expand(pending,scratch,producer-1u64,host);}
  }else{read_expand(pending,scratch,task.id,host);}
 }
}

void read_demand(address pending,address scratch,usize id,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.emit) {
 read_expand(pending,scratch,id,host);
 drain_reads(pending,scratch,host);
}

void queue_read(address input,address pending,address scratch,View inst,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.store) {
 usize id=unsafe_position<ReadScratch>(scratch).pending;
 Results outputs=view_results(inst,host.ir);
 ReadStorage producers=unsafe_position<ReadScratch>(scratch).producers;
 Word reg=next_result(outputs);
 while(reg.present!=0){
  unsafe_position<usize>(read_row<usize>(producers,reg.value))=id+1u64;
  reg=next_result(outputs);
 }
 host.store(input,pending,true);
 unsafe_position<ReadScratch>(scratch).pending=id+1u64;
}

void finish_shared_reads(address pending,address scratch,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.emit) {
 ReadList shared=unsafe_position<ReadScratch>(scratch).shared;
 usize index=0u64;
 while(index<shared.count){
  read_demand(pending,scratch,unsafe_position<usize>(read_row<usize>(shared.storage,index)),host);
  index+=1u64;
 }
 unsafe_position<ReadScratch>(scratch).shared.count=0u64;
}

void finish_overlapping_reads(address pending,address scratch,usize first,usize end,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued) requires nonzero(host.emit) {
 ReadList locals=unsafe_position<ReadScratch>(scratch).locals;
 usize kept=0u64;
 usize index=0u64;
 while(index<locals.count){
  ReadPrivate row=unsafe_position<ReadPrivate>(read_row<ReadPrivate>(locals.storage,index));
  if(address_bits(host.queued(pending,row.id).data)!=0u64){
   if(row.first<end&&first<row.end){read_demand(pending,scratch,row.id,host);}
   else{
    unsafe_position<ReadPrivate>(read_row<ReadPrivate>(locals.storage,kept))=row;
    kept+=1u64;
   }
  }
  index+=1u64;
 }
 unsafe_position<ReadScratch>(scratch).locals.count=kept;
}

void read_finish(address pending,address scratch,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.queued)
 requires nonzero(host.emit) requires nonzero(host.clear) {
 usize count=unsafe_position<ReadScratch>(scratch).pending;
 usize index=0u64;
 while(index<count){
  read_demand(pending,scratch,index,host);
  index+=1u64;
 }
 host.clear(pending);
 unsafe_position<ReadScratch>(scratch).pending=0u64;
 unsafe_position<ReadScratch>(scratch).shared.count=0u64;
 unsafe_position<ReadScratch>(scratch).locals.count=0u64;
}

bool binding_written(Span writes,usize binding) on cpu {
 usize index=0u64;
 while(index<writes.count){
  if(word_at(writes,index)==binding){return true;}
  index+=1u64;
 }
 return false;
}

extern "C" void place_read_block(address input,address pending,address scratch,Span writes,usize tail,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.math_outgoing)
 requires nonzero(host.next) requires nonzero(host.sample) requires nonzero(host.children)
 requires nonzero(host.store) requires nonzero(host.queued) requires nonzero(host.emit) requires nonzero(host.clear) {
 while(true){
  Node node=host.next(input,tail);
  if(address_bits(node.data)==0u64){break;}
  View inst=host.ir.inspect(node);
  u32 tag=inst.tag;
  if(tag==OP_TEXTURE_SAMPLE){
   usize registers=host.sample(input);
   reserve_reads(unsafe_position<ReadScratch>(scratch).producers,registers,size_of<usize>(),READ_HEAP_ZERO);
   continue;
  }
  if(tag==OP_CONSTANT||tag==OP_INDEX_VALUE||tag==OP_UNARY||tag==OP_BINARY||tag==OP_INTEGER_DIVIDE||
     tag==OP_CAST||tag==OP_SELECT_VALUE||(tag==OP_MATH&&host.math_outgoing(u32(inst.b))==0u64)){
   queue_read(input,pending,scratch,inst,host);
  }else if(tag==OP_LOAD_LOCAL||tag==OP_LOAD_LOCAL_INDEXED){
   usize end=inst.b+1u64;
   if(tag==OP_LOAD_LOCAL_INDEXED){end=inst.b+inst.c;}
   usize id=unsafe_position<ReadScratch>(scratch).pending;
   push_read<ReadPrivate>(unsafe_position<ReadScratch>(scratch).locals,ReadPrivate(id,inst.b,end));
   queue_read(input,pending,scratch,inst,host);
  }else if(tag==OP_VIEW_LOAD||(tag==OP_LOAD&&!binding_written(writes,inst.b))){
   usize id=unsafe_position<ReadScratch>(scratch).pending;
   push_read<usize>(unsafe_position<ReadScratch>(scratch).shared,id);
   queue_read(input,pending,scratch,inst,host);
  }else{
   if(tag==OP_STORE_LOCAL||tag==OP_STORE_LOCAL_INDEXED){
    finish_shared_reads(pending,scratch,host);
    usize end=inst.a+1u64;
    if(tag==OP_STORE_LOCAL_INDEXED){end=inst.a+inst.b;}
    finish_overlapping_reads(pending,scratch,inst.a,end,host);
    append_read_operands(inst,scratch,true,host.ir);
    drain_reads(pending,scratch,host);
   }else if(tag==OP_STORE||tag==OP_STORE_INDEXED||tag==OP_LOAD_INDEXED||tag==OP_LOAD){
    finish_shared_reads(pending,scratch,host);
    append_read_operands(inst,scratch,true,host.ir);
    drain_reads(pending,scratch,host);
   }else if(tag!=OP_UNIFORM){
    // A child borrows the emptied scheduler after the parent finishes every producer.
    read_finish(pending,scratch,host);
    if(tag==OP_BRANCH||tag==OP_LOOP_VALUE||tag==OP_SCOPE){
     host.children(input,scratch);
    }
   }
   host.store(input,pending,false);
  }
 }
 read_finish(pending,scratch,host);
}

extern "C" void place_reads(address input,address pending,Span writes,ReadPhases phases,ReadHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.math_outgoing)
 requires nonzero(host.next) requires nonzero(host.sample) requires nonzero(host.children)
 requires nonzero(host.store) requires nonzero(host.queued) requires nonzero(host.emit)
 requires nonzero(host.clear) requires nonzero(host.written) {
 ReadStorage empty=ReadStorage(address(),0u64);
 ReadList list=ReadList(empty,0u64);
 ReadScratch scratch=ReadScratch(empty,list,list,list,0u64);
 // Every operand is an index in the original register table. Texture lowering
 // is its only producer during placement and publishes each new length.
 reserve_reads(scratch.producers,phases.registers,size_of<usize>(),READ_HEAP_ZERO);
 usize phase=0u64;
 while(phase<phases.ends.count){
  address end=unsafe_address(address_bits(phases.ends.data)+phase*phases.ends.stride);
  usize tail=phases.instructions-unsafe_position<usize>(end);
  // All pending definitions are emitted before the next phase starts.
  place_read_block(input,pending,address_of(scratch),writes,tail,host);
  unsafe_position<usize>(end)=host.written(pending);
  phase+=1u64;
 }
 place_read_block(input,pending,address_of(scratch),writes,0u64,host);
 release_reads(scratch.producers);
 release_reads(scratch.work.storage);
 release_reads(scratch.shared.storage);
 release_reads(scratch.locals.storage);
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
 requires nonzero(host.move_block) requires nonzero(host.branch) requires nonzero(host.size)
 requires nonzero(host.keep_slot) {
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
 usize field=0u64;
 while(field<input.fields){
  host.keep_slot(context,carry+field,true);
  field+=1u64;
 }
 if(history){host.keep_slot(context,flag_slot,true);}
 usize count_slot=0u64;
 if(counted){
  count_slot=host.local(context,CpuRangeType::U32);
  host.keep_slot(context,count_slot,true);
 }
 usize private_index=host.reg(context,CpuRangeType::U32,0u64);
 if(apply || finish){host.put(context,body,CpuRangeOp::Constant,private_index,0u64,0u64,0);}
 usize length_slot=0u64;
 if(finish) {
  length_slot=host.local(context,CpuRangeType::U32);
  host.keep_slot(context,length_slot,true);
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
   usize initial=0u64;
   while(initial<input.initial.count){
    host.keep_slot(context,word_at(input.initial,initial),false);
    initial+=1u64;
   }
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
 return CpuRangeBody(initialization_end,loop_end,carry,u32(apply && has_head));
}



struct SdkResourceEmission { address state; address block; };

u8 sdk_symbol_byte(Span text, usize index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(text.data) + index));
}

bool sdk_prefix<const N:u32>(Span text, read array<u8,N> prefix) on cpu {
    if (text.count < u64(N)) { return false; }
    for (index in 0..N) {
        if (sdk_symbol_byte(text, u64(index)) != prefix[index]) { return false; }
    }
    return true;
}

// Only the compiler-produced decimal resource names enter this pass.
Word sdk_index<const N:u32>(Span text, read array<u8,N> prefix) on cpu {
    if (text.count <= u64(N) || !sdk_prefix(text, prefix)) { return absent(); }
    usize index = u64(N);
    if (text.count > index + 1u64 && sdk_symbol_byte(text,index) == u8(48)) { return absent(); }
    usize value = 0u64;
    while (index < text.count) {
        u8 digit = sdk_symbol_byte(text, index);
        if (digit < u8(48) || digit > u8(57)) { return absent(); }
        u64 next = u64(digit - u8(48));
        if (value > (18446744073709551615u64 - next) / 10u64) { return absent(); }
        value = value * 10u64 + next;
        index += 1u64;
    }
    return found(value);
}

extern "C" ApplicationResourceSlot application_resource_slot(Span library,Span symbol) on cpu {
    if(library.count!=19u64 || !sdk_prefix(library,"@blink.application@")){
        return ApplicationResourceSlot(ApplicationResourceKind::Other,0u64);
    }
    Word index=sdk_index(symbol,"blink_allocate_");
    if(index.present!=0){return ApplicationResourceSlot(ApplicationResourceKind::Allocate,index.value);}
    index=sdk_index(symbol,"blink_run_");
    if(index.present!=0){return ApplicationResourceSlot(ApplicationResourceKind::Run,index.value);}
    index=sdk_index(symbol,"blink_complete_");
    if(index.present!=0){return ApplicationResourceSlot(ApplicationResourceKind::Complete,index.value);}
    if(symbol.count==16u64 && sdk_prefix(symbol,"blink_task_start")){
        return ApplicationResourceSlot(ApplicationResourceKind::TaskStart,0u64);
    }
    return ApplicationResourceSlot(ApplicationResourceKind::Other,0u64);
}

// Numbered call producers pass one scalar per argument after the Frame.
// The source Frame argument itself remains in its original two-word form.
usize sdk_operand(Span arguments, usize index, Host ir) on cpu requires nonzero(ir.inspect) {
    return word_at(ir.inspect(node_at(arguments, index, N_ARGUMENT)).first, 0u64);
}

usize sdk_emit(SdkResourceEmission out, SdkResourceHost host, SdkResourceOp op,
    u32 scalar, u64 a, usize b, usize c) on cpu requires nonzero(host.emit) {
    usize dst = unsafe_position<SdkResourceState>(out.state).slots.register;
    unsafe_position<SdkResourceState>(out.state).slots.register = dst + 1u64;
    host.emit(out.state, out.block, op, scalar, dst, a, b, c);
    return dst;
}

usize sdk_constant(SdkResourceEmission out, SdkResourceHost host, u32 scalar, u64 bits) on cpu
    requires nonzero(host.emit) {
    return sdk_emit(out, host, SdkResourceOp::Constant, scalar, bits, 0u64, 0u64);
}

usize sdk_binary(SdkResourceEmission out, SdkResourceHost host, SdkResourceOp op,
    u32 scalar, usize left, usize right) on cpu requires nonzero(host.emit) {
    return sdk_emit(out, host, op, scalar, left, right, 0u64);
}

SdkArgument sdk_packet(SdkResourceEmission out, SdkArgumentKind kind, usize count, usize words) on cpu {
    usize base = unsafe_position<SdkResourceState>(out.state).slots.local;
    unsafe_position<SdkResourceState>(out.state).slots.local = base + count * words;
    return SdkArgument(kind, base, count);
}

void sdk_store(SdkResourceEmission out, SdkResourceHost host, usize slot, usize reg) on cpu
    requires nonzero(host.emit) {
    host.emit(out.state, out.block, SdkResourceOp::Store, 0, slot, reg, 0u64, 0u64);
}

usize sdk_bits(SdkResourceEmission out, SdkResourceHost host, u32 scalar, usize value) on cpu
    requires nonzero(host.emit) {
    u32 bits_type = scalar;
    usize bits = value;
    if (scalar == u32(BuiltinScalar::F32) || scalar == u32(BuiltinScalar::F64)) {
        bits_type = scalar == u32(BuiltinScalar::F32) ? u32(BuiltinScalar::U32) : u32(BuiltinScalar::U64);
        bits = sdk_emit(out, host, SdkResourceOp::Bitcast, bits_type, bits, 0u64, 0u64);
    }
    if (bits_type != u32(BuiltinScalar::U64)) {
        bits = sdk_emit(out, host, SdkResourceOp::Cast, u32(BuiltinScalar::U64), bits, 0u64, 0u64);
    }
    return bits;
}

void sdk_allocate(SdkResourceEmission out, SdkResourceHost host, usize index) on cpu
    requires nonzero(host.emit) requires nonzero(host.call) {
    usize value = sdk_constant(out, host, u32(BuiltinScalar::U32), index);
    let arguments = array<SdkArgument,3>(
        SdkArgument(SdkArgumentKind::Original, 0u64, 0u64),
        SdkArgument(SdkArgumentKind::U32, value, 0u64),
        SdkArgument(SdkArgumentKind::Original, 1u64, 0u64));
    host.call(out.state, out.block, SdkResourceCall::Allocate, address_of(arguments[0]), 3u64);
}

void sdk_run(SdkResourceEmission out, SdkResourceHost host, SdkResourceLayout layout,
    Span source, usize index) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.emit) requires nonzero(host.call) {
    usize at = 1u64;
    SdkArgument data = sdk_packet(out, SdkArgumentKind::Datasets, layout.datasets, 2u64);
    usize dataset_slot = 0u64;
    while (dataset_slot < layout.datasets * 2u64) {
        sdk_store(out, host, data.value + dataset_slot, sdk_operand(source, at, host.ir));
        at += 1u64;
        dataset_slot += 1u64;
    }

    // The producer's scalar argument order is the original uniform layout order.
    usize words_base = unsafe_position<SdkResourceState>(out.state).slots.local;
    usize words = 0u64;
    usize uniform = 0u64;
    while (uniform < layout.uniforms.count) {
        Span components = host.ir.inspect(node_at(layout.uniforms, uniform, N_UNIFORM)).first;
        usize field = 0u64;
        while (field < components.count) {
            u32 scalar = u32(host.ir.inspect(node_at(components, field, N_COMPONENT)).c);
            usize kind = sdk_constant(out, host, u32(BuiltinScalar::U32), u64(scalar));
            usize bits = sdk_bits(out, host, scalar, sdk_operand(source, at, host.ir));
            at += 1u64;
            sdk_store(out, host, words_base + words, kind);
            sdk_store(out, host, words_base + words + 1u64, bits);
            words += 2u64;
            field += 1u64;
        }
        uniform += 1u64;
    }
    SdkArgument values = sdk_packet(out, SdkArgumentKind::Uniforms, words / 2u64, 2u64);
    SdkArgument textures = sdk_packet(out, SdkArgumentKind::Textures, layout.textures, 1u64);
    usize texture_slot = 0u64;
    while (texture_slot < layout.textures) {
        sdk_store(out, host, textures.value + texture_slot, sdk_operand(source, at, host.ir));
        at += 1u64;
        texture_slot += 1u64;
    }

    usize program_id = sdk_constant(out, host, u32(BuiltinScalar::U32), index);
    let arguments = array<SdkArgument,5>(
        SdkArgument(SdkArgumentKind::Original, 0u64, 0u64),
        SdkArgument(SdkArgumentKind::U32, program_id, 0u64), data, values, textures);
    host.call(out.state, out.block, SdkResourceCall::Run, address_of(arguments[0]), 5u64);
}

struct SdkLimit { usize value; usize count; };

SdkLimit sdk_limit(SdkResourceEmission out, SdkResourceHost host, usize source,
    u32 distance, usize one) on cpu requires nonzero(host.emit) {
    if (distance == 0) {
        // A strict upper bound of UINT32_MAX + 1 carries no new fact.
        usize maximum = sdk_constant(out, host, u32(BuiltinScalar::U32), 4294967295u64);
        usize valid = sdk_binary(out, host, SdkResourceOp::NotEqual, u32(BuiltinScalar::Bool), source, maximum);
        usize count = sdk_emit(out, host, SdkResourceOp::Cast, u32(BuiltinScalar::U32), valid, 0u64, 0u64);
        usize value = sdk_binary(out, host, SdkResourceOp::Add, u32(BuiltinScalar::U32), source, one);
        return SdkLimit(value, count);
    }
    if (distance == 1) { return SdkLimit(source, one); }
    usize removed = sdk_constant(out, host, u32(BuiltinScalar::U32), u64(distance - 1));
    usize small = sdk_binary(out, host, SdkResourceOp::Less, u32(BuiltinScalar::Bool), source, removed);
    usize clipped = sdk_emit(out, host, SdkResourceOp::Select, u32(BuiltinScalar::U32), small, source, removed);
    return SdkLimit(sdk_binary(out, host, SdkResourceOp::Subtract, u32(BuiltinScalar::U32), source, clipped), one);
}

void sdk_complete_field(SdkResourceEmission out, SdkResourceHost host, SdkCompletion field,
    Span source) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.emit) requires nonzero(host.call) {
    SdkArgument ranges = sdk_packet(out, SdkArgumentKind::Intervals, field.lower.count, 2u64);
    usize range_row = 0u64;
    while (range_row < field.lower.count) {
        usize lower = sdk_constant(out, host, u32(BuiltinScalar::U64), word_at(field.lower, range_row));
        usize upper = sdk_constant(out, host, u32(BuiltinScalar::U64), word_at(field.upper, range_row));
        sdk_store(out, host, ranges.value + range_row * 2u64, lower);
        sdk_store(out, host, ranges.value + range_row * 2u64 + 1u64, upper);
        range_row += 1u64;
    }
    SdkArgument bounds = sdk_packet(out, SdkArgumentKind::Limits, field.parameters.count, 1u64);
    usize count = sdk_constant(out, host, u32(BuiltinScalar::U32), 0u64);
    usize one = sdk_constant(out, host, u32(BuiltinScalar::U32), 1u64);
    usize stride = sdk_constant(out, host, u32(BuiltinScalar::U32), size_of<u32>());
    usize limit_row = 0u64;
    while (limit_row < field.parameters.count) {
        usize source_value = sdk_operand(source, 3u64 + word_at(field.parameters, limit_row), host.ir);
        address at = node_at(field.distances, limit_row, N_INSTRUCTION).data;
        SdkLimit limit = sdk_limit(out, host, source_value, unsafe_position<u32>(at), one);
        usize offset = sdk_binary(out, host, SdkResourceOp::Multiply, u32(BuiltinScalar::U32), count, stride);
        host.emit(out.state, out.block, SdkResourceOp::StoreIndexed, 0,
            bounds.value, bounds.count, offset, limit.value);
        count = sdk_binary(out, host, SdkResourceOp::Add, u32(BuiltinScalar::U32), count, limit.count);
        limit_row += 1u64;
    }

    usize field_offset = sdk_constant(out, host, u32(BuiltinScalar::U32), u64(field.offset));
    usize offset = sdk_binary(out, host, SdkResourceOp::Add, u32(BuiltinScalar::U32),
        sdk_operand(source, 2u64, host.ir), field_offset);
    usize mask = sdk_constant(out, host, u32(BuiltinScalar::U64), field.mask);
    usize bits = sdk_constant(out, host, u32(BuiltinScalar::U64), field.bits);
    usize range_count = sdk_constant(out, host, u32(BuiltinScalar::U64), field.lower.count);
    usize limit_count = sdk_emit(out, host, SdkResourceOp::Cast, u32(BuiltinScalar::U64), count, 0u64, 0u64);
    let arguments = array<SdkArgument,9>(
        SdkArgument(SdkArgumentKind::Original, 0u64, 0u64),
        SdkArgument(SdkArgumentKind::Original, 1u64, 0u64),
        SdkArgument(SdkArgumentKind::U32, offset, 0u64), ranges,
        SdkArgument(SdkArgumentKind::U64, range_count, 0u64), bounds,
        SdkArgument(SdkArgumentKind::U64, limit_count, 0u64),
        SdkArgument(SdkArgumentKind::U64, mask, 0u64),
        SdkArgument(SdkArgumentKind::U64, bits, 0u64));
    host.call(out.state, out.block, SdkResourceCall::Complete, address_of(arguments[0]), 9u64);
}

bool sdk_resource_call(SdkResourceEmission out, read SdkResourceInput input,
    SdkResourceHost host, Node call) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.emit) requires nonzero(host.call)
    requires nonzero(host.layout) requires nonzero(host.completions) requires nonzero(host.completion) {
    View invocation = host.ir.inspect(call);
    View target = host.ir.inspect(invocation.left);
    if(target.tag!=0){return false;}
    let resource=application_resource_slot(target.first,target.second);
    if(resource.kind==ApplicationResourceKind::Allocate){
        sdk_allocate(out,host,resource.index);
        return true;
    }
    if(resource.kind==ApplicationResourceKind::Run){
        sdk_run(out,host,host.layout(input.programs,resource.index),invocation.first,resource.index);
        return true;
    }
    if(resource.kind!=ApplicationResourceKind::Complete){return false;}
    Span fields=host.completions(input.completions,resource.index);
    usize row = 0u64;
    while (row < fields.count) {
        SdkCompletion field = host.completion(node_at(fields, row, N_INSTRUCTION).data);
        sdk_complete_field(out, host, field, invocation.first);
        row += 1u64;
    }
    return true;
}

extern "C" void sdk_resource_block(address state, read SdkResourceInput input,
    SdkResourceHost host, address block) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.next) requires nonzero(host.child)
    requires nonzero(host.block) requires nonzero(host.keep) requires nonzero(host.emit)
    requires nonzero(host.call) requires nonzero(host.layout)
    requires nonzero(host.completions) requires nonzero(host.completion) {
    SdkResourceEmission out = SdkResourceEmission(state, block);
    Node current = host.next(block);
    while (address_bits(current.data) != 0u64) {
        View instruction = host.ir.inspect(current);
        u32 kind = instruction.tag;
        if (kind == OP_SCOPE || kind == OP_BRANCH || kind == OP_LOOP_VALUE) {
            host.block(host.child(block, 0), state, address_of(input), host);
            if (kind != OP_SCOPE) {
                host.block(host.child(block, 1), state, address_of(input), host);
            }
        }
        if (kind != OP_INVOKE || !sdk_resource_call(out, input, host, instruction.left)) {
            host.keep(block);
        }
        current = host.next(block);
    }
}

void sdk_resource_body(SdkResourceBody body, read SdkResourceInput input, SdkResourceHost host) on cpu
    requires nonzero(host.slots) requires nonzero(host.block) {
    SdkResourceState state = SdkResourceState(body.registers, body.locals,
        host.slots(body.registers, body.locals));
    host.block(body.code, address_of(state), address_of(input), host);
}

void sdk_resource_stages(Span stages, read SdkResourceInput input, SdkResourceHost host) on cpu
    requires nonzero(host.stage) requires nonzero(host.slots) requires nonzero(host.block) {
    usize index = 0u64;
    while (index < stages.count) {
        SdkResourceStage stage = host.stage(node_at(stages, index, N_STAGE).data);
        sdk_resource_body(stage.main, input, host);
        if (address_bits(stage.project.code) != 0u64) { sdk_resource_body(stage.project, input, host); }
        if (address_bits(stage.identity.code) != 0u64) {
            sdk_resource_body(stage.identity, input, host);
            sdk_resource_body(stage.combine, input, host);
        }
        index += 1u64;
    }
}

extern "C" void lower_sdk_resources(read SdkResourceInput input, SdkResourceHost host) on cpu
    requires nonzero(host.function) requires nonzero(host.stages) requires nonzero(host.stage)
    requires nonzero(host.slots) requires nonzero(host.block) {
    usize function_index = 0u64;
    while (function_index < input.functions.count) {
        SdkResourceBody body = host.function(node_at(input.functions, function_index, N_INSTRUCTION).data);
        sdk_resource_body(body, input, host);
        function_index += 1u64;
    }
    sdk_resource_stages(input.root, input, host);
    usize program_index = 0u64;
    while (program_index < input.program_count) {
        sdk_resource_stages(host.stages(input.programs, program_index), input, host);
        program_index += 1u64;
    }
}

TileKey tile_key_for_view(View tile, usize index, u32 byte) on cpu {
    return tile.tag == 0 ? TileKey(0u64, tile.a, u32(tile.b) + byte) : TileKey(1u64, index, byte);
}

TileKey tile_key(Span tiles, usize index, u32 byte, Host ir) on cpu requires nonzero(ir.inspect) {
    address element = unsafe_address(address_bits(tiles.data) + index * tiles.stride);
    return tile_key_for_view(ir.inspect(Node(element, N_TILE)), index, byte);
}
