//! B emits synchronous completion control up to each actual asynchronous release.
use super::*;
use crate::check::application::TaskOutputSelection;
use std::{ffi::c_void, ptr};

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Cursor {
    phase: u32,
    index: usize,
    owner: usize,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Completion {
    result_owns: u32,
    positions: usize,
    datasets: usize,
}

#[repr(C)]
struct Ready { present: u32, owners: usize }

#[repr(C)]
struct Dataset { writes: u32, owners: usize }

#[allow(dead_code)] // Produced by bootstrap.task_completion.
#[repr(u32)]
enum Need { Result, Position, Owner, Dataset, Complete }

#[repr(C)]
struct Release { cursor: Cursor, need: Need }

#[repr(C)]
struct Host {
    begin_scope: unsafe extern "C" fn(*mut c_void),
    end_scope: unsafe extern "C" fn(*mut c_void, u32),
    position: unsafe extern "C" fn(*mut c_void, usize) -> Ready,
    begin_owner: unsafe extern "C" fn(*mut c_void, usize),
    end_owner: unsafe extern "C" fn(*mut c_void),
    dataset: unsafe extern "C" fn(*mut c_void, usize) -> Dataset,
    descriptor: unsafe extern "C" fn(*mut c_void, usize),
    guard: unsafe extern "C" fn(*mut c_void, usize, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.task_completion::next_release"]
    fn next_release(cursor: Cursor, completion: Completion, context: *mut c_void, host: Host) -> Release;
}

type Position<'a> = (u32, &'a Ty, Option<usize>);
type DatasetLoan<'a> = (u32, &'a Ty, &'a Ty, Access, Option<usize>);

struct CompletionCode<'b, 'w, 'a> {
    builder: &'b mut Builder<'w>,
    positions: &'a [Position<'a>],
    datasets: &'a [DatasetLoan<'a>],
    selection: &'a [TaskOutputSelection<'a>],
    output: [Reg; 2],
    input: [Reg; 2],
    zero: Reg,
    completed: Reg,

    address: [Reg; 2],
    ready: Option<&'a TaskOutputSelection<'a>>,
    outer: Vec<Inst>,
    owner_parent: Vec<Inst>,
    owner_condition: Reg,
    descriptor: Vec<Reg>,
    guards: Vec<owned_presence::Presence>,
}

impl CompletionCode<'_, '_, '_> {
    fn load(&mut self, address: [Reg; 2], scalar: Scalar, word: u32) -> Reg {
        let zero = self.zero;
        let dst = self.builder.reg(scalar);
        let source = IndexedSource::Pointer { address, index: zero, stride: 1, aliases: Vec::new() };
        self.builder.emit(InstKind::LoadIndexed { dst, source, offset: zero, word });
        dst
    }
}

unsafe extern "C" fn begin_scope(context: *mut c_void) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    code.outer = std::mem::take(&mut code.builder.code);
}

unsafe extern "C" fn end_scope(context: *mut c_void, skip_empty: u32) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let yes = std::mem::replace(&mut code.builder.code, std::mem::take(&mut code.outer));
    if skip_empty == 0 || !yes.is_empty() {
        code.builder.emit(InstKind::If { condition: code.completed, yes, no: Vec::new() });
    }
}

unsafe extern "C" fn position(context: *mut c_void, index: usize) -> Ready {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let (packet, _, selected) = code.positions[index];
    code.ready = selected.map(|index| &code.selection[index]);
    code.address = std::array::from_fn(|part| code.load(code.input, Scalar::U32, packet + part as u32 * 4));
    Ready { present: u32::from(code.ready.is_some()), owners: code.ready.map_or(0, |item| item.owners.len()) }
}

unsafe extern "C" fn begin_owner(context: *mut c_void, owner: usize) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let base = unsafe { code.ready.unwrap_unchecked() }.base;
    code.owner_condition = code.load(code.output, Scalar::Bool, base + owner as u32 * Scalar::Bool.size());
    code.owner_parent = std::mem::take(&mut code.builder.code);
}

unsafe extern "C" fn end_owner(context: *mut c_void) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let yes = std::mem::replace(&mut code.builder.code, std::mem::take(&mut code.owner_parent));
    code.builder.emit(InstKind::If { condition: code.owner_condition, yes, no: Vec::new() });
}

unsafe extern "C" fn dataset(context: *mut c_void, index: usize) -> Dataset {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let (_, _, _, access, selected) = code.datasets[index];
    code.ready = selected.map(|index| &code.selection[index]);
    Dataset { writes: u32::from(access == Access::Write), owners: code.ready.map_or(0, |item| item.owners.len()) }
}

unsafe extern "C" fn descriptor(context: *mut c_void, index: usize) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let (packet, layout, element, ..) = code.datasets[index];
    code.descriptor = layout.components.iter().map(|&(scalar, offset)| code.load(code.input, scalar, packet + offset)).collect();
    code.guards = vec![owned_presence::Presence::All; element.components.len()];
}

unsafe extern "C" fn guard(context: *mut c_void, index: usize, owner: usize) {
    let code = unsafe { &mut *context.cast::<CompletionCode<'_, '_, '_>>() };
    let element = code.datasets[index].2;
    let item = unsafe { code.ready.unwrap_unchecked() };
    let offset = item.owners[owner].0;
    let reg = code.load(code.output, Scalar::Bool, item.base + owner as u32 * Scalar::Bool.size());
    let component = element.components.iter().position(|&(_, at)| at == offset).unwrap();
    code.guards[component] = owned_presence::Presence::input(&Value {
        ty: Ty::scalar(Scalar::Bool), regs: vec![reg],
        proof: bounds::Proof::default().with_identity(ValueIdentity::Register(reg)),
        uniform: false, literal: false, constant: None,
    });
}

const HOST: Host = Host { begin_scope, end_scope, position, begin_owner, end_owner, dataset, descriptor, guard };

impl Builder<'_> {
    pub(in crate::check) async fn task_completion_cleanup(
        &mut self, positions: &[Position<'_>], datasets: &[DatasetLoan<'_>],
        selection: &[TaskOutputSelection<'_>], result: &Ty, span: Span,
    ) -> Result<String> {
        let name = format!("{}::@task_completion_cleanup{}", self.scope.module(), self.functions.units.len());
        let pointer = Ty::vector(Scalar::U32, 2);
        let inputs = [
            ("output", pointer.clone()), ("input", pointer),
            ("status", Ty::scalar(Scalar::U64)), ("completed", Ty::scalar(Scalar::Bool)),
        ];
        unsafe { self.world.call(self.task_callback(name, &inputs, true, span, async |builder, registers| {
            let [out_low, out_high, in_low, in_high, status, completed] = *registers else { unreachable!() };
            let slot = builder.local_slots(&Ty::scalar(Scalar::U64)).0[0];
            builder.emit(InstKind::StoreLocal(slot, status));
            builder.pending_error = Some(PendingError::Exit(slot));
            let zero = builder.constant(Scalar::U32, 0).regs[0];
            let mut code = CompletionCode {
                builder, positions, datasets, selection, output: [out_low, out_high], input: [in_low, in_high],
                zero, completed, address: [0; 2], ready: None, outer: Vec::new(), owner_parent: Vec::new(),
                owner_condition: 0, descriptor: Vec::new(), guards: Vec::new(),
            };
            let completion = Completion {
                result_owns: u32::from(result.owns_resources()), positions: positions.len(), datasets: datasets.len(),
            };
            let mut cursor = Cursor::default();
            loop {
                let release = unsafe { next_release(cursor, completion, ptr::from_mut(&mut code).cast(), HOST) };
                cursor = release.cursor;
                let builder = &mut *code.builder;
                match release.need {
                    Need::Result => unsafe {
                        builder.world.call(builder.drop_task_completion_value(result, code.output, 0, span)).await?;
                    }
                    Need::Position => unsafe {
                        builder.world.call(builder.drop_task_completion_value(positions[cursor.index].1, code.address, 0, span)).await?;
                    }
                    Need::Owner => {
                        let (offset, ty) = unsafe { code.ready.unwrap_unchecked() }.owners[cursor.owner];
                        unsafe { builder.world.call(builder.drop_task_completion_value(ty, code.address, offset, span)).await?; }
                    }
                    Need::Dataset => unsafe {
                        builder.world.call(builder.drop_task_completion_dataset(datasets[cursor.index].2, &code.descriptor, &code.guards, span)).await?;
                    }
                    Need::Complete => {
                        let error = builder.reg(Scalar::U64);
                        builder.emit(InstKind::LoadLocal(error, slot));
                        builder.emit(InstKind::Raise(error));
                        return Ok(());
                    }
                }
            }
        })).await }
    }
}
