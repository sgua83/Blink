use std::path::PathBuf;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub text: std::sync::Arc<str>,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub source: Source,
    pub span: Span,
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = &self.source.text[..self.span.start.min(self.source.text.len())];
        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let start = prefix.rfind('\n').map_or(0, |i| i + 1);
        let column = prefix[start..].chars().count() + 1;
        let tail = &self.source.text[start..];
        let end = tail.find('\n').map_or(tail.len(), |i| i);
        let content = &tail[..end];
        write!(
            f,
            "{}:{line}:{column}: {}: {}\n  {content}\n  {}^",
            self.source.path.display(),
            self.code,
            self.message,
            " ".repeat(column - 1)
        )
    }
}
impl std::error::Error for Diagnostic {}
pub type Result<T> = std::result::Result<T, Diagnostic>;
impl Source {
    pub fn fail<T>(&self, span: Span, code: &'static str, message: impl Into<String>) -> Result<T> {
        Err(Diagnostic {
            source: self.clone(),
            span,
            code,
            message: message.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Access {
    Value,
    Read,
    Write,
    InOut,
    Atomic,
}
impl Access {
    pub fn reads(self) -> bool {
        matches!(self, Self::Read | Self::InOut | Self::Atomic)
    }
    pub fn writes(self) -> bool {
        matches!(self, Self::Write | Self::InOut | Self::Atomic)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Target {
    Compute,
    Cpu,
    Gpu,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextureDimension {
    D2,
    D3,
}
impl TextureDimension {
    pub fn components(self) -> usize {
        match self { Self::D2 => 2, Self::D3 => 3 }
    }
    pub fn name(self) -> &'static str {
        match self { Self::D2 => "texture2d", Self::D3 => "texture3d" }
    }
}
/// 原texel编码；B资源声明、原生描述和BLTEX共用同一格式身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TextureFormat {
    Rgba32Float = 0,
    Rgba8Unorm = 1,
    Rgba8Srgb = 2,
    R8Unorm = 3,
    Rg16Unorm = 4,
    Rgb10A2Unorm = 5,
    R11G11B10Float = 6,
    R16Float = 7,
}
impl TextureFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rgba32Float => "rgba32_float", Self::Rgba8Unorm => "rgba8_unorm",
            Self::Rgba8Srgb => "rgba8_srgb", Self::R8Unorm => "r8_unorm",
            Self::Rg16Unorm => "rg16_unorm", Self::Rgb10A2Unorm => "rgb10a2_unorm",
            Self::R11G11B10Float => "r11g11b10_float",
            Self::R16Float => "r16_float",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "rgba32_float" => Some(Self::Rgba32Float), "rgba8_unorm" => Some(Self::Rgba8Unorm),
            "rgba8_srgb" => Some(Self::Rgba8Srgb), "r8_unorm" => Some(Self::R8Unorm),
            "rg16_unorm" => Some(Self::Rg16Unorm), "rgb10a2_unorm" => Some(Self::Rgb10A2Unorm),
            "r11g11b10_float" => Some(Self::R11G11B10Float),
            "r16_float" => Some(Self::R16Float),
            _ => None,
        }
    }
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Rgba32Float), 1 => Some(Self::Rgba8Unorm), 2 => Some(Self::Rgba8Srgb),
            3 => Some(Self::R8Unorm), 4 => Some(Self::Rg16Unorm), 5 => Some(Self::Rgb10A2Unorm),
            6 => Some(Self::R11G11B10Float),
            7 => Some(Self::R16Float),
            _ => None,
        }
    }
    pub fn texel_bytes(self) -> usize {
        match self {
            Self::Rgba32Float => 16, Self::R8Unorm => 1, Self::R16Float => 2,
            Self::Rgba8Unorm | Self::Rgba8Srgb | Self::Rg16Unorm | Self::Rgb10A2Unorm | Self::R11G11B10Float => 4,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeName {
    pub name: String,
    /// 已由实际 Ty 绑定的完整名义键；源码名称始终按词法作用域解析。
    pub(crate) canonical: bool,
    pub arguments: Vec<GenericArgument>,
    pub dataset: bool,
    pub view: bool,
    pub tile: bool,
    pub count: Option<Shape>,
    pub contracts: Option<Box<Vec<ContractPredicate>>>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    Literal(u32),
    Constant(String),
    Expression(Box<Expr>),
}

#[derive(Clone, Copy)]
pub(crate) enum BorrowedShape<'a> {
    Literal(u32),
    Constant(&'a str),
    Expression(&'a Expr),
}

impl BorrowedShape<'_> {
    fn into_owned(self) -> Shape {
        match self {
            Self::Literal(value) => Shape::Literal(value),
            Self::Constant(name) => Shape::Constant(name.into()),
            Self::Expression(expression) => Shape::Expression(Box::new(expression.clone())),
        }
    }
}

impl Shape {
    pub(crate) fn borrowed(&self) -> BorrowedShape<'_> {
        match self {
            Self::Literal(value) => BorrowedShape::Literal(*value),
            Self::Constant(name) => BorrowedShape::Constant(name),
            Self::Expression(expression) => BorrowedShape::Expression(expression),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TextureType<'a> {
    pub dimension: TextureDimension,
    pub format: Option<TextureFormat>,
    pub length: Option<BorrowedShape<'a>>,
}

impl TypeName {
    pub fn is_never(&self) -> bool {
        self.name == "never"
            && self.arguments.is_empty()
            && !self.dataset
            && !self.view
            && !self.tile
            && self.count.is_none()
            && self.contracts.is_none()
    }
    pub fn mentions_owned(&self) -> bool {
        self.dataset
            || self
                .arguments
                .iter()
                .any(|arg| matches!(arg,GenericArgument::Type(ty) if ty.mentions_owned()))
    }
    pub fn is_acceleration(&self) -> bool {
        self.name == "acceleration"
            && self.arguments.is_empty()
            && !self.dataset
            && !self.view
            && !self.tile
    }
    fn texture_element(&self) -> Option<TextureType<'_>> {
        if self.dataset || self.view || self.tile {
            return None;
        }
        let dimension = match self.name.as_str() {
            "texture2d" => TextureDimension::D2,
            "texture3d" => TextureDimension::D3,
            _ => return None,
        };
        let format = if self.arguments.is_empty() { None } else { Some(self.texture_format_argument()?) };
        Some(TextureType { dimension, format, length: None })
    }

    pub(crate) fn texture_type(&self) -> Option<TextureType<'_>> {
        if self.name != "array" {
            return self.texture_element();
        }
        if self.dataset || self.view || self.tile {
            return None;
        }
        let [GenericArgument::Type(element), length] = self.arguments.as_slice() else { return None; };
        let mut texture = element.texture_element()?;
        texture.length = Some(length.borrowed_shape()?);
        Some(texture)
    }

    pub fn texture_array_shape(&self) -> Option<Shape> {
        self.texture_type()?.length.map(BorrowedShape::into_owned)
    }

    pub fn is_texture_resource(&self) -> bool {
        self.texture_type().is_some()
    }

    pub fn texture_dimension(&self) -> Option<TextureDimension> {
        self.texture_type().map(|texture| texture.dimension)
    }

    pub fn texture_format(&self) -> Option<TextureFormat> {
        if self.name == "array" && let Some(texture) = self.texture_type() {
            return texture.format;
        }
        self.texture_format_argument()
    }

    fn texture_format_argument(&self) -> Option<TextureFormat> {
        let [GenericArgument::Type(format)] = self.arguments.as_slice() else { return None; };
        if format.canonical || !format.arguments.is_empty() || format.dataset || format.view
            || format.tile || format.count.is_some() || format.contracts.is_some()
        {
            return None;
        }
        TextureFormat::from_name(&format.name)
    }

    pub fn is_texture(&self) -> bool {
        self.texture_element().is_some()
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            canonical: false,
            arguments: Vec::new(),
            dataset: false,
            view: false,
            tile: false,
            count: None,
            contracts: None,
        }
    }
    pub fn mentions(&self, name: &str) -> bool {
        self.name == name
            || self
                .arguments
                .iter()
                .any(|arg| matches!(arg,GenericArgument::Type(ty) if ty.mentions(name)))
    }
}
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: TypeName,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct VariantDeclaration {
    pub name: String,
    pub payload: TypeName,
    pub discriminant: Expr,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub name: String,
    pub types: Vec<TypeParameter>,
    pub values: Vec<ValueParameter>,
    pub stride: Option<Shape>,
    pub enumeration: Option<TypeName>,
    pub variants: Vec<VariantDeclaration>,
    pub fields: Vec<Field>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: TypeName,
    pub access: Access,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractKind {
    Requires,
    Ensures,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContractPredicate {
    Prefix {
        data: String,
        count: String,
        field: String,
    },
    Nonzero(String),
    Nonempty(String),
    Contiguous(String),
    Memory(Option<Access>),
    Index {
        value: String,
        resource: String,
    },
    Range { value: String, intervals: Vec<(OutputCondition, OutputCondition)> },
    SameLength { left: String, right: String },
    Initialized {
        value: String,
        condition: OutputCondition,
    },
    Cleanup { value: String, condition: OutputCondition },
}
// A source interface expression has no calls, storage access or effects. Names
// denote immutable input values; `returned` denotes this call's actual result.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputCondition {
    Name(String),
    Number(String),
    Bool(bool),
    Member(Box<Self>, String),
    Unary(String, Box<Self>),
    Binary(String, Box<Self>, Box<Self>),
    StaticU32(u32),
    Constant(OutputConstant),
}
#[derive(Clone, Debug)]
pub struct OutputConstant {
    pub ty: crate::ir::Ty,
    pub bits: Vec<crate::ir::Bits>,
    #[cfg(feature = "compiler")]
    pub generated_origin: Option<std::sync::Arc<crate::ir::GeneratedOrigin>>,
}
impl OutputConstant {
    pub fn new(ty: crate::ir::Ty, bits: Vec<crate::ir::Bits>) -> Self {
        Self { ty, bits, #[cfg(feature = "compiler")] generated_origin: None }
    }
}
impl PartialEq for OutputConstant {
    fn eq(&self, other: &Self) -> bool { self.ty == other.ty && self.bits == other.bits }
}
impl Eq for OutputConstant {}
impl PartialOrd for OutputConstant {
    fn partial_cmp(&self, other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}
}
impl Ord for OutputConstant {
    fn cmp(&self,other:&Self)->std::cmp::Ordering{
        self.ty.cmp(&other.ty).then_with(||self.bits.cmp(&other.bits))
    }
}
impl OutputCondition {
    pub fn from_expr(value: &Expr) -> Option<Self> {
        Some(match &value.kind {
            ExprKind::Name(name) => Self::Name(name.clone()),
            ExprKind::Number(number) => Self::Number(number.clone()),
            ExprKind::StaticU32(value) => Self::StaticU32(*value),
            ExprKind::Bool(value) => Self::Bool(*value),
            ExprKind::Member(base, field) => Self::Member(Box::new(Self::from_expr(base)?), field.clone()),
            ExprKind::Unary(op, value) => Self::Unary(op.clone(), Box::new(Self::from_expr(value)?)),
            ExprKind::Binary(op, a, b) => Self::Binary(op.clone(), Box::new(Self::from_expr(a)?), Box::new(Self::from_expr(b)?)),
            _ => return None,
        })
    }
}
#[derive(Clone, Debug)]
pub struct FunctionContract {
    pub kind: ContractKind,
    pub predicate: ContractPredicate,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeFamily {
    Numeric,
    Floating,
    Integer,
    ScalarNumeric,
    ScalarInteger,
    Named(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: String,
    pub family: Option<TypeFamily>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueParameter {
    pub name: String,
    pub choices: Option<Vec<u32>>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenericArgument {
    Type(TypeName),
    Value(Shape),
}
impl GenericArgument {
    fn borrowed_shape(&self) -> Option<BorrowedShape<'_>> {
        match self {
            Self::Value(shape) => Some(shape.borrowed()),
            Self::Type(ty) if ty.arguments.is_empty() && !ty.dataset && !ty.view && !ty.tile => {
                Some(BorrowedShape::Constant(&ty.name))
            }
            _ => None,
        }
    }

    pub(crate) fn shape(&self) -> Option<Shape> {
        self.borrowed_shape().map(BorrowedShape::into_owned)
    }
}
#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub types: Vec<TypeParameter>,
    pub values: Vec<ValueParameter>,
    pub params: Vec<Param>,
    pub result: TypeName,
    pub kind: FunctionKind,
    pub target: Target,
    pub contracts: Vec<FunctionContract>,
    pub body: Option<Vec<Stmt>>,
    pub c_abi: bool,
    pub external: Option<ExternalName>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalName {
    pub library: String,
    pub symbol: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FunctionKind {
    Value,
    Cooperative,
    Constant,
}
#[derive(Clone, Debug)]
pub struct TypeSet {
    pub name: String,
    pub members: Vec<TypeName>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub imports: Vec<String>,
    pub records: Vec<Record>,
    pub type_sets: Vec<TypeSet>,
    pub functions: Vec<Function>,
    pub source: Source,
}
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
// Source positions locate syntax; expression identity is its actual syntax.
impl PartialEq for Expr { fn eq(&self,other:&Self)->bool { self.kind==other.kind } }
impl Eq for Expr {}
impl PartialOrd for Expr {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for Expr {fn cmp(&self,other:&Self)->std::cmp::Ordering{self.kind.cmp(&other.kind)}}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExprKind {
    Number(String),
    StaticU32(u32),
    Bool(bool),
    Name(String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Invoke(Box<Expr>, Vec<Expr>),
    Construct(TypeName, Vec<Expr>),
    Specialize(String, Vec<GenericArgument>, Vec<Expr>),
    FunctionRef(String, Vec<GenericArgument>),
    Cooperate(String, Option<Vec<GenericArgument>>, Vec<Expr>),
    Target(Target, Box<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Select(Box<Expr>, Box<Expr>, Box<Expr>),
}
#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct ParallelDomain {
    pub dataset: String,
    pub count: Option<Expr>,
    pub selection: Option<Expr>,
}
#[derive(Clone, Debug)]
pub struct MatchArm {
    pub variant: String,
    pub binding: Option<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum StmtKind {
    Declare(TypeName, String),
    Let(Option<TypeName>, String, Expr),
    Assign(Expr, String, Expr),
    Expr(Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Match(Expr, Vec<MatchArm>),
    While(Expr, Vec<Stmt>),
    For(String, Expr, Option<Expr>, Vec<Stmt>),
    Parallel(String, ParallelDomain, Vec<Stmt>),
    Return(Option<Expr>),
    Break,
    Continue,
    Phase,
    Seek(String, Expr),
}

pub(crate) fn type_name(ty: &TypeName) -> String {
    let mut name = String::new();
    write_type_name(&mut name, ty);
    name
}

pub(crate) fn write_type_name(name: &mut String, ty: &TypeName) {
    crate::codec::types::source_type_name(ty, name);
}

pub(crate) fn write_type_contracts(name: &mut String, outputs: &[ContractPredicate]) {
    crate::codec::types::contract_names(outputs, name);
}

pub(crate) fn expression_name(expression: &Expr) -> String {
    let mut name = String::new();
    crate::codec::types::expression_name(expression, &mut name);
    name
}
