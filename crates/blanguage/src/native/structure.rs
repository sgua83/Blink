//! Synchronous access to the original x64 emitter and its scalar or packet blocks.
use super::*;
use std::ffi::c_void;
use crate::ir::dataflow::{self, Span, View};

type Slot = *mut c_void;

pub(super) enum Flow<'a, I> {
    If(Reg, &'a [I], &'a [I]),
    Loop(&'a [I], Reg, &'a [I]),
    Scope(&'a [I]),
    Exit,
    Break,
    Continue,
    Raise(Reg),
}

#[repr(u32)]
enum Kind { If, Loop, Scope, Exit, Break, Continue, Raise }

#[repr(C)]
struct Control {
    first: Span,
    second: Span,
    register: Reg,
    kind: Kind,
}

impl<I> From<Flow<'_, I>> for Control {
    fn from(flow: Flow<'_, I>) -> Self {
        let (kind, register, first, second) = match flow {
            Flow::If(reg, yes, no) => (Kind::If, reg, Span::slice(yes), Span::slice(no)),
            Flow::Loop(condition, reg, body) => (Kind::Loop, reg, Span::slice(condition), Span::slice(body)),
            Flow::Scope(body) => (Kind::Scope, 0, Span::slice(body), Span::slice::<I>(&[])),
            Flow::Exit => (Kind::Exit, 0, Span::slice::<I>(&[]), Span::slice::<I>(&[])),
            Flow::Break => (Kind::Break, 0, Span::slice::<I>(&[]), Span::slice::<I>(&[])),
            Flow::Continue => (Kind::Continue, 0, Span::slice::<I>(&[]), Span::slice::<I>(&[])),
            Flow::Raise(reg) => (Kind::Raise, reg, Span::slice::<I>(&[]), Span::slice::<I>(&[])),
        };
        Self { first, second, register, kind }
    }
}

#[repr(u32)]
enum Jump { Always, Zero, Nonzero }

#[repr(u32)]
enum Scope { Block, Loop }

#[repr(u32)]
enum Target { Block, Loop, Function }

#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    emit: unsafe extern "C" fn(Slot, Slot, Span) -> u32,
    load: unsafe extern "C" fn(Slot, Slot, Reg),
    offset: unsafe extern "C" fn(Slot) -> usize,
    jump: unsafe extern "C" fn(Slot, Jump) -> usize,
    jump_to: unsafe extern "C" fn(Slot, usize),
    patch_here: unsafe extern "C" fn(Slot, usize),
    open: unsafe extern "C" fn(Slot, Scope, usize),
    close: unsafe extern "C" fn(Slot, Scope),
    defer: unsafe extern "C" fn(Slot, Target, usize),
    continuation: unsafe extern "C" fn(Slot) -> usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_control"]
    fn emit_control(context: Slot, emitter: Slot, control: Control, host: Host) -> u32;
    #[link_name = "bootstrap.x64_instruction::emit_scalar_body"]
    fn emit_scalar_body(context: Slot, emitter: Slot, code: Span, host: ScalarHost) -> ScalarBodyStatus;
}

struct Context<F, L> {
    emit: F,
    load: L,
    result: Result<(), &'static str>,
}

unsafe fn emitter<'a, 'b>(slot: Slot) -> &'a mut Emitter<'b> {
    unsafe { &mut *slot.cast() }
}

unsafe extern "C" fn child<'a, I, F, L>(context: Slot, slot: Slot, block: Span) -> u32
where
    F: Fn(&mut Emitter<'a>, &[I]) -> Result<(), &'static str>,
{
    let context = unsafe { &mut *context.cast::<Context<F, L>>() };
    let code = unsafe { block.as_slice() };
    context.result = (context.emit)(unsafe { emitter(slot) }, code);
    u32::from(context.result.is_err())
}

unsafe extern "C" fn test<'a, F, L>(context: Slot, slot: Slot, register: Reg)
where
    L: Fn(&mut Emitter<'a>, Reg),
{
    let context = unsafe { &*context.cast::<Context<F, L>>() };
    (context.load)(unsafe { emitter(slot) }, register);
}

unsafe extern "C" fn offset(slot: Slot) -> usize { unsafe { emitter(slot) }.code.len() }

unsafe extern "C" fn jump(slot: Slot, kind: Jump) -> usize {
    let branch = match kind {
        Jump::Always => Branch::Always,
        Jump::Zero => Branch::Zero32,
        Jump::Nonzero => Branch::Nonzero32,
    };
    unsafe { emitter(slot) }.jump(branch)
}

unsafe extern "C" fn jump_to(slot: Slot, target: usize) { unsafe { emitter(slot) }.jump_to(target); }

unsafe extern "C" fn patch_here(slot: Slot, field: usize) {
    let e = unsafe { emitter(slot) };
    e.patch(field, e.code.len());
}

unsafe extern "C" fn open(slot: Slot, scope: Scope, target: usize) {
    let e = unsafe { emitter(slot) };
    match scope {
        Scope::Block => e.exits.push(Vec::new()),
        Scope::Loop => {
            e.breaks.push(Vec::new());
            e.continues.push(target);
        }
    }
}

unsafe extern "C" fn close(slot: Slot, scope: Scope) {
    let e = unsafe { emitter(slot) };
    match scope {
        Scope::Block => e.close_exits(),
        Scope::Loop => {
            e.continues.pop();
            e.close_breaks();
        }
    }
}

unsafe extern "C" fn defer(slot: Slot, target: Target, field: usize) {
    let e = unsafe { emitter(slot) };
    match target {
        Target::Block => e.exits.last_mut().unwrap().push(field),
        Target::Loop => e.breaks.last_mut().unwrap().push(field),
        Target::Function => e.exits[0].push(field),
    }
}

unsafe extern "C" fn continuation(slot: Slot) -> usize {
    *unsafe { emitter(slot) }.continues.last().unwrap()
}

#[repr(u32)]
#[allow(dead_code)]
pub(super) enum ScalarLeaf {
    Call, Print, Texture, Append, Atomic, Publish, Collective, TileLength, TileLane, ViewLoad,
    Indexed, FunctionAddress, Storage, Value,
}

#[repr(u32)]
#[allow(dead_code)]
enum ScalarBodyStatus { Done, Unlowered, Subgroup, RayQuery }

#[repr(C)]
struct ScalarCursor { start: usize, ranges: usize }

#[repr(C)]
#[derive(Clone, Copy)]
struct ScalarHost {
    ir: dataflow::Host,
    control: Host,
    mark: unsafe extern "C" fn(Slot) -> ScalarCursor,
    publish: unsafe extern "C" fn(Slot, *const Inst, ScalarCursor),
    emit: unsafe extern "C" fn(Slot, ScalarLeaf, View),
}

unsafe extern "C" fn scalar_child(context: Slot, slot: Slot, code: Span) -> u32 {
    let host = unsafe { *context.cast::<ScalarHost>() };
    unsafe { emit_scalar_body(context, slot, code, host) as u32 }
}

unsafe extern "C" fn scalar_load(_: Slot, slot: Slot, register: Reg) {
    unsafe { emitter(slot) }.load(register);
}

unsafe extern "C" fn mark_scalar(slot: Slot) -> ScalarCursor {
    let e = unsafe { emitter(slot) };
    ScalarCursor { start: e.code.len(), ranges: e.operation_ranges.len() }
}

unsafe extern "C" fn publish_scalar(slot: Slot, inst: *const Inst, cursor: ScalarCursor) {
    unsafe { emitter(slot) }.operation_range(unsafe { &*inst }, cursor.start, cursor.ranges);
}

pub(super) unsafe extern "C" fn emit_scalar(slot: Slot, leaf: ScalarLeaf, value: View) {
    let e = unsafe { emitter(slot) };
    match leaf {
        ScalarLeaf::Call => e.native_call(unsafe { value.left_as() }),
        ScalarLeaf::Print => e.print_call(value.a, value.b),
        ScalarLeaf::Texture => e.texture_access(value),
        ScalarLeaf::Append => e.append(value),
        ScalarLeaf::Atomic => e.atomic(value),
        ScalarLeaf::Publish => e.publish_values(unsafe { value.left_as() }),
        ScalarLeaf::Collective => e.tile_collective(unsafe { value.left_as() }),
        ScalarLeaf::TileLength => e.tile_length(value.a),
        ScalarLeaf::TileLane => e.tile_lane(value.a),
        ScalarLeaf::ViewLoad => e.view_load(value),
        ScalarLeaf::Indexed => e.indexed_storage(value),
        ScalarLeaf::FunctionAddress => {
            match unsafe { value.left_as() } {
                CallTarget::Defined(name) => e.emit_symbol_target(NativeSymbol::Function(name.clone()), false),
                CallTarget::External(name) => e.emit_symbol_target(NativeSymbol::External(name.clone()), false),
                CallTarget::Indirect(reg) => e.load(*reg),
            }
            e.store(value.a);
        }
        ScalarLeaf::Storage => e.scalar_storage(value),
        ScalarLeaf::Value => e.scalar_value(value),
    }
}

impl<'a> Emitter<'a> {
    pub(super) fn block(&mut self, code: &[Inst]) -> Result<(), &'static str> {
        let mut host = ScalarHost {
            ir: dataflow::HOST,
            control: Host { emit: scalar_child, load: scalar_load, offset, jump, jump_to,
                patch_here, open, close, defer, continuation },
            mark: mark_scalar,
            publish: publish_scalar,
            emit: emit_scalar,
        };
        let result = unsafe {
            emit_scalar_body(std::ptr::from_mut(&mut host).cast(), std::ptr::from_mut(self).cast(),
                Span::slice(code), host)
        };
        match result {
            ScalarBodyStatus::Done => Ok(()),
            ScalarBodyStatus::Unlowered => Err("原生操作尚未降低到CPU执行IR"),
            ScalarBodyStatus::Subgroup => Err("CPU 后端不能执行 GPU subgroup 协作"),
            ScalarBodyStatus::RayQuery => Err("CPU 后端不能执行硬件光线查询"),
        }
    }

    pub(super) fn lower_control<I, F, L>(&mut self, flow: Flow<'_, I>, emit: F, load: L)
        -> Result<(), &'static str>
    where
        F: Fn(&mut Self, &[I]) -> Result<(), &'static str>,
        L: Fn(&mut Self, Reg),
    {
        let mut ctx = Context { emit, load, result: Ok(()) };
        let host = Host {
            emit: child::<I, F, L>, load: test::<F, L>,
            offset, jump, jump_to, patch_here, open, close, defer, continuation,
        };
        unsafe { emit_control(std::ptr::from_mut(&mut ctx).cast(), std::ptr::from_mut(self).cast(), flow.into(), host) };
        ctx.result
    }
}
