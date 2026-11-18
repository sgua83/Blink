//! Constant control borrows the original values and instruction storage.
//! B selects eligibility and retained source operations; Rust owns their actual
//! types, source references and one result Vec. No alternate IR is constructed.
use super::*;
use std::{ffi::c_void,ptr};

#[derive(Clone,Copy)]
#[repr(C)]
struct Name {data:*const u8,count:usize}
#[derive(Clone,Copy)]
#[repr(C)]
struct Origin {present:u32,source:*const c_void,generated:*const c_void}
#[repr(C)]
struct OriginHost {
    inspect:unsafe extern "C" fn(*const Inst)->Origin,
    append:unsafe extern "C" fn(*mut c_void,*const Inst),
}
unsafe extern "C" {
    #[link_name="bootstrap.constant_evaluation::declaration_type"]
    fn b_declaration_type(name:Name,wrappers:u32)->u32;
    #[link_name="bootstrap.constant_evaluation::constant_result"]
    fn b_constant_result(facts:u32,code:crate::ir::dataflow::Span,ir:crate::ir::dataflow::Host)->u32;
    #[link_name="bootstrap.constant_evaluation::source_operations"]
    fn b_source_operations(code:crate::ir::dataflow::Span,first:usize,used:crate::ir::dataflow::Span,
        output:*mut c_void,origin:OriginHost,ir:crate::ir::dataflow::Host);
}
pub(super) fn declaration_type(value:&TypeName)->bool {
    let wrappers=u32::from(value.dataset)|(u32::from(value.view)<<1)|(u32::from(value.tile)<<2);
    unsafe{b_declaration_type(Name{data:value.name.as_ptr(),count:value.name.len()},wrappers)!=0}
}
pub(super) fn constant_result(views:bool,constant:bool,code:&[Inst])->bool {
    unsafe{b_constant_result(u32::from(views)|(u32::from(constant)<<1),
        crate::ir::dataflow::Span::slice(code),crate::ir::dataflow::HOST)!=0}
}
unsafe extern "C" fn inspect_origin(value:*const Inst)->Origin {
    let value=unsafe{&*value};
    Origin{present:u32::from(value.generated_origin.is_some()),
        source:value.source_site.as_ref().map_or(ptr::null(),|value|Arc::as_ptr(value).cast()),
        generated:value.generated_origin.as_ref().map_or(ptr::null(),|value|Arc::as_ptr(value).cast())}
}
unsafe extern "C" fn append_operation(output:*mut c_void,value:*const Inst) {
    unsafe{&mut *output.cast::<Vec<OperationOrigin>>()}.push(OperationOrigin::from_instruction(unsafe{&*value}));
}
pub(super) fn source_operations(code:&[Inst],first:usize,used:&[Reg])->Vec<OperationOrigin> {
    let mut output=Vec::new();
    unsafe{b_source_operations(crate::ir::dataflow::Span::slice(code),first,
        crate::ir::dataflow::Span::slice(used),ptr::from_mut(&mut output).cast(),
        OriginHost{inspect:inspect_origin,append:append_operation},crate::ir::dataflow::HOST);}
    output
}

#[repr(u32)]
enum ModuleEntry {Cached,Evaluate}
#[repr(u32)]
enum ModuleNeed {Instance,ReturnType,Body,InvalidType}
#[repr(C)]
struct ModuleCursor {phase:u32}
unsafe extern "C" {
    #[link_name="bootstrap.constant_evaluation::module_entry"]
    fn b_module_entry(cached:u32)->ModuleEntry;
    #[link_name="bootstrap.constant_evaluation::module_start"]
    fn b_module_start()->ModuleCursor;
    #[link_name="bootstrap.constant_evaluation::module_step"]
    fn b_module_step(cursor:*mut ModuleCursor,type_allowed:u32)->ModuleNeed;
}

type ConstantValue=(Ty,Vec<Bits>,Option<Arc<GeneratedOrigin>>);

// The B request owns control, while this future holds the one original typed
// instance across binding and body needs. Every child leaves native before it
// is awaited on this same World.
async fn module_value(
    world:&World,key:&str,calls:&[String],definition:&Arc<crate::frontend::Definition>,
)->Result<ConstantValue> {
    let mut cursor=unsafe{b_module_start()};
    let mut instance:Option<crate::generics::Instance>=None;
    let mut type_allowed=0;
    loop {
        match unsafe{b_module_step(&mut cursor,type_allowed)} {
            ModuleNeed::Instance=>{
                instance=Some(unsafe{world.call(crate::generics::instantiate(
                    world,key,definition,Vec::new(),&[])).await}?);
            }
            ModuleNeed::ReturnType=>{
                // This request follows the actual completed Instance request;
                // no second instance, placeholder type or initialized registry.
                let instance=unsafe{instance.as_ref().unwrap_unchecked()};
                let ty=unsafe{world.call(instance.bindings.type_name(
                    world,&definition.function.result(),
                    (&definition.source,&definition.module,&definition.imports),
                    definition.function.span())).await}?;
                type_allowed=u32::from(declaration_type(&ty));
            }
            ModuleNeed::InvalidType=>return definition.source.fail(
                definition.function.span(),"B234","const 需要普通值类型，不能持有资源或 void"),
            ModuleNeed::Body=>{
                let instance=unsafe{instance.take().unwrap_unchecked()};
                return module_body(world,key,calls,definition,instance).await;
            }
        }
    }
}

async fn module_body(
    world:&World,key:&str,calls:&[String],definition:&Arc<crate::frontend::Definition>,
    instance:crate::generics::Instance,
)->Result<ConstantValue> {
    let context=constant_context(key);
    let scope=LexicalScope::Function(instance.clone());
    let(source_definition,source_context)=definition.syntax_context(Some(key));
    let mut builder=Builder::from_checked(world,&scope,&context,0,
        source_definition,source_context,BTreeSet::new());
    builder.in_function=true;
    builder.calls.extend_from_slice(calls);
    builder.calls.push(key.into());
    let body=definition.function.body().unwrap();
    let(returned,_)=unsafe{world.call(builder.function_body(&instance,&body)).await}?;
    let value=builder.ordinary_return(returned,definition.function.span())?.unwrap();
    if !constant_result(value.ty.contains_views(),value.constant.is_some(),&builder.code) {
        return definition.source.fail(definition.function.span(),"B234",
            "const 初始化尚不能完全在编译期求值；不允许资源、运行期运算或控制流");
    }
    let operations=source_operations(&builder.code,0,&[]);
    let origin=(!operations.is_empty()).then(||Arc::new(GeneratedOrigin {
        reason:GenerationReason::ModuleConstantEvaluation,
        trigger:Some(GenerationAnchor::ModuleConstantEvaluation {
            definition:definition.source_origin().map(Arc::new),constant:Arc::from(key),operations,
        }),
    }));
    Ok((value.ty,value.constant.unwrap(),origin))
}

pub(crate) async fn evaluate<'w>(world:&'w World,key:&str,calls:&[String])->Result<&'w crate::frontend::EvaluatedConstant> {
    let definition=&world.functions[key];
    let cached=definition.constant.get();
    match unsafe{b_module_entry(u32::from(cached.is_some()))} {
        ModuleEntry::Cached=>{
            if let Some(inputs)=&world.build_declarations {
                inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
            }
            // The entry is selected only from this same original cache borrow.
            return Ok(unsafe{cached.unwrap_unchecked()}.as_ref());
        }
        ModuleEntry::Evaluate=>{}
    }
    unsafe{world.call(world.evaluating_constants.evaluate(key,async || {
        let parent_inputs=world.build_declarations.as_ref().map(|inputs|inputs.take());
        let result=module_value(world,key,calls,definition).await;
        let mut used=BTreeSet::new();
        if let(Some(inputs),Some(parent))=(&world.build_declarations,parent_inputs) {
            used=inputs.replace(parent);
        }
        let value=result?;
        let constant=definition.constant.get_or_init(||Arc::new(crate::frontend::EvaluatedConstant {
            value,inputs:used,
        }));
        if let Some(inputs)=&world.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
        }
        Ok(constant.as_ref())
    })).await}.unwrap_or_else(||definition.source.fail(
        definition.function.span(),"B160",format!("编译期值与类型递归依赖 {key}")))
}

#[derive(Clone,Copy,PartialEq,Eq)]
#[repr(u32)]
enum ExpressionKind {Number,StaticU32,Bool,Name,Member,Index,Call,Invoke,Construct,Specialize,FunctionRef,Cooperate,Target,Unary,Binary,Select}
#[derive(Clone,Copy)]
#[repr(C)]
struct ExpressionView {
 kind:ExpressionKind,text:Name,first:*const crate::ast::Expr,second:*const crate::ast::Expr,
 third:*const crate::ast::Expr,ty:*const TypeName,types:*const GenericArgument,type_count:usize,
 args:*const crate::ast::Expr,arg_count:usize,bits:u64,
}
#[repr(C)]
struct ExpressionHost {inspect:unsafe extern "C" fn(*const crate::ast::Expr)->ExpressionView}
impl Name {
 fn of(value:&str)->Self{Self{data:value.as_ptr(),count:value.len()}}
 unsafe fn text<'a>(self)->&'a str{
  unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data,self.count))}
 }
}
// This is one synchronous field projection of the actual SDK expression. The
// owning schema and all child storage outlive this reader; nothing is retained.
fn expression_view(expression:&crate::ast::Expr)->ExpressionView {
 use crate::ast::ExprKind as K;
 let mut view=ExpressionView{kind:ExpressionKind::Number,text:Name::of(""),
  first:ptr::null(),second:ptr::null(),third:ptr::null(),ty:ptr::null(),
  types:[].as_ptr(),type_count:0,args:[].as_ptr(),arg_count:0,bits:0};
 match &expression.kind{
  K::Number(text)=>{view.kind=ExpressionKind::Number;view.text=Name::of(text);}
  K::StaticU32(value)=>{view.kind=ExpressionKind::StaticU32;view.bits=(*value).into();}
  K::Bool(value)=>{view.kind=ExpressionKind::Bool;view.bits=u64::from(*value);}
  K::Name(name)=>{view.kind=ExpressionKind::Name;view.text=Name::of(name);}
  K::Member(base,name)=>{view.kind=ExpressionKind::Member;view.first=&**base;view.text=Name::of(name);}
  K::Index(base,index)=>{view.kind=ExpressionKind::Index;view.first=&**base;view.second=&**index;}
  K::Call(name,args)=>{view.kind=ExpressionKind::Call;view.text=Name::of(name);view.args=args.as_ptr();view.arg_count=args.len();}
  K::Invoke(callee,args)=>{view.kind=ExpressionKind::Invoke;view.first=&**callee;view.args=args.as_ptr();view.arg_count=args.len();}
  K::Construct(ty,args)=>{view.kind=ExpressionKind::Construct;view.ty=ty;view.args=args.as_ptr();view.arg_count=args.len();}
  K::Specialize(name,types,args)=>{view.kind=ExpressionKind::Specialize;view.text=Name::of(name);view.types=types.as_ptr();view.type_count=types.len();view.args=args.as_ptr();view.arg_count=args.len();}
  K::FunctionRef(name,types)=>{view.kind=ExpressionKind::FunctionRef;view.text=Name::of(name);view.types=types.as_ptr();view.type_count=types.len();}
  K::Cooperate(name,types,args)=>{view.kind=ExpressionKind::Cooperate;view.text=Name::of(name);view.args=args.as_ptr();view.arg_count=args.len();if let Some(types)=types{view.types=types.as_ptr();view.type_count=types.len();}}
  K::Target(_,expression)=>{view.kind=ExpressionKind::Target;view.first=&**expression;}
  K::Unary(op,value)=>{view.kind=ExpressionKind::Unary;view.text=Name::of(op);view.first=&**value;}
  K::Binary(op,left,right)=>{view.kind=ExpressionKind::Binary;view.text=Name::of(op);view.first=&**left;view.second=&**right;}
  K::Select(condition,yes,no)=>{view.kind=ExpressionKind::Select;view.first=&**condition;view.second=&**yes;view.third=&**no;}
 }
 view
}
unsafe extern "C" fn inspect_expression(expression:*const crate::ast::Expr)->ExpressionView {
 expression_view(unsafe{&*expression})
}
#[repr(u32)]
enum SyntaxNeed {Number,StaticU32,Bool,Name,First,Second,Member,IndexType,Index,InvalidIndex,Negative,Unary,Binary,ShortCircuit,Select,Type,Construct,Call,FunctionRef,Cooperate,Target,Invoke,Empty}
#[repr(C)]
struct SyntaxCursor {phase:u32}
#[repr(u32)]
pub(super) enum ValueNeed {Expected,Read,Validate,Reject,Sources,Literal,Reference}
#[repr(C)]
pub(super) struct ValueCursor {phase:u32,flags:u32}
unsafe extern "C" {
 #[link_name="bootstrap.constant_evaluation::syntax_start"]
 fn b_syntax_start()->SyntaxCursor;
 #[link_name="bootstrap.constant_evaluation::syntax_step"]
 fn b_syntax_step(cursor:*mut SyntaxCursor,expression:ExpressionView,facts:u32,host:ExpressionHost)->SyntaxNeed;
 #[link_name="bootstrap.constant_evaluation::value_start"]
 fn b_value_start(flags:u32)->ValueCursor;
 #[link_name="bootstrap.constant_evaluation::value_step"]
 fn b_value_step(cursor:*mut ValueCursor,facts:u32,code:crate::ir::dataflow::Span,ir:crate::ir::dataflow::Host)->ValueNeed;
}
impl ValueCursor {
 pub(super) fn start(annotation:bool,sources:bool,name:bool)->Self{
  unsafe{b_value_start(u32::from(annotation)|(u32::from(sources)<<1)|(u32::from(name)<<2))}
 }
 pub(super) fn step(&mut self,facts:u32,code:&[Inst])->ValueNeed{
  unsafe{b_value_step(self,facts,crate::ir::dataflow::Span::slice(code),crate::ir::dataflow::HOST)}
 }
}

impl Builder<'_> {
 pub(super) async fn reference_syntax(&mut self,expression:&crate::ast::Expr,expected:Option<&Ty>)->Result<Option<Value>> {
  let view=expression_view(expression);let span=expression.span;
  let mut cursor=unsafe{b_syntax_start()};let mut facts=0;
  let mut first:Option<Value>=None;let mut second:Option<Value>=None;let mut ty:Option<Ty>=None;
  loop {
   match unsafe{b_syntax_step(&mut cursor,view,facts,ExpressionHost{inspect:inspect_expression})} {
    SyntaxNeed::Number=>return self.number_expression(unsafe{view.text.text()},span),
    SyntaxNeed::StaticU32=>return self.constant_expression(Scalar::U32,view.bits),
    SyntaxNeed::Bool=>return self.constant_expression(Scalar::Bool,view.bits),
    SyntaxNeed::Name=>return unsafe{self.world.call(self.name_expr(view.text.text(),span)).await}.map(Some),
    SyntaxNeed::First=>{
     first=unsafe{self.world.call(self.reference_expr(&*view.first,None)).await}?;
     facts=u32::from(first.is_some());
    }
    SyntaxNeed::Second=>{
     second=unsafe{self.world.call(self.reference_expr(&*view.second,None)).await}?;
     facts=u32::from(second.is_some());
    }
    SyntaxNeed::Member=>return self.member_value(unsafe{first.take().unwrap_unchecked()},unsafe{view.text.text()},span).map(Some),
    SyntaxNeed::IndexType=>{
     facts|=u32::from(unsafe{first.as_ref().unwrap_unchecked()}.ty.indexed_element().is_some())<<1;
    }
    SyntaxNeed::InvalidIndex=>return self.fail(span,"B136","此类型不能按固定值下标读取"),
    SyntaxNeed::Index=>{
     let value=unsafe{first.take().unwrap_unchecked()};let index=unsafe{&*view.second};
     return unsafe{self.world.call(self.array_value_at(value,index.span,async |builder|
      unsafe{builder.world.call(builder.reference_expr(index,None)).await})).await};
    }
    SyntaxNeed::Negative=>return self.negative_number(unsafe{expression_view(&*view.first).text.text()},span).map(Some),
    SyntaxNeed::Unary=>return self.unary_value(unsafe{view.text.text()},unsafe{first.take().unwrap_unchecked()},span).map(Some),
    SyntaxNeed::Binary=>return self.binary(unsafe{view.text.text()},unsafe{first.take().unwrap_unchecked()},
     unsafe{second.take().unwrap_unchecked()},span,None).map(Some),
    SyntaxNeed::ShortCircuit=>{
     let left=unsafe{first.take().unwrap_unchecked()};let right=unsafe{&*view.second};
     return unsafe{self.world.call(self.short_circuit_value(view.text.text(),left,right.span,async |builder|
      Ok(unsafe{builder.world.call(builder.reference_expr(right,None)).await}?.map(returns::Returned::value)),
      async |_,_|Ok(()),span)).await};
    }
    SyntaxNeed::Select=>{
     let condition=unsafe{first.take().unwrap_unchecked()};
     self.require_bool(&condition,span)?;
     let yes=unsafe{&*view.second};let no=unsafe{&*view.third};
     let returned=unsafe{self.world.call(self.selection_after_condition(condition,
      (yes.span,async |builder|Ok(unsafe{builder.world.call(builder.reference_expr(yes,expected)).await}?.map(returns::Returned::value))),
      (no.span,async |builder|Ok(unsafe{builder.world.call(builder.reference_expr(no,expected)).await}?.map(returns::Returned::value))),
      async |_,_|Ok(()),span)).await}?;
     return self.ordinary_return(returned,span);
    }
    SyntaxNeed::Type=>{
     ty=Some(unsafe{self.world.call(self.world.ty(&*view.ty,self.scope.source(),span,self.scope.module(),self.scope.imports())).await}?);
    }
    SyntaxNeed::Construct=>{
     let args=unsafe{std::slice::from_raw_parts(view.args,view.arg_count)};
     return unsafe{self.world.call(self.record_values(ty.take().unwrap_unchecked(),args.len(),span,
      async |builder,index,expected|unsafe{builder.world.call(builder.reference_expr(&args[index],expected)).await})).await};
    }
    SyntaxNeed::Call=>{
     let args=unsafe{std::slice::from_raw_parts(view.args,view.arg_count)};
     let explicit=(view.kind==ExpressionKind::Specialize).then(||unsafe{std::slice::from_raw_parts(view.types,view.type_count)});
     return unsafe{self.world.call(self.reference_call(view.text.text(),explicit,args,expected,span)).await};
    }
    SyntaxNeed::FunctionRef=>return self.fail(span,"B236","静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值"),
    SyntaxNeed::Cooperate=>return self.fail(span,"B208","cooperate 调用只能作为协作根块的局部声明"),
    SyntaxNeed::Target=>return self.fail(span,"B266","target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置"),
    SyntaxNeed::Invoke=>{
     let callee=unsafe{&*view.first};let args=unsafe{std::slice::from_raw_parts(view.args,view.arg_count)};
     let result=unsafe{self.world.call(self.function_value_arguments(callee.span,span,
      async |builder|unsafe{builder.world.call(builder.reference_expr(callee,None)).await},
      async |builder,parameters|unsafe{builder.world.call(builder.reference_call_arguments(cooperative::CallParameters::CFunction(parameters),args,span)).await})).await}?;
     return self.ordinary_return(result,span);
    }
    SyntaxNeed::Empty=>return Ok(None),
   }
  }
 }
}

#[repr(C)]
struct PlaceHost {
 inspect:unsafe extern "C" fn(*const crate::ast::Expr)->ExpressionView,
 binding:unsafe extern "C" fn(*const Builder<'_>,Name)->u32,
}
unsafe extern "C" fn place_binding(builder:*const Builder<'_>,name:Name)->u32{
 let builder=unsafe{&*builder};
 u32::from(matches!(builder.env.get(unsafe{name.text()}),
  Some(Binding::AtomicBorrow(_)|Binding::ViewElement{..}|Binding::Owned(_)|Binding::OwnedValue(_))))
}
#[repr(u32)]
enum PlaceNeed {Name,External,Parent,Member,BaseIsPlace,BaseValue,IndexedType,ArrayType,Materialize,IndexValue,ArrayIndex,ReadPlace,ViewAt,Empty,Invalid}
#[repr(C)]
struct PlaceCursor {phase:u32}
#[repr(u32)]
enum ArgumentNeed {PlaceTest,Place,AtomicPlace,MutablePlace,Expected,Read,Flow,MutableReturned,AtomicError,Resource,Empty}
#[repr(C)]
struct ArgumentCursor {phase:u32,access:u32}
#[repr(u32)]
enum ArgumentsNeed {Read,Append,Empty,Finish}
#[repr(C)]
struct ArgumentsCursor {index:usize,count:usize,phase:u32}
unsafe extern "C" {
 #[link_name="bootstrap.constant_evaluation::is_place"]
 fn b_is_place(expression:*const crate::ast::Expr,context:*const Builder<'_>,host:PlaceHost)->u32;
 #[link_name="bootstrap.constant_evaluation::place_start"]
 fn b_place_start()->PlaceCursor;
 #[link_name="bootstrap.constant_evaluation::place_step"]
 fn b_place_step(cursor:*mut PlaceCursor,expression:ExpressionView,facts:u32)->PlaceNeed;
 #[link_name="bootstrap.constant_evaluation::argument_start"]
 fn b_argument_start(access:u32)->ArgumentCursor;
 #[link_name="bootstrap.constant_evaluation::argument_step"]
 fn b_argument_step(cursor:*mut ArgumentCursor,facts:u32)->ArgumentNeed;
 #[link_name="bootstrap.constant_evaluation::arguments_start"]
 fn b_arguments_start(count:usize)->ArgumentsCursor;
 #[link_name="bootstrap.constant_evaluation::arguments_step"]
 fn b_arguments_step(cursor:*mut ArgumentsCursor,present:u32)->ArgumentsNeed;
}
impl Builder<'_> {
 pub(super) fn reference_is_place(&self,expression:&crate::ast::Expr)->bool{
  unsafe{b_is_place(expression,self,PlaceHost{inspect:inspect_expression,binding:place_binding})!=0}
 }
 pub(super) async fn reference_place(&mut self,expression:&crate::ast::Expr)->Result<Option<Place>>{
  unsafe{self.world.call(self.at_source(expression.span,SourceSyntax::Place,
   async |builder|builder.reference_place_body(expression).await)).await}
 }
 async fn reference_place_body(&mut self,expression:&crate::ast::Expr)->Result<Option<Place>>{
  let span=expression.span;self.view_uses.before(span);
  let view=expression_view(expression);let mut cursor=unsafe{b_place_start()};let mut facts=0;
  let mut place:Option<Place>=None;let mut value:Option<Value>=None;let mut index:Option<Value>=None;
  loop {
   match unsafe{b_place_step(&mut cursor,view,facts)}{
    PlaceNeed::Name=>return self.name_place(unsafe{view.text.text()},span).map(Some),
    PlaceNeed::External=>{
     let args=unsafe{std::slice::from_raw_parts(view.args,view.arg_count)};
     let types=unsafe{std::slice::from_raw_parts(view.types,view.type_count)};
     return unsafe{self.world.call(self.external_position_value(types,args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
      async |builder|unsafe{builder.world.call(builder.reference_expr(&args[0],None)).await})).await};
    }
    PlaceNeed::Parent=>{
     place=unsafe{self.world.call(self.reference_place(&*view.first)).await}?;
     facts=u32::from(place.is_some());
    }
    PlaceNeed::Member=>return self.member_position(unsafe{place.take().unwrap_unchecked()},unsafe{view.text.text()},span),
    PlaceNeed::BaseIsPlace=>{facts=u32::from(self.reference_is_place(unsafe{&*view.first}))<<1;}
    PlaceNeed::BaseValue=>{
     value=unsafe{self.world.call(self.reference_expr(&*view.first,None)).await}?;
     facts=u32::from(value.is_some());
    }
    PlaceNeed::IndexedType=>{facts|=u32::from(unsafe{place.as_ref().unwrap_unchecked()}.ty().indexed_element().is_some())<<2;}
    PlaceNeed::ArrayType=>{facts|=u32::from(unsafe{value.as_ref().unwrap_unchecked()}.ty.array.is_some())<<2;}
    PlaceNeed::Materialize=>{place=Some(self.materialize_array(unsafe{value.take().unwrap_unchecked()},None).place);}
    PlaceNeed::IndexValue=>{
     index=unsafe{self.world.call(self.reference_expr(&*view.second,None)).await}?;
     facts=u32::from(index.is_some());
    }
    PlaceNeed::ArrayIndex=>return self.array_index(unsafe{place.take().unwrap_unchecked()},
     unsafe{index.take().unwrap_unchecked()},unsafe{&*view.second}.span).map(Some),
    PlaceNeed::ReadPlace=>{
     value=Some(unsafe{self.world.call(self.read_place(place.as_ref().unwrap_unchecked(),(&*view.first).span,None)).await}?);
    }
    PlaceNeed::ViewAt=>{
     let index=unsafe{&*view.second};
     return unsafe{self.world.call(self.view_value_at(value.take().unwrap_unchecked(),index.span,
      async |builder|unsafe{builder.world.call(builder.reference_expr(index,None)).await})).await};
    }
    PlaceNeed::Empty=>return Ok(None),
    PlaceNeed::Invalid=>return self.fail(span,"B138","表达式不是可赋值位置"),
   }
  }
 }
 pub(super) async fn reference_argument(&mut self,parameters:cooperative::CallParameters<'_>,
  expression:&crate::ast::Expr,index:usize)->Result<Option<cooperative::Argument>>{
  let access=parameters.access(index);
  let code=match access{Access::InOut=>1,Access::Write=>2,Access::Atomic=>3,_=>0};
  let mut cursor=unsafe{b_argument_start(code)};let mut facts=0;
  let mut place:Option<Place>=None;let mut expected:Option<Ty>=None;let mut value:Option<Value>=None;
  loop{
   match unsafe{b_argument_step(&mut cursor,facts)}{
    ArgumentNeed::PlaceTest=>{facts=u32::from(self.reference_is_place(expression));}
    ArgumentNeed::Place=>{
     place=unsafe{self.world.call(self.reference_place(expression)).await}?;
     facts=u32::from(place.is_some())<<1;
    }
    ArgumentNeed::AtomicPlace=>return self.atomic_place_value(unsafe{place.take().unwrap_unchecked()},expression.span)
     .map(|place|place.map(cooperative::Argument::Atomic)),
    ArgumentNeed::MutablePlace=>{
     let place=unsafe{self.world.call(self.mutable_place_value(place.take().unwrap_unchecked(),access,expression.span)).await}?;
     return Ok(Some(cooperative::Argument::Reference(self.borrow_owned(place))));
    }
    ArgumentNeed::Expected=>{expected=unsafe{self.world.call(parameters.expected(self.world,&self.scope,index)).await}?;}
    ArgumentNeed::Read=>{value=unsafe{self.world.call(self.reference_expr(expression,expected.as_ref())).await}?;}
    ArgumentNeed::Flow=>{facts=u32::from(self.flow.continues())<<2;}
    ArgumentNeed::MutableReturned=>return unsafe{self.world.call(self.mutable_returned_argument(
     value.take().map(returns::Returned::value),access,expression.span)).await},
    ArgumentNeed::AtomicError=>return self.fail(expression.span,"B138","表达式不是可赋值位置"),
    ArgumentNeed::Resource=>return Ok(value.take().map(returns::Returned::value).map(cooperative::Argument::resource)),
    ArgumentNeed::Empty=>return Ok(None),
   }
  }
 }
 pub(super) async fn reference_arguments(&mut self,args:&[crate::ast::Expr])->Result<Option<Vec<Value>>>{
  let mut cursor=unsafe{b_arguments_start(args.len())};
  let mut values=Vec::new();let mut value:Option<Value>=None;
  loop{
   match unsafe{b_arguments_step(&mut cursor,u32::from(value.is_some()))}{
    ArgumentsNeed::Read=>{value=unsafe{self.world.call(self.reference_expr(&args[cursor.index],None)).await}?;}
    ArgumentsNeed::Append=>{values.push(unsafe{value.take().unwrap_unchecked()});}
    ArgumentsNeed::Empty=>return Ok(None),
    ArgumentsNeed::Finish=>return Ok(Some(values)),
   }
  }
 }
}

#[repr(u32)]
enum ShapeNeed {Absent,Literal,Binding,Bound,Resolve,Unknown,Evaluate,Constant,InvalidType,Expression}
#[repr(C)]
struct ShapeCursor {phase:u32,kind:u32,source:u32}
#[repr(u32)]
pub(super) enum IntegerNeed {Read,Validate,Reject,Finish}
#[repr(C)]
pub(super) struct IntegerCursor {phase:u32}
unsafe extern "C"{
 #[link_name="bootstrap.constant_evaluation::shape_start"]
 fn b_shape_start(kind:u32,source:u32)->ShapeCursor;
 #[link_name="bootstrap.constant_evaluation::shape_step"]
 fn b_shape_step(cursor:*mut ShapeCursor,present:u32)->ShapeNeed;
 #[link_name="bootstrap.constant_evaluation::integer_start"]
 fn b_integer_start()->IntegerCursor;
 #[link_name="bootstrap.constant_evaluation::integer_step"]
 fn b_integer_step(cursor:*mut IntegerCursor,facts:u32,code:crate::ir::dataflow::Span,ir:crate::ir::dataflow::Host)->IntegerNeed;
}
impl IntegerCursor{
 pub(super) fn start()->Self{unsafe{b_integer_start()}}
 pub(super) fn step(&mut self,facts:u32,code:&[Inst])->IntegerNeed{
  unsafe{b_integer_step(self,facts,crate::ir::dataflow::Span::slice(code),crate::ir::dataflow::HOST)}
 }
}
enum ShapeExpression<'a>{Source(Expr),Reference(&'a crate::ast::Expr)}
struct ShapeParts<'a>{kind:u32,literal:u32,name:Option<&'a str>,expression:Option<ShapeExpression<'a>>}
pub(super) async fn source_shape(world:&World,shape:&crate::syntax::Shape,bindings:&Arc<crate::generics::Bindings>,
 source:&Source,span:Span,module:&str,imports:&[String])->Result<u32>{
 // These are borrowed fields from the original sealed node. The Expr handle
 // keeps that same storage alive; it is not a second syntax representation.
 let parts=match shape.kind(){
  crate::syntax::ShapeKind::Literal(value)=>ShapeParts{kind:1,literal:value,name:None,expression:None},
  crate::syntax::ShapeKind::Constant(name)=>ShapeParts{kind:2,literal:0,name:Some(name),expression:None},
  crate::syntax::ShapeKind::Expression(expression)=>ShapeParts{kind:3,literal:0,name:None,expression:Some(ShapeExpression::Source(expression))},
 };
 Ok(unsafe{shape_value(world,parts,Some(bindings),source,span,module,imports).await?.unwrap_unchecked()})
}
pub(super) async fn shape(world:&World,shape:&Option<Shape>,source:&Source,span:Span,module:&str,imports:&[String])->Result<Option<u32>>{
 let parts=match shape{
  None=>ShapeParts{kind:0,literal:0,name:None,expression:None},
  Some(Shape::Literal(value))=>ShapeParts{kind:1,literal:*value,name:None,expression:None},
  Some(Shape::Constant(name))=>ShapeParts{kind:2,literal:0,name:Some(name),expression:None},
  Some(Shape::Expression(expression))=>ShapeParts{kind:3,literal:0,name:None,expression:Some(ShapeExpression::Reference(expression))},
 };
 shape_value(world,parts,None,source,span,module,imports).await
}
async fn shape_value(world:&World,parts:ShapeParts<'_>,bindings:Option<&Arc<crate::generics::Bindings>>,
 source:&Source,span:Span,module:&str,imports:&[String])->Result<Option<u32>>{
 let mut cursor=unsafe{b_shape_start(parts.kind,u32::from(bindings.is_some()))};
 let mut present=0;let mut bound:Option<u32>=None;let mut key:Option<String>=None;
 let mut constant:Option<&crate::frontend::EvaluatedConstant>=None;
 loop{
  match unsafe{b_shape_step(&mut cursor,present)}{
   ShapeNeed::Absent=>return Ok(None),
   ShapeNeed::Literal=>return Ok(Some(parts.literal)),
   ShapeNeed::Binding=>{
    bound=unsafe{bindings.unwrap_unchecked()}.values.get(unsafe{parts.name.unwrap_unchecked()}).copied();
    present=u32::from(bound.is_some());
   }
   ShapeNeed::Bound=>return Ok(bound),
   ShapeNeed::Resolve=>{
    key=world.resolve(unsafe{parts.name.unwrap_unchecked()},module,imports,true).unique();
    present=key.as_ref().map_or(0,|key|1|(u32::from(world.functions[key].function.kind()==FunctionKind::Constant)<<1));
   }
   ShapeNeed::Unknown=>return Err(Diagnostic{
    source:source.clone(),span,code:"B015",message:format!("未知或不明确的编译期 const {}",unsafe{parts.name.unwrap_unchecked()}),
   }),
   ShapeNeed::Evaluate=>{
    let value=unsafe{world.call(evaluate(world,key.as_ref().unwrap_unchecked(),&[])).await}?;
    present=u32::from(value.value.0==Ty::scalar(Scalar::U32));
    constant=Some(value);
   }
   ShapeNeed::Constant=>return Ok(Some(unsafe{constant.unwrap_unchecked()}.value.1[0]as u32)),
   ShapeNeed::InvalidType=>return source.fail(span,"B015","形状或 const 实参必须为 u32 标量"),
   ShapeNeed::Expression=>{
    return match unsafe{parts.expression.as_ref().unwrap_unchecked()}{
     ShapeExpression::Source(expression)=>{
      let bindings=unsafe{bindings.unwrap_unchecked()};
      let key=format!("<shape>:{}:{module}:{:?}:{:?}:{:?}:{:?}",
       source.path.display(),expression,expression.span(),bindings.types,bindings.values);
      unsafe{world.call(integer_expression(world,&key,source,module,imports,&Ty::scalar(Scalar::U32),
       expression,bindings,("B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"))).await}.map(Some)
     }
     ShapeExpression::Reference(expression)=>{
      let key=format!("<shape>:{}:{module}:{}",source.path.display(),crate::ast::expression_name(expression));
      let bindings=Arc::new(crate::generics::Bindings::default());
      unsafe{world.call(integer_value(world,&key,source,module,imports,&Ty::scalar(Scalar::U32),expression.span,&bindings,
       ("B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"),
       async |builder|unsafe{world.call(builder.reference_expr(expression,None)).await})).await}.map(Some)
     }
    };
   }
  }
 }
}

#[repr(C)]
struct CallInput {name:Name,explicit:u32,types:*const GenericArgument,type_count:usize,arg_count:usize,expected_result:u32}
#[repr(C)]
struct CallFacts {present:u32,owns:u32,resolution:u32,parameters:usize}
#[repr(C)]
struct CallHost {type_argument:unsafe extern "C" fn(*const GenericArgument,usize)->u32}
#[repr(u32)]
enum CallNeed {VariantLookup,Owns,Variant,ResourceError,ResultType,ExpectedResult,InvalidResultTypes,MissingResultContext,FunctionFrom,PositionFrom,ReadPosition,Empty,InvalidConst,ResolveType,ExplicitType,CheckType,ResolveFunction,ParameterCount,Arguments,Instance,Invoke,Ambiguous,InvalidArgumentCount,LayoutType,LayoutSize,LayoutAlign,InvalidLayoutArguments,UnknownGeneric,CFunction,Println,Subgroup,SubgroupValue,ReadView,UnsafeRead,InvalidReadArguments,FinalType,Construct,IndexContext,OrdinalContext,Builtin,CheckOwns}
#[repr(C)]
struct CallCursor {phase:u32,type_known:u32,type_visible:u32,variant:usize}
#[repr(u32)]
enum CReferenceNeed {Count,Name,FunctionRef,Invalid}
unsafe extern "C"{
 #[link_name="bootstrap.constant_evaluation::call_start"]
 fn b_call_start()->CallCursor;
 #[link_name="bootstrap.constant_evaluation::call_step"]
 fn b_call_step(cursor:*mut CallCursor,input:CallInput,facts:CallFacts,host:CallHost)->CallNeed;
 #[link_name="bootstrap.constant_evaluation::c_reference"]
 fn b_c_reference(args:crate::ir::dataflow::Span,host:ExpressionHost)->CReferenceNeed;
}
unsafe extern "C" fn generic_type_argument(arguments:*const GenericArgument,index:usize)->u32{
 u32::from(matches!(unsafe{&*arguments.add(index)},GenericArgument::Type(_)))
}
unsafe fn generic_type(argument:&GenericArgument)->&TypeName{
 match argument{GenericArgument::Type(ty)=>ty,_=>unsafe{std::hint::unreachable_unchecked()}}
}
impl Builder<'_>{
 pub(super) async fn reference_call(&mut self,name:&str,explicit:Option<&[GenericArgument]>,args:&[crate::ast::Expr],expected:Option<&Ty>,span:Span)->Result<Option<Value>>{
  let input=CallInput{name:Name::of(name),explicit:u32::from(explicit.is_some()),
   types:explicit.map_or([].as_ptr(),|types|types.as_ptr()),type_count:explicit.map_or(0,|types|types.len()),
   arg_count:args.len(),expected_result:u32::from(expected.is_some_and(|ty|ty.result))};
  let mut cursor=unsafe{b_call_start()};
  let mut facts=CallFacts{present:0,owns:0,resolution:0,parameters:0};
  let mut ty:Option<Ty>=None;let mut variant=0;
  let mut key:Option<String>=None;let mut declaration=None;
  let mut arguments:Option<Vec<cooperative::Argument>>=None;let mut instance=None;
  let mut place:Option<Place>=None;let mut view:Option<Value>=None;let mut subgroup=None;
  loop{
   match unsafe{b_call_step(&mut cursor,CallInput{..input},CallFacts{..facts},CallHost{type_argument:generic_type_argument})}{
    CallNeed::VariantLookup=>{
     let result=unsafe{self.world.call(self.variant_constructor_type(name,explicit,expected,span)).await}?;
     facts.present=u32::from(result.is_some());
     if let Some((value,index))=result{ty=Some(value);variant=index;}
    }
    CallNeed::Owns=>{facts.owns=u32::from(unsafe{ty.as_ref().unwrap_unchecked()}.owns_resources());}
    CallNeed::CheckOwns=>{facts.owns=u32::from(unsafe{ty.take().unwrap_unchecked()}.owns_resources());}
    CallNeed::Variant=>return unsafe{self.world.call(self.variant_values(ty.take().unwrap_unchecked(),variant,args.len(),span,
     async |builder,payload|unsafe{builder.world.call(builder.reference_expr(&args[0],Some(payload))).await})).await},
    CallNeed::ResourceError=>return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织"),
    CallNeed::ResultType=>{
     let mut name=TypeName::named("result");name.arguments=unsafe{explicit.unwrap_unchecked()}.to_vec();
     ty=Some(unsafe{self.world.call(self.expected_storage(&name,span)).await}?);variant=cursor.variant;
    }
    CallNeed::ExpectedResult=>{ty=Some(unsafe{expected.unwrap_unchecked()}.clone());variant=cursor.variant;}
    CallNeed::InvalidResultTypes=>return self.fail(span,"B269","Ok／Err需要两个明确类型实参"),
    CallNeed::MissingResultContext=>return self.fail(span,"B269","Ok／Err需要已声明的result上下文或两个明确类型实参"),
    CallNeed::FunctionFrom=>{
     let types=unsafe{explicit.unwrap_unchecked()};
     return unsafe{self.world.call(self.function_from_address(types,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
      async |builder|unsafe{builder.world.call(builder.reference_expr(&args[0],None)).await})).await};
    }
    CallNeed::PositionFrom=>{
     let types=unsafe{explicit.unwrap_unchecked()};
     place=unsafe{self.world.call(self.external_position_value(types,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
      async |builder|unsafe{builder.world.call(builder.reference_expr(&args[0],None)).await})).await}?;
     facts.present=u32::from(place.is_some());
    }
    CallNeed::ReadPosition=>return unsafe{self.world.call(self.read_place(place.as_ref().unwrap_unchecked(),span,None)).await}.map(Some),
    CallNeed::Empty=>return Ok(None),
    CallNeed::InvalidConst=>return self.fail(span,"B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"),
    CallNeed::ResolveType=>{facts.present=u32::from(!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing());}
    CallNeed::ExplicitType=>{
     let mut name=TypeName::named(name);name.arguments=unsafe{explicit.unwrap_unchecked()}.to_vec();
     ty=Some(unsafe{self.world.call(self.world.ty(&name,self.scope.source(),span,self.scope.module(),self.scope.imports())).await}?);
    }
    CallNeed::CheckType=>{ty=Some(unsafe{self.world.call(self.expected_storage(&TypeName::named(name),span)).await}?);}
    CallNeed::ResolveFunction=>{
     let resolution=self.world.resolve(name,self.scope.module(),self.scope.imports(),true);
     facts.resolution=match &resolution{crate::frontend::NameResolution::Unique(_)=>1,crate::frontend::NameResolution::Ambiguous=>2,crate::frontend::NameResolution::Missing=>0};
     key=resolution.unique();
    }
    CallNeed::ParameterCount=>{facts.parameters=self.world.functions[unsafe{key.as_ref().unwrap_unchecked()}].function.params().len();}
    CallNeed::Arguments=>{
     declaration=Some(self.world.functions[unsafe{key.as_ref().unwrap_unchecked()}].clone());
     arguments=unsafe{self.world.call(self.reference_call_arguments(
      cooperative::CallParameters::Declaration(declaration.as_ref().unwrap_unchecked()),args,span)).await}?;
     facts.present=u32::from(arguments.is_some());
    }
    CallNeed::Instance=>{
     instance=Some(unsafe{self.world.call(self.specialized_arguments(
      key.take().unwrap_unchecked(),arguments.as_ref().unwrap_unchecked(),explicit,span)).await}?);
    }
    CallNeed::Invoke=>{
     let returned=unsafe{self.world.call(self.invoke_returned(instance.take().unwrap_unchecked(),
      arguments.take().unwrap_unchecked(),span,None,None)).await}?;
     return self.ordinary_return(returned,span);
    }
    CallNeed::Ambiguous=>return self.fail(span,"B158",format!("函数名称{name}有多个可见声明，须使用模块限定名")),
    CallNeed::InvalidArgumentCount=>return self.fail(span,"B111","函数实参数量与声明不匹配"),
    CallNeed::LayoutType=>{
     let name=unsafe{generic_type(explicit.unwrap_unchecked().first().unwrap_unchecked())};
     ty=Some(unsafe{self.world.call(self.expected_storage(name,span)).await}?);
    }
    CallNeed::LayoutSize=>return self.constant_expression(Scalar::U64,Bits::from(unsafe{ty.as_ref().unwrap_unchecked()}.size)),
    CallNeed::LayoutAlign=>return self.constant_expression(Scalar::U64,Bits::from(unsafe{ty.as_ref().unwrap_unchecked()}.align)),
    CallNeed::InvalidLayoutArguments=>return self.fail(span,"B153",format!("{name}<T>()需要一个明确类型且不接收值实参")),
    CallNeed::UnknownGeneric=>return self.fail(span,"B158",format!("未知或未导出的泛型函数{name}")),
    CallNeed::CFunction=>return unsafe{self.world.call(self.c_function_from(span,async |builder|{
     let need=unsafe{b_c_reference(crate::ir::dataflow::Span::slice(args),ExpressionHost{inspect:inspect_expression})};
     match need{
      CReferenceNeed::Count=>builder.fail(span,"B153","c_function需要一个明确的C ABI函数引用"),
      CReferenceNeed::Invalid=>builder.fail(args[0].span,"B153","c_function需要C ABI函数名及具体类型／const实参"),
      need @ (CReferenceNeed::Name|CReferenceNeed::FunctionRef)=>{
       let view=expression_view(&args[0]);
       let types=matches!(need,CReferenceNeed::FunctionRef).then(||unsafe{std::slice::from_raw_parts(view.types,view.type_count)});
       unsafe{builder.world.call(callback::signature_named(builder.world,&builder.scope,view.text.text(),span,args[0].span,
        async ||Ok(types.map(std::borrow::Cow::Borrowed)))).await}
      }
     }
    })).await}.map(Some),
    CallNeed::Println=>return unsafe{self.world.call(self.println_value(args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
     async |builder|unsafe{builder.world.call(builder.reference_expr(&args[0],None)).await})).await},
    CallNeed::Subgroup=>{
     subgroup=unsafe{self.world.call(self.subgroup(name,args.len(),span,
      async |builder,index|unsafe{builder.world.call(builder.reference_expr(&args[index],None)).await},|index|args[index].span)).await};
     facts.present=u32::from(subgroup.is_some());
    }
    CallNeed::SubgroupValue=>return unsafe{subgroup.take().unwrap_unchecked()},
    CallNeed::ReadView=>{
     view=unsafe{self.world.call(self.reference_expr(&args[0],None)).await}?;
     facts.present=u32::from(view.is_some());
    }
    CallNeed::UnsafeRead=>return unsafe{self.world.call(self.unsafe_read_view(view.take().unwrap_unchecked(),span,args[1].span,
     async |builder|unsafe{builder.world.call(builder.reference_expr(&args[1],None)).await})).await},
    CallNeed::InvalidReadArguments=>return self.fail(span,"B181","unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性"),
    CallNeed::FinalType=>{ty=Some(unsafe{self.world.call(self.world.ty(&TypeName::named(name),self.scope.source(),span,self.scope.module(),self.scope.imports())).await}?);}
    CallNeed::Construct=>return unsafe{self.world.call(self.record_values(ty.take().unwrap_unchecked(),args.len(),span,
     async |builder,index,expected|unsafe{builder.world.call(builder.reference_expr(&args[index],expected)).await})).await},
    CallNeed::IndexContext=>return self.fail(span,"B155","index() 只能在当前 program 的 parallel 域中使用"),
    CallNeed::OrdinalContext=>return self.fail(span,"B168","ordinal只能用于元素游标"),
    CallNeed::Builtin=>return unsafe{self.world.call(self.builtin_values(name,span,
     async |builder|unsafe{builder.world.call(builder.reference_arguments(args)).await})).await},
   }
  }
 }
}
