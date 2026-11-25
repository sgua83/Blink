//! Read-only serialization of the original Ty and program interface layouts.
use super::{ProgramInterface, Ty, align_up};
use crate::{ast::{Access, Target, TypeName}, json::string};
use std::fmt::Write;

impl Ty {
    /// Describes this actual value layout. Referenced storage is separate from its descriptor.
    pub fn layout_json(&self) -> String {
        let mut out = String::new();
        ty(&mut out, self);
        out
    }
}

fn ty(out: &mut String, value: &Ty) {
    let mut layout = TypeJson::new(value);
    while layout.next(out).is_some() {}
}

// Each part borrows the actual layout and resumes at its original output field.
// The dataset consumer may append its element before the closing part resumes.
enum Part<'a> {
    Value(&'a Ty),
    Fields(&'a Ty, usize),
    Variants(&'a Ty, usize),
    Array(&'a Ty),
    View(&'a Ty),
    Dataset(&'a Ty),
    End(&'a Ty),
    Close,
}
pub(crate) struct TypeJson<'a> {
    parts: Vec<Part<'a>>,
}
impl<'a> TypeJson<'a> {
    pub(crate) fn new(value: &'a Ty) -> Self {
        Self { parts: vec![Part::Value(value)] }
    }
    pub(crate) fn next(&mut self, out: &mut String) -> Option<&'a TypeName> {
        while let Some(part) = self.parts.pop() {
            match part {
                Part::Value(value) => {
                    out.push_str("{\"name\":");
                    string(out, &value.name);
                    out.push_str(",\"kind\":");
                    string(out, if value.dataset.is_some() { "dataset_reference" }
                        else if value.view.is_some() { "view_reference" }
                        else if value.array.is_some() { "array" }
                        else if value.result { "result" }
                        else if value.has_variants() { "variant" }
                        else if value.enumeration { "enumeration" }
                        else if value.cleanup.is_some() { "owned" }
                        else if value.function.is_some() { "c_function" }
                        else if value.name == "void" { "unit" }
                        else if !value.fields.is_empty() { "record" }
                        else { "value" });
                    write!(out, ",\"size_bytes\":{},\"align_bytes\":{},\"stride_bytes\":{},\"resource_word_bytes\":4,\"resource_word_count\":{},\"type_arguments\":[",
                        value.size, value.align, align_up(value.size, value.align), value.size.div_ceil(4)).unwrap();
                    for (i, argument) in value.arguments.iter().enumerate() {
                        if i != 0 { out.push(','); }
                        string(out, &argument.name);
                    }
                    out.push_str("],\"const_arguments\":[");
                    for (i, argument) in value.values.iter().enumerate() {
                        if i != 0 { out.push(','); }
                        write!(out, "{argument}").unwrap();
                    }
                    out.push_str("],\"components\":[");
                    for (i, (scalar, offset)) in value.components.iter().enumerate() {
                        if i != 0 { out.push(','); }
                        out.push_str("{\"scalar\":"); string(out, scalar.name());
                        write!(out, ",\"offset_bytes\":{offset},\"size_bytes\":{},\"bit_width\":{},\"first_resource_word\":{},\"byte_in_word\":{},\"resource_word_count\":{}}}",
                            scalar.size(), scalar.bit_width(), offset / 4, offset % 4, (offset % 4 + scalar.size()).div_ceil(4)).unwrap();
                    }
                    out.push(']');
                    if (value.components.is_empty() && (value.array.is_some() || !value.fields.is_empty()))
                        || (value.has_variants() && value.components.len() == 1 && value.size > value.align)
                    {
                        out.push_str(",\"component_layout\":\"structural\"");
                    }
                    out.push_str(",\"fields\":[");
                    self.parts.push(Part::Fields(value, 0));
                }
                Part::Fields(value, index) => {
                    if let Some((name, offset, field)) = value.fields.get(index) {
                        if index != 0 { out.push(','); }
                        out.push_str("{\"name\":"); string(out, name);
                        write!(out, ",\"offset_bytes\":{offset},\"layout\":").unwrap();
                        self.parts.push(Part::Fields(value, index + 1));
                        self.parts.push(Part::Close);
                        self.parts.push(Part::Value(field));
                    } else {
                        out.push_str("],\"variants\":[");
                        self.parts.push(Part::Variants(value, 0));
                    }
                }
                Part::Variants(value, index) => {
                    if let Some(variant) = value.variants.get(index) {
                        if index != 0 { out.push(','); }
                        let (offset, payload) = value.field(&variant.name).unwrap();
                        out.push_str("{\"name\":"); string(out, &variant.name);
                        write!(out, ",\"discriminant\":{},\"payload_offset_bytes\":{offset},\"layout\":", variant.discriminant).unwrap();
                        self.parts.push(Part::Variants(value, index + 1));
                        self.parts.push(Part::Close);
                        self.parts.push(Part::Value(payload));
                    } else {
                        out.push_str("],\"array\":");
                        self.parts.push(Part::Array(value));
                    }
                }
                Part::Array(value) => {
                    self.parts.push(Part::View(value));
                    if let Some(element) = &value.array {
                        write!(out, "{{\"count\":{},\"element_stride_bytes\":{},\"element\":", value.values[0], align_up(element.size, element.align)).unwrap();
                        self.parts.push(Part::Close);
                        self.parts.push(Part::Value(element));
                    } else { out.push_str("null"); }
                }
                Part::View(value) => {
                    out.push_str(",\"view\":");
                    self.parts.push(Part::Dataset(value));
                    if let Some(element) = &value.view {
                        write!(out, "{{\"element_stride_bytes\":{},\"element\":", align_up(element.size, element.align)).unwrap();
                        self.parts.push(Part::Close);
                        self.parts.push(Part::Value(element));
                    } else { out.push_str("null"); }
                }
                Part::Dataset(value) => {
                    out.push_str(",\"dataset\":");
                    self.parts.push(Part::End(value));
                    if let Some(reference) = &value.dataset {
                        out.push_str("{\"fixed_count\":");
                        if let Some(count) = value.values.first() { write!(out, "{count}").unwrap(); }
                        else { out.push_str("null"); }
                        out.push_str(",\"element_reference\":");
                        string(out, &crate::ast::type_name(reference));
                        self.parts.push(Part::Close);
                        return Some(reference);
                    } else { out.push_str("null"); }
                }
                Part::End(value) => {
                    out.push_str(",\"cleanup\":");
                    if let Some(cleanup) = &value.cleanup { string(out, cleanup); }
                    else { out.push_str("null"); }
                    out.push('}');
                }
                Part::Close => out.push('}'),
            }
        }
        None
    }
}

/// JSON of the existing interface's data layouts. This adds no method to its runtime vtable.
pub fn interface_layout_json<P: ProgramInterface + ?Sized>(program: &P) -> String {
    let mut out = String::from("{\"kind\":\"interface_layout\",\"program\":");
    string(&mut out, program.name());
    out.push_str(",\"target\":");
    string(&mut out, match program.target() { Target::Cpu => "cpu", Target::Gpu => "gpu", Target::Compute => "compute" });
    write!(&mut out, ",\"uniform_bytes\":{},\"buffers\":[", program.uniform_bytes()).unwrap();
    for (i, buffer) in program.buffers().iter().enumerate() {
        if i != 0 { out.push(','); }
        write!(&mut out, "{{\"binding\":{i},\"name\":").unwrap(); string(&mut out, &buffer.name);
        out.push_str(",\"access\":");
        string(&mut out, match buffer.access { Access::Value => "value", Access::Read => "read", Access::Write => "write", Access::InOut => "inout", Access::Atomic => "atomic" });
        write!(&mut out, ",\"stride_bytes\":{},\"fixed_count\":", buffer.stride).unwrap();
        if let Some(count) = buffer.count { write!(&mut out, "{count}").unwrap(); }
        else { out.push_str("null"); }
        out.push_str(",\"element\":"); ty(&mut out, &buffer.ty); out.push('}');
    }
    out.push_str("],\"uniforms\":[");
    for (i, uniform) in program.uniforms().iter().enumerate() {
        if i != 0 { out.push(','); }
        out.push_str("{\"name\":"); string(&mut out, &uniform.name);
        write!(&mut out, ",\"offset_bytes\":{},\"layout\":", uniform.offset).unwrap();
        ty(&mut out, &uniform.ty); out.push('}');
    }
    out.push_str("]}");
    out
}
