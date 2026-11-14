//! Native storage operations for the synchronous B source-liveness producer.
use super::*;
use crate::syntax::{wire, SourceView};
use std::ffi::c_void;
type Address = *mut c_void;

#[derive(Clone, Copy)]
#[repr(C)]
struct Bytes { data: *const u8, length: u64 }
impl Bytes {
    fn from_str(value: &str) -> Self { Self { data: value.as_ptr(), length: value.len() as u64 } }
    unsafe fn text<'a>(self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data, self.length as usize)) }
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct PathView { path: *const Path, count: u64 }
#[derive(Clone, Copy)]
#[repr(C)]
struct LocationView { name: Bytes, path: PathView }
#[repr(C)]
struct PathStep { field: Bytes, kind: u32, known: u32, index: u32 }
#[repr(C)]
struct Row { key: Address, value: Address, count: u64, present: u32 }
type StepReader = unsafe extern "C" fn(*const Path, u64) -> PathStep;
#[repr(C)]
struct Host {
    fresh: unsafe extern "C" fn() -> Address,
    copy: unsafe extern "C" fn(*const Live) -> Address,
    release: unsafe extern "C" fn(Address),
    equal: unsafe extern "C" fn(*const Live, *const Live) -> bool,
    make_location: unsafe extern "C" fn(Bytes) -> Address,
    field: unsafe extern "C" fn(*mut Location, Bytes),
    index: unsafe extern "C" fn(*mut Location, wire::Optional<u32>),
    drop_location: unsafe extern "C" fn(Address),
    read: unsafe extern "C" fn(*mut Live, Address),
    retain: unsafe extern "C" fn(*mut Live, *const Location, u32),
    entry: unsafe extern "C" fn(*mut Live, *const Location, *const BTreeSet<Path>) -> Address,
    iterate: unsafe extern "C" fn(Address, u32) -> Address,
    next: unsafe extern "C" fn(*mut Iter) -> Row,
    drop_iterator: unsafe extern "C" fn(Address),
    make_paths: unsafe extern "C" fn() -> Address,
    insert_path: unsafe extern "C" fn(*mut BTreeSet<Path>, *const Path),
    replace_paths: unsafe extern "C" fn(*mut BTreeSet<Path>, Address),
    location: unsafe extern "C" fn(*const Location) -> LocationView,
    step: StepReader,
    point_invocation: unsafe extern "C" fn(Address, wire::Span) -> Address,
    put_point: unsafe extern "C" fn(Address, wire::Span, Address, Address, Address),
    resolve: unsafe extern "C" fn(Address, Bytes) -> wire::Optional<wire::SignatureNode>,
    literal: unsafe extern "C" fn(Address, wire::ExprRef) -> wire::Optional<u32>,
    finish: unsafe extern "C" fn(Address, Address, Address),
}
unsafe extern "C" {
    #[link_name = "bootstrap.view_liveness::run"]
    fn produce(function: wire::SignatureNode, host: Host, context: Address);
    #[link_name = "bootstrap.view_liveness::retain_action"]
    fn retain_action(location: LocationView, written: LocationView, name_only: u32, step: StepReader) -> u32;
}
struct Context<'a> {
    world: &'a World,
    instance: &'a crate::generics::Instance,
    source: SourceView<'a>,
    state: State,
}
pub(super) fn run(world: &World, instance: &crate::generics::Instance) -> State {
    let function = &instance.definition.function;
    let mut context = Context { world, instance, source: function.source_view(), state: State::default() };
    let host = Host {
        fresh, copy, release, equal, make_location, field, index, drop_location,
        read, retain, entry, iterate, next, drop_iterator, make_paths, insert_path,
        replace_paths, location, step, point_invocation, put_point, resolve, literal, finish,
    };
    // World and the original syntax owner remain borrowed until this call ends.
    // B transfers each owned Live into the one State or releases it before return.
    unsafe { produce(function.signature_node(), host, (&mut context as *mut Context<'_>).cast()); }
    context.state
}
fn own<T>(value: T) -> Address { Box::into_raw(Box::new(value)).cast() }
unsafe fn take<T>(value: Address) -> T { unsafe { *Box::from_raw(value.cast::<T>()) } }
unsafe extern "C" fn fresh() -> Address { own(Live::default()) }
unsafe extern "C" fn copy(live: *const Live) -> Address { own(unsafe { (*live).clone() }) }
unsafe extern "C" fn release(live: Address) { drop(unsafe { Box::from_raw(live.cast::<Live>()) }); }
unsafe extern "C" fn equal(a: *const Live, b: *const Live) -> bool { unsafe { *a == *b } }
unsafe extern "C" fn make_location(name: Bytes) -> Address {
    own(Location { name: unsafe { name.text() }.to_owned(), path: Vec::new() })
}
unsafe extern "C" fn field(location: *mut Location, field: Bytes) {
    unsafe { (*location).path.push(Step::Field(field.text().to_owned())); }
}
unsafe extern "C" fn index(location: *mut Location, index: wire::Optional<u32>) {
    unsafe { (*location).path.push(Step::Index((index.present != 0).then_some(index.value))); }
}
unsafe extern "C" fn drop_location(location: Address) { drop(unsafe { Box::from_raw(location.cast::<Location>()) }); }
unsafe extern "C" fn read(live: *mut Live, location: Address) {
    unsafe { (*live).0.insert(take::<Location>(location), BTreeSet::new()); }
}
unsafe extern "C" fn location(value: *const Location) -> LocationView {
    let value = unsafe { &*value };
    LocationView { name: Bytes::from_str(&value.name), path: PathView { path: &value.path, count: value.path.len() as u64 } }
}
unsafe extern "C" fn step(path: *const Path, index: u64) -> PathStep {
    match unsafe { &(&*path)[index as usize] } {
        Step::Field(name) => PathStep { field: Bytes::from_str(name), kind: 0, known: 0, index: 0 },
        Step::Index(value) => PathStep { field: Bytes::from_str(""), kind: 1, known: u32::from(value.is_some()), index: value.unwrap_or(0) },
    }
}
unsafe extern "C" fn retain(live: *mut Live, written: *const Location, name_only: u32) {
    let written = unsafe { &*written };
    let view = unsafe { location(written) };
    unsafe { &mut *live }.0.retain(|key, excluded| {
        let action = unsafe { retain_action(location(key), view, name_only, step) };
        if action == 2 { excluded.insert(written.path.clone()); }
        action != 0
    });
}
unsafe extern "C" fn entry(live: *mut Live, key: *const Location, paths: *const BTreeSet<Path>) -> Address {
    use std::collections::btree_map::Entry;
    match unsafe { &mut *live }.0.entry(unsafe { (*key).clone() }) {
        Entry::Occupied(entry) => (entry.into_mut() as *mut BTreeSet<Path>).cast(),
        Entry::Vacant(entry) => { entry.insert(unsafe { (*paths).clone() }); std::ptr::null_mut() }
    }
}
// The erased iterators borrow the original maps/sets. B drops each iterator
// before replacing its source set; map traversals never mutate that map's keys.
enum Iter {
    Live(std::collections::btree_map::Iter<'static, Location, BTreeSet<Path>>),
    Paths(std::collections::btree_set::Iter<'static, Path>),
}
unsafe extern "C" fn iterate(value: Address, kind: u32) -> Address {
    if kind == 0 { own(Iter::Live(unsafe { (&*value.cast::<Live>()).0.iter() })) }
    else { own(Iter::Paths(unsafe { (&*value.cast::<BTreeSet<Path>>()).iter() })) }
}
unsafe extern "C" fn next(iterator: *mut Iter) -> Row {
    let mut row = Row { key: std::ptr::null_mut(), value: std::ptr::null_mut(), count: 0, present: 0 };
    match unsafe { &mut *iterator } {
        Iter::Live(iter) => if let Some((key, paths)) = iter.next() {
            row = Row { key: (key as *const Location).cast_mut().cast(), value: (paths as *const BTreeSet<Path>).cast_mut().cast(), count: 0, present: 1 };
        },
        Iter::Paths(iter) => if let Some(path) = iter.next() {
            row = Row { key: (path as *const Path).cast_mut().cast(), value: std::ptr::null_mut(), count: path.len() as u64, present: 1 };
        },
    }
    row
}
unsafe extern "C" fn drop_iterator(iterator: Address) { drop(unsafe { Box::from_raw(iterator.cast::<Iter>()) }); }
unsafe extern "C" fn make_paths() -> Address { own(BTreeSet::<Path>::new()) }
unsafe extern "C" fn insert_path(paths: *mut BTreeSet<Path>, path: *const Path) {
    unsafe { (*paths).insert((*path).clone()); }
}
unsafe extern "C" fn replace_paths(paths: *mut BTreeSet<Path>, value: Address) {
    unsafe { *paths = take(value); }
}
unsafe extern "C" fn point_invocation(context: Address, span: wire::Span) -> Address {
    let context = unsafe { &*context.cast::<Context<'_>>() };
    context.state.points.get(&(span.start as usize, span.end as usize))
        .and_then(|point| point.invocation.clone()).map_or(std::ptr::null_mut(), own)
}
unsafe extern "C" fn put_point(context: Address, span: wire::Span, before: Address, after: Address, invocation: Address) {
    let context = unsafe { &mut *context.cast::<Context<'_>>() };
    context.state.points.insert((span.start as usize, span.end as usize), Point {
        before: unsafe { take(before) }, after: unsafe { take(after) },
        invocation: if invocation.is_null() { None } else { Some(unsafe { take(invocation) }) },
    });
}
unsafe extern "C" fn resolve(context: Address, name: Bytes) -> wire::Optional<wire::SignatureNode> {
    let context = unsafe { &*context.cast::<Context<'_>>() };
    let definition = &context.instance.definition;
    let function = context.world.resolve(unsafe { name.text() }, &definition.module, &definition.imports, true)
        .unique().and_then(|key| context.world.functions.get(&key));
    match function {
        Some(function) => wire::Optional::some(function.function.signature_node()),
        None => wire::Optional { present: 0, value: definition.function.signature_node() },
    }
}
unsafe extern "C" fn literal(context: Address, expression: wire::ExprRef) -> wire::Optional<u32> {
    let context = unsafe { &*context.cast::<Context<'_>>() };
    let expression: crate::syntax::Expr = context.source.node(expression.offset);
    let value = match expression.kind() {
        ExprKind::StaticU32(value) => Some(value),
        ExprKind::Name(name) => context.instance.bindings.values.get(name).copied(),
        ExprKind::Number(text) => number(&context.instance.definition.source, expression.span(), text).ok()
            .and_then(|(ty, value)| (ty == Ty::scalar(Scalar::U32)).then_some(value as u32)),
        _ => None,
    };
    value.map_or_else(wire::Optional::none, wire::Optional::some)
}
unsafe extern "C" fn finish(context: Address, current: Address, outputs: Address) {
    let context = unsafe { &mut *context.cast::<Context<'_>>() };
    context.state.current = unsafe { take(current) };
    context.state.outputs = unsafe { take(outputs) };
}
