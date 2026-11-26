//! B selects nominal C signatures and classifies actual physical layouts.
//! World children execute outside native stepping; actual containers stay here.
use crate::{ast::*,ir::{CFunctionType,CallParameter,Ty},Source,World,Diagnostic,Result};
use std::{borrow::Cow, collections::BTreeSet, ffi::c_void, ptr};

#[repr(C)]
#[derive(Clone,Copy)]
struct Text {bytes:*const u8,length:u64}
#[repr(C)]
#[derive(Clone,Copy)]
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
struct Layout {dataset:u32,owned:u32,views:u32,empty:u32}
impl Layout {
    fn of(ty:&Ty,syntax:&TypeName)->Self {
        Self{dataset:u32::from(syntax.dataset),owned:u32::from(ty.owns_resources()),
            views:u32::from(ty.contains_views()),empty:u32::from(ty.components.is_empty())}
    }
}
#[repr(u32)]
enum Problem {MissingResult,TypeArgument,Position,Resource,EmptyOrView}
#[repr(u32)]
enum BindingNeed {Done,Result,AppendParameter,ReplaceParameter}
#[repr(C)]
struct BindingCursor {source:*const c_void,current:*const c_void,count:u64,index:u64,access:Access,mode:u32,stage:u32}
#[repr(C)]
struct BindingHost {
    argument:unsafe extern "C" fn(*const c_void,u64)->Type,
    reserve:unsafe extern "C" fn(*mut c_void,u64),
    parameter:unsafe extern "C" fn(*mut c_void,u64)->Parameter,
    fail:unsafe extern "C" fn(*mut c_void,Problem),
}
#[repr(u32)]
enum SignatureNeed {Done,Parameter,Result,ResultName}
#[repr(C)]
#[derive(Clone, Copy)]
struct ResultShape {
    name: Text,
    arguments: u64,
    wrappers: u32,
    decorations: u32,
    declaration: u32,
}
impl ResultShape {
    fn of(name: &TypeName, declaration: bool) -> Self {
        let source = Type::of(name);
        Self {
            name: source.name, arguments: source.arguments, wrappers: source.wrappers,
            decorations: source.decorations, declaration: u32::from(declaration),
        }
    }
}
#[repr(C)]
struct SignatureCursor {count:u64,index:u64,layout:Layout,access:Access,stage:u32,never:u32}
#[repr(C)]
struct SignatureHost {
    append_value:unsafe extern "C" fn(*mut c_void),
    append_reference:unsafe extern "C" fn(*mut c_void,Access),
    use_result:unsafe extern "C" fn(*mut c_void),
    fail:unsafe extern "C" fn(*mut c_void,Problem),
}
unsafe extern "C" {
    #[link_name="bootstrap.c_function::reference_start"]
    fn reference_start(source:*const c_void,count:u64)->BindingCursor;
    #[link_name="bootstrap.c_function::declaration_start"]
    fn declaration_start(result:*const c_void,count:u64)->BindingCursor;
    #[link_name="bootstrap.c_function::binding_step"]
    fn binding_step(state:*mut BindingCursor,context:*mut c_void,host:BindingHost)->BindingNeed;
    #[link_name="bootstrap.c_function::signature_start"]
    fn signature_start(count:u64)->SignatureCursor;
    #[link_name="bootstrap.c_function::signature_step"]
    fn signature_step(state:*mut SignatureCursor,result:ResultShape,context:*mut c_void,host:SignatureHost)->SignatureNeed;
}
#[derive(Clone,Copy)]
struct Context<'a> {world:&'a World,source:&'a Source,span:Span,module:&'a str,imports:&'a [String]}
impl Context<'_> {
    async fn reference(&self,input:&TypeName)->Result<TypeName> {
        unsafe { self.world.call(self.world.type_reference(input,self.source,self.span,self.module,self.imports)).await }
    }
    async fn layout(&self,input:&TypeName)->Result<Ty> {
        unsafe { self.world.call(self.world.ty_inner(input,self.source,self.span,self.module,self.imports,&mut BTreeSet::new())).await }
    }
    async fn declared_layout(&self, input: &TypeName) -> Result<Ty> {
        if input.dataset || input.tile || input.is_acceleration() || input.is_texture_resource() {
            return self.source.fail(self.span, "B263", "extern C 参数与结果需要普通C签名，不能传递dataset、tile或设备资源");
        }
        unsafe { self.world.call(self.world.ty(input, self.source, self.span, self.module, self.imports)).await }
    }
    fn error(&self,problem:Problem)->Diagnostic {
        let(code,message)=match problem {
            Problem::MissingResult=>("B263","cfn<R,P...>至少需要一个返回类型"),
            Problem::TypeArgument=>("B263","cfn签名只接受类型，不接受const数值"),
            Problem::Position=>("B263","cfn位置参数使用read<T>／write<T>／inout<T>／atomic<T>"),
            Problem::Resource=>("B224","cfn普通C签名不能交接Blink拥有资源"),
            Problem::EmptyOrView=>("B263","cfn需要普通非空C布局，不能交接Blink资源引用"),
        };
        Diagnostic{source:self.source.clone(),span:self.span,code,message:message.into()}
    }
}
struct Binding<'a> {context:Context<'a>,outputs:&'a [ContractPredicate],result:Option<TypeName>,parameters:Vec<(Access,TypeName)>,error:Option<Diagnostic>}
unsafe fn binding<'a>(raw:*mut c_void)->&'a mut Binding<'a> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn argument(parent:*const c_void,index:u64)->Type {
    match &unsafe{&*parent.cast::<TypeName>()}.arguments[index as usize] {
        GenericArgument::Type(value)=>Type::of(value),GenericArgument::Value(_)=>Type::absent(),
    }
}
unsafe extern "C" fn reserve(raw:*mut c_void,count:u64) {unsafe{binding(raw)}.parameters.reserve(count as usize);}
unsafe extern "C" fn declaration_parameter(raw:*mut c_void,index:u64)->Parameter {
    let(access,value)=&unsafe{binding(raw)}.parameters[index as usize];Parameter{access:*access,value:Type::of(value)}
}
unsafe extern "C" fn binding_fail(raw:*mut c_void,problem:Problem) {let f=unsafe{binding(raw)};f.error=Some(f.context.error(problem));}
async fn drive_binding(mut f:Binding<'_>,mut cursor:BindingCursor)->Result<CFunctionType> {
    let c=f.context;
    loop {
        let need=unsafe{binding_step(&mut cursor,(&mut f as *mut Binding<'_>).cast(),BindingHost{argument,reserve,parameter:declaration_parameter,fail:binding_fail})};
        if let Some(error)=f.error.take(){return Err(error);}
        match need {
            BindingNeed::Done=>break,
            BindingNeed::Result=>{
                let input=unsafe{&*cursor.current.cast::<TypeName>()};
                f.result=Some(unsafe{c.world.call(c.reference(input)).await}?);
            }
            BindingNeed::AppendParameter=>{
                let input=unsafe{&*cursor.current.cast::<TypeName>()};
                let value=unsafe{c.world.call(c.reference(input)).await}?;
                f.parameters.push((cursor.access,value));
            }
            BindingNeed::ReplaceParameter=>{
                let input=unsafe{&*cursor.current.cast::<TypeName>()};
                let value=unsafe{c.world.call(c.reference(input)).await}?;
                f.parameters[cursor.index as usize]=(cursor.access,value);
            }
        }
    }
    let result=f.result.take().unwrap();
    let contracts=unsafe{c.world.call(crate::check::outputs::bind_function_contracts(c.world,&result,&f.parameters,f.outputs,c.source,c.span,c.module,c.imports)).await}?;
    Ok(CFunctionType{result,parameters:f.parameters,contracts})
}
pub(super) async fn reference(world:&World,input:&TypeName,source:&Source,span:Span,module:&str,imports:&[String])->Result<CFunctionType> {
    let f=Binding{context:Context{world,source,span,module,imports},outputs:input.contracts.as_deref().map_or(&[],|v|v.as_slice()),
        result:None,parameters:Vec::new(),error:None};
    let cursor=unsafe{reference_start((input as *const TypeName).cast(),input.arguments.len() as u64)};
    unsafe{world.call(drive_binding(f,cursor)).await}
}
pub(super) async fn declaration(world:&World,result:TypeName,parameters:Vec<(Access,TypeName)>,outputs:&[ContractPredicate],
    (source,span,module,imports):(&Source,Span,&str,&[String]))->Result<CFunctionType> {
    let result=Box::new(result);
    let cursor=unsafe{declaration_start((&*result as *const TypeName).cast(),parameters.len() as u64)};
    let f=Binding{context:Context{world,source,span,module,imports},outputs,result:None,parameters,error:None};
    // The descriptor borrows this actual owned input at a fixed address until
    // the original normalization and its final contract consumer are finished.
    let output=unsafe{world.call(drive_binding(f,cursor)).await};
    drop(result);
    output
}
struct Signature<'a> {context:Context<'a>,declaration:bool,current:Option<Ty>,parameters:Vec<CallParameter>,result:Option<Ty>,error:Option<Diagnostic>}
unsafe fn signature_frame<'a>(raw:*mut c_void)->&'a mut Signature<'a> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn append_value(raw:*mut c_void) {let f=unsafe{signature_frame(raw)};f.parameters.push(CallParameter::value(f.current.take().unwrap()));}
unsafe extern "C" fn append_reference(raw:*mut c_void,access:Access) {let f=unsafe{signature_frame(raw)};f.parameters.push(CallParameter::reference(f.current.take().unwrap(),access));}
unsafe extern "C" fn use_result(raw:*mut c_void) {let f=unsafe{signature_frame(raw)};f.result=f.current.take();}
unsafe extern "C" fn signature_fail(raw: *mut c_void, problem: Problem) {
    let f = unsafe { signature_frame(raw) };
    let message = if f.declaration {
        match problem {
            Problem::Resource => Some(("B224", "拥有结果需要Frame所有权协议，不能跨当前普通C ABI")),
            Problem::EmptyOrView => Some(("B263", "C ABI 需要非空普通值布局，不能传递 Blink 资源引用")),
            _ => None,
        }
    } else {
        None
    };
    f.error = Some(match message {
        Some((code, message)) => Diagnostic {
            source: f.context.source.clone(), span: f.context.span, code, message: message.into(),
        },
        None => f.context.error(problem),
    });
}

#[derive(Clone, Copy)]
enum SignatureInput<'a> {
    Declaration(&'a crate::generics::Instance),
    Indirect(&'a CFunctionType),
}
impl<'a> SignatureInput<'a> {
    fn len(self) -> usize {
        match self {
            Self::Declaration(instance) => instance.definition.function.params().len(),
            Self::Indirect(function) => function.parameters.len(),
        }
    }

    async fn parameter(self, context: &mut Context<'a>, index: usize) -> Result<(Access, Cow<'a, TypeName>)> {
        match self {
            Self::Declaration(instance) => {
                let parameter = instance.definition.function.params().at(index);
                context.span = parameter.span();
                let name = unsafe { context.world.call(instance.bindings.type_name(
                    context.world, &parameter.ty(), (context.source, context.module, context.imports), context.span,
                )).await }?;
                Ok((parameter.access(), Cow::Owned(name)))
            }
            Self::Indirect(function) => {
                let (access, name) = &function.parameters[index];
                Ok((*access, Cow::Borrowed(name)))
            }
        }
    }

    async fn result_name(self, context: &mut Context<'a>) -> Result<Cow<'a, TypeName>> {
        match self {
            Self::Declaration(instance) => {
                let function = &instance.definition.function;
                context.span = function.span();
                let name = unsafe { context.world.call(instance.bindings.type_name(
                    context.world, &function.result(), (context.source, context.module, context.imports), context.span,
                )).await }?;
                Ok(Cow::Owned(name))
            }
            Self::Indirect(function) => Ok(Cow::Borrowed(&function.result)),
        }
    }
}

pub(crate) struct CheckedSignature<'a> {
    pub(crate) parameters: Vec<CallParameter>,
    pub(crate) result: Option<Ty>,
    pub(crate) never: bool,
    pub(crate) result_name: Cow<'a, TypeName>,
}
impl CheckedSignature<'_> {
    pub(crate) fn parameter_type(&self, index: usize) -> &Ty {
        use crate::ir::CallParameterKind;
        match &self.parameters[index].kind {
            CallParameterKind::Value(ty) | CallParameterKind::Reference {ty, ..} => ty,
        }
    }
}
async fn drive_signature<'a>(input: SignatureInput<'a>, context: Context<'a>) -> Result<CheckedSignature<'a>> {
    let declaration = matches!(input, SignatureInput::Declaration(_));
    let mut f = Signature {
        context, declaration, current: None, parameters: Vec::new(), result: None, error: None,
    };
    let mut cursor = unsafe { signature_start(input.len() as u64) };
    let mut result_name = None;
    let mut result_shape = ResultShape {
        name: Text {bytes: ptr::null(), length: 0}, arguments: 0, wrappers: 0, decorations: 0,
        declaration: u32::from(declaration),
    };
    loop {
        let need = unsafe { signature_step(
            &mut cursor, result_shape, (&mut f as *mut Signature<'_>).cast(),
            SignatureHost {append_value, append_reference, use_result, fail: signature_fail},
        ) };
        if let Some(error) = f.error.take() {
            return Err(error);
        }
        match need {
            SignatureNeed::Done => return Ok(CheckedSignature {
                parameters: f.parameters, result: f.result, never: cursor.never != 0,
                result_name: result_name.unwrap(),
            }),
            SignatureNeed::ResultName => {
                let name = unsafe { context.world.call(input.result_name(&mut f.context)).await }?;
                result_shape = ResultShape::of(&name, declaration);
                result_name = Some(name);
            }
            SignatureNeed::Parameter | SignatureNeed::Result => {
                let syntax = match need {
                    SignatureNeed::Parameter => {
                        let (access, syntax) = unsafe { context.world.call(
                            input.parameter(&mut f.context, cursor.index as usize),
                        ).await }?;
                        cursor.access = access;
                        syntax
                    }
                    _ => Cow::Borrowed(result_name.as_deref().unwrap()),
                };
                let ty = if declaration {
                    unsafe { context.world.call(f.context.declared_layout(&syntax)).await }?
                } else {
                    unsafe { context.world.call(f.context.layout(&syntax)).await }?
                };
                cursor.layout = Layout::of(&ty, &syntax);
                f.current = Some(ty);
            }
        }
    }
}

pub(crate) async fn named_signature<'a>(world: &'a World, instance: &'a crate::generics::Instance)
    -> Result<CheckedSignature<'a>> {
    let d = &instance.definition;
    let context = Context {
        world, source: &d.source, span: d.function.span(), module: &d.module, imports: &d.imports,
    };
    unsafe { world.call(drive_signature(SignatureInput::Declaration(instance), context)).await }
}

pub(super) async fn signature(world: &World, function: &CFunctionType, source: &Source,
    span: Span, module: &str, imports: &[String]) -> Result<(Vec<CallParameter>, Option<Ty>, bool)> {
    let context = Context {world, source, span, module, imports};
    let signature = unsafe { world.call(drive_signature(SignatureInput::Indirect(function), context)).await }?;
    Ok((signature.parameters, signature.result, signature.never))
}
