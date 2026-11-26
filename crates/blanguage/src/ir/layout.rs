//! JSON of the original value and program interface layouts. Blink controls the
//! traversal and writes the schema. The host lends values and owns the buffers.
use super::{ProgramInterface, Ty};
#[cfg(feature = "compiler")]
use crate::ast::TypeName;
#[cfg(feature = "compiler")]
use std::collections::BTreeSet;
use std::{ffi::c_void, fmt::Write, ptr, slice, str};

#[repr(C)]
#[derive(Clone, Copy)]
struct Text {
    bytes: *const u8,
    length: usize,
}

impl Text {
    unsafe fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.bytes, self.length)) }
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self { bytes: value.as_ptr(), length: value.len() }
    }
}

#[repr(C)]
struct LayoutNode {
    name: Text,
    flags: u32,
    size: u32,
    align: u32,
    arguments: usize,
    values: usize,
    components: usize,
}

#[repr(C)]
struct LayoutComponent {
    scalar: Text,
    offset: u32,
    size: u32,
    bit_width: u32,
}

#[repr(C)]
struct LayoutField {
    name: Text,
    offset: u32,
    value: *const c_void,
}

#[repr(C)]
struct LayoutVariant {
    name: Text,
    discriminant: u32,
    offset: u32,
    value: *const c_void,
}

#[repr(u32)]
#[allow(dead_code)] // Blink produces the continuation actions.
enum PartKind { Done, Value, Fields, Variants, Array, View, Dataset, End, Close, Release }

#[repr(C)]
struct Part {
    kind: PartKind,
    value: *const c_void,
    index: usize,
}

#[cfg(feature = "compiler")]
#[repr(u32)]
#[allow(dead_code)] // Blink produces the requests.
enum Need { Done, Resolve, Release }

#[cfg(feature = "compiler")]
#[repr(C)]
struct Request {
    need: Need,
    value: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LayoutSink {
    text: unsafe extern "C" fn(*mut c_void, Text),
    string: unsafe extern "C" fn(*mut c_void, Text),
    number: unsafe extern "C" fn(*mut c_void, u64),
    push: unsafe extern "C" fn(*mut c_void, Part),
    pop: unsafe extern "C" fn(*mut c_void) -> Part,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LayoutSource {
    node: unsafe extern "C" fn(*const c_void) -> LayoutNode,
    argument: unsafe extern "C" fn(*const c_void, usize) -> Text,
    value: unsafe extern "C" fn(*const c_void, usize) -> u32,
    component: unsafe extern "C" fn(*const c_void, usize) -> LayoutComponent,
    field: unsafe extern "C" fn(*const c_void, usize) -> LayoutField,
    variant: unsafe extern "C" fn(*const c_void, usize) -> LayoutVariant,
    array: unsafe extern "C" fn(*const c_void) -> *const c_void,
    view: unsafe extern "C" fn(*const c_void) -> *const c_void,
    dataset_name: unsafe extern "C" fn(*mut c_void, *const c_void),
    cleanup: unsafe extern "C" fn(*const c_void) -> Text,
}

#[repr(C)]
struct LayoutProgram {
    name: Text,
    target: u32,
    uniform_bytes: u32,
    buffers: usize,
    uniforms: usize,
}

#[repr(C)]
struct LayoutBuffer {
    name: Text,
    access: u32,
    stride: u32,
    count_present: u32,
    count: u32,
    element: *const c_void,
}

#[repr(C)]
struct LayoutUniform {
    name: Text,
    offset: u32,
    value: *const c_void,
}

#[repr(C)]
struct LayoutInterface {
    owner: unsafe extern "C" fn(*const c_void) -> LayoutProgram,
    buffer: unsafe extern "C" fn(*const c_void, usize) -> LayoutBuffer,
    uniform: unsafe extern "C" fn(*const c_void, usize) -> LayoutUniform,
}

unsafe extern "C" {
    #[link_name = "codec.layout::write_value"]
    fn write_value(out: *mut c_void, value: *const c_void, sink: LayoutSink, source: LayoutSource);
    #[cfg(feature = "compiler")]
    #[link_name = "codec.layout::next_reference"]
    fn next_reference(out: *mut c_void, sink: LayoutSink, source: LayoutSource) -> Request;
    #[cfg(feature = "compiler")]
    #[link_name = "codec.layout::resolved_element"]
    fn resolved_element(out: *mut c_void, value: *const c_void, repeated: u32,
        sink: LayoutSink, source: LayoutSource);
    #[link_name = "codec.layout::write_interface"]
    fn write_interface(out: *mut c_void, program: *const c_void, sink: LayoutSink,
        source: LayoutSource, interface: LayoutInterface);
}

const SINK: LayoutSink = LayoutSink {
    text: sink_text, string: sink_string, number: sink_number, push: push_part, pop: pop_part,
};
const SOURCE: LayoutSource = LayoutSource {
    node, argument, value, component, field, variant, array, view, dataset_name, cleanup,
};

#[derive(Default)]
struct Output {
    text: String,
    parts: Vec<Part>,
}

impl Output {
    fn handle(&mut self) -> *mut c_void {
        ptr::from_mut(self).cast()
    }
}

impl Ty {
    /// Describes this actual value layout. Referenced storage is separate from its descriptor.
    pub fn layout_json(&self) -> String {
        let mut out = Output::default();
        unsafe { write_value(out.handle(), ptr::from_ref(self).cast(), SINK, SOURCE) };
        out.text
    }
}

/// The source query lends its root and holds only the nominal elements still referenced by Blink.
#[cfg(feature = "compiler")]
pub(crate) struct ExpandedLayout<'a> {
    out: Output,
    active: BTreeSet<String>,
    resolved: Vec<Box<Ty>>,
    _root: &'a Ty,
}

#[cfg(feature = "compiler")]
impl<'a> ExpandedLayout<'a> {
    pub(crate) fn new(root: &'a Ty) -> Self {
        let mut out = Output::default();
        out.parts.push(Part { kind: PartKind::Value, value: ptr::from_ref(root).cast(), index: 0 });
        Self { out, active: BTreeSet::from([root.name.clone()]), resolved: Vec::new(), _root: root }
    }

    pub(crate) fn next(&mut self) -> Option<&TypeName> {
        loop {
            let request = unsafe { next_reference(self.out.handle(), SINK, SOURCE) };
            match request.need {
                Need::Done => return None,
                Need::Resolve => return unsafe { &*request.value.cast::<Ty>() }.dataset.as_deref(),
                Need::Release => {
                    self.active.remove(&unsafe { &*request.value.cast::<Ty>() }.name);
                    self.resolved.pop();
                }
            }
        }
    }

    pub(crate) fn expand(&mut self, value: Ty) {
        let repeated = !self.active.insert(value.name.clone());
        if repeated {
            unsafe { resolved_element(self.out.handle(), ptr::from_ref(&value).cast(), 1, SINK, SOURCE) };
        } else {
            let value = Box::new(value);
            let pointer = ptr::from_ref(value.as_ref()).cast();
            self.resolved.push(value);
            unsafe { resolved_element(self.out.handle(), pointer, 0, SINK, SOURCE) };
        }
    }

    pub(crate) fn finish(self) -> String {
        self.out.text
    }
}

/// JSON of the existing interface's data layouts, including interfaces loaded from an image.
pub fn interface_layout_json<P: ProgramInterface + ?Sized>(program: &P) -> String {
    let mut out = Output::default();
    let interface = LayoutInterface {
        owner: interface_header::<P>, buffer: interface_buffer::<P>, uniform: interface_uniform::<P>,
    };
    unsafe { write_interface(out.handle(), ptr::from_ref(&program).cast(), SINK, SOURCE, interface) };
    out.text
}

unsafe extern "C" fn sink_text(out: *mut c_void, value: Text) {
    unsafe { &mut *out.cast::<Output>() }.text.push_str(unsafe { value.as_str() });
}

unsafe extern "C" fn sink_string(out: *mut c_void, value: Text) {
    crate::json::string(&mut unsafe { &mut *out.cast::<Output>() }.text, unsafe { value.as_str() });
}

unsafe extern "C" fn sink_number(out: *mut c_void, value: u64) {
    write!(unsafe { &mut *out.cast::<Output>() }.text, "{value}").unwrap();
}

unsafe extern "C" fn push_part(out: *mut c_void, part: Part) {
    unsafe { &mut *out.cast::<Output>() }.parts.push(part);
}

unsafe extern "C" fn pop_part(out: *mut c_void) -> Part {
    unsafe { &mut *out.cast::<Output>() }.parts.pop()
        .unwrap_or(Part { kind: PartKind::Done, value: ptr::null(), index: 0 })
}

unsafe extern "C" fn node(value: *const c_void) -> LayoutNode {
    let ty = unsafe { &*value.cast::<Ty>() };
    let flags = u32::from(ty.dataset.is_some())
        | (u32::from(ty.view.is_some()) << 1)
        | (u32::from(ty.array.is_some()) << 2)
        | (u32::from(ty.result) << 3)
        | (u32::from(ty.enumeration) << 4)
        | (u32::from(ty.cleanup.is_some()) << 5)
        | (u32::from(ty.function.is_some()) << 6)
        | (u32::from(ty.name == "void") << 7)
        | (u32::from(ty.has_variants()) << 8)
        | (u32::from(!ty.fields.is_empty()) << 9);
    LayoutNode {
        name: ty.name.as_str().into(), flags, size: ty.size, align: ty.align,
        arguments: ty.arguments.len(), values: ty.values.len(), components: ty.components.len(),
    }
}

unsafe extern "C" fn argument(value: *const c_void, index: usize) -> Text {
    unsafe { &*value.cast::<Ty>() }.arguments[index].name.as_str().into()
}

unsafe extern "C" fn value(value: *const c_void, index: usize) -> u32 {
    unsafe { &*value.cast::<Ty>() }.values[index]
}

unsafe extern "C" fn component(value: *const c_void, index: usize) -> LayoutComponent {
    let (scalar, offset) = unsafe { &*value.cast::<Ty>() }.components[index];
    LayoutComponent { scalar: scalar.name().into(), offset, size: scalar.size(), bit_width: scalar.bit_width() }
}

unsafe extern "C" fn field(value: *const c_void, index: usize) -> LayoutField {
    let ty = unsafe { &*value.cast::<Ty>() };
    match ty.fields.get(index) {
        Some((name, offset, field)) => LayoutField {
            name: name.as_str().into(), offset: *offset, value: ptr::from_ref(field.as_ref()).cast(),
        },
        None => LayoutField { name: "".into(), offset: 0, value: ptr::null() },
    }
}

unsafe extern "C" fn variant(value: *const c_void, index: usize) -> LayoutVariant {
    let ty = unsafe { &*value.cast::<Ty>() };
    match ty.variants.get(index) {
        Some(variant) => LayoutVariant {
            name: variant.name.as_str().into(), discriminant: variant.discriminant,
            offset: ty.variant_payload_offset(), value: ptr::from_ref(variant.ty.as_ref()).cast(),
        },
        None => LayoutVariant { name: "".into(), discriminant: 0, offset: 0, value: ptr::null() },
    }
}

unsafe extern "C" fn array(value: *const c_void) -> *const c_void {
    unsafe { &*value.cast::<Ty>() }.array.as_deref().map_or(ptr::null(), |element| ptr::from_ref(element).cast())
}

unsafe extern "C" fn view(value: *const c_void) -> *const c_void {
    unsafe { &*value.cast::<Ty>() }.view.as_deref().map_or(ptr::null(), |element| ptr::from_ref(element).cast())
}

unsafe extern "C" fn dataset_name(out: *mut c_void, value: *const c_void) {
    let reference = unsafe { &*value.cast::<Ty>() }.dataset.as_deref().unwrap();
    let name = crate::ast::type_name(reference);
    crate::json::string(&mut unsafe { &mut *out.cast::<Output>() }.text, &name);
}

unsafe extern "C" fn cleanup(value: *const c_void) -> Text {
    unsafe { &*value.cast::<Ty>() }.cleanup.as_deref().unwrap_or("").into()
}

unsafe extern "C" fn interface_header<P: ProgramInterface + ?Sized>(context: *const c_void) -> LayoutProgram {
    let program = unsafe { *context.cast::<&P>() };
    LayoutProgram {
        name: program.name().into(), target: program.target() as u32, uniform_bytes: program.uniform_bytes(),
        buffers: program.buffers().len(), uniforms: program.uniforms().len(),
    }
}

unsafe extern "C" fn interface_buffer<P: ProgramInterface + ?Sized>(context: *const c_void, index: usize) -> LayoutBuffer {
    let program = unsafe { *context.cast::<&P>() };
    let buffer = &program.buffers()[index];
    LayoutBuffer {
        name: buffer.name.as_str().into(), access: buffer.access as u32, stride: buffer.stride,
        count_present: u32::from(buffer.count.is_some()), count: buffer.count.unwrap_or(0),
        element: ptr::from_ref(&buffer.ty).cast(),
    }
}

unsafe extern "C" fn interface_uniform<P: ProgramInterface + ?Sized>(context: *const c_void, index: usize) -> LayoutUniform {
    let program = unsafe { *context.cast::<&P>() };
    let uniform = &program.uniforms()[index];
    LayoutUniform { name: uniform.name.as_str().into(), offset: uniform.offset, value: ptr::from_ref(&uniform.ty).cast() }
}

#[repr(u32)]
#[allow(dead_code, reason = "The shared B layout supplies the ABI tags.")]
enum SlotChildKind { Argument, Field, Variant, Element }

#[repr(C)]
struct SlotChild { value: *mut c_void, offset: u32 }

#[repr(C)]
struct SlotType {
    arguments: usize,
    fields: usize,
    variants: usize,
    components: usize,
    array: u32,
    count: u32,
    size: u32,
    alignment: u32,
}

#[repr(C)]
struct SlotHost {
    shape: unsafe extern "C" fn(*const c_void) -> SlotType,
    child: unsafe extern "C" fn(*mut c_void, SlotChildKind, usize) -> SlotChild,
    prepare: unsafe extern "C" fn(*mut c_void, usize, usize),
    copy: unsafe extern "C" fn(*mut c_void, *const c_void, u32),
    payload_word: unsafe extern "C" fn(*mut c_void, u32),
}

unsafe extern "C" {
    #[link_name = "codec.layout::fill_value_slots"]
    fn b_fill_value_slots(value: *mut c_void, complete: u32, host: SlotHost);
}

pub(super) fn fill_value_slots(value: &mut Ty, complete: bool) {
    let host = SlotHost {
        shape: slot_type,
        child: slot_child,
        prepare: prepare_slots,
        copy: copy_slots,
        payload_word: append_payload_word,
    };
    unsafe { b_fill_value_slots(ptr::from_mut(value).cast(), u32::from(complete), host) };
}

unsafe extern "C" fn slot_type(value: *const c_void) -> SlotType {
    let ty = unsafe { &*value.cast::<Ty>() };
    let array = ty.array.is_some();
    SlotType {
        arguments: ty.arguments.len(),
        fields: ty.fields.len(),
        variants: ty.variants.len(),
        components: ty.components.len(),
        array: u32::from(array),
        count: if array { ty.values[0] } else { 0 },
        size: ty.size,
        alignment: ty.align,
    }
}

unsafe extern "C" fn slot_child(value: *mut c_void, kind: SlotChildKind, index: usize) -> SlotChild {
    let ty = unsafe { &mut *value.cast::<Ty>() };
    match kind {
        SlotChildKind::Argument => SlotChild { value: ptr::from_mut(&mut ty.arguments[index]).cast(), offset: 0 },
        SlotChildKind::Field => {
            let (_, offset, field) = &mut ty.fields[index];
            SlotChild { value: ptr::from_mut(field.as_mut()).cast(), offset: *offset }
        }
        SlotChildKind::Variant => SlotChild { value: ptr::from_mut(ty.variants[index].ty.as_mut()).cast(), offset: 0 },
        SlotChildKind::Element => SlotChild { value: ptr::from_mut(ty.array.as_deref_mut().unwrap()).cast(), offset: 0 },
    }
}

unsafe extern "C" fn prepare_slots(value: *mut c_void, keep: usize, additional: usize) {
    let slots = unsafe { &mut (*value.cast::<Ty>()).components };
    slots.truncate(keep);
    slots.reserve(additional);
}

unsafe extern "C" fn copy_slots(value: *mut c_void, child: *const c_void, base: u32) {
    let slots = unsafe { &mut (*value.cast::<Ty>()).components };
    let child_slots = unsafe { &(*child.cast::<Ty>()).components };
    slots.extend(child_slots.iter().map(|&(scalar, offset)| (scalar, base + offset)));
}

unsafe extern "C" fn append_payload_word(value: *mut c_void, offset: u32) {
    unsafe { &mut (*value.cast::<Ty>()).components }.push((super::Scalar::U32, offset));
}
