//! The original typed contract fields and the owning output byte vector.
//! B selects the wire order/tags and recursively writes the actual fields.
use crate::{ast, ir};
use super::Writer;
use std::{ffi::c_void, ptr};
type Address = *const c_void;
fn address<T>(value: &T) -> Address { ptr::from_ref(value).cast() }
fn optional<T>(value: Option<&T>) -> Address { value.map_or(ptr::null(), address) }

macro_rules! pod {
    ($name:ident { $($field:ident:$ty:ty),* $(,)? }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct $name { $($field:$ty),* }
    };
}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub(crate) struct Bytes { pub(crate) data:*const u8,pub(crate) count:usize }
impl Bytes { fn of(value:&str)->Self { Self { data:value.as_ptr(),count:value.len() } } }
pod!(Span { data:Address,count:usize,stride:usize });
impl Span { fn of<T>(values:&[T])->Self { Self { data:values.as_ptr().cast(),count:values.len(),stride:std::mem::size_of::<T>() } } }
pod!(Component { scalar:u32,word:u32 });
pod!(Field { name:Bytes,word:u32,ty:Address });
pod!(Variant { name:Bytes,discriminant:u32,ty:Address });
pod!(Parameter { access:u32,ty:Address });
pod!(Function { result:Address,parameters:Span,contracts:Span });
pod!(Type {
    name:Bytes,arguments:Span,values:Span,size:u32,alignment:u32,
    components:Span,fields:Span,view:Address,array:Address,
    enumeration:u32,result:u32,variants:Span,dataset:Address,
    cleanup:Bytes,cleanup_present:u32,function:Address,
});
pod!(TypeName {
    name:Bytes,canonical:u32,arguments:Span,dataset:u32,view:u32,tile:u32,
    count:Address,contracts:Span,contracts_present:u32,
});
pod!(Argument { kind:u32,value:Address });
pod!(Shape { kind:u32,literal:u32,name:Bytes,expression:Address });
pod!(Expression {
    kind:u32,literal:u32,name:Bytes,a:Address,b:Address,c:Address,
    arguments:Span,types:Span,types_present:u32,ty:Address,target:u32,
    start:usize,end:usize,
});
pod!(Predicate {
    kind:u32,first:Bytes,second:Bytes,third:Bytes,condition:Address,
    intervals:Span,access:u32,access_present:u32,
});
pod!(Interval { lower:Address,upper:Address });
pod!(Condition { kind:u32,literal:u32,name:Bytes,a:Address,b:Address,constant:Address });
pod!(Constant { ty:Address,bits:Span });
#[repr(C)]
struct Host {
    append:unsafe extern "C" fn(*mut c_void,*const u8,usize),
    ty:unsafe extern "C" fn(Address)->Type,
    component:unsafe extern "C" fn(Address)->Component,
    field:unsafe extern "C" fn(Address)->Field,
    variant:unsafe extern "C" fn(Address)->Variant,
    function:unsafe extern "C" fn(Address)->Function,
    parameter:unsafe extern "C" fn(Address)->Parameter,
    type_name:unsafe extern "C" fn(Address)->TypeName,
    argument:unsafe extern "C" fn(Address)->Argument,
    shape:unsafe extern "C" fn(Address)->Shape,
    expression:unsafe extern "C" fn(Address)->Expression,
    predicate:unsafe extern "C" fn(Address)->Predicate,
    interval:unsafe extern "C" fn(Address)->Interval,
    condition:unsafe extern "C" fn(Address)->Condition,
    constant:unsafe extern "C" fn(Address)->Constant,
}
unsafe extern "C" {
    #[link_name="codec.value_format::write_type"]
    fn write_type(value:Address,output:*mut c_void,host:Host);
    #[link_name = "codec.value_format::write_source_type_name"]
    fn write_source_type_name(value: Address, output: *mut c_void, host: Host);
    #[link_name = "codec.value_format::write_expression_name"]
    fn write_expression_name(value: Address, output: *mut c_void, host: Host);
    #[link_name = "codec.value_format::write_contract_names"]
    fn write_contract_names(values: Span, output: *mut c_void, host: Host);
}
pub(super) fn encode(value:&ir::Ty,output:&mut Writer) {
    unsafe { write_type(address(value),ptr::from_mut(output).cast(),HOST); }
}

pub(crate) fn source_type_name(value: &ast::TypeName, output: &mut String) {
    let host = Host { append: append_name, ..HOST };
    unsafe { write_source_type_name(address(value), ptr::from_mut(output).cast(), host); }
}

pub(crate) fn expression_name(value: &ast::Expr, output: &mut String) {
    let host = Host { append: append_name, ..HOST };
    unsafe { write_expression_name(address(value), ptr::from_mut(output).cast(), host); }
}

pub(crate) fn contract_names(values: &[ast::ContractPredicate], output: &mut String) {
    let host = Host { append: append_name, ..HOST };
    unsafe { write_contract_names(Span::of(values), ptr::from_mut(output).cast(), host); }
}

unsafe extern "C" fn append_name(output: *mut c_void, data: *const u8, count: usize) {
    // B only appends borrowed Rust strings or its own ASCII punctuation and digits.
    let bytes = unsafe { std::slice::from_raw_parts(data, count) };
    let text = unsafe { std::str::from_utf8_unchecked(bytes) };
    unsafe { &mut *output.cast::<String>() }.push_str(text);
}

unsafe extern "C" fn append(output:*mut c_void,data:*const u8,count:usize) {
    unsafe { &mut *output.cast::<Writer>() }.0.extend_from_slice(unsafe { std::slice::from_raw_parts(data,count) });
}
unsafe extern "C" fn ty(source:Address)->Type {
    let value=unsafe { &*source.cast::<ir::Ty>() };
    Type {
        name:Bytes::of(&value.name),arguments:Span::of(&value.arguments),values:Span::of(&value.values),
        size:value.size,alignment:value.align,components:Span::of(&value.components),fields:Span::of(&value.fields),
        view:optional(value.view.as_deref()),array:optional(value.array.as_deref()),
        enumeration:u32::from(value.enumeration),result:u32::from(value.result),variants:Span::of(&value.variants),
        dataset:optional(value.dataset.as_deref()),cleanup:value.cleanup.as_deref().map_or_else(Bytes::default,Bytes::of),
        cleanup_present:u32::from(value.cleanup.is_some()),function:optional(value.function.as_deref()),
    }
}
unsafe extern "C" fn component(source:Address)->Component {
    let &(scalar,word)=unsafe { &*source.cast::<(ir::Scalar,u32)>() };Component { scalar:scalar as u32,word }
}
unsafe extern "C" fn field(source:Address)->Field {
    let (name,word,ty)=unsafe { &*source.cast::<(String,u32,Box<ir::Ty>)>() };
    Field { name:Bytes::of(name),word:*word,ty:address(ty.as_ref()) }
}
unsafe extern "C" fn variant(source:Address)->Variant {
    let value=unsafe { &*source.cast::<ir::Variant>() };
    Variant { name:Bytes::of(&value.name),discriminant:value.discriminant,ty:address(value.ty.as_ref()) }
}
unsafe extern "C" fn function(source:Address)->Function {
    let value=unsafe { &*source.cast::<ir::CFunctionType>() };
    Function { result:address(&value.result),parameters:Span::of(&value.parameters),contracts:Span::of(&value.contracts) }
}
unsafe extern "C" fn parameter(source:Address)->Parameter {
    let (access,ty)=unsafe { &*source.cast::<(ast::Access,ast::TypeName)>() };
    Parameter { access:*access as u32,ty:address(ty) }
}
unsafe extern "C" fn type_name(source:Address)->TypeName {
    let value=unsafe { &*source.cast::<ast::TypeName>() };
    TypeName {
        name:Bytes::of(&value.name),canonical:u32::from(value.canonical),arguments:Span::of(&value.arguments),
        dataset:u32::from(value.dataset),view:u32::from(value.view),tile:u32::from(value.tile),count:optional(value.count.as_ref()),
        contracts:Span::of(value.contracts.as_deref().map_or(&[],Vec::as_slice)),contracts_present:u32::from(value.contracts.is_some()),
    }
}
unsafe extern "C" fn argument(source:Address)->Argument {
    match unsafe { &*source.cast::<ast::GenericArgument>() } {
        ast::GenericArgument::Type(value)=>Argument { kind:0,value:address(value) },
        ast::GenericArgument::Value(value)=>Argument { kind:1,value:address(value) },
    }
}
unsafe extern "C" fn shape(source:Address)->Shape {
    match unsafe { &*source.cast::<ast::Shape>() } {
        ast::Shape::Literal(value)=>Shape { kind:0,literal:*value,..Shape::default() },
        ast::Shape::Constant(name)=>Shape { kind:1,name:Bytes::of(name),..Shape::default() },
        ast::Shape::Expression(value)=>Shape { kind:2,expression:address(value.as_ref()),..Shape::default() },
    }
}
unsafe extern "C" fn expression(source:Address)->Expression {
    use ast::ExprKind as K;
    let value=unsafe { &*source.cast::<ast::Expr>() };
    let mut output=Expression { start:value.span.start,end:value.span.end,..Expression::default() };
    match &value.kind {
        K::Number(name)=>{output.kind=0;output.name=Bytes::of(name);}
        K::StaticU32(value)=>{output.kind=1;output.literal=*value;}
        K::Bool(value)=>{output.kind=2;output.literal=u32::from(*value);}
        K::Name(name)=>{output.kind=3;output.name=Bytes::of(name);}
        K::Member(a,name)=>{output.kind=4;output.a=address(a.as_ref());output.name=Bytes::of(name);}
        K::Index(a,b)=>{output.kind=5;output.a=address(a.as_ref());output.b=address(b.as_ref());}
        K::Call(name,args)=>{output.kind=6;output.name=Bytes::of(name);output.arguments=Span::of(args);}
        K::Invoke(a,args)=>{output.kind=7;output.a=address(a.as_ref());output.arguments=Span::of(args);}
        K::Construct(ty,args)=>{output.kind=8;output.ty=address(ty);output.arguments=Span::of(args);}
        K::Specialize(name,types,args)=>{output.kind=9;output.name=Bytes::of(name);output.types=Span::of(types);output.arguments=Span::of(args);}
        K::FunctionRef(name,types)=>{output.kind=10;output.name=Bytes::of(name);output.types=Span::of(types);}
        K::Cooperate(name,types,args)=>{output.kind=11;output.name=Bytes::of(name);output.types=Span::of(types.as_deref().unwrap_or_default());output.types_present=u32::from(types.is_some());output.arguments=Span::of(args);}
        K::Target(target,a)=>{output.kind=12;output.target=*target as u32;output.a=address(a.as_ref());}
        K::Unary(name,a)=>{output.kind=13;output.name=Bytes::of(name);output.a=address(a.as_ref());}
        K::Binary(name,a,b)=>{output.kind=14;output.name=Bytes::of(name);output.a=address(a.as_ref());output.b=address(b.as_ref());}
        K::Select(a,b,c)=>{output.kind=15;output.a=address(a.as_ref());output.b=address(b.as_ref());output.c=address(c.as_ref());}
    }
    output
}
unsafe extern "C" fn predicate(source:Address)->Predicate {
    use ast::ContractPredicate as K;
    match unsafe { &*source.cast::<K>() } {
        K::Prefix { data,count,field }=>Predicate { kind:0,first:Bytes::of(data),second:Bytes::of(count),third:Bytes::of(field),..Predicate::default() },
        K::Nonzero(name)=>Predicate { kind:1,first:Bytes::of(name),..Predicate::default() },
        K::Nonempty(name)=>Predicate { kind:2,first:Bytes::of(name),..Predicate::default() },
        K::Contiguous(name)=>Predicate { kind:3,first:Bytes::of(name),..Predicate::default() },
        K::Memory(access)=>Predicate { kind:4,access:access.map_or(0,|value|value as u32),access_present:u32::from(access.is_some()),..Predicate::default() },
        K::Index { value,resource }=>Predicate { kind:5,first:Bytes::of(value),second:Bytes::of(resource),..Predicate::default() },
        K::Range { value,intervals }=>Predicate { kind:6,first:Bytes::of(value),intervals:Span::of(intervals),..Predicate::default() },
        K::SameLength { left,right }=>Predicate { kind:7,first:Bytes::of(left),second:Bytes::of(right),..Predicate::default() },
        K::Initialized { value,condition }=>Predicate { kind:8,first:Bytes::of(value),condition:address(condition),..Predicate::default() },
        K::Cleanup { value,condition }=>Predicate { kind:9,first:Bytes::of(value),condition:address(condition),..Predicate::default() },
    }
}
unsafe extern "C" fn interval(source:Address)->Interval {
    let (lower,upper)=unsafe { &*source.cast::<(ast::OutputCondition,ast::OutputCondition)>() };
    Interval { lower:address(lower),upper:address(upper) }
}
unsafe extern "C" fn condition(source:Address)->Condition {
    use ast::OutputCondition as K;
    match unsafe { &*source.cast::<K>() } {
        K::Name(name)=>Condition { kind:0,name:Bytes::of(name),..Condition::default() },
        K::Number(name)=>Condition { kind:1,name:Bytes::of(name),..Condition::default() },
        K::Bool(value)=>Condition { kind:2,literal:u32::from(*value),..Condition::default() },
        K::Member(a,name)=>Condition { kind:3,a:address(a.as_ref()),name:Bytes::of(name),..Condition::default() },
        K::Unary(name,a)=>Condition { kind:4,name:Bytes::of(name),a:address(a.as_ref()),..Condition::default() },
        K::Binary(name,a,b)=>Condition { kind:5,name:Bytes::of(name),a:address(a.as_ref()),b:address(b.as_ref()),..Condition::default() },
        K::StaticU32(value)=>Condition { kind:6,literal:*value,..Condition::default() },
        K::Constant(value)=>Condition { kind:7,constant:address(value),..Condition::default() },
    }
}
unsafe extern "C" fn constant(source:Address)->Constant {
    let value=unsafe { &*source.cast::<ast::OutputConstant>() };
    Constant { ty:address(&value.ty),bits:Span::of(&value.bits) }
}
const HOST:Host=Host { append,ty,component,field,variant,function,parameter,type_name,argument,shape,expression,predicate,interval,condition,constant };

// The reader owns only synchronous construction frames and the original Result.
pub(crate) mod reading;

// Physical construction for the original public contract wire.
impl crate::codec::contracts::Native for ir::Ty {
    fn empty() -> Self {
        reading::empty_type()
    }
    unsafe fn write(&self, output: *mut c_void) {
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_write_ast_type"]
            fn write(source: Address, output: *mut c_void, host: Host);
        }
        unsafe { write(address(self), output, HOST) }
    }
    unsafe fn read(context: *mut c_void, output: &mut Self) -> crate::codec::ReadStatus {
        unsafe { reading::native_type(context, output) }
    }
}

impl crate::codec::contracts::Native for ast::ContractPredicate {
    fn empty() -> Self {
        ast::ContractPredicate::Nonzero(String::new())
    }
    unsafe fn write(&self, output: *mut c_void) {
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_write_ast_predicate"]
            fn write(source: Address, output: *mut c_void, host: Host);
        }
        unsafe { write(address(self), output, HOST) }
    }
    unsafe fn read(context: *mut c_void, output: &mut Self) -> crate::codec::ReadStatus {
        unsafe { reading::native_predicate(context, output) }
    }
}

impl crate::codec::contracts::Native for ast::OutputCondition {
    fn empty() -> Self {
        ast::OutputCondition::Bool(false)
    }
    unsafe fn write(&self, output: *mut c_void) {
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_write_ast_condition"]
            fn write(source: Address, output: *mut c_void, host: Host);
        }
        unsafe { write(address(self), output, HOST) }
    }
    unsafe fn read(context: *mut c_void, output: &mut Self) -> crate::codec::ReadStatus {
        unsafe { reading::native_condition(context, output) }
    }
}
