use crate::{Declarations, ast::*, build::Declaration, syntax::wire};
use std::{cell::RefCell, collections::BTreeSet, ffi::c_void};

/// Participation inferred from definitions, not a source function kind or runtime permit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Requirement {
    #[default]
    Invocation,
    Domain,
    Host,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct NameBytes { bytes: *const u8, length: u64 }
impl NameBytes {
    fn new(name: &str) -> Self { Self { bytes: name.as_ptr(), length: name.len() as u64 } }
    unsafe fn text<'a>(self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.length as usize)) }
    }
}

#[repr(C)]
struct FunctionInput { key: NameBytes, source: wire::SignatureNode, compiled: u32, requirement: u32 }
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct BodyState { domain: u32, local: u32, host: u32, requirement: u32 }
#[repr(C)]
struct CallEdge { caller: u64, callee: u64, statement: u64 }

struct Lookup<'a> {
    world: &'a Declarations,
    functions: &'a [FunctionInput],
    inputs: Option<&'a RefCell<BTreeSet<Declaration>>>,
    calls: Vec<CallEdge>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.participation::scan"]
    fn scan(functions: *const FunctionInput, count: u64, states: *mut BodyState, context: *mut c_void,
        lookup: unsafe extern "C" fn(*mut c_void, u64, NameBytes, u32) -> u32);
    #[link_name = "bootstrap.participation::less_call"]
    fn less_call(left: *const c_void, right: *const c_void, context: *const c_void) -> u32;
    #[link_name = "bootstrap.participation::solve"]
    fn solve(functions: *const FunctionInput, count: u64, states: *mut BodyState, calls: *mut CallEdge, call_count: u64,
        less: unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> u32);
}

// This adapter asks the original World and records its original dependency. B owns
// the source traversal and participation rules; this callback retains no B address.
unsafe extern "C" fn lookup(context: *mut c_void, caller: u64, name: NameBytes, statement: u32) -> u32 {
    let lookup = unsafe { &mut *context.cast::<Lookup<'_>>() };
    let key = unsafe { lookup.functions[caller as usize].key.text() };
    let d = &lookup.world.functions[key];
    let name = unsafe { name.text() };
    if let Some(inputs) = lookup.inputs {
        inputs.borrow_mut().insert(Declaration::RecordName {
            name: name.into(), module: d.module.clone(), imports: d.imports.to_vec(),
        });
    }
    if !lookup.world.resolve(name, &d.module, &d.imports, false).is_missing() { return 1; }
    if let Some(inputs) = lookup.inputs {
        inputs.borrow_mut().insert(Declaration::ValueName {
            name: name.into(), module: d.module.clone(), imports: d.imports.to_vec(),
        });
    }
    match lookup.world.resolve(name, &d.module, &d.imports, true) {
        crate::frontend::NameResolution::Unique(key) => {
            // Both views come from the same sorted original function map.
            let callee = lookup.functions.partition_point(|function| unsafe { function.key.text() } < key.as_str());
            lookup.calls.push(CallEdge { caller, callee: callee as u64, statement: u64::from(statement) });
            0
        }
        crate::frontend::NameResolution::Missing => if crate::check::stage_intrinsic(name) { 4 } else { 2 },
        crate::frontend::NameResolution::Ambiguous => 3,
    }
}

pub(crate) fn infer(world: &mut Declarations, inputs: Option<&RefCell<BTreeSet<Declaration>>>) {
    let functions = world.functions.iter().map(|(key, definition)| FunctionInput {
        key: NameBytes::new(key), source: definition.function.signature_node(),
        compiled: u32::from(definition.compiled.is_some()),
        requirement: match definition.execution { Requirement::Invocation => 0, Requirement::Domain => 1, Requirement::Host => 2 },
    }).collect::<Vec<_>>();
    let mut states = vec![BodyState::default(); functions.len()];
    let mut lookup = Lookup { world, functions: &functions, inputs, calls: Vec::new() };
    unsafe { scan(functions.as_ptr(), functions.len() as u64, states.as_mut_ptr(),
        (&mut lookup as *mut Lookup<'_>).cast(), self::lookup) };
    unsafe { solve(functions.as_ptr(), functions.len() as u64, states.as_mut_ptr(),
        lookup.calls.as_mut_ptr(), lookup.calls.len() as u64, less_call) };
    drop(lookup);
    drop(functions);
    // All source and key borrows finish before the original definitions are updated.
    for (definition, state) in world.functions.values_mut().zip(states) {
        std::sync::Arc::make_mut(definition).execution = match state.requirement {
            2 => Requirement::Host, 1 => Requirement::Domain, _ => Requirement::Invocation,
        };
    }
}
