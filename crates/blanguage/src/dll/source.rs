//! 输出完整原生程序映像；运行时只装载机器码与调度布局，不携带值 IR。
use crate::{
    ExecutableProgram,
    native::{
        ContinuationCode, NativeCode, NativeIntrinsic, NativeSymbol, RangeResult, Relocation,
        SortCode as NativeSortCode, StageCode as NativeStageCode,
    },
    spirv::{ShaderCode, StageCode as ShaderStageCode},
};
use crate::{
    ast::{Access, ExternalName, Target},
    ir::*,
};
use std::collections::BTreeSet;

pub(super) trait Source {
    fn source(&self) -> String;
}
macro_rules! number {
    ($($t:ty),*) => {$(impl Source for $t {
        fn source(&self) -> String { self.to_string() }
    })*};
}
number!(u8, u32, u64, usize, bool);
impl Source for String {
    fn source(&self) -> String {
        format!("{self:?}.into()")
    }
}
impl Source for &'static str {
    fn source(&self) -> String {
        format!("{self:?}")
    }
}
impl<T: Source> Source for Vec<T> {
    fn source(&self) -> String {
        format!("vec![{}]", list(self.iter()))
    }
}
impl<T: Source> Source for Option<T> {
    fn source(&self) -> String {
        match self {
            Some(x) => format!("Some({})", x.source()),
            None => "None".into(),
        }
    }
}
impl<T: Source> Source for Box<T> {
    fn source(&self) -> String {
        format!("Box::new({})", (**self).source())
    }
}
impl<A: Source, B: Source> Source for (A, B) {
    fn source(&self) -> String {
        format!("({},{})", self.0.source(), self.1.source())
    }
}
impl<A: Source, B: Source, C: Source> Source for (A, B, C) {
    fn source(&self) -> String {
        format!(
            "({},{},{})",
            self.0.source(),
            self.1.source(),
            self.2.source()
        )
    }
}
fn list<'a, T: Source + 'a>(values: impl Iterator<Item = &'a T>) -> String {
    values.map(Source::source).collect::<Vec<_>>().join(",")
}
macro_rules! record {
    ($t:ident $(<$g:ident>)? { $($f:ident),* $(,)? }) => {impl $(<$g: Source>)? Source for $t $(<$g>)? {
        fn source(&self) -> String {
            let Self { $($f),* } = self;
            format!(concat!(stringify!($t), "{{", $(stringify!($f), ":{},",)* "}}"), $($f.source()),*)
        }
    }};
}
macro_rules! variants {
    ($t:ident $(<$g:ident>)? { $($v:ident $(($($a:ident),*))? $({$($f:ident),*})?),* $(,)? }) => {impl $(<$g: Source>)? Source for $t $(<$g>)? {
        fn source(&self) -> String {
            match self { $(Self::$v $(($($a),*))? $({$($f),*})? => {
                #[allow(unused_mut)]
                let mut out = concat!(stringify!($t), "::", stringify!($v)).to_owned();
                $(out.push('('); $(out.push_str(&$a.source()); out.push(',');)* out.push(')');)?
                $(out.push('{'); $(out.push_str(concat!(stringify!($f), ":")); out.push_str(&$f.source()); out.push(',');)* out.push('}');)?
                out
            }),* }
        }
    }};
}
variants!(Access {
    Value,
    Read,
    Write,
    InOut,
    Atomic
});
variants!(Target { Compute, Cpu, Gpu });
variants!(Scalar {
    F32,
    U32,
    I32,
    Bool
});
record!(Ty {
    name,
    arguments,
    values,
    size,
    align,
    components,
    fields,
    view,
    array,
    enumeration,
    result,
    dataset
});
record!(Buffer {
    name,
    access,
    ty,
    stride,
    count
});
record!(Uniform { name, ty, offset });
impl<T: Source> Source for [T] {
    fn source(&self) -> String {
        format!("vec![{}]", list(self.iter()))
    }
}
impl<T: Source> Source for BTreeSet<T> {
    fn source(&self) -> String {
        format!("BTreeSet::from([{}])", list(self.iter()))
    }
}
record!(ExternalName { library, symbol });
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
variants!(SortKey { U32, I32, F32 });
variants!(SortOrder {
    Ascending,
    Descending
});
record!(Sorting {
    source,
    keys,
    target
});
variants!(Operation<R> {Map, Reduce(r), CustomReduce(r), CustomScan(s), CustomSegmentedReduce(r),
    SegmentedReduce(r), ScanExclusive(s), Compact(c), StableSort(s)});
record!(PrefixCount { binding, word });
record!(PrefixRelation { domain, count });
record!(Selection { binding, word });
variants!(CollectiveOperator {
    Sum,
    Min,
    Max,
    BitAnd,
    BitOr,
    BitXor
});
record!(ReductionLayout { fields, ty });
record!(CustomScan<R> { values, head_word });
record!(CustomSegmentedReduction<R> { values, head_word, count });
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
variants!(NativeIntrinsic {
    Sin,
    Cos,
    Floor,
    Exp,
    Exp2,
    Log,
    Log2,
    Pow,
    PrintU32,
    PrintI32,
    PrintBool,
    PrintF32
});
variants!(NativeSymbol { Intrinsic(i), External(name) });
record!(Relocation { offset, target });
record!(NativeCode {
    bytes,
    relocations,
    stack_bytes
});
variants!(RangeResult { None, Values(n), UnsignedSum });
variants!(ContinuationCode { ReductionTail(code), ScanAdd(code), CompactScatter(code), SegmentedFinish(code), Scan { prefix, apply, source, target } });
record!(NativeSortCode {
    source,
    target,
    initialize,
    histogram,
    scatter,
    gather
});
variants!(NativeStageCode { Element { main, continuation, result }, Sort(code) });
record!(ShaderCode { words, ranks });
record!(ShaderStageCode {
    main,
    tail,
    scan_add,
    finish,
    dispatch,
    selector,
    sort
});

pub(super) fn constructor(p: &ExecutableProgram) -> Result<String, Box<dyn std::error::Error>> {
    let cpu = if p.target() == Target::Gpu {
        "return Err(std::io::Error::other(\"GPU 专属程序不能提交给 CPU\").into())".into()
    } else {
        let image = crate::native::compile_program(p.clone()).map_err(std::io::Error::other)?;
        format!(
            "LibraryProgram::Cpu(CpuProgram::from_image(unsafe {{ NativeProgramImage::<ExecutableProgram>::from_compiler_output({}, {}) }})?)",
            image.program().source(),
            image.stages().source()
        )
    };
    let gpu = if p.target() == Target::Cpu {
        "return Err(std::io::Error::other(\"CPU 专属程序不能提交给 GPU\").into())".into()
    } else {
        let image = crate::spirv::compile_program(p.clone()).map_err(std::io::Error::other)?;
        format!(
            "{{ let image = unsafe {{ ShaderProgramImage::<ExecutableProgram>::from_compiler_output({}, {}) }}; let device = Device::for_program(image.program())?; let program = device.load(image)?; LibraryProgram::Gpu {{ device, program }} }}",
            image.program().source(),
            image.stages().source()
        )
    };
    Ok(format!(
        "match backend {{0 => {{ {cpu} }}, 1 => {{ {gpu} }}, _ => return Err(std::io::Error::other(\"backend 必须为 CPU(0) 或 GPU(1)\").into()),}}"
    ))
}

pub(crate) fn application_source(
    resources: &crate::application::Resources,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write;
    let mut images = Vec::new();
    let mut functions = String::new();
    for index in 0..resources.types.len() {
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn blink_allocate_{index}(frame: *mut Frame, count: u32) -> u32 {{\n let result = unsafe {{ &mut *frame }}.allocate({index}, count);\n unsafe {{ finish(frame, result) }}\n}}"
        )?;
    }
    for (index, (target, program)) in resources.programs.iter().enumerate() {
        let template = crate::TemplateProgram(program.clone());
        let image = if *target == Target::Cpu {
            let image = crate::native::compile_program(template).map_err(std::io::Error::other)?;
            format!(
                "ProgramImage::Cpu(unsafe {{ NativeProgramImage::<TemplateProgram>::from_compiler_output({}, {}) }})",
                image.program().source(),
                image.stages().source()
            )
        } else {
            let image = crate::spirv::compile_program(template).map_err(std::io::Error::other)?;
            format!(
                "ProgramImage::Gpu(unsafe {{ ShaderProgramImage::<TemplateProgram>::from_compiler_output({}, {}) }})",
                image.program().source(),
                image.stages().source()
            )
        };
        images.push(image);
        let mut params = vec!["frame: *mut Frame".to_string()];
        let handles = (0..program.buffers.len())
            .map(|i| {
                params.push(format!("data_{i}: u32"));
                format!("data_{i}")
            })
            .collect::<Vec<_>>();
        let mut words = Vec::new();
        for uniform in &program.uniforms {
            for (scalar, _) in &uniform.ty.components {
                let name = format!("value_{}", words.len());
                params.push(format!(
                    "{name}: {}",
                    match scalar {
                        Scalar::F32 => "f32",
                        Scalar::I32 => "i32",
                        _ => "u32",
                    }
                ));
                words.push(match scalar {
                    Scalar::F32 => format!("Value::F32({name})"),
                    Scalar::I32 => format!("Value::I32({name})"),
                    Scalar::Bool => format!("Value::Bool({name} != 0)"),
                    Scalar::U32 => format!("Value::U32({name})"),
                });
            }
        }
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn blink_run_{index}({}) {{\n let result = unsafe {{ &mut *frame }}.run({index}, &[{}], &[{}]);\n unsafe {{ finish(frame, result) }}\n}}",
            params.join(","),
            handles.join(","),
            words.join(",")
        )?;
    }
    Ok(include_str!("../application/runtime.rs.in")
        .replace("@TYPES@", &resources.types.source())
        .replace("@IMAGES@", &format!("vec![{}]", images.join(",")))
        .replace("@FUNCTIONS@", &functions))
}
