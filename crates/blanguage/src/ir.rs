pub(crate) mod scalar_ranges;
use crate::ast::{Access, Target, TextureDimension};
use std::{collections::{BTreeMap, BTreeSet}, num::NonZeroU64};
#[cfg(feature = "compiler")]
mod origin;
#[cfg(feature = "compiler")]
pub use origin::{SourceOrigin, SourceDeclaration, DeclarationRepresentation, SourceSite, SourceSyntax, InlineSite, DomainSite, DomainTransition, ValueCodeEntry, GeneratedOrigin, GenerationReason, GenerationAnchor, IntegerKnownValueBasis, OperationOrigin, StageForBound, StageForIteration, ExpressionReuse, ReuseResult, LocalReadForward, IndexedStoreReadForward, InputReadForward, ReductionInputRole, ReductionInputReadForward, ReductionPartition, ReductionUniformOperation, EliminatedOperation};
#[cfg(feature = "compiler")]
pub(crate) use origin::SourceContext;
mod layout;
mod properties;
#[cfg(feature = "compiler")]
pub(crate) use properties::{ReferenceNeed, ReferenceWalk, SourceMember, StorageMode, StorageNeed, StorageWalk, source_member, dataset_count, dataset_path, dataset_paths};
pub use layout::interface_layout_json;
#[cfg(feature = "compiler")]
pub(crate) use layout::ExpandedLayout;
mod external;
pub(crate) use external::DatasetBorrow;
#[cfg(feature = "compiler")]
pub use external::{ParameterOrigin, ArgumentOrigin};
pub use external::{
    Call, CallArgument, CallParameter, CallParameterKind, CallReference, CallStorage, CallTarget, FunctionBody, MemoryAccess, MemoryAlias,
};
mod execution;
pub use execution::{
    DeviceRequirements, ElementRange, ElementFact, ExecutionPlan, ExecutionStage, ProgramInterface, ProgramLayout,
    ReductionLayout,
};
mod atomic;
pub use atomic::{Atomic, AtomicOp, MemoryOrder};
mod custom_reduce;
pub use custom_reduce::{
    CustomReduction, CustomScan, CustomSegmentedReduction, ValueCode,
};
#[cfg(feature = "compiler")]
pub use custom_reduce::ReductionKernel;
#[cfg(feature = "compiler")]
pub(crate) use custom_reduce::{PreparedReductionKernel, ReductionBody, ReductionParts};

pub type Reg = usize;
pub type Bits = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Scalar {
    F32,
    U32,
    I32,
    Bool,
    U64,
    I64,
    F64,
    U8,
    I8,
    U16,
    I16,
}
impl Scalar {
    pub const fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::U32 => "u32",
            Self::I32 => "i32",
            Self::Bool => "bool",
            Self::U64 => "u64",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::I8 => "i8",
            Self::U16 => "u16",
            Self::I16 => "i16",
        }
    }
    pub const fn size(self) -> u32 {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::F32 | Self::U32 | Self::I32 | Self::Bool => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }
    pub const fn align(self) -> u32 {
        self.size()
    }
    pub const fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }
    pub const fn is_integer(self) -> bool {
        !self.is_float() && !matches!(self, Self::Bool)
    }
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
    pub const fn bit_width(self) -> u32 {
        self.size() * 8
    }
    pub const fn bit_mask(self) -> Bits {
        self.truncate(Bits::MAX)
    }
    pub const fn order_bias(self) -> Bits {
        if self.is_signed() { 1 << (self.bit_width() - 1) } else { 0 }
    }
    pub const fn truncate(self, bits: Bits) -> Bits {
        bits & (Bits::MAX >> (64 - self.bit_width()))
    }
    /// The actual ordered interval and fixed raw bits must imply the required set.
    pub fn covers_range(self, range: (Bits, Bits), known_bits: (Bits, Bits), intervals: &[(Bits, Bits)], required_bits: (Bits, Bits)) -> bool {
        scalar_ranges::covers(self, range, known_bits, intervals, required_bits)
    }
    pub const fn signed(self, bits: Bits) -> i64 {
        let shift = 64 - self.bit_width();
        ((bits << shift) as i64) >> shift
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Variant {
    pub name: String,
    pub discriminant: u32,
    pub ty: Box<Ty>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CFunctionType {
    pub result: crate::ast::TypeName,
    pub parameters: Vec<(crate::ast::Access, crate::ast::TypeName)>,
    pub contracts: Vec<crate::ast::ContractPredicate>,
}
#[cfg(feature = "compiler")]
#[repr(C)]
struct VariantLayout {
    size: u64,
    alignment: u32,
}

#[repr(C)]
struct VectorLayout {
    size: u64,
    alignment: u32,
}

#[repr(C)]
pub(crate) struct ArrayLayout {
    pub(crate) size: u64,
    pub(crate) stride: u64,
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct FieldPlacement {
    offset: u64,
    end: u64,
    alignment: u32,
}

unsafe extern "C" {
    #[cfg(feature = "compiler")]
    #[link_name = "codec.type_layout::place"]
    fn place_field(size: u32, alignment: u32, field_size: u32, field_alignment: u32) -> FieldPlacement;
    #[cfg(feature = "compiler")]
    #[link_name = "codec.type_layout::natural_size"]
    fn natural_record_size(size: u32, alignment: u32) -> u64;
    #[link_name = "codec.type_layout::vector"]
    fn vector_layout(scalar_size: u32, scalar_alignment: u32, count: u64) -> VectorLayout;
    #[link_name = "codec.type_layout::fixed_array"]
    pub(crate) fn fixed_array_layout(element_size: u32, alignment: u32, count: u32) -> ArrayLayout;
    #[cfg(feature = "compiler")]
    #[link_name = "codec.type_layout::tagged_variant"]
    fn tagged_variant_layout(payload_size: u32, payload_alignment: u32) -> VariantLayout;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    pub variants: Vec<Variant>,
    pub dataset: Option<Box<crate::ast::TypeName>>,
    pub cleanup: Option<String>,
    pub function: Option<Box<CFunctionType>>,
}
impl Ty {
    pub(crate) fn syntax(&self) -> crate::ast::TypeName { crate::codec::types::syntax(self) }
    pub(crate) fn task_result(&self) -> Option<&Self> {
        if self.name.starts_with("task<") { self.arguments.first() } else { None }
    }
    pub(crate) fn task(result: Self) -> Self {
        let mut ty=Self::scalar(Scalar::U64);
        ty.name=format!("task<{}>",result.name);
        ty.arguments.push(result);
        ty
    }
    pub(crate) const TASK_COMPLETION: &'static str = "__blink_task_completion";
    pub(crate) const TASK_RETURN_SOURCE: &'static str = "__blink_task_return_source";
    #[cfg(feature = "compiler")]
    pub(crate) fn task_components(&self) -> Vec<usize> {
        if !self.owns_resources(){return Vec::new();}
        self.borrow_fields().into_iter().filter(|(_,ty)|ty.task_result().is_some())
            .map(|(offset,_)|self.components.iter().position(|(_,at)|*at==offset).unwrap())
            .collect::<std::collections::BTreeSet<_>>().into_iter().collect()
    }
    pub(crate) fn task_return_layout(components:&[usize]) -> Self {
        let mut ty=Self::unit();
        ty.name=Self::TASK_RETURN_SOURCE.into();
        ty.align=Scalar::U32.align();
        ty.size=components.len() as u32*Scalar::U32.size();
        ty.components=(0..components.len()).map(|index|(Scalar::U32,index as u32*Scalar::U32.size())).collect();
        ty
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn task_wait_layout(&self) -> Option<Self> {
        let mut ty=self.clone();
        let components=self.task_components();
        if !components.is_empty() {
            let sources=Self::task_return_layout(&components);
            ty.size=self.size.checked_add(sources.size)?.checked_next_multiple_of(self.align)?;
            ty.components.extend(sources.components.into_iter().map(|(scalar,word)|(scalar,self.size+word)));
        }
        Some(ty)
    }
    pub const TASK_COMPLETION_STATUS:u32=0;
    pub const TASK_COMPLETION_PENDING:u32=0;
    pub const TASK_COMPLETION_SUCCESS:u32=1;
    pub const TASK_COMPLETION_FAILED:u32=2;
    pub const TASK_COMPLETION_EXTENT:u32=4;
    pub const TASK_COMPLETION_PAYLOAD:u32=8;
    #[cfg(feature = "compiler")]
    pub(crate) fn task_completion(&self) -> Option<Self> {
        self.task_wait_layout()?.task_completion_storage()
    }
    pub(crate) fn task_completion_storage(&self) -> Option<Self> {
        let mut ty=Self::unit();
        ty.name=Self::TASK_COMPLETION.into();
        ty.align=self.align.max(Scalar::U32.align());
        let payload=(Self::TASK_COMPLETION_PAYLOAD+Scalar::U32.size()).checked_next_multiple_of(ty.align)?;
        ty.size=payload.checked_add(self.size)?.checked_next_multiple_of(ty.align)?;
        ty.components.extend([
            (Scalar::U32,Self::TASK_COMPLETION_STATUS),
            (Scalar::U32,Self::TASK_COMPLETION_EXTENT),
            (Scalar::U32,Self::TASK_COMPLETION_PAYLOAD),
        ]);
        ty.components.extend(self.components.iter().map(|&(scalar,word)|(scalar,payload+word)));
        ty.fields.push(("result".into(),payload,Box::new(self.clone())));
        Some(ty)
    }
    pub fn task_completion_payload(&self) -> (u32,&Self) {
        let (_,offset,ty)=&self.fields[0];
        (*offset,ty)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn append_field_layout(&mut self, field: &Self) -> Option<u32> {
        let offset = self.place_field_layout(field)?;
        self.components.extend(field.components.iter().map(|&(scalar, word)| (scalar, offset + word)));
        Some(offset)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn place_field_layout(&mut self, field: &Self) -> Option<u32> {
        // Bound field types carry nonzero alignment.
        let placement = unsafe { place_field(self.size, self.align, field.size, field.align) };
        self.size = u32::try_from(placement.end).ok()?;
        self.align = placement.alignment;
        Some(placement.offset as u32)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn finish_record_layout(&mut self) -> Option<()> {
        let size = unsafe { natural_record_size(self.size, self.align) };
        self.size = u32::try_from(size).ok()?;
        Some(())
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn task_start_error(input: Self) -> Option<Self> {
        let mut ty = Self::task_start_error_layout(input)?;
        ty.fill_components();
        Some(ty)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn task_start_error_layout(input: Self) -> Option<Self> {
        let mut ty = Self::scalar(Scalar::U32);
        ty.name = format!("task_start_error<{}>", input.name);
        let offset = ty.place_field_layout(&input)?;
        ty.components.clear();
        ty.finish_record_layout()?;
        ty.fields = vec![
            ("code".into(), 0, Box::new(Self::scalar(Scalar::U32))),
            ("input".into(), offset, Box::new(input.clone())),
        ];
        ty.arguments.push(input);
        Some(ty)
    }
    // A variant, a user-owned value and a task have presence on the value itself.
    #[cfg(feature = "compiler")]
    pub(crate) fn has_owned_presence(&self) -> bool {
        self.has_variants() || self.cleanup.is_some() || self.task_result().is_some()
    }
    /// A task packet stores the ordinary ABI field, marked read when its Frame data stays owned by the caller.
    pub const TASK_READ_FIELD_PREFIX: &'static str = "read:";
    pub const TASK_BORROWED_DATASET_PREFIX: &'static str = "borrow:dataset:";
    pub const TASK_OWNED_REFERENCE_PREFIX: &'static str = "owned-reference:";

    pub fn contains_dataset(&self) -> bool {
        properties::contains(self, properties::Query::Dataset)
    }

    /// Independently present owners within a task's actual output position.
    pub fn task_output_owners(&self) -> Vec<(u32, &Self)> {
        properties::task_output_owners(self)
    }

    pub fn owns_resources(&self) -> bool {
        properties::contains(self, properties::Query::Resources)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn aggregate_len(&self) -> usize {
        if self.array.is_some() {
            self.values[0] as usize
        } else {
            self.fields.len()
        }
    }
    pub(crate) fn aggregate_part(&self, index: usize) -> (u32, &Self) {
        if let Some(element) = &self.array {
            (
                index as u32 * align_up(element.size, element.align),
                element,
            )
        } else {
            (self.fields[index].1, &self.fields[index].2)
        }
    }
    pub fn owned(payload: Self, cleanup: String) -> Self {
        Self {name:format!("owned<{},{}>",payload.name,cleanup),arguments:vec![payload.clone()],values:Vec::new(),size:payload.size,align:payload.align,components:payload.components.clone(),fields:vec![("value".into(),0,Box::new(payload))],view:None,array:None,enumeration:false,result:false,variants:Vec::new(),dataset:None,cleanup:Some(cleanup),function:None}
    }
    pub fn owned_dataset(element: crate::ast::TypeName, count: Option<u32>) -> Self {
        Self {
            name: format!(
                "dataset<{}{}>",
                crate::ast::type_name(&element),
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
            variants: Vec::new(),
            dataset: Some(Box::new(element)), cleanup: None, function: None,
        }
    }
    #[cfg(feature = "compiler")]
    pub fn result(ok: Self, error: Self) -> Option<Self> {
        let mut ty = Self::result_layout(ok, error)?;
        ty.fill_components();
        Some(ty)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn result_layout(ok: Self, error: Self) -> Option<Self> {
        let name = format!("result<{},{}>", ok.name, error.name);
        let arguments = vec![ok.clone(), error.clone()];
        let variants = vec![
            Variant { name: "Ok".into(), discriminant: 0, ty: Box::new(ok) },
            Variant { name: "Err".into(), discriminant: 1, ty: Box::new(error) },
        ];
        Self::variant_layout(name, arguments, Vec::new(), Scalar::Bool, variants).map(|mut ty| { ty.result = true; ty })
    }
    #[cfg(feature = "compiler")]
    pub fn variant(name: String, arguments: Vec<Self>, values: Vec<u32>, tag: Scalar, variants: Vec<Variant>) -> Option<Self> {
        let mut ty = Self::variant_layout(name, arguments, values, tag, variants)?;
        ty.fill_components();
        Some(ty)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn variant_layout(name: String, arguments: Vec<Self>, values: Vec<u32>, tag: Scalar, variants: Vec<Variant>) -> Option<Self> {
        let (payload_size, payload_alignment) = variants.iter().fold((0, 0), |(size, align), variant| {
            (size.max(variant.ty.size), align.max(variant.ty.align))
        });
        let layout = unsafe { tagged_variant_layout(payload_size, payload_alignment) };
        let size = u32::try_from(layout.size).ok()?;
        let align = layout.alignment;
        Some(Self {
            name, arguments, values, size, align,
            components: vec![(tag, 0)],
            fields: Vec::new(), view: None, array: None, enumeration: false, result: false,
            variants, dataset: None, cleanup: None, function: None,
        })
    }
    /// Materialize the scalar value slots along inline storage. A view already
    /// has its own slots, and its referenced storage stays in layout form.
    #[doc(hidden)]
    pub fn complete_value_layout(&mut self) {
        layout::fill_value_slots(self, true);
    }

    #[cfg(feature = "compiler")]
    pub(crate) fn fill_components(&mut self) {
        layout::fill_value_slots(self, false);
    }

    /// Compare the referenced storage with actual storage, whose derived
    /// array, record and variant slots may already have been materialized.
    pub fn same_layout(&self, other: &Self) -> bool {
        layout::same_layout(self, other)
    }
    pub fn has_variants(&self) -> bool { !self.variants.is_empty() }
    pub(crate) fn variant_tag(&self, bits: u32) -> u32 {
        if self.components[0].0 == Scalar::Bool { u32::from(bits != 0) } else { bits }
    }
    pub(crate) fn variant_payload_offset(&self) -> u32 {
        // The four-byte tag occupies the first aligned block.
        self.align
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn zero_valid(&self)->bool { properties::zero_valid(self) }
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
            variants: Vec::new(),
            dataset: None, cleanup: None, function: None,
        }
    }
    pub fn scalar(s: Scalar) -> Self {
        Self {
            name: s.name().into(),
            arguments: Vec::new(),
            values: Vec::new(),
            size: s.size(),
            align: s.align(),
            components: vec![(s, 0)],
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            variants: Vec::new(),
            dataset: None, cleanup: None, function: None,
        }
    }
    pub fn vector(s: Scalar, n: usize) -> Self {
        if n == 1 {
            return Self::scalar(s);
        }
        let layout = unsafe { vector_layout(s.size(), s.align(), n as u64) };
        Self {
            name: match s {
                Scalar::F32 => format!("float{n}"),
                Scalar::U32 => format!("uint{n}"),
                Scalar::I32 => format!("int{n}"),
                Scalar::Bool => format!("bool{n}"),
                _ => format!("{}x{n}", s.name()),
            },
            arguments: Vec::new(),
            values: Vec::new(),
            size: layout.size as u32,
            align: layout.alignment,
            components: properties::vector_slots(s, n),
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            variants: Vec::new(),
            dataset: None, cleanup: None, function: None,
        }
    }
    pub fn matrix(rows: usize, columns: usize) -> Self {
        let row = Self::vector(Scalar::F32, columns);
        let layout = unsafe { fixed_array_layout(row.size, row.align, rows as u32) };
        let mut ty = Self::unit();
        ty.name = format!("float{rows}x{columns}");
        ty.size = layout.size as u32;
        ty.align = row.align;
        ty.fields = properties::matrix_fields(&row, rows, layout.stride as u32);
        layout::fill_value_slots(&mut ty, false);
        ty
    }
    pub fn matrix_shape(&self) -> Option<(usize, usize)> {
        let name = crate::builtin_types::Builtin::recognize(&self.name);
        (name.kind == crate::builtin_types::Kind::Numeric && name.rows > 1)
            .then_some((name.rows as usize, name.columns as usize))
    }
    pub fn builtin(name: &str) -> Option<Self> {
        Self::from_builtin(crate::builtin_types::Builtin::recognize(name))
    }
    pub(crate) fn from_builtin(builtin: crate::builtin_types::Builtin) -> Option<Self> {
        use crate::builtin_types::Kind;
        match builtin.kind {
            Kind::Numeric => Some(if builtin.rows > 1 {
                Self::matrix(builtin.rows as usize, builtin.columns as usize)
            } else {
                Self::vector(builtin.scalar(), builtin.columns as usize)
            }),
            Kind::Address => {
                let mut ty = Self::scalar(Scalar::U64);
                ty.name = "address".into();
                Some(ty)
            }
            Kind::Handle => {
                let mut ty = Self::vector(Scalar::U32, 2);
                ty.name = "handle".into();
                Some(ty)
            }
            Kind::RayHit => Some(Self::ray_hit()),
            _ => None,
        }
    }
    fn ray_hit() -> Self {
        let mut ty = Self::unit();
        ty.name = "RayHit".into();
        let shape = properties::ray_hit_fields(&mut ty);
        ty.size = shape.size as u32;
        ty.align = shape.alignment;
        layout::fill_value_slots(&mut ty, false);
        ty
    }
    pub fn is_address(&self) -> bool {
        crate::builtin_types::Builtin::recognize(&self.name).kind == crate::builtin_types::Kind::Address
    }
    pub fn is_handle(&self) -> bool {
        crate::builtin_types::Builtin::recognize(&self.name).kind == crate::builtin_types::Kind::Handle
    }
    pub fn is_c_function(&self) -> bool {
        self.function.is_some()
    }
    pub fn is_numeric(&self) -> bool { properties::numeric(self) }
    pub fn contains_arrays(&self) -> bool { properties::contains(self, properties::Query::Arrays) }
    pub fn contains_views(&self) -> bool { properties::contains(self, properties::Query::Views) }
    pub fn contains_borrows(&self) -> bool { properties::contains(self, properties::Query::Borrows) }
    /// Borrow-bearing values in this layout, without following their pointees.
    #[cfg(feature = "compiler")]
    pub(crate) fn borrow_fields(&self) -> Vec<(u32, &Ty)> { properties::borrows(self) }
    #[cfg(feature = "compiler")]
    pub(crate) fn visit_borrow_fields<'a>(&'a self, visit: impl FnMut(u32, &'a Ty)) {
        properties::visit_borrows(self, visit);
    }
    /// 全部可能的view字段，仅用于类型能力；重叠载荷不同时活动。
    pub fn view_fields(&self) -> Vec<(u32, &Ty)> { crate::codec::types::view_fields(self) }
    /// 只遍历本条记录的实际活动载荷，引用子存储由其自身类型和所有者处理。
    pub fn visit_view_fields<'a>(&'a self, words:&[u32], field:impl FnMut(u32,&'a Ty))->Result<(),String> {
        crate::codec::types::visit_view_fields(self,words,field)
    }
    /// 已编译逻辑参数按真实字节位置借用同一声明字段；不改写物理根类型。
    pub fn field_layout_at(&self, offset: u32, expected: &Ty) -> Option<&Ty> {
        properties::field_layout_at(self, offset, expected)
    }
    pub fn field(&self, path: &str) -> Option<(u32, &Ty)> {self.field_in(path,None)}
    /// 与field共用声明路径，在绑定原生引用时同时选择实际活动分支。
    pub fn field_at(&self,path:&str,words:&[u32])->Option<(u32,&Ty)> {self.field_in(path,Some(words))}
    fn field_in(&self, path: &str, words: Option<&[u32]>) -> Option<(u32, &Ty)> {
        properties::field(self, path, words)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnaryOp {
    FloatNegate,
    SignedNegate,
    UnsignedNegate,
    LogicalNot,
    BitNot,
    Bitcast,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
                if scalar.is_float() {
                    FAdd
                } else {
                    IAdd
                }
            }
            "-" => {
                if scalar.is_float() {
                    FSub
                } else {
                    ISub
                }
            }
            "*" => {
                if scalar.is_float() {
                    FMul
                } else {
                    IMul
                }
            }
            "/" if scalar.is_float() => FDiv,
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
                if scalar.is_signed() {
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
    Constant(NonZeroU64),
    /// 当前控制域内已证明非零的整数 SSA 值；不得移出该证明成立的控制流。
    Value(Reg),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    Ceil,
}
impl MathOp {
    pub fn from_source(name: &str) -> Option<Self> { crate::codec::types::SourceMath::recognize(name).operation() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementStride {
    Constant(u64),
    Value([Reg; 2]),
}
impl ElementStride {
    #[cfg(feature = "compiler")]
    pub(crate) fn all_operands(&self,f:&mut impl FnMut(Reg)->bool)->bool{
        dataflow::stride_operands(self, f)
    }
    #[cfg(not(feature = "compiler"))]
    pub(crate) fn all_operands(&self,f:&mut impl FnMut(Reg)->bool)->bool{
        match self {Self::Constant(_)=>true,Self::Value(words)=>words.iter().all(|reg|f(*reg))}
    }
    #[cfg(not(feature = "compiler"))]
    pub(crate) fn map(&mut self,f:&impl Fn(Reg)->Reg){
        if let Self::Value(words)=self{words.iter_mut().for_each(|reg|*reg=f(*reg));}
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
        stride: ElementStride,
    },
    Pointer {
        address: [Reg; 2],
        index: Reg,
        stride: u32,
        aliases: Vec<MemoryAlias>,
    },
    Tile {
        tile: usize,
        index: Reg,
    },
}
impl IndexedSource {
    #[cfg(feature = "compiler")]
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        dataflow::indexed_operands(self, f)
    }
    #[cfg(not(feature = "compiler"))]
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        match self {
            Self::Data { index, .. } | Self::Tile { index, .. } => f(*index),
            Self::View { address, index, stride } =>
                address.iter().all(|reg| f(*reg)) && stride.all_operands(f) && f(*index),
            Self::Pointer { address, index, .. } => f(address[0]) && f(address[1]) && f(*index),
        }
    }
    #[cfg(not(feature = "compiler"))]
    pub(crate) fn map(&mut self, r: &impl Fn(Reg) -> Reg) {
        match self {
            Self::Data { index, .. } | Self::Tile { index, .. } => *index = r(*index),
            Self::View { address, index, stride } => {
                address.iter_mut().for_each(|v| *v = r(*v));
                stride.map(r);
                *index = r(*index);
            }
            Self::Pointer { address, index, .. } => {
                address.iter_mut().for_each(|v| *v = r(*v));
                *index = r(*index);
            }
        }
    }
    pub(crate) fn aliases(&self) -> impl Iterator<Item = MemoryAlias> + '_ {
        let direct = match self {
            Self::Data { binding, .. } => Some(MemoryAlias::Data(*binding)),
            _ => None,
        };
        let aliases = match self {
            Self::Pointer { aliases, .. } => aliases.as_slice(),
            _ => &[],
        };
        direct.into_iter().chain(aliases.iter().copied())
    }
    #[cfg(not(feature = "compiler"))]
    pub(crate) fn map_locals(&mut self, s: &impl Fn(usize) -> usize) {
        if let Self::Pointer { aliases, .. } = self {
            for alias in aliases {
                alias.map_locals(s);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TextureReadWidth {
    Byte = 1,
    Half = 2,
    Word = 4,
}

#[derive(Clone, Copy, Debug)]
pub struct TextureSource {
    pub binding: usize,
    pub element: Option<Reg>,
    /// 普通CPU形参借用的原BufferAddress列表；绑定入口由原范围参数表提供。
    pub parameter: Option<Reg>,
}
#[derive(Clone, Debug)]
pub struct TextureBinding {
    pub name: String,
    pub count: Option<u32>,
    pub dimension: TextureDimension,
    pub format: Option<crate::ast::TextureFormat>,
}
impl TextureBinding {
    pub fn elements(&self) -> u32 {
        self.count.unwrap_or(1)
    }
}
#[derive(Clone, Debug)]
pub struct Inst {
    pub kind: InstKind,
    // Semantic requirement from this operation's operand and result facts, not source provenance.
    pub(crate) float_preserve: bool,
    #[cfg(feature = "compiler")]
    pub(crate) source_site: Option<std::sync::Arc<SourceSite>>,
    #[cfg(feature = "compiler")]
    pub(crate) generated_origin: Option<std::sync::Arc<GeneratedOrigin>>,
}
impl Inst {
    pub fn new(kind: InstKind) -> Self {
        Self { kind, float_preserve: true, #[cfg(feature = "compiler")] source_site: None, #[cfg(feature = "compiler")] generated_origin: None }
    }
    #[cfg(feature = "compiler")]
    pub fn source_site(&self) -> Option<&SourceSite> { self.source_site.as_deref() }
    #[cfg(feature = "compiler")]
    pub(crate) fn generated(kind: InstKind, origin: std::sync::Arc<GeneratedOrigin>) -> Self {
        Self { kind, float_preserve: true, source_site: None, generated_origin: Some(origin) }
    }
    #[cfg(feature = "compiler")]
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> { self.generated_origin.as_deref() }
}
#[derive(Clone, Debug)]
pub enum InstKind {
    /// 普通CPU调用借用原纹理绑定项或固定数组首项，不交接所有权。
    TextureBindingAddress { dst: Reg, texture: TextureSource },
    TextureSample {
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: Vec<Reg>,
        lod: Reg,
    },
    TextureSize {
        result: Vec<Reg>,
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
        coordinate: Vec<Reg>,
        level: Reg,
        width: TextureReadWidth,
    },
    TextureFetch {
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: Vec<Reg>,
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
    Call(Call),
    ViewLoad {
        dst: Reg,
        address: [Reg; 2],
        index: Reg,
        stride: ElementStride,
        word: u32,
    },
    RayQuery {
        scene: usize,
        origin: [Reg; 3],
        direction: [Reg; 3],
        minimum: Reg,
        maximum: Reg,
        cull_back_faces: Option<Reg>,
        result: RayResult,
    },
    Constant(Reg, Scalar, Bits),
    FunctionAddress(Reg, CallTarget),
    PositionAddress { dst: Reg, address: CallReference },
    /// Actual CPU BufferAddress stride; metadata only, never a payload read.
    BufferStride(Reg, usize),
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
    /// 将唯一拥有错误交回本普通函数的真实调用者，不退出最近的Scope。
    Raise(Reg),
    #[cfg(feature = "compiler")]
    Cleanup { slot:usize, body:Vec<Inst>, failure:Option<Box<crate::check::FailureScope>> },
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
        self.values.ty.size.div_ceil(4) as usize + 2
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
        self.ty.size.div_ceil(4) as usize + usize::from(self.head_word.is_some())
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
            || program.buffers()[self.values.target].stride != self.ty.size.div_ceil(4) * 4
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
pub enum SortOrder {
    Ascending,
    Descending,
}
#[derive(Clone, Debug)]
pub struct Sorting {
    pub source: usize,
    pub keys: Vec<(u32, Scalar, SortOrder)>,
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
    pub domain_region: Option<(u32,u32)>,
    pub count: PrefixCount,
}
impl PrefixRelation {
    pub fn domain_layout(&self,buffers:&[Buffer])->(u32,u32){self.domain_region.unwrap_or((0,buffers[self.domain].ty.size))}
    pub fn covers(&self,required:&Self,buffers:&[Buffer])->bool {
        let(a,bytes)=self.domain_layout(buffers);let(b,needed)=required.domain_layout(buffers);
        self.domain==required.domain&&self.count==required.count&&a<=b&&u64::from(b)+u64::from(needed)<=u64::from(a)+u64::from(bytes)
    }
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
    #[cfg(feature = "compiler")]
    pub(crate) operation_source: Option<std::sync::Arc<SourceSite>>,
    #[cfg(feature = "compiler")]
    pub(crate) generated_origin: Option<std::sync::Arc<GeneratedOrigin>>,
    pub boundary_before: bool,
    pub registers: Vec<Scalar>,
    // Key is the first logical component after this declaration; padding has no slot.
    pub locals: BTreeMap<usize, Ty>,
    pub code: Vec<Inst>,
    pub reads: Vec<usize>,
    pub writes: Vec<usize>,
    pub write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    pub prefix_guarantees: Vec<PrefixRelation>,
    pub element_guarantees: Vec<ElementFact>,
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
#[repr(u32)]
pub enum CollectiveOperator {
    Sum,
    Min,
    Max,
    BitAnd,
    BitOr,
    BitXor,
}
impl CollectiveOperator {
    pub fn identity(self, scalar: Scalar) -> Bits {
        match self {
            Self::Sum | Self::BitOr | Self::BitXor => 0,
            Self::BitAnd => scalar.bit_mask(),
            Self::Min => {
                if scalar.is_float() {
                    if scalar == Scalar::F64 { f64::INFINITY.to_bits() } else { Bits::from(f32::INFINITY.to_bits()) }
                } else if scalar.is_signed() {
                    scalar.order_bias() - 1
                } else {
                    scalar.bit_mask()
                }
            }
            Self::Max => {
                if scalar.is_float() {
                    if scalar == Scalar::F64 { f64::NEG_INFINITY.to_bits() } else { Bits::from(f32::NEG_INFINITY.to_bits()) }
                } else if scalar.is_signed() {
                    scalar.order_bias()
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
#[cfg(feature = "compiler")]
pub(crate) mod dataflow;
#[cfg(feature = "compiler")]
pub(crate) mod rewrite;
mod tile;
pub use tile::SharedWord;
#[cfg(feature = "compiler")]
pub(crate) use tile::{PRIVATE_LIVE_HOST, PrivateLiveHost, TilePhase};
#[cfg(feature = "compiler")]
#[cfg(feature = "compiler")]
pub(crate) use tile::{PlacedTiles, PrivateSlot};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
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
#[cfg(not(feature = "compiler"))]
const APPEND_SUBGROUP_OPERATIONS: [SubgroupOp; 4] = [
    SubgroupOp::Count, SubgroupOp::PrefixCount, SubgroupOp::Elect, SubgroupOp::BroadcastFirst,
];
#[derive(Clone, Copy, Debug, Default)]
pub struct SubgroupCapabilities {
    pub basic: bool,
    pub vote: bool,
    pub arithmetic: bool,
    pub ballot: bool,
    pub shuffle: bool,
    pub shuffle_relative: bool,
    pub extended_types: bool,
}
impl Stage {
    /// 对实际消费的命中字段发射设备读取；未消费的顶点和矩阵不生成读取。
    pub fn ray_result_queries(&self, range: std::ops::Range<usize>) -> BTreeSet<Reg> {
        let mut queries = BTreeSet::new();
        self.visit_ray_result_queries(&[range], &mut |_, query| { queries.insert(query); true });
        queries
    }
    fn visit_ray_result_queries(&self, ranges: &[std::ops::Range<usize>],
        visit: &mut impl FnMut(usize, Reg) -> bool) -> bool {
        #[cfg(feature = "compiler")]
        { dataflow::ray_result_queries(self, ranges, visit) }
        #[cfg(not(feature = "compiler"))]
        {
            fn collect(code: &[Inst], used: &mut BTreeSet<Reg>) {
                for inst in code {
                    inst.all_operands(&mut |reg| { used.insert(reg); true });
                    match &inst.kind {
                        InstKind::Scope(body) => collect(body, used),
                        InstKind::If { yes, no, .. } => { collect(yes, used); collect(no, used); }
                        InstKind::Loop { condition, body, .. } => { collect(condition, used); collect(body, used); }
                        _ => {}
                    }
                }
            }
            fn has_closest(code: &[Inst]) -> bool {
                code.iter().any(|inst| match &inst.kind {
                    InstKind::RayQuery { result: RayResult::Closest(_), .. } => true,
                    InstKind::Scope(body) => has_closest(body),
                    InstKind::If { yes, no, .. } => has_closest(yes) || has_closest(no),
                    InstKind::Loop { condition, body, .. } => has_closest(condition) || has_closest(body),
                    _ => false,
                })
            }
            fn select(code: &[Inst], ranges: &[std::ops::Range<usize>], used: &BTreeSet<Reg>,
                visit: &mut impl FnMut(usize, Reg) -> bool) -> bool {
                for inst in code {
                    match &inst.kind {
                        InstKind::RayQuery { result: RayResult::Closest(regs), .. } => {
                            for (group, range) in ranges.iter().enumerate() {
                                if regs[range.clone()].iter().any(|reg| used.contains(reg)) && !visit(group, regs[0]) { return false; }
                            }
                        }
                        InstKind::Scope(body) => { if !select(body, ranges, used, visit) { return false; } }
                        InstKind::If { yes, no, .. } => {
                            if !select(yes, ranges, used, visit) || !select(no, ranges, used, visit) { return false; }
                        }
                        InstKind::Loop { condition, body, .. } => {
                            if !select(condition, ranges, used, visit) || !select(body, ranges, used, visit) { return false; }
                        }
                        _ => {}
                    }
                }
                true
            }
            if !has_closest(&self.code) { return true; }
            let mut used = BTreeSet::new();
            collect(&self.code, &mut used);
            select(&self.code, ranges, &used, visit)
        }
    }
    pub fn requires_maximal_reconvergence(&self) -> bool {
        #[cfg(feature = "compiler")]
        { dataflow::maximal_reconvergence(self) }
        #[cfg(not(feature = "compiler"))]
        {
            fn uses(code: &[Inst]) -> bool {
                code.iter().any(|i| match &(i).kind {
                    InstKind::Append(_) => true,
                    InstKind::Scope(body) => uses(body),
                    InstKind::If { yes, no, .. } => uses(yes) || uses(no),
                    InstKind::Loop {
                        condition, body, ..
                    } => uses(condition) || uses(body),
                    _ => false,
                })
            }
            uses(&self.code)
        }
    }
    pub fn requires_float_preserve(&self, scalar: Scalar) -> bool {
        #[cfg(feature = "compiler")]
        { dataflow::float_preserve(self, scalar) }
        #[cfg(not(feature = "compiler"))]
        {
        fn arithmetic(code: &[Inst], registers: &[Scalar], scalar: Scalar) -> bool {
            code.iter().any(|inst| match &(inst).kind {
                InstKind::Math { dst, op, .. } => inst.float_preserve && registers[*dst] == scalar && *op != MathOp::Abs,
                InstKind::Binary { a, .. } => inst.float_preserve && registers[*a] == scalar,
                InstKind::Cast { dst, src } => {
                    registers[*src].is_float() && !registers[*dst].is_integer() && ((registers[*dst] == scalar) != (registers[*src] == scalar))
                }
                InstKind::TileCollective(c) => {
                    c.results.iter().any(|(_, r)| registers[*r] == scalar)
                }
                InstKind::Subgroup { dst, op, .. } => {
                    registers[*dst] == scalar
                        && matches!(
                            op,
                            SubgroupOp::Sum
                                | SubgroupOp::ExclusiveSum
                                | SubgroupOp::Min
                                | SubgroupOp::Max
                        )
                }
                InstKind::Scope(body) => arithmetic(body, registers, scalar),
                InstKind::If { yes, no, .. } => arithmetic(yes, registers, scalar) || arithmetic(no, registers, scalar),
                InstKind::Loop {
                    condition, body, ..
                } => arithmetic(condition, registers, scalar) || arithmetic(body, registers, scalar),
                _ => false,
            })
        }
        matches!(&self.operation, Operation::Reduce(r) if r.ty.components[0].0 == scalar)
            || matches!(&self.operation, Operation::CustomReduce(r) | Operation::CustomScan(CustomScan { values:r, .. }) | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values:r, .. }) if [&r.identity,&r.combine].into_iter().chain(&r.project).any(|body| arithmetic(&body.code,&body.registers,scalar)))
            || matches!(&self.operation, Operation::SegmentedReduce(r) if r.values.ty.components[0].0 == scalar)
            || matches!(&self.operation, Operation::ScanExclusive(s) if s.ty.components[0].0 == scalar)
            || arithmetic(&self.code, &self.registers, scalar)
        }
    }
    pub fn has_tile_scope(&self) -> bool {
        self.has_tile_scope_for(&self.code)
    }

    fn has_tile_scope_for(&self, code: &[Inst]) -> bool {
        #[cfg(feature = "compiler")]
        { dataflow::tile_scope(code, !self.tiles.is_empty()) }
        #[cfg(not(feature = "compiler"))]
        {
            fn uses(code: &[Inst], has_tiles: bool) -> bool {
                code.iter().any(|inst| match &inst.kind {
                    InstKind::TileLength(_) | InstKind::TileLane(_) | InstKind::TilePublish(_)
                    | InstKind::TileBarrier | InstKind::TileCollective(_) | InstKind::TileLoad { .. }
                    | InstKind::Atomic(Atomic { target: IndexedSource::Tile { .. }, .. })
                    | InstKind::LoadIndexed { source: IndexedSource::Tile { .. }, .. } => true,
                    InstKind::Append(_) | InstKind::Subgroup { .. } => has_tiles,
                    InstKind::Scope(body) => uses(body, has_tiles),
                    InstKind::If { yes, no, .. } => uses(yes, has_tiles) || uses(no, has_tiles),
                    InstKind::Loop { condition, body, .. } => uses(condition, has_tiles) || uses(body, has_tiles),
                    _ => false,
                })
            }
            uses(code, !self.tiles.is_empty())
        }
    }
    pub fn storage_tiles(&self) -> std::collections::BTreeSet<usize> {
        #[cfg(feature = "compiler")]
        { dataflow::storage_tiles(self) }
        #[cfg(not(feature = "compiler"))]
        {
            fn collect(code: &[Inst], output: &mut std::collections::BTreeSet<usize>) {
                for inst in code {
                    match &inst.kind {
                        InstKind::Atomic(Atomic { target: IndexedSource::Tile { tile, .. }, .. })
                        | InstKind::LoadIndexed { source: IndexedSource::Tile { tile, .. }, .. }
                        | InstKind::TileLoad { tile, .. } => { output.insert(*tile); }
                        InstKind::TileCollective(value) => {
                            if !value.results.is_empty() { output.insert(value.tile); }
                        }
                        InstKind::Scope(body) => collect(body, output),
                        InstKind::If { yes, no, .. } => { collect(yes, output); collect(no, output); }
                        InstKind::Loop { condition, body, .. } => { collect(condition, output); collect(body, output); }
                        _ => {}
                    }
                }
            }
            let mut output = std::collections::BTreeSet::new();
            collect(&self.code, &mut output);
            output
        }
    }
    pub(crate) const DEFAULT_WORKGROUP_WIDTH: u32 = 64;

    pub(crate) fn tile_workgroup_width(&self) -> Option<u32> {
        self.tile_workgroup_width_for(&self.code)
    }

    pub(crate) fn tile_workgroup_width_for(&self, code: &[Inst]) -> Option<u32> {
        self.has_tile_scope_for(code).then(|| self.tiles[0].width)
    }

    pub fn workgroup_width(&self) -> u32 {
        self.tile_workgroup_width().unwrap_or(Self::DEFAULT_WORKGROUP_WIDTH)
    }
    pub fn storage_words(&self) -> std::collections::BTreeSet<SharedWord> {
        #[cfg(feature = "compiler")]
        { dataflow::storage_words(self) }
        #[cfg(not(feature = "compiler"))]
        {
        fn visit(stage: &Stage, code: &[Inst], words: &mut std::collections::BTreeSet<SharedWord>) {
            for inst in code {
                match &(inst).kind {
                    InstKind::Atomic(Atomic {
                        target: IndexedSource::Tile { tile, .. },
                        word, scalar,
                        offset: None,
                        ..
                    }) => {
                        words.extend((0..scalar.size().div_ceil(4)).map(|part| stage.shared_word(*tile, *word + part * 4)));
                    }
                    InstKind::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | InstKind::Atomic(Atomic {
                        target: IndexedSource::Tile { tile, .. },
                        offset: Some(_),
                        ..
                    }) => {
                        words.extend(
                            stage.tiles[*tile].ty.components.iter().flat_map(|(scalar, byte)|
                                (0..scalar.size().div_ceil(4)).map(move |part|
                                    stage.shared_word(*tile, *byte + part * 4))),
                        );
                    }
                    InstKind::TileLoad { dst, tile, word, .. } => {
                        words.extend((0..stage.registers[*dst].size().div_ceil(4))
                            .map(|part| stage.shared_word(*tile, *word + part * 4)));
                    }
                    InstKind::TileCollective(c) => {
                        words.extend(
                            c.results
                                .iter()
                                .flat_map(|(word, result)| (0..stage.registers[*result].size().div_ceil(4))
                                    .map(move |part| stage.shared_word(c.tile, *word + part * 4))),
                        );
                    }
                    InstKind::Scope(body) => visit(stage, body, words),
                    InstKind::If { yes, no, .. } => {
                        visit(stage, yes, words);
                        visit(stage, no, words);
                    }
                    InstKind::Loop {
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
    }
    pub fn subgroup_operations(&self) -> std::collections::BTreeSet<SubgroupOp> {
        #[cfg(feature = "compiler")]
        { dataflow::subgroup_operations(self) }
        #[cfg(not(feature = "compiler"))]
        {
        fn visit(code: &[Inst], ops: &mut std::collections::BTreeSet<SubgroupOp>) {
            for inst in code {
                match &(inst).kind {
                    InstKind::Append(_) => {
                        ops.extend(APPEND_SUBGROUP_OPERATIONS);
                    }
                    InstKind::Subgroup { op, .. } => {
                        ops.insert(*op);
                    }
                    InstKind::Scope(body) => visit(body, ops),
                    InstKind::If { yes, no, .. } => {
                        visit(yes, ops);
                        visit(no, ops);
                    }
                    InstKind::Loop {
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
}
impl SubgroupCapabilities {
    pub fn from_operations(ops: impl IntoIterator<Item = SubgroupOp>) -> Self {
        #[cfg(feature = "compiler")]
        { dataflow::subgroup_from_operations(ops) }
        #[cfg(not(feature = "compiler"))]
        {
        let mut caps = Self::default();
        for op in ops {
            caps.include_operation(op);
        }
        caps
        }
    }
    #[cfg(not(feature = "compiler"))]
    fn include_operation(&mut self, op: SubgroupOp) {
            self.basic = true;
            match op {
                SubgroupOp::Any | SubgroupOp::All => self.vote = true,
                SubgroupOp::Count | SubgroupOp::PrefixCount | SubgroupOp::BroadcastFirst => {
                    self.ballot = true
                }
                SubgroupOp::Shuffle | SubgroupOp::ShuffleXor => self.shuffle = true,
                SubgroupOp::ShuffleUp | SubgroupOp::ShuffleDown => self.shuffle_relative = true,
                SubgroupOp::Sum
                | SubgroupOp::ExclusiveSum
                | SubgroupOp::Min
                | SubgroupOp::Max
                | SubgroupOp::BitAnd
                | SubgroupOp::BitOr
                | SubgroupOp::BitXor
                | SubgroupOp::ExclusiveBitAnd
                | SubgroupOp::ExclusiveBitOr
                | SubgroupOp::ExclusiveBitXor => self.arithmetic = true,
                SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id | SubgroupOp::Elect => {}
            }
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) name: String,
    #[cfg(feature = "compiler")]
    pub(crate) source_origin: Option<SourceOrigin>,
    #[cfg(feature = "compiler")]
    pub(crate) generated_origin: Option<std::sync::Arc<GeneratedOrigin>>,
    pub(crate) target: Target,
    pub(crate) buffers: Vec<Buffer>,
    pub(crate) accelerations: Vec<String>,
    pub(crate) textures: Vec<TextureBinding>,
    pub(crate) uniforms: Vec<Uniform>,
    pub(crate) uniform_bytes: u32,
    pub(crate) native_frame: Option<u32>,
    pub(crate) source_stages: usize,
    pub(crate) stages: Vec<Stage>,
    pub(crate) imports: BTreeMap<String, Vec<u8>>,
    #[cfg(feature = "compiler")]
    pub(crate) functions: std::sync::Arc<[FunctionBody]>,
    #[cfg(feature = "compiler")]
    pub(crate) resources: Option<Box<crate::application::Resources>>,
    pub(crate) prefix_requirements: Vec<PrefixRelation>,
    pub(crate) prefix_guarantees: Vec<PrefixRelation>,
    pub(crate) element_requirements: Vec<ElementFact>,
    pub(crate) element_guarantees: Vec<ElementFact>,
    pub(crate) length_requirements: Vec<(usize, usize)>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ExecutionBuffers {
    pub(crate) indirect: usize,
    pub(crate) auxiliary: usize,
}
#[cfg(feature = "compiler")]
impl ExecutionBuffers {
    pub(crate) fn selection(self) -> usize { self.indirect + 1 }
    pub(crate) fn scan_source(self) -> usize { self.indirect + 2 }
}
impl Program {
    pub(crate) fn write_regions(&self) -> BTreeMap<usize, BTreeSet<(u32, u32)>> {
        Self::stage_write_regions(&self.stages)
    }
    pub(crate) fn stage_write_regions(stages: &[Stage]) -> BTreeMap<usize, BTreeSet<(u32, u32)>> {
        let mut regions = BTreeMap::new();
        for stage in stages {
            for (&binding, written) in &stage.write_regions {
                regions.entry(binding).or_insert_with(BTreeSet::new).extend(written.iter().copied());
            }
        }
        regions
    }
    #[cfg(feature = "compiler")]
    pub fn source_origin(&self) -> Option<&SourceOrigin> { self.source_origin.as_ref() }
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
        self.stages.iter().any(|stage|
            !stage.visit_ray_result_queries(&[6..15], &mut |_, _| false))
    }
    pub fn requires_texture_indexing(&self) -> bool {
        self.textures.iter().any(|t| t.count.is_some())
    }
    pub fn requires_maximal_reconvergence(&self) -> bool {
        #[cfg(feature = "compiler")]
        { dataflow::program_maximal_reconvergence(self) }
        #[cfg(not(feature = "compiler"))]
        {
            self.stages
                .iter()
                .any(Stage::requires_maximal_reconvergence)
        }
    }
    pub fn subgroup_capabilities(&self) -> SubgroupCapabilities {
        #[cfg(feature = "compiler")]
        { dataflow::subgroup_capabilities(self) }
        #[cfg(not(feature = "compiler"))]
        {
            fn visit(code: &[Inst], registers: &[Scalar], caps: &mut SubgroupCapabilities) {
                for inst in code {
                    match &inst.kind {
                        InstKind::Append(_) => {
                            for operation in APPEND_SUBGROUP_OPERATIONS { caps.include_operation(operation); }
                        }
                        InstKind::Subgroup { dst, op, .. } => {
                            caps.include_operation(*op);
                            caps.extended_types |= registers[*dst].size() != 4;
                        }
                        InstKind::Scope(body) => visit(body, registers, caps),
                        InstKind::If { yes, no, .. } => { visit(yes, registers, caps); visit(no, registers, caps); }
                        InstKind::Loop { condition, body, .. } => { visit(condition, registers, caps); visit(body, registers, caps); }
                        _ => {}
                    }
                }
            }
            let mut caps = SubgroupCapabilities::default();
            for stage in &self.stages { visit(&stage.code, &stage.registers, &mut caps); }
            caps
        }
    }
    /// 内部设备绑定：集合中间量与间接域参数各有一个必要槽，不占源码参数。
    pub fn indirect_buffer(&self) -> usize {
        #[cfg(feature = "compiler")]
        { dataflow::indirect_buffer(self) }
        #[cfg(not(feature = "compiler"))]
        { self.buffers.len() + usize::from(self.stages.iter().any(Self::needs_intermediate)) }
    }
    pub fn selection_buffer(&self) -> usize { self.indirect_buffer() + 1 }
    pub fn scan_source_buffer(&self) -> usize { self.indirect_buffer() + 2 }
    pub fn auxiliary_buffers(&self) -> usize { self.execution_buffers().auxiliary }
    #[cfg(not(feature = "compiler"))]
    fn needs_intermediate(stage: &Stage) -> bool {
        stage.selection.is_some() || matches!(stage.operation,
            Operation::ScanExclusive(_) | Operation::CustomScan(_)
            | Operation::CustomSegmentedReduce(_) | Operation::SegmentedReduce(_)
            | Operation::Compact(_) | Operation::StableSort(_))
    }
    pub(crate) fn execution_buffers(&self) -> ExecutionBuffers {
        #[cfg(feature = "compiler")]
        { dataflow::execution_buffers(self) }
        #[cfg(not(feature = "compiler"))]
        {
            let (mut intermediate, mut prefix, mut selection, mut scan_alias) = (false, false, false, false);
            for stage in &self.stages {
                intermediate |= Self::needs_intermediate(stage);
                selection |= stage.selection.is_some();
                prefix |= stage.prefix.is_some() || stage.selection.is_some();
                scan_alias |= match &stage.operation {
                    Operation::ScanExclusive(v) => v.values.source == v.values.target,
                    Operation::CustomScan(v) => v.values.fields.source == v.values.fields.target,
                    Operation::SegmentedReduce(v) => v.values.fields.source == v.values.fields.target,
                    Operation::CustomSegmentedReduce(v) => v.values.fields.source == v.values.fields.target,
                    _ => false,
                };
            }
            let indirect = self.buffers.len() + usize::from(intermediate);
            let mut auxiliary = usize::from(intermediate) + usize::from(prefix) + usize::from(selection);
            if scan_alias { auxiliary = auxiliary.max(indirect + 3 - self.buffers.len()); }
            ExecutionBuffers { indirect, auxiliary }
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
    pub fn element_requirements(&self) -> &[ElementFact] { &self.element_requirements }
    pub fn element_guarantees(&self) -> &[ElementFact] { &self.element_guarantees }
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
    #[cfg(feature = "compiler")]
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
                "  texture {}: Read {name} : {}{}\n",
                self.buffers.len() + self.accelerations.len() + i, texture.dimension.name(),
                texture.format.map(|format| format!("<{}>", format.name())).unwrap_or_default()
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
                    "  {kind} prefix({} region {:?}, {} word {})\n",
                    self.buffers[relation.domain].name, relation.domain_layout(&self.buffers),
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
                    "    tile_width={}, shared_values={}, phases={}, tile_word_bytes={}, collective_scratch_bytes={}\n",
                    s.workgroup_width(),
                    s.storage_tiles().len(),
                    s.code.iter().filter(|inst| matches!(&inst.kind, InstKind::TilePublish(_) | InstKind::TileCollective(_) | InstKind::TileBarrier)).count() + 1,
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

impl Ty {
    pub(crate) fn indexed_count(&self) -> Option<u32> {
        if self.array.is_some() {
            Some(self.values[0])
        } else if self.is_numeric() && self.components.len() > 1 {
            Some(self.components.len() as u32)
        } else {
            None
        }
    }

    pub(crate) fn indexed_element(&self) -> Option<(Self, u32)> {
        let count = self.indexed_count()?;
        let element = self.array.as_deref().cloned()
            .unwrap_or_else(|| Self::scalar(self.components[0].0));
        Some((element, count))
    }
}

impl CFunctionType {

    pub(crate) fn into_syntax(self) -> crate::ast::TypeName { crate::codec::types::function_syntax(self) }

    #[cfg(feature = "compiler")]
    pub(crate) fn into_ty(self) -> Ty {
        let name=crate::codec::types::function_name(&self);
        let mut ty=Ty::scalar(Scalar::U64);
        ty.name=name;
        ty.function=Some(Box::new(self));
        ty
    }
}

mod inspect;
