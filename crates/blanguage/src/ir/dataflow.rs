//! Borrow the original typed IR fields for the compiler's B dataflow queries.
//! The model selects roles and order; these views neither enumerate query
//! results nor own, copy, retain, or rewrite the IR containers.
use super::*;
use crate::builtin_types::ScalarCode;
use std::{ffi::c_void, marker::PhantomData, mem::{MaybeUninit, size_of}, ptr};

#[derive(Clone, Copy)]
#[repr(u32)]
enum Kind {
    Instruction, Call, Target, Argument, DatasetBorrow, Reference, Storage,
    Indexed, Stride, Texture, AtomicOp, Alias, Effect, Division, Divisor, Words3,
    Component, Stage, RayRange, Operation, Buffer, Uniform, ValueCode, CustomReduction, Scalar, Tile, Registers,
    FirstComponentScalar, PageBuffer, PageType, PageVariant, PageField, Comparison, BinaryOperation,
    TilePublication, TileCollective, Type,
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Node { data: *const c_void, kind: Kind }
impl Node {
    pub(crate) fn instruction(value: &Inst) -> Self { Self::of(value, Kind::Instruction) }
    fn of<T>(value: &T, kind: Kind) -> Self { Self { data: ptr::from_ref(value).cast(), kind } }
    pub(crate) fn call(value: &Call) -> Self { Self::of(value, Kind::Call) }
    pub(crate) fn ty(value: &Ty) -> Self { Self::of(value, Kind::Type) }
    pub(crate) fn operation(value: &Operation) -> Self { Self::of(value, Kind::Operation) }
    pub(crate) const EMPTY: Self = Self { data: ptr::null(), kind: Kind::Instruction };
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Span { data: *const c_void, pub(crate) count: usize, stride: usize }
impl Span {
    pub(crate) const EMPTY: Self = Self { data: ptr::null(), count: 0, stride: 0 };
    pub(crate) fn slice<T>(values: &[T]) -> Self { Self { data: values.as_ptr().cast(), count: values.len(), stride: size_of::<T>() } }
    pub(crate) fn mutable<T>(values: &mut [T]) -> Self {
        Self { data: values.as_mut_ptr().cast(), count: values.len(), stride: size_of::<T>() }
    }
    pub(crate) fn field<T, U>(values: &[T], field: fn(&T) -> &U) -> Self {
        let offset = values.first().map_or(0, |value| {
            ptr::from_ref(field(value)).addr() - ptr::from_ref(value).addr()
        });
        Self {
            data: values.as_ptr().cast::<u8>().wrapping_add(offset).cast(),
            count: values.len(), stride: size_of::<T>(),
        }
    }
    // The caller must pass a span borrowed from a slice of T.
    pub(crate) unsafe fn as_slice<'a, T>(self) -> &'a [T] {
        unsafe { std::slice::from_raw_parts(self.data.cast(), self.count) }
    }
    fn pairs(values: &[(u32, Reg)]) -> Self {
        Self {
            data: values.first().map_or(ptr::null(), |value| ptr::from_ref(&value.1).cast()),
            count: values.len(), stride: size_of::<(u32, Reg)>(),
        }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct View {
    tag: u32, pub(crate) a: usize, pub(crate) b: usize, c: usize, d: usize,
    pub(crate) first: Span, second: Span, left: Node, right: Node,
}
impl View {
    // The selected instruction role fixes T and the lifetime of the original loan.
    pub(crate) unsafe fn left_as<'a, T>(self) -> &'a T {
        unsafe { &*self.left.data.cast() }
    }
    fn tag(tag: u32) -> Self {
        Self { tag, a: 0, b: 0, c: 0, d: 0, first: Span::EMPTY, second: Span::EMPTY,
            left: Node::EMPTY, right: Node::EMPTY }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Host { inspect: unsafe extern "C" fn(Node) -> View }
pub(crate) const HOST: Host = Host { inspect };
#[derive(Clone, Copy)]
#[repr(u32)]
pub(crate) enum ResultRole { Value, Quotient, Remainder }
impl ResultRole {
    pub(crate) fn reuse(self) -> ReuseResult {
        match self {
            Self::Value => ReuseResult::Value,
            Self::Quotient => ReuseResult::Quotient,
            Self::Remainder => ReuseResult::Remainder,
        }
    }
    pub(crate) fn division(self) -> Option<IntegerDivisionOp> {
        match self {
            Self::Value => None,
            Self::Quotient => Some(IntegerDivisionOp::Quotient),
            Self::Remainder => Some(IntegerDivisionOp::Remainder),
        }
    }
}
pub(crate) type ResultOriginVisitor =
    unsafe extern "C" fn(*mut c_void, Reg, usize, usize, ResultRole) -> u32;
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
    #[link_name = "bootstrap.ir_dataflow::result_origins"]
    fn b_result_origins(source: Node, host: Host, context: *mut c_void, visit: ResultOriginVisitor);

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

pub(crate) fn result_origins<F: FnMut(Reg, ResultRole)>(value: &Inst, mut visit: F) {
    unsafe extern "C" fn emit<F: FnMut(Reg, ResultRole)>(
        context: *mut c_void, reg: Reg, _: usize, _: usize, role: ResultRole,
    ) -> u32 {
        unsafe { (&mut *context.cast::<F>())(reg, role); }
        0
    }
    unsafe { b_result_origins(Node::of(value, Kind::Instruction), HOST,
        ptr::from_mut(&mut visit).cast(), emit::<F>); }
}

pub(super) fn results(value: &Inst) -> impl Iterator<Item = Reg> + '_ {
    ResultIter { state: unsafe { b_results(Node::of(value, Kind::Instruction), HOST) }, owner: PhantomData }
}
pub(super) fn instruction_locals(value: &Inst, writing: bool) -> impl Iterator<Item = usize> + '_ {
    LocalIter { state: unsafe { b_locals(Node::of(value, Kind::Instruction), u32::from(writing), HOST) }, owner: PhantomData }
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
        Kind::TilePublication => tile_publication(unsafe { &*node.data.cast() }),
        Kind::TileCollective => tile_collective(unsafe { &*node.data.cast() }),
        Kind::Call => {
            let call = unsafe { &*node.data.cast::<Call>() };
            let mut view = View::tag(0);
            view.first = Span::slice(&call.arguments); view.second = Span::slice(&call.memory);
            view.left = Node::of(&call.target, Kind::Target);
            view.right = call.result.as_ref().map_or(Node::EMPTY, Node::ty);
            if let Some(error) = call.error { view.a = error; view.b = 1; }
            view
        }
        Kind::Target => match unsafe { &*node.data.cast::<CallTarget>() } {
            CallTarget::External(name) => {
                let mut view = View::tag(0);
                view.first = Span::slice(name.library.as_bytes());
                view.second = Span::slice(name.symbol.as_bytes());
                view
            }
            CallTarget::Defined(_) => View::tag(1),
            CallTarget::Indirect(reg) => { let mut view = View::tag(2); view.a = *reg; view }
        },
        Kind::Argument => match unsafe { &*node.data.cast::<CallArgument>() } {
            CallArgument::Value { ty, values, dataset_borrow, .. } => {
                let mut view = View::tag(0); view.first = Span::slice(values);
                view.left = dataset_borrow.as_deref().map_or(Node::EMPTY, |value| Node::of(value, Kind::DatasetBorrow));
                view.right = Node::ty(ty);
                view
            }
            CallArgument::Reference { address, access, ty, .. } => {
                let mut view = View::tag(1); view.a = *access as usize;
                view.second = Span::slice(&ty.components);
                view.left = Node::of(address, Kind::Reference); view
            }
        },
        Kind::Type => {
            let ty = unsafe { &*node.data.cast::<Ty>() };
            let mut view = View::tag(0);
            view.a = ty.size as usize;
            view.b = ty.align as usize;
            view.first = Span::slice(ty.name.as_bytes());
            view.second = Span::slice(&ty.components);
            view
        }
        Kind::DatasetBorrow => {
            let value = unsafe { &*node.data.cast::<DatasetBorrow>() };
            let mut view = View::tag(0); view.first = Span::slice(&value.frame);
            view.second = Span::slice(std::slice::from_ref(&value.handle)); view
        }
        Kind::Reference => {
            let value = unsafe { &*node.data.cast::<CallReference>() };
            let mut view = View::tag(0); view.a = value.word as usize; view.first = Span::slice(value.offset.as_slice());
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
            IndexedSource::Pointer { address, index, stride, aliases } => {
                let mut view = View::tag(2); view.first = Span::slice(address); view.a = *index;
                view.b = *stride as usize; view.second = Span::slice(aliases); view
            }
            IndexedSource::Tile { tile, index } => { let mut view = View::tag(3); view.a = *tile; view.b = *index; view }
        },
        Kind::Stride => match unsafe { &*node.data.cast::<ElementStride>() } {
            ElementStride::Constant(bytes) => { let mut view = View::tag(0); view.a = *bytes as usize; view },
            ElementStride::Value(values) => { let mut view = View::tag(1); view.first = Span::slice(values); view }
        },
        Kind::Texture => {
            let value = unsafe { &*node.data.cast::<TextureSource>() };
            let mut view = View::tag(0); view.a = value.binding; view.first = Span::slice(value.parameter.as_slice());
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
            let mut view = View::tag(tag); view.a = a; view.b = b;
            if let AtomicOp::CompareExchange { failure, .. } = value { view.c = *failure as usize; }
            view
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
            IntegerDivisor::Constant(value) => { let mut view = View::tag(0); view.a = value.get() as usize; view },
            IntegerDivisor::Value(a) => { let mut view = View::tag(1); view.a = *a; view }
        },
        Kind::Words3 => {
            let values = unsafe { &*node.data.cast::<[Reg; 3]>() };
            let mut view = View::tag(0); view.first = Span::slice(values); view
        }
        Kind::Component => {
            let (scalar, byte) = unsafe { &*node.data.cast::<(Scalar, u32)>() };
            let mut view = View::tag(0); view.a = *byte as usize; view.b = scalar.size() as usize;
            view.c = crate::builtin_types::ScalarCode::from(*scalar) as usize; view
        }
        Kind::Stage => {
            let stage = unsafe { &*node.data.cast::<Stage>() };
            let mut view = View::tag(0); view.first = Span::slice(&stage.code);
            view.a = usize::from(stage.prefix.is_some()); view.b = usize::from(stage.selection.is_some());
            view.left = Node::of(&stage.operation, Kind::Operation);
            view.second = Span::slice(&stage.registers); view.c = ptr::from_ref(&stage.locals).addr(); view
        }
        Kind::Operation => {
            let pair = |tag, source, target| { let mut view = View::tag(tag); view.a = source; view.b = target; view };
            let typed = |tag, values: &Reduction, ty: &Ty| {
                let mut view = pair(tag, values.source, values.target);
                view.first = Span::slice(&ty.components);
                view.left = Node::of(ty, Kind::FirstComponentScalar); view
            };
            let custom = |tag, values: &CustomReduction| {
                let mut view = pair(tag, values.fields.source, values.fields.target);
                view.left = Node::of(values, Kind::CustomReduction); view
            };
            match unsafe { &*node.data.cast::<Operation>() } {
                Operation::Map => View::tag(0),
                Operation::Reduce(v) => typed(1, &v.fields, &v.ty),
                Operation::CustomReduce(v) => custom(2, v),
                Operation::CustomScan(v) => custom(3, &v.values),
                Operation::CustomSegmentedReduce(v) => custom(4, &v.values),
                Operation::SegmentedReduce(v) => typed(5, &v.values.fields, &v.values.ty),
                Operation::ScanExclusive(v) => typed(6, &v.values, &v.ty),
                Operation::Compact(v) => {
                    let mut view = pair(7, v.source, v.target);
                    view.c = v.flags;
                    view.d = v.flag_word as usize;
                    view
                }
                Operation::StableSort(v) => pair(8, v.source, v.target),
            }
        }
        Kind::Buffer => {
            let value = unsafe { &*node.data.cast::<Buffer>() };
            let mut view = View::tag(0); view.first = Span::slice(&value.ty.components); view
        }
        Kind::PageBuffer => {
            let buffer = unsafe { &*node.data.cast::<Buffer>() };
            let mut view = View::tag(0);
            view.a = buffer.stride as usize;
            view.left = Node::of(&buffer.ty, Kind::PageType);
            view
        }
        Kind::PageType => {
            let ty = unsafe { &*node.data.cast::<Ty>() };
            let mut view = View::tag(0);
            view.first = Span::slice(&ty.variants);
            view.second = Span::slice(&ty.fields);
            if let Some(element) = &ty.view {
                view.tag = 1;
                view.a = element.size as usize;
            } else if let Some(element) = &ty.array {
                view.tag = 2;
                view.b = ty.values[0] as usize;
                view.left = Node::of(element.as_ref(), Kind::PageType);
            }
            view
        }
        Kind::PageVariant => {
            let variant = unsafe { &*node.data.cast::<Variant>() };
            let mut view = View::tag(0);
            view.left = Node::of(variant.ty.as_ref(), Kind::PageType);
            view
        }
        Kind::BinaryOperation => View::tag(binary_code(unsafe { *node.data.cast::<BinaryOp>() })),
        Kind::Comparison => View::tag(unsafe { *node.data.cast::<Comparison>() } as u32),
        Kind::PageField => {
            let field = unsafe { &*node.data.cast::<(String, u32, Box<Ty>)>() };
            let mut view = View::tag(0);
            view.left = Node::of(field.2.as_ref(), Kind::PageType);
            view
        }
        Kind::Uniform => {
            let value = unsafe { &*node.data.cast::<Uniform>() };
            let mut view = View::tag(0); view.first = Span::slice(&value.ty.components); view
        }
        Kind::ValueCode => {
            let value = unsafe { &*node.data.cast::<ValueCode>() };
            let mut view = View::tag(0); view.first = Span::slice(&value.code);
            view.second = Span::slice(&value.registers);
            view.c = ptr::from_ref(&value.locals).addr();
            view.left = Node::of(&value.inputs, Kind::Registers); view.right = Node::of(&value.outputs, Kind::Registers); view
        }
        Kind::CustomReduction => {
            let value = unsafe { &*node.data.cast::<CustomReduction>() };
            let mut view = View::tag(0); view.left = Node::of(&value.identity, Kind::ValueCode);
            view.right = Node::of(&value.combine, Kind::ValueCode); view.first = Span::slice(value.project.as_slice()); view.a = value.ty.components.len(); view
        }
        Kind::Registers => {
            let values = unsafe { &*node.data.cast::<Vec<Reg>>() };
            let mut view = View::tag(0); view.first = Span::slice(values); view
        }
        Kind::Scalar => View::tag(crate::builtin_types::ScalarCode::from(unsafe { *node.data.cast::<Scalar>() }) as u32),
        Kind::FirstComponentScalar => {
            let ty = unsafe { &*node.data.cast::<Ty>() };
            View::tag(crate::builtin_types::ScalarCode::from(ty.components[0].0) as u32)
        }
        Kind::Tile => {
            let tile = unsafe { &*node.data.cast::<Tile>() };
            let mut view = match tile.source {
                TileSource::Input { binding, word } => {
                    let mut view = View::tag(0); view.a = binding; view.b = word as usize; view
                }
                TileSource::Published => View::tag(1),
            };
            view.first = Span::slice(&tile.ty.components); view.c = tile.ty.size as usize; view
        }
        Kind::RayRange => {
            let range = unsafe { &*node.data.cast::<std::ops::Range<usize>>() };
            let mut view = View::tag(0); view.a = range.start; view.b = range.end; view
        }
    }
}

fn binary_code(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::FAdd => 0, BinaryOp::FSub => 1, BinaryOp::FMul => 2, BinaryOp::FDiv => 3,
        BinaryOp::IAdd => 4, BinaryOp::ISub => 5, BinaryOp::IMul => 6,
        BinaryOp::Compare(cmp) => 7 + cmp as u32,
        BinaryOp::And => 13, BinaryOp::Or => 14, BinaryOp::Xor => 15,
        BinaryOp::ShiftLeft => 16, BinaryOp::ShiftRightLogical => 17, BinaryOp::ShiftRightArithmetic => 18,
    }
}

fn tile_publication(value: &Publication) -> View {
    let mut view = View::tag(13);
    view.a = value.tile;
    view.first = Span::pairs(&value.values);
    view.second = Span::field(&value.values, |field| &field.0);
    view.left = Node::of(value, Kind::TilePublication);
    view
}
fn tile_collective(value: &TileCollective) -> View {
    let mut view = View::tag(14);
    view.a = value.tile;
    view.b = value.op.operator() as usize;
    view.c = usize::from(value.op.exclusive());
    view.first = Span::pairs(&value.results);
    view.second = Span::field(&value.results, |field| &field.0);
    view.left = Node::of(value, Kind::TileCollective);
    view
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
        InstKind::TextureRead { dst, texture, coordinate, level, width } => {
            view.tag = 6; view.a = *dst; view.b = *level; view.c = *width as usize; view.first = Span::slice(coordinate); view.left = Node::of(texture, Kind::Texture);
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
            view.tag = 10; view.a = value.word as usize; view.b = value.scalar.size() as usize;
            view.c = usize::from(value.scalar.is_signed()); view.d = value.order as usize;
            view.first = Span::slice(value.result.as_slice()); view.second = Span::slice(value.offset.as_slice());
            view.left = Node::of(&value.target, Kind::Indexed); view.right = Node::of(&value.op, Kind::AtomicOp);
        }
        InstKind::Append(value) => {
            view.tag = 11; view.a = value.accepted; view.b = value.count.binding; view.c = value.target;
            view.d = value.count.word as usize;
            view.first = Span::pairs(&value.values);
            view.second = Span {
                data: value.values.first().map_or(ptr::null(), |value| ptr::from_ref(&value.0).cast()),
                count: value.values.len(), stride: size_of::<(u32, Reg)>(),
            };
        }
        InstKind::TileBarrier => view.tag = 12,
        InstKind::TilePublish(value) => return tile_publication(value),
        InstKind::TileCollective(value) => return tile_collective(value),
        InstKind::TileLength(dst) => { view.tag = 15; view.a = *dst; }
        InstKind::TileLane(dst) => { view.tag = 16; view.a = *dst; }
        InstKind::TileLoad { dst, tile, index, word } => { view.tag = 17; view.a = *dst; view.b = *tile; view.c = *index; view.d = *word as usize; }
        InstKind::Subgroup { dst, value, argument, op } => {
            view.tag = 18; view.a = *dst; view.c = *op as usize; view.first = Span::slice(value.as_slice()); view.second = Span::slice(argument.as_slice());
        }
        InstKind::Print { dst, value } => { view.tag = 19; view.a = *dst; view.b = *value; }
        InstKind::Call(call) => {
            view.tag = 20; view.first = Span::slice(&call.values); view.second = Span::slice(call.error.as_slice()); view.left = Node::of(call, Kind::Call);
            #[cfg(feature="compiler")]
            {view.a = usize::from(matches!(&call.failure, Some(crate::check::FailureContinuation::Scope(_))));}
        }
        InstKind::ViewLoad { dst, address, index, stride, word } => {
            view.tag = 21; view.a = *dst; view.b = *index; view.c = *word as usize; view.first = Span::slice(address); view.left = Node::of(stride, Kind::Stride);
        }
        InstKind::RayQuery { scene, origin, direction, minimum, maximum, cull_back_faces, result } => {
            view.tag = 22; view.first = Span::slice(match result { RayResult::Any(reg) => std::slice::from_ref(reg), RayResult::Closest(regs) => regs });
            view.second = Span::slice(origin); view.left = Node::of(direction, Kind::Words3); view.a = *minimum; view.b = *maximum;
            // The optional cull operand stores register + 1. Zero denotes no operand.
            view.c = *scene; view.d = cull_back_faces.map_or(0, |register| register + 1);
        }
        InstKind::Constant(dst, scalar, bits) => { view.tag = 23; view.a = *dst; view.b = *bits as usize; view.c = *scalar as usize; }
        InstKind::FunctionAddress(dst, target) => { view.tag = 24; view.a = *dst; view.left = Node::of(target, Kind::Target); }
        InstKind::PositionAddress { dst, address } => { view.tag = 25; view.a = *dst; view.left = Node::of(address, Kind::Reference); }
        InstKind::BufferStride(dst, binding) => { view.tag = 26; view.a = *dst; view.b = *binding; }
        InstKind::Index(dst) => { view.tag = 27; view.a = *dst; }
        InstKind::Uniform(dst, word) => { view.tag = 28; view.a = *dst; view.b = *word as usize; }
        InstKind::LoadLocal(dst, slot) => { view.tag = 29; view.a = *dst; view.b = *slot; }
        InstKind::StoreLocal(slot, value) => { view.tag = 30; view.a = *slot; view.b = *value; }
        InstKind::LoadLocalIndexed { dst, base, len, index, word } => { view.tag = 31; view.a = *dst; view.b = *base; view.c = *len; view.d = *index; view.first = Span::slice(std::slice::from_ref(word)); }
        InstKind::StoreLocalIndexed { base, len, index, value, word } => { view.tag = 32; view.a = *base; view.b = *len; view.c = *index; view.d = *value; view.first = Span::slice(std::slice::from_ref(word)); }
        InstKind::Load { dst, binding, index, word } => { view.tag = 33; view.a = *dst; view.b = *binding; view.c = *index; view.d = *word as usize; }
        InstKind::Store { binding, index, word, value } => { view.tag = 34; view.a = *binding; view.b = *index; view.c = *word as usize; view.d = *value; }
        InstKind::Unary { dst, op, a } => { view.tag = 35; view.a = *dst; view.b = *a; view.c = *op as usize; }
        InstKind::Binary { dst, op, a, b } => {
            view.tag = 36; view.a = *dst; view.b = *a; view.c = *b;
            view.d = usize::from(value.float_preserve);
            view.right = Node::of(op, Kind::BinaryOperation);
            if let BinaryOp::Compare(comparison) = op {
                view.left = Node::of(comparison, Kind::Comparison);
            }
        }
        InstKind::IntegerDivide { result, src, divisor } => {
            view.tag = 37; view.a = *src; view.left = Node::of(divisor, Kind::Divisor); view.right = Node::of(result, Kind::Division);
        }
        InstKind::Cast { dst, src } => { view.tag = 38; view.a = *dst; view.b = *src; }
        InstKind::Math { dst, args, op } => {
            view.tag = 39; view.a = *dst; view.b = *op as usize;
            view.d = usize::from(value.float_preserve);
            view.first = Span::slice(args);
        }
        InstKind::Select { dst, condition, a, b } => { view.tag = 40; view.a = *dst; view.b = *condition; view.c = *a; view.d = *b; }
        InstKind::If { condition, yes, no } => { view.tag = 41; view.a = *condition; view.first = Span::slice(yes); view.second = Span::slice(no); }
        InstKind::Loop { test, condition, body } => { view.tag = 42; view.a = *test; view.first = Span::slice(condition); view.second = Span::slice(body); }
        InstKind::Break => view.tag = 43,
        InstKind::Continue => view.tag = 44,
        InstKind::Scope(body) => { view.tag = 45; view.first = Span::slice(body); }
        InstKind::Exit => view.tag = 46,
        InstKind::Raise(error) => { view.tag = 47; view.a = *error; }
        InstKind::Cleanup { body, failure, .. } => { view.tag = 48; view.first = Span::slice(body); view.a = usize::from(failure.is_some()); }
    }
    view
}

#[cfg(feature="compiler")]
unsafe extern "C" {
    #[link_name="bootstrap.ir_dataflow::parallel_frame_used"]fn b_parallel_frame_used(code:Span,frame:Span,host:Host)->u32;
    #[link_name="bootstrap.ir_dataflow::parallel_scope_required"]fn b_parallel_scope_required(code:Span,host:Host)->u32;
}
#[cfg(feature="compiler")]
pub(crate) fn parallel_frame_used(code:&[Inst],frame:&[Reg])->bool {
    unsafe{b_parallel_frame_used(Span::slice(code),Span::slice(frame),HOST)!=0}
}
#[cfg(feature="compiler")]
pub(crate) fn parallel_scope_required(code:&[Inst])->bool {
    unsafe{b_parallel_scope_required(Span::slice(code),HOST)!=0}
}

#[derive(Clone, Copy)]
#[repr(C)]
struct RegisterFacts { kind: u32, integer: u32, floating: u32, bytes: u32 }

#[repr(C)]
pub(crate) struct RegisterHost {
    ir: Host,
    scalar: unsafe extern "C" fn(Span, usize) -> RegisterFacts,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::float_preserve"]
    fn b_float_preserve(stage: Node, requested: u32, host: RegisterHost) -> u32;
}

unsafe extern "C" fn register_facts(registers: Span, index: usize) -> RegisterFacts {
    let registers = unsafe { registers.as_slice::<Scalar>() };
    let scalar = registers[index];
    RegisterFacts {
        kind: crate::builtin_types::ScalarCode::from(scalar) as u32,
        integer: u32::from(scalar.is_integer()),
        floating: u32::from(scalar.is_float()),
        bytes: scalar.size(),
    }
}

pub(crate) const REGISTER_HOST: RegisterHost = RegisterHost { ir: HOST, scalar: register_facts };
const PRESERVE_F32: u32 = 1 << (crate::builtin_types::ScalarCode::F32 as u32);
const PRESERVE_F64: u32 = 1 << (crate::builtin_types::ScalarCode::F64 as u32);
const PRESERVE_FLOATS: u32 = PRESERVE_F32 | PRESERVE_F64;

fn float_modes(bits: u32) -> (bool, bool) {
    (bits & PRESERVE_F32 != 0, bits & PRESERVE_F64 != 0)
}

pub(super) fn float_preserve(stage: &Stage, scalar: Scalar) -> bool {
    let requested = 1 << (crate::builtin_types::ScalarCode::from(scalar) as u32);
    unsafe { b_float_preserve(Node::of(stage, Kind::Stage), requested, REGISTER_HOST) != 0 }
}

#[repr(C)]
struct StorageHost {
    ir: Host, stage: *const c_void,
    scalar_bytes: unsafe extern "C" fn(*const c_void, usize) -> u32,
}

#[repr(C)]
struct StorageOutput {
    context: *mut c_void,
    read: unsafe extern "C" fn(*mut c_void, usize, u32, i64),
    publication_phases: Span,
    group_storage: *mut c_void,
    reserve_groups: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
}

#[repr(C)]
struct StorageFacts { preserve: u32, group_count: usize }

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::storage_facts"]
    fn b_storage_facts(code: Span, tiles: Span, host: StorageHost, phase: i64, output: StorageOutput) -> StorageFacts;
}

unsafe extern "C" fn register_bytes(stage: *const c_void, register: usize) -> u32 {
    unsafe { &*stage.cast::<Stage>() }.registers[register].size()
}

fn collect_storage_reads(
    stage: &Stage, phase: i64, publication_phases: Span, group_storage: *mut c_void,
) -> (Vec<TileUse>, StorageFacts) {
    let mut output = ReadOutput { stage, reads: BTreeMap::new() };
    let facts = unsafe {
        b_storage_facts(
            Span::slice(&stage.code), Span::slice(&stage.tiles),
            StorageHost { ir: HOST, stage: ptr::from_ref(stage).cast(), scalar_bytes: register_bytes },
            phase,
            StorageOutput {
                context: ptr::from_mut(&mut output).cast(), read: emit_storage_read,
                publication_phases, group_storage, reserve_groups: reserve_tile_groups,
            },
        )
    };
    let reads = output.reads.into_iter().map(|(word, (end, read_mask))| {
        TileUse { key: word.into(), end, read_mask }
    }).collect();
    (reads, facts)
}

pub(crate) fn storage_usage(stage: &Stage) -> (Vec<TileUse>, u32) {
    if stage.tiles.is_empty() { return (Vec::new(), 0); }
    let (reads, facts) = collect_storage_reads(stage, -1, Span::EMPTY, ptr::null_mut());
    (reads, facts.preserve)
}

pub(super) fn storage_words(stage: &Stage) -> BTreeSet<SharedWord> {
    storage_usage(stage).0.into_iter().flat_map(|row| {
        (0..4).filter_map(move |part| {
            if row.read_mask & (1 << part) == 0 { return None; }
            let mut key = row.key;
            key.byte += part;
            Some(key.word())
        })
    }).collect()
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TilePlacedWord {
    pub(crate) key: TileKey,
    // A slot index until the CPU frame consumes these rows and writes byte offsets.
    pub(crate) position: usize,
    pub(crate) part: u32,
    pub(crate) unit: ScalarCode,
}
pub(crate) type TileLayout = Vec<TilePlacedWord>;

#[repr(C)]
pub(crate) struct TileUse { pub(super) key: TileKey, pub(super) end: i64, pub(super) read_mask: u32 }

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct TileGroup {
    storage: u64,
    owner: usize,
    first: u32,
    end: u32,
}

unsafe extern "C" fn reserve_tile_groups(storage: *mut c_void, count: usize) -> *mut c_void {
    let groups = unsafe { &mut *storage.cast::<Vec<MaybeUninit<TileGroup>>>() };
    groups.resize_with(count, MaybeUninit::uninit);
    groups.as_mut_ptr().cast()
}

pub(super) struct LayoutFacts {
    pub(super) reads: Vec<TileUse>,
    pub(super) publication_phases: Vec<i64>,
    pub(super) groups: Vec<MaybeUninit<TileGroup>>,
}

struct ReadOutput<'a> {
    stage: &'a Stage,
    reads: BTreeMap<SharedWord, (i64, u32)>,
}

pub(super) fn tile_layout_facts(stage: &Stage) -> LayoutFacts {
    let mut publication_phases = vec![-1i64; stage.tiles.len()];
    let mut groups = Vec::new();
    let (reads, facts) = collect_storage_reads(
        stage, 0, Span::mutable(&mut publication_phases), ptr::from_mut(&mut groups).cast(),
    );
    groups.truncate(facts.group_count);
    LayoutFacts { reads, publication_phases, groups }
}

unsafe extern "C" fn emit_storage_read(context: *mut c_void, tile: usize, offset: u32, phase: i64) {
    let output = unsafe { &mut *context.cast::<ReadOutput<'_>>() };
    let word = output.stage.shared_word(tile, offset);
    let byte = match word {
        SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word,
    };
    let (end, mask) = output.reads.entry(word.aligned()).or_insert((-1, 0));
    *end = (*end).max(phase);
    *mask |= 1 << (byte % 4);
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TileInputFragment { binding: usize, byte: u32, unit: ScalarCode }

impl TileInputFragment {
    fn order(&self) -> (usize, u32, u32) { (self.binding, self.byte, self.unit as u32) }
}

type TileInputWord = unsafe extern "C" fn(*mut c_void, TileInputFragment);
pub(crate) type InputFragments = Vec<TileInputFragment>;

pub(crate) fn input_snapshot_words(fragments: &[TileInputFragment]) -> BTreeSet<(usize, u32)> {
    fragments.iter().map(|fragment| (fragment.binding, fragment.byte & !3)).collect()
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::tile_input_words"]
    fn b_tile_input_words(
        tiles: Span, host: Host, uses: Span, output: *mut c_void, emit: TileInputWord,
    );
}

pub(super) fn input_fragments(stage: &Stage, uses: &[TileUse]) -> InputFragments {
    let mut output = Vec::new();
    if uses.is_empty() {
        return output;
    }
    unsafe {
        b_tile_input_words(
            Span::slice(&stage.tiles), HOST, Span::slice(uses),
            ptr::from_mut(&mut output).cast(), emit_input_fragment,
        );
    }
    output.sort_unstable_by_key(TileInputFragment::order);
    output.dedup_by_key(|fragment| fragment.order());
    output
}

unsafe extern "C" fn emit_input_fragment(output: *mut c_void, fragment: TileInputFragment) {
    unsafe { &mut *output.cast::<InputFragments>() }.push(fragment);
}

#[derive(Default)]
#[repr(C)]
pub(crate) struct ShaderSubgroups {
    capabilities: SubgroupFlags,
    builtins: u32,
    pub(crate) maximal_reconvergence: u32,
}
impl ShaderSubgroups {
    pub(crate) fn capabilities(&self) -> SubgroupCapabilities {
        self.capabilities.into()
    }

    pub(crate) fn uses_builtin(&self, operation: SubgroupOp) -> bool {
        self.builtins & (1 << operation as u32) != 0
    }
}
#[repr(C)]
pub(crate) struct ShaderCodeRequirements {
    pub(crate) subgroups: ShaderSubgroups,
    atomic64: u32,
    float_preserve: u32,
}
impl ShaderCodeRequirements {
    pub(crate) fn has_atomic64(&self) -> bool { self.atomic64 != 0 }

    pub(crate) fn float_modes(&self) -> (bool, bool) { float_modes(self.float_preserve) }
}
#[repr(C)]
struct ProgramCodeRequirements {
    subgroup: SubgroupFlags,
    atomic64: u32,
    float_preserve: u32,
    maximal_reconvergence: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::shader_code_requirements"]
    fn b_shader_code_requirements(operation: Node, code: Span, registers: Span, requested: u32,
        subgroups: bool, host: RegisterHost) -> ShaderCodeRequirements;
    #[link_name = "bootstrap.ir_dataflow::program_code_requirements"]
    fn b_program_code_requirements(stages: Span, requested: u32, host: RegisterHost) -> ProgramCodeRequirements;
}
pub(crate) fn shader_code_requirements(
    operation: &Operation, code: &[Inst], registers: &[Scalar], float_preserve: bool, subgroups: bool,
) -> ShaderCodeRequirements {
    let requested = if float_preserve { PRESERVE_FLOATS } else { 0 };
    unsafe {
        b_shader_code_requirements(Node::of(operation, Kind::Operation), Span::slice(code), Span::slice(registers),
            requested, subgroups, REGISTER_HOST)
    }
}
pub(super) fn program_code_requirements(program: &Program) -> DeviceRequirements {
    let facts = unsafe {
        b_program_code_requirements(Span::slice(program.stages()), PRESERVE_FLOATS, REGISTER_HOST)
    };
    let (atomic64, shared_atomic64) = atomic64_flags(facts.atomic64);
    let (float32_preserve, float64_preserve) = float_modes(facts.float_preserve);
    DeviceRequirements {
        atomic64, shared_atomic64, float32_preserve, float64_preserve,
        subgroup: facts.subgroup.into(),
        maximal_reconvergence: facts.maximal_reconvergence != 0,
        ..DeviceRequirements::default()
    }
}

#[repr(C)]
struct RayHost {
    ir: Host, used: *mut c_void,
    include: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    contains: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    fields: unsafe extern "C" fn(Span, usize, usize) -> Span,
}
#[repr(C)]
pub(crate) struct RayQuery {
    fields: Span,
    result: Reg,
    ordinal: u32,
    used: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::scan_ray_queries"]
    fn b_scan_ray_queries(code: Span, host: Host, closest_only: bool, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, Span, Reg)) -> u32;
    #[link_name = "bootstrap.ir_dataflow::ray_result_queries"]
    fn b_ray_result_queries(code: Span, queries: Span, ranges: Span, host: RayHost, context: *mut c_void,
        visit: unsafe extern "C" fn(*mut c_void, usize, usize) -> u32) -> u32;
    #[link_name = "bootstrap.ir_dataflow::shader_ray_usage"]
    fn b_shader_ray_usage(code: Span, queries: Span, closest: bool, host: RayHost) -> u32;
}
fn scan_ray_queries(code: Span, closest_only: bool) -> (Vec<RayQuery>, bool) {
    unsafe extern "C" fn push(context: *mut c_void, fields: Span, result: Reg) {
        unsafe { &mut *context.cast::<Vec<RayQuery>>() }.push(RayQuery { fields, result, ordinal: 0, used: 0 });
    }
    let mut queries = Vec::new();
    let closest = unsafe {
        b_scan_ray_queries(code, HOST, closest_only, ptr::from_mut(&mut queries).cast(), push) != 0
    };
    (queries, closest)
}

fn ray_host(used: &mut BTreeSet<Reg>) -> RayHost {
    RayHost { ir: HOST, used: ptr::from_mut(used).cast(),
        include: include_ray_operand, contains: contains_ray_operand, fields: ray_fields }
}

fn visit_ray_fields<F: FnMut(usize, usize) -> bool>(
    code: Span, queries: &[RayQuery], ranges: &[std::ops::Range<usize>], output: &mut F,
) -> bool {
    let mut used = BTreeSet::new();
    unsafe { b_ray_result_queries(code, Span::slice(queries), Span::slice(ranges), ray_host(&mut used),
        ptr::from_mut(output).cast(), visit_ray_query::<F>) != 0 }
}

pub(super) fn ray_result_queries<F: FnMut(usize, Reg) -> bool>(stage: &Stage,
    ranges: &[std::ops::Range<usize>], output: &mut F) -> bool {
    let code = Span::slice(&stage.code);
    let (queries, closest) = scan_ray_queries(code, true);
    !closest || visit_ray_fields(code, &queries, ranges, &mut |group, query| {
        output(group, queries[query].result)
    })
}

pub(crate) struct ShaderRayQueries {
    pub(crate) rows: Vec<RayQuery>,
    pub(crate) position_fetch: u32,
}

pub(crate) fn shader_ray_queries(code: &[Inst]) -> ShaderRayQueries {
    let code = Span::slice(code);
    let (mut rows, closest) = scan_ray_queries(code, false);
    let mut used = BTreeSet::new();
    let position_fetch = unsafe { b_shader_ray_usage(code, Span::mutable(&mut rows), closest, ray_host(&mut used)) };
    // Lookup order is independent of the declaration ordinal assigned by B.
    rows.sort_unstable_by_key(|row| (row.result, row.ordinal));
    ShaderRayQueries { rows, position_fetch }
}

unsafe extern "C" fn include_ray_operand(context: *mut c_void, reg: usize) -> u32 {
    unsafe { &mut *context.cast::<BTreeSet<Reg>>() }.insert(reg); 1
}
unsafe extern "C" fn contains_ray_operand(context: *mut c_void, reg: usize) -> u32 {
    u32::from(unsafe { &*context.cast::<BTreeSet<Reg>>() }.contains(&reg))
}
unsafe extern "C" fn ray_fields(values: Span, begin: usize, end: usize) -> Span {
    let fields = unsafe { std::slice::from_raw_parts(values.data.cast::<Reg>(), values.count) };
    Span::slice(&fields[begin..end])
}
unsafe extern "C" fn visit_ray_query<F: FnMut(usize, Reg) -> bool>(context: *mut c_void,
    group: usize, query: usize) -> u32 {
    u32::from(unsafe { &mut *context.cast::<F>() }(group, query))
}

#[repr(C)]
struct NumericRequirements { float64: u32, integer8: u32, integer16: u32 }
#[repr(C)]
struct NumericHost {
    ir: Host,
    locals: unsafe extern "C" fn(*const c_void, *mut c_void, unsafe extern "C" fn(*mut c_void, u32)),
}
unsafe extern "C" fn numeric_locals(source: *const c_void, context: *mut c_void,
    visit: unsafe extern "C" fn(*mut c_void, u32))
{
    for ty in unsafe { &*source.cast::<BTreeMap<usize, Ty>>() }.values() {
        for (scalar, _) in &ty.components {
            unsafe { visit(context, crate::builtin_types::ScalarCode::from(*scalar) as u32) };
        }
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::numeric_requirements"]
    fn b_numeric_requirements(stages: Span, buffers: Span, uniforms: Span, host: NumericHost) -> NumericRequirements;
}
pub(crate) fn numeric_requirements(program: &Program) -> (bool, bool, bool) {
    let flags = unsafe { b_numeric_requirements(Span::slice(program.stages()), Span::slice(&program.buffers),
        Span::slice(&program.uniforms), NumericHost { ir: HOST, locals: numeric_locals }) };
    (flags.float64 != 0, flags.integer8 != 0, flags.integer16 != 0)
}

fn atomic64_flags(flags: u32) -> (bool, bool) { (flags & 1 != 0, flags & 2 != 0) }

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::maximal_reconvergence"]
    fn b_maximal_reconvergence(code: Span, host: Host) -> u32;
    #[link_name = "bootstrap.ir_dataflow::program_maximal_reconvergence"]
    fn b_program_maximal_reconvergence(stages: Span, host: Host) -> u32;
}
pub(super) fn maximal_reconvergence(stage: &Stage) -> bool {
    unsafe { b_maximal_reconvergence(Span::slice(&stage.code), HOST) != 0 }
}
pub(super) fn program_maximal_reconvergence(program: &Program) -> bool {
    unsafe { b_program_maximal_reconvergence(Span::slice(program.stages()), HOST) != 0 }
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::shader_pages_fit"]
    fn b_shader_pages_fit(buffers: Span, bytes: u32, host: Host) -> u32;
}

pub(crate) fn shader_pages_fit(program: &Program, bytes: u32) -> bool {
    unsafe { b_shader_pages_fit(Span::slice(&program.buffers), bytes, HOST) != 0 }
}
