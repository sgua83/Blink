//! Original return and call fields remain borrowed for one synchronous B query.
use super::*;
use std::{ffi::c_void, ptr};
type Address = *mut c_void;
type Arguments = BTreeMap<ValueIdentity, bounds::Proof>;
fn address<T>(value: &T) -> Address { ptr::from_ref(value).cast_mut().cast() }
#[derive(Clone, Copy)]
#[repr(C)]
struct Name { data: Address, length: u64 }
fn name(value: &str) -> Name { Name { data: value.as_ptr().cast_mut().cast(), length: value.len() as u64 } }
#[repr(C)]
struct ModelCall { function: Name, memory: usize, arguments: Address }
#[repr(C)]
struct NativeCall { defined: u32, function: Name, memory_present: u32, memory: usize, regs: Address, count: usize }
#[repr(C)]
struct ReturnValue { node: Address, count: usize }
fn value(value: &Value) -> ReturnValue { ReturnValue { node: address(value), count: value.regs.len() } }
fn value_mut(value: &mut Value) -> ReturnValue { ReturnValue { count: value.regs.len(), node: ptr::from_mut(value).cast() } }
#[repr(C)]
struct Forward { component: usize, arguments: Address }
#[repr(C)]
struct Domain { original: Address, kind: u32, origin: Address, origin_kind: u32 }
#[derive(Clone, Copy)]
#[repr(C)]
struct Host {
    model: unsafe extern "C" fn(Address, usize) -> ModelCall,
    remember_argument: unsafe extern "C" fn(Address, usize, Address),
    calls: unsafe extern "C" fn(Address),
    arguments: unsafe extern "C" fn(Address, usize) -> Address,
    remember_forward: unsafe extern "C" fn(Address, usize, usize, Address),
    exit: unsafe extern "C" fn(Address, usize) -> ReturnValue,
    reg: unsafe extern "C" fn(Address, usize) -> usize,
    forwarded: unsafe extern "C" fn(Address, usize) -> Forward,
    domains: unsafe extern "C" fn(Address, Address, usize),
    contains_domain: unsafe extern "C" fn(Address, usize, Address) -> u32,
    origin: unsafe extern "C" fn(Address, Address, usize) -> Address,
    same_origin: unsafe extern "C" fn(Address, Address) -> u32,
    inherit: unsafe extern "C" fn(Address, usize, Address),
    apply: unsafe extern "C" fn(Address),
    inherited: unsafe extern "C" fn(Address, usize) -> Address,
    mark_domains: unsafe extern "C" fn(Address, usize, Address),
}
struct Query<'a> {
    name: Name,
    models: &'a [view_contracts::Call],
    code: *mut Inst,
    code_len: usize,
    exits: *mut ReturnExit,
    exit_len: usize,
    arguments: BTreeMap<usize, Address>,
    forwarded: BTreeMap<Reg, (usize, Address)>,
    inherited: BTreeMap<Reg, Domains>,
}
unsafe extern "C" {
    #[link_name = "bootstrap.function_body::recursive_ranges"]
    fn source_ranges(context: Address, function: Name, models: usize, exits: usize, host: Host);
    #[link_name = "bootstrap.function_body::recursive_call"]
    fn source_call(context: Address, function: Name, call: NativeCall, host: Host);
    #[link_name = "bootstrap.function_body::recursive_domain"]
    fn source_domain(context: Address, component: usize, domain: Domain, exits: usize, host: Host);
    #[link_name = "bootstrap.function_body::recursive_apply"]
    fn source_apply(context: Address, value: ReturnValue, host: Host);
}
unsafe extern "C" fn model(context: Address, at: usize) -> ModelCall {
    let call = &unsafe { &*context.cast::<Query<'_>>() }.models[at];
    ModelCall { function: name(&call.function), memory: call.memory, arguments: address(&call.arguments) }
}
unsafe extern "C" fn remember_argument(context: Address, memory: usize, arguments: Address) {
    unsafe { &mut *context.cast::<Query<'_>>() }.arguments.insert(memory, arguments);
}
unsafe extern "C" fn calls(context: Address) {
    let (function_name, code, count) = { let query = unsafe { &*context.cast::<Query<'_>>() }; (query.name, query.code, query.code_len) };
    native_function::calls(unsafe { std::slice::from_raw_parts_mut(code, count) }, &mut |call| {
        let (defined, function) = match &call.target {
            ir::CallTarget::Defined(function) => (1, name(function)),
            _ => (0, Name { data: ptr::null_mut(), length: 0 }),
        };
        let fields = NativeCall { defined, function, memory_present: u32::from(call.memory_use.is_some()),
            memory: call.memory_use.unwrap_or(0), regs: call.values.as_ptr().cast_mut().cast(), count: call.values.len() };
        unsafe { source_call(context, function_name, fields, HOST); }
    });
}
unsafe extern "C" fn arguments(context: Address, memory: usize) -> Address {
    unsafe { &*context.cast::<Query<'_>>() }.arguments.get(&memory).copied().unwrap_or(ptr::null_mut())
}
unsafe extern "C" fn remember_forward(context: Address, reg: usize, component: usize, arguments: Address) {
    unsafe { &mut *context.cast::<Query<'_>>() }.forwarded.insert(reg, (component, arguments));
}
unsafe extern "C" fn exit(context: Address, at: usize) -> ReturnValue {
    let exits = unsafe { &*context.cast::<Query<'_>>() }.exits;
    unsafe { &*exits.add(at) }.value.as_ref().and_then(Returned::view_value)
        .map_or(ReturnValue { node: ptr::null_mut(), count: 0 }, value)
}
unsafe extern "C" fn reg(value: Address, component: usize) -> usize { unsafe { &*value.cast::<Value>() }.regs[component] }
unsafe extern "C" fn forwarded(context: Address, reg: usize) -> Forward {
    unsafe { &*context.cast::<Query<'_>>() }.forwarded.get(&reg)
        .map_or(Forward { component: 0, arguments: ptr::null_mut() }, |&(component, arguments)| Forward { component, arguments })
}
unsafe extern "C" fn domains(context: Address, value: Address, component: usize) {
    let exits = unsafe { &*context.cast::<Query<'_>>() }.exit_len;
    for domain in unsafe { &*value.cast::<Value>() }.proof.domains(component).iter() {
        let mut fields = Domain { original: address(domain), kind: 0, origin: ptr::null_mut(), origin_kind: 0 };
        if let IndexDomain::CommonRange(origin, ..) = domain {
            fields.kind = 1; fields.origin = address(origin);
            fields.origin_kind = u32::from(matches!(origin, ValueIdentity::DatasetContent(..)));
        }
        unsafe { source_domain(context, component, fields, exits, HOST); }
    }
}
unsafe extern "C" fn contains_domain(value: Address, component: usize, domain: Address) -> u32 {
    u32::from(unsafe { &*value.cast::<Value>() }.proof.domains(component).contains(unsafe { &*domain.cast::<IndexDomain>() }))
}
unsafe extern "C" fn origin(arguments: Address, id: Address, at: usize) -> Address {
    unsafe { &*arguments.cast::<Arguments>() }.get(unsafe { &*id.cast::<ValueIdentity>() })
        .and_then(|proof| proof.origins.first()).and_then(|origins| origins.get(at)).map_or(ptr::null_mut(), address)
}
unsafe extern "C" fn same_origin(left: Address, right: Address) -> u32 {
    u32::from(unsafe { *left.cast::<ValueIdentity>() == *right.cast::<ValueIdentity>() })
}
unsafe extern "C" fn remember_domain(context: Address, reg: usize, domain: Address) {
    unsafe { &mut *context.cast::<Query<'_>>() }.inherited.entry(reg).or_default().insert(unsafe { *domain.cast::<IndexDomain>() });
}
unsafe extern "C" fn apply(context: Address) {
    let (exits, count) = { let query = unsafe { &*context.cast::<Query<'_>>() }; (query.exits, query.exit_len) };
    for exit in unsafe { std::slice::from_raw_parts_mut(exits, count) } {
        if let Some(returned) = &mut exit.value {
            returned.visit_values_mut(&mut |actual| unsafe { source_apply(context, value_mut(actual), HOST) });
        }
    }
}
unsafe extern "C" fn inherited(context: Address, reg: usize) -> Address {
    unsafe { &*context.cast::<Query<'_>>() }.inherited.get(&reg).map_or(ptr::null_mut(), address)
}
unsafe extern "C" fn mark_domains(value: Address, component: usize, domains: Address) {
    let value = unsafe { &mut *value.cast::<Value>() };
    for domain in unsafe { &*domains.cast::<Domains>() } { value.proof.mark_domain(component, *domain); }
}
const HOST: Host = Host { model, remember_argument, calls, arguments, remember_forward, exit, reg, forwarded,
    domains, contains_domain, origin, same_origin, inherit: remember_domain, apply, inherited, mark_domains };
pub(super) fn inherit(function: &str, models: &[view_contracts::Call], code: &mut [Inst], exits: &mut [ReturnExit]) {
    let mut query = Query { name: name(function), models, code: code.as_mut_ptr(), code_len: code.len(),
        exits: exits.as_mut_ptr(), exit_len: exits.len(), arguments: BTreeMap::new(), forwarded: BTreeMap::new(), inherited: BTreeMap::new() };
    unsafe { source_ranges(ptr::from_mut(&mut query).cast(), query.name, models.len(), exits.len(), HOST); }
}
