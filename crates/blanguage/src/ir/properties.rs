//! Synchronously borrow the original Ty containers for B layout queries.
use super::Ty;
use std::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Query { Dataset, Resources, Cleanup, Arrays, Views, Borrows }
#[derive(Clone, Copy)]
#[repr(u32)]
enum Child { Variant, Array, Field }
#[repr(C)]
struct Node {
    task: u32, cleanup: u32, dataset: u32, view: u32, array: u32, address: u32,
    variants: usize, fields: usize, count: u32, alignment: u32,
}
#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(*const Ty) -> Node,
    child: unsafe extern "C" fn(*const Ty, Child, usize) -> *const Ty,
}
#[repr(C)]
struct Part { value: *const Ty, offset: u32 }
#[repr(C)]
struct FieldHost {
    types: Host,
    part: unsafe extern "C" fn(*const Ty, usize) -> Part,
}
#[repr(C)]
struct Text { bytes: *const u8, length: usize }
#[repr(C)]
struct PathEntry { name: Text, part: Part, discriminant: u32 }
#[repr(C)]
struct PathHost {
    fields: FieldHost,
    entry: unsafe extern "C" fn(*const Ty, Child, usize) -> PathEntry,
    tag: unsafe extern "C" fn(*const Ty, u32) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::contains"]
    fn b_contains(value: *const Ty, query: Query, host: Host) -> u32;
    #[link_name = "bootstrap.type_layout::zero_valid"]
    fn b_zero_valid(value: *const Ty, host: Host,
        discriminant: unsafe extern "C" fn(*const Ty, usize) -> u32) -> u32;
    #[link_name = "bootstrap.type_layout::borrow_fields"]
    fn b_borrow_fields(value: *const Ty, base: u32, host: FieldHost, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u32, *const Ty));
    #[link_name = "bootstrap.type_layout::task_output_owners"]
    fn b_task_output_owners(value: *const Ty, base: u32, host: FieldHost, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u32, *const Ty));
    #[link_name = "bootstrap.type_layout::field"]
    fn b_field(value: *const Ty, path: Text, words: *const u32, word_count: usize,
        active: u32, host: PathHost) -> Part;
}

pub(super) fn contains(value: &Ty, query: Query) -> bool {
    unsafe { b_contains(value, query, Host { inspect, child }) != 0 }
}
unsafe extern "C" fn inspect(value: *const Ty) -> Node {
    let ty = unsafe { &*value };
    Node {
        task: u32::from(ty.task_result().is_some()),
        cleanup: u32::from(ty.cleanup.is_some()),
        dataset: u32::from(ty.dataset.is_some()),
        view: u32::from(ty.view.is_some()),
        array: u32::from(ty.array.is_some()),
        address: u32::from(ty.is_address()),
        variants: ty.variants.len(), fields: ty.fields.len(),
        count: if ty.array.is_some() { ty.values[0] } else { 0 }, alignment: ty.align,
    }
}
unsafe extern "C" fn child(value: *const Ty, kind: Child, index: usize) -> *const Ty {
    let ty = unsafe { &*value };
    ptr::from_ref(match kind {
        Child::Variant => ty.variants[index].ty.as_ref(),
        Child::Array => ty.array.as_deref().unwrap(),
        Child::Field => ty.fields[index].2.as_ref(),
    })
}

pub(super) fn borrows(value: &Ty) -> Vec<(u32, &Ty)> {
    let mut output = Vec::new();
    visit_borrows(value, |offset, value| output.push((offset, value)));
    output
}

pub(super) fn visit_borrows<'a, F: FnMut(u32, &'a Ty)>(value: &'a Ty, mut visit: F) {
    unsafe extern "C" fn dispatch<'a, F: FnMut(u32, &'a Ty)>(
        context: *mut c_void, offset: u32, value: *const Ty,
    ) {
        // B visits only the original value and layout children during this call.
        unsafe { (&mut *context.cast::<F>())(offset, &*value) };
    }
    unsafe {
        b_borrow_fields(value, 0, FieldHost { types: Host { inspect, child }, part },
            ptr::from_mut(&mut visit).cast(), dispatch::<F>);
    }
}

pub(super) fn task_output_owners(value: &Ty) -> Vec<(u32, &Ty)> {
    let mut output: Vec<(u32, &Ty)> = Vec::new();
    unsafe {
        b_task_output_owners(value, 0, FieldHost { types: Host { inspect, child }, part },
            ptr::from_mut(&mut output).cast(), emit);
    }
    output
}
unsafe extern "C" fn part(value: *const Ty, index: usize) -> Part {
    let (offset, value) = unsafe { &*value }.aggregate_part(index);
    Part { value: ptr::from_ref(value), offset }
}
// B emits only the original value or its owned layout children. The result
// remains borrowed from the input Ty; no reference escapes that ownership.
unsafe extern "C" fn emit(context: *mut c_void, offset: u32, value: *const Ty) {
    let output = unsafe { &mut *context.cast::<Vec<(u32, &Ty)>>() };
    output.push((offset, unsafe { &*value }));
}

pub(super) fn zero_valid(value: &Ty) -> bool {
    unsafe { b_zero_valid(value, Host { inspect, child }, discriminant) != 0 }
}
unsafe extern "C" fn discriminant(value: *const Ty, index: usize) -> u32 {
    unsafe { &*value }.variants[index].discriminant
}

pub(super) fn field<'a>(value: &'a Ty, path: &str, words: Option<&[u32]>) -> Option<(u32, &'a Ty)> {
    let (words, word_count, active) = match words {
        Some(words) => (words.as_ptr(), words.len(), 1),
        None => (ptr::null(), 0, 0),
    };
    let found = unsafe {
        b_field(value, Text { bytes: path.as_ptr(), length: path.len() }, words, word_count,
            active, PathHost { fields: FieldHost { types: Host { inspect, child }, part }, entry, tag })
    };
    if found.value.is_null() { None } else { Some((found.offset, unsafe { &*found.value })) }
}
unsafe extern "C" fn entry(value: *const Ty, kind: Child, index: usize) -> PathEntry {
    let ty = unsafe { &*value };
    let (name, offset, value, discriminant) = if matches!(kind, Child::Variant) {
        let variant = &ty.variants[index];
        (&variant.name, ty.variant_payload_offset(), variant.ty.as_ref(), variant.discriminant)
    } else {
        let (name, offset, field) = &ty.fields[index];
        (name, *offset, field.as_ref(), 0)
    };
    PathEntry { name: Text { bytes: name.as_ptr(), length: name.len() },
        part: Part { value: ptr::from_ref(value), offset }, discriminant }
}
unsafe extern "C" fn tag(value: *const Ty, bits: u32) -> u32 {
    unsafe { &*value }.variant_tag(bits)
}

#[repr(u32)]
enum MemberKind {
    Missing,
    Variant,
    HiddenView,
    Length,
    Declared,
    Swizzle,
    UnknownComponent,
    ComponentBounds,
}
#[repr(C)]
struct MemberSelection {
    kind: MemberKind,
    count: u32,
    part: Part,
    lanes: [u32; 4],
}
#[repr(C)]
struct MemberHost {
    inspect: unsafe extern "C" fn(*const Ty) -> Node,
    entry: unsafe extern "C" fn(*const Ty, Child, usize) -> PathEntry,
    swizzle_width: unsafe extern "C" fn(*const Ty) -> usize,
}

pub(crate) enum SourceMember<'a> {
    Missing,
    Variant,
    HiddenView,
    Length,
    Declared { ty: &'a Ty, offset: u32 },
    Swizzle { lanes: [u32; 4], count: u32 },
    UnknownComponent,
    ComponentBounds,
}
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::source_member"]
    fn b_source_member(value: *const Ty, name: Text, host: MemberHost) -> MemberSelection;
}
unsafe extern "C" fn swizzle_width(value: *const Ty) -> usize {
    let ty = unsafe { &*value };
    if ty.is_handle() { 0 } else { ty.components.len() }
}
pub(crate) fn source_member<'a>(value: &'a Ty, name: &str) -> SourceMember<'a> {
    let selected = unsafe {
        b_source_member(value, Text { bytes: name.as_ptr(), length: name.len() },
            MemberHost { inspect, entry, swizzle_width })
    };
    match selected.kind {
        MemberKind::Missing => SourceMember::Missing,
        MemberKind::Variant => SourceMember::Variant,
        MemberKind::HiddenView => SourceMember::HiddenView,
        MemberKind::Length => SourceMember::Length,
        MemberKind::Declared => SourceMember::Declared {
            ty: unsafe { &*selected.part.value }, offset: selected.part.offset,
        },
        MemberKind::Swizzle => SourceMember::Swizzle { lanes: selected.lanes, count: selected.count },
        MemberKind::UnknownComponent => SourceMember::UnknownComponent,
        MemberKind::ComponentBounds => SourceMember::ComponentBounds,
    }
}
