//! Synchronous projections of the original resource layout; B owns field order.
use super::ProgramLayout;
use crate::codec::{
    ReadStatus,
    contracts::{self, Native, Read, Slot, Write},
};
use crate::{
    ast::Target,
    ir::{Buffer, DeviceRequirements, ElementFact, PrefixRelation, TextureBinding, Uniform},
};
use std::collections::{BTreeMap, BTreeSet};

#[repr(C)]
struct Fields {
    write_regions: Slot,
    name: Slot,
    target: Slot,
    buffers: Slot,
    accelerations: Slot,
    textures: Slot,
    uniforms: Slot,
    uniform_bytes: Slot,
    prefix_requirements: Slot,
    prefix_guarantees: Slot,
    element_requirements: Slot,
    element_guarantees: Slot,
    length_requirements: Slot,
    requirements: Slot,
}
#[repr(C)]
struct Writer {
    write_regions: Write,
    name: Write,
    target: Write,
    buffers: Write,
    accelerations: Write,
    textures: Write,
    uniforms: Write,
    uniform_bytes: Write,
    prefix_requirements: Write,
    prefix_guarantees: Write,
    element_requirements: Write,
    element_guarantees: Write,
    length_requirements: Write,
    requirements: Write,
}
#[repr(C)]
struct Reader {
    write_regions: Read,
    name: Read,
    target: Read,
    buffers: Read,
    accelerations: Read,
    textures: Read,
    uniforms: Read,
    uniform_bytes: Read,
    prefix_requirements: Read,
    prefix_guarantees: Read,
    element_requirements: Read,
    element_guarantees: Read,
    length_requirements: Read,
    requirements: Read,
}
unsafe extern "C" {
    #[link_name = "codec.value_format::resource_write_program_layout"]
    fn write(output: Slot, fields: Fields, host: Writer);
    #[link_name = "codec.value_format::resource_read_program_layout"]
    fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
}
impl Native for ProgramLayout {
    fn empty() -> Self {
        Self {
            write_regions: <BTreeMap<usize, BTreeSet<(u32, u32)>> as Native>::empty(),
            name: <String as Native>::empty(),
            target: <Target as Native>::empty(),
            buffers: <Vec<Buffer> as Native>::empty(),
            accelerations: <Vec<String> as Native>::empty(),
            textures: <Vec<TextureBinding> as Native>::empty(),
            uniforms: <Vec<Uniform> as Native>::empty(),
            uniform_bytes: <u32 as Native>::empty(),
            prefix_requirements: <Vec<PrefixRelation> as Native>::empty(),
            prefix_guarantees: <Vec<PrefixRelation> as Native>::empty(),
            element_requirements: <Vec<ElementFact> as Native>::empty(),
            element_guarantees: <Vec<ElementFact> as Native>::empty(),
            length_requirements: <Vec<(usize, usize)> as Native>::empty(),
            requirements: <DeviceRequirements as Native>::empty(),
        }
    }
    unsafe fn write(&self, output: Slot) {
        unsafe {
            write(
                output,
                Fields {
                    write_regions: contracts::address(&self.write_regions).cast_mut(),
                    name: contracts::address(&self.name).cast_mut(),
                    target: contracts::address(&self.target).cast_mut(),
                    buffers: contracts::address(&self.buffers).cast_mut(),
                    accelerations: contracts::address(&self.accelerations).cast_mut(),
                    textures: contracts::address(&self.textures).cast_mut(),
                    uniforms: contracts::address(&self.uniforms).cast_mut(),
                    uniform_bytes: contracts::address(&self.uniform_bytes).cast_mut(),
                    prefix_requirements: contracts::address(&self.prefix_requirements).cast_mut(),
                    prefix_guarantees: contracts::address(&self.prefix_guarantees).cast_mut(),
                    element_requirements: contracts::address(&self.element_requirements).cast_mut(),
                    element_guarantees: contracts::address(&self.element_guarantees).cast_mut(),
                    length_requirements: contracts::address(&self.length_requirements).cast_mut(),
                    requirements: contracts::address(&self.requirements).cast_mut(),
                },
                Writer {
                    write_regions: contracts::write_value::<BTreeMap<usize, BTreeSet<(u32, u32)>>>,
                    name: contracts::write_value::<String>,
                    target: contracts::write_value::<Target>,
                    buffers: contracts::write_value::<Vec<Buffer>>,
                    accelerations: contracts::write_value::<Vec<String>>,
                    textures: contracts::write_value::<Vec<TextureBinding>>,
                    uniforms: contracts::write_value::<Vec<Uniform>>,
                    uniform_bytes: contracts::write_value::<u32>,
                    prefix_requirements: contracts::write_value::<Vec<PrefixRelation>>,
                    prefix_guarantees: contracts::write_value::<Vec<PrefixRelation>>,
                    element_requirements: contracts::write_value::<Vec<ElementFact>>,
                    element_guarantees: contracts::write_value::<Vec<ElementFact>>,
                    length_requirements: contracts::write_value::<Vec<(usize, usize)>>,
                    requirements: contracts::write_value::<DeviceRequirements>,
                },
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe {
            read(
                context,
                Fields {
                    write_regions: contracts::slot(&mut output.write_regions),
                    name: contracts::slot(&mut output.name),
                    target: contracts::slot(&mut output.target),
                    buffers: contracts::slot(&mut output.buffers),
                    accelerations: contracts::slot(&mut output.accelerations),
                    textures: contracts::slot(&mut output.textures),
                    uniforms: contracts::slot(&mut output.uniforms),
                    uniform_bytes: contracts::slot(&mut output.uniform_bytes),
                    prefix_requirements: contracts::slot(&mut output.prefix_requirements),
                    prefix_guarantees: contracts::slot(&mut output.prefix_guarantees),
                    element_requirements: contracts::slot(&mut output.element_requirements),
                    element_guarantees: contracts::slot(&mut output.element_guarantees),
                    length_requirements: contracts::slot(&mut output.length_requirements),
                    requirements: contracts::slot(&mut output.requirements),
                },
                Reader {
                    write_regions: contracts::read_value::<BTreeMap<usize, BTreeSet<(u32, u32)>>>,
                    name: contracts::read_value::<String>,
                    target: contracts::read_value::<Target>,
                    buffers: contracts::read_value::<Vec<Buffer>>,
                    accelerations: contracts::read_value::<Vec<String>>,
                    textures: contracts::read_value::<Vec<TextureBinding>>,
                    uniforms: contracts::read_value::<Vec<Uniform>>,
                    uniform_bytes: contracts::read_value::<u32>,
                    prefix_requirements: contracts::read_value::<Vec<PrefixRelation>>,
                    prefix_guarantees: contracts::read_value::<Vec<PrefixRelation>>,
                    element_requirements: contracts::read_value::<Vec<ElementFact>>,
                    element_guarantees: contracts::read_value::<Vec<ElementFact>>,
                    length_requirements: contracts::read_value::<Vec<(usize, usize)>>,
                    requirements: contracts::read_value::<DeviceRequirements>,
                },
            )
        }
    }
}
contracts::codec!(ProgramLayout);
