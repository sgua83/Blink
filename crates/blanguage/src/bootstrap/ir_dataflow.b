module bootstrap.ir_dataflow;

// These tags describe the existing borrowed Rust IR fields at this one
// private native handoff. They do not name a public IR format or registry.
const u32 N_INSTRUCTION=0;const u32 N_CALL=1;const u32 N_TARGET=2;const u32 N_ARGUMENT=3;
const u32 N_DATASET_BORROW=4;const u32 N_REFERENCE=5;const u32 N_STORAGE=6;
const u32 N_INDEXED=7;const u32 N_STRIDE=8;const u32 N_TEXTURE=9;const u32 N_ATOMIC_OP=10;
const u32 N_MEMORY_ALIAS=11;const u32 N_MEMORY_EFFECT=12;const u32 N_DIVISION=13;
const u32 N_DIVISOR=14;const u32 N_WORDS3=15;
const u32 N_COMPONENT=16;
const u32 N_STAGE=17;const u32 N_RAY_RANGE=18;

const u32 OP_TEXTURE_BINDING_ADDRESS=0;const u32 OP_TEXTURE_SAMPLE=1;const u32 OP_TEXTURE_SIZE=2;
const u32 OP_TEXTURE_LEVELS=3;const u32 OP_TEXTURE_ADDRESS=4;const u32 OP_TEXTURE_FORMAT=5;
const u32 OP_TEXTURE_READ=6;const u32 OP_TEXTURE_FETCH=7;const u32 OP_LOAD_INDEXED=8;
const u32 OP_STORE_INDEXED=9;const u32 OP_ATOMIC=10;const u32 OP_APPEND=11;
const u32 OP_TILE_BARRIER=12;const u32 OP_TILE_PUBLISH=13;const u32 OP_TILE_COLLECTIVE=14;
const u32 OP_TILE_LENGTH=15;const u32 OP_TILE_LANE=16;const u32 OP_TILE_LOAD=17;
const u32 OP_SUBGROUP=18;const u32 OP_PRINT=19;const u32 OP_INVOKE=20;const u32 OP_VIEW_LOAD=21;
const u32 OP_RAY_QUERY=22;const u32 OP_CONSTANT=23;const u32 OP_FUNCTION_ADDRESS=24;
const u32 OP_POSITION_ADDRESS=25;const u32 OP_BUFFER_STRIDE=26;const u32 OP_INDEX_VALUE=27;
const u32 OP_UNIFORM=28;const u32 OP_LOAD_LOCAL=29;const u32 OP_STORE_LOCAL=30;
const u32 OP_LOAD_LOCAL_INDEXED=31;const u32 OP_STORE_LOCAL_INDEXED=32;
const u32 OP_LOAD=33;const u32 OP_STORE=34;const u32 OP_UNARY=35;const u32 OP_BINARY=36;
const u32 OP_INTEGER_DIVIDE=37;const u32 OP_CAST=38;const u32 OP_MATH=39;const u32 OP_SELECT_VALUE=40;
const u32 OP_BRANCH=41;const u32 OP_LOOP_VALUE=42;const u32 OP_BREAK_VALUE=43;
const u32 OP_CONTINUE_VALUE=44;const u32 OP_SCOPE=45;const u32 OP_EXIT_VALUE=46;
const u32 OP_RAISE=47;const u32 OP_CLEANUP=48;

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

extern "C" Results results(Node source,Host host) on cpu requires nonzero(host.inspect) {
 Results output=Results(empty_span(),empty_span(),0u64,0u64,0u64,0u64);
 View value=host.inspect(source);u32 kind=value.tag;
 if(kind==OP_INVOKE||kind==OP_TEXTURE_SAMPLE||kind==OP_TEXTURE_FETCH||kind==OP_TEXTURE_SIZE
     ||kind==OP_RAY_QUERY||kind==OP_TILE_COLLECTIVE){
  output.first=value.first;
  if(kind==OP_INVOKE){output.second=value.second;}
 }else if(kind==OP_ATOMIC){
  output.first=value.first;
 }else if(kind==OP_INTEGER_DIVIDE){
  View result=host.inspect(value.right);output.a=result.a;output.b=result.b;
  output.inline_count=result.tag==2 ? 2u64 : 1u64;
 }else if(kind==OP_TEXTURE_BINDING_ADDRESS||kind==OP_TEXTURE_LEVELS||kind==OP_TEXTURE_ADDRESS
     ||kind==OP_TEXTURE_FORMAT||kind==OP_TEXTURE_READ||kind==OP_APPEND||kind==OP_CONSTANT
     ||kind==OP_FUNCTION_ADDRESS||kind==OP_INDEX_VALUE||kind==OP_UNIFORM||kind==OP_BUFFER_STRIDE
     ||kind==OP_LOAD_LOCAL||kind==OP_TILE_LENGTH||kind==OP_TILE_LANE||kind==OP_POSITION_ADDRESS
     ||kind==OP_LOAD_INDEXED||kind==OP_LOAD||kind==OP_LOAD_LOCAL_INDEXED||kind==OP_SUBGROUP
     ||kind==OP_PRINT||kind==OP_TILE_LOAD||kind==OP_VIEW_LOAD||kind==OP_UNARY||kind==OP_BINARY
     ||kind==OP_CAST||kind==OP_MATH||kind==OP_SELECT_VALUE){
  output.a=value.a;output.inline_count=1u64;
 }
 return output;
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
u32 visit_instruction(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit) {
 View value=host.inspect(source);u32 kind=value.tag;u32 continued=1;
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
 return visit_instruction(source,host,context,visit);
}

bool accessed(usize access,u32 writing) on cpu {
 return writing!=0 ? access==2u64||access==3u64||access==4u64 : access==1u64||access==3u64||access==4u64;
}
struct RangePart { u32 more; u32 local; usize begin; usize end; };
RangePart no_range() on cpu {return RangePart(0,0,0u64,0u64);}
RangePart indexed_alias_range(Node source,usize index,Host host) on cpu requires nonzero(host.inspect) {
 View value=host.inspect(source);
 if(value.tag!=2||index>=value.second.count){return no_range();}
 View alias=host.inspect(node_at(value.second,index,N_MEMORY_ALIAS));
 return RangePart(1,alias.tag==0 ? 1 : 0,alias.a,alias.a+alias.b);
}
RangePart alias_range(Node source,usize index,Host host) on cpu requires nonzero(host.inspect) {
 if(source.kind==N_REFERENCE){
  View reference=host.inspect(source);View storage=host.inspect(reference.left);
  if(storage.tag==0){return index==0u64 ? RangePart(1,1,storage.a,storage.a+storage.b) : no_range();}
  return indexed_alias_range(storage.left,index,host);
 }
 return indexed_alias_range(source,index,host);
}
extern "C" Locals locals(Node source,u32 writing,Host host) on cpu requires nonzero(host.inspect) {
 Locals state=Locals(source,0,writing,0u64,0u64,0u64,0u64,0u64,0);
 if(source.kind==N_CALL){state.mode=4;return state;}
 View value=host.inspect(source);u32 kind=value.tag;
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
void compact_range(address marks,usize begin,usize end) on cpu {
 usize position=begin;
 while(position<end){unsafe_position<usize>(compact_at(marks,position))=1u64;position+=1u64;}
}
void compact_aliases(Node source,address marks,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(true){
  RangePart part=alias_range(source,index,host);
  if(part.more==0){return;}
  if(part.local!=0){compact_range(marks,part.begin,part.end);}
  index+=1u64;
 }
}
void compact_locals(View value,address marks,Host host) on cpu requires nonzero(host.inspect) {
 if(value.tag==OP_LOAD_LOCAL){compact_range(marks,value.b,value.b+1u64);}
 else if(value.tag==OP_STORE_LOCAL){compact_range(marks,value.a,value.a+1u64);}
 else if(value.tag==OP_LOAD_LOCAL_INDEXED){compact_range(marks,value.b,value.b+value.c);}
 else if(value.tag==OP_STORE_LOCAL_INDEXED){compact_range(marks,value.a,value.a+value.b);}
 else if(value.tag==OP_INVOKE){
  View call=host.inspect(value.left);usize index=0u64;
  // Passing an address preserves its whole local range even without a read
  // or write effect. This is the union needed by the original compactor.
  while(index<call.first.count){
   View argument=host.inspect(node_at(call.first,index,N_ARGUMENT));
   if(argument.tag==1){compact_aliases(argument.left,marks,host);}
   index+=1u64;
  }
  index=0u64;
  while(index<call.second.count){
   View effect=host.inspect(node_at(call.second,index,N_MEMORY_EFFECT));
   View alias=host.inspect(effect.left);
   if(alias.tag==0){compact_range(marks,alias.a,alias.a+alias.b);}
   index+=1u64;
  }
 }else if(value.tag==OP_POSITION_ADDRESS||value.tag==OP_LOAD_INDEXED
     ||value.tag==OP_STORE_INDEXED||value.tag==OP_ATOMIC){
  compact_aliases(value.left,marks,host);
 }
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
void rewrite_word(address field,address context,cfn<usize,address,usize> ensures memory(read) map) on cpu requires nonzero(map) {
 unsafe_position<usize>(field)=map(context,unsafe_position<usize>(field));
}
void rewrite_optional(address field,address context,cfn<usize,address,usize> ensures memory(read) map) on cpu requires nonzero(map) {
 if(address_bits(field)!=0u64){rewrite_word(field,context,map);}
}
void rewrite_words(RewriteSpan words,address context,cfn<usize,address,usize> ensures memory(read) map) on cpu requires nonzero(map) {
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
void rewrite_result(RewriteNode source,RewriteHost host,address context,cfn<usize,address,usize> ensures memory(read) map) on cpu
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

u32 fill_failure_branch(address block,address output,LowerBranch branch,u32 side,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare) requires nonzero(host.raise) requires nonzero(host.join) requires nonzero(host.scope) requires nonzero(host.branches) requires nonzero(host.finish_if)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 if(side!=0){
  if(branch.kind==4){host.store(block,output,branch.slot,branch.error);}
  return 0;
 }
 if(branch.kind==1){host.raise(block,output,branch.error);}
 else if(branch.kind==2){return host.scope(block,output,branch.error,Word(branch.existing,branch.slot));}
 else if(branch.kind==3){
  usize previous=host.reg(block,0);
  usize joined=host.reg(block,0);
  usize zero=host.reg(block,0);
  usize present=host.reg(block,1);
  host.load(block,output,previous,branch.slot);
  host.constant(block,output,zero,0u64);
  host.compare(block,output,present,previous,zero);
  return host.branches(block,output,Word(1,present),LowerBranch(4,branch.error,branch.slot,0,previous,joined));
 }else if(branch.kind==4){
  host.join(block,output,branch.joined,branch.previous,branch.error);
  host.store(block,output,branch.slot,branch.joined);
 }
 return 0;
}

extern "C" u32 emit_failure_if(address block,address output,address yes,address no,Word test,LowerBranch branch,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare) requires nonzero(host.raise) requires nonzero(host.join) requires nonzero(host.scope) requires nonzero(host.branches) requires nonzero(host.finish_if)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 if(fill_failure_branch(block,yes,branch,0,host)!=0){return 1;}
 if(fill_failure_branch(block,no,branch,1,host)!=0){return 1;}
 usize condition=test.value;
 if(test.present==0){
  usize zero=host.reg(block,0);
  condition=host.reg(block,1);
  host.constant(block,output,zero,0u64);
  host.compare(block,output,condition,test.value,zero);
 }
 host.finish_if(block,output,condition,yes,no);
 return 0;
}

extern "C" u32 lower_failure_block(address block,address output,LowerHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.child) requires nonzero(host.reg) requires nonzero(host.capture) requires nonzero(host.set_error) requires nonzero(host.original) requires nonzero(host.cleanup_body) requires nonzero(host.cleanup_scope) requires nonzero(host.fold) requires nonzero(host.constant) requires nonzero(host.load) requires nonzero(host.store) requires nonzero(host.compare) requires nonzero(host.raise) requires nonzero(host.join) requires nonzero(host.scope) requires nonzero(host.branches) requires nonzero(host.finish_if)
 requires nonzero(host.effects.inspect) requires nonzero(host.effects.contains) {
 u32 flags=0;
 while(true){
  LowerNode node=host.next(block);
  if(node.present==0){return flags;}
  if(node.kind==1){
   if(!effect_body(node.body,host.effects,1)){
    host.fold(node.first,node.slot);
    host.cleanup_scope(block,output,node.first);
    flags|=1;
    continue;
   }
   host.capture(block,node.source,node.payload,1);
   usize zero=host.reg(block,0);
   usize error=host.reg(block,0);
   usize failed=host.reg(block,1);
   host.constant(block,output,zero,0u64);
   host.store(block,output,node.slot,zero);
   host.cleanup_body(block,output,node.first);
   host.load(block,output,error,node.slot);
   host.compare(block,output,failed,error,zero);
   u32 kind=node.continuation==2 ? 2 : 1;
   if(kind==2){flags|=2;}
   if(host.branches(block,output,Word(1,failed),LowerBranch(kind,error,node.slot,1,0u64,0u64))!=0){return 4;}
   flags|=1;
   continue;
  }
  if(node.kind==3||node.kind==4||node.kind==5){
   u32 nested=host.child(block,node.first);
   if((nested&4)!=0){return 4;}
   flags|=nested;
   if(node.kind!=3){
    nested=host.child(block,node.second);
    if((nested&4)!=0){return 4;}
    flags|=nested;
   }
  }
  bool lower=node.kind==2 && node.continuation!=0 && (node.error.present!=0
       ||(address_bits(node.target)!=0u64 && host.effects.contains(host.effects.known,node.target)!=0));
  if(!lower){host.original(block,output);continue;}
  usize error=node.error.value;
  if(node.error.present==0){error=host.reg(block,0);host.set_error(node.error_slot,error);}
  host.capture(block,node.source,node.payload,0);
  host.original(block,output);
  if(node.continuation==2){flags|=2;}
  if(host.branches(block,output,Word(0,error),LowerBranch(node.continuation,error,node.slot,0,0u64,0u64))!=0){return 4;}
  flags|=1;
 }
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

u32 float_code(Span code,address registers,u32 scalar,FloatHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_MATH) {
   FloatScalar target=host.scalar(registers,value.a);
   if(target.kind==scalar && value.b!=0u64){return 1;}
  }else if(value.tag==OP_BINARY) {
   if(value.d!=0u64) {
    FloatScalar input=host.scalar(registers,value.b);
    if(input.kind==scalar){return 1;}
   }
  }else if(value.tag==OP_CAST) {
   FloatScalar source=host.scalar(registers,value.b);
   if(source.floating!=0) {
    FloatScalar target=host.scalar(registers,value.a);
    if(target.integer==0 && ((target.kind==scalar)!=(source.kind==scalar))){return 1;}
   }
  }else if(value.tag==OP_TILE_COLLECTIVE) {
   usize result=0u64;
   while(result<value.first.count) {
    FloatScalar target=host.scalar(registers,word_at(value.first,result));
    if(target.kind==scalar){return 1;}
    result+=1u64;
   }
  }else if(value.tag==OP_SUBGROUP) {
   FloatScalar target=host.scalar(registers,value.a);
   // Original SubgroupOp Sum, ExclusiveSum, Min and Max discriminants.
   if(target.kind==scalar && (value.c==13u64 || value.c==14u64
       || value.c==15u64 || value.c==16u64)){return 1;}
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(float_code(value.first,registers,scalar,host)!=0){return 1;}
   if(float_code(value.second,registers,scalar,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
extern "C" u32 float_preserve(FloatInput input,u32 scalar,FloatHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar) {
 if(input.collective.present!=0 && input.collective.value==usize(scalar)){return 1;}
 if(float_code(input.identity.code,input.identity.registers,scalar,host)!=0){return 1;}
 if(float_code(input.combine.code,input.combine.registers,scalar,host)!=0){return 1;}
 if(float_code(input.project.code,input.project.registers,scalar,host)!=0){return 1;}
 return float_code(input.body.code,input.body.registers,scalar,host);
}

extern "C" u32 has_tile_scope(Span code,u32 has_tiles,Host host) on cpu
 requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_TILE_LENGTH || value.tag==OP_TILE_LANE || value.tag==OP_TILE_PUBLISH
      || value.tag==OP_TILE_BARRIER || value.tag==OP_TILE_COLLECTIVE || value.tag==OP_TILE_LOAD
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

void shared_parts(usize tile,u32 byte,u32 bytes,address context,
 cfn<void,address,usize,u32> emit) on cpu requires nonzero(emit) {
 u32 part=0;
 while(part<(bytes+3)/4){emit(context,tile,byte+part*4);part+=1;}
}
void shared_components(usize tile,StorageHost host,address context,
 cfn<void,address,usize,u32> emit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.components) requires nonzero(emit) {
 Span components=host.components(host.stage,tile);usize index=0u64;
 while(index<components.count) {
  View component=host.ir.inspect(node_at(components,index,N_COMPONENT));
  shared_parts(tile,u32(component.a),u32(component.b),context,emit);
  index+=1u64;
 }
}
extern "C" void storage_words(Span code,StorageHost host,address context,
 cfn<void,address,usize,u32> emit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.components)
 requires nonzero(host.scalar_bytes) requires nonzero(emit) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_ATOMIC || value.tag==OP_LOAD_INDEXED) {
   View source=host.ir.inspect(value.left);
   if(source.tag==3) {
    if(value.tag==OP_ATOMIC && value.second.count==0u64) {
     shared_parts(source.a,u32(value.a),u32(value.b),context,emit);
    }else{shared_components(source.a,host,context,emit);}
   }
  }else if(value.tag==OP_TILE_LOAD) {
   shared_parts(value.b,u32(value.d),host.scalar_bytes(host.stage,value.a),context,emit);
  }else if(value.tag==OP_TILE_COLLECTIVE) {
   usize result=0u64;
   while(result<value.first.count) {
    // Read only the original pair's u32 byte offset, never its Rust padding.
    u32 byte=unsafe_position<u32>(unsafe_address(address_bits(value.second.data)+u64(result)*u64(value.second.stride)));
    shared_parts(value.a,byte,host.scalar_bytes(host.stage,word_at(value.first,result)),context,emit);
    result+=1u64;
   }
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   storage_words(value.first,host,context,emit);
   storage_words(value.second,host,context,emit);
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
struct AppendOperations { usize count; usize prefix; usize elect; usize first; };
AppendOperations append_operations() on cpu {return AppendOperations(6u64,7u64,3u64,8u64);}
extern "C" void subgroup_operations(Span code,Host host,address context,
 cfn<void,address,usize> emit) on cpu requires nonzero(host.inspect) requires nonzero(emit) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_APPEND) {
   AppendOperations operations=append_operations();
   emit(context,operations.count);emit(context,operations.prefix);
   emit(context,operations.elect);emit(context,operations.first);
  }else if(value.tag==OP_SUBGROUP){emit(context,value.c);}
  else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   subgroup_operations(value.first,host,context,emit);
   subgroup_operations(value.second,host,context,emit);
  }
  index+=1u64;
 }
}
SubgroupFlags empty_subgroup_flags() on cpu {return SubgroupFlags(0,0,0,0,0,0,0);}
void include_subgroup(inout SubgroupFlags flags,usize operation) on cpu {
 flags.basic=1;
 if(operation==4u64 || operation==5u64){flags.vote=1;}
 else if(operation==6u64 || operation==7u64 || operation==8u64){flags.ballot=1;}
 else if(operation==9u64 || operation==10u64){flags.shuffle=1;}
 else if(operation==11u64 || operation==12u64){flags.shuffle_relative=1;}
 else if(operation==13u64 || operation==14u64 || operation==15u64 || operation==16u64
     || operation==17u64 || operation==18u64 || operation==19u64 || operation==20u64
     || operation==21u64 || operation==22u64){flags.arithmetic=1;}
}
void subgroup_flags(Span code,Host host,inout SubgroupFlags flags) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_APPEND) {
   AppendOperations operations=append_operations();
   include_subgroup(flags,operations.count);include_subgroup(flags,operations.prefix);
   include_subgroup(flags,operations.elect);include_subgroup(flags,operations.first);
  }else if(value.tag==OP_SUBGROUP){include_subgroup(flags,value.c);}
  else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   subgroup_flags(value.first,host,flags);subgroup_flags(value.second,host,flags);
  }
  index+=1u64;
 }
}
extern "C" SubgroupFlags subgroup_from_operations(address context,cfn<Word,address> next) on cpu
 requires nonzero(next) {
 SubgroupFlags flags=empty_subgroup_flags();
 while(true) {
  Word operation=next(context);if(operation.present==0){return flags;}
  include_subgroup(flags,operation.value);
 }
}
u32 subgroup_extended(Span code,address stage,SubgroupHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_SUBGROUP) {
   if(host.scalar_bytes(stage,value.a)!=4){return 1;}
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(subgroup_extended(value.first,stage,host)!=0){return 1;}
   if(subgroup_extended(value.second,stage,host)!=0){return 1;}
  }
  index+=1u64;
 }
 return 0;
}
extern "C" SubgroupFlags subgroup_capabilities(Span stages,SubgroupHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.scalar_bytes) {
 SubgroupFlags flags=empty_subgroup_flags();usize index=0u64;
 while(index<stages.count) {
  Node stage=node_at(stages,index,N_STAGE);View value=host.ir.inspect(stage);
  subgroup_flags(value.first,host.ir,flags);
  if(flags.extended_types==0){flags.extended_types=subgroup_extended(value.first,stage.data,host);}
  index+=1u64;
 }
 return flags;
}

void collect_ray_operands(Span code,RayHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) {
 usize index=0u64;
 while(index<code.count) {
  Node source=node_at(code,index,N_INSTRUCTION);
  visit_instruction(source,host.ir,host.used,host.include);
  View value=host.ir.inspect(source);
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   collect_ray_operands(value.first,host);collect_ray_operands(value.second,host);
  }
  index+=1u64;
 }
}
bool has_closest_ray(Span code,Host host) on cpu requires nonzero(host.inspect) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_RAY_QUERY && value.first.count==39u64){return true;}
  if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(has_closest_ray(value.first,host) || has_closest_ray(value.second,host)){return true;}
  }
  index+=1u64;
 }
 return false;
}
u32 select_ray_queries(Span code,Span ranges,RayHost host,address context,
 cfn<u32,address,usize,usize> visit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.contains)
 requires nonzero(host.fields) requires nonzero(visit) {
 usize index=0u64;
 while(index<code.count) {
  View value=host.ir.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_RAY_QUERY && value.first.count==39u64) {
   usize group=0u64;
   while(group<ranges.count) {
    View range=host.ir.inspect(node_at(ranges,group,N_RAY_RANGE));
    Span fields=host.fields(value.first,range.a,range.b);usize field=0u64;bool consumed=false;
    while(field<fields.count && !consumed) {
     consumed=host.contains(host.used,word_at(fields,field))!=0;field+=1u64;
    }
    if(consumed && visit(context,group,word_at(value.first,0u64))==0){return 0;}
    group+=1u64;
   }
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   if(select_ray_queries(value.first,ranges,host,context,visit)==0){return 0;}
   if(select_ray_queries(value.second,ranges,host,context,visit)==0){return 0;}
  }
  index+=1u64;
 }
 return 1;
}
extern "C" u32 ray_result_queries(Span code,Span ranges,RayHost host,address context,
 cfn<u32,address,usize,usize> visit) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.include) requires nonzero(host.contains)
 requires nonzero(host.fields) requires nonzero(visit) {
 if(!has_closest_ray(code,host.ir)){return 1;}
 collect_ray_operands(code,host);
 return select_ray_queries(code,ranges,host,context,visit);
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

extern "C" u32 atomic64_requirements(Span code,Host host) on cpu requires nonzero(host.inspect) {
 u32 flags=0;usize index=0u64;
 while(index<code.count && flags!=3) {
  View value=host.inspect(node_at(code,index,N_INSTRUCTION));
  if(value.tag==OP_ATOMIC && value.b==8u64) {
   View target=host.inspect(value.left);
   if(target.tag==0){flags=flags|1;}else if(target.tag==3){flags=flags|2;}
  }else if(value.tag==OP_SCOPE || value.tag==OP_BRANCH || value.tag==OP_LOOP_VALUE) {
   flags=flags|atomic64_requirements(value.first,host);
   if(flags!=3){flags=flags|atomic64_requirements(value.second,host);}
  }
  index+=1u64;
 }
 return flags;
}
extern "C" u32 program_atomic64_requirements(Span stages,Host host) on cpu requires nonzero(host.inspect) {
 u32 flags=0;usize index=0u64;
 while(index<stages.count && flags!=3) {
  View stage=host.inspect(node_at(stages,index,N_STAGE));
  flags=flags|atomic64_requirements(stage.first,host);index+=1u64;
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
