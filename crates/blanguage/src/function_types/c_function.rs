//! B owns C-signature selection; these adapters lend actual values and construct
//! their original semantic containers. No source or type tree is mirrored.
use crate::{ast::*,ir::{CFunctionType,CallParameter,Ty},Source,World,Diagnostic,Result};
use std::{collections::BTreeSet,ffi::c_void,mem::{self,MaybeUninit},ptr};

#[repr(C)]
struct Text {bytes:*const u8,length:u64}
#[repr(C)]
struct Type {reference:*const c_void,name:Text,arguments:u64,wrappers:u32,decorations:u32}
impl Type {
    fn of(value:&TypeName)->Self {
        Self{reference:(value as *const TypeName).cast(),name:Text{bytes:value.name.as_ptr(),length:value.name.len() as u64},
            arguments:value.arguments.len() as u64,wrappers:u32::from(value.dataset)|(u32::from(value.view)<<1)|(u32::from(value.tile)<<2),
            decorations:u32::from(value.count.is_some())|(u32::from(value.contracts.is_some())<<1)|(u32::from(value.canonical)<<2)}
    }
    fn absent()->Self {Self{reference:ptr::null(),name:Text{bytes:ptr::null(),length:0},arguments:0,wrappers:0,decorations:0}}
}
#[repr(C)]
struct Parameter {access:Access,value:Type}
#[repr(C)]
struct Layout {present:u32,dataset:u32,owned:u32,views:u32,empty:u32}
impl Layout {
    fn of(ty:&Ty,syntax:&TypeName)->Self {
        Self{present:1,dataset:u32::from(syntax.dataset),owned:u32::from(ty.owns_resources()),
            views:u32::from(ty.contains_views()),empty:u32::from(ty.components.is_empty())}
    }
    fn absent()->Self {Self{present:0,dataset:0,owned:0,views:0,empty:0}}
}
#[repr(C)]
struct ParameterLayout {access:Access,value:Layout}
#[repr(u32)]
enum Problem {MissingResult,TypeArgument,Position,Resource,EmptyOrView}
unsafe extern "C" {
    #[link_name="bootstrap.c_function::reference"]
    fn reference_b(source:Type,context:*mut c_void,
        argument:unsafe extern "C" fn(*const c_void,u64)->Type,
        bind_result:unsafe extern "C" fn(*mut c_void,*const c_void)->bool,
        reserve:unsafe extern "C" fn(*mut c_void,u64),
        bind_parameter:unsafe extern "C" fn(*mut c_void,u64,Access,*const c_void)->bool,
        fail:unsafe extern "C" fn(*mut c_void,Problem),finish:unsafe extern "C" fn(*mut c_void));
    #[link_name="bootstrap.c_function::declaration"]
    fn declaration_b(result:Type,count:u64,context:*mut c_void,
        parameter:unsafe extern "C" fn(*mut c_void,u64)->Parameter,
        bind_result:unsafe extern "C" fn(*mut c_void,*const c_void)->bool,
        bind_parameter:unsafe extern "C" fn(*mut c_void,u64,Access,*const c_void)->bool,
        finish:unsafe extern "C" fn(*mut c_void));
    #[link_name="bootstrap.c_function::signature"]
    fn signature_b(result:Type,count:u64,context:*mut c_void,
        parameter:unsafe extern "C" fn(*mut c_void,u64)->ParameterLayout,
        result_layout:unsafe extern "C" fn(*mut c_void)->Layout,
        append_value:unsafe extern "C" fn(*mut c_void),
        append_reference:unsafe extern "C" fn(*mut c_void,Access),
        use_result:unsafe extern "C" fn(*mut c_void),fail:unsafe extern "C" fn(*mut c_void,Problem),
        finish:unsafe extern "C" fn(*mut c_void,u32));
}
#[derive(Clone,Copy)]
struct Context<'a> {world:&'a World,source:&'a Source,span:Span,module:&'a str,imports:&'a [String]}
impl Context<'_> {
    fn reference(&self,input:&TypeName)->Result<TypeName> {self.world.type_reference(input,self.source,self.span,self.module,self.imports)}
    fn layout(&self,input:&TypeName)->Result<Ty> {self.world.ty_inner(input,self.source,self.span,self.module,self.imports,&mut BTreeSet::new())}
    fn error(&self,problem:Problem)->Diagnostic {
        let (code,message)=match problem {
            Problem::MissingResult=>("B263","cfn<R,P...>至少需要一个返回类型"),
            Problem::TypeArgument=>("B263","cfn签名只接受类型，不接受const数值"),
            Problem::Position=>("B263","cfn位置参数使用read<T>／write<T>／inout<T>"),
            Problem::Resource=>("B224","cfn普通C签名不能交接Blink拥有资源"),
            Problem::EmptyOrView=>("B263","cfn需要普通非空C布局，不能交接Blink资源引用"),
        };
        Diagnostic{source:self.source.clone(),span:self.span,code,message:message.into()}
    }
}
struct Binding<'a> {
    context:Context<'a>,outputs:&'a [ContractPredicate],result:Option<TypeName>,
    parameters:Vec<(Access,TypeName)>,output:MaybeUninit<CFunctionType>,error:Option<Diagnostic>,
}
unsafe fn binding<'a>(raw:*mut c_void)->&'a mut Binding<'a> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn argument(parent:*const c_void,index:u64)->Type {
    match &unsafe{&*parent.cast::<TypeName>()}.arguments[index as usize] {
        GenericArgument::Type(value)=>Type::of(value),GenericArgument::Value(_)=>Type::absent(),
    }
}
unsafe extern "C" fn bind_result(raw:*mut c_void,input:*const c_void)->bool {
    let f=unsafe{binding(raw)};
    match f.context.reference(unsafe{&*input.cast::<TypeName>()}) {
        Ok(value)=>{f.result=Some(value);true},Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn reserve(raw:*mut c_void,count:u64) {unsafe{binding(raw)}.parameters.reserve(count as usize);}
unsafe extern "C" fn append_parameter(raw:*mut c_void,_index:u64,access:Access,input:*const c_void)->bool {
    let f=unsafe{binding(raw)};
    match f.context.reference(unsafe{&*input.cast::<TypeName>()}) {
        Ok(value)=>{f.parameters.push((access,value));true},Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn declaration_parameter(raw:*mut c_void,index:u64)->Parameter {
    let (access,value)=&unsafe{binding(raw)}.parameters[index as usize];Parameter{access:*access,value:Type::of(value)}
}
unsafe extern "C" fn replace_parameter(raw:*mut c_void,index:u64,access:Access,input:*const c_void)->bool {
    let f=unsafe{binding(raw)};
    match f.context.reference(unsafe{&*input.cast::<TypeName>()}) {
        Ok(value)=>{f.parameters[index as usize]=(access,value);true},Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn binding_fail(raw:*mut c_void,problem:Problem) {let f=unsafe{binding(raw)};f.error=Some(f.context.error(problem));}
unsafe extern "C" fn finish_binding(raw:*mut c_void) {
    let f=unsafe{binding(raw)};let result=f.result.take().unwrap();let c=f.context;
    match crate::check::outputs::bind_function_contracts(c.world,&result,&f.parameters,f.outputs,c.source,c.span,c.module,c.imports) {
        Ok(contracts)=>{f.output.write(CFunctionType{result,parameters:mem::take(&mut f.parameters),contracts});},
        Err(error)=>f.error=Some(error),
    }
}
pub(super) fn reference(world:&World,input:&TypeName,source:&Source,span:Span,module:&str,imports:&[String])->Result<CFunctionType> {
    let mut f=Binding{context:Context{world,source,span,module,imports},outputs:input.contracts.as_deref().map_or(&[],|v|v.as_slice()),
        result:None,parameters:Vec::new(),output:MaybeUninit::uninit(),error:None};
    unsafe{reference_b(Type::of(input),(&mut f as *mut Binding<'_>).cast(),argument,bind_result,reserve,append_parameter,binding_fail,finish_binding)};
    match f.error {Some(error)=>Err(error),None=>Ok(unsafe{f.output.assume_init()})}
}
pub(super) fn declaration(world:&World,result:TypeName,parameters:Vec<(Access,TypeName)>,outputs:&[ContractPredicate],
    (source,span,module,imports):(&Source,Span,&str,&[String]))->Result<CFunctionType> {
    let mut f=Binding{context:Context{world,source,span,module,imports},outputs,result:None,parameters,output:MaybeUninit::uninit(),error:None};
    unsafe{declaration_b(Type::of(&result),f.parameters.len() as u64,(&mut f as *mut Binding<'_>).cast(),declaration_parameter,bind_result,replace_parameter,finish_binding)};
    match f.error {Some(error)=>Err(error),None=>Ok(unsafe{f.output.assume_init()})}
}
struct Signature<'a> {
    context:Context<'a>,function:&'a CFunctionType,current:Option<Ty>,parameters:Vec<CallParameter>,result:Option<Ty>,
    output:MaybeUninit<(Vec<CallParameter>,Option<Ty>,bool)>,error:Option<Diagnostic>,
}
unsafe fn signature_frame<'a>(raw:*mut c_void)->&'a mut Signature<'a> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn parameter_layout(raw:*mut c_void,index:u64)->ParameterLayout {
    let f=unsafe{signature_frame(raw)};let (access,syntax)=&f.function.parameters[index as usize];
    match f.context.layout(syntax) {
        Ok(ty)=>{let value=Layout::of(&ty,syntax);f.current=Some(ty);ParameterLayout{access:*access,value}},
        Err(error)=>{f.error=Some(error);ParameterLayout{access:*access,value:Layout::absent()}},
    }
}
unsafe extern "C" fn result_layout(raw:*mut c_void)->Layout {
    let f=unsafe{signature_frame(raw)};
    match f.context.layout(&f.function.result) {
        Ok(ty)=>{let value=Layout::of(&ty,&f.function.result);f.current=Some(ty);value},
        Err(error)=>{f.error=Some(error);Layout::absent()},
    }
}
unsafe extern "C" fn append_value(raw:*mut c_void) {let f=unsafe{signature_frame(raw)};f.parameters.push(CallParameter::value(f.current.take().unwrap()));}
unsafe extern "C" fn append_reference(raw:*mut c_void,access:Access) {let f=unsafe{signature_frame(raw)};f.parameters.push(CallParameter::reference(f.current.take().unwrap(),access));}
unsafe extern "C" fn use_result(raw:*mut c_void) {let f=unsafe{signature_frame(raw)};f.result=f.current.take();}
unsafe extern "C" fn signature_fail(raw:*mut c_void,problem:Problem) {let f=unsafe{signature_frame(raw)};f.error=Some(f.context.error(problem));}
unsafe extern "C" fn finish_signature(raw:*mut c_void,never:u32) {
    let f=unsafe{signature_frame(raw)};f.output.write((mem::take(&mut f.parameters),f.result.take(),never!=0));
}
pub(super) fn signature(world:&World,function:&CFunctionType,source:&Source,span:Span,module:&str,imports:&[String])
    ->Result<(Vec<CallParameter>,Option<Ty>,bool)> {
    let mut f=Signature{context:Context{world,source,span,module,imports},function,current:None,parameters:Vec::new(),result:None,output:MaybeUninit::uninit(),error:None};
    unsafe{signature_b(Type::of(&function.result),function.parameters.len() as u64,(&mut f as *mut Signature<'_>).cast(),
        parameter_layout,result_layout,append_value,append_reference,use_result,signature_fail,finish_signature)};
    match f.error {Some(error)=>Err(error),None=>Ok(unsafe{f.output.assume_init()})}
}
