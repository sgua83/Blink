//! 模块资源 ABI 2：编译后的类型、执行布局及原生映像；不传递 Rust 容器表示。
use crate::module::codec::{self, Decode, Encode, Reader, Writer, record, variants};
use crate::{TemplateProgram, ast::Target, ir::*, native, spirv};

// C 入口签名或本文件的字段／标签编码改变时，改变这一实际 ABI 身份。
pub const ABI: &[u8; 8] = b"BLRI\x02\0\0\0";
pub const F32: u32 = 0;
pub const U32: u32 = 1;
pub const I32: u32 = 2;
pub const BOOL: u32 = 3;
pub(crate) const fn scalar_tag(scalar: Scalar) -> u32 {
    match scalar {
        Scalar::F32 => F32,
        Scalar::U32 => U32,
        Scalar::I32 => I32,
        Scalar::Bool => BOOL,
    }
}
pub enum Image {
    Cpu(native::ProgramImage<TemplateProgram>),
    Gpu(spirv::ProgramImage<TemplateProgram>),
}
pub struct Module {
    pub types: Vec<Ty>,
    pub images: Vec<Image>,
}
record!(Module { types, images });
variants!(Image { 0=>Cpu(image), 1=>Gpu(image) });

pub(crate) fn compile(resources: &crate::application::Resources) -> codec::Result<Vec<u8>> {
    let images = resources
        .programs
        .iter()
        .map(|(target, program)| {
            let template = TemplateProgram(program.clone());
            if *target == Target::Cpu {
                native::compile_program(template).map(Image::Cpu)
            } else {
                spirv::compile_program(template).map(Image::Gpu)
            }
            .map_err(std::io::Error::other)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bytes = ABI.to_vec();
    bytes.extend(
        Module {
            types: resources.types.clone(),
            images,
        }
        .bytes(),
    );
    Ok(bytes)
}
/// # Safety
/// bytes 必须是此资源 ABI 的实际编译产物，原机器码、布局、访问与前缀合同未被伪造。
/// 资源 ABI 不使用生成它的 Rust 容器布局或编译器源码身份。
pub unsafe fn from_compiler_output(bytes: &[u8]) -> codec::Result<Module> {
    let bytes = bytes.strip_prefix(ABI).ok_or("不支持的模块资源 ABI")?;
    Module::from_bytes(bytes)
}
impl Encode for native::ProgramImage<TemplateProgram> {
    fn encode(&self, writer: &mut Writer) {
        self.program().encode(writer);
        self.stages().encode(writer);
    }
}
impl Decode for native::ProgramImage<TemplateProgram> {
    fn decode(reader: &mut Reader<'_>) -> codec::Result<Self> {
        let program = ExecutionPlan::decode(reader)?;
        let stages = Vec::<native::StageCode>::decode(reader)?;
        Ok(unsafe { Self::from_compiler_output(program, stages) })
    }
}
impl Encode for spirv::ProgramImage<TemplateProgram> {
    fn encode(&self, writer: &mut Writer) {
        self.program().encode(writer);
        self.stages().encode(writer);
    }
}
impl Decode for spirv::ProgramImage<TemplateProgram> {
    fn decode(reader: &mut Reader<'_>) -> codec::Result<Self> {
        let program = ExecutionPlan::decode(reader)?;
        let stages = Vec::<spirv::StageCode>::decode(reader)?;
        Ok(unsafe { Self::from_compiler_output(program, stages) })
    }
}
record!(crate::ast::ExternalName { library, symbol });
record!(TextureBinding { name, count });
record!(Reduction {
    source,
    source_word,
    target,
    target_word
});
record!(ReductionSpec {
    fields,
    ty,
    operator
});
record!(SegmentedReduction {
    values,
    head_word,
    count
});
record!(Scan {
    values,
    ty,
    operator,
    head_word
});
record!(Compaction {
    source,
    flags,
    flag_word,
    target,
    count,
    count_word
});
variants!(SortKey {0=>U32,1=>I32,2=>F32});
variants!(SortOrder {0=>Ascending,1=>Descending});
record!(Sorting {
    source,
    keys,
    target
});
variants!(Operation<ReductionLayout> {0=>Map,1=>Reduce(r),2=>CustomReduce(r),3=>CustomScan(s),4=>CustomSegmentedReduce(r),5=>SegmentedReduce(r),6=>ScanExclusive(s),7=>Compact(c),8=>StableSort(s)});
record!(PrefixCount { binding, word });
record!(PrefixRelation { domain, count });
record!(Selection { binding, word });
variants!(CollectiveOperator {0=>Sum,1=>Min,2=>Max,3=>BitAnd,4=>BitOr,5=>BitXor});
record!(ReductionLayout { fields, ty });
record!(CustomScan<ReductionLayout> { values, head_word });
record!(CustomSegmentedReduction<ReductionLayout> { values, head_word, count });
record!(SubgroupCapabilities {
    basic,
    vote,
    arithmetic,
    ballot,
    shuffle,
    shuffle_relative
});
record!(DeviceRequirements {
    ray_query,
    sorting,
    maximal_reconvergence,
    texture_indexing,
    position_fetch,
    subgroup
});
record!(ProgramLayout {
    name,
    target,
    buffers,
    accelerations,
    textures,
    uniforms,
    uniform_bytes,
    prefix_requirements,
    prefix_guarantees,
    requirements
});
record!(ExecutionStage {
    domain,
    prefix,
    selection,
    operation,
    reads,
    writes,
    inputs,
    tile_scope,
    width,
    shared_bytes,
    float32_preserve
});
record!(ExecutionPlan {
    interface,
    stages,
    indirect_buffer,
    auxiliary_buffers
});
variants!(native::NativeIntrinsic {0=>Sin,1=>Cos,2=>Floor,3=>Exp,4=>Exp2,5=>Log,6=>Log2,7=>Pow,8=>PrintU32,9=>PrintI32,10=>PrintBool,11=>PrintF32});
variants!(native::NativeSymbol {0=>Intrinsic(i),1=>External(name),2=>Function(name)});
record!(native::Relocation { offset, target });
record!(native::NativeCode {
    bytes,
    relocations,
    stack_bytes
});
variants!(native::RangeResult {0=>None,1=>Values(n),2=>UnsignedSum});
variants!(native::ContinuationCode {0=>ReductionTail(code),1=>ScanAdd(code),2=>CompactScatter(code),3=>SegmentedFinish(code),4=>Scan{prefix,apply,source,target}});
record!(native::SortCode {
    source,
    target,
    initialize,
    histogram,
    scatter,
    gather
});
variants!(native::StageCode {0=>Element{main,continuation,result},1=>Sort(code)});
record!(spirv::ShaderCode { words, ranks });
record!(spirv::StageCode {
    main,
    tail,
    scan_add,
    finish,
    dispatch,
    selector,
    sort
});
