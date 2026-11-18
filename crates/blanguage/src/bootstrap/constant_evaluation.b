module bootstrap.constant_evaluation;
import bootstrap.ir_dataflow;

// Exact existing builtin spellings, read directly from the original name.
bool name_is(Name name,u64 spelling,usize length) on cpu {
 if(name.count!=length){return false;}
 usize index=0u64;
 while(index<length){
  u8 actual=unsafe_position<u8>(unsafe_address(address_bits(name.data)+u64(index)));
  if(actual!=u8(spelling>>(u64(index)*8u64))){return false;}
  index+=1u64;
 }
 return true;
}
extern "C" u32 declaration_type(Name name,u32 wrappers) on cpu {
 return wrappers==0&&!name_is(name,1684631414u64,4u64)&&!name_is(name,491328529774u64,5u64) ? 1 : 0;
}
Node instruction_at(Span code,usize index) on cpu {
 return Node(unsafe_address(address_bits(code.data)+u64(index)*u64(code.stride)),0);
}
extern "C" u32 constant_result(u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect) {
 if((facts&1)!=0||(facts&2)==0){return 0;}
 usize index=0u64;
 while(index<code.count){
  if(ir.inspect(instruction_at(code,index)).tag!=OP_CONSTANT){return 0;}
  index+=1u64;
 }
 return 1;
}
bool selected_register(Span used,usize value) on cpu {
 usize first=0u64;usize last=used.count;
 while(first<last){
  usize middle=first+(last-first)/2u64;
  usize at=unsafe_position<usize>(unsafe_address(address_bits(used.data)+u64(middle)*u64(used.stride)));
  if(at<value){first=middle+1u64;}else{last=middle;}
 }
 return first<used.count&&unsafe_position<usize>(unsafe_address(address_bits(used.data)+u64(first)*u64(used.stride)))==value;
}
extern "C" void source_operations(Span code,usize first,Span used,address output,OriginHost origin,Host ir) on cpu
 requires nonzero(origin.inspect) requires nonzero(origin.append) requires nonzero(ir.inspect) {
 usize index=0u64;u32 previous=0;
 address source=unsafe_address(0u64);address generated=unsafe_address(0u64);
 while(index<code.count){
  Node node=instruction_at(code,index);
  bool selected=true;
  if(index<first){
   View view=ir.inspect(node);
   selected=view.tag==OP_CONSTANT&&selected_register(used,view.a);
  }
  if(selected){
   Origin actual=origin.inspect(node.data);
   if(actual.present!=0){
    if(previous==0||actual.source!=source||actual.generated!=generated){
     origin.append(output,node.data);
     source=actual.source;generated=actual.generated;previous=1;
    }
   }
  }
  index+=1u64;
 }
}

extern "C" ModuleEntry module_entry(u32 cached) on cpu {
 return cached!=0 ? ModuleEntry::Cached : ModuleEntry::Evaluate;
}
extern "C" ModuleCursor module_start() on cpu {return ModuleCursor(0);}
extern "C" ModuleNeed module_step(inout ModuleCursor cursor,u32 type_allowed) on cpu {
 if(cursor.phase==0){cursor.phase=1;return ModuleNeed::Instance;}
 if(cursor.phase==1){cursor.phase=2;return ModuleNeed::ReturnType;}
 return type_allowed!=0 ? ModuleNeed::Body : ModuleNeed::InvalidType;
}

extern "C" SyntaxCursor syntax_start() on cpu {return SyntaxCursor(0);}
extern "C" SyntaxNeed syntax_step(inout SyntaxCursor cursor,ExpressionView expression,u32 facts,ExpressionHost host) on cpu
 requires nonzero(host.inspect) {
 if(cursor.phase==0){
  cursor.phase=1;
  if(expression.kind==ExpressionKind::Number){return SyntaxNeed::Number;}
  if(expression.kind==ExpressionKind::StaticU32){return SyntaxNeed::StaticU32;}
  if(expression.kind==ExpressionKind::Bool){return SyntaxNeed::Bool;}
  if(expression.kind==ExpressionKind::Name){return SyntaxNeed::Name;}
  if(expression.kind==ExpressionKind::Construct){cursor.phase=3;return SyntaxNeed::Type;}
  if(expression.kind==ExpressionKind::Call||expression.kind==ExpressionKind::Specialize){return SyntaxNeed::Call;}
  if(expression.kind==ExpressionKind::FunctionRef){return SyntaxNeed::FunctionRef;}
  if(expression.kind==ExpressionKind::Cooperate){return SyntaxNeed::Cooperate;}
  if(expression.kind==ExpressionKind::Target){return SyntaxNeed::Target;}
  if(expression.kind==ExpressionKind::Invoke){return SyntaxNeed::Invoke;}
  if(expression.kind==ExpressionKind::Unary&&name_is(expression.text,45u64,1u64)){
   ExpressionView child=host.inspect(expression.first);
   if(child.kind==ExpressionKind::Number){return SyntaxNeed::Negative;}
  }
  return SyntaxNeed::First;
 }
 if(cursor.phase==3){return SyntaxNeed::Construct;}
 if((facts&1)==0){return SyntaxNeed::Empty;}
 if(cursor.phase==4){return (facts&2)!=0 ? SyntaxNeed::Index : SyntaxNeed::InvalidIndex;}
 if(cursor.phase==2){return SyntaxNeed::Binary;}
 if(expression.kind==ExpressionKind::Member){return SyntaxNeed::Member;}
 if(expression.kind==ExpressionKind::Index){cursor.phase=4;return SyntaxNeed::IndexType;}
 if(expression.kind==ExpressionKind::Unary){return SyntaxNeed::Unary;}
 if(expression.kind==ExpressionKind::Select){return SyntaxNeed::Select;}
 if(name_is(expression.text,9766u64,2u64)||name_is(expression.text,31868u64,2u64)){return SyntaxNeed::ShortCircuit;}
 cursor.phase=2;return SyntaxNeed::Second;
}
extern "C" ValueCursor value_start(u32 flags) on cpu {return ValueCursor(0,flags);}
extern "C" ValueNeed value_step(inout ValueCursor cursor,u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect) {
 if(cursor.phase==0){
  cursor.phase=1;
  if((cursor.flags&1)!=0){return ValueNeed::Expected;}
 }
 if(cursor.phase==1){cursor.phase=2;return ValueNeed::Read;}
 if(cursor.phase==2){cursor.phase=3;return ValueNeed::Validate;}
 if(cursor.phase==3){
  cursor.phase=4;
  if(constant_result(facts,code,ir)==0){return ValueNeed::Reject;}
  if((cursor.flags&2)!=0){return ValueNeed::Sources;}
 }
 return (cursor.flags&4)!=0 ? ValueNeed::Reference : ValueNeed::Literal;
}

bool name_pair(Name name,u64 first,u64 second,usize length) on cpu {
 if(name.count!=length){return false;}
 usize index=0u64;
 while(index<length){
  u64 packed=index<8u64 ? first : second;
  u8 actual=unsafe_position<u8>(unsafe_address(address_bits(name.data)+u64(index)));
  if(actual!=u8(packed>>(u64(index%8u64)*8u64))){return false;}
  index+=1u64;
 }
 return true;
}
bool unsafe_position_name(Name name) on cpu {return name_pair(name,8097302145431465589u64,31084746153489263u64,15u64);}
extern "C" u32 is_place(address expression,address context,PlaceHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.binding) {
 address current=expression;
 while(true){
  ExpressionView view=host.inspect(current);
  if(view.kind==ExpressionKind::Name){return host.binding(context,view.text);}
  if(view.kind==ExpressionKind::Member){current=view.first;continue;}
  if(view.kind==ExpressionKind::Index){return 1;}
  return view.kind==ExpressionKind::Specialize&&unsafe_position_name(view.text) ? 1 : 0;
 }
}
extern "C" PlaceCursor place_start() on cpu {return PlaceCursor(0);}
extern "C" PlaceNeed place_step(inout PlaceCursor cursor,ExpressionView expression,u32 facts) on cpu {
 if(cursor.phase==0){
  if(expression.kind==ExpressionKind::Name){return PlaceNeed::Name;}
  if(expression.kind==ExpressionKind::Specialize&&unsafe_position_name(expression.text)){return PlaceNeed::External;}
  if(expression.kind==ExpressionKind::Member){cursor.phase=1;return PlaceNeed::Parent;}
  if(expression.kind==ExpressionKind::Index){cursor.phase=2;return PlaceNeed::BaseIsPlace;}
  return PlaceNeed::Invalid;
 }
 if(cursor.phase==1){return (facts&1)!=0 ? PlaceNeed::Member : PlaceNeed::Empty;}
 if(cursor.phase==2){
  if((facts&2)!=0){cursor.phase=3;return PlaceNeed::Parent;}
  cursor.phase=6;return PlaceNeed::BaseValue;
 }
 if(cursor.phase==3){
  if((facts&1)==0){return PlaceNeed::Empty;}
  cursor.phase=4;return PlaceNeed::IndexedType;
 }
 if(cursor.phase==4){
  if((facts&4)!=0){cursor.phase=5;return PlaceNeed::IndexValue;}
  cursor.phase=9;return PlaceNeed::ReadPlace;
 }
 if(cursor.phase==5){return (facts&1)!=0 ? PlaceNeed::ArrayIndex : PlaceNeed::Empty;}
 if(cursor.phase==6){
  if((facts&1)==0){return PlaceNeed::Empty;}
  cursor.phase=7;return PlaceNeed::ArrayType;
 }
 if(cursor.phase==7){
  if((facts&4)!=0){cursor.phase=8;return PlaceNeed::Materialize;}
  return PlaceNeed::ViewAt;
 }
 if(cursor.phase==8){cursor.phase=5;return PlaceNeed::IndexValue;}
 return PlaceNeed::ViewAt;
}
extern "C" ArgumentCursor argument_start(u32 access) on cpu {return ArgumentCursor(0,access);}
extern "C" ArgumentNeed argument_step(inout ArgumentCursor cursor,u32 facts) on cpu {
 if(cursor.phase==0){
  if(cursor.access!=0){cursor.phase=1;return ArgumentNeed::PlaceTest;}
  cursor.phase=3;return ArgumentNeed::Expected;
 }
 if(cursor.phase==1){
  if((facts&1)!=0){cursor.phase=2;return ArgumentNeed::Place;}
  cursor.phase=3;return ArgumentNeed::Expected;
 }
 if(cursor.phase==2){
  if((facts&2)==0){return ArgumentNeed::Empty;}
  return cursor.access==3 ? ArgumentNeed::AtomicPlace : ArgumentNeed::MutablePlace;
 }
 if(cursor.phase==3){cursor.phase=4;return ArgumentNeed::Read;}
 if(cursor.phase==4){cursor.phase=5;return ArgumentNeed::Flow;}
 if((facts&4)==0){return ArgumentNeed::Empty;}
 if(cursor.access==1||cursor.access==2){return ArgumentNeed::MutableReturned;}
 return cursor.access==3 ? ArgumentNeed::AtomicError : ArgumentNeed::Resource;
}
extern "C" ArgumentsCursor arguments_start(usize count) on cpu {return ArgumentsCursor(0u64,count,0);}
extern "C" ArgumentsNeed arguments_step(inout ArgumentsCursor cursor,u32 present) on cpu {
 if(cursor.phase==1){
  if(present==0){return ArgumentsNeed::Empty;}
  cursor.phase=2;return ArgumentsNeed::Append;
 }
 if(cursor.phase==2){cursor.index+=1u64;}
 if(cursor.index==cursor.count){return ArgumentsNeed::Finish;}
 cursor.phase=1;return ArgumentsNeed::Read;
}

extern "C" ShapeCursor shape_start(u32 kind,u32 source) on cpu {return ShapeCursor(0,kind,source);}
extern "C" ShapeNeed shape_step(inout ShapeCursor cursor,u32 present) on cpu {
 if(cursor.phase==0){
  if(cursor.kind==0){return ShapeNeed::Absent;}
  if(cursor.kind==1){return ShapeNeed::Literal;}
  if(cursor.kind==3){return ShapeNeed::Expression;}
  if(cursor.source!=0){cursor.phase=1;return ShapeNeed::Binding;}
  cursor.phase=2;return ShapeNeed::Resolve;
 }
 if(cursor.phase==1){
  if(present!=0){return ShapeNeed::Bound;}
  cursor.phase=2;return ShapeNeed::Resolve;
 }
 if(cursor.phase==2){
  if((present&3)!=3){return ShapeNeed::Unknown;}
  cursor.phase=3;return ShapeNeed::Evaluate;
 }
 return present!=0 ? ShapeNeed::Constant : ShapeNeed::InvalidType;
}
extern "C" IntegerCursor integer_start() on cpu {return IntegerCursor(0);}
extern "C" IntegerNeed integer_step(inout IntegerCursor cursor,u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect) {
 if(cursor.phase==0){cursor.phase=1;return IntegerNeed::Read;}
 if(cursor.phase==1){cursor.phase=2;return IntegerNeed::Validate;}
 return constant_result(facts,code,ir)!=0 ? IntegerNeed::Finish : IntegerNeed::Reject;
}

bool called_Ok(Name name) on cpu {return name_pair(name,27471u64,0u64,2u64);}
bool called_Err(Name name) on cpu {return name_pair(name,7500357u64,0u64,3u64);}
bool called_unsafe_function(Name name) on cpu {return name_pair(name,7376726205052186229u64,31084746153094773u64,15u64);}
bool called_unsafe_position(Name name) on cpu {return name_pair(name,8097302145431465589u64,31084746153489263u64,15u64);}
bool called_view(Name name) on cpu {return name_pair(name,2003134838u64,0u64,4u64);}
bool called_dataset(Name name) on cpu {return name_pair(name,32762643529097572u64,0u64,7u64);}
bool called_array(Name name) on cpu {return name_pair(name,521325933153u64,0u64,5u64);}
bool called_owned(Name name) on cpu {return name_pair(name,431198467951u64,0u64,5u64);}
bool called_size_of(Name name) on cpu {return name_pair(name,28832903139584371u64,0u64,7u64);}
bool called_align_of(Name name) on cpu {return name_pair(name,7381223242069535841u64,0u64,8u64);}
bool called_c_function(Name name) on cpu {return name_pair(name,7598807797348065123u64,28271u64,10u64);}
bool called_println(Name name) on cpu {return name_pair(name,31081494762582640u64,0u64,7u64);}
bool called_unsafe_read(Name name) on cpu {return name_pair(name,8241417333507321461u64,6578533u64,11u64);}
bool called_index(Name name) on cpu {return name_pair(name,517097156201u64,0u64,5u64);}
bool called_ordinal(Name name) on cpu {return name_pair(name,30506424327238255u64,0u64,7u64);}

extern "C" CallCursor call_start() on cpu {return CallCursor(0,0,0,0u64);}
extern "C" CallNeed call_step(inout CallCursor cursor,CallInput input,CallFacts facts,CallHost host) on cpu
 requires nonzero(host.type_argument) {
 while(true){
  if(cursor.phase==0){cursor.phase=1;return CallNeed::VariantLookup;}
  if(cursor.phase==1){
   if(facts.present!=0){cursor.phase=3;return CallNeed::Owns;}
   if(called_Ok(input.name)||called_Err(input.name)){
    cursor.variant=called_Err(input.name) ? 1u64 : 0u64;
    cursor.phase=2;
    if(input.explicit!=0){
     if(input.type_count!=2u64||host.type_argument(input.types,0u64)==0||host.type_argument(input.types,1u64)==0){return CallNeed::InvalidResultTypes;}
     return CallNeed::ResultType;
    }
    return input.expected_result!=0 ? CallNeed::ExpectedResult : CallNeed::MissingResultContext;
   }
   cursor.phase=4;continue;
  }
  if(cursor.phase==2){cursor.phase=3;return CallNeed::Owns;}
  if(cursor.phase==3){return facts.owns!=0 ? CallNeed::ResourceError : CallNeed::Variant;}
  if(cursor.phase==4){
   if(input.explicit!=0){
    if(called_unsafe_function(input.name)){return CallNeed::FunctionFrom;}
    if(called_unsafe_position(input.name)){cursor.phase=16;return CallNeed::PositionFrom;}
    if(called_view(input.name)||called_dataset(input.name)){return CallNeed::InvalidConst;}
    if(called_array(input.name)||called_owned(input.name)){cursor.phase=15;return CallNeed::ExplicitType;}
   }else{
    if(called_array(input.name)||called_owned(input.name)){cursor.phase=6;return CallNeed::CheckType;}
   }
   cursor.phase=5;return CallNeed::ResolveType;
  }
  if(cursor.phase==5){
   cursor.type_known=1;cursor.type_visible=facts.present;
   if(facts.present!=0){
    if(input.explicit!=0){cursor.phase=15;return CallNeed::ExplicitType;}
    cursor.phase=6;return CallNeed::CheckType;
   }
   cursor.phase=18;continue;
  }
  if(cursor.phase==6){cursor.phase=7;return CallNeed::CheckOwns;}
  if(cursor.phase==7){
   if(facts.owns!=0){return CallNeed::ResourceError;}
   cursor.phase=18;continue;
  }
  if(cursor.phase==18){cursor.phase=8;return CallNeed::ResolveFunction;}
  if(cursor.phase==8){
   if(facts.resolution==1){cursor.phase=9;return CallNeed::ParameterCount;}
   if(facts.resolution==2){return CallNeed::Ambiguous;}
   cursor.phase=12;continue;
  }
  if(cursor.phase==9){
   if(facts.parameters!=input.arg_count){return CallNeed::InvalidArgumentCount;}
   cursor.phase=10;return CallNeed::Arguments;
  }
  if(cursor.phase==10){
   if(facts.present==0){return CallNeed::Empty;}
   cursor.phase=11;return CallNeed::Instance;
  }
  if(cursor.phase==11){return CallNeed::Invoke;}
  if(cursor.phase==12){
   if(input.explicit!=0){
    if(called_size_of(input.name)||called_align_of(input.name)){
     if(input.type_count!=1u64||input.arg_count!=0u64||host.type_argument(input.types,0u64)==0){return CallNeed::InvalidLayoutArguments;}
     cursor.phase=17;return CallNeed::LayoutType;
    }
    return CallNeed::UnknownGeneric;
   }
   if(called_c_function(input.name)){return CallNeed::CFunction;}
   if(called_println(input.name)){return CallNeed::Println;}
   cursor.phase=13;return CallNeed::Subgroup;
  }
  if(cursor.phase==13){
   if(facts.present!=0){return CallNeed::SubgroupValue;}
   cursor.phase=19;continue;
  }
  if(cursor.phase==19){
   if(called_unsafe_read(input.name)){
    if(input.arg_count!=2u64){return CallNeed::InvalidReadArguments;}
    cursor.phase=14;return CallNeed::ReadView;
   }
   if(cursor.type_known==0){cursor.phase=20;return CallNeed::ResolveType;}
   cursor.phase=21;continue;
  }
  if(cursor.phase==14){return facts.present!=0 ? CallNeed::UnsafeRead : CallNeed::Empty;}
  if(cursor.phase==15){return CallNeed::Construct;}
  if(cursor.phase==16){return facts.present!=0 ? CallNeed::ReadPosition : CallNeed::Empty;}
  if(cursor.phase==17){return called_size_of(input.name) ? CallNeed::LayoutSize : CallNeed::LayoutAlign;}
  if(cursor.phase==20){cursor.type_known=1;cursor.type_visible=facts.present;cursor.phase=21;continue;}
  if(cursor.type_visible!=0){cursor.phase=15;return CallNeed::FinalType;}
  if(called_index(input.name)){return CallNeed::IndexContext;}
  if(called_ordinal(input.name)){return CallNeed::OrdinalContext;}
  return CallNeed::Builtin;
 }
}
extern "C" CReferenceNeed c_reference(Span args,ExpressionHost host) on cpu requires nonzero(host.inspect) {
 if(args.count!=1u64){return CReferenceNeed::Count;}
 ExpressionView first=host.inspect(args.data);
 if(first.kind==ExpressionKind::Name){return CReferenceNeed::Name;}
 return first.kind==ExpressionKind::FunctionRef ? CReferenceNeed::FunctionRef : CReferenceNeed::Invalid;
}
