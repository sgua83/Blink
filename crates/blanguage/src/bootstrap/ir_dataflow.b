module bootstrap.ir_dataflow;

// These tags describe the existing borrowed Rust IR fields at this one
// private native handoff. They do not name a public IR format or registry.
const u32 N_INSTRUCTION=0;const u32 N_CALL=1;const u32 N_TARGET=2;const u32 N_ARGUMENT=3;
const u32 N_DATASET_BORROW=4;const u32 N_REFERENCE=5;const u32 N_STORAGE=6;
const u32 N_INDEXED=7;const u32 N_STRIDE=8;const u32 N_TEXTURE=9;const u32 N_ATOMIC_OP=10;
const u32 N_MEMORY_ALIAS=11;const u32 N_MEMORY_EFFECT=12;const u32 N_DIVISION=13;
const u32 N_DIVISOR=14;const u32 N_WORDS3=15;

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
