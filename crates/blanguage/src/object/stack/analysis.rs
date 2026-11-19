//! Synchronous access to the original native code and the query's native containers.
use super::{Call, Code, Target};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    rc::Rc,
};

#[repr(C)]
#[derive(Clone,Copy)]
struct Name {data:*const u8,length:u64}
impl Name {fn new(text:&str)->Self {Self{data:text.as_ptr(),length:text.len() as u64}}}
unsafe fn text<'a>(name:Name)->&'a str {unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(name.data,name.length as usize))}}
#[repr(C)]
struct FrameInfo {calls:u64,local:u32,positioned:u32}
#[repr(u32)]
#[derive(Clone,Copy)]
enum CallKind {Reference,Tail,Indirect,Unreported}
#[repr(u32)]
#[derive(Clone,Copy)]
enum TargetKind {Section,Symbol,Import}
#[repr(C)]
#[derive(Clone,Copy)]
struct CallInfo {name:Name,dll:Name,section:u64,start:u32,kind:CallKind,target:TargetKind}
#[repr(C)]
struct Position {section:u64,start:u32,present:u32}
#[repr(C)]
struct Index {value:u64,present:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct Parent {parent:u64,tail:u32}
#[repr(u32)]
enum Notice {Unreported,Indirect,UnresolvedModule,External,MissingBody}
#[repr(u32)]
pub(super) enum Budget {NotSupplied,LocalExceeds,UpperFits,UpperExceeds,Unknown}
#[repr(C)]
pub(super) struct Amount {low:u64,high:u64}
#[repr(C)]
pub(super) struct Metric {amount:Amount,pub closed:u32,pub upper:u32,pub capacity:Budget}
impl Metric {pub fn bytes(&self)->u128 {u128::from(self.amount.low)|(u128::from(self.amount.high)<<64)}}
#[repr(C)]
struct Scratch {pending:u64,ready:u64}
#[repr(C)]
struct Host {
    frame:unsafe extern "C" fn(*mut c_void,u64)->FrameInfo,
    call:unsafe extern "C" fn(*mut c_void,u64,u64)->CallInfo,
    register_frame:unsafe extern "C" fn(*mut c_void,u64),
    register_symbol:unsafe extern "C" fn(*mut c_void,u64),
    symbol:unsafe extern "C" fn(*mut c_void,Name)->Position,
    position:unsafe extern "C" fn(*mut c_void,Position)->Index,
    edge:unsafe extern "C" fn(*mut c_void,u64,u64,u32),
    parent_count:unsafe extern "C" fn(*mut c_void,u64)->u64,
    parent:unsafe extern "C" fn(*mut c_void,u64,u64)->Parent,
    notice:unsafe extern "C" fn(*mut c_void,u64,Notice,CallInfo),
    union_unknown:unsafe extern "C" fn(*mut c_void,u64,u64),
    unknown_empty:unsafe extern "C" fn(*mut c_void,u64)->u32,
}
unsafe extern "C" {
    #[link_name="bootstrap.stack_report::analyze"]
    fn analyze_graph(context:*mut c_void,frames:u64,symbols:u64,has_budget:u32,budget:u64,
        metrics:*mut Metric,scratch:*mut Scratch,host:Host);
}
const EXTERNAL_PREFIX: &str = "external ";

#[derive(Clone)]
pub(super) enum Label<'a> {
    Borrowed(&'a str),
    Generated(Rc<String>),
    ImportName(Rc<String>),
}

impl Label<'_> {
    pub(super) fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(text) => text,
            Self::Generated(text) => text.as_str(),
            Self::ImportName(text) => &text[EXTERNAL_PREFIX.len()..],
        }
    }
}

// Ordering and equality use the written text, including for reasons read from an object.
impl PartialEq for Label<'_> {
    fn eq(&self, other: &Self) -> bool { self.as_str() == other.as_str() }
}
impl Eq for Label<'_> {}
impl PartialOrd for Label<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for Label<'_> {
    fn cmp(&self, other: &Self) -> Ordering { self.as_str().cmp(other.as_str()) }
}

struct Context<'a> {
    code: Code<'a>,
    positions:BTreeMap<(usize,u32),usize>,
    symbols:BTreeMap<&'a str,(usize,u32)>,
    parents:Vec<Vec<Parent>>,
    unknown: Vec<BTreeSet<Label<'a>>>,
    callees: Vec<BTreeSet<Label<'a>>>,
}
unsafe extern "C" fn read_frame(slot:*mut c_void,index:u64)->FrameInfo {
    let context=unsafe{&*slot.cast::<Context<'_>>()};let frame=&context.code.frames[index as usize];
    FrameInfo{calls:frame.calls.len() as u64,local:frame.local,positioned:u32::from(frame.coff.is_some())}
}
unsafe extern "C" fn read_call(slot:*mut c_void,index:u64,ordinal:u64)->CallInfo {
    let context=unsafe{&*slot.cast::<Context<'_>>()};let frame=&context.code.frames[index as usize];
    let mut value=CallInfo{name:Name::new(""),dll:Name::new(""),section:0,start:0,kind:CallKind::Indirect,target:TargetKind::Section};
    match &frame.calls[ordinal as usize] {
        Call::Unreported(reason)=>{value.kind=CallKind::Unreported;value.name=Name::new(reason);}
        Call::Indirect=>{},
        Call::Reference { relocation, tail } => {
            value.kind=if *tail{CallKind::Tail}else{CallKind::Reference};
            let relocations = (context.code.relocations)(frame.coff.unwrap().0);
            let target = &relocations[*relocation].target;
            match target {
                Target::Section(section,start)=>{value.section=*section as u64;value.start=*start;}
                Target::Symbol(name)=>{value.target=TargetKind::Symbol;value.name=Name::new(name);}
                Target::Import(import)=>{value.target=TargetKind::Import;value.name=Name::new(&import.name);value.dll=Name::new(&import.dll);}
            }
        }
    }
    value
}
unsafe extern "C" fn register_frame(slot:*mut c_void,index:u64) {
    let context=unsafe{&mut *slot.cast::<Context<'_>>()};context.positions.insert(context.code.frames[index as usize].coff.unwrap(),index as usize);
}
unsafe extern "C" fn register_symbol(slot:*mut c_void,index:u64) {
    let context=unsafe{&mut *slot.cast::<Context<'_>>()};let symbol=context.code.definitions.at(index as usize);
    context.symbols.insert(symbol.name.as_str(),(symbol.section,symbol.value));
}
unsafe extern "C" fn lookup_symbol(slot:*mut c_void,name:Name)->Position {
    let context=unsafe{&*slot.cast::<Context<'_>>()};context.symbols.get(unsafe{text(name)}).map_or(Position{section:0,start:0,present:0},|&(section,start)|Position{section:section as u64,start,present:1})
}
unsafe extern "C" fn lookup_position(slot:*mut c_void,position:Position)->Index {
    let context=unsafe{&*slot.cast::<Context<'_>>()};context.positions.get(&(position.section as usize,position.start)).map_or(Index{value:0,present:0},|&value|Index{value:value as u64,present:1})
}
unsafe extern "C" fn edge(slot:*mut c_void,parent:u64,child:u64,tail:u32) {
    let context=unsafe{&mut *slot.cast::<Context<'_>>()};context.parents[child as usize].push(Parent{parent,tail});
    let name = context.code.frames[child as usize].name.as_str();
    context.callees[parent as usize].insert(Label::Borrowed(name));
}
unsafe extern "C" fn parent_count(slot:*mut c_void,index:u64)->u64 {unsafe{&*slot.cast::<Context<'_>>()}.parents[index as usize].len() as u64}
unsafe extern "C" fn read_parent(slot:*mut c_void,index:u64,ordinal:u64)->Parent {unsafe{&*slot.cast::<Context<'_>>()}.parents[index as usize][ordinal as usize]}
unsafe extern "C" fn notice(slot:*mut c_void,index:u64,notice:Notice,call:CallInfo) {
    let context=unsafe{&mut *slot.cast::<Context<'_>>()};let index=index as usize;
    // read_call borrows the names from Code, which lives through report rendering.
    let reason = match notice {
        Notice::Unreported => Label::Borrowed(unsafe { text(call.name) }),
        Notice::Indirect => Label::Borrowed("indirect target"),
        Notice::UnresolvedModule => {
            let name = unsafe { text(call.name) };
            context.callees[index].insert(Label::Borrowed(name));
            Label::Generated(Rc::new(format!("unresolved module {name}")))
        }
        Notice::External => {
            let dll = unsafe { text(call.dll) };
            let name = unsafe { text(call.name) };
            let reason = Rc::new(format!("{EXTERNAL_PREFIX}{dll}!{name}"));
            context.callees[index].insert(Label::ImportName(Rc::clone(&reason)));
            Label::Generated(reason)
        }
        Notice::MissingBody => {
            Label::Generated(Rc::new(format!("unreported native body section {}+{}", call.section, call.start)))
        }
    };
    context.unknown[index].insert(reason);
}
unsafe extern "C" fn union_unknown(slot:*mut c_void,parent:u64,child:u64) {
    let context=unsafe{&mut *slot.cast::<Context<'_>>()};let parent=parent as usize;let child=child as usize;
    // Propagation only visits a closed child and its distinct unfinished parent.
    let(target,source)=if parent<child {let(a,b)=context.unknown.split_at_mut(child);(&mut a[parent],&b[0])}
        else {let(a,b)=context.unknown.split_at_mut(parent);(&mut b[0],&a[child])};
    target.extend(source.iter().cloned());
}
unsafe extern "C" fn unknown_empty(slot:*mut c_void,index:u64)->u32 {u32::from(unsafe{&*slot.cast::<Context<'_>>()}.unknown[index as usize].is_empty())}
pub(super) struct Analysis<'a> {
    pub metrics: Vec<Metric>,
    pub unknown: Vec<BTreeSet<Label<'a>>>,
    pub callees: Vec<BTreeSet<Label<'a>>>,
}
pub(super) fn analyze<'a>(code: Code<'a>, capacity: Option<u64>) -> Analysis<'a> {
    let count = code.frames.len();
    let symbol_count = code.definitions.len();
    let mut context=Context{code,positions:BTreeMap::new(),symbols:BTreeMap::new(),parents:vec![Vec::new();count],unknown:vec![BTreeSet::new();count],callees:vec![BTreeSet::new();count]};
    let mut metrics=Vec::with_capacity(count);let mut scratch=Vec::<Scratch>::with_capacity(count);
    let host=Host{frame:read_frame,call:read_call,register_frame,register_symbol,symbol:lookup_symbol,position:lookup_position,edge,parent_count,parent:read_parent,notice,union_unknown,unknown_empty};
    unsafe{analyze_graph(std::ptr::from_mut(&mut context).cast(),count as u64,symbol_count as u64,u32::from(capacity.is_some()),capacity.unwrap_or(0),metrics.as_mut_ptr(),scratch.as_mut_ptr(),host);metrics.set_len(count);}
    Analysis{metrics,unknown:context.unknown,callees:context.callees}
}
