//! 将已经检查的编译结果写成 Rust 构造表达式；DLL 不携带源码文件或文件系统加载器。
use crate::{
    ast::{Access, Target},
    ir::*,
};
use std::num::NonZeroU32;

pub(super) trait Source {
    fn source(&self) -> String;
}
macro_rules! number {
    ($($t:ty),*) => {$(impl Source for $t {
        fn source(&self) -> String { self.to_string() }
    })*};
}
number!(u32, usize, bool);
impl Source for String {
    fn source(&self) -> String {
        format!("{self:?}.into()")
    }
}
impl Source for NonZeroU32 {
    fn source(&self) -> String {
        format!("NonZeroU32::new({}).unwrap()", self.get())
    }
}
impl<T: Source> Source for Vec<T> {
    fn source(&self) -> String {
        format!("vec![{}]", list(self.iter()))
    }
}
impl<T: Source, const N: usize> Source for [T; N] {
    fn source(&self) -> String {
        format!("[{}]", list(self.iter()))
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
    ($t:ident { $($f:ident),* $(,)? }) => {impl Source for $t {
        fn source(&self) -> String {
            let Self { $($f),* } = self;
            format!(concat!(stringify!($t), "{{", $(stringify!($f), ":{},",)* "}}"), $($f.source()),*)
        }
    }};
}
macro_rules! variants {
    ($t:ident { $($v:ident $(($($a:ident),*))? $({$($f:ident),*})?),* $(,)? }) => {impl Source for $t {
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
    array
});
record!(Buffer {
    name,
    access,
    ty,
    stride,
    count
});
record!(Uniform { name, ty, offset });
variants!(Comparison {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual
});
variants!(UnaryOp {
    FloatNegate,
    SignedNegate,
    UnsignedNegate,
    LogicalNot,
    BitNot
});
variants!(BinaryOp {FAdd, FSub, FMul, FDiv, IAdd, ISub, IMul, Compare(c), And, Or, Xor,
    ShiftLeft, ShiftRightLogical, ShiftRightArithmetic});
variants!(IntegerDivisor {Constant(n), Value(r)});
variants!(IntegerDivisionResult {Quotient(r), Remainder(r), Both {quotient, remainder}});
variants!(MathOp {
    Abs,
    Floor,
    Sin,
    Cos,
    Pow,
    Exp,
    Log,
    Exp2,
    Log2,
    Sqrt,
    Min,
    Max
});
variants!(IndexedSource {Data {binding, index}, View {address, index, stride}, Tile {tile, index}});
record!(TextureSource { binding, element });
record!(TextureBinding { name, count });
variants!(Inst {
    TextureSample {result, texture, coordinate, lod}, TextureSize {result, texture, level},
    TextureLevels {dst, texture}, TextureFetch {result, texture, coordinate, level},
    LoadIndexed {dst, source, offset, word}, StoreIndexed {source, offset, word, value},
    Atomic(a), Append(a), TileBarrier, TilePublish(p), TileCollective(c), TileLength(r), TileLane(r),
    TileLoad {dst, tile, index, word}, Subgroup {dst, op, value, argument},
    ViewLoad {dst, address, index, stride, word},
    RayQuery {scene, origin, direction, minimum, maximum, result},
    Constant(r, s, v), Index(r), Uniform(r, offset), LoadLocal(r, slot), StoreLocal(slot, r),
    LoadLocalIndexed {dst, base, len, index, word}, StoreLocalIndexed {base, len, index, word, value},
    Load {dst, binding, index, word}, Store {binding, index, word, value},
    Unary {dst, op, a}, Binary {dst, op, a, b}, IntegerDivide {result, src, divisor},
    Cast {dst, src}, Math {dst, op, args}, Select {dst, condition, a, b},
    If {condition, yes, no}, Loop {condition, test, body}, Break, Continue, Scope(code), Exit
});
variants!(RayResult {Any(r), Closest(r)});
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
record!(Sorting {
    source,
    keys,
    target
});
variants!(Operation {Map, Reduce(r), CustomReduce(r), CustomScan(s), CustomSegmentedReduce(r),
    SegmentedReduce(r), ScanExclusive(s), Compact(c), StableSort(s)});
record!(PrefixCount { binding, word });
record!(PrefixRelation { domain, count });
record!(Selection { binding, word });
record!(Stage {
    domain,
    prefix,
    selection,
    operation,
    boundary_before,
    registers,
    locals,
    code,
    reads,
    writes,
    cross_access,
    tiles
});
record!(Tile { source, ty, width });
variants!(TileSource {Input {binding, word}, Published});
record!(Publication { tile, values });
record!(Append {
    accepted,
    target,
    count,
    values
});
variants!(TileOp {
    ReduceSum,
    ScanExclusiveSum,
    ReduceMin,
    ReduceMax,
    ScanExclusiveMin,
    ScanExclusiveMax,
    ReduceBitAnd,
    ReduceBitOr,
    ReduceBitXor,
    ScanExclusiveBitAnd,
    ScanExclusiveBitOr,
    ScanExclusiveBitXor
});
variants!(CollectiveOperator {
    Sum,
    Min,
    Max,
    BitAnd,
    BitOr,
    BitXor
});
record!(TileCollective { op, tile, results });
variants!(SubgroupOp {
    Lane,
    Size,
    Id,
    Elect,
    Any,
    All,
    Count,
    PrefixCount,
    BroadcastFirst,
    Shuffle,
    ShuffleXor,
    ShuffleUp,
    ShuffleDown,
    Sum,
    ExclusiveSum,
    Min,
    Max,
    BitAnd,
    BitOr,
    BitXor,
    ExclusiveBitAnd,
    ExclusiveBitOr,
    ExclusiveBitXor
});
variants!(MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    AcquireRelease
});
variants!(AtomicOp {Load, Store(r), Add(r), Sub(r), Exchange(r), CompareExchange {expected, desired, failure},
    Min(r), Max(r), And(r), Or(r), Xor(r)});
record!(Atomic {
    result,
    target,
    index,
    word,
    offset,
    op,
    order
});
variants!(AtomicTarget {Dataset(b), Tile(t)});
record!(ValueCode {
    registers,
    locals,
    code,
    inputs,
    outputs
});
record!(CustomReduction {
    fields,
    source_ty,
    ty,
    project,
    identity,
    combine
});
record!(CustomScan { values, head_word });
record!(CustomSegmentedReduction {
    values,
    head_word,
    count
});

pub(super) fn program(p: &Program) -> String {
    let Program {
        name,
        target,
        buffers,
        accelerations,
        textures,
        uniforms,
        uniform_bytes,
        source_stages,
        stages,
        prefix_requirements,
        prefix_guarantees,
    } = p;
    format!(
        "unsafe {{ ExecutableProgram::from_compiler_output({},{},{},{},{},{},{},{},{},{},{}) }}",
        name.source(),
        target.source(),
        buffers.source(),
        accelerations.source(),
        textures.source(),
        uniforms.source(),
        uniform_bytes.source(),
        source_stages.source(),
        stages.source(),
        prefix_requirements.source(),
        prefix_guarantees.source()
    )
}
