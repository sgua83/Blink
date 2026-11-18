module bootstrap.source_call;
import bootstrap.ast_storage;
import bootstrap.signatures;

address at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
extern "C" CallSyntax inspect(SignatureNode source) on cpu {
 ExprNode expression=unsafe_position<ExprNode>(at(source.storage.nodes,source.offset));
 CallSyntax result=CallSyntax(CallForm::Other,Text(0,Span(0u64,0u64)),Optional<Slice>(0,Slice(0u64,0u64)),Slice(0u64,0u64),ExprRef(0u64));
 match(expression.kind){
  ExprKind::Call(value){result.form=CallForm::Named;result.name=value.name;result.arguments=value.arguments;}
  ExprKind::Specialize(value){result.form=CallForm::Explicit;result.name=value.name;result.types=Optional<Slice>(1,value.types);result.arguments=value.arguments;}
  ExprKind::Invoke(value){result.form=CallForm::Indirect;result.arguments=value.arguments;result.callee=value.callee;}
  ExprKind::Number(unused){}
  ExprKind::StaticU32(unused){}
  ExprKind::Bool(unused){}
  ExprKind::Name(unused){}
  ExprKind::Member(unused){}
  ExprKind::Index(unused){}
  ExprKind::Construct(unused){}
  ExprKind::FunctionRef(unused){}
  ExprKind::Cooperate(unused){}
  ExprKind::Target(unused){}
  ExprKind::Unary(unused){}
  ExprKind::Binary(unused){}
  ExprKind::Select(unused){}
 }
 return result;
}
bool name_is(SignatureNode source,Text name,u64 first,u64 second,u64 length) on cpu {
 if(name.range.end-name.range.start!=length){return false;}
 address base=name.kind==0 ? source.storage.source : source.storage.text;
 address bytes=at(base,name.range.start);u64 index=0u64;
 while(index<length){
  u64 packed=index<8u64 ? first : second;
  if(unsafe_position<u8>(at(bytes,index))!=u8(packed>>((index%8u64)*8u64))){return false;}
  index+=1u64;
 }
 return true;
}
bool called_unsafe_function(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7376726205052186229u64,31084746153094773u64,15u64);}
bool called_unsafe_position(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,8097302145431465589u64,31084746153489263u64,15u64);}
bool called_view(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,2003134838u64,0u64,4u64);}
bool called_Ok(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,27471u64,0u64,2u64);}
bool called_Err(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7500357u64,0u64,3u64);}
bool called_array(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,521325933153u64,0u64,5u64);}
bool called_owned(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,431198467951u64,0u64,5u64);}
bool called_size_of(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,28832903139584371u64,0u64,7u64);}
bool called_align_of(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7381223242069535841u64,0u64,8u64);}

extern "C" SourceCallCursor specialized_start() on cpu {return SourceCallCursor(0);}
extern "C" SpecializedNeed specialized_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,BoundArguments arguments,CallFacts facts,CallHost host) on cpu
 requires nonzero(host.is_type) {
 if(cursor.phase==0){cursor.phase=1;return SpecializedNeed::Bind;}
 if(cursor.phase==1||cursor.phase==11||cursor.phase==12){
  bool prepared=cursor.phase!=1;bool resolved=cursor.phase==12;
  if(called_unsafe_function(source,syntax)){return SpecializedNeed::UnsafeFunction;}
  if(called_unsafe_position(source,syntax)){cursor.phase=2;return SpecializedNeed::UnsafePosition;}
  if(called_view(source,syntax)){return SpecializedNeed::View;}
  if(resolved){
   if(called_Ok(source,syntax)||called_Err(source,syntax)){cursor.phase=8;return SpecializedNeed::Result;}
   if(facts.present!=0){cursor.phase=8;return SpecializedNeed::Record;}
   cursor.phase=13;
  }else{cursor.phase=3;if(!prepared){return SpecializedNeed::VariantType;}}
 }
 if(cursor.phase==2){return facts.present!=0 ? SpecializedNeed::ReadPosition : SpecializedNeed::Empty;}
 if(cursor.phase==3){
  if(facts.present!=0){return facts.owned!=0 ? SpecializedNeed::VariantOwned : SpecializedNeed::VariantValue;}
  if(called_Ok(source,syntax)||called_Err(source,syntax)){cursor.phase=8;return SpecializedNeed::Result;}
  if(called_array(source,syntax)||called_owned(source,syntax)){cursor.phase=8;return SpecializedNeed::Record;}
  cursor.phase=4;return SpecializedNeed::RecordName;
 }
 if(cursor.phase==4){
  if(facts.present!=0){cursor.phase=8;return SpecializedNeed::Record;}
  cursor.phase=5;return SpecializedNeed::FunctionName;
 }
 if(cursor.phase==5||cursor.phase==13){
  if(cursor.phase==5&&facts.present!=0){cursor.phase=6;return SpecializedNeed::Arguments;}
  if(called_size_of(source,syntax)||called_align_of(source,syntax)){
   if(arguments.count!=1u64||syntax.arguments.count!=0u64||host.is_type(arguments.data,0u64)==0){return SpecializedNeed::InvalidLayout;}
   cursor.phase=10;return SpecializedNeed::LayoutType;
  }
  return SpecializedNeed::Unknown;
 }
 if(cursor.phase==6){
  if(facts.present==0){cursor.phase=9;return SpecializedNeed::Flow;}
  cursor.phase=7;return SpecializedNeed::Instance;
 }
 if(cursor.phase==7){cursor.phase=8;return SpecializedNeed::Invoke;}
 if(cursor.phase==8){cursor.phase=9;return SpecializedNeed::Flow;}
 if(cursor.phase==10){cursor.phase=8;return called_size_of(source,syntax) ? SpecializedNeed::LayoutSize : SpecializedNeed::LayoutAlign;}
 return facts.continuing!=0 ? SpecializedNeed::Value : SpecializedNeed::Empty;
}

extern "C" SourceCallCursor expected_start(SignatureNode source,CallSyntax syntax) on cpu {
 SourceCallCursor cursor=SourceCallCursor(4);
 if(syntax.form==CallForm::Explicit){cursor.phase=0;return cursor;}
 if(syntax.form==CallForm::Named){cursor.phase=1;return cursor;}
 ExprNode expression=unsafe_position<ExprNode>(at(source.storage.nodes,source.offset));
 match(expression.kind){
  ExprKind::Select(unused){cursor.phase=5;}
  ExprKind::Number(unused){}
  ExprKind::StaticU32(unused){}
  ExprKind::Bool(unused){}
  ExprKind::Name(unused){}
  ExprKind::Member(unused){}
  ExprKind::Index(unused){}
  ExprKind::Call(unused){}
  ExprKind::Invoke(unused){}
  ExprKind::Construct(unused){}
  ExprKind::Specialize(unused){}
  ExprKind::FunctionRef(unused){}
  ExprKind::Cooperate(unused){}
  ExprKind::Target(unused){}
  ExprKind::Unary(unused){}
  ExprKind::Binary(unused){}
 }
 return cursor;
}
extern "C" ExpectedNeed expected_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,ExpectedFacts facts) on cpu {
 if(cursor.phase==0){cursor.phase=1;return ExpectedNeed::Bind;}
 if(cursor.phase==1){cursor.phase=2;return ExpectedNeed::VariantType;}
 if(cursor.phase==2){
  if(facts.present!=0){return facts.owned!=0 ? ExpectedNeed::VariantOwned : ExpectedNeed::VariantValue;}
  if(syntax.form==CallForm::Named&&(called_Ok(source,syntax)||called_Err(source,syntax))){
   if(facts.result==0){return ExpectedNeed::MissingResult;}
   return called_Ok(source,syntax) ? ExpectedNeed::ResultOk : ExpectedNeed::ResultErr;
  }
  if(syntax.form==CallForm::Explicit){cursor.phase=11;return ExpectedNeed::Specialized;}
  return ExpectedNeed::Expression;
 }
 return cursor.phase==5 ? ExpectedNeed::Selection : ExpectedNeed::Expression;
}

bool called_dataset(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,32762643529097572u64,0u64,7u64);}
bool called_start_task(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7022342672717280371u64,27507u64,10u64);}
bool called_wait_task(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,8314054340426424695u64,107u64,9u64);}
bool called_try(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7959156u64,0u64,3u64);}
bool called_resize(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,111576429585778u64,0u64,6u64);}
bool called_take_last(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,8314045544081875316u64,116u64,9u64);}
bool called_own(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7239535u64,0u64,3u64);}
bool called_into_inner(SignatureNode source,CallSyntax syntax) on cpu {return name_is(source,syntax.name,7957413450222956137u64,29285u64,10u64);}

ReturnedNeed returned_record(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax) on cpu {
 if(called_array(source,syntax)||called_owned(source,syntax)){cursor.phase=14;return ReturnedNeed::RecordType;}
 cursor.phase=13;return ReturnedNeed::RecordName;
}
extern "C" SourceCallCursor returned_start() on cpu {return SourceCallCursor(0);}
extern "C" ReturnedNeed returned_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,ReturnedFacts facts) on cpu {
 if(cursor.phase==0||cursor.phase==30){
  bool prepared=cursor.phase==30;
  if(syntax.form==CallForm::Other){return ReturnedNeed::Other;}
  if(syntax.form==CallForm::Indirect){return ReturnedNeed::Indirect;}
  if(syntax.form==CallForm::Explicit){cursor.phase=5;return ReturnedNeed::Bind;}
  if(called_start_task(source,syntax)||called_wait_task(source,syntax)){return ReturnedNeed::Task;}
  cursor.phase=prepared ? 31 : 1;return ReturnedNeed::TypeCall;
 }
 if(cursor.phase==1||cursor.phase==31){
  if(facts.kind==1){return ReturnedNeed::AliasBuiltin;}
  if(facts.kind==2){cursor.phase=40;return ReturnedNeed::AliasType;}
  cursor.phase=cursor.phase==31 ? 32 : 2;return ReturnedNeed::Environment;
 }
 if(cursor.phase==40){
  if(facts.kind!=0){return ReturnedNeed::TaskType;}
  return facts.present!=0 ? ReturnedNeed::OwnedRecord : ReturnedNeed::RecordValue;
 }
 if(cursor.phase==2||cursor.phase==32){
  if(facts.present!=0){return ReturnedNeed::LocalFunction;}
  bool prepared=cursor.phase==32;cursor.phase=6;
  if(!prepared){return ReturnedNeed::VariantType;}
 }
 if(cursor.phase==5){cursor.phase=6;return ReturnedNeed::VariantType;}
 if(cursor.phase==6){
  // A prepared named entry reaches here after an absent constructor decision.
  if(facts.present!=0){return facts.kind!=0 ? ReturnedNeed::VariantOwned : ReturnedNeed::VariantValue;}
  if(syntax.form==CallForm::Explicit){
   if(called_dataset(source,syntax)){return ReturnedNeed::Dataset;}
   if(called_Ok(source,syntax)||called_Err(source,syntax)){cursor.phase=10;return ReturnedNeed::ResultType;}
  }else{
   if((called_Ok(source,syntax)||called_Err(source,syntax))&&facts.expected!=0){cursor.phase=7;return ReturnedNeed::ExpectedOwns;}
   if(called_try(source,syntax)){return ReturnedNeed::Propagate;}
  }
  return returned_record(cursor,source,syntax);
 }
 if(cursor.phase==7){
  if(facts.present!=0){return ReturnedNeed::ExpectedOwnedResult;}
  return returned_record(cursor,source,syntax);
 }
 if(cursor.phase==10){
  if(facts.present!=0){return ReturnedNeed::OwnedResult;}
  return returned_record(cursor,source,syntax);
 }
 if(cursor.phase==13){
  if(facts.present!=0){cursor.phase=14;return ReturnedNeed::RecordType;}
  cursor.phase=16;return ReturnedNeed::FunctionName;
 }
 if(cursor.phase==14){
  if(facts.present!=0){return ReturnedNeed::OwnedRecord;}
  cursor.phase=16;return ReturnedNeed::FunctionName;
 }
 if(cursor.phase==16){
  if(facts.kind==1){cursor.phase=17;return ReturnedNeed::Arguments;}
  if(facts.kind==2){return ReturnedNeed::Ambiguous;}
  cursor.phase=20;
 }
 if(cursor.phase==17){
  if(facts.present==0){return ReturnedNeed::Empty;}
  cursor.phase=18;return ReturnedNeed::Instance;
 }
 if(cursor.phase==18){return ReturnedNeed::Invoke;}
 if(syntax.form==CallForm::Explicit){return ReturnedNeed::Fallback;}
 if(called_Ok(source,syntax)||called_Err(source,syntax)){return facts.expected==2 ? ReturnedNeed::ExpectedResult : ReturnedNeed::MissingResult;}
 if(called_resize(source,syntax)){return ReturnedNeed::Resize;}
 if(called_take_last(source,syntax)){return ReturnedNeed::TakeLast;}
 if(called_own(source,syntax)){return ReturnedNeed::Own;}
 if(called_into_inner(source,syntax)){return ReturnedNeed::IntoInner;}
 return ReturnedNeed::Builtin;
}
