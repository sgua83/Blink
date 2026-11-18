//! B controls nominal-reference selection. Every real World child runs after
//! native stepping returns; this query retains its actual partial arguments.
use super::super::*;
use crate::builtin_types::{Builtin,Kind};
use std::{ffi::c_void,mem::{self,MaybeUninit},ptr,slice,str};

#[repr(C)]
#[derive(Clone,Copy)]
struct Name { bytes:*const u8,length:u64 }
impl Name {
    fn new(value:&str)->Self {Self{bytes:value.as_ptr(),length:value.len() as u64}}
    unsafe fn text<'a>(self)->&'a str {unsafe{str::from_utf8_unchecked(slice::from_raw_parts(self.bytes,self.length as usize))}}
}
#[repr(C)]
#[derive(Clone,Copy)]
struct Reference {kind:Kind,wrappers:u32,resource:u32,count:u32,arguments:u64,name:Name}
#[repr(C)]
struct Argument {reference:*const c_void,wrappers:u32,arguments:u64}
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
struct Frame<'a> {
    world:&'a World,input:&'a TypeName,source:&'a Source,span:Span,module:&'a str,imports:&'a [String],
    name:&'a str,resolved:Option<String>,arguments:Vec<GenericArgument>,count:Option<u32>,
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
    let record=&f.world.record(&key).0;
    let types=record.types().len() as u64;let count=types+record.values().len() as u64;f.resolved=Some(key);
    Parameters{present:1,types,count}
}
unsafe extern "C" fn reserve(raw:*mut c_void,count:u64) {unsafe{frame(raw)}.arguments.reserve(count as usize);}
unsafe extern "C" fn argument(raw:*mut c_void,index:u64)->Argument {
    let f=unsafe{frame(raw)};
    match &f.input.arguments[index as usize] {
        GenericArgument::Type(value)=>Argument{reference:(value as *const TypeName).cast(),wrappers:wrappers(value),arguments:value.arguments.len() as u64},
        GenericArgument::Value(_)=>Argument{reference:ptr::null(),wrappers:0,arguments:0},
    }
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
pub(super) async fn bind(world:&World,input:&TypeName,source:&Source,span:Span,module:&str,imports:&[String])->Result<TypeName> {
    let builtin=Builtin::recognize(&input.name);
    let name=builtin.canonical(&input.name).unwrap_or(&input.name);
    let descriptor=Reference{kind:builtin.kind,wrappers:wrappers(input),resource:u32::from(input.is_texture()||input.is_acceleration()),
        count:u32::from(input.count.is_some()),arguments:input.arguments.len() as u64,name:Name::new(name)};
    let mut f=Frame{world,input,source,span,module,imports,name,resolved:None,arguments:Vec::new(),count:None,result:MaybeUninit::uninit(),error:None};
    let mut cursor=unsafe{start(descriptor)};
    loop {
        let need=unsafe{step(&mut cursor,(&mut f as *mut Frame<'_>).cast(),Host{resource,record,reserve,argument,append_release,fail,finish})};
        if let Some(error)=f.error.take(){return Err(error);}
        match need {
            Need::Done=>return Ok(unsafe{f.result.assume_init()}),
            Need::Function=>return unsafe{world.call(world.c_function_reference(input,source,span,module,imports)).await}.map(|value|value.into_syntax()),
            Need::Type=>{
                let value=unsafe{&*cursor.reference.cast::<TypeName>()};
                let value=unsafe{world.call(bind(world,value,source,span,module,imports)).await}?;
                f.arguments.push(GenericArgument::Type(value));
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
