//! B selects and traverses nominal references; World and the actual semantic
//! containers keep their original ownership and synchronous lifetime.
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
struct Reference {kind:Kind,wrappers:u32,resource:u32,arguments:u64,name:Name}
#[repr(C)]
struct Argument {reference:*const c_void,wrappers:u32,arguments:u64}
#[repr(C)]
struct Parameters {present:u32,types:u64,count:u64}
#[repr(u32)]
enum Code {Owned=224,Void=228,Ordinary=232,Array=238,Result=269}
impl Code {
    fn text(self)->&'static str {match self {Self::Owned=>"B224",Self::Void=>"B228",Self::Ordinary=>"B232",Self::Array=>"B238",Self::Result=>"B269"}}
}
#[repr(u32)]
enum Problem {Arity,Type,ReleaseType,ReleaseForm}
unsafe extern "C" {
    #[link_name="bootstrap.type_reference::bind"]
    fn bind_b(source:Reference,context:*mut c_void,
        function:unsafe extern "C" fn(*mut c_void),resource:unsafe extern "C" fn(*mut c_void),
        record:unsafe extern "C" fn(*mut c_void)->Parameters,reserve:unsafe extern "C" fn(*mut c_void,u64),
        argument:unsafe extern "C" fn(*mut c_void,u64)->Argument,
        append_type:unsafe extern "C" fn(*mut c_void,*const c_void)->bool,
        append_release:unsafe extern "C" fn(*mut c_void,*const c_void)->bool,
        append_value:unsafe extern "C" fn(*mut c_void,u64)->bool,
        fail:unsafe extern "C" fn(*mut c_void,Code,Problem,u64),finish:unsafe extern "C" fn(*mut c_void,u32,Name));
}
struct Frame<'a> {
    world:&'a World,input:&'a TypeName,source:&'a Source,span:Span,module:&'a str,imports:&'a [String],
    name:&'a str,resolved:Option<String>,arguments:Vec<GenericArgument>,result:MaybeUninit<TypeName>,error:Option<Diagnostic>,
}
unsafe fn frame<'a>(raw:*mut c_void)->&'a mut Frame<'a> {unsafe{&mut *raw.cast()}}
fn wrappers(value:&TypeName)->u32 {u32::from(value.dataset)|(u32::from(value.view)<<1)|(u32::from(value.tile)<<2)}
unsafe extern "C" fn function(raw:*mut c_void) {
    let f=unsafe{frame(raw)};
    match f.world.c_function_reference(f.input,f.source,f.span,f.module,f.imports) {
        Ok(value)=>{f.result.write(value.into_syntax());},Err(error)=>f.error=Some(error),
    }
}
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
unsafe extern "C" fn append_type(raw:*mut c_void,value:*const c_void)->bool {
    let f=unsafe{frame(raw)};let value=unsafe{&*value.cast::<TypeName>()};
    match bind(f.world,value,f.source,f.span,f.module,f.imports) {
        Ok(value)=>{f.arguments.push(GenericArgument::Type(value));true},Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn append_release(raw:*mut c_void,value:*const c_void)->bool {
    let f=unsafe{frame(raw)};let value=unsafe{&*value.cast::<TypeName>()};
    let key=if value.canonical && f.world.functions.contains_key(&value.name) {Some(value.name.clone())}
        else {f.world.resolve(&value.name,f.module,f.imports,true).unique()};
    match key {
        Some(key)=>{let mut value=TypeName::named(key);value.canonical=true;f.arguments.push(GenericArgument::Type(value));true},
        None=>{f.error=Some(Diagnostic{source:f.source.clone(),span:f.span,code:"B158",message:format!("未知或未导出的释放函数 {}",value.name)});false},
    }
}
unsafe extern "C" fn append_value(raw:*mut c_void,index:u64)->bool {
    let f=unsafe{frame(raw)};
    let Some(value)=f.input.arguments[index as usize].shape() else {
        f.error=Some(Diagnostic{source:f.source.clone(),span:f.span,code:"B235",message:"const实参需要u32编译期表达式".into()});return false;
    };
    match f.world.shape(&Some(value),f.source,f.span,f.module,f.imports) {
        Ok(value)=>{f.arguments.push(GenericArgument::Value(Shape::Literal(value.unwrap())));true},
        Err(error)=>{f.error=Some(error);false},
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
    match f.world.shape(&f.input.count,f.source,f.span,f.module,f.imports) {
        Ok(count)=>{f.result.write(TypeName{name:f.resolved.take().unwrap_or_else(||unsafe{name.text()}.into()),canonical:canonical!=0,
            arguments:mem::take(&mut f.arguments),dataset:f.input.dataset,view:f.input.view,tile:f.input.tile,
            count:count.map(Shape::Literal),contracts:f.input.contracts.clone()});},
        Err(error)=>f.error=Some(error),
    }
}
pub(super) fn bind(world:&World,input:&TypeName,source:&Source,span:Span,module:&str,imports:&[String])->Result<TypeName> {
    let builtin=Builtin::recognize(&input.name);
    let name=builtin.canonical(&input.name).unwrap_or(&input.name);
    let descriptor=Reference{kind:builtin.kind,wrappers:wrappers(input),resource:u32::from(input.is_texture()||input.is_acceleration()),
        arguments:input.arguments.len() as u64,name:Name::new(name)};
    let mut f=Frame{world,input,source,span,module,imports,name,resolved:None,arguments:Vec::new(),result:MaybeUninit::uninit(),error:None};
    unsafe{bind_b(descriptor,(&mut f as *mut Frame<'_>).cast(),function,resource,record,reserve,argument,append_type,append_release,append_value,fail,finish)};
    match f.error {Some(error)=>Err(error),None=>Ok(unsafe{f.result.assume_init()})}
}
