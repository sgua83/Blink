use crate::ir::{CallArgument, Ty};
use std::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
struct TypeRef {
    value: *const Ty,
    present: u32,
}

impl TypeRef {
    fn from(value: Option<&Ty>) -> Self {
        Self { value: value.map_or(ptr::null(), ptr::from_ref), present: u32::from(value.is_some()) }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Scope {
    input: usize,
    argument: usize,
    parent: usize,
    first: usize,
    nested: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Slot {
    input: usize,
    argument: usize,
    parent: usize,
    component: usize,
    nested: u32,
}

pub(in crate::check) struct TaskCompletionParameter {
    slot: Slot,
    pub result: Ty,
}

impl TaskCompletionParameter {
    pub(in crate::check) fn input(&self) -> usize { self.slot.input }

    pub(in crate::check) fn parent(&self) -> Option<(usize, usize)> {
        (self.slot.nested != 0).then_some((self.slot.parent, self.slot.component))
    }

    pub(super) fn argument_register(&self, arguments: &[CallArgument]) -> usize {
        let CallArgument::Value { values, .. } = &arguments[self.slot.argument] else { unreachable!() };
        values[self.slot.component]
    }
}

type Plan = Vec<TaskCompletionParameter>;

#[derive(Clone, Copy)]
#[repr(C)]
struct Host {
    visit: unsafe extern "C" fn(*mut c_void, *const Ty, Scope),
    task_result: unsafe extern "C" fn(*const Ty) -> TypeRef,
    component: unsafe extern "C" fn(*const Ty, u32) -> usize,
    length: unsafe extern "C" fn(*const c_void) -> usize,
    slot: unsafe extern "C" fn(*const c_void, usize) -> Slot,
    same_type: unsafe extern "C" fn(*const c_void, usize, *const Ty) -> u32,
    append: unsafe extern "C" fn(*mut c_void, Slot, *const Ty) -> usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.task_parameters::plan_input"]
    fn plan_input(plan: *mut c_void, ty: TypeRef, input: usize, argument: usize, width: usize, host: Host) -> usize;

    #[link_name = "bootstrap.task_parameters::plan_field"]
    fn plan_field(plan: *mut c_void, root: *const Ty, scope: Scope, word: u32, leaf: *const Ty, host: Host);
}

const HOST: Host = Host { visit, task_result, component, length, slot, same_type, append };

unsafe extern "C" fn visit(plan: *mut c_void, root: *const Ty, scope: Scope) {
    // root and its children belong to the original argument types, never to the growing plan.
    unsafe { &*root }.visit_borrow_fields(|word, leaf| {
        unsafe { plan_field(plan, root, scope, word, leaf, HOST) };
    });
}

unsafe extern "C" fn task_result(leaf: *const Ty) -> TypeRef {
    TypeRef::from(unsafe { &*leaf }.task_result())
}

unsafe extern "C" fn component(root: *const Ty, word: u32) -> usize {
    unsafe { &*root }.components.iter().position(|(_, at)| *at == word).unwrap()
}

unsafe extern "C" fn length(plan: *const c_void) -> usize { unsafe { &*plan.cast::<Plan>() }.len() }

unsafe extern "C" fn slot(plan: *const c_void, index: usize) -> Slot {
    let plan = unsafe { &*plan.cast::<Plan>() };
    plan[index].slot
}

unsafe extern "C" fn same_type(plan: *const c_void, index: usize, result: *const Ty) -> u32 {
    u32::from(unsafe { &*plan.cast::<Plan>() }[index].result == *unsafe { &*result })
}

unsafe extern "C" fn append(plan: *mut c_void, slot: Slot, result: *const Ty) -> usize {
    let plan = unsafe { &mut *plan.cast::<Plan>() };
    let index = plan.len();
    plan.push(TaskCompletionParameter { slot, result: unsafe { &*result }.clone() });
    index
}

pub(in crate::check) fn task_completion_parameters<'a>(
    parameters: impl IntoIterator<Item = (usize, Option<&'a Ty>)>,
    start: usize,
) -> (usize, Vec<TaskCompletionParameter>) {
    let mut plan = Vec::new();
    let mut input = start;
    for (argument, (width, ty)) in parameters.into_iter().enumerate() {
        input = unsafe { plan_input(ptr::from_mut(&mut plan).cast(), TypeRef::from(ty), input, argument, width, HOST) };
    }
    (input, plan)
}

pub(in crate::check) fn task_argument_slots(argument: &CallArgument) -> (usize, Option<&Ty>) {
    match argument {
        CallArgument::Value { ty, .. } => (ty.components.len(), Some(ty)),
        CallArgument::Reference { .. } => (2, None),
    }
}
