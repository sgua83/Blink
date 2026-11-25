//! B controls nominal-reference selection. Every real World child runs after
//! native stepping returns; this query retains its actual partial arguments.
use super::super::*;
use super::RecordDeclaration;
use crate::{builtin_types::{Builtin,Kind},syntax::wire::Optional};
use std::{collections::BTreeMap,ffi::c_void,mem::{self,MaybeUninit},ptr,slice,str,sync::Arc};

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
#[repr(u32)]
enum Projection { Member, Length, LengthTail, Variant }
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
    #[link_name="bootstrap.type_reference::contract_projection"]
    fn contract_projection(kind:Kind,wrappers:u32,field:Name,tail:u32)->Projection;
}
struct Frame<'a> {
    world:&'a World,input:&'a TypeName,source:&'a Source,span:Span,module:&'a str,imports:&'a [String],
    name:&'a str,resolved:Option<String>,record:Option<&'a RecordDeclaration>,
    arguments:Vec<GenericArgument>,count:Option<u32>,
    result:MaybeUninit<TypeName>,error:Option<Diagnostic>,
}
unsafe fn frame<'a>(raw:*mut c_void)->&'a mut Frame<'a> {unsafe{&mut *raw.cast()}}
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

pub(super) async fn bind<'a>(world:&'a World,input:&'a TypeName,source:&'a Source,span:Span,
    module:&'a str,imports:&'a [String],builtin:Builtin)->Result<BoundReference<'a>> {
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
        let need=unsafe{step(&mut cursor,(&mut f as *mut Frame<'_>).cast(),Host{resource,record,reserve,argument,append_value,append_release,fail,finish})};
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

fn variant_condition(rest: &[&str], source: &Source, span: Span) -> Result<ir::Ty> {
    if rest.is_empty() {
        Ok(ir::Ty::scalar(ir::Scalar::Bool))
    } else {
        source.fail(span, "B225", "变体条件没有后续字段")
    }
}

impl World {
    /// Contract paths select nominal fields before requesting a layout.
    /// A callback can refer to the record that holds it.
    pub(crate) async fn output_path_type(&self, input: &TypeName, fields: &[&str], source: &Source,
        span: Span, module: &str, imports: &[String],
    ) -> Result<ir::Ty> {
        let builtin = Builtin::recognize(&input.name);
        let flags = wrappers(input);
        if builtin.kind == Kind::CFunction && flags == 0 {
            return source.fail(span, if fields.is_empty() { "B148" } else { "B225" },
                "函数签名没有数值运算或字段；原生地址通过function_address显式取得");
        }
        let BoundReference { name, record } =
            unsafe { self.call(bind(self, input, source, span, module, imports, builtin)).await }?;
        let declaration = || record.or_else(|| {
            self.resolve(&name.name, module, imports, false).unique().map(|key| self.record(&key))
        });
        if let Some((field, rest)) = fields.split_first() {
            match unsafe { contract_projection(builtin.kind, flags, Name::new(field), u32::from(!rest.is_empty())) } {
                Projection::Length => return Ok(ir::Ty::scalar(ir::Scalar::U32)),
                Projection::LengthTail => return source.fail(span, "B225", "长度值没有后续字段"),
                Projection::Variant => return variant_condition(rest, source, span),
                Projection::Member => {}
            }
            if let Some(ty) = ir::Ty::from_builtin(builtin) {
                let selected = fields.iter().try_fold(std::borrow::Cow::Borrowed(&ty), |ty, field| {
                    crate::check::member_type_only(ty, field, source, span)
                })?;
                return Ok(selected.into_owned());
            }
            let Some((record, record_source, record_module, record_imports)) = declaration() else {
                return source.fail(span, "B031", format!("未知类型 {}", name.name));
            };
            let selected = match record.contract_member(field) {
                crate::syntax::ContractMember::Field(ty) => ty,
                crate::syntax::ContractMember::Variant => return variant_condition(rest, source, span),
                crate::syntax::ContractMember::Missing => {
                    return source.fail(span, "B225", format!("类型 {} 没有字段 {field}", name.name));
                }
            };
            let mut arguments = name.arguments.into_iter();
            let types = record.types().iter().zip(&mut arguments).map(|(parameter, argument)| {
                let GenericArgument::Type(ty) = argument else {
                    return source.fail(span, "B232", "此记录参数需要类型实参");
                };
                Ok((parameter.name().to_owned(), ty))
            }).collect::<Result<_>>()?;
            let mut values = BTreeMap::new();
            for (parameter, argument) in record.values().iter().zip(arguments) {
                let value = match argument {
                    GenericArgument::Value(Shape::Literal(value)) => value,
                    argument => unsafe {
                        self.call(self.shape(&Some(argument.shape().unwrap()),source,span,module,imports)).await
                    }?.unwrap(),
                };
                values.insert(parameter.name().to_owned(), value);
            }
            let bindings = Arc::new(crate::generics::Bindings { types, values });
            let selected = unsafe {
                self.call(bindings.type_name(self, &selected, (record_source, record_module, record_imports), span)).await
            }?;
            return unsafe { self.call(self.output_path_type(&selected, rest, record_source, span, record_module, record_imports)).await };
        }

        // An ordinary record is not a numeric input; do not expand recursive callbacks for its layout.
        if let Some((record, ..)) = declaration() {
            if record.enumeration().is_none() {
                return source.fail(span, "B147", "结构体和资源视图不能作为数值进行运算");
            }
        }
        unsafe { self.call(self.ty(&name,source,span,module,imports)).await }
    }
}
