//! Synchronous access to the original x64 emitter and its scalar or packet blocks.
use super::*;
use std::ffi::c_void;
use std::ptr::NonNull;

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

#[repr(C)]
#[derive(Clone, Copy)]
struct Block {
    start: *const c_void,
    count: usize,
}

impl Block {
    const EMPTY: Self = Self { start: NonNull::<usize>::dangling().as_ptr().cast(), count: 0 };

    fn from_slice<I>(code: &[I]) -> Self {
        Self { start: code.as_ptr().cast(), count: code.len() }
    }
}

#[repr(u32)]
enum Kind { If, Loop, Scope, Exit, Break, Continue, Raise }

#[repr(C)]
struct Control {
    first: Block,
    second: Block,
    register: Reg,
    kind: Kind,
}

impl<I> From<Flow<'_, I>> for Control {
    fn from(flow: Flow<'_, I>) -> Self {
        let (kind, register, first, second) = match flow {
            Flow::If(reg, yes, no) => (Kind::If, reg, Block::from_slice(yes), Block::from_slice(no)),
            Flow::Loop(condition, reg, body) => (Kind::Loop, reg, Block::from_slice(condition), Block::from_slice(body)),
            Flow::Scope(body) => (Kind::Scope, 0, Block::from_slice(body), Block::EMPTY),
            Flow::Exit => (Kind::Exit, 0, Block::EMPTY, Block::EMPTY),
            Flow::Break => (Kind::Break, 0, Block::EMPTY, Block::EMPTY),
            Flow::Continue => (Kind::Continue, 0, Block::EMPTY, Block::EMPTY),
            Flow::Raise(reg) => (Kind::Raise, reg, Block::EMPTY, Block::EMPTY),
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
struct Host {
    emit: unsafe extern "C" fn(Slot, Slot, Block) -> u32,
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
    fn emit_control(context: Slot, emitter: Slot, control: Control, host: Host);
}

struct Context<F, L> {
    emit: F,
    load: L,
    result: Result<(), &'static str>,
}

unsafe fn emitter<'a, 'b>(slot: Slot) -> &'a mut Emitter<'b> {
    unsafe { &mut *slot.cast() }
}

unsafe extern "C" fn child<'a, I, F, L>(context: Slot, slot: Slot, block: Block) -> u32
where
    F: Fn(&mut Emitter<'a>, &[I]) -> Result<(), &'static str>,
{
    let context = unsafe { &mut *context.cast::<Context<F, L>>() };
    let code = unsafe { std::slice::from_raw_parts(block.start.cast(), block.count) };
    context.result = (context.emit)(unsafe { emitter(slot) }, code);
    u32::from(context.result.is_ok())
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

impl<'a> Emitter<'a> {
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
