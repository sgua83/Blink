//! B controls nominal-reference selection. Every real World child runs after
//! native stepping returns; this query retains its actual partial arguments.
use super::super::*;
use super::RecordDeclaration;
use crate::{builtin_types::{Builtin,Kind},syntax::wire::Optional};
use std::{borrow::Cow,ffi::c_void,mem::{self,MaybeUninit},ptr,slice,str,sync::Arc};

#[repr(C)]
#[derive(Clone,Copy)]
struct Name { bytes:*const u8,length:u64 }
impl Name {
    fn new(value:&str)->Self {Self{bytes:value.as_ptr(),length:value.len() as u64}}
    unsafe fn text<'a>(self)->&'a str {unsafe{str::from_utf8_unchecked(slice::from_raw_parts(self.bytes,self.length as usize))}}
}
#[repr(C)]
#[derive(Clone,Copy)]
struct Reference {kind:Kind,wrappers:u32,resource:u32,pending_count:u32,arguments:u64,name:Name}
#[repr(C)]
struct Argument {reference:*const c_void,wrappers:u32,arguments:u64,literal:Optional<u32>}
#[repr(C)]
struct Parameters {present:u32,types:u64,count:u64}
#[repr(u32)]
#[derive(Clone,Copy)]
enum Code {Owned=224,Void=228,Ordinary=232,Array=238,Result=269}
impl Code {
    fn text(self)->&'static str {match self {Self::Owned=>"B224",Self::Void=>"B228",Self::Ordinary=>"B232",Self::Array=>"B238",Self::Result=>"B269"}}
}
#[repr(u32)]
enum Problem {Arity,Type,ReleaseType,ReleaseForm}
#[repr(u32)]
enum Need {Done,Function,Type,Value,Count}
#[repr(C)]
struct Cursor {source:Reference,types:u64,count:u64,index:u64,code:Code,canonical:u32,stage:u32,reference:*const c_void}
#[repr(C)]
struct Host {
    resource:unsafe extern "C" fn(*mut c_void),
    record:unsafe extern "C" fn(*mut c_void)->Parameters,
    reserve:unsafe extern "C" fn(*mut c_void,u64),
    argument:unsafe extern "C" fn(*mut c_void,u64)->Argument,
    append_value:unsafe extern "C" fn(*mut c_void,u32),
    append_release:unsafe extern "C" fn(*mut c_void,*const c_void)->u32,
    fail:unsafe extern "C" fn(*mut c_void,Code,Problem,u64),
    finish:unsafe extern "C" fn(*mut c_void,u32,Name),
}
unsafe extern "C" {
    #[link_name="bootstrap.type_reference::start"]
    fn start(source:Reference)->Cursor;
    #[link_name="bootstrap.type_reference::step"]
    fn step(state:*mut Cursor,context:*mut c_void,host:Host)->Need;
}
struct Frame<'world, 'a> {
    world:&'world World,input:&'a TypeName,source:&'a Source,span:Span,module:&'a str,imports:&'a [String],
    name:&'a str,resolved:Option<String>,record:Option<&'world RecordDeclaration>,
    arguments:Vec<GenericArgument>,count:Option<u32>,
    result:MaybeUninit<TypeName>,error:Option<Diagnostic>,
}
unsafe fn frame<'world, 'a>(raw:*mut c_void)->&'a mut Frame<'world, 'a> {unsafe{&mut *raw.cast()}}
fn wrappers(value:&TypeName)->u32 {u32::from(value.dataset)|(u32::from(value.view)<<1)|(u32::from(value.tile)<<2)}
unsafe extern "C" fn resource(raw:*mut c_void) {
    let f=unsafe{frame(raw)};let mut value=f.input.clone();value.canonical=false;f.result.write(value);
}
unsafe extern "C" fn record(raw:*mut c_void)->Parameters {
    let f=unsafe{frame(raw)};
    let key=if f.input.canonical {f.world.records.get_key_value(&f.input.name).map(|(key,_)|key.clone())}
        else {f.world.resolve(&f.input.name,f.module,f.imports,false).unique()};
    let Some(key)=key else {
        f.error=Some(Diagnostic{source:f.source.clone(),span:f.span,code:"B031",message:format!("未知或不明确的类型 {}",f.input.name)});
        return Parameters{present:0,types:0,count:0};
    };
    let declaration=f.world.record(&key);
    let record=&declaration.0;
    let types=record.types().len() as u64;let count=types+record.values().len() as u64;
    f.record=Some(declaration);f.resolved=Some(key);
    Parameters{present:1,types,count}
}
unsafe extern "C" fn reserve(raw:*mut c_void,count:u64) {unsafe{frame(raw)}.arguments.reserve(count as usize);}
unsafe extern "C" fn argument(raw:*mut c_void,index:u64)->Argument {
    let f=unsafe{frame(raw)};
    match &f.input.arguments[index as usize] {
        GenericArgument::Type(value) => Argument { reference: (value as *const TypeName).cast(),
            wrappers: wrappers(value), arguments: value.arguments.len() as u64, literal: Optional::none() },
        GenericArgument::Value(value) => {
            let literal=match value {
                Shape::Literal(value) => Optional::some(*value),
                _ => Optional::none(),
            };
            Argument { reference: ptr::null(), wrappers: 0, arguments: 0, literal }
        }
    }
}
unsafe extern "C" fn append_value(raw:*mut c_void, value:u32) {
    unsafe { frame(raw) }.arguments.push(GenericArgument::Value(Shape::Literal(value)));
}
unsafe extern "C" fn append_release(raw:*mut c_void,value:*const c_void)->u32 {
    let f=unsafe{frame(raw)};let value=unsafe{&*value.cast::<TypeName>()};
    let key=if value.canonical && f.world.functions.contains_key(&value.name) {Some(value.name.clone())}
        else {f.world.resolve(&value.name,f.module,f.imports,true).unique()};
    match key {
        Some(key)=>{let mut value=TypeName::named(key);value.canonical=true;f.arguments.push(GenericArgument::Type(value));1},
        None=>{f.error=Some(Diagnostic{source:f.source.clone(),span:f.span,code:"B158",message:format!("未知或未导出的释放函数 {}",value.name)});0},
    }
}
unsafe extern "C" fn fail(raw:*mut c_void,code:Code,problem:Problem,expected:u64) {
    let f=unsafe{frame(raw)};
    let message=match problem {
        Problem::Arity=>format!("类型 {} 需要 {expected} 个类型及const实参，实际为 {}",f.resolved.as_deref().unwrap_or(f.name),f.input.arguments.len()),
        Problem::Type=>"此位置需要类型实参".into(),
        Problem::ReleaseType=>"owned<T,F>需要载荷类型与静态释放函数".into(),
        Problem::ReleaseForm=>"释放函数由载荷推导泛型，声明处使用函数名称".into(),
    };
    f.error=Some(Diagnostic{source:f.source.clone(),span:f.span,code:code.text(),message});
}
unsafe extern "C" fn finish(raw:*mut c_void,canonical:u32,name:Name) {
    let f=unsafe{frame(raw)};
    f.result.write(TypeName{name:f.resolved.take().unwrap_or_else(||unsafe{name.text()}.into()),canonical:canonical!=0,
        arguments:mem::take(&mut f.arguments),dataset:f.input.dataset,view:f.input.view,tile:f.input.tile,
        count:f.count.map(Shape::Literal),contracts:f.input.contracts.clone()});
}
pub(super) struct BoundReference<'a> {
    pub(super) name: TypeName,
    record: Option<&'a RecordDeclaration>,
}

pub(super) async fn bind<'world>(world:&'world World,input:&TypeName,source:&Source,span:Span,
    module:&str,imports:&[String],builtin:Builtin)->Result<BoundReference<'world>> {
    let name=builtin.canonical(&input.name).unwrap_or(&input.name);
    let (count,pending_count)=match input.count.as_ref() {
        None => (None,0),
        Some(Shape::Literal(value)) => (Some(*value),0),
        Some(_) => (None,1),
    };
    let descriptor=Reference{kind:builtin.kind,wrappers:wrappers(input),resource:u32::from(input.is_texture()||input.is_acceleration()),
        pending_count,arguments:input.arguments.len() as u64,name:Name::new(name)};
    let mut f=Frame{world,input,source,span,module,imports,name,resolved:None,record:None,arguments:Vec::new(),count,result:MaybeUninit::uninit(),error:None};
    let mut cursor=unsafe{start(descriptor)};
    loop {
        let need=unsafe{step(&mut cursor,(&mut f as *mut Frame<'_, '_>).cast(),Host{resource,record,reserve,argument,append_value,append_release,fail,finish})};
        if let Some(error)=f.error.take(){return Err(error);}
        match need {
            Need::Done=>return Ok(BoundReference{name:unsafe{f.result.assume_init()},record:f.record}),
            Need::Function=>return unsafe{world.call(world.c_function_reference(input,source,span,module,imports)).await}.map(|value|BoundReference{name:value.into_syntax(),record:None}),
            Need::Type=>{
                let value=unsafe{&*cursor.reference.cast::<TypeName>()};
                let builtin=Builtin::recognize(&value.name);
                let bound=unsafe{world.call(bind(world,value,source,span,module,imports,builtin)).await}?;
                f.arguments.push(GenericArgument::Type(bound.name));
            }
            Need::Value=>{
                let Some(value)=input.arguments[cursor.index as usize].shape()else{
                    return source.fail(span,"B235","const实参需要u32编译期表达式");
                };
                let value=unsafe{world.call(world.shape(&Some(value),source,span,module,imports)).await}?;
                f.arguments.push(GenericArgument::Value(Shape::Literal(value.unwrap())));
            }
            Need::Count=>f.count=unsafe{world.call(world.shape(&input.count,source,span,module,imports)).await}?,
        }
    }
}

#[repr(u32)]
enum PathStart { Bind, FunctionNumeric, FunctionField }

#[repr(C)]
struct ContractPath {
    index: u64,
    count: u64,
}

#[repr(u32)]
enum PathMember { Field, Variant, MissingType, MissingField }

#[repr(u32)]
enum PathAction {
    Layout, Specialize, Builtin, Length, Variant, LengthTail, VariantTail,
    MissingType, MissingField, RecordNumeric,
}

#[repr(C)]
struct PathInput { kind: Kind, wrappers: u32, builtin: u32, field: Name }

#[repr(C)]
struct PathHost {
    context: *mut c_void,
    member: unsafe extern "C" fn(*mut c_void, Name) -> PathMember,
    ordinary_record: unsafe extern "C" fn(*mut c_void) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.type_reference::contract_path_start"]
    fn contract_path_start(kind: Kind, wrappers: u32, remaining: u64) -> PathStart;
    #[link_name = "bootstrap.type_reference::contract_path_next"]
    fn contract_path_next(path: &mut ContractPath, input: PathInput, host: PathHost) -> PathAction;
}

struct PathLookup<'world, 'a> {
    world: &'world World,
    name: &'a TypeName,
    module: &'a str,
    imports: &'a [String],
    record: Option<&'world RecordDeclaration>,
    selected: Option<(&'world RecordDeclaration, crate::syntax::Type)>,
}

impl<'world> PathLookup<'world, '_> {
    fn declaration(&self) -> Option<&'world RecordDeclaration> {
        self.record.or_else(|| {
            self.world.resolve(&self.name.name, self.module, self.imports, false)
                .unique().map(|key| self.world.record(&key))
        })
    }
}

unsafe extern "C" fn path_member(raw: *mut c_void, field: Name) -> PathMember {
    let lookup = unsafe { &mut *raw.cast::<PathLookup<'_, '_>>() };
    let Some(declaration) = lookup.declaration() else { return PathMember::MissingType; };
    match declaration.0.contract_member(unsafe { field.text() }) {
        crate::syntax::ContractMember::Field(ty) => {
            lookup.selected = Some((declaration, ty));
            PathMember::Field
        }
        crate::syntax::ContractMember::Variant => PathMember::Variant,
        crate::syntax::ContractMember::Missing => PathMember::MissingField,
    }
}

unsafe extern "C" fn path_ordinary_record(raw: *mut c_void) -> u32 {
    let lookup = unsafe { &*raw.cast::<PathLookup<'_, '_>>() };
    u32::from(lookup.declaration().is_some_and(|(record, ..)| record.enumeration().is_none()))
}

impl World {
    /// Resolve each selected member in the scope of its declaration before laying out the leaf.
    pub(crate) async fn output_path_type(&self, input: &TypeName, fields: &[&str], source: &Source,
        span: Span, module: &str, imports: &[String],
    ) -> Result<ir::Ty> {
        let mut path = ContractPath { index: 0, count: fields.len() as u64 };
        let mut input = Cow::Borrowed(input);
        let mut scope = (source, module, imports);
        loop {
            let (source, module, imports) = scope;
            let builtin = Builtin::recognize(&input.name);
            let wrappers = wrappers(&input);
            match unsafe { contract_path_start(builtin.kind, wrappers, path.count - path.index) } {
                PathStart::Bind => {}
                use_kind => return source.fail(span,
                    match use_kind { PathStart::FunctionNumeric => "B148", _ => "B225" },
                    "函数签名没有数值运算或字段；原生地址通过function_address显式取得"),
            }

            let BoundReference { name, record } =
                unsafe { self.call(bind(self, &input, source, span, module, imports, builtin)).await }?;
            let field_index = path.index as usize;
            let field = fields.get(field_index).copied().unwrap_or_default();
            let mut lookup = PathLookup { world: self, name: &name, module, imports, record, selected: None };
            let descriptor = PathInput { kind: builtin.kind, wrappers, builtin: u32::from(builtin.is_present()),
                field: Name::new(field) };
            let host = PathHost { context: (&mut lookup as *mut PathLookup<'_, '_>).cast(),
                member: path_member, ordinary_record: path_ordinary_record };
            let action = unsafe { contract_path_next(&mut path, descriptor, host) };
            let (declaration, field_type) = match action {
                PathAction::Specialize => lookup.selected.unwrap(),
                PathAction::Layout => return unsafe { self.call(self.ty(&name, source, span, module, imports)).await },
                PathAction::Builtin => {
                    let ty = ir::Ty::from_builtin(builtin).unwrap();
                    let selected = fields[field_index..].iter().try_fold(Cow::Borrowed(&ty), |ty, field| {
                        crate::check::member_type_only(ty, field, source, span)
                    })?;
                    return Ok(selected.into_owned());
                }
                PathAction::Length => return Ok(ir::Ty::scalar(ir::Scalar::U32)),
                PathAction::Variant => return Ok(ir::Ty::scalar(ir::Scalar::Bool)),
                PathAction::LengthTail => return source.fail(span, "B225", "长度值没有后续字段"),
                PathAction::VariantTail => return source.fail(span, "B225", "变体条件没有后续字段"),
                PathAction::MissingType => return source.fail(span, "B031", format!("未知类型 {}", name.name)),
                PathAction::MissingField => return source.fail(span, "B225", format!("类型 {} 没有字段 {field}", name.name)),
                PathAction::RecordNumeric => return source.fail(span, "B147", "结构体和资源视图不能作为数值进行运算"),
            };
            let (record, record_source, record_module, record_imports) = declaration;
            let mut bindings = crate::generics::Bindings::default();
            let mut arguments = name.arguments.into_iter();
            for (parameter, argument) in record.types().iter().zip(&mut arguments) {
                let GenericArgument::Type(ty) = argument else {
                    return source.fail(span, "B232", "此记录参数需要类型实参");
                };
                bindings.add_type(parameter.name(), ty);
            }
            for (parameter, argument) in record.values().iter().zip(arguments) {
                let value = match argument {
                    GenericArgument::Value(Shape::Literal(value)) => value,
                    argument => unsafe {
                        self.call(self.shape(&Some(argument.shape().unwrap()), source, span, module, imports)).await
                    }?.unwrap(),
                };
                bindings.add_value(parameter.name(), value);
            }
            let bindings = Arc::new(bindings);
            scope = (record_source, record_module, record_imports);
            input = Cow::Owned(unsafe { self.call(bindings.type_name(self, &field_type, scope, span)).await }?);
        }
    }
}
