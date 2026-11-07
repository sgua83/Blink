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
}
impl TextureFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rgba32Float => "rgba32_float", Self::Rgba8Unorm => "rgba8_unorm",
            Self::Rgba8Srgb => "rgba8_srgb", Self::R8Unorm => "r8_unorm",
            Self::Rg16Unorm => "rg16_unorm", Self::Rgb10A2Unorm => "rgb10a2_unorm",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "rgba32_float" => Some(Self::Rgba32Float), "rgba8_unorm" => Some(Self::Rgba8Unorm),
            "rgba8_srgb" => Some(Self::Rgba8Srgb), "r8_unorm" => Some(Self::R8Unorm),
            "rg16_unorm" => Some(Self::Rg16Unorm), "rgb10a2_unorm" => Some(Self::Rgb10A2Unorm),
            _ => None,
        }
    }
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Rgba32Float), 1 => Some(Self::Rgba8Unorm), 2 => Some(Self::Rgba8Srgb),
            3 => Some(Self::R8Unorm), 4 => Some(Self::Rg16Unorm), 5 => Some(Self::Rgb10A2Unorm),
            _ => None,
        }
    }
    pub fn texel_bytes(self) -> usize {
        match self {
            Self::Rgba32Float => 16, Self::R8Unorm => 1,
            Self::Rgba8Unorm | Self::Rgba8Srgb | Self::Rg16Unorm | Self::Rgb10A2Unorm => 4,
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
    pub fn texture_array_shape(&self) -> Option<Shape> {
        if self.name == "array"
            && !self.dataset
            && !self.view
            && !self.tile
            && self.arguments.len() == 2
            && let GenericArgument::Type(element) = &self.arguments[0]
            && element.is_texture()
        {
            return self.arguments[1].shape();
        }
        None
    }
    pub fn is_texture_resource(&self) -> bool {
        self.is_texture() || self.texture_array_shape().is_some()
    }
    pub fn texture_dimension(&self) -> Option<TextureDimension> {
        match self.name.as_str() {
            "texture2d" if self.is_texture() => Some(TextureDimension::D2),
            "texture3d" if self.is_texture() => Some(TextureDimension::D3),
            "array" if self.texture_array_shape().is_some() => match &self.arguments[0] {
                GenericArgument::Type(element) => element.texture_dimension(),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn texture_format(&self) -> Option<TextureFormat> {
        if self.name == "array" && self.texture_array_shape().is_some() {
            if let GenericArgument::Type(element) = &self.arguments[0] { return element.texture_format(); }
        }
        let [GenericArgument::Type(format)] = self.arguments.as_slice() else { return None; };
        if *format != TypeName::named(&format.name) { return None; }
        TextureFormat::from_name(&format.name)
    }
    pub fn is_texture(&self) -> bool {
        matches!(self.name.as_str(), "texture2d" | "texture3d")
            && (self.arguments.is_empty() || self.texture_format().is_some())
            && !self.dataset
            && !self.view
            && !self.tile
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
        use crate::codec::Encode;
        Encode::bytes(&self.ty).cmp(&Encode::bytes(&other.ty)).then_with(||self.bits.cmp(&other.bits))
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
    pub(crate) fn shape(&self) -> Option<Shape> {
        match self {
            Self::Value(shape) => Some(shape.clone()),
            Self::Type(ty) if ty.arguments.is_empty() && !ty.dataset && !ty.view && !ty.tile => {
                Some(Shape::Constant(ty.name.clone()))
            }
            _ => None,
        }
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
    let shape = |value: &Shape| match value { Shape::Literal(value) => value.to_string(), Shape::Constant(name) => name.clone(), Shape::Expression(expression)=>expression_name(expression) };
    let mut name = ty.name.clone();
    if !ty.arguments.is_empty() {
        name.push('<');
        name.push_str(&ty.arguments.iter().map(|argument| match argument {
            GenericArgument::Type(ty) => type_name(ty),
            GenericArgument::Value(value) => shape(value),
        }).collect::<Vec<_>>().join(","));
        name.push('>');
    }
    for (present, prefix) in [(ty.dataset, "dataset"), (ty.view, "view"), (ty.tile, "tile")] {
        if present {
            name = if let Some(count) = &ty.count { format!("{prefix}<{name},{}>", shape(count)) }
                else { format!("{prefix}<{name}>") };
        }
    }
    if let Some(outputs) = &ty.contracts {
        for output in outputs.iter() {
            if let ContractPredicate::Nonzero(value) = output {
                name.push_str(&format!(" requires nonzero({value})"));
            } else if let ContractPredicate::Memory(access)=output {
                let mode=match access{None=>"none",Some(Access::Read)=>"read",Some(Access::Write)=>"write",Some(Access::InOut)=>"inout",Some(Access::Value)=>"value",Some(Access::Atomic)=>"atomic"};
                name.push_str(&format!(" ensures memory({mode})"));
            } else if let ContractPredicate::Initialized { value, condition } = output {
                name.push_str(&format!(" ensures initialized({value},{})", condition_name(condition)));
            }
        }
    }
    name
}
pub(crate) fn expression_name(expression:&Expr)->String {
    let arguments=|values:&[Expr]|values.iter().map(expression_name).collect::<Vec<_>>().join(",");
    let generic=|values:&[GenericArgument]|values.iter().map(|value|match value {
        GenericArgument::Type(ty)=>type_name(ty),
        GenericArgument::Value(Shape::Literal(value))=>value.to_string(),
        GenericArgument::Value(Shape::Constant(name))=>name.clone(),
        GenericArgument::Value(Shape::Expression(value))=>expression_name(value),
    }).collect::<Vec<_>>().join(",");
    match &expression.kind {
        ExprKind::Number(value)|ExprKind::Name(value)=>value.clone(),
        ExprKind::StaticU32(value)=>value.to_string(),ExprKind::Bool(value)=>value.to_string(),
        ExprKind::Member(base,field)=>format!("{}.{}",expression_name(base),field),
        ExprKind::Index(base,index)=>format!("{}[{}]",expression_name(base),expression_name(index)),
        ExprKind::Unary(op,value)=>format!("({op}{})",expression_name(value)),
        ExprKind::Binary(op,a,b)=>format!("({}{op}{})",expression_name(a),expression_name(b)),
        ExprKind::Select(a,b,c)=>format!("({}?{}:{})",expression_name(a),expression_name(b),expression_name(c)),
        ExprKind::Call(name,args)=>format!("{name}({})",arguments(args)),
        ExprKind::Invoke(callee,args)=>format!("{}({})",expression_name(callee),arguments(args)),
        ExprKind::Construct(ty,args)=>format!("{}({})",type_name(ty),arguments(args)),
        ExprKind::Specialize(name,types,args)=>format!("{name}<{}>({})",generic(types),arguments(args)),
        ExprKind::FunctionRef(name,types)=>format!("{name}<{}>",generic(types)),
        ExprKind::Cooperate(name,types,args)=>format!("cooperate {name}{}({})",types.as_ref().map(|types|format!("<{}>",generic(types))).unwrap_or_default(),arguments(args)),
        ExprKind::Target(target,value)=>format!("target({},{})",match target {Target::Cpu=>"cpu",Target::Gpu=>"gpu",Target::Compute=>"compute"},expression_name(value)),
    }
}
fn condition_name(condition: &OutputCondition) -> String {
    match condition {
        OutputCondition::Name(name) | OutputCondition::Number(name) => name.clone(),
        OutputCondition::Bool(value) => value.to_string(),
        OutputCondition::StaticU32(value) => value.to_string(),
        OutputCondition::Member(base, field) => format!("{}.{}", condition_name(base), field),
        OutputCondition::Unary(op, value) => format!("({op}{})", condition_name(value)),
        OutputCondition::Binary(op, a, b) => format!("({}{op}{})", condition_name(a), condition_name(b)),
        OutputCondition::Constant(value) => format!("{}[{}]", value.ty.name,
            value.bits.iter().map(|bits| format!("{bits:016x}")).collect::<Vec<_>>().join(",")),
    }
}
