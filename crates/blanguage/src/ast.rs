use std::path::PathBuf;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeName {
    pub name: String,
    /// 已由实际 Ty 绑定的完整名义键；源码名称始终按词法作用域解析。
    pub(crate) canonical: bool,
    pub arguments: Vec<GenericArgument>,
    pub dataset: bool,
    pub view: bool,
    pub tile: bool,
    pub count: Option<Shape>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    Literal(u32),
    Constant(String),
}
impl TypeName {
    pub fn is_never(&self) -> bool {
        self.name == "never"
            && self.arguments.is_empty()
            && !self.dataset
            && !self.view
            && !self.tile
            && self.count.is_none()
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
    pub fn is_texture(&self) -> bool {
        self.name == "texture2d"
            && self.arguments.is_empty()
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
    Index {
        value: String,
        resource: String,
    },
    Initialized {
        value: String,
        condition: OutputCondition,
    },
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputConstant {
    pub ty: crate::ir::Ty,
    pub bits: Vec<u32>,
}
impl PartialOrd for OutputConstant {
    fn partial_cmp(&self, other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}
}
impl Ord for OutputConstant {
    fn cmp(&self,other:&Self)->std::cmp::Ordering{
        use crate::module::codec::Encode;
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
pub enum ExprKind {
    Number(String),
    StaticU32(u32),
    Bool(bool),
    Name(String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
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
