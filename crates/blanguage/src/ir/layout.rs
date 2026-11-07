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
    match write_type_json::<std::convert::Infallible>(out, value, &mut |_, _| Ok(())) {
        Ok(()) => (),
        Err(never) => match never {},
    }
}

pub(crate) fn write_type_json<E>(out: &mut String, value: &Ty, references: &mut dyn FnMut(&mut String, &TypeName) -> Result<(), E>) -> Result<(), E> {
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
    out.push_str("],\"fields\":[");
    for (i, (name, offset, field)) in value.fields.iter().enumerate() {
        if i != 0 { out.push(','); }
        out.push_str("{\"name\":"); string(out, name);
        write!(out, ",\"offset_bytes\":{offset},\"layout\":").unwrap();
        write_type_json(out, field, references)?; out.push('}');
    }
    out.push_str("],\"variants\":[");
    for (i, variant) in value.variants.iter().enumerate() {
        if i != 0 { out.push(','); }
        let (offset, payload) = value.field(&variant.name).unwrap();
        out.push_str("{\"name\":"); string(out, &variant.name);
        write!(out, ",\"discriminant\":{},\"payload_offset_bytes\":{offset},\"layout\":", variant.discriminant).unwrap();
        write_type_json(out, payload, references)?; out.push('}');
    }
    out.push_str("],\"array\":");
    if let Some(element) = &value.array {
        write!(out, "{{\"count\":{},\"element_stride_bytes\":{},\"element\":", value.values[0], align_up(element.size, element.align)).unwrap();
        write_type_json(out, element, references)?; out.push('}');
    } else { out.push_str("null"); }
    out.push_str(",\"view\":");
    element(out, value.view.as_deref(), references)?;
    out.push_str(",\"dataset\":");
    if let Some(value_type) = &value.dataset {
        out.push_str("{\"fixed_count\":");
        if let Some(count) = value.values.first() { write!(out, "{count}").unwrap(); }
        else { out.push_str("null"); }
        out.push_str(",\"element_reference\":");
        string(out,&crate::ast::type_name(value_type));
        references(out, value_type)?; out.push('}');
    } else { out.push_str("null"); }
    out.push_str(",\"cleanup\":");
    if let Some(cleanup) = &value.cleanup { string(out, cleanup); }
    else { out.push_str("null"); }
    out.push('}');
    Ok(())
}

fn element<E>(out: &mut String, value: Option<&Ty>, references: &mut dyn FnMut(&mut String, &TypeName) -> Result<(), E>) -> Result<(), E> {
    if let Some(value) = value {
        write!(out, "{{\"element_stride_bytes\":{},\"element\":", align_up(value.size, value.align)).unwrap();
        write_type_json(out, value, references)?; out.push('}');
    } else { out.push_str("null"); }
    Ok(())
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
