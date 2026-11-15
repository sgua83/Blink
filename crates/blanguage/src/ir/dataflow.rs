//! Borrow the original typed IR fields for the compiler's B dataflow queries.
//! The model selects roles and order; these views neither enumerate query
//! results nor own, copy, retain, or rewrite the IR containers.
use super::*;
use std::{ffi::c_void, marker::PhantomData, mem::size_of, ptr};

#[derive(Clone, Copy)]
#[repr(u32)]
enum Kind {
    Instruction, Call, Target, Argument, DatasetBorrow, Reference, Storage,
    Indexed, Stride, Texture, AtomicOp, Alias, Effect, Division, Divisor, Words3,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Node { data: *const c_void, kind: Kind }
impl Node {
    fn of<T>(value: &T, kind: Kind) -> Self { Self { data: ptr::from_ref(value).cast(), kind } }
    const EMPTY: Self = Self { data: ptr::null(), kind: Kind::Instruction };
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    const EMPTY: Self = Self { data: ptr::null(), count: 0, stride: 0 };
    fn slice<T>(values: &[T]) -> Self { Self { data: values.as_ptr().cast(), count: values.len(), stride: size_of::<T>() } }
    fn pairs(values: &[(u32, Reg)]) -> Self {
        Self {
            data: values.first().map_or(ptr::null(), |value| ptr::from_ref(&value.1).cast()),
            count: values.len(), stride: size_of::<(u32, Reg)>(),
        }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct View {
    tag: u32, a: usize, b: usize, c: usize, d: usize,
    first: Span, second: Span, left: Node, right: Node,
}
impl View {
    fn tag(tag: u32) -> Self {
        Self { tag, a: 0, b: 0, c: 0, d: 0, first: Span::EMPTY, second: Span::EMPTY,
            left: Node::EMPTY, right: Node::EMPTY }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Host { inspect: unsafe extern "C" fn(Node) -> View }
pub(crate) const HOST: Host = Host { inspect };
#[repr(C)]
struct Word { present: u32, value: usize }
#[repr(C)]
struct Results { first: Span, second: Span, a: usize, b: usize, inline_count: usize, index: usize }
#[repr(C)]
struct Locals {
    source: Node, mode: u32, writing: u32, phase: usize, entry: usize, alias: usize,
    position: usize, end: usize, active: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::results"]
    fn b_results(source: Node, host: Host) -> Results;
    #[link_name = "bootstrap.ir_dataflow::next_result"]
    fn b_next_result(state: *mut Results) -> Word;
    #[link_name = "bootstrap.ir_dataflow::operands"]
    fn b_operands(source: Node, host: Host, context: *mut c_void,
        visit: unsafe extern "C" fn(*mut c_void, usize) -> u32) -> u32;
    #[link_name = "bootstrap.ir_dataflow::locals"]
    fn b_locals(source: Node, writing: u32, host: Host) -> Locals;
    #[link_name = "bootstrap.ir_dataflow::next_local"]
    fn b_next_local(state: *mut Locals, host: Host) -> Word;
}

struct ResultIter<'a> { state: Results, owner: PhantomData<&'a Inst> }
impl Iterator for ResultIter<'_> {
    type Item = Reg;
    fn next(&mut self) -> Option<Reg> {
        let value = unsafe { b_next_result(&mut self.state) };
        (value.present != 0).then_some(value.value)
    }
}
struct LocalIter<'a> { state: Locals, owner: PhantomData<&'a ()> }
impl Iterator for LocalIter<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        let value = unsafe { b_next_local(&mut self.state, HOST) };
        (value.present != 0).then_some(value.value)
    }
}
pub(super) fn results(value: &Inst) -> impl Iterator<Item = Reg> + '_ {
    ResultIter { state: unsafe { b_results(Node::of(value, Kind::Instruction), HOST) }, owner: PhantomData }
}
fn locals<'a, T>(value: &'a T, kind: Kind, writing: bool) -> LocalIter<'a> {
    LocalIter { state: unsafe { b_locals(Node::of(value, kind), u32::from(writing), HOST) }, owner: PhantomData }
}
pub(super) fn instruction_locals(value: &Inst, writing: bool) -> impl Iterator<Item = usize> + '_ {
    locals(value, Kind::Instruction, writing)
}
pub(super) fn call_locals(value: &Call, writing: bool) -> impl Iterator<Item = usize> + '_ {
    locals(value, Kind::Call, writing)
}
unsafe extern "C" fn visit<F: FnMut(Reg) -> bool>(context: *mut c_void, value: usize) -> u32 {
    u32::from(unsafe { (&mut *context.cast::<F>())(value) })
}
fn operands<T, F: FnMut(Reg) -> bool>(value: &T, kind: Kind, f: &mut F) -> bool {
    unsafe { b_operands(Node::of(value, kind), HOST, ptr::from_mut(f).cast(), visit::<F>) != 0 }
}
pub(super) fn instruction_operands(value: &Inst, f: &mut impl FnMut(Reg) -> bool) -> bool {
    operands(value, Kind::Instruction, f)
}
pub(super) fn call_operands(value: &Call, f: &mut impl FnMut(Reg) -> bool) -> bool {
    operands(value, Kind::Call, f)
}
pub(super) fn reference_operands(value: &CallReference, f: &mut impl FnMut(Reg) -> bool) -> bool {
    operands(value, Kind::Reference, f)
}
pub(super) fn indexed_operands(value: &IndexedSource, f: &mut impl FnMut(Reg) -> bool) -> bool {
    operands(value, Kind::Indexed, f)
}
pub(super) fn stride_operands(value: &ElementStride, f: &mut impl FnMut(Reg) -> bool) -> bool {
    operands(value, Kind::Stride, f)
}

// Each Node points at the original concrete Rust field with the matching
// Kind. The caller borrows that owner throughout the synchronous query or
// lazy iterator. B receives only these views; it does not retain the owner.
unsafe extern "C" fn inspect(node: Node) -> View {
    match node.kind {
        Kind::Instruction => instruction(unsafe { &*node.data.cast::<Inst>() }),
        Kind::Call => {
            let call = unsafe { &*node.data.cast::<Call>() };
            let mut view = View::tag(0);
            view.first = Span::slice(&call.arguments); view.second = Span::slice(&call.memory);
            view.left = Node::of(&call.target, Kind::Target); view
        }
        Kind::Target => match unsafe { &*node.data.cast::<CallTarget>() } {
            CallTarget::External(_) => View::tag(0),
            CallTarget::Defined(_) => View::tag(1),
            CallTarget::Indirect(reg) => { let mut view = View::tag(2); view.a = *reg; view }
        },
        Kind::Argument => match unsafe { &*node.data.cast::<CallArgument>() } {
            CallArgument::Value { values, dataset_borrow, .. } => {
                let mut view = View::tag(0); view.first = Span::slice(values);
                view.left = dataset_borrow.as_deref().map_or(Node::EMPTY, |value| Node::of(value, Kind::DatasetBorrow)); view
            }
            CallArgument::Reference { address, access, .. } => {
                let mut view = View::tag(1); view.a = *access as usize;
                view.left = Node::of(address, Kind::Reference); view
            }
        },
        Kind::DatasetBorrow => {
            let value = unsafe { &*node.data.cast::<DatasetBorrow>() };
            let mut view = View::tag(0); view.first = Span::slice(&value.frame);
            view.second = Span::slice(std::slice::from_ref(&value.handle)); view
        }
        Kind::Reference => {
            let value = unsafe { &*node.data.cast::<CallReference>() };
            let mut view = View::tag(0); view.first = Span::slice(value.offset.as_slice());
            view.left = Node::of(&value.storage, Kind::Storage); view
        }
        Kind::Storage => match unsafe { &*node.data.cast::<CallStorage>() } {
            CallStorage::Local { base, len } => { let mut view = View::tag(0); view.a = *base; view.b = *len; view }
            CallStorage::Indexed(source) => { let mut view = View::tag(1); view.left = Node::of(source, Kind::Indexed); view }
        },
        Kind::Indexed => match unsafe { &*node.data.cast::<IndexedSource>() } {
            IndexedSource::Data { binding, index } => { let mut view = View::tag(0); view.a = *binding; view.b = *index; view }
            IndexedSource::View { address, index, stride } => {
                let mut view = View::tag(1); view.first = Span::slice(address); view.a = *index;
                view.left = Node::of(stride, Kind::Stride); view
            }
            IndexedSource::Pointer { address, index, aliases, .. } => {
                let mut view = View::tag(2); view.first = Span::slice(address); view.a = *index;
                view.second = Span::slice(aliases); view
            }
            IndexedSource::Tile { tile, index } => { let mut view = View::tag(3); view.a = *tile; view.b = *index; view }
        },
        Kind::Stride => match unsafe { &*node.data.cast::<ElementStride>() } {
            ElementStride::Constant(_) => View::tag(0),
            ElementStride::Value(values) => { let mut view = View::tag(1); view.first = Span::slice(values); view }
        },
        Kind::Texture => {
            let value = unsafe { &*node.data.cast::<TextureSource>() };
            let mut view = View::tag(0); view.first = Span::slice(value.parameter.as_slice());
            view.second = Span::slice(value.element.as_slice()); view
        }
        Kind::AtomicOp => {
            let value = unsafe { &*node.data.cast::<AtomicOp>() };
            let (tag, a, b) = match value {
                AtomicOp::Load => (0, 0, 0), AtomicOp::Store(a) => (1, *a, 0),
                AtomicOp::Add(a) => (2, *a, 0), AtomicOp::Sub(a) => (3, *a, 0),
                AtomicOp::Exchange(a) => (4, *a, 0),
                AtomicOp::CompareExchange { expected, desired, .. } => (5, *expected, *desired),
                AtomicOp::Min(a) => (6, *a, 0), AtomicOp::Max(a) => (7, *a, 0),
                AtomicOp::And(a) => (8, *a, 0), AtomicOp::Or(a) => (9, *a, 0), AtomicOp::Xor(a) => (10, *a, 0),
            };
            let mut view = View::tag(tag); view.a = a; view.b = b; view
        }
        Kind::Alias => match unsafe { &*node.data.cast::<MemoryAlias>() } {
            MemoryAlias::Local { base, len } => { let mut view = View::tag(0); view.a = *base; view.b = *len; view }
            MemoryAlias::Data(binding) => { let mut view = View::tag(1); view.a = *binding; view }
        },
        Kind::Effect => {
            let value = unsafe { &*node.data.cast::<MemoryAccess>() };
            let mut view = View::tag(0); view.a = value.access as usize;
            view.left = Node::of(&value.alias, Kind::Alias); view
        }
        Kind::Division => {
            let (tag, a, b) = match unsafe { &*node.data.cast::<IntegerDivisionResult>() } {
                IntegerDivisionResult::Quotient(a) => (0, *a, 0),
                IntegerDivisionResult::Remainder(a) => (1, *a, 0),
                IntegerDivisionResult::Both { quotient, remainder } => (2, *quotient, *remainder),
            };
            let mut view = View::tag(tag); view.a = a; view.b = b; view
        }
        Kind::Divisor => match unsafe { &*node.data.cast::<IntegerDivisor>() } {
            IntegerDivisor::Constant(_) => View::tag(0),
            IntegerDivisor::Value(a) => { let mut view = View::tag(1); view.a = *a; view }
        },
        Kind::Words3 => {
            let values = unsafe { &*node.data.cast::<[Reg; 3]>() };
            let mut view = View::tag(0); view.first = Span::slice(values); view
        }
    }
}

fn instruction(value: &Inst) -> View {
    // Numeric tags follow the declaration order in the paired B model. Each
    // arm below only projects the named original fields into that handoff.
    let mut view = View::tag(0);
    match &value.kind {
        InstKind::TextureBindingAddress { dst, texture } => {
            view.tag = 0; view.a = *dst; view.left = Node::of(texture, Kind::Texture);
        }
        InstKind::TextureSample { result, texture, coordinate, lod } => {
            view.tag = 1; view.first = Span::slice(result); view.second = Span::slice(coordinate);
            view.a = *lod; view.left = Node::of(texture, Kind::Texture);
        }
        InstKind::TextureSize { result, texture, level } => {
            view.tag = 2; view.first = Span::slice(result); view.a = *level; view.left = Node::of(texture, Kind::Texture);
        }
        InstKind::TextureLevels { dst, texture } => { view.tag = 3; view.a = *dst; view.left = Node::of(texture, Kind::Texture); }
        InstKind::TextureAddress { dst, texture } => { view.tag = 4; view.a = *dst; view.left = Node::of(texture, Kind::Texture); }
        InstKind::TextureFormat { dst, texture } => { view.tag = 5; view.a = *dst; view.left = Node::of(texture, Kind::Texture); }
        InstKind::TextureRead { dst, texture, coordinate, level, .. } => {
            view.tag = 6; view.a = *dst; view.b = *level; view.first = Span::slice(coordinate); view.left = Node::of(texture, Kind::Texture);
        }
        InstKind::TextureFetch { result, texture, coordinate, level } => {
            view.tag = 7; view.first = Span::slice(result); view.second = Span::slice(coordinate);
            view.a = *level; view.left = Node::of(texture, Kind::Texture);
        }
        InstKind::LoadIndexed { dst, source, offset, word } => {
            view.tag = 8; view.a = *dst; view.b = *offset; view.c = *word as usize; view.left = Node::of(source, Kind::Indexed);
        }
        InstKind::StoreIndexed { source, offset, value, word } => {
            view.tag = 9; view.a = *offset; view.b = *value; view.c = *word as usize; view.left = Node::of(source, Kind::Indexed);
        }
        InstKind::Atomic(value) => {
            view.tag = 10; view.first = Span::slice(value.result.as_slice()); view.second = Span::slice(value.offset.as_slice());
            view.left = Node::of(&value.target, Kind::Indexed); view.right = Node::of(&value.op, Kind::AtomicOp);
        }
        InstKind::Append(value) => { view.tag = 11; view.a = value.accepted; view.first = Span::pairs(&value.values); }
        InstKind::TileBarrier => view.tag = 12,
        InstKind::TilePublish(value) => { view.tag = 13; view.first = Span::pairs(&value.values); }
        InstKind::TileCollective(value) => { view.tag = 14; view.first = Span::pairs(&value.results); }
        InstKind::TileLength(dst) => { view.tag = 15; view.a = *dst; }
        InstKind::TileLane(dst) => { view.tag = 16; view.a = *dst; }
        InstKind::TileLoad { dst, tile, index, .. } => { view.tag = 17; view.a = *dst; view.b = *tile; view.c = *index; }
        InstKind::Subgroup { dst, value, argument, op } => {
            view.tag = 18; view.a = *dst; view.c = *op as usize; view.first = Span::slice(value.as_slice()); view.second = Span::slice(argument.as_slice());
        }
        InstKind::Print { dst, value } => { view.tag = 19; view.a = *dst; view.b = *value; }
        InstKind::Call(call) => {
            view.tag = 20; view.first = Span::slice(&call.values); view.second = Span::slice(call.error.as_slice()); view.left = Node::of(call, Kind::Call);
        }
        InstKind::ViewLoad { dst, address, index, stride, .. } => {
            view.tag = 21; view.a = *dst; view.b = *index; view.first = Span::slice(address); view.left = Node::of(stride, Kind::Stride);
        }
        InstKind::RayQuery { origin, direction, minimum, maximum, cull_back_faces, result, .. } => {
            view.tag = 22; view.first = Span::slice(match result { RayResult::Any(reg) => std::slice::from_ref(reg), RayResult::Closest(regs) => regs });
            view.second = Span::slice(origin); view.left = Node::of(direction, Kind::Words3); view.a = *minimum; view.b = *maximum;
            if let Some(reg) = cull_back_faces { view.c = 1; view.d = *reg; }
        }
        InstKind::Constant(dst, ..) => { view.tag = 23; view.a = *dst; }
        InstKind::FunctionAddress(dst, target) => { view.tag = 24; view.a = *dst; view.left = Node::of(target, Kind::Target); }
        InstKind::PositionAddress { dst, address } => { view.tag = 25; view.a = *dst; view.left = Node::of(address, Kind::Reference); }
        InstKind::BufferStride(dst, binding) => { view.tag = 26; view.a = *dst; view.b = *binding; }
        InstKind::Index(dst) => { view.tag = 27; view.a = *dst; }
        InstKind::Uniform(dst, word) => { view.tag = 28; view.a = *dst; view.b = *word as usize; }
        InstKind::LoadLocal(dst, slot) => { view.tag = 29; view.a = *dst; view.b = *slot; }
        InstKind::StoreLocal(slot, value) => { view.tag = 30; view.a = *slot; view.b = *value; }
        InstKind::LoadLocalIndexed { dst, base, len, index, .. } => { view.tag = 31; view.a = *dst; view.b = *base; view.c = *len; view.d = *index; }
        InstKind::StoreLocalIndexed { base, len, index, value, .. } => { view.tag = 32; view.a = *base; view.b = *len; view.c = *index; view.d = *value; }
        InstKind::Load { dst, binding, index, .. } => { view.tag = 33; view.a = *dst; view.b = *binding; view.c = *index; }
        InstKind::Store { binding, index, value, .. } => { view.tag = 34; view.a = *binding; view.b = *index; view.d = *value; }
        InstKind::Unary { dst, a, .. } => { view.tag = 35; view.a = *dst; view.b = *a; }
        InstKind::Binary { dst, a, b, .. } => { view.tag = 36; view.a = *dst; view.b = *a; view.c = *b; view.d = usize::from(value.float_preserve); }
        InstKind::IntegerDivide { result, src, divisor } => {
            view.tag = 37; view.a = *src; view.left = Node::of(divisor, Kind::Divisor); view.right = Node::of(result, Kind::Division);
        }
        InstKind::Cast { dst, src } => { view.tag = 38; view.a = *dst; view.b = *src; }
        InstKind::Math { dst, args, op } => { view.tag = 39; view.a = *dst; view.b = *op as usize; view.first = Span::slice(args); }
        InstKind::Select { dst, condition, a, b } => { view.tag = 40; view.a = *dst; view.b = *condition; view.c = *a; view.d = *b; }
        InstKind::If { condition, yes, no } => { view.tag = 41; view.a = *condition; view.first = Span::slice(yes); view.second = Span::slice(no); }
        InstKind::Loop { test, condition, body } => { view.tag = 42; view.a = *test; view.first = Span::slice(condition); view.second = Span::slice(body); }
        InstKind::Break => view.tag = 43,
        InstKind::Continue => view.tag = 44,
        InstKind::Scope(body) => { view.tag = 45; view.first = Span::slice(body); }
        InstKind::Exit => view.tag = 46,
        InstKind::Raise(error) => { view.tag = 47; view.a = *error; }
        InstKind::Cleanup { .. } => view.tag = 48,
    }
    view
}

#[derive(Clone, Copy)]
#[repr(C)]
struct FloatCode { code: Span, registers: *const c_void }
impl FloatCode {
    const EMPTY: Self = Self { code: Span::EMPTY, registers: ptr::null() };
    fn of(code: &[Inst], registers: &Vec<Scalar>) -> Self {
        Self { code: Span::slice(code), registers: ptr::from_ref(registers).cast() }
    }
    fn body(value: &ValueCode) -> Self { Self::of(&value.code, &value.registers) }
}
#[repr(C)]
struct FloatInput {
    body: FloatCode, identity: FloatCode, combine: FloatCode, project: FloatCode,
    collective: Word,
}
#[repr(C)]
struct FloatScalar { kind: u32, integer: u32, floating: u32 }
#[repr(C)]
struct FloatHost {
    ir: Host,
    scalar: unsafe extern "C" fn(*const c_void, usize) -> FloatScalar,
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::float_preserve"]
    fn b_float_preserve(input: FloatInput, scalar: u32, host: FloatHost) -> u32;
}
unsafe extern "C" fn float_scalar(registers: *const c_void, index: usize) -> FloatScalar {
    let scalar = unsafe { &*registers.cast::<Vec<Scalar>>() }[index];
    FloatScalar { kind: scalar as u32, integer: u32::from(scalar.is_integer()), floating: u32::from(scalar.is_float()) }
}
pub(super) fn float_preserve(stage: &Stage, scalar: Scalar) -> bool {
    let mut input = FloatInput {
        body: FloatCode::of(&stage.code, &stage.registers), identity: FloatCode::EMPTY,
        combine: FloatCode::EMPTY, project: FloatCode::EMPTY,
        collective: Word { present: 0, value: 0 },
    };
    let collective = match &stage.operation {
        Operation::Reduce(value) => Some(&value.ty),
        Operation::SegmentedReduce(value) => Some(&value.values.ty),
        Operation::ScanExclusive(value) => Some(&value.ty),
        _ => None,
    };
    if let Some(ty) = collective {
        input.collective = Word { present: 1, value: ty.components[0].0 as usize };
    }
    let custom = match &stage.operation {
        Operation::CustomReduce(value)
        | Operation::CustomScan(CustomScan { values: value, .. })
        | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values: value, .. }) => Some(value),
        _ => None,
    };
    if let Some(value) = custom {
        input.identity = FloatCode::body(&value.identity);
        input.combine = FloatCode::body(&value.combine);
        if let Some(project) = &value.project { input.project = FloatCode::body(project); }
    }
    unsafe { b_float_preserve(input, scalar as u32, FloatHost { ir: HOST, scalar: float_scalar }) != 0 }
}
