use crate::ast::{Access, Target};
use std::num::NonZeroU32;
mod external;
pub use external::{ExternalArgument, ExternalCall, ExternalReference, ExternalStorage};
mod execution;
pub use execution::{
    DeviceRequirements, ExecutionPlan, ExecutionStage, ProgramInterface, ProgramLayout,
    ReductionLayout,
};
mod atomic;
pub use atomic::{Atomic, AtomicOp, AtomicTarget, MemoryOrder};
mod custom_reduce;
pub use custom_reduce::{
    CustomReduction, CustomScan, CustomSegmentedReduction, ReductionKernel, ValueCode,
};

pub type Reg = usize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scalar {
    F32,
    U32,
    I32,
    Bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ty {
    pub name: String,
    pub arguments: Vec<Ty>,
    pub values: Vec<u32>,
    pub size: u32,
    pub align: u32,
    pub components: Vec<(Scalar, u32)>,
    pub fields: Vec<(String, u32, Box<Ty>)>,
    pub view: Option<Box<Ty>>,
    pub array: Option<Box<Ty>>,
    pub enumeration: bool,
    pub result: bool,
    pub dataset: Option<Box<Ty>>,
}
impl Ty {
    pub(crate) fn syntax(&self) -> crate::ast::TypeName {
        if let Some(element) = &self.dataset {
            let mut ty = element.syntax();
            ty.dataset = true;
            ty.count = self.values.first().map(|v| crate::ast::Shape::Literal(*v));
            return ty;
        }
        if let Some(element) = &self.array {
            let mut ty = crate::ast::TypeName::named("array");
            ty.arguments = vec![
                crate::ast::GenericArgument::Type(element.syntax()),
                crate::ast::GenericArgument::Value(crate::ast::Shape::Literal(self.values[0])),
            ];
            return ty;
        }
        if let Some(element) = &self.view {
            let mut ty = element.syntax();
            ty.view = true;
            return ty;
        }
        let mut ty = crate::ast::TypeName::named(
            self.name
                .split_once('<')
                .map_or(self.name.as_str(), |(base, _)| base),
        );
        ty.arguments = self
            .arguments
            .iter()
            .map(|ty| crate::ast::GenericArgument::Type(ty.syntax()))
            .chain(self.values.iter().map(|value| {
                crate::ast::GenericArgument::Value(crate::ast::Shape::Literal(*value))
            }))
            .collect();
        ty
    }
    pub fn owns_resources(&self) -> bool {
        self.dataset.is_some() || (self.result && self.arguments.iter().any(Self::owns_resources)) || self.fields.iter().any(|(_,_,ty)|ty.owns_resources())
    }
    pub fn owned_dataset(element: Self, count: Option<u32>) -> Self {
        Self {
            name: format!(
                "dataset<{}{}>",
                element.name,
                count.map(|v| format!(",{v}")).unwrap_or_default()
            ),
            arguments: Vec::new(),
            values: count.into_iter().collect(),
            size: 8,
            align: 4,
            components: vec![(Scalar::U32, 0), (Scalar::U32, 4)],
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            dataset: Some(Box::new(element)),
        }
    }
    pub fn result(ok: Self, error: Self) -> Option<Self> {
        let align = ok.align.max(error.align).max(4);
        let offset = align_up(4, align);
        let size = u32::try_from(
            (u64::from(offset) + u64::from(ok.size.max(error.size))).div_ceil(u64::from(align))
                * u64::from(align),
        )
        .ok()?;
        Some(Self {
            name: format!("result<{},{}>", ok.name, error.name),
            arguments: vec![ok, error],
            values: Vec::new(),
            size,
            align,
            components: std::iter::once((Scalar::Bool, 0))
                .chain((offset..size).step_by(4).map(|at| (Scalar::U32, at)))
                .collect(),
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: true,
            dataset: None,
        })
    }
    pub fn unit() -> Self {
        Self {
            name: "void".into(),
            arguments: Vec::new(),
            values: Vec::new(),
            size: 0,
            align: 1,
            components: Vec::new(),
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            dataset: None,
        }
    }
    pub fn scalar(s: Scalar) -> Self {
        Self {
            name: match s {
                Scalar::F32 => "f32",
                Scalar::U32 => "u32",
                Scalar::I32 => "i32",
                Scalar::Bool => "bool",
            }
            .into(),
            arguments: Vec::new(),
            values: Vec::new(),
            size: 4,
            align: 4,
            components: vec![(s, 0)],
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            dataset: None,
        }
    }
    pub fn vector(s: Scalar, n: usize) -> Self {
        if n == 1 {
            return Self::scalar(s);
        }
        Self {
            name: format!(
                "{}{}",
                match s {
                    Scalar::F32 => "float",
                    Scalar::U32 => "uint",
                    Scalar::I32 => "int",
                    Scalar::Bool => "bool",
                },
                n
            ),
            arguments: Vec::new(),
            values: Vec::new(),
            size: n as u32 * 4,
            align: if n == 2 { 8 } else { 16 },
            components: (0..n).map(|i| (s, i as u32 * 4)).collect(),
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            dataset: None,
        }
    }
    pub fn matrix(rows: usize, columns: usize) -> Self {
        let row = Self::vector(Scalar::F32, columns);
        let stride = align_up(row.size, row.align);
        Self {
            name: format!("float{rows}x{columns}"),
            arguments: Vec::new(),
            values: Vec::new(),
            size: stride * rows as u32,
            align: row.align,
            components: (0..rows)
                .flat_map(|r| {
                    row.components
                        .iter()
                        .map(move |(s, o)| (*s, r as u32 * stride + o))
                })
                .collect(),
            fields: (0..rows)
                .map(|r| (format!("r{r}"), r as u32 * stride, Box::new(row.clone())))
                .collect(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            dataset: None,
        }
    }
    pub fn matrix_shape(&self) -> Option<(usize, usize)> {
        matrix_shape(&self.name)
    }
    pub fn builtin(s: &str) -> Option<Self> {
        if let Some((rows, columns)) = matrix_shape(s) {
            return Some(Self::matrix(rows, columns));
        }
        Some(match s {
            "f32" | "float" => Self::scalar(Scalar::F32),
            "u32" | "uint" => Self::scalar(Scalar::U32),
            "i32" | "int" => Self::scalar(Scalar::I32),
            "bool" => Self::scalar(Scalar::Bool),
            "float2" => Self::vector(Scalar::F32, 2),
            "float3" => Self::vector(Scalar::F32, 3),
            "float4" => Self::vector(Scalar::F32, 4),
            "uint2" => Self::vector(Scalar::U32, 2),
            "uint3" => Self::vector(Scalar::U32, 3),
            "uint4" => Self::vector(Scalar::U32, 4),
            "int2" => Self::vector(Scalar::I32, 2),
            "int3" => Self::vector(Scalar::I32, 3),
            "int4" => Self::vector(Scalar::I32, 4),
            "RayHit" => Self {
                name: "RayHit".into(),
                arguments: Vec::new(),
                values: Vec::new(),
                size: 176,
                align: 16,
                view: None,
                array: None,
                enumeration: false,
                result: false,
                dataset: None,
                components: vec![
                    (Scalar::Bool, 0),
                    (Scalar::F32, 4),
                    (Scalar::F32, 8),
                    (Scalar::F32, 12),
                    (Scalar::U32, 16),
                    (Scalar::U32, 20),
                ]
                .into_iter()
                .chain((0..3).flat_map(|v| (0..3).map(move |c| (Scalar::F32, 32 + v * 16 + c * 4))))
                .chain((0..24).map(|c| (Scalar::F32, 80 + c * 4)))
                .collect(),
                fields: vec![
                    ("exists".into(), 0, Box::new(Self::scalar(Scalar::Bool))),
                    ("distance".into(), 4, Box::new(Self::scalar(Scalar::F32))),
                    (
                        "barycentric".into(),
                        8,
                        Box::new(Self::vector(Scalar::F32, 2)),
                    ),
                    ("instance".into(), 16, Box::new(Self::scalar(Scalar::U32))),
                    ("primitive".into(), 20, Box::new(Self::scalar(Scalar::U32))),
                    ("vertex0".into(), 32, Box::new(Self::vector(Scalar::F32, 3))),
                    ("vertex1".into(), 48, Box::new(Self::vector(Scalar::F32, 3))),
                    ("vertex2".into(), 64, Box::new(Self::vector(Scalar::F32, 3))),
                    ("object_to_world".into(), 80, Box::new(Self::matrix(3, 4))),
                    ("world_to_object".into(), 128, Box::new(Self::matrix(3, 4))),
                ],
            },
            _ => return None,
        })
    }
    pub fn is_numeric(&self) -> bool {
        !self.owns_resources()
            && !self.enumeration
            && !self.result
            && self.view.is_none()
            && self.array.is_none()
            && self.fields.is_empty()
            && self.components.iter().all(|x| x.0 != Scalar::Bool)
    }
    pub fn contains_arrays(&self) -> bool {
        (self.result && self.arguments.iter().any(|ty| ty.contains_arrays()))
            || self.array.is_some()
            || self.fields.iter().any(|(_, _, t)| t.contains_arrays())
    }
    pub fn contains_views(&self) -> bool {
        (self.result && self.arguments.iter().any(|ty| ty.contains_views()))
            || self.view.is_some()
            || self.array.as_ref().is_some_and(|t| t.contains_views())
            || self.fields.iter().any(|(_, _, t)| t.contains_views())
    }
    pub fn view_fields(&self) -> Vec<(u32, &Ty)> {
        if let Some(element) = &self.view {
            return vec![(0, element.as_ref())];
        }
        if let Some(element) = &self.array {
            let stride = align_up(element.size, element.align);
            return (0..self.values[0])
                .flat_map(|i| {
                    element
                        .view_fields()
                        .into_iter()
                        .map(move |(at, ty)| (i * stride + at, ty))
                })
                .collect();
        }
        self.fields
            .iter()
            .flat_map(|(_, offset, t)| {
                t.view_fields()
                    .into_iter()
                    .map(move |(o, element)| (offset + o, element))
            })
            .collect()
    }
    pub fn field(&self, path: &str) -> Option<(u32, &Ty)> {
        let mut ty = self;
        let mut offset = 0;
        for part in path.split('.') {
            let split = part.find('[').unwrap_or(part.len());
            let (name, mut indices) = part.split_at(split);
            if !name.is_empty() {
                let (_, at, field) = ty.fields.iter().find(|(n, _, _)| n == name)?;
                offset += at;
                ty = field.as_ref();
            } else if indices.is_empty() {
                return None;
            }
            while !indices.is_empty() {
                let end = indices.find(']')?;
                let index: u32 = indices.strip_prefix('[')?[..end - 1].parse().ok()?;
                let element = ty.array.as_deref()?;
                if index >= ty.values[0] {
                    return None;
                }
                offset += index * align_up(element.size, element.align);
                ty = element;
                indices = &indices[end + 1..];
            }
        }
        Some((offset, ty))
    }
}
fn matrix_shape(name: &str) -> Option<(usize, usize)> {
    match name.as_bytes() {
        [
            b'f',
            b'l',
            b'o',
            b'a',
            b't',
            r @ b'2'..=b'4',
            b'x',
            c @ b'2'..=b'4',
        ] => Some(((r - b'0') as usize, (c - b'0') as usize)),
        _ => None,
    }
}
#[derive(Clone, Debug)]
pub struct Buffer {
    pub name: String,
    pub access: Access,
    pub ty: Ty,
    pub stride: u32,
    pub count: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct Uniform {
    pub name: String,
    pub ty: Ty,
    pub offset: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    FloatNegate,
    SignedNegate,
    UnsignedNegate,
    LogicalNot,
    BitNot,
    Bitcast,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    FAdd,
    FSub,
    FMul,
    FDiv,
    IAdd,
    ISub,
    IMul,
    Compare(Comparison),
    And,
    Or,
    Xor,
    ShiftLeft,
    ShiftRightLogical,
    ShiftRightArithmetic,
}
impl BinaryOp {
    pub fn from_source(op: &str, scalar: Scalar) -> Option<Self> {
        use BinaryOp::*;
        Some(match op {
            "+" => {
                if scalar == Scalar::F32 {
                    FAdd
                } else {
                    IAdd
                }
            }
            "-" => {
                if scalar == Scalar::F32 {
                    FSub
                } else {
                    ISub
                }
            }
            "*" => {
                if scalar == Scalar::F32 {
                    FMul
                } else {
                    IMul
                }
            }
            "/" if scalar == Scalar::F32 => FDiv,
            "==" => Compare(Comparison::Equal),
            "!=" => Compare(Comparison::NotEqual),
            "<" => Compare(Comparison::Less),
            ">" => Compare(Comparison::Greater),
            "<=" => Compare(Comparison::LessEqual),
            ">=" => Compare(Comparison::GreaterEqual),
            "&" => And,
            "|" => Or,
            "^" => Xor,
            "<<" => ShiftLeft,
            ">>" => {
                if scalar == Scalar::I32 {
                    ShiftRightArithmetic
                } else {
                    ShiftRightLogical
                }
            }
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntegerDivisor {
    Constant(NonZeroU32),
    /// 当前控制域内已证明非零的整数 SSA 值；不得移出该证明成立的控制流。
    Value(Reg),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerDivisionOp {
    Quotient,
    Remainder,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerDivisionResult {
    Quotient(Reg),
    Remainder(Reg),
    Both { quotient: Reg, remainder: Reg },
}
impl IntegerDivisionResult {
    pub fn quotient(self) -> Option<Reg> {
        match self {
            Self::Quotient(r) | Self::Both { quotient: r, .. } => Some(r),
            Self::Remainder(_) => None,
        }
    }
    pub fn remainder(self) -> Option<Reg> {
        match self {
            Self::Remainder(r) | Self::Both { remainder: r, .. } => Some(r),
            Self::Quotient(_) => None,
        }
    }
    pub fn from_parts(quotient: Option<Reg>, remainder: Option<Reg>) -> Option<Self> {
        match (quotient, remainder) {
            (Some(quotient), Some(remainder)) => Some(Self::Both {
                quotient,
                remainder,
            }),
            (Some(r), None) => Some(Self::Quotient(r)),
            (None, Some(r)) => Some(Self::Remainder(r)),
            (None, None) => None,
        }
    }
    pub fn outputs(self) -> impl Iterator<Item = (IntegerDivisionOp, Reg)> {
        self.quotient()
            .map(|r| (IntegerDivisionOp::Quotient, r))
            .into_iter()
            .chain(self.remainder().map(|r| (IntegerDivisionOp::Remainder, r)))
    }
    pub fn map(&mut self, f: &impl Fn(Reg) -> Reg) {
        match self {
            Self::Quotient(r) | Self::Remainder(r) => *r = f(*r),
            Self::Both {
                quotient,
                remainder,
            } => {
                *quotient = f(*quotient);
                *remainder = f(*remainder);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathOp {
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
    Max,
}
impl MathOp {
    pub fn from_source(name: &str) -> Option<Self> {
        Some(match name {
            "abs" => Self::Abs,
            "floor" => Self::Floor,
            "sin" => Self::Sin,
            "cos" => Self::Cos,
            "pow" => Self::Pow,
            "exp" => Self::Exp,
            "log" => Self::Log,
            "exp2" => Self::Exp2,
            "log2" => Self::Log2,
            "sqrt" => Self::Sqrt,
            "min" => Self::Min,
            "max" => Self::Max,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug)]
pub enum IndexedSource {
    Data {
        binding: usize,
        index: Reg,
    },
    View {
        address: [Reg; 2],
        index: Reg,
        stride: u32,
    },
    Tile {
        tile: usize,
        index: Reg,
    },
}
impl IndexedSource {
    pub(crate) fn operands(&self) -> Vec<Reg> {
        match self {
            Self::Data { index, .. } | Self::Tile { index, .. } => vec![*index],
            Self::View { address, index, .. } => vec![address[0], address[1], *index],
        }
    }
    pub(crate) fn map(&mut self, r: &impl Fn(Reg) -> Reg) {
        match self {
            Self::Data { index, .. } | Self::Tile { index, .. } => *index = r(*index),
            Self::View { address, index, .. } => {
                address.iter_mut().for_each(|v| *v = r(*v));
                *index = r(*index);
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TextureSource {
    pub binding: usize,
    pub element: Option<Reg>,
}
#[derive(Clone, Debug)]
pub struct TextureBinding {
    pub name: String,
    pub count: Option<u32>,
}
impl TextureBinding {
    pub fn elements(&self) -> u32 {
        self.count.unwrap_or(1)
    }
}
#[derive(Clone, Debug)]
pub enum Inst {
    TextureSample {
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: [Reg; 2],
        lod: Reg,
    },
    TextureSize {
        result: [Reg; 2],
        texture: TextureSource,
        level: Reg,
    },
    TextureLevels {
        dst: Reg,
        texture: TextureSource,
    },
    /// CPU采样降低阶段读取原资源描述符中的逐轴寻址位；不属于B可见指令。
    TextureAddress {
        dst: Reg,
        texture: TextureSource,
    },
    TextureFormat {
        dst: Reg,
        texture: TextureSource,
    },
    TextureRead {
        dst: Reg,
        texture: TextureSource,
        coordinate: [Reg; 2],
        level: Reg,
        byte: bool,
    },
    TextureFetch {
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: [Reg; 2],
        level: Reg,
    },
    LoadIndexed {
        dst: Reg,
        source: IndexedSource,
        offset: Reg,
        word: u32,
    },
    StoreIndexed {
        source: IndexedSource,
        offset: Reg,
        word: u32,
        value: Reg,
    },
    Atomic(Atomic),
    Append(Append),
    TileBarrier,
    TilePublish(Publication),
    TileCollective(TileCollective),
    TileLength(Reg),
    TileLane(Reg),
    TileLoad {
        dst: Reg,
        tile: usize,
        index: Reg,
        word: u32,
    },
    Subgroup {
        dst: Reg,
        op: SubgroupOp,
        value: Option<Reg>,
        argument: Option<Reg>,
    },
    Print {
        dst: Reg,
        value: Reg,
    },
    ExternalCall(ExternalCall),
    ViewLoad {
        dst: Reg,
        address: [Reg; 2],
        index: Reg,
        stride: u32,
        word: u32,
    },
    RayQuery {
        scene: usize,
        origin: [Reg; 3],
        direction: [Reg; 3],
        minimum: Reg,
        maximum: Reg,
        result: RayResult,
    },
    Constant(Reg, Scalar, u32),
    Index(Reg),
    Uniform(Reg, u32),
    LoadLocal(Reg, usize),
    StoreLocal(usize, Reg),
    LoadLocalIndexed {
        dst: Reg,
        base: usize,
        len: usize,
        index: Reg,
        word: u32,
    },
    StoreLocalIndexed {
        base: usize,
        len: usize,
        index: Reg,
        word: u32,
        value: Reg,
    },
    Load {
        dst: Reg,
        binding: usize,
        index: Reg,
        word: u32,
    },
    Store {
        binding: usize,
        index: Reg,
        word: u32,
        value: Reg,
    },
    Unary {
        dst: Reg,
        op: UnaryOp,
        a: Reg,
    },
    Binary {
        dst: Reg,
        op: BinaryOp,
        a: Reg,
        b: Reg,
    },
    IntegerDivide {
        result: IntegerDivisionResult,
        src: Reg,
        divisor: IntegerDivisor,
    },
    Cast {
        dst: Reg,
        src: Reg,
    },
    Math {
        dst: Reg,
        op: MathOp,
        args: Vec<Reg>,
    },
    Select {
        dst: Reg,
        condition: Reg,
        a: Reg,
        b: Reg,
    },
    If {
        condition: Reg,
        yes: Vec<Inst>,
        no: Vec<Inst>,
    },
    Loop {
        condition: Vec<Inst>,
        test: Reg,
        body: Vec<Inst>,
    },
    Break,
    Continue,
    // 当前静态展开函数的退出域；Exit 只离开最近的 Scope，不结束 invocation。
    Scope(Vec<Inst>),
    Exit,
}
#[derive(Clone, Debug)]
pub enum RayResult {
    Any(Reg),
    Closest([Reg; 39]), // 基础命中、物体空间顶点、object_to_world、world_to_object。
}
impl RayResult {
    pub fn registers(&self) -> &[Reg] {
        match self {
            Self::Any(r) => std::slice::from_ref(r),
            Self::Closest(r) => r,
        }
    }
    pub fn map(&mut self, f: &impl Fn(Reg) -> Reg) {
        match self {
            Self::Any(r) => *r = f(*r),
            Self::Closest(r) => r.iter_mut().for_each(|r| *r = f(*r)),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Reduction {
    pub source: usize,
    pub source_word: u32,
    pub target: usize,
    pub target_word: u32,
}
#[derive(Clone, Debug)]
pub struct ReductionSpec {
    pub fields: Reduction,
    pub ty: Ty,
    pub operator: CollectiveOperator,
}
#[derive(Clone, Debug)]
pub struct SegmentedReduction {
    pub values: ReductionSpec,
    pub head_word: u32,
    pub count: PrefixCount,
}
impl SegmentedReduction {
    pub fn summary_words(&self) -> usize {
        self.values.ty.components.len() + 2
    }
}
#[derive(Clone, Debug)]
pub struct Scan {
    pub values: Reduction,
    pub ty: Ty,
    pub operator: CollectiveOperator,
    /// 同一不可变源记录的 bool 段起点；None 表示普通连续扫描。
    pub head_word: Option<u32>,
}
impl Scan {
    pub fn summary_words(&self) -> usize {
        self.ty.components.len() + usize::from(self.head_word.is_some())
    }
    pub fn counts(values: Reduction) -> Self {
        Self {
            values,
            ty: Ty::scalar(Scalar::U32),
            operator: CollectiveOperator::Sum,
            head_word: None,
        }
    }
    pub fn needs_root_add(&self, program: &dyn ProgramInterface) -> bool {
        self.head_word.is_some()
            || self.values.target_word != 0
            || program.buffers()[self.values.target].stride != self.ty.components.len() as u32 * 4
    }
}
#[derive(Clone, Debug)]
pub struct Compaction {
    pub source: usize,
    pub flags: usize,
    pub flag_word: u32,
    pub target: usize,
    pub count: usize,
    pub count_word: u32,
}
impl Compaction {
    pub fn scan(&self) -> Reduction {
        Reduction {
            source: self.flags,
            source_word: self.flag_word,
            target: self.target,
            target_word: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    U32,
    I32,
    F32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}
#[derive(Clone, Debug)]
pub struct Sorting {
    pub source: usize,
    pub keys: Vec<(u32, SortKey, SortOrder)>,
    pub target: usize,
}
#[derive(Clone, Debug)]
pub enum Operation<R = CustomReduction> {
    Map,
    Reduce(ReductionSpec),
    CustomReduce(R),
    CustomScan(CustomScan<R>),
    CustomSegmentedReduce(CustomSegmentedReduction<R>),
    SegmentedReduce(SegmentedReduction),
    ScanExclusive(Scan),
    Compact(Compaction),
    StableSort(Sorting),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrefixCount {
    pub binding: usize,
    pub word: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrefixRelation {
    pub domain: usize,
    pub count: PrefixCount,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub binding: usize,
    pub word: u32,
}
impl Selection {
    pub fn scan(self, target: usize) -> Reduction {
        Reduction {
            source: self.binding,
            source_word: self.word,
            target,
            target_word: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Stage {
    pub domain: usize,
    pub prefix: Option<PrefixCount>,
    pub selection: Option<Selection>,
    pub operation: Operation,
    pub boundary_before: bool,
    pub registers: Vec<Scalar>,
    pub locals: Vec<Scalar>,
    pub code: Vec<Inst>,
    pub reads: Vec<usize>,
    pub writes: Vec<usize>,
    pub cross_access: Vec<usize>,
    pub tiles: Vec<Tile>,
}
#[derive(Clone, Debug)]
pub struct Tile {
    pub source: TileSource,
    pub ty: Ty,
    pub width: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileSource {
    Input { binding: usize, word: u32 },
    Published,
}
#[derive(Clone, Debug)]
pub struct Publication {
    pub tile: usize,
    pub values: Vec<(u32, Reg)>,
}
/// 一次有界接纳包含预留与完整记录写入；不向源码泄露可散写票号。
#[derive(Clone, Debug)]
pub struct Append {
    pub accepted: Reg,
    pub target: usize,
    pub count: PrefixCount,
    pub values: Vec<(u32, Reg)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileOp {
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
    ScanExclusiveBitXor,
}
impl TileOp {
    pub fn from_source(name: &str) -> Option<Self> {
        Some(match name {
            "tile_reduce_sum" => Self::ReduceSum,
            "tile_scan_exclusive_sum" => Self::ScanExclusiveSum,
            "tile_reduce_min" => Self::ReduceMin,
            "tile_reduce_max" => Self::ReduceMax,
            "tile_scan_exclusive_min" => Self::ScanExclusiveMin,
            "tile_scan_exclusive_max" => Self::ScanExclusiveMax,
            "tile_reduce_bit_and" => Self::ReduceBitAnd,
            "tile_reduce_bit_or" => Self::ReduceBitOr,
            "tile_reduce_bit_xor" => Self::ReduceBitXor,
            "tile_scan_exclusive_bit_and" => Self::ScanExclusiveBitAnd,
            "tile_scan_exclusive_bit_or" => Self::ScanExclusiveBitOr,
            "tile_scan_exclusive_bit_xor" => Self::ScanExclusiveBitXor,
            _ => return None,
        })
    }
    pub fn operator(self) -> CollectiveOperator {
        match self {
            Self::ReduceSum | Self::ScanExclusiveSum => CollectiveOperator::Sum,
            Self::ReduceMin | Self::ScanExclusiveMin => CollectiveOperator::Min,
            Self::ReduceMax | Self::ScanExclusiveMax => CollectiveOperator::Max,
            Self::ReduceBitAnd | Self::ScanExclusiveBitAnd => CollectiveOperator::BitAnd,
            Self::ReduceBitOr | Self::ScanExclusiveBitOr => CollectiveOperator::BitOr,
            Self::ReduceBitXor | Self::ScanExclusiveBitXor => CollectiveOperator::BitXor,
        }
    }
    pub fn exclusive(self) -> bool {
        matches!(
            self,
            Self::ScanExclusiveSum
                | Self::ScanExclusiveMin
                | Self::ScanExclusiveMax
                | Self::ScanExclusiveBitAnd
                | Self::ScanExclusiveBitOr
                | Self::ScanExclusiveBitXor
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectiveOperator {
    Sum,
    Min,
    Max,
    BitAnd,
    BitOr,
    BitXor,
}
impl CollectiveOperator {
    pub fn from_scan_source(name: &str) -> Option<Self> {
        Self::scan_operator(name.strip_prefix("scan_exclusive_")?)
    }
    fn scan_operator(suffix: &str) -> Option<Self> {
        match suffix {
            "sum" => Some(Self::Sum),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "bit_and" => Some(Self::BitAnd),
            "bit_or" => Some(Self::BitOr),
            "bit_xor" => Some(Self::BitXor),
            _ => None,
        }
    }
    pub fn from_segmented_scan_source(name: &str) -> Option<Self> {
        Self::scan_operator(name.strip_prefix("scan_segmented_exclusive_")?)
    }
    pub fn from_reduction_source(name: &str) -> Option<Self> {
        Self::scan_operator(name.strip_prefix("reduce_")?)
    }
    pub fn from_segmented_reduction_source(name: &str) -> Option<Self> {
        Self::scan_operator(name.strip_prefix("reduce_segmented_")?)
    }
    pub fn accepts(self, scalar: Scalar) -> bool {
        match self {
            Self::BitAnd | Self::BitOr | Self::BitXor => {
                matches!(scalar, Scalar::U32 | Scalar::I32)
            }
            Self::Sum | Self::Min | Self::Max => scalar != Scalar::Bool,
        }
    }
    pub fn identity(self, scalar: Scalar) -> u32 {
        match self {
            Self::Sum | Self::BitOr | Self::BitXor => 0,
            Self::BitAnd => u32::MAX,
            Self::Min => {
                if scalar == Scalar::F32 {
                    f32::INFINITY.to_bits()
                } else if scalar == Scalar::I32 {
                    i32::MAX as u32
                } else {
                    u32::MAX
                }
            }
            Self::Max => {
                if scalar == Scalar::F32 {
                    f32::NEG_INFINITY.to_bits()
                } else if scalar == Scalar::I32 {
                    i32::MIN as u32
                } else {
                    0
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct TileCollective {
    pub op: TileOp,
    pub tile: usize,
    pub results: Vec<(u32, Reg)>,
}
mod tile;
pub use tile::SharedWord;
pub(crate) use tile::{PrivateSlot, TileStep, live_before, loop_live, private_spans};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubgroupOp {
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
    ExclusiveBitXor,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SubgroupCapabilities {
    pub basic: bool,
    pub vote: bool,
    pub arithmetic: bool,
    pub ballot: bool,
    pub shuffle: bool,
    pub shuffle_relative: bool,
}
impl Stage {
    pub(crate) fn has_external_effects(&self) -> bool {
        fn contains(code: &[Inst]) -> bool {
            code.iter().any(|inst| match inst {
                Inst::Print { .. } | Inst::ExternalCall(_) => true,
                Inst::Scope(body) => contains(body),
                Inst::If { yes, no, .. } => contains(yes) || contains(no),
                Inst::Loop {
                    condition, body, ..
                } => contains(condition) || contains(body),
                _ => false,
            })
        }
        contains(&self.code)
    }
    /// 对实际消费的命中字段发射设备读取；未消费的顶点和矩阵不生成读取。
    pub fn ray_result_queries(
        &self,
        range: std::ops::Range<usize>,
    ) -> std::collections::BTreeSet<Reg> {
        fn visit(
            code: &[Inst],
            used: &mut std::collections::BTreeSet<Reg>,
            queries: &mut Vec<[Reg; 39]>,
        ) {
            for inst in code {
                used.extend(inst.operands());
                match inst {
                    Inst::RayQuery {
                        result: RayResult::Closest(regs),
                        ..
                    } => queries.push(*regs),
                    Inst::Scope(body) => visit(body, used, queries),
                    Inst::If { yes, no, .. } => {
                        visit(yes, used, queries);
                        visit(no, used, queries);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        visit(condition, used, queries);
                        visit(body, used, queries);
                    }
                    _ => {}
                }
            }
        }
        let mut used = std::collections::BTreeSet::new();
        let mut queries = Vec::new();
        visit(&self.code, &mut used, &mut queries);
        queries
            .into_iter()
            .filter(|r| r[range.clone()].iter().any(|r| used.contains(r)))
            .map(|r| r[0])
            .collect()
    }
    pub fn requires_maximal_reconvergence(&self) -> bool {
        fn uses(code: &[Inst]) -> bool {
            code.iter().any(|i| match i {
                Inst::Append(_) => true,
                Inst::Scope(body) => uses(body),
                Inst::If { yes, no, .. } => uses(yes) || uses(no),
                Inst::Loop {
                    condition, body, ..
                } => uses(condition) || uses(body),
                _ => false,
            })
        }
        uses(&self.code)
    }
    pub fn requires_float32_preserve(&self) -> bool {
        fn arithmetic(code: &[Inst], registers: &[Scalar]) -> bool {
            code.iter().any(|inst| match inst {
                Inst::Math { op, .. } => *op != MathOp::Abs,
                Inst::Binary { a, .. } => registers[*a] == Scalar::F32,
                Inst::Cast { dst, src } => {
                    registers[*dst] == Scalar::F32 || registers[*src] == Scalar::F32
                }
                Inst::TileCollective(c) => {
                    c.results.iter().any(|(_, r)| registers[*r] == Scalar::F32)
                }
                Inst::Subgroup { dst, op, .. } => {
                    registers[*dst] == Scalar::F32
                        && matches!(
                            op,
                            SubgroupOp::Sum
                                | SubgroupOp::ExclusiveSum
                                | SubgroupOp::Min
                                | SubgroupOp::Max
                        )
                }
                Inst::Scope(body) => arithmetic(body, registers),
                Inst::If { yes, no, .. } => arithmetic(yes, registers) || arithmetic(no, registers),
                Inst::Loop {
                    condition, body, ..
                } => arithmetic(condition, registers) || arithmetic(body, registers),
                _ => false,
            })
        }
        matches!(&self.operation, Operation::Reduce(r) if r.ty.components[0].0 == Scalar::F32)
            || matches!(&self.operation, Operation::CustomReduce(r) | Operation::CustomScan(CustomScan { values:r, .. }) | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values:r, .. }) if r.identity.registers.iter().chain(&r.combine.registers).chain(r.project.iter().flat_map(|p|&p.registers)).any(|s| *s == Scalar::F32))
            || matches!(&self.operation, Operation::SegmentedReduce(r) if r.values.ty.components[0].0 == Scalar::F32)
            || matches!(&self.operation, Operation::ScanExclusive(s) if s.ty.components[0].0 == Scalar::F32)
            || arithmetic(&self.code, &self.registers)
    }
    fn tile_usage(&self) -> (bool, std::collections::BTreeSet<usize>) {
        fn visit(code: &[Inst], scope: &mut bool, sources: &mut std::collections::BTreeSet<usize>) {
            for inst in code {
                match inst {
                    Inst::TileLength(_)
                    | Inst::TileLane(_)
                    | Inst::TilePublish(_)
                    | Inst::TileBarrier => *scope = true,
                    Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        ..
                    }) => {
                        *scope = true;
                        sources.insert(*tile);
                    }
                    Inst::TileCollective(c) => {
                        *scope = true;
                        if !c.results.is_empty() {
                            sources.insert(c.tile);
                        }
                    }
                    Inst::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | Inst::TileLoad { tile, .. } => {
                        *scope = true;
                        sources.insert(*tile);
                    }
                    Inst::Scope(body) => visit(body, scope, sources),
                    Inst::If { yes, no, .. } => {
                        visit(yes, scope, sources);
                        visit(no, scope, sources);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        visit(condition, scope, sources);
                        visit(body, scope, sources);
                    }
                    _ => {}
                }
            }
        }
        let mut scope = false;
        let mut sources = std::collections::BTreeSet::new();
        visit(&self.code, &mut scope, &mut sources);
        (scope, sources)
    }
    pub fn has_tile_scope(&self) -> bool {
        self.tile_usage().0 || (!self.tiles.is_empty() && !self.subgroup_operations().is_empty())
    }
    pub fn storage_tiles(&self) -> std::collections::BTreeSet<usize> {
        self.tile_usage().1
    }
    pub fn workgroup_width(&self) -> u32 {
        if self.has_tile_scope() {
            self.tiles[0].width
        } else {
            64
        }
    }
    pub fn storage_words(&self) -> std::collections::BTreeSet<SharedWord> {
        fn visit(stage: &Stage, code: &[Inst], words: &mut std::collections::BTreeSet<SharedWord>) {
            for inst in code {
                match inst {
                    Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        word,
                        offset: None,
                        ..
                    }) => {
                        words.insert(stage.shared_word(*tile, *word));
                    }
                    Inst::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        offset: Some(_),
                        ..
                    }) => {
                        words.extend(
                            (0..stage.tiles[*tile].ty.size / 4)
                                .map(|w| stage.shared_word(*tile, w)),
                        );
                    }
                    Inst::TileLoad { tile, word, .. } => {
                        words.insert(stage.shared_word(*tile, *word));
                    }
                    Inst::TileCollective(c) => {
                        words.extend(
                            c.results
                                .iter()
                                .map(|(word, _)| stage.shared_word(c.tile, *word)),
                        );
                    }
                    Inst::Scope(body) => visit(stage, body, words),
                    Inst::If { yes, no, .. } => {
                        visit(stage, yes, words);
                        visit(stage, no, words);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        visit(stage, condition, words);
                        visit(stage, body, words);
                    }
                    _ => {}
                }
            }
        }
        let mut words = std::collections::BTreeSet::new();
        visit(self, &self.code, &mut words);
        words
    }
    pub fn subgroup_operations(&self) -> std::collections::BTreeSet<SubgroupOp> {
        fn visit(code: &[Inst], ops: &mut std::collections::BTreeSet<SubgroupOp>) {
            for inst in code {
                match inst {
                    Inst::Append(_) => {
                        ops.extend([
                            SubgroupOp::Count,
                            SubgroupOp::PrefixCount,
                            SubgroupOp::Elect,
                            SubgroupOp::BroadcastFirst,
                        ]);
                    }
                    Inst::Subgroup { op, .. } => {
                        ops.insert(*op);
                    }
                    Inst::Scope(body) => visit(body, ops),
                    Inst::If { yes, no, .. } => {
                        visit(yes, ops);
                        visit(no, ops);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        visit(condition, ops);
                        visit(body, ops);
                    }
                    _ => {}
                }
            }
        }
        let mut ops = std::collections::BTreeSet::new();
        visit(&self.code, &mut ops);
        ops
    }
}
impl SubgroupCapabilities {
    pub fn from_operations(ops: impl IntoIterator<Item = SubgroupOp>) -> Self {
        let mut caps = Self::default();
        for op in ops {
            caps.basic = true;
            match op {
                SubgroupOp::Any | SubgroupOp::All => caps.vote = true,
                SubgroupOp::Count | SubgroupOp::PrefixCount | SubgroupOp::BroadcastFirst => {
                    caps.ballot = true
                }
                SubgroupOp::Shuffle | SubgroupOp::ShuffleXor => caps.shuffle = true,
                SubgroupOp::ShuffleUp | SubgroupOp::ShuffleDown => caps.shuffle_relative = true,
                SubgroupOp::Sum
                | SubgroupOp::ExclusiveSum
                | SubgroupOp::Min
                | SubgroupOp::Max
                | SubgroupOp::BitAnd
                | SubgroupOp::BitOr
                | SubgroupOp::BitXor
                | SubgroupOp::ExclusiveBitAnd
                | SubgroupOp::ExclusiveBitOr
                | SubgroupOp::ExclusiveBitXor => caps.arithmetic = true,
                SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id | SubgroupOp::Elect => {}
            }
        }
        caps
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) name: String,
    pub(crate) target: Target,
    pub(crate) buffers: Vec<Buffer>,
    pub(crate) accelerations: Vec<String>,
    pub(crate) textures: Vec<TextureBinding>,
    pub(crate) uniforms: Vec<Uniform>,
    pub(crate) uniform_bytes: u32,
    pub(crate) source_stages: usize,
    pub(crate) stages: Vec<Stage>,
    pub(crate) prefix_requirements: Vec<PrefixRelation>,
    pub(crate) prefix_guarantees: Vec<PrefixRelation>,
}
impl Program {
    pub fn textures(&self) -> &[TextureBinding] {
        &self.textures
    }
    pub fn texture_count(&self) -> usize {
        self.textures.iter().map(|t| t.elements() as usize).sum()
    }
    pub fn texture_offset(&self, binding: usize) -> usize {
        self.textures[..binding]
            .iter()
            .map(|t| t.elements() as usize)
            .sum()
    }
    pub fn requires_ray_position_fetch(&self) -> bool {
        self.stages
            .iter()
            .any(|stage| !stage.ray_result_queries(6..15).is_empty())
    }
    pub fn requires_texture_indexing(&self) -> bool {
        self.textures.iter().any(|t| t.count.is_some())
    }
    pub fn requires_maximal_reconvergence(&self) -> bool {
        self.stages
            .iter()
            .any(Stage::requires_maximal_reconvergence)
    }
    pub fn subgroup_capabilities(&self) -> SubgroupCapabilities {
        SubgroupCapabilities::from_operations(
            self.stages.iter().flat_map(Stage::subgroup_operations),
        )
    }
    /// 内部设备绑定：集合中间量与间接域参数各有一个必要槽，不占源码参数。
    pub fn indirect_buffer(&self) -> usize {
        self.buffers.len()
            + usize::from(self.stages.iter().any(|s| {
                matches!(
                    s.operation,
                    Operation::ScanExclusive(_)
                        | Operation::CustomScan(_)
                        | Operation::CustomSegmentedReduce(_)
                        | Operation::SegmentedReduce(_)
                        | Operation::Compact(_)
                        | Operation::StableSort(_)
                ) || s.selection.is_some()
            }))
    }
    pub fn selection_buffer(&self) -> usize {
        self.indirect_buffer() + 1
    }
    pub fn scan_source_buffer(&self) -> usize {
        self.selection_buffer() + 1
    }
    pub fn auxiliary_buffers(&self) -> usize {
        let original = self.indirect_buffer() - self.buffers.len()
            + usize::from(
                self.stages
                    .iter()
                    .any(|s| s.prefix.is_some() || s.selection.is_some()),
            )
            + usize::from(self.stages.iter().any(|s| s.selection.is_some()));
        if self.stages.iter().any(|stage| match &stage.operation {
            Operation::ScanExclusive(value) => value.values.source == value.values.target,
            Operation::CustomScan(value) => {
                value.values.fields.source == value.values.fields.target
            }
            Operation::SegmentedReduce(value) => {
                value.values.fields.source == value.values.fields.target
            }
            Operation::CustomSegmentedReduce(value) => {
                value.values.fields.source == value.values.fields.target
            }
            _ => false,
        }) {
            original.max(self.scan_source_buffer() + 1 - self.buffers.len())
        } else {
            original
        }
    }

    pub fn requires_sort(&self) -> bool {
        self.stages
            .iter()
            .any(|s| matches!(s.operation, Operation::StableSort(_)))
    }
    pub fn prefix_requirements(&self) -> &[PrefixRelation] {
        &self.prefix_requirements
    }
    pub fn prefix_guarantees(&self) -> &[PrefixRelation] {
        &self.prefix_guarantees
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }
    pub fn accelerations(&self) -> &[String] {
        &self.accelerations
    }
    pub fn uniforms(&self) -> &[Uniform] {
        &self.uniforms
    }
    pub fn uniform_bytes(&self) -> u32 {
        self.uniform_bytes
    }
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }
    pub fn describe(&self) -> String {
        let mut out = format!(
            "program {} on {:?}\npush_constants={} bytes, source_stages={}, executable_stages={}\n",
            self.name,
            self.target,
            self.uniform_bytes,
            self.source_stages,
            self.stages.len()
        );
        for (i, b) in self.buffers.iter().enumerate() {
            out += &format!(
                "  binding {i}: {:?} {} : {}, stride={}, shape={:?}\n",
                b.access, b.name, b.ty.name, b.stride, b.count
            );
            for (name, off, ty) in &b.ty.fields {
                out += &format!("    {name}: {} @ {off}\n", ty.name);
            }
        }
        for u in &self.uniforms {
            out += &format!("  uniform {} : {} @ {}\n", u.name, u.ty.name, u.offset);
        }
        for (i, name) in self.accelerations.iter().enumerate() {
            out += &format!(
                "  binding {}: read acceleration {name}\n",
                self.buffers.len() + i
            );
        }
        for (i, texture) in self.textures.iter().enumerate() {
            let name = &texture.name;
            out += &format!(
                "  texture {}: Read {name} : texture2d\n",
                self.buffers.len() + self.accelerations.len() + i
            );
            if let Some(count) = texture.count {
                out += &format!("    array elements={count}\n");
            }
        }
        for (kind, relations) in [
            ("requires", &self.prefix_requirements),
            ("ensures", &self.prefix_guarantees),
        ] {
            for relation in relations {
                out += &format!(
                    "  {kind} prefix({}, {} word {})\n",
                    self.buffers[relation.domain].name,
                    self.buffers[relation.count.binding].name,
                    relation.count.word
                );
            }
        }
        let subgroup = self.subgroup_capabilities();
        if subgroup.basic {
            out += &format!("  subgroup={subgroup:?}\n");
        }
        for (i, s) in self.stages.iter().enumerate() {
            out += &format!(
                "  stage {i}: {:?}, domain={}, reads={:?}, writes={:?}, scalar_values={}, locals={}, prefix={:?}, selection={:?}\n",
                s.operation,
                self.buffers[s.domain].name,
                s.reads,
                s.writes,
                s.registers.len(),
                s.locals.len(),
                s.prefix,
                s.selection
            );
            if s.has_tile_scope() {
                out += &format!(
                    "    tile_width={}, shared_values={}, phases={}, shared_bytes={}, collective_scratch_bytes={}\n",
                    s.workgroup_width(),
                    s.storage_tiles().len(),
                    s.tile_phases().len(),
                    s.tile_bytes(),
                    s.tile_scratch_bytes()
                );
                for tile in s.storage_tiles() {
                    let t = &s.tiles[tile];
                    match t.source {
                        TileSource::Input { binding, word } => {
                            out += &format!(
                                "      input {} word {} : {}\n",
                                self.buffers[binding].name, word, t.ty.name
                            )
                        }
                        TileSource::Published => {
                            out += &format!("      publication {tile} : {}\n", t.ty.name)
                        }
                    }
                }
            }
        }
        out
    }
}
pub fn align_up(n: u32, a: u32) -> u32 {
    (n + a - 1) & !(a - 1)
}
