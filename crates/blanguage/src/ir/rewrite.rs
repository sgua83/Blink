//! Mutable physical fields of the original compiler IR. B selects field roles,
//! order and structured recursion; this module only projects concrete storage.
use super::*;
use std::{ffi::c_void, mem::size_of, ptr};

#[derive(Clone, Copy)]
#[repr(u32)]
enum Kind {
    Instruction, Call, Target, Argument, DatasetBorrow, Reference, Storage,
    Indexed, Stride, Texture, AtomicOp, Alias, Effect, Division, Divisor,
    Ray = 16,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Node { data: *mut c_void, kind: Kind }
impl Node {
    fn of<T>(value: &mut T, kind: Kind) -> Self { Self { data: ptr::from_mut(value).cast(), kind } }
    const EMPTY: Self = Self { data: ptr::null_mut(), kind: Kind::Instruction };
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Span { data: *mut c_void, count: usize, stride: usize }
impl Span {
    const EMPTY: Self = Self { data: ptr::null_mut(), count: 0, stride: 0 };
    pub(crate) fn slice<T>(values: &mut [T]) -> Self {
        let count=values.len();
        Self { data: values.as_mut_ptr().cast(), count, stride: size_of::<T>() }
    }
    fn pairs(values: &mut [(u32, Reg)]) -> Self {
        let count=values.len();
        Self { data: if count==0 { ptr::null_mut() } else {
                // Keep the full slice's provenance while selecting the pair field.
                unsafe { ptr::addr_of_mut!((*values.as_mut_ptr()).1).cast() }
            },
            count, stride: size_of::<(u32, Reg)>() }
    }
    fn publication_pairs(values: &mut [(u32, Reg)]) -> (Self, Self) {
        let count = values.len();
        if count == 0 { return (Self::EMPTY, Self::EMPTY); }
        let base = values.as_mut_ptr();
        let stride = size_of::<(u32, Reg)>();
        // Both fields keep the provenance of the same exclusive slice.
        let (word, register) = unsafe { (ptr::addr_of_mut!((*base).0), ptr::addr_of_mut!((*base).1)) };
        (Self { data: register.cast(), count, stride }, Self { data: word.cast(), count, stride })
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct View {
    tag: u32, a: *mut usize, b: *mut usize, c: *mut usize, d: *mut usize,
    first: Span, second: Span, third: Span, left: Node, right: Node, len: usize,
}
impl View {
    fn tag(tag: u32) -> Self {
        Self { tag, a: ptr::null_mut(), b: ptr::null_mut(), c: ptr::null_mut(), d: ptr::null_mut(),
            first: Span::EMPTY, second: Span::EMPTY, third: Span::EMPTY,
            left: Node::EMPTY, right: Node::EMPTY, len: 0 }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Host { inspect: unsafe extern "C" fn(Node) -> View }
pub(crate) const HOST: Host = Host { inspect };
#[repr(C)]
struct Mapping {
    context: *const c_void,
    reg: *const c_void,
    slot: *const c_void,
}
#[repr(C)]
struct InputMapping { context: *const c_void, reg: *const c_void }
// These are the actual immutable mapping materials held by the producers.
// They cannot contain a user callback or an interior-mutable mapping policy.
pub(crate) enum Remap<'a> {
    Offset { registers: usize, locals: usize },
    Compact { registers: &'a [usize], locals: &'a [usize] },
    Bindings(&'a BTreeMap<Reg, Reg>),
}
unsafe extern "C-unwind" {
    #[link_name = "bootstrap.ir_dataflow::rewrite_code"]
    fn b_code(code: Span, host: Host, mapping: Mapping);
    #[link_name = "bootstrap.ir_dataflow::rewrite_operands"]
    fn b_operands(source: Node, host: Host, mapping: InputMapping);
}
unsafe extern "C-unwind" fn register(context: *const c_void, value: usize) -> usize {
    match unsafe { &*context.cast::<Remap<'_>>() } {
        Remap::Offset { registers, .. } => value + registers,
        Remap::Compact { registers, .. } => registers[value] - 1,
        Remap::Bindings(values) => values.get(&value).copied().unwrap_or(value),
    }
}
unsafe extern "C-unwind" fn local(context: *const c_void, value: usize) -> usize {
    match unsafe { &*context.cast::<Remap<'_>>() } {
        Remap::Offset { locals, .. } => value + locals,
        Remap::Compact { locals, .. } => locals[value] - 1,
        Remap::Bindings(_) => value,
    }
}
pub(crate) fn map(code: &mut [Inst], mapping: Remap<'_>) {
    unsafe { b_code(Span::slice(code), HOST, Mapping {
        context: ptr::from_ref(&mapping).cast(), reg: register as *const c_void, slot: local as *const c_void,
    }); }
}
// The value-flow owner supplies its existing C-ABI binding of the B alias-root
// algorithm to the actual immutable Values/get pair. No root algorithm or
// state is copied here. The reader must only read this borrowed source and
// neither retain it nor retain any instruction projection after this call.
pub(crate) unsafe fn operands<T>(value: &mut Inst, source: &T, root: extern "C" fn(&T, Reg) -> Reg) {
    unsafe { b_operands(Node::of(value, Kind::Instruction), HOST, InputMapping {
        context: ptr::from_ref(source).cast(), reg: root as *const c_void,
    }); }
}
fn optional(value: &mut Option<usize>) -> *mut usize {
    value.as_mut().map_or(ptr::null_mut(), ptr::from_mut)
}

// Nodes originate in exclusive original owners, never in the read-only query
// views. Each projection splits disjoint concrete fields once. The B caller
// finishes using those fields before projecting the same node again; it never
// resizes/replaces containers, retains a view, or reborrows an enclosing owner
// while a projected child is in use. Only genuine structured blocks recurse.
unsafe extern "C" fn inspect(node: Node) -> View {
    match node.kind {
        Kind::Instruction => instruction(unsafe { &mut *node.data.cast::<Inst>() }),
        Kind::Call => {
            let call = unsafe { &mut *node.data.cast::<Call>() };
            let mut v = View::tag(0);
            v.first = Span::slice(&mut call.arguments); v.second = Span::slice(&mut call.memory);
            v.third = Span::slice(&mut call.values); v.a = optional(&mut call.error);
            v.left = Node::of(&mut call.target, Kind::Target); v
        }
        Kind::Target => match unsafe { &mut *node.data.cast::<CallTarget>() } {
            CallTarget::External(_) => View::tag(0), CallTarget::Defined(_) => View::tag(1),
            CallTarget::Indirect(reg) => { let mut v=View::tag(2); v.a=reg; v }
        },
        Kind::Argument => match unsafe { &mut *node.data.cast::<CallArgument>() } {
            CallArgument::Value { values, dataset_borrow, .. } => {
                let mut v=View::tag(0); v.first=Span::slice(values);
                v.left=dataset_borrow.as_deref_mut().map_or(Node::EMPTY, |b| Node::of(b, Kind::DatasetBorrow)); v
            }
            CallArgument::Reference { address, .. } => { let mut v=View::tag(1); v.left=Node::of(address, Kind::Reference); v }
        },
        Kind::DatasetBorrow => {
            let b=unsafe { &mut *node.data.cast::<DatasetBorrow>() }; let mut v=View::tag(0);
            v.first=Span::slice(&mut b.frame); v.a=&mut b.handle; v
        }
        Kind::Reference => {
            let r=unsafe { &mut *node.data.cast::<CallReference>() }; let mut v=View::tag(0);
            v.a=optional(&mut r.offset); v.left=Node::of(&mut r.storage, Kind::Storage); v
        }
        Kind::Storage => match unsafe { &mut *node.data.cast::<CallStorage>() } {
            CallStorage::Local { base, len } => { let mut v=View::tag(0); v.a=base; v.len=*len; v }
            CallStorage::Indexed(source) => { let mut v=View::tag(1); v.left=Node::of(source, Kind::Indexed); v }
        },
        Kind::Indexed => match unsafe { &mut *node.data.cast::<IndexedSource>() } {
            IndexedSource::Data { index, .. } => { let mut v=View::tag(0); v.a=index; v }
            IndexedSource::View { address, index, stride } => {
                let mut v=View::tag(1); v.first=Span::slice(address); v.a=index; v.left=Node::of(stride, Kind::Stride); v
            }
            IndexedSource::Pointer { address, index, aliases, .. } => {
                let mut v=View::tag(2); v.first=Span::slice(address); v.a=index; v.second=Span::slice(aliases); v
            }
            IndexedSource::Tile { index, .. } => { let mut v=View::tag(3); v.a=index; v }
        },
        Kind::Stride => match unsafe { &mut *node.data.cast::<ElementStride>() } {
            ElementStride::Constant(_) => View::tag(0),
            ElementStride::Value(words) => { let mut v=View::tag(1); v.first=Span::slice(words); v }
        },
        Kind::Texture => {
            let t=unsafe { &mut *node.data.cast::<TextureSource>() }; let mut v=View::tag(0);
            v.a=optional(&mut t.element); v.b=optional(&mut t.parameter); v
        }
        Kind::AtomicOp => {
            let mut v=View::tag(0);
            match unsafe { &mut *node.data.cast::<AtomicOp>() } {
                AtomicOp::Load => {}
                AtomicOp::Store(a)|AtomicOp::Add(a)|AtomicOp::Sub(a)|AtomicOp::Exchange(a)
                |AtomicOp::Min(a)|AtomicOp::Max(a)|AtomicOp::And(a)|AtomicOp::Or(a)|AtomicOp::Xor(a) => { v.tag=1; v.a=a; }
                AtomicOp::CompareExchange { expected, desired, .. } => { v.tag=2; v.a=expected; v.b=desired; }
            } v
        }
        Kind::Alias => match unsafe { &mut *node.data.cast::<MemoryAlias>() } {
            MemoryAlias::Local { base, len } => { let mut v=View::tag(0); v.a=base; v.len=*len; v }
            MemoryAlias::Data(_) => View::tag(1),
        },
        Kind::Effect => {
            let e=unsafe { &mut *node.data.cast::<MemoryAccess>() }; let mut v=View::tag(0);
            v.left=Node::of(&mut e.alias, Kind::Alias); v
        }
        Kind::Division => {
            let mut v=View::tag(0);
            match unsafe { &mut *node.data.cast::<IntegerDivisionResult>() } {
                IntegerDivisionResult::Quotient(a)|IntegerDivisionResult::Remainder(a) => v.a=a,
                IntegerDivisionResult::Both { quotient, remainder } => { v.a=quotient; v.b=remainder; }
            } v
        }
        Kind::Divisor => match unsafe { &mut *node.data.cast::<IntegerDivisor>() } {
            IntegerDivisor::Constant(_) => View::tag(0),
            IntegerDivisor::Value(a) => { let mut v=View::tag(1); v.a=a; v }
        },
        Kind::Ray => {
            let mut v=View::tag(0);
            v.first=Span::slice(match unsafe { &mut *node.data.cast::<RayResult>() } {
                RayResult::Any(a) => std::slice::from_mut(a), RayResult::Closest(words) => words,
            }); v
        }
    }
}

fn instruction(value: &mut Inst) -> View {
    let mut v=View::tag(0);
    match &mut value.kind {
        InstKind::TextureBindingAddress { dst, texture } => { v.tag=0; v.a=dst; v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureSample { result, texture, coordinate, lod } => {
            v.tag=1; v.first=Span::slice(result); v.second=Span::slice(coordinate); v.a=lod; v.left=Node::of(texture,Kind::Texture);
        }
        InstKind::TextureSize { result, texture, level } => { v.tag=2; v.first=Span::slice(result); v.a=level; v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureLevels { dst, texture } => { v.tag=3; v.a=dst; v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureAddress { dst, texture } => { v.tag=4; v.a=dst; v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureFormat { dst, texture } => { v.tag=5; v.a=dst; v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureRead { dst, texture, coordinate, level, .. } => { v.tag=6; v.a=dst; v.b=level; v.first=Span::slice(coordinate); v.left=Node::of(texture,Kind::Texture); }
        InstKind::TextureFetch { result, texture, coordinate, level } => { v.tag=7; v.first=Span::slice(result); v.second=Span::slice(coordinate); v.a=level; v.left=Node::of(texture,Kind::Texture); }
        InstKind::LoadIndexed { dst, source, offset, .. } => { v.tag=8; v.a=dst; v.b=offset; v.left=Node::of(source,Kind::Indexed); }
        InstKind::StoreIndexed { source, offset, value, .. } => { v.tag=9; v.a=offset; v.b=value; v.left=Node::of(source,Kind::Indexed); }
        InstKind::Atomic(a) => { v.tag=10; v.a=optional(&mut a.result); v.b=optional(&mut a.offset); v.left=Node::of(&mut a.target,Kind::Indexed); v.right=Node::of(&mut a.op,Kind::AtomicOp); }
        InstKind::Append(a) => { v.tag=11; v.a=&mut a.accepted; v.first=Span::pairs(&mut a.values); }
        InstKind::TileBarrier => v.tag=12,
        InstKind::TilePublish(publication) => {
            v.tag = 13;
            v.a = ptr::from_mut(&mut publication.tile);
            (v.first, v.second) = Span::publication_pairs(&mut publication.values);
        }
        InstKind::TileCollective(c) => { v.tag=14; v.first=Span::pairs(&mut c.results); }
        InstKind::TileLength(dst) => { v.tag=15; v.a=dst; }
        InstKind::TileLane(dst) => { v.tag=16; v.a=dst; }
        InstKind::TileLoad { dst, index, .. } => { v.tag=17; v.a=dst; v.c=index; }
        InstKind::Subgroup { dst, value, argument, .. } => { v.tag=18; v.a=dst; v.first=Span::slice(value.as_mut_slice()); v.second=Span::slice(argument.as_mut_slice()); }
        InstKind::Print { dst, value } => { v.tag=19; v.a=dst; v.b=value; }
        InstKind::Call(call) => { v.tag=20; v.left=Node::of(call,Kind::Call); }
        InstKind::ViewLoad { dst, address, index, stride, .. } => { v.tag=21; v.a=dst; v.b=index; v.first=Span::slice(address); v.left=Node::of(stride,Kind::Stride); }
        InstKind::RayQuery { origin, direction, minimum, maximum, cull_back_faces, result, .. } => {
            v.tag=22; v.first=Span::slice(origin); v.second=Span::slice(direction); v.a=minimum; v.b=maximum; v.c=optional(cull_back_faces); v.left=Node::of(result,Kind::Ray);
        }
        InstKind::Constant(dst, ..) => { v.tag=23; v.a=dst; }
        InstKind::FunctionAddress(dst, target) => { v.tag=24; v.a=dst; v.left=Node::of(target,Kind::Target); }
        InstKind::PositionAddress { dst, address } => { v.tag=25; v.a=dst; v.left=Node::of(address,Kind::Reference); }
        InstKind::BufferStride(dst, _) => { v.tag=26; v.a=dst; }
        InstKind::Index(dst) => { v.tag=27; v.a=dst; }
        InstKind::Uniform(dst, _) => { v.tag=28; v.a=dst; }
        InstKind::LoadLocal(dst, slot) => { v.tag=29; v.a=dst; v.b=slot; }
        InstKind::StoreLocal(slot, value) => { v.tag=30; v.a=slot; v.b=value; }
        InstKind::LoadLocalIndexed { dst, base, index, .. } => { v.tag=31; v.a=dst; v.b=base; v.d=index; }
        InstKind::StoreLocalIndexed { base, index, value, .. } => { v.tag=32; v.a=base; v.c=index; v.d=value; }
        InstKind::Load { dst, index, .. } => { v.tag=33; v.a=dst; v.c=index; }
        InstKind::Store { index, value, .. } => { v.tag=34; v.b=index; v.d=value; }
        InstKind::Unary { dst, a, .. } => { v.tag=35; v.a=dst; v.b=a; }
        InstKind::Binary { dst, a, b, .. } => { v.tag=36; v.a=dst; v.b=a; v.c=b; }
        InstKind::IntegerDivide { result, src, divisor } => { v.tag=37; v.a=src; v.left=Node::of(divisor,Kind::Divisor); v.right=Node::of(result,Kind::Division); }
        InstKind::Cast { dst, src } => { v.tag=38; v.a=dst; v.b=src; }
        InstKind::Math { dst, args, .. } => { v.tag=39; v.a=dst; v.first=Span::slice(args); }
        InstKind::Select { dst, condition, a, b } => { v.tag=40; v.a=dst; v.b=condition; v.c=a; v.d=b; }
        InstKind::If { condition, yes, no } => { v.tag=41; v.a=condition; v.first=Span::slice(yes); v.second=Span::slice(no); }
        InstKind::Loop { condition, test, body } => { v.tag=42; v.a=test; v.first=Span::slice(condition); v.second=Span::slice(body); }
        InstKind::Break => v.tag=43,
        InstKind::Continue => v.tag=44,
        InstKind::Scope(body) => { v.tag=45; v.first=Span::slice(body); }
        InstKind::Exit => v.tag=46,
        InstKind::Raise(error) => { v.tag=47; v.a=error; }
        InstKind::Cleanup { .. } => v.tag=48,
    } v
}
