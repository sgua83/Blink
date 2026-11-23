//! Source call control borrows the same sealed syntax and actual bound results.
//! B selects each operation; original World children run only after native
//! stepping returns, with the original typed products held by this future.
use super::*;
use crate::syntax::wire;
use std::ptr;
#[derive(Clone,Copy)]
#[repr(u32)]
enum CallForm{Other,Named,Explicit,Indirect}
#[derive(Clone,Copy)]
#[repr(C)]
struct CallSyntax{form:CallForm,name:wire::Text,types:wire::Optional<wire::Slice>,arguments:wire::Slice,callee:wire::ExprRef}
#[repr(C)]
struct SourceCallCursor{phase:u32}
struct Call{source:wire::SignatureNode,syntax:CallSyntax,cursor:SourceCallCursor}
#[repr(C)]
struct BoundArguments{data:*const GenericArgument,count:u64}
#[repr(C)]
struct CallFacts{present:u32,continuing:u32,owned:u32}
#[repr(C)]
struct CallHost{is_type:unsafe extern "C" fn(*const GenericArgument,u64)->u32}
#[repr(u32)]
enum SpecializedNeed{Bind,UnsafeFunction,UnsafePosition,ReadPosition,View,VariantType,VariantValue,Result,RecordName,Record,FunctionName,Arguments,Instance,Invoke,LayoutType,LayoutSize,LayoutAlign,InvalidLayout,Unknown,Flow,Value,Empty,VariantOwned}
unsafe extern "C"{
 #[link_name="bootstrap.source_call::specialized_start"]
 fn b_specialized_start()->SourceCallCursor;
 #[link_name="bootstrap.source_call::inspect"]
 fn b_inspect(source:wire::SignatureNode)->CallSyntax;
 #[link_name="bootstrap.source_call::specialized_step"]
 fn b_specialized_step(cursor:*mut SourceCallCursor,source:wire::SignatureNode,syntax:CallSyntax,arguments:BoundArguments,facts:CallFacts,host:CallHost)->SpecializedNeed;
}
unsafe extern "C" fn is_type(arguments:*const GenericArgument,index:u64)->u32{
 u32::from(matches!(unsafe{&*arguments.add(index as usize)},GenericArgument::Type(_)))
}
unsafe fn type_argument(argument:&GenericArgument)->&TypeName{
 match argument{GenericArgument::Type(ty)=>ty,_=>unsafe{std::hint::unreachable_unchecked()}}
}
pub(super) async fn specialized(builder:&mut Builder<'_>,expression:&Expr)->Result<Option<Value>>{
 let source=expression.signature_node();
 let cursor=Call{source,syntax:unsafe{b_inspect(source)},cursor:unsafe{b_specialized_start()}};
 specialized_with(builder,expression,cursor,None,None).await
}
async fn specialized_with(builder:&mut Builder<'_>,expression:&Expr,mut cursor:Call,
 mut explicit:Option<TypeName>,mut prepared:Option<Ty>)->Result<Option<Value>>{
 let world=builder.world;
 let name=expression.source_view().text(cursor.syntax.name);
 let args=expression.list::<wire::ExprNode>(cursor.syntax.arguments);
 let mut variant:Option<(Ty,usize)>=None;let mut place:Option<Place>=None;
 let mut key:Option<String>=None;let mut arguments:Option<Vec<cooperative::Argument>>=None;
 let mut instance=None;let mut ty:Option<Ty>=None;let mut value:Option<Value>=None;
 let mut facts=CallFacts{present:u32::from(prepared.is_some()),continuing:0,owned:0};
 loop{
  let bound=explicit.as_ref().map_or(BoundArguments{data:ptr::null(),count:0},|value|
   BoundArguments{data:value.arguments.as_ptr(),count:value.arguments.len()as u64});
  match unsafe{b_specialized_step(&mut cursor.cursor,cursor.source,cursor.syntax,bound,CallFacts{..facts},CallHost{is_type})}{
   SpecializedNeed::Bind=>{
    let source=expression.list::<wire::ArgumentNode>(cursor.syntax.types.value);
    let mut bound=TypeName::named(name);
    bound.arguments=unsafe{world.call(builder.scope.bindings().arguments(world,&source,
     (builder.scope.source(),builder.scope.module(),builder.scope.imports()),expression.span())).await}?;
    explicit=Some(bound);
   }
   SpecializedNeed::UnsafeFunction=>return unsafe{world.call(builder.function_from_address(
    explicit.as_ref().unwrap_unchecked().arguments.as_slice(),args.len(),expression.span(),args.first().map(|arg|arg.span()).unwrap_or(expression.span()),
    async |builder|unsafe{builder.world.call(builder.expr(&args.at(0))).await})).await},
   SpecializedNeed::UnsafePosition=>{
    place=unsafe{world.call(builder.external_position(explicit.as_ref().unwrap_unchecked().arguments.as_slice(),&args,expression.span())).await}?;
    facts.present=u32::from(place.is_some());
   }
   SpecializedNeed::ReadPosition=>return unsafe{world.call(builder.read_place(place.as_ref().unwrap_unchecked(),expression.span(),None)).await}.map(Some),
   SpecializedNeed::View=>return unsafe{world.call(builder.create_application_view(explicit.as_ref().unwrap_unchecked().arguments.as_slice(),&args,expression.span())).await},
   SpecializedNeed::VariantType=>{
    variant=unsafe{world.call(builder.variant_constructor_type_with(name,None,expression.span(),|parent|explicit.take().map(|mut syntax|{syntax.name=parent.into();syntax}))).await}?;
    facts.present=u32::from(variant.is_some());facts.owned=u32::from(variant.as_ref().is_some_and(|(ty,_)|ty.owns_resources()));
   }
   need @ (SpecializedNeed::VariantValue|SpecializedNeed::VariantOwned)=>{
    let(ty,index)=unsafe{variant.take().unwrap_unchecked()};
    let returned=unsafe{world.call(builder.prepared_variant_returned(expression,ty,index,&args,matches!(need,SpecializedNeed::VariantOwned))).await}?;
    return builder.ordinary_return(returned,expression.span());
   }
   SpecializedNeed::Result=>{
    let actual=if let Some(ty)=prepared.take(){ty}else{
     let mut syntax=unsafe{explicit.take().unwrap_unchecked()};syntax.name="result".into();
     unsafe{world.call(world.ty(&syntax,builder.scope.source(),expression.span(),builder.scope.module(),builder.scope.imports())).await}?
    };
    value=unsafe{world.call(builder.value_variant_constructor(actual,usize::from(name!="Ok"),&args,expression.span())).await}?;
   }
   SpecializedNeed::RecordName=>{facts.present=u32::from(!world.resolve(name,builder.scope.module(),builder.scope.imports(),false).is_missing());}
   SpecializedNeed::Record=>{
    let actual=if let Some(ty)=prepared.take(){ty}else{
     let syntax=unsafe{explicit.take().unwrap_unchecked()};
     unsafe{world.call(world.ty(&syntax,builder.scope.source(),expression.span(),builder.scope.module(),builder.scope.imports())).await}?
    };
    value=unsafe{world.call(builder.record_values(actual,args.len(),expression.span(),async |builder,index,expected|
     unsafe{builder.world.call(builder.expr_expected(&args.at(index),expected)).await})).await}?;
   }
   SpecializedNeed::FunctionName=>{
    key=world.resolve(name,builder.scope.module(),builder.scope.imports(),true).unique();
    facts.present=u32::from(key.is_some());
   }
   SpecializedNeed::Arguments=>{
    arguments=unsafe{world.call(builder.function_arguments(key.as_ref().unwrap_unchecked(),&args,expression.span())).await}?;
    facts.present=u32::from(arguments.is_some());
   }
   SpecializedNeed::Instance=>{
    instance=Some(unsafe{world.call(builder.specialized_arguments(key.take().unwrap_unchecked(),
     arguments.as_ref().unwrap_unchecked(),Some(explicit.as_ref().unwrap_unchecked().arguments.as_slice()),expression.span())).await}?);
    // Instance owns its final bindings; the original borrowed explicit input
    // has no consumer after this same specialization completes.
    explicit.take();
   }
   SpecializedNeed::Invoke=>{
    value=unsafe{world.call(builder.invoke_instance(instance.take().unwrap_unchecked(),
     arguments.take().unwrap_unchecked(),expression.span(),Some(&args))).await}?;
   }
   SpecializedNeed::LayoutType=>{
    let syntax=unsafe{type_argument(explicit.as_ref().unwrap_unchecked().arguments.first().unwrap_unchecked())};
    ty=Some(unsafe{world.call(builder.expected_storage(syntax,expression.span())).await}?);
   }
   SpecializedNeed::LayoutSize=>{value=Some(builder.constant(Scalar::U64,Bits::from(unsafe{ty.as_ref().unwrap_unchecked()}.size)));}
   SpecializedNeed::LayoutAlign=>{value=Some(builder.constant(Scalar::U64,Bits::from(unsafe{ty.as_ref().unwrap_unchecked()}.align)));}
   SpecializedNeed::InvalidLayout=>return builder.fail(expression.span(),"B153",format!("{name}<T>()需要一个明确类型且不接收值实参")),
   SpecializedNeed::Unknown=>return Err(Diagnostic{source:builder.scope.source().clone(),span:expression.span(),code:"B158",message:format!("未知或未导出的泛型函数 {name}")}),
   SpecializedNeed::Flow=>{facts.continuing=u32::from(builder.flow.continues());}
   SpecializedNeed::Value=>return builder.value_result(value.take(),expression.span()).map(Some),
   SpecializedNeed::Empty=>return Ok(None),
  }
 }
}

#[repr(C)]
struct ExpectedFacts{present:u32,result:u32,owned:u32}
#[repr(u32)]
enum ExpectedNeed{Bind,VariantType,VariantValue,ResultOk,ResultErr,MissingResult,Specialized,Expression,Selection,VariantOwned}
unsafe extern "C"{
 #[link_name="bootstrap.source_call::expected_start"]
 fn b_expected_start(source:wire::SignatureNode,syntax:CallSyntax)->SourceCallCursor;
 #[link_name="bootstrap.source_call::expected_step"]
 fn b_expected_step(cursor:*mut SourceCallCursor,source:wire::SignatureNode,syntax:CallSyntax,facts:ExpectedFacts)->ExpectedNeed;
}
pub(super) async fn expected(builder:&mut Builder<'_>,expression:&Expr,expected:Option<&Ty>)->Result<Option<Value>>{
 let world=builder.world;let source=expression.signature_node();let syntax=unsafe{b_inspect(source)};
 let mut cursor=Call{source,syntax,cursor:unsafe{b_expected_start(source,syntax)}};
 let mut explicit:Option<TypeName>=None;
 let mut constructor:Option<(Ty,usize)>=None;
 let mut facts=ExpectedFacts{present:0,result:u32::from(expected.is_some_and(|ty|ty.result)),owned:0};
 loop{
  match unsafe{b_expected_step(&mut cursor.cursor,cursor.source,cursor.syntax,ExpectedFacts{..facts})}{
   ExpectedNeed::Bind=>{
    let source=expression.list::<wire::ArgumentNode>(cursor.syntax.types.value);
    let mut bound=TypeName::named(expression.source_view().text(cursor.syntax.name));
    bound.arguments=unsafe{world.call(builder.scope.bindings().arguments(world,&source,
     (builder.scope.source(),builder.scope.module(),builder.scope.imports()),expression.span())).await}?;
    explicit=Some(bound);
   }
   ExpectedNeed::VariantType=>{
    let name=expression.source_view().text(cursor.syntax.name);
    constructor=unsafe{world.call(builder.variant_constructor_type_with(name,expected,expression.span(),|parent|explicit.take().map(|mut syntax|{syntax.name=parent.into();syntax}))).await}?;
    facts.present=u32::from(constructor.is_some());facts.owned=u32::from(constructor.as_ref().is_some_and(|(ty,_)|ty.owns_resources()));
   }
   need @ (ExpectedNeed::VariantValue|ExpectedNeed::VariantOwned)=>{
    let args=expression.list::<wire::ExprNode>(cursor.syntax.arguments);
    let(ty,index)=unsafe{constructor.take().unwrap_unchecked()};
    let value=unsafe{world.call(builder.prepared_variant_returned(expression,ty,index,&args,matches!(need,ExpectedNeed::VariantOwned))).await}?;
    return builder.ordinary_return(value,expression.span());
   }
   need @ (ExpectedNeed::ResultOk|ExpectedNeed::ResultErr)=>{
    let args=expression.list::<wire::ExprNode>(cursor.syntax.arguments);
    return unsafe{world.call(builder.value_variant_constructor(expected.unwrap_unchecked().clone(),
     usize::from(!matches!(need,ExpectedNeed::ResultOk)),&args,expression.span())).await};
   }
   ExpectedNeed::MissingResult=>return builder.fail(expression.span(),"B269",
    "Ok／Err 需要已声明的 result<T,E> 上下文，或显式给出两个类型实参"),
   ExpectedNeed::Specialized=>{
    // This same B cursor already owns the completed binding and absent
    // constructor decision. Enter the original value SourceSite exactly once.
    let mut value=unsafe{world.call(builder.at_source(expression.span(),SourceSyntax::Expression,async |builder|{
     builder.view_uses.before(expression.span());
     specialized_with(builder,expression,cursor,explicit,None).await
    })).await}?;
    if let Some(value)=&mut value{builder.bound_value(value);}
    return Ok(value);
   }
   ExpectedNeed::Expression=>{
     if matches!(cursor.syntax.form,CallForm::Named){
      cursor.cursor=SourceCallCursor{phase:30};
      return named_value_scoped(builder,expression,cursor).await;
     }
     return unsafe{world.call(builder.expr(expression)).await};
    },
   ExpectedNeed::Selection=>{
    let ExprKind::Select(condition,yes,no)=expression.kind()else{unsafe{std::hint::unreachable_unchecked()}};
    return unsafe{world.call(builder.selection_expected(&condition,&yes,&no,expression.span(),expected)).await};
   }
  }
 }
}


#[repr(C)]
struct ReturnedFacts{present:u32,kind:u32,expected:u32}
#[repr(u32)]
enum ReturnedNeed{Other,Task,TypeCall,AliasBuiltin,AliasType,Environment,Indirect,LocalFunction,Bind,VariantType,VariantValue,Dataset,ExpectedOwns,ExpectedOwnedResult,ResultType,OwnedResult,RecordName,RecordType,OwnedRecord,FunctionName,Arguments,Instance,Invoke,Ambiguous,Empty,Fallback,ExpectedResult,MissingResult,Resize,TakeLast,Own,IntoInner,Builtin,Propagate,VariantOwned,TaskType,RecordValue}
unsafe extern "C"{
 #[link_name="bootstrap.source_call::returned_start"]
 fn b_returned_start()->SourceCallCursor;
 #[link_name="bootstrap.source_call::returned_step"]
 fn b_returned_step(cursor:*mut SourceCallCursor,source:wire::SignatureNode,syntax:CallSyntax,facts:ReturnedFacts)->ReturnedNeed;
}
pub(super) async fn returned(builder:&mut Builder<'_>,expression:&Expr,expected:Option<&Ty>)->Result<Option<returns::Returned>>{
 let source=expression.signature_node();
 let call=Call{source,syntax:unsafe{b_inspect(source)},cursor:unsafe{b_returned_start()}};
 returned_with(builder,expression,expected,call).await
}
async fn returned_with(builder:&mut Builder<'_>,expression:&Expr,expected:Option<&Ty>,mut call:Call)->Result<Option<returns::Returned>>{
 use returns::Returned;
 let world=builder.world;
 // The original instance owns a possible alias throughout this synchronous call.
 let bindings=builder.scope.bindings().clone();
 let mut alias=None;
 let mut explicit:Option<TypeName>=None;
 let mut constructor=None;let mut result_ty:Option<Ty>=None;let mut record_ty:Option<Ty>=None;
 let mut key=None;let mut arguments=None;let mut instance=None;
 let mut facts=ReturnedFacts{present:0,kind:0,expected:expected.map_or(0,|ty|if ty.result{2}else{1})};
 loop{
  let need=unsafe{b_returned_step(&mut call.cursor,call.source,call.syntax,ReturnedFacts{..facts})};
  if matches!(need,ReturnedNeed::Other){
   return unsafe{world.call(builder.evaluate_returned_other(expression,expected)).await};
  }
  // Only call forms expose these offsets; no name/argument view is made for Other.
  let name=expression.source_view().text(call.syntax.name);
  let args=expression.list::<wire::ExprNode>(call.syntax.arguments);
  match need{
   ReturnedNeed::Other=>unsafe{std::hint::unreachable_unchecked()},
   ReturnedNeed::Task=>return unsafe{world.call(builder.task_call(name,&args,expression.span())).await},
   ReturnedNeed::TypeCall=>{
    alias=bindings.type_call(name);
    facts.kind=match alias{None=>0,Some(crate::generics::TypeCall::Builtin(_))=>1,Some(crate::generics::TypeCall::Construct(_))=>2};
   }
   ReturnedNeed::AliasBuiltin=>{
    let Some(crate::generics::TypeCall::Builtin(name))=alias else{unsafe{std::hint::unreachable_unchecked()}};
    return unsafe{world.call(builder.builtin_call(name,&args,expression.span(),None)).await}.map(|value|value.map(Returned::value));
   }
   ReturnedNeed::AliasType=>{
    let Some(crate::generics::TypeCall::Construct(syntax))=alias else{unsafe{std::hint::unreachable_unchecked()}};
    let ty=unsafe{world.call(builder.expected_storage(syntax,expression.span())).await}?;
    facts.kind=u32::from(ty.task_result().is_some());facts.present=u32::from(ty.owns_resources());record_ty=Some(ty);
   }
   ReturnedNeed::TaskType=>return builder.fail(expression.span(),"B224","任务只能从start_task取得"),
   ReturnedNeed::RecordValue=>return unsafe{world.call(builder.record_values(record_ty.take().unwrap_unchecked(),args.len(),expression.span(),
    async |builder,index,expected|unsafe{builder.world.call(builder.expr_expected(&args.at(index),expected)).await})).await}.map(|value|value.map(Returned::value)),
   ReturnedNeed::Environment=>facts.present=u32::from(builder.env.contains_key(name)),
   ReturnedNeed::Indirect=>{
    let callee=expression.child::<wire::ExprNode>(call.syntax.callee.offset);
    return unsafe{world.call(builder.function_value_call(callee.span(),&args,expression.span(),async |builder|
     unsafe{builder.world.call(builder.expr(&callee)).await})).await};
   }
   ReturnedNeed::LocalFunction=>return unsafe{world.call(builder.function_value_call(expression.span(),&args,expression.span(),async |builder|{
    let mut value=unsafe{builder.world.call(builder.name_expr(name,expression.span())).await}?;
    builder.bound_value(&mut value);Ok(Some(value))
   })).await},
   ReturnedNeed::Bind=>{
    let mut bound=TypeName::named(name);
    let source=expression.list::<wire::ArgumentNode>(call.syntax.types.value);
    bound.arguments=unsafe{world.call(builder.scope.bindings().arguments(world,&source,
     (builder.scope.source(),builder.scope.module(),builder.scope.imports()),expression.span())).await}?;
    explicit=Some(bound);
   }
   ReturnedNeed::VariantType=>{
    constructor=unsafe{world.call(builder.variant_constructor_type_with(name,expected,expression.span(),|parent|explicit.take().map(|mut syntax|{syntax.name=parent.into();syntax}))).await}?;
    facts.present=u32::from(constructor.is_some());facts.kind=u32::from(constructor.as_ref().is_some_and(|(ty,_)|ty.owns_resources()));
   }
   need @ (ReturnedNeed::VariantValue|ReturnedNeed::VariantOwned)=>{
    let(ty,index)=unsafe{constructor.take().unwrap_unchecked()};
    return if matches!(need,ReturnedNeed::VariantOwned){
     unsafe{world.call(builder.make_owned_variant(ty,index,&args,expression.span())).await}.map(|value|value.map(Returned::OwnedValue))
    }else{unsafe{world.call(builder.value_variant_constructor(ty,index,&args,expression.span())).await}.map(|value|value.map(Returned::value))};
   }
   ReturnedNeed::Dataset=>return unsafe{world.call(builder.create_application_dataset(expression,
    explicit.as_ref().unwrap_unchecked().arguments.as_slice(),&args,expression.span())).await}.map(|value|value.map(Returned::Dataset)),
   ReturnedNeed::ExpectedOwns=>facts.present=u32::from(unsafe{expected.unwrap_unchecked()}.owns_resources()),
   ReturnedNeed::ExpectedOwnedResult=>return unsafe{world.call(builder.make_owned_variant(expected.unwrap_unchecked().clone(),usize::from(name!="Ok"),&args,expression.span())).await}.map(|value|value.map(Returned::OwnedValue)),
   ReturnedNeed::ResultType=>{
    let syntax=unsafe{explicit.as_mut().unwrap_unchecked()};syntax.name="result".into();
    let ty=unsafe{world.call(builder.expected_storage(syntax,expression.span())).await}?;
    facts.present=u32::from(ty.owns_resources());result_ty=Some(ty);
   }
   ReturnedNeed::OwnedResult=>return unsafe{world.call(builder.make_owned_variant(
    result_ty.take().unwrap_unchecked(),usize::from(name!="Ok"),&args,expression.span())).await}.map(|value|value.map(Returned::OwnedValue)),
   ReturnedNeed::RecordName=>facts.present=u32::from(!world.resolve(name,builder.scope.module(),builder.scope.imports(),false).is_missing()),
   ReturnedNeed::RecordType=>{
    let syntax=explicit.get_or_insert_with(||TypeName::named(name));syntax.name=name.into();
    let ty=unsafe{world.call(builder.expected_storage(syntax,expression.span())).await}?;
    facts.present=u32::from(ty.owns_resources());record_ty=Some(ty);
   }
   ReturnedNeed::OwnedRecord=>return unsafe{world.call(builder.make_owned_record(record_ty.take().unwrap_unchecked(),&args,expression.span())).await},
   ReturnedNeed::FunctionName=>{
    use crate::frontend::NameResolution;
    facts.kind=match world.resolve(name,builder.scope.module(),builder.scope.imports(),true){
     NameResolution::Unique(value)=>{key=Some(value);1},NameResolution::Ambiguous=>2,NameResolution::Missing=>0};
   }
   ReturnedNeed::Arguments=>{
    arguments=unsafe{world.call(builder.function_arguments(key.as_ref().unwrap_unchecked(),&args,expression.span())).await}?;
    facts.present=u32::from(arguments.is_some());
   }
   ReturnedNeed::Instance=>{
    instance=Some(unsafe{world.call(builder.specialized_arguments(key.take().unwrap_unchecked(),
     arguments.as_ref().unwrap_unchecked(),if matches!(call.syntax.form,CallForm::Explicit){
      Some(explicit.as_ref().unwrap_unchecked().arguments.as_slice())}else{None},expression.span())).await}?);
    explicit.take();result_ty.take();record_ty.take();
   }
   ReturnedNeed::Invoke=>return unsafe{world.call(builder.invoke_returned(instance.take().unwrap_unchecked(),
    arguments.take().unwrap_unchecked(),expression.span(),None,Some(&args))).await},
   ReturnedNeed::Ambiguous=>return builder.fail(expression.span(),"B158",format!("函数名称 {name} 有多个可见声明，须使用模块限定名")),
   ReturnedNeed::Empty=>return Ok(None),
   ReturnedNeed::Fallback=>{
    call.cursor=SourceCallCursor{phase:12};
    let actual=result_ty.take().or(record_ty.take());
    let mut value=unsafe{world.call(builder.at_source(expression.span(),SourceSyntax::Expression,async |builder|{
     builder.view_uses.before(expression.span());
     specialized_with(builder,expression,call,explicit,actual).await
    })).await}?;
    if let Some(value)=&mut value{builder.bound_value(value);}
    return Ok(value.map(Returned::value));
   }
   ReturnedNeed::ExpectedResult=>return unsafe{world.call(builder.value_variant_constructor(expected.unwrap_unchecked().clone(),
    usize::from(name!="Ok"),&args,expression.span())).await}.map(|value|value.map(Returned::value)),
   ReturnedNeed::MissingResult=>return builder.fail(expression.span(),"B269",
    "Ok／Err 需要已声明的 result<T,E> 上下文，或显式给出两个类型实参"),
   ReturnedNeed::Resize=>return unsafe{world.call(builder.resize_application_dataset(&args,expression.span())).await}.map(|value|value.map(Returned::Dataset)),
   ReturnedNeed::TakeLast=>return unsafe{world.call(builder.take_last_application_dataset(&args,expression.span())).await}.map(|value|value.map(Returned::Dataset)),
   ReturnedNeed::Own=>return unsafe{world.call(builder.make_user_owned(&args,expression.span())).await},
   ReturnedNeed::IntoInner=>return unsafe{world.call(builder.into_user_inner(&args,expression.span())).await},
   ReturnedNeed::Builtin=>return unsafe{world.call(builder.builtin_call(name,&args,expression.span(),Some(record_ty.take()))).await}.map(|value|value.map(Returned::value)),
   ReturnedNeed::Propagate=>return unsafe{world.call(builder.propagate_returned(&args,expression.span())).await},
  }
 }
}
async fn named_value_scoped(builder:&mut Builder<'_>,expression:&Expr,call:Call)->Result<Option<Value>>{
 let world=builder.world;
 let mut value=unsafe{world.call(builder.at_source(expression.span(),SourceSyntax::Expression,async |builder|{
  builder.view_uses.before(expression.span());
  let mut returned=unsafe{world.call(builder.at_source(expression.span(),SourceSyntax::Expression,async |builder|{
   builder.view_uses.before(expression.span());builder.require_returned_projection(expression)?;
   returned_with(builder,expression,None,call).await
  })).await}?;
  match &mut returned{
   Some(returns::Returned::Value(value))=>builder.bound_value(value),
   Some(returns::Returned::OwnedValue(value))=>builder.bound_value(&mut value.value),
   _=>{}
  }
  builder.returned_expression(returned,expression.span(),false)
 })).await}?;
 if let Some(value)=&mut value{builder.bound_value(value);}
 Ok(value)
}
