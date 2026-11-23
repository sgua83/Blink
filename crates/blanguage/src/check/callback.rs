//! 明确C ABI定义的原生符号直接交给调用者；不创建回调宿主或Frame。
use super::*;

use std::{ffi::c_void,ptr};
#[repr(C)]#[derive(Clone,Copy)]struct NamedCursor{index:u64,phase:u32}
#[repr(C)]struct NamedFacts{present:u32,c_abi:u32,parameters:u64}
#[repr(u32)]enum NamedNeed{Resolve,Unknown,CAbi,InvalidAbi,Arguments,Instance,Contracts,Result,Parameter,Bind,Complete}
#[repr(C)]struct NamedStep{cursor:NamedCursor,need:NamedNeed}
#[repr(C)]#[derive(Clone,Copy)]struct Text{bytes:*const u8,length:u64}
impl Text{fn of(value:&str)->Self{Self{bytes:value.as_ptr(),length:value.len()as u64}}}
#[repr(C)]struct ReferenceContract{present:u32,kind:u32,predicate:u32,name:*mut c_void,condition:*mut c_void}
#[repr(C)]struct ReferenceCondition{kind:u32,left:*mut c_void,right:*mut c_void}
#[repr(C)]struct ReferenceHost{
 next:unsafe extern "C" fn(*mut c_void)->ReferenceContract,
 append:unsafe extern "C" fn(*mut c_void),
 parameter:unsafe extern "C" fn(*mut c_void,u64)->Text,
 name:unsafe extern "C" fn(*mut c_void)->Text,
 rename:unsafe extern "C" fn(*mut c_void,u64,u64),
 condition:unsafe extern "C" fn(*mut c_void)->ReferenceCondition,
}
#[repr(C)]#[derive(Clone,Copy)]struct AddressCursor{phase:u32}
#[repr(C)]struct AddressFacts{cpu:u32,application:u32,external:u32,declaration:u32}
#[repr(u32)]enum AddressNeed{BadTarget,Application,Signature,ExternalKind,ExternalTarget,NativeTarget,Register,Emit,Declaration,Reference,Complete}
#[repr(C)]struct AddressStep{cursor:AddressCursor,need:AddressNeed}
unsafe extern "C"{
 #[link_name="bootstrap.c_function::named_reference_step"]fn named_reference_step(cursor:NamedCursor,facts:NamedFacts)->NamedStep;
 #[link_name="bootstrap.c_function::reference_contracts"]fn project_contracts(context:*mut c_void,parameters:u64,host:ReferenceHost);
 #[link_name="bootstrap.c_function::address_value_step"]fn address_value_step(cursor:AddressCursor,facts:AddressFacts)->AddressStep;
}
struct Contracts<I>{parameters:crate::syntax::List<crate::syntax::wire::ParamNode>,source:I,current:Option<FunctionContract>,output:Vec<ContractPredicate>}
unsafe extern "C" fn next_contract<I:Iterator<Item=FunctionContract>>(context:*mut c_void)->ReferenceContract{
 let state=unsafe{&mut *context.cast::<Contracts<I>>()};state.current=state.source.next();
 let mut view=ReferenceContract{present:0,kind:0,predicate:0,name:ptr::null_mut(),condition:ptr::null_mut()};
 if let Some(contract)=&mut state.current{view.present=1;view.kind=contract.kind as u32;match &mut contract.predicate{
  ContractPredicate::Nonzero(name)=>{view.predicate=1;view.name=ptr::from_mut(name).cast();},
  ContractPredicate::Memory(_)=>view.predicate=2,
  ContractPredicate::Initialized{value,condition}=>{view.predicate=3;view.name=ptr::from_mut(value).cast();view.condition=ptr::from_mut(condition).cast();},
  _=>{},
 }}view
}
unsafe extern "C" fn append_contract<I>(context:*mut c_void){let state=unsafe{&mut *context.cast::<Contracts<I>>()};state.output.push(state.current.take().unwrap().predicate);}
unsafe extern "C" fn parameter<I>(context:*mut c_void,index:u64)->Text{let state=unsafe{&*context.cast::<Contracts<I>>()};Text::of(state.parameters.at(index as usize).name())}
unsafe extern "C" fn name(value:*mut c_void)->Text{Text::of(unsafe{&*value.cast::<String>()})}
unsafe extern "C" fn rename(value:*mut c_void,index:u64,end:u64){unsafe{&mut *value.cast::<String>()}.replace_range(..end as usize,&format!("argument{index}"));}
unsafe extern "C" fn condition(value:*mut c_void)->ReferenceCondition{
 let mut view=ReferenceCondition{kind:0,left:ptr::null_mut(),right:ptr::null_mut()};match unsafe{&mut *value.cast::<OutputCondition>()}{
  OutputCondition::Name(name)=>{view.kind=1;view.left=ptr::from_mut(name).cast();},
  OutputCondition::Member(base,_)|OutputCondition::Unary(_,base)=>{view.kind=2;view.left=ptr::from_mut(&mut **base).cast();},
  OutputCondition::Binary(_,left,right)=>{view.kind=3;view.left=ptr::from_mut(&mut **left).cast();view.right=ptr::from_mut(&mut **right).cast();},
  _=>{},
 }view
}
fn reference_contracts<I:Iterator<Item=FunctionContract>>(source:I,parameters:crate::syntax::List<crate::syntax::wire::ParamNode>)->Vec<ContractPredicate>{
 let count=parameters.len()as u64;let mut state=Contracts{parameters,source,current:None,output:Vec::new()};
 unsafe{project_contracts(ptr::from_mut(&mut state).cast(),count,ReferenceHost{next:next_contract::<I>,append:append_contract::<I>,parameter:parameter::<I>,name,rename,condition})};state.output
}

pub(super) async fn signature(world: &World, caller: &LexicalScope, args: &Expressions, span: Span) -> Result<(crate::generics::Instance, Ty)> {
    if args.len()!=1 {
        return caller.source().fail(span, "B153", "c_function需要一个明确的C ABI函数引用");
    }
    let reference=&args.at(0);
    let (name, explicit) = match reference.kind() {
        ExprKind::Name(name) => (name, None),
        ExprKind::FunctionRef(name, arguments) => (name, Some(arguments)),
        _ => return caller.source().fail(reference.span(), "B153", "c_function需要C ABI函数名及具体类型／const实参"),
    };
    unsafe { world.call(signature_named(world,caller,name,span,reference.span(),async ||{
        match explicit.as_ref() {
            Some(arguments)=>unsafe { world.call(caller.bindings().arguments(world,arguments,
                (caller.source(),caller.module(),caller.imports()),reference.span())).await }.map(|v|Some(std::borrow::Cow::Owned(v))),
            None=>Ok(None),
        }
    })).await }
}
pub(super) async fn signature_named<'a>(world:&World,caller:&LexicalScope,name:&str,span:Span,reference_span:Span,
 arguments:impl std::ops::AsyncFnOnce()->Result<Option<std::borrow::Cow<'a,[GenericArgument]>>>)->Result<(crate::generics::Instance,Ty)>{
 let mut cursor=NamedCursor{index:0,phase:0};let mut facts=NamedFacts{present:0,c_abi:0,parameters:0};
 let mut key=None;let mut arguments=Some(arguments);let mut explicit=None;let mut instance:Option<crate::generics::Instance>=None;
 let mut contracts=None;let mut result=None;let mut parameters=Vec::new();let mut ty=None;
 loop{let step=unsafe{named_reference_step(cursor,NamedFacts{..facts})};cursor=step.cursor;match step.need{
  NamedNeed::Resolve=>{key=world.resolve(name,caller.module(),caller.imports(),true).unique();facts.present=u32::from(key.is_some());},
  NamedNeed::Unknown=>return caller.source().fail(span,"B158",format!("未知函数 {name}")),
  NamedNeed::CAbi=>facts.c_abi=u32::from(world.functions[key.as_ref().unwrap()].function.c_abi()),
  NamedNeed::InvalidAbi=>return caller.source().fail(span,"B263","c_function需要明确的extern C声明或定义，不自动改变普通B函数ABI"),
  NamedNeed::Arguments=>explicit=arguments.take().unwrap()().await?,
  NamedNeed::Instance=>{instance=Some(unsafe{world.call(generics::specialize(world,key.as_ref().unwrap(),&[],explicit.as_deref(),(caller.source(),caller.module(),caller.imports()),reference_span)).await}?);explicit.take();facts.parameters=instance.as_ref().unwrap().definition.function.params().len()as u64;},
  NamedNeed::Contracts=>{let value=instance.as_ref().unwrap();contracts=Some(reference_contracts(value.contracts(),value.definition.function.params()));},
  NamedNeed::Result=>{let value=instance.as_ref().unwrap();let d=&value.definition;result=Some(unsafe{world.call(value.bindings.type_name(world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())).await}?);},
  NamedNeed::Parameter=>{let value=instance.as_ref().unwrap();let d=&value.definition;let parameter=d.function.params().at(cursor.index as usize);let ty=unsafe{world.call(value.bindings.type_name(world,&parameter.ty(),(&d.source,&d.module,&d.imports),parameter.span())).await}?;parameters.push((parameter.access(),ty));},
  NamedNeed::Bind=>{let d=&instance.as_ref().unwrap().definition;ty=Some(unsafe{world.call(world.bind_c_function_reference(result.take().unwrap(),std::mem::take(&mut parameters),contracts.as_ref().unwrap(),(&d.source,d.function.span(),&d.module,&d.imports))).await}?.into_ty());},
  NamedNeed::Complete=>return Ok((instance.take().unwrap(),ty.take().unwrap())),
 }}
}
impl Builder<'_> {
    pub(super) async fn c_function_value(&mut self, args: &Expressions, span: Span) -> Result<Value> {
        unsafe { self.world.call(self.c_function_from(span,async |builder|unsafe { builder.world.call(signature(builder.world,&builder.scope,args,span)).await })).await }
    }
    pub(super) async fn c_function_from(&mut self,span:Span,
        signature:impl std::ops::AsyncFnOnce(&mut Self)->Result<(crate::generics::Instance,Ty)>)->Result<Value>{
        let mut cursor=AddressCursor{phase:0};let mut facts=AddressFacts{cpu:u32::from(self.program.target==Target::Cpu),application:0,external:0,declaration:0};
        let mut signature=Some(signature);let mut instance:Option<crate::generics::Instance>=None;let mut ty=None;let mut target=None;let mut reg=None;let mut first=0;let mut declaration=None;
        loop{let step=unsafe{address_value_step(cursor,AddressFacts{..facts})};cursor=step.cursor;match step.need{
            AddressNeed::BadTarget=>return self.fail(span,"B265","c_function在普通CPU应用或模块中取得C函数符号地址"),
            AddressNeed::Application=>facts.application=u32::from(self.application_entry()),
            AddressNeed::Signature=>{let(value,signature_ty)=signature.take().unwrap()(self).await?;instance=Some(value);ty=Some(signature_ty);},
            AddressNeed::ExternalKind=>facts.external=u32::from(instance.as_ref().unwrap().definition.function.external().is_some()),
            AddressNeed::ExternalTarget=>{let name=instance.as_ref().unwrap().definition.function.external().unwrap();target=Some(CallTarget::External(ExternalName{library:name.library().into(),symbol:name.symbol().into()}));},
            AddressNeed::NativeTarget=>{unsafe{self.world.call(self.native_declaration(instance.as_ref().unwrap())).await}?;target=Some(CallTarget::Defined(std::mem::take(&mut instance.as_mut().unwrap().name)));},
            AddressNeed::Register=>{reg=Some(self.reg(Scalar::U64));first=self.code.len();},
            AddressNeed::Emit=>self.emit(InstKind::FunctionAddress(reg.unwrap(),target.take().unwrap())),
            AddressNeed::Declaration=>{declaration=instance.as_ref().unwrap().definition.declaration_origin().map(Arc::new);facts.declaration=u32::from(declaration.is_some());},
            AddressNeed::Reference=>{for instruction in &mut self.code[first..]{instruction.reference_declaration(declaration.as_ref().unwrap().clone());}},
            AddressNeed::Complete=>{let mut proof=bounds::Proof::default();proof.mark_nonzero(0);return Ok(Value{proof,literal:false,ty:ty.take().unwrap(),regs:vec![reg.unwrap()],uniform:true,constant:None});},
        }}
    }
}
