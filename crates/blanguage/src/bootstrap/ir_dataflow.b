module bootstrap.ir_dataflow;

// These tags describe the existing borrowed Rust IR fields at this one
// private native handoff. They do not name a public IR format or registry.
const u32 N_INSTRUCTION=0;const u32 N_CALL=1;const u32 N_TARGET=2;const u32 N_ARGUMENT=3;
const u32 N_DATASET_BORROW=4;const u32 N_REFERENCE=5;const u32 N_STORAGE=6;
const u32 N_INDEXED=7;const u32 N_STRIDE=8;const u32 N_TEXTURE=9;const u32 N_ATOMIC_OP=10;
const u32 N_MEMORY_ALIAS=11;const u32 N_MEMORY_EFFECT=12;const u32 N_DIVISION=13;
const u32 N_DIVISOR=14;const u32 N_WORDS3=15;
const u32 N_COMPONENT=16;

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

bool effect_body(Span code,EffectHost host,u32 explicit_errors) on cpu
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
  if(unsafe_position<u8>(item.flag)==u8(0) && effect_body(item.body,host,explicit_errors)) {
   unsafe_position<u8>(item.flag)=u8(1);
   changed=1;
  }
 }
}

extern "C" void propagate_effects(address context,cfn<u32,address> round) on cpu
 requires nonzero(round) {
 while(round(context)!=0){}
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
