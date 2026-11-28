//! Synchronously borrow the original Ty containers for B layout queries.
use super::{Scalar, Ty, VectorLayout};
use crate::builtin_types::Builtin;
use std::{ffi::c_void, ptr};

fn address(value: &Ty) -> *const c_void { ptr::from_ref(value).cast() }

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Query { Dataset=0, Resources=1, Arrays=3, Views=4, Borrows=5 }
#[derive(Clone, Copy)]
#[repr(u32)]
#[allow(dead_code)] // B supplies the child kind.
enum Child { Variant, Array, Field }
#[repr(C)]
struct Node {
    task: u32, cleanup: u32, dataset: u32, view: u32, array: u32, address: u32,
    variants: usize, fields: usize, count: u32, alignment: u32,
}
#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(*const c_void) -> Node,
    child: unsafe extern "C" fn(*const c_void, Child, usize) -> *const c_void,
}
#[repr(C)]
struct NumericNode {
    name:*const u8,length:usize,function:u32,enumeration:u32,variants:usize,
    view:u32,array:u32,fields:usize,components:usize,
}
#[repr(C)]
struct NumericHost {
    types:Host,
    inspect:unsafe extern "C" fn(*const c_void)->NumericNode,
    scalar:unsafe extern "C" fn(*const c_void,usize)->u32,
}
#[repr(C)]
struct Part { value: *const c_void, offset: u32 }
#[cfg(feature = "compiler")]
#[repr(u32)]
pub(super) enum FieldSelection { Borrow, Address, Task }

#[repr(C)]
struct FieldHost {
    types: Host,
    part: unsafe extern "C" fn(*const c_void, usize) -> Part,
}

#[repr(C)]
#[cfg(feature = "compiler")]
struct DatasetPathsHost {
    types: Host,
    push: unsafe extern "C" fn(*mut c_void, usize),
    pop: unsafe extern "C" fn(*mut c_void),
    emit: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
struct Text { bytes: *const u8, length: usize }
#[repr(C)]
struct PathEntry { name: Text, part: Part, discriminant: u32 }
#[repr(C)]
struct PathHost {
    fields: FieldHost,
    entry: unsafe extern "C" fn(*const c_void, Child, usize) -> PathEntry,
    tag: unsafe extern "C" fn(*const c_void, u32) -> u32,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
#[cfg(feature = "compiler")]
pub(crate) enum StorageMode { Borrow, CleanupBorrow, CleanupRequirement }
#[repr(u32)]
#[cfg(feature = "compiler")]
pub(crate) enum StorageNeed { Absent, Present, Enter, Contract, Field, Variant, Array, Dataset }
#[repr(C)]
#[cfg(feature = "compiler")]
struct StorageCursor {
    next: usize,
    mode: StorageMode,
}
#[repr(C)]
#[cfg(feature = "compiler")]
pub(crate) struct StorageStep {
    pub(crate) index: usize,
    pub(crate) need: StorageNeed,
}

#[cfg(feature = "compiler")]
pub(crate) struct StorageWalk {
    cursor: StorageCursor,
    node: Node,
    empty_dataset: u32,
}

#[cfg(feature = "compiler")]
impl StorageWalk {
    pub(crate) fn new(ty: &Ty, mode: StorageMode) -> Self {
        Self {
            cursor: StorageCursor { next: 0, mode },
            node: unsafe { inspect(address(ty)) },
            empty_dataset: u32::from(ty.values.first() == Some(&0)),
        }
    }

    pub(crate) fn mode(&self) -> StorageMode {
        self.cursor.mode
    }

    pub(crate) fn step(&mut self, found: bool) -> StorageStep {
        unsafe { b_storage_step(&mut self.cursor, &self.node, self.empty_dataset, u32::from(found)) }
    }
}

#[repr(u32)]
#[cfg(feature = "compiler")]
pub(crate) enum ReferenceNeed {
    Done, Enter, Signature, Parameter, Result, Argument, Field, Variant, Array, View, Dataset,
}
#[repr(C)]
#[derive(Default)]
#[cfg(feature = "compiler")]
struct ReferenceCursor {
    next: usize,
    phase: u32,
}
#[repr(C)]
#[cfg(feature = "compiler")]
struct ReferenceShape {
    arguments: usize,
    fields: usize,
    variants: usize,
    parameters: usize,
    signature: u32,
    result: u32,
    array: u32,
    view: u32,
    dataset: u32,
}
#[repr(C)]
#[cfg(feature = "compiler")]
pub(crate) struct ReferenceStep {
    pub(crate) index: usize,
    pub(crate) need: ReferenceNeed,
}

#[cfg(feature = "compiler")]
pub(crate) struct ReferenceWalk {
    cursor: ReferenceCursor,
    shape: ReferenceShape,
}

#[cfg(feature = "compiler")]
impl ReferenceWalk {
    pub(crate) fn new(ty: &Ty) -> Self {
        Self {
            cursor: ReferenceCursor::default(),
            shape: ReferenceShape {
                arguments: ty.arguments.len(),
                fields: ty.fields.len(),
                variants: ty.variants.len(),
                parameters: 0,
                signature: u32::from(ty.function.is_some()),
                result: 0,
                array: u32::from(ty.array.is_some()),
                view: u32::from(ty.view.is_some()),
                dataset: u32::from(ty.dataset.is_some()),
            },
        }
    }

    pub(crate) fn signature(&mut self, parameters: usize, result: bool) {
        self.shape.parameters = parameters;
        self.shape.result = u32::from(result);
    }

    pub(crate) fn step(&mut self) -> ReferenceStep {
        unsafe { b_reference_step(&mut self.cursor, &self.shape) }
    }
}

unsafe extern "C" {
    #[link_name = "codec.type_layout::numeric"]
    fn b_numeric(value:*const c_void,host:NumericHost)->u32;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::reference_step"]
    fn b_reference_step(cursor: &mut ReferenceCursor, shape: &ReferenceShape) -> ReferenceStep;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::storage_step"]
    fn b_storage_step(cursor: &mut StorageCursor, node: &Node, empty_dataset: u32, found: u32) -> StorageStep;
    #[link_name = "codec.type_layout::contains"]
    fn b_contains(value: *const c_void, query: Query, host: Host) -> u32;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::zero_valid"]
    fn b_zero_valid(value: *const c_void, host: Host,
        discriminant: unsafe extern "C" fn(*const c_void, usize) -> u32) -> u32;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::visit_fields"]
    fn b_visit_fields(value: *const c_void, host: FieldHost, selection: FieldSelection, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u32, *const c_void));
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::select_snapshot_columns"]
    fn b_select_snapshot_columns(columns: *mut SnapshotColumn, count: usize, ranges: usize,
        write: u32, context: *const c_void, host: SnapshotColumnHost);
    #[link_name = "codec.type_layout::task_output_owners"]
    fn b_task_output_owners(value: *const c_void, base: u32, host: FieldHost, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u32, *const c_void));
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::dataset_count"]
    fn source_dataset_count(value: *const c_void, host: Host) -> usize;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::dataset_path"]
    fn source_dataset_path(
        value: *const c_void, ordinal: usize, host: Host, context: *mut c_void,
        push: unsafe extern "C" fn(*mut c_void, usize),
    ) -> u32;
    #[cfg(feature = "compiler")]
    #[link_name = "bootstrap.type_layout::dataset_paths"]
    fn source_dataset_paths(value: *const c_void, context: *mut c_void, host: DatasetPathsHost);
    #[link_name = "codec.type_layout::field"]
    fn b_field(value: *const c_void, path: Text, words: *const u32, word_count: usize,
        active: u32, host: PathHost) -> Part;
    #[link_name = "codec.type_layout::field_layout_at"]
    fn b_field_layout_at(value: *const c_void, offset: u32, expected: *const c_void, host: PathHost,
        equal: unsafe extern "C" fn(*const c_void, *const c_void) -> u32) -> *const c_void;
    #[link_name = "codec.type_layout::linear_positions"]
    fn linear_positions(stride: u32, count: u64, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u64, u32));
    #[link_name = "codec.type_layout::ray_hit_fields"]
    fn b_ray_hit_fields(context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, Text, u32, Builtin)) -> VectorLayout;
}

#[cfg(feature = "compiler")]
pub(crate) fn dataset_count(value: &Ty) -> usize {
    unsafe { source_dataset_count(address(value), Host { inspect, child }) }
}

#[cfg(feature = "compiler")]
pub(crate) fn dataset_path(value: &Ty, ordinal: usize) -> Option<Vec<usize>> {
    unsafe extern "C" fn push(out: *mut c_void, index: usize) {
        unsafe { &mut *out.cast::<Vec<usize>>() }.push(index);
    }
    let mut path = Vec::new();
    let found = unsafe {
        source_dataset_path(address(value), ordinal, Host { inspect, child }, ptr::from_mut(&mut path).cast(), push)
    };
    if found == 0 {
        return None;
    }
    // B appends only the chosen indices while returning from the leaf.
    path.reverse();
    Some(path)
}

#[cfg(feature = "compiler")]
pub(crate) fn dataset_paths(value: &Ty) -> Vec<Vec<usize>> {
    #[derive(Default)]
    struct Paths {
        path: Vec<usize>,
        output: Vec<Vec<usize>>,
    }
    unsafe extern "C" fn push(context: *mut c_void, index: usize) {
        unsafe { &mut *context.cast::<Paths>() }.path.push(index);
    }
    unsafe extern "C" fn pop(context: *mut c_void) {
        unsafe { &mut *context.cast::<Paths>() }.path.pop();
    }
    unsafe extern "C" fn emit(context: *mut c_void) {
        let paths = unsafe { &mut *context.cast::<Paths>() };
        paths.output.push(paths.path.clone());
    }

    let mut paths = Paths::default();
    unsafe {
        source_dataset_paths(address(value), ptr::from_mut(&mut paths).cast(), DatasetPathsHost {
            types: Host { inspect, child }, push, pop, emit,
        });
    }
    paths.output
}

pub(super) fn contains(value: &Ty, query: Query) -> bool {
    unsafe { b_contains(address(value), query, Host { inspect, child }) != 0 }
}
pub(super) fn numeric(value:&Ty)->bool {
    unsafe { b_numeric(address(value),NumericHost {types:Host {inspect,child},inspect:numeric_node,scalar:numeric_scalar})!=0 }
}
unsafe extern "C" fn numeric_node(value:*const c_void)->NumericNode {
    let ty=unsafe { &*value.cast::<Ty>() };
    NumericNode {name:ty.name.as_ptr(),length:ty.name.len(),function:u32::from(ty.function.is_some()),
        enumeration:u32::from(ty.enumeration),variants:ty.variants.len(),view:u32::from(ty.view.is_some()),
        array:u32::from(ty.array.is_some()),fields:ty.fields.len(),components:ty.components.len()}
}
unsafe extern "C" fn numeric_scalar(value:*const c_void,index:usize)->u32 {
    unsafe { &*value.cast::<Ty>() }.components[index].0 as u32
}
unsafe extern "C" fn inspect(value: *const c_void) -> Node {
    let ty = unsafe { &*value.cast::<Ty>() };
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
unsafe extern "C" fn child(value: *const c_void, kind: Child, index: usize) -> *const c_void {
    let ty = unsafe { &*value.cast::<Ty>() };
    address(match kind {
        Child::Variant => ty.variants[index].ty.as_ref(),
        Child::Array => ty.array.as_deref().unwrap(),
        Child::Field => ty.fields[index].2.as_ref(),
    })
}

#[cfg(feature = "compiler")]
pub(super) fn borrows(value: &Ty) -> Vec<(u32, &Ty)> {
    let mut output = Vec::new();
    visit_fields(value, FieldSelection::Borrow, |offset, value| output.push((offset, value)));
    output
}

#[cfg(feature = "compiler")]
pub(super) fn visit_fields<'a, F: FnMut(u32, &'a Ty)>(value: &'a Ty, selection: FieldSelection, mut visit: F) {
    unsafe extern "C" fn dispatch<'a, F: FnMut(u32, &'a Ty)>(
        context: *mut c_void, offset: u32, value: *const c_void,
    ) {
        // B visits only the original value and layout children during this call.
        unsafe { (&mut *context.cast::<F>())(offset, &*value.cast::<Ty>()) };
    }
    unsafe {
        b_visit_fields(address(value), FieldHost { types: Host { inspect, child }, part }, selection,
            ptr::from_mut(&mut visit).cast(), dispatch::<F>);
    }
}

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SnapshotColumn { Skip, Scalar, Address }

#[cfg(feature = "compiler")]
impl SnapshotColumn {
    pub(crate) fn captured(self) -> bool { self != Self::Skip }
    pub(crate) fn address(self) -> bool { self == Self::Address }
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct MetadataRange { start: u32, end: u32 }

#[cfg(feature = "compiler")]
#[repr(C)]
struct SnapshotColumnHost {
    word: unsafe extern "C" fn(*const c_void, usize) -> u32,
    metadata: unsafe extern "C" fn(*const c_void, usize) -> MetadataRange,
}

#[cfg(feature = "compiler")]
impl Ty {
    pub(crate) fn snapshot_columns(
        &self, count: usize, write: bool, ranges: &[std::ops::Range<u32>],
    ) -> Vec<SnapshotColumn> {
        struct Marks<'a> { ty: &'a Ty, columns: &'a mut [SnapshotColumn] }
        struct Source<'a> { ty: &'a Ty, ranges: &'a [std::ops::Range<u32>] }

        unsafe extern "C" fn mark(context: *mut c_void, word: u32, _: *const c_void) {
            let source = unsafe { &mut *context.cast::<Marks<'_>>() };
            let component = source.ty.components.iter().position(|(_, offset)| *offset == word).unwrap();
            if let Some(mark) = source.columns.get_mut(component) {
                *mark = SnapshotColumn::Address;
            }
        }
        unsafe extern "C" fn word(context: *const c_void, component: usize) -> u32 {
            unsafe { &*context.cast::<Source<'_>>() }.ty.components[component].1
        }
        unsafe extern "C" fn metadata(context: *const c_void, index: usize) -> MetadataRange {
            let range = &unsafe { &*context.cast::<Source<'_>>() }.ranges[index];
            MetadataRange { start: range.start, end: range.end }
        }

        let mut columns = vec![SnapshotColumn::Skip; count];
        unsafe {
            let mut marks = Marks { ty: self, columns: &mut columns };
            b_visit_fields(address(self), FieldHost { types: Host { inspect, child }, part },
                FieldSelection::Address, ptr::from_mut(&mut marks).cast(), mark);
            let source = Source { ty: self, ranges };
            b_select_snapshot_columns(columns.as_mut_ptr(), count, ranges.len(), u32::from(write),
                ptr::from_ref(&source).cast(), SnapshotColumnHost { word, metadata });
        }
        columns
    }
}

pub(super) fn task_output_owners(value: &Ty) -> Vec<(u32, &Ty)> {
    let mut output: Vec<(u32, &Ty)> = Vec::new();
    unsafe {
        b_task_output_owners(address(value), 0, FieldHost { types: Host { inspect, child }, part },
            ptr::from_mut(&mut output).cast(), emit);
    }
    output
}
unsafe extern "C" fn part(value: *const c_void, index: usize) -> Part {
    let (offset, value) = unsafe { &*value.cast::<Ty>() }.aggregate_part(index);
    Part { value: address(value), offset }
}
// B emits only the original value or its owned layout children. The result
// remains borrowed from the input Ty; no reference escapes that ownership.
unsafe extern "C" fn emit(context: *mut c_void, offset: u32, value: *const c_void) {
    let output = unsafe { &mut *context.cast::<Vec<(u32, &Ty)>>() };
    output.push((offset, unsafe { &*value.cast::<Ty>() }));
}

#[cfg(feature = "compiler")]
pub(super) fn zero_valid(value: &Ty) -> bool {
    unsafe { b_zero_valid(address(value), Host { inspect, child }, discriminant) != 0 }
}
#[cfg(feature = "compiler")]
unsafe extern "C" fn discriminant(value: *const c_void, index: usize) -> u32 {
    unsafe { &*value.cast::<Ty>() }.variants[index].discriminant
}

pub(super) fn field<'a>(value: &'a Ty, path: &str, words: Option<&[u32]>) -> Option<(u32, &'a Ty)> {
    let (words, word_count, active) = match words {
        Some(words) => (words.as_ptr(), words.len(), 1),
        None => (ptr::null(), 0, 0),
    };
    let found = unsafe {
        b_field(address(value), Text { bytes: path.as_ptr(), length: path.len() }, words, word_count,
            active, path_host())
    };
    if found.value.is_null() { None } else { Some((found.offset, unsafe { &*found.value.cast::<Ty>() })) }
}
unsafe extern "C" fn entry(value: *const c_void, kind: Child, index: usize) -> PathEntry {
    let ty = unsafe { &*value.cast::<Ty>() };
    let (name, offset, value, discriminant) = if matches!(kind, Child::Variant) {
        let variant = &ty.variants[index];
        (&variant.name, ty.variant_payload_offset(), variant.ty.as_ref(), variant.discriminant)
    } else {
        let (name, offset, field) = &ty.fields[index];
        (name, *offset, field.as_ref(), 0)
    };
    PathEntry { name: Text { bytes: name.as_ptr(), length: name.len() },
        part: Part { value: address(value), offset }, discriminant }
}
unsafe extern "C" fn tag(value: *const c_void, bits: u32) -> u32 {
    unsafe { &*value.cast::<Ty>() }.variant_tag(bits)
}

fn path_host() -> PathHost {
    PathHost { fields: FieldHost { types: Host { inspect, child }, part }, entry, tag }
}

pub(super) fn field_layout_at<'a>(value: &'a Ty, offset: u32, expected: &Ty) -> Option<&'a Ty> {
    unsafe extern "C" fn equal(left: *const c_void, right: *const c_void) -> u32 {
        u32::from(unsafe { *left.cast::<Ty>() == *right.cast::<Ty>() })
    }
    let found = unsafe { b_field_layout_at(address(value), offset, address(expected), path_host(), equal) };
    unsafe { found.cast::<Ty>().as_ref() }
}

pub(super) fn vector_slots(scalar: Scalar, count: usize) -> Vec<(Scalar, u32)> {
    struct Slots {
        scalar: Scalar,
        result: Vec<(Scalar, u32)>,
    }
    unsafe extern "C" fn append(context: *mut c_void, _: u64, offset: u32) {
        let slots = unsafe { &mut *context.cast::<Slots>() };
        slots.result.push((slots.scalar, offset));
    }

    let mut slots = Slots { scalar, result: Vec::with_capacity(count) };
    unsafe { linear_positions(scalar.size(), count as u64, ptr::from_mut(&mut slots).cast(), append) };
    slots.result
}

pub(super) fn matrix_fields(row: &Ty, count: usize, stride: u32) -> Vec<(String, u32, Box<Ty>)> {
    struct Fields<'a> {
        row: &'a Ty,
        result: Vec<(String, u32, Box<Ty>)>,
    }
    unsafe extern "C" fn append(context: *mut c_void, index: u64, offset: u32) {
        let fields = unsafe { &mut *context.cast::<Fields<'_>>() };
        fields.result.push((format!("r{index}"), offset, Box::new(fields.row.clone())));
    }

    let mut fields = Fields { row, result: Vec::with_capacity(count) };
    unsafe { linear_positions(stride, count as u64, ptr::from_mut(&mut fields).cast(), append) };
    fields.result
}

pub(super) fn ray_hit_fields(value: &mut Ty) -> VectorLayout {
    unsafe extern "C" fn append(context: *mut c_void, name: Text, offset: u32, kind: Builtin) {
        let value = unsafe { &mut *context.cast::<Ty>() };
        let spelling = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name.bytes, name.length)) };
        let field = Ty::from_builtin(kind).unwrap();
        value.fields.push((spelling.to_owned(), offset, Box::new(field)));
    }

    unsafe { b_ray_hit_fields(ptr::from_mut(value).cast(), append) }
}

#[repr(u32)]
#[cfg(feature = "compiler")]
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
#[cfg(feature = "compiler")]
struct MemberSelection {
    kind: MemberKind,
    count: u32,
    part: Part,
    lanes: [u32; 4],
}
#[repr(C)]
#[cfg(feature = "compiler")]
struct MemberHost {
    inspect: unsafe extern "C" fn(*const c_void) -> Node,
    entry: unsafe extern "C" fn(*const c_void, Child, usize) -> PathEntry,
    swizzle_width: unsafe extern "C" fn(*const c_void) -> usize,
}

#[cfg(feature = "compiler")]
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
#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::source_member"]
    fn b_source_member(value: *const c_void, name: Text, host: MemberHost) -> MemberSelection;
}
#[cfg(feature = "compiler")]
unsafe extern "C" fn swizzle_width(value: *const c_void) -> usize {
    let ty = unsafe { &*value.cast::<Ty>() };
    if ty.is_handle() { 0 } else { ty.components.len() }
}
#[cfg(feature = "compiler")]
pub(crate) fn source_member<'a>(value: &'a Ty, name: &str) -> SourceMember<'a> {
    let selected = unsafe {
        b_source_member(address(value), Text { bytes: name.as_ptr(), length: name.len() },
            MemberHost { inspect, entry, swizzle_width })
    };
    match selected.kind {
        MemberKind::Missing => SourceMember::Missing,
        MemberKind::Variant => SourceMember::Variant,
        MemberKind::HiddenView => SourceMember::HiddenView,
        MemberKind::Length => SourceMember::Length,
        MemberKind::Declared => SourceMember::Declared {
            ty: unsafe { &*selected.part.value.cast::<Ty>() }, offset: selected.part.offset,
        },
        MemberKind::Swizzle => SourceMember::Swizzle { lanes: selected.lanes, count: selected.count },
        MemberKind::UnknownComponent => SourceMember::UnknownComponent,
        MemberKind::ComponentBounds => SourceMember::ComponentBounds,
    }
}

#[repr(C)]
#[cfg(feature = "compiler")]
struct VariantMember {
    present: u32,
    discriminant: u32,
}
#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::declared_variant"]
    fn b_declared_variant(value: *const c_void, name: Text, count: usize,
        entry: unsafe extern "C" fn(*const c_void, Child, usize) -> PathEntry) -> VariantMember;
}
#[cfg(feature = "compiler")]
impl Ty {
    pub(crate) fn declared_variant(&self, name: &str) -> Option<u32> {
        let selected = unsafe {
            b_declared_variant(address(self), Text { bytes: name.as_ptr(), length: name.len() },
                self.variants.len(), entry)
        };
        (selected.present != 0).then_some(selected.discriminant)
    }
}
