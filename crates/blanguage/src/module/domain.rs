//! 整域模块的实际输入合同、公开布局及完成效果；不包含执行器或可变运行状态。
use crate::{ast::Target, ir::*};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Output {
    pub binding: usize,
    pub domain: Option<usize>,
    pub words: Vec<u32>,
}
#[derive(Clone)]
pub(crate) struct Interface {
    pub instance: String,
    pub target: Target,
    pub buffers: Vec<Buffer>,
    pub textures: Vec<TextureBinding>,
    pub write_regions: std::collections::BTreeMap<usize, std::collections::BTreeSet<(u32, u32)>>,
    pub uniforms: Vec<Uniform>,
    pub length_requirements: Vec<(usize, usize)>,
    pub element_requirements: Vec<ElementFact>,
    pub prefix_guarantees: Vec<PrefixRelation>,
    pub outputs: Vec<Output>,
    pub elements: std::collections::BTreeMap<usize, crate::check::Proof>,
}
pub(crate) fn symbol(name: &str, target: Target) -> String {
    format!(
        "{name}#domain{}",
        if target == Target::Cpu { "cpu" } else { "gpu" }
    )
}



// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Output);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        binding: Slot,
        domain: Slot,
        words: Slot,
    }
    #[repr(C)]
    struct Writer {
        binding: WriteCallback,
        domain: WriteCallback,
        words: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        binding: ReadCallback,
        domain: ReadCallback,
        words: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_domain_output"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_domain_output"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Output {
        fn empty() -> Self {
            Self {
                binding: <usize as Native>::empty(),
                domain: <Option<usize> as Native>::empty(),
                words: <Vec<u32> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        binding: contracts::address(&self.binding).cast_mut(),
                        domain: contracts::address(&self.domain).cast_mut(),
                        words: contracts::address(&self.words).cast_mut(),
                    },
                    Writer {
                        binding: contracts::write_value::<usize>,
                        domain: contracts::write_value::<Option<usize>>,
                        words: contracts::write_value::<Vec<u32>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        binding: contracts::slot(&mut output.binding),
                        domain: contracts::slot(&mut output.domain),
                        words: contracts::slot(&mut output.words),
                    },
                    Reader {
                        binding: contracts::read_value::<usize>,
                        domain: contracts::read_value::<Option<usize>>,
                        words: contracts::read_value::<Vec<u32>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Interface);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        instance: Slot,
        target: Slot,
        buffers: Slot,
        textures: Slot,
        write_regions: Slot,
        uniforms: Slot,
        length_requirements: Slot,
        element_requirements: Slot,
        prefix_guarantees: Slot,
        outputs: Slot,
        elements: Slot,
    }
    #[repr(C)]
    struct Writer {
        instance: WriteCallback,
        target: WriteCallback,
        buffers: WriteCallback,
        textures: WriteCallback,
        write_regions: WriteCallback,
        uniforms: WriteCallback,
        length_requirements: WriteCallback,
        element_requirements: WriteCallback,
        prefix_guarantees: WriteCallback,
        outputs: WriteCallback,
        elements: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        instance: ReadCallback,
        target: ReadCallback,
        buffers: ReadCallback,
        textures: ReadCallback,
        write_regions: ReadCallback,
        uniforms: ReadCallback,
        length_requirements: ReadCallback,
        element_requirements: ReadCallback,
        prefix_guarantees: ReadCallback,
        outputs: ReadCallback,
        elements: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_domain_interface"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_domain_interface"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Interface {
        fn empty() -> Self {
            Self {
                instance: <String as Native>::empty(),
                target: <Target as Native>::empty(),
                buffers: <Vec<Buffer> as Native>::empty(),
                textures: <Vec<TextureBinding> as Native>::empty(),
                write_regions: <std::collections::BTreeMap<
                    usize,
                    std::collections::BTreeSet<(u32, u32)>,
                > as Native>::empty(),
                uniforms: <Vec<Uniform> as Native>::empty(),
                length_requirements: <Vec<(usize, usize)> as Native>::empty(),
                element_requirements: <Vec<ElementFact> as Native>::empty(),
                prefix_guarantees: <Vec<PrefixRelation> as Native>::empty(),
                outputs: <Vec<Output> as Native>::empty(),
                elements: <std::collections::BTreeMap<usize, crate::check::Proof> as Native>::empty(
                ),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        instance: contracts::address(&self.instance).cast_mut(),
                        target: contracts::address(&self.target).cast_mut(),
                        buffers: contracts::address(&self.buffers).cast_mut(),
                        textures: contracts::address(&self.textures).cast_mut(),
                        write_regions: contracts::address(&self.write_regions).cast_mut(),
                        uniforms: contracts::address(&self.uniforms).cast_mut(),
                        length_requirements: contracts::address(&self.length_requirements)
                            .cast_mut(),
                        element_requirements: contracts::address(&self.element_requirements)
                            .cast_mut(),
                        prefix_guarantees: contracts::address(&self.prefix_guarantees).cast_mut(),
                        outputs: contracts::address(&self.outputs).cast_mut(),
                        elements: contracts::address(&self.elements).cast_mut(),
                    },
                    Writer {
                        instance: contracts::write_value::<String>,
                        target: contracts::write_value::<Target>,
                        buffers: contracts::write_value::<Vec<Buffer>>,
                        textures: contracts::write_value::<Vec<TextureBinding>>,
                        write_regions: contracts::write_value::<
                            std::collections::BTreeMap<
                                usize,
                                std::collections::BTreeSet<(u32, u32)>,
                            >,
                        >,
                        uniforms: contracts::write_value::<Vec<Uniform>>,
                        length_requirements: contracts::write_value::<Vec<(usize, usize)>>,
                        element_requirements: contracts::write_value::<Vec<ElementFact>>,
                        prefix_guarantees: contracts::write_value::<Vec<PrefixRelation>>,
                        outputs: contracts::write_value::<Vec<Output>>,
                        elements: contracts::write_value::<
                            std::collections::BTreeMap<usize, crate::check::Proof>,
                        >,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        instance: contracts::slot(&mut output.instance),
                        target: contracts::slot(&mut output.target),
                        buffers: contracts::slot(&mut output.buffers),
                        textures: contracts::slot(&mut output.textures),
                        write_regions: contracts::slot(&mut output.write_regions),
                        uniforms: contracts::slot(&mut output.uniforms),
                        length_requirements: contracts::slot(&mut output.length_requirements),
                        element_requirements: contracts::slot(&mut output.element_requirements),
                        prefix_guarantees: contracts::slot(&mut output.prefix_guarantees),
                        outputs: contracts::slot(&mut output.outputs),
                        elements: contracts::slot(&mut output.elements),
                    },
                    Reader {
                        instance: contracts::read_value::<String>,
                        target: contracts::read_value::<Target>,
                        buffers: contracts::read_value::<Vec<Buffer>>,
                        textures: contracts::read_value::<Vec<TextureBinding>>,
                        write_regions: contracts::read_value::<
                            std::collections::BTreeMap<
                                usize,
                                std::collections::BTreeSet<(u32, u32)>,
                            >,
                        >,
                        uniforms: contracts::read_value::<Vec<Uniform>>,
                        length_requirements: contracts::read_value::<Vec<(usize, usize)>>,
                        element_requirements: contracts::read_value::<Vec<ElementFact>>,
                        prefix_guarantees: contracts::read_value::<Vec<PrefixRelation>>,
                        outputs: contracts::read_value::<Vec<Output>>,
                        elements: contracts::read_value::<
                            std::collections::BTreeMap<usize, crate::check::Proof>,
                        >,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};
