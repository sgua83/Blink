//! B selects specialization and instance formation. Native operations lend the
//! original World and Ty values, then write their one actual result container.
//! B bool crosses this C ABI as u32; Rust booleans stay inside the bridge.
use super::*;
use crate::{
    Definition, Diagnostic,
    check::generics::Actual,
    syntax::{self, wire},
};
use std::{ffi::c_void, mem, ptr, sync::Arc};

#[repr(C)]
#[derive(Clone, Copy)]
struct TypeInfo {
    value: *const c_void,
    name: NameBytes,
    arguments: u64,
    values: *const u32,
    value_count: u64,
    parameters: u64,
    flags: u32,
}
impl TypeInfo {
    fn of(value: &Ty) -> Self {
        Self {
            value: (value as *const Ty).cast(),
            name: NameBytes::new(&value.name),
            arguments: value.arguments.len() as u64,
            values: value.values.as_ptr(),
            value_count: value.values.len() as u64,
            parameters: value
                .function
                .as_ref()
                .map_or(0, |f| f.parameters.len() as u64),
            flags: u32::from(value.dataset.is_some())
                | (u32::from(value.view.is_some()) << 1)
                | (u32::from(value.array.is_some()) << 2)
                | (u32::from(value.result) << 3)
                | (u32::from(value.function.is_some()) << 4),
        }
    }
    fn absent() -> Self {
        Self {
            value: ptr::null(),
            name: NameBytes::new(""),
            arguments: 0,
            values: ptr::null(),
            value_count: 0,
            parameters: 0,
            flags: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SemanticInfo {
    value: *const c_void,
    name: NameBytes,
    arguments: u64,
    flags: u32,
    never: u32,
}
impl SemanticInfo {
    fn of(value: &TypeName) -> Self {
        Self {
            value: (value as *const TypeName).cast(),
            name: NameBytes::new(&value.name),
            arguments: value.arguments.len() as u64,
            flags: u32::from(value.dataset)
                | (u32::from(value.view) << 1)
                | (u32::from(value.tile) << 2)
                | (u32::from(value.count.is_some()) << 3)
                | (u32::from(value.contracts.is_some()) << 4)
                | (u32::from(value.canonical) << 5),
            never: u32::from(value.is_never()),
        }
    }
    fn absent() -> Self {
        Self {
            value: ptr::null(),
            name: NameBytes::new(""),
            arguments: 0,
            flags: 0,
            never: 0,
        }
    }
}
#[repr(C)]
struct ActualArgument {
    ty: TypeInfo,
    count: wire::Optional<u32>,
}
#[repr(C)]
struct CfnSlot {
    ty: SemanticInfo,
    access: u32,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum SpecializeNeed {
    Failed = 0,
    TypeChild = 1,
    ValueChild = 2,
    ExplicitType = 3,
    ExplicitDataset = 4,
    FamilyCurrent = 5,
    ExplicitValue = 7,
    Instantiate = 11,
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum MatchNeed {
    Done = 0,
    TypeChild = 1,
    ValueChild = 2,
    FamilyActual = 6,
    ActualType = 8,
    ChildType = 9,
    RecordName = 10,
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum InstanceNeed {
    Done = 0,
    CAbi = 12,
    Interface = 13,
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum MatchMode {
    Type,
    Value,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct SpecializeCursor {
    function: wire::SignatureNode,
    phase: u32,
    actual_count: u64,
    explicit_count: wire::Optional<u64>,
    explicit_types: u32,
    first_value: u64,
    index: u64,
    position: u64,
    name: NameBytes,
    inferred: wire::Optional<u32>,
    scalar: u32,
    unit: u32,
    semantic: SemanticInfo,
    next: wire::SignatureNode,
    next_actual: TypeInfo,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct MatchCursor {
    mode: MatchMode,
    phase: u32,
    source: wire::SignatureNode,
    function_offset: u64,
    actual: TypeInfo,
    name: NameBytes,
    base: NameBytes,
    index: u64,
    position: u64,
    inferred: wire::Optional<u32>,
    scalar: u32,
    unit: u32,
    known: u32,
    next: wire::SignatureNode,
    next_actual: TypeInfo,
    semantic: SemanticInfo,
    next_known: u32,
}
#[repr(C)]
struct InstanceCursor {
    function: wire::SignatureNode,
    phase: u32,
    tracking: u32,
    type_count: u64,
    value_count: u64,
    index: u64,
}
#[repr(C)]
struct Host {
    fail: unsafe extern "C" fn(*mut c_void, Problem, NameBytes, *const c_void, u32, u32),
    actual: unsafe extern "C" fn(*mut c_void, u64) -> ActualArgument,
    explicit_argument: unsafe extern "C" fn(*mut c_void, u64) -> SemanticInfo,
    has_type: unsafe extern "C" fn(*mut c_void) -> u32,
    same_type: unsafe extern "C" fn(*mut c_void, *const c_void) -> u32,
    keep_type: unsafe extern "C" fn(*mut c_void, *const c_void),
    push_type: unsafe extern "C" fn(*mut c_void),
    push_value: unsafe extern "C" fn(*mut c_void, u32),
    child: unsafe extern "C" fn(*const c_void, u64) -> TypeInfo,
    element: unsafe extern "C" fn(*const c_void) -> TypeInfo,
    view: unsafe extern "C" fn(*const c_void) -> TypeInfo,
    dataset_syntax: unsafe extern "C" fn(*const c_void) -> SemanticInfo,
    cfn_slot: unsafe extern "C" fn(*const c_void, u64) -> CfnSlot,
    record_matches: unsafe extern "C" fn(*const c_void, NameBytes) -> u32,
}
fn host() -> Host {
    Host {
        fail,
        actual,
        explicit_argument,
        has_type,
        same_type,
        keep_type,
        push_type,
        push_value,
        child,
        element,
        view,
        dataset_syntax,
        cfn_slot,
        record_matches,
    }
}
#[repr(C)]
struct InstanceHost {
    implementation: unsafe extern "C" fn(*mut c_void),
    external: unsafe extern "C" fn(*mut c_void),
    plain: unsafe extern "C" fn(*mut c_void),
    name: unsafe extern "C" fn(*mut c_void),
    type_argument: unsafe extern "C" fn(*mut c_void, NameBytes),
    value_argument: unsafe extern "C" fn(*mut c_void, NameBytes, u64),
    constant: unsafe extern "C" fn(*mut c_void, NameBytes) -> wire::Optional<u32>,
    nonzero_error: unsafe extern "C" fn(*mut c_void, u64, NameBytes),
    uses: unsafe extern "C" fn(*mut c_void) -> u32,
    construct: unsafe extern "C" fn(*mut c_void),
}
fn instance_host() -> InstanceHost {
    InstanceHost {
        implementation,
        external,
        plain,
        name,
        type_argument,
        value_argument,
        constant,
        nonzero_error,
        uses,
        construct,
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.generic_instance::specialize_start"]
    fn specialize_start(
        function: wire::SignatureNode,
        actual_count: u64,
        explicit_count: wire::Optional<u64>,
    ) -> SpecializeCursor;
    #[link_name = "bootstrap.generic_instance::specialize_step"]
    fn specialize_step(
        state: *mut SpecializeCursor,
        host: Host,
        context: *mut c_void,
    ) -> SpecializeNeed;
    #[link_name = "bootstrap.generic_instance::match_start"]
    fn match_start(
        mode: MatchMode,
        source: wire::SignatureNode,
        function_offset: u64,
        actual: TypeInfo,
        index: u64,
        name: NameBytes,
        inferred: wire::Optional<u32>,
        known: u32,
    ) -> MatchCursor;
    #[link_name = "bootstrap.generic_instance::match_step"]
    fn match_step(state: *mut MatchCursor, host: Host, context: *mut c_void) -> MatchNeed;
    #[link_name = "bootstrap.generic_instance::instance_start"]
    fn instance_start(
        function: wire::SignatureNode,
        tracking: u32,
        type_count: u64,
        value_count: u64,
    ) -> InstanceCursor;
    #[link_name = "bootstrap.generic_instance::instance_step"]
    fn instance_step(
        state: *mut InstanceCursor,
        host: InstanceHost,
        context: *mut c_void,
    ) -> InstanceNeed;
    #[link_name = "bootstrap.generic_instance::is_open"]
    fn is_open_b(
        type_storage: wire::StorageView,
        types: wire::Slice,
        value_storage: wire::StorageView,
        values: wire::Slice,
    ) -> u32;
}

#[repr(u32)]
enum Problem {
    GenericArguments,
    TypePosition,
    TypeWrapped,
    ExplicitFamily,
    Resource,
    MissingType,
    ConstShape,
    RepeatCount,
    MissingValue,
    ValueDomain,
    ViewExpected,
    TypeParameterArguments,
    InferredFamily,
    TypeMismatch,
    CfnType,
    CfnArity,
    CfnTypeArgument,
    CfnAccess,
    CfnPosition,
    ResultType,
    ResultTypeArgument,
    ArrayType,
    ArrayElement,
    ArrayArity,
    RecordType,
    RecordTypeArgument,
    DatasetCount,
    CfnValue,
    CfnValueArity,
    ResultValue,
    ArrayValue,
    ArrayCount,
    RecordValue,
    RecordConstConflict,
}

#[derive(Clone, Copy)]
struct Context<'a> {
    world: &'a World,
    key: &'a str,
    callee: &'a Arc<Definition>,
    source: &'a Source,
    module: &'a str,
    imports: &'a [String],
    span: Span,
}
struct Frame<'a, 'b> {
    context: Context<'a>,
    actuals: &'a [Actual<'b>],
    explicit: Option<&'a [GenericArgument]>,
    current: Option<Ty>,
    types: Vec<TypeArgument>,
    values: Vec<u32>,
    error: Option<Diagnostic>,
}
unsafe fn frame<'a, 'b>(raw: *mut c_void) -> &'a mut Frame<'a, 'b> {
    unsafe { &mut *raw.cast() }
}
unsafe fn ty<'a>(raw: *const c_void) -> &'a Ty {
    unsafe { &*raw.cast() }
}
unsafe fn semantic<'a>(value: SemanticInfo) -> &'a TypeName {
    unsafe { &*value.value.cast() }
}
unsafe fn text<'a>(value: NameBytes) -> &'a str {
    unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            value.bytes,
            value.length as usize,
        ))
    }
}
fn optional(value: Option<u32>) -> wire::Optional<u32> {
    wire::Optional {
        present: u32::from(value.is_some()),
        value: value.unwrap_or(0),
    }
}
unsafe extern "C" fn fail(
    raw: *mut c_void,
    problem: Problem,
    name: NameBytes,
    actual: *const c_void,
    first: u32,
    second: u32,
) {
    let f = unsafe { frame(raw) };
    let name = unsafe { text(name) };
    let (code, message): (&'static str, String) = match problem {
        Problem::GenericArguments => (
            "B235",
            "泛型实参必须完整给出，或只给全部 const 实参并推导类型".into(),
        ),
        Problem::TypePosition => ("B231", "此泛型位置需要普通值类型".into()),
        Problem::TypeWrapped => ("B231", "类型实参不能直接是资源包装".into()),
        Problem::ExplicitFamily => (
            "B231",
            format!(
                "类型 {} 不属于参数 {name} 的允许集合",
                f.current.as_ref().unwrap().name
            ),
        ),
        Problem::Resource => ("B231", "资源句柄不能用于普通值类型参数推导".into()),
        Problem::MissingType => (
            "B231",
            format!("无法推导类型参数 {name}，请显式指定全部类型实参"),
        ),
        Problem::ConstShape => ("B235", "const 实参需要 u32 编译期表达式".into()),
        Problem::RepeatCount => ("B235", "const 参数的重复静态推导不一致".into()),
        Problem::MissingValue => (
            "B235",
            format!("const 参数 {name} 无法从静态形状或名义类型推导，请显式指定"),
        ),
        Problem::ValueDomain => (
            "B235",
            format!("const 实参 {first} 不属于 {name} 的允许值域"),
        ),
        Problem::ViewExpected => ("B231", "view 类型参数需要不可变 view 实参或位置".into()),
        Problem::TypeParameterArguments => {
            ("B232", "类型参数已代表完整类型，不能再附加实参".into())
        }
        Problem::InferredFamily => (
            "B231",
            format!(
                "类型 {} 不满足参数 {name} 的普通值类型或有限集合约束",
                unsafe { ty(actual) }.name
            ),
        ),
        Problem::TypeMismatch => (
            "B124",
            format!(
                "类型不匹配：需要 {}，实际为 {}",
                f.current.as_ref().unwrap().name,
                unsafe { ty(actual) }.name
            ),
        ),
        Problem::CfnType => ("B263", "cfn类型推导需要相同函数签名种类".into()),
        Problem::CfnArity => ("B263", "cfn类型推导需要相同参数数量".into()),
        Problem::CfnTypeArgument => ("B263", "cfn签名只接受类型".into()),
        Problem::CfnAccess => ("B124", "cfn参数访问方式不匹配".into()),
        Problem::CfnPosition => ("B263", "cfn位置参数使用read<T>／write<T>／inout<T>".into()),
        Problem::ResultType => ("B269", "结果类型推导需要相同结果种类".into()),
        Problem::ResultTypeArgument => ("B269", "结果实参需要类型".into()),
        Problem::ArrayType => ("B238", "array 类型推导需要相同数组种类".into()),
        Problem::ArrayElement => ("B238", "array 首参数需要值类型".into()),
        Problem::ArrayArity => ("B238", "array 需要元素类型和固定长度".into()),
        Problem::RecordType => (
            "B232",
            format!(
                "不能从 {} 推导记录 {name} 的类型参数",
                unsafe { ty(actual) }.name
            ),
        ),
        Problem::RecordTypeArgument => ("B232", "记录类型位置需要类型实参".into()),
        Problem::DatasetCount => ("B235", "dataset 的重复长度推导不一致".into()),
        Problem::CfnValue => ("B263", "cfn const推导需要同类函数签名".into()),
        Problem::CfnValueArity => ("B263", "cfn const推导需要相同参数数量".into()),
        Problem::ResultValue => ("B269", "结果 const 推导需要相同结果种类".into()),
        Problem::ArrayValue => ("B238", "array 值推导需要相同数组种类".into()),
        Problem::ArrayCount => ("B235", "array 的重复长度推导不一致".into()),
        Problem::RecordValue => ("B232", "无法从不同名义记录推导 const 参数".into()),
        Problem::RecordConstConflict => (
            "B235",
            format!("const 参数 {name} 的名义实参不一致：{first} 与 {second}"),
        ),
    };
    f.error = Some(Diagnostic {
        source: f.context.source.clone(),
        span: f.context.span,
        code,
        message,
    });
}
unsafe extern "C" fn actual(raw: *mut c_void, index: u64) -> ActualArgument {
    let f = unsafe { &*raw.cast::<Frame<'_, '_>>() };
    let value = &f.actuals[index as usize];
    ActualArgument {
        ty: value
            .ty
            .as_deref()
            .map_or_else(TypeInfo::absent, TypeInfo::of),
        count: optional(value.count),
    }
}
unsafe extern "C" fn explicit_argument(raw: *mut c_void, index: u64) -> SemanticInfo {
    match &unsafe { &*raw.cast::<Frame<'_, '_>>() }.explicit.unwrap()[index as usize] {
        GenericArgument::Type(value) => SemanticInfo::of(value),
        GenericArgument::Value(_) => SemanticInfo::absent(),
    }
}
unsafe extern "C" fn has_type(raw: *mut c_void) -> u32 {
    u32::from(unsafe { &*raw.cast::<Frame<'_, '_>>() }.current.is_some())
}
unsafe extern "C" fn same_type(raw: *mut c_void, value: *const c_void) -> u32 {
    u32::from(
        unsafe { &*raw.cast::<Frame<'_, '_>>() }
            .current
            .as_ref()
            .unwrap()
            == unsafe { ty(value) },
    )
}
unsafe extern "C" fn keep_type(raw: *mut c_void, value: *const c_void) {
    // The candidate may belong to the synchronous World-query continuation.
    // The inferred result must own it after that actual temporary is released.
    unsafe { frame(raw) }.current = Some(unsafe { ty(value) }.clone());
}
unsafe extern "C" fn push_type(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    let value = f.current.take().unwrap();
    let syntax = value.syntax();
    f.types.push(TypeArgument {
        name: value.name,
        syntax,
    });
}
unsafe extern "C" fn push_value(raw: *mut c_void, value: u32) {
    unsafe { frame(raw) }.values.push(value);
}
unsafe extern "C" fn child(value: *const c_void, index: u64) -> TypeInfo {
    TypeInfo::of(&unsafe { ty(value) }.arguments[index as usize])
}
unsafe extern "C" fn element(value: *const c_void) -> TypeInfo {
    TypeInfo::of(unsafe { ty(value) }.array.as_ref().unwrap())
}
unsafe extern "C" fn view(value: *const c_void) -> TypeInfo {
    TypeInfo::of(unsafe { ty(value) }.view.as_ref().unwrap())
}
unsafe extern "C" fn dataset_syntax(value: *const c_void) -> SemanticInfo {
    SemanticInfo::of(unsafe { ty(value) }.dataset.as_ref().unwrap())
}
unsafe extern "C" fn cfn_slot(value: *const c_void, index: u64) -> CfnSlot {
    let function = unsafe { ty(value) }.function.as_ref().unwrap();
    if index == 0 {
        CfnSlot {
            ty: SemanticInfo::of(&function.result),
            access: 0,
        }
    } else {
        let (access, syntax) = &function.parameters[index as usize - 1];
        CfnSlot {
            ty: SemanticInfo::of(syntax),
            access: *access as u32,
        }
    }
}
async fn family(c: Context<'_>, index: u64, value: &Ty, span: Span) -> Result<bool> {
    unsafe { c.world.call(c.world.family_accepts(
        &c.callee.function.types().at(index as usize).family(),
        value,
        &c.callee.source,
        span,
        &c.callee.module,
        &c.callee.imports,
        &mut BTreeSet::new(),
    )).await }
}
fn record_base(c: Context<'_>, formal: wire::SignatureNode, value: bool) -> Result<String> {
    let source = c.callee.function.child::<wire::TypeNode>(formal.offset);
    c.world
        .resolve(source.name(), &c.callee.module, &c.callee.imports, false)
        .unique()
        .ok_or_else(|| Diagnostic {
            source: c.callee.source.clone(),
            span: c.span,
            code: "B031",
            message: if value {
                format!("未知记录 {}", source.name())
            } else {
                format!("未知或不明确的记录类型 {}", source.name())
            },
        })
}
unsafe extern "C" fn record_matches(value: *const c_void, base: NameBytes) -> u32 {
    u32::from(record_key_matches(unsafe { text(base) }, unsafe {
        ty(value)
    }))
}

struct InstanceFrame<'a> {
    world: &'a World,
    base: &'a str,
    definition: &'a Arc<Definition>,
    arguments: std::vec::IntoIter<TypeArgument>,
    values: Values<'a>,
    name: Option<String>,
    bindings: Bindings,
    result: Option<Instance>,
    error: Option<Diagnostic>,
}
unsafe fn instance_frame<'a>(raw: *mut c_void) -> &'a mut InstanceFrame<'a> {
    unsafe { &mut *raw.cast() }
}
unsafe extern "C" fn implementation(raw: *mut c_void) {
    let f = unsafe { instance_frame(raw) };
    f.world
        .build_declarations
        .as_ref()
        .unwrap()
        .borrow_mut()
        .insert(crate::build::Declaration::Implementation(f.base.into()));
}
unsafe extern "C" fn external(raw: *mut c_void) {
    let f = unsafe { instance_frame(raw) };
    f.world
        .build_declarations
        .as_ref()
        .unwrap()
        .borrow_mut()
        .insert(crate::build::Declaration::External(f.base.into()));
}
unsafe extern "C" fn plain(raw: *mut c_void) {
    let f = unsafe { instance_frame(raw) };
    f.result = Some(Instance {
        name: f.base.into(),
        definition: f.definition.clone(),
        bindings: Arc::new(Bindings::default()),
    });
}
unsafe extern "C" fn name(raw: *mut c_void) {
    let f = unsafe { instance_frame(raw) };
    f.name = Some(key(
        f.base,
        f.arguments.as_slice().iter().map(|a| a.name.as_str()),
        f.values.as_slice(),
    ));
}
unsafe extern "C" fn type_argument(raw: *mut c_void, name: NameBytes) {
    let f = unsafe { instance_frame(raw) };
    f.bindings.types.insert(
        unsafe { text(name) }.into(),
        f.arguments.next().unwrap().syntax,
    );
}
unsafe extern "C" fn value_argument(raw: *mut c_void, name: NameBytes, index: u64) {
    let f = unsafe { instance_frame(raw) };
    f.bindings.values.insert(
        unsafe { text(name) }.into(),
        f.values.as_slice()[index as usize],
    );
}
unsafe extern "C" fn constant(raw: *mut c_void, name: NameBytes) -> wire::Optional<u32> {
    let f = unsafe { &*raw.cast::<InstanceFrame<'_>>() };
    optional(f.bindings.values.get(unsafe { text(name) }).copied())
}
unsafe extern "C" fn nonzero_error(raw: *mut c_void, index: u64, name: NameBytes) {
    let f = unsafe { instance_frame(raw) };
    let name = unsafe { text(name) };
    f.error = Some(Diagnostic {
        source: f.definition.source.clone(),
        span: f.definition.function.contracts().at(index as usize).span(),
        code: "B226",
        message: format!("const 参数 {name}=0 不满足 requires nonzero({name})"),
    });
}
unsafe extern "C" fn uses(raw: *mut c_void) -> u32 {
    let f = unsafe { instance_frame(raw) };
    match f
        .bindings
        .check_function(&f.definition.function, &f.definition.source)
    {
        Ok(()) => 1,
        Err(error) => {
            f.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn construct(raw: *mut c_void) {
    let f = unsafe { instance_frame(raw) };
    f.result = Some(Instance {
        name: f.name.take().unwrap(),
        definition: f.definition.clone(),
        bindings: Arc::new(mem::take(&mut f.bindings)),
    });
}

struct Matching {
    cursor: MatchCursor,
    // Dataset and cfn-slot queries are distinct actual values. A child cannot
    // replace an actual Ty still borrowed by the parent's remaining slots.
    actual: Option<Box<Ty>>,
    child: Option<Box<Ty>>,
    base: Option<String>,
}
pub(super) struct Specialization<'a, 'b> {
    data: Box<Frame<'a, 'b>>,
    cursor: SpecializeCursor,
    matches: Vec<Matching>,
}
pub(super) enum SpecializationNeed {
    ExplicitType,
    ExplicitDataset,
    FamilyCurrent,
    FamilyActual,
    ExplicitValue,
    ActualType,
    ChildType,
    RecordName,
}
pub(super) enum SpecializationProgress<'a, 'b> {
    Need(Specialization<'a, 'b>, SpecializationNeed),
    Instance(Instantiation<'a>),
}
impl<'a, 'b> Specialization<'a, 'b> {
    fn new(
        world: &'a World,
        key: &'a str,
        actuals: &'a [Actual<'b>],
        explicit: Option<&'a [GenericArgument]>,
        (source, module, imports): (&'a Source, &'a str, &'a [String]),
        span: Span,
    ) -> Self {
        let callee = &world.functions[key];
        let cursor = unsafe {
            specialize_start(
                callee.function.signature_node(),
                actuals.len() as u64,
                wire::Optional {
                    present: u32::from(explicit.is_some()),
                    value: explicit.map_or(0, |v| v.len() as u64),
                },
            )
        };
        Self {
            data: Box::new(Frame {
                context: Context {
                    world,
                    key,
                    callee,
                    source,
                    module,
                    imports,
                    span,
                },
                actuals,
                explicit,
                current: None,
                types: Vec::new(),
                values: Vec::new(),
                error: None,
            }),
            cursor,
            matches: Vec::new(),
        }
    }
    fn push_match(
        &mut self,
        mode: MatchMode,
        source: wire::SignatureNode,
        function_offset: u64,
        actual: TypeInfo,
        index: u64,
        name: NameBytes,
        inferred: wire::Optional<u32>,
        known: u32,
    ) {
        let cursor = unsafe {
            match_start(
                mode,
                source,
                function_offset,
                actual,
                index,
                name,
                inferred,
                known,
            )
        };
        self.matches.push(Matching {
            cursor,
            actual: None,
            child: None,
            base: None,
        });
    }
    pub(super) fn advance(mut self) -> Result<SpecializationProgress<'a, 'b>> {
        loop {
            if let Some(active) = self.matches.last_mut() {
                let need = unsafe {
                    match_step(
                        &mut active.cursor,
                        host(),
                        (&mut *self.data as *mut Frame<'_, '_>).cast(),
                    )
                };
                if let Some(error) = self.data.error.take() {
                    return Err(error);
                }
                match need {
                    MatchNeed::Done => {
                        let finished = self.matches.pop().unwrap();
                        if finished.cursor.mode == MatchMode::Value {
                            if let Some(parent) = self.matches.last_mut() {
                                parent.cursor.inferred = finished.cursor.inferred;
                            } else {
                                self.cursor.inferred = finished.cursor.inferred;
                            }
                        }
                    }
                    MatchNeed::TypeChild | MatchNeed::ValueChild => {
                        let c = self.matches.last().unwrap().cursor;
                        let mode = match need {
                            MatchNeed::TypeChild => MatchMode::Type,
                            _ => MatchMode::Value,
                        };
                        self.push_match(
                            mode,
                            c.next,
                            c.function_offset,
                            c.next_actual,
                            c.index,
                            c.name,
                            c.inferred,
                            c.next_known,
                        );
                    }
                    MatchNeed::FamilyActual => {
                        return Ok(SpecializationProgress::Need(
                            self,
                            SpecializationNeed::FamilyActual,
                        ));
                    }
                    MatchNeed::ActualType => {
                        return Ok(SpecializationProgress::Need(
                            self,
                            SpecializationNeed::ActualType,
                        ));
                    }
                    MatchNeed::ChildType => {
                        return Ok(SpecializationProgress::Need(
                            self,
                            SpecializationNeed::ChildType,
                        ));
                    }
                    MatchNeed::RecordName => {
                        return Ok(SpecializationProgress::Need(
                            self,
                            SpecializationNeed::RecordName,
                        ));
                    }
                }
                continue;
            }
            let need = unsafe {
                specialize_step(
                    &mut self.cursor,
                    host(),
                    (&mut *self.data as *mut Frame<'_, '_>).cast(),
                )
            };
            match need {
                SpecializeNeed::Failed => return Err(self.data.error.take().unwrap()),
                SpecializeNeed::TypeChild | SpecializeNeed::ValueChild => {
                    let c = self.cursor;
                    let mode = match need {
                        SpecializeNeed::TypeChild => MatchMode::Type,
                        _ => MatchMode::Value,
                    };
                    self.push_match(
                        mode,
                        c.next,
                        c.function.offset,
                        c.next_actual,
                        c.index,
                        c.name,
                        c.inferred,
                        u32::from(mode == MatchMode::Type),
                    );
                }
                SpecializeNeed::ExplicitType => {
                    return Ok(SpecializationProgress::Need(
                        self,
                        SpecializationNeed::ExplicitType,
                    ));
                }
                SpecializeNeed::ExplicitDataset => {
                    return Ok(SpecializationProgress::Need(
                        self,
                        SpecializationNeed::ExplicitDataset,
                    ));
                }
                SpecializeNeed::FamilyCurrent => {
                    return Ok(SpecializationProgress::Need(
                        self,
                        SpecializationNeed::FamilyCurrent,
                    ));
                }
                SpecializeNeed::ExplicitValue => {
                    return Ok(SpecializationProgress::Need(
                        self,
                        SpecializationNeed::ExplicitValue,
                    ));
                }
                SpecializeNeed::Instantiate => {
                    let c = self.data.context;
                    let next = Instantiation::new(
                        c.world,
                        c.key,
                        c.callee,
                        mem::take(&mut self.data.types),
                        Values::Owned(mem::take(&mut self.data.values)),
                    );
                    return Ok(SpecializationProgress::Instance(next));
                }
            }
        }
    }
    pub(super) async fn fulfill(mut self, need: SpecializationNeed) -> Result<Self> {
        let c = self.data.context;
        match need {
            SpecializationNeed::ExplicitType => {
                let input = unsafe { semantic(self.cursor.semantic) };
                self.data.current = Some(if self.cursor.unit != 0 {
                    Ty::unit()
                } else {
                    unsafe { c.world.call(c.world.ty(input, c.source, c.span, c.module, c.imports)).await }?
                });
            }
            SpecializationNeed::ExplicitDataset => {
                let input = unsafe { semantic(self.cursor.semantic) };
                let count = unsafe { c.world.call(c.world.shape(&input.count, c.source, c.span, c.module, c.imports)).await }?;
                self.data.current = Some(Ty::owned_dataset(
                    self.data.current.take().unwrap().syntax(),
                    count,
                ));
            }
            SpecializationNeed::FamilyCurrent => {
                self.cursor.scalar = u32::from(unsafe { c.world.call(family(
                    c,
                    self.cursor.index,
                    self.data.current.as_ref().unwrap(),
                    c.callee.function.span(),
                )).await }?);
            }
            SpecializationNeed::ExplicitValue => {
                let input = self.data.explicit.unwrap()
                    [(self.cursor.first_value + self.cursor.index) as usize]
                    .shape()
                    .unwrap();
                self.cursor.scalar = unsafe { c.world.call(c.world.shape(&Some(input), c.source, c.span, c.module, c.imports)).await }?.unwrap();
            }
            SpecializationNeed::FamilyActual => {
                let m = self.matches.last_mut().unwrap();
                m.cursor.scalar = u32::from(unsafe { c.world.call(family(
                    c,
                    m.cursor.index,
                    ty(m.cursor.actual.value),
                    c.span,
                )).await }?);
            }
            SpecializationNeed::ActualType | SpecializationNeed::ChildType => {
                let m = self.matches.last_mut().unwrap();
                let input = unsafe { semantic(m.cursor.semantic) };
                let actual = Box::new(if m.cursor.unit != 0 {
                    Ty::unit()
                } else {
                    unsafe { c.world.call(c.world.ty(input, c.source, c.span, &c.callee.module, &c.callee.imports)).await }?
                });
                let description = TypeInfo::of(&actual);
                match need {
                    SpecializationNeed::ActualType => {
                        m.actual = Some(actual);
                        m.cursor.actual = description;
                    }
                    _ => {
                        m.child = Some(actual);
                        m.cursor.next_actual = description;
                    }
                }
            }
            SpecializationNeed::RecordName => {
                let m = self.matches.last_mut().unwrap();
                m.base = Some(record_base(
                    c,
                    m.cursor.source,
                    m.cursor.mode == MatchMode::Value,
                )?);
                m.cursor.base = NameBytes::new(m.base.as_ref().unwrap());
            }
        }
        Ok(self)
    }
}
pub(crate) async fn specialize(
    world: &World,
    key: &str,
    actuals: &[Actual<'_>],
    explicit: Option<&[GenericArgument]>,
    context: (&Source, &str, &[String]),
    span: Span,
) -> Result<Instance> {
    let mut query = Specialization::new(world, key, actuals, explicit, context, span);
    loop {
        match query.advance()? {
            SpecializationProgress::Need(pending, need) => query = unsafe { world.call(pending.fulfill(need)).await }?,
            SpecializationProgress::Instance(instance) => return unsafe { world.call(instance.drive()).await },
        }
    }
}
enum Values<'a> {
    Borrowed(&'a [u32]),
    Owned(Vec<u32>),
}
impl Values<'_> {
    fn as_slice(&self) -> &[u32] {
        match self {
            Self::Borrowed(v) => v,
            Self::Owned(v) => v,
        }
    }
}
pub(super) struct Instantiation<'a> {
    data: Box<InstanceFrame<'a>>,
    cursor: InstanceCursor,
}
pub(super) enum InstantiationNeed {
    CAbi,
    Interface,
}
pub(super) enum InstantiationProgress<'a> {
    Complete(Instance),
    Need(Instantiation<'a>, InstantiationNeed),
}
impl<'a> Instantiation<'a> {
    fn new(
        world: &'a World,
        base: &'a str,
        definition: &'a Arc<Definition>,
        arguments: Vec<TypeArgument>,
        values: Values<'a>,
    ) -> Self {
        let cursor = unsafe {
            instance_start(
                definition.function.signature_node(),
                u32::from(world.build_declarations.is_some()),
                arguments.len() as u64,
                values.as_slice().len() as u64,
            )
        };
        Self {
            cursor,
            data: Box::new(InstanceFrame {
                world,
                base,
                definition,
                arguments: arguments.into_iter(),
                values,
                name: None,
                bindings: Bindings::default(),
                result: None,
                error: None,
            }),
        }
    }
    pub(super) fn advance(mut self) -> Result<InstantiationProgress<'a>> {
        let need = unsafe {
            instance_step(
                &mut self.cursor,
                instance_host(),
                (&mut *self.data as *mut InstanceFrame<'_>).cast(),
            )
        };
        if let Some(error) = self.data.error.take() {
            return Err(error);
        }
        Ok(match need {
            InstanceNeed::Done => InstantiationProgress::Complete(self.data.result.take().unwrap()),
            InstanceNeed::CAbi => InstantiationProgress::Need(self, InstantiationNeed::CAbi),
            InstanceNeed::Interface => {
                InstantiationProgress::Need(self, InstantiationNeed::Interface)
            }
        })
    }
    pub(super) async fn fulfill(self, need: InstantiationNeed) -> Result<Self> {
        let f = &self.data;
        match need {
            InstantiationNeed::CAbi => {
                unsafe { f.world.call(crate::check::external::check_declaration(f.world, f.result.as_ref().unwrap())).await }?
            }
            InstantiationNeed::Interface => unsafe { f.world.call(f.world.check_interface_types(f.definition, &f.result.as_ref().unwrap().bindings)).await }?,
        }
        Ok(self)
    }
    async fn drive(mut self) -> Result<Instance> {
        let world=self.data.world;
        loop {
            match self.advance()? {
                InstantiationProgress::Complete(instance) => return Ok(instance),
                InstantiationProgress::Need(pending, need) => self = unsafe { world.call(pending.fulfill(need)).await }?,
            }
        }
    }
}
pub(crate) async fn instantiate(
    world: &World,
    base: &str,
    definition: &Arc<Definition>,
    arguments: Vec<TypeArgument>,
    values: &[u32],
) -> Result<Instance> {
    unsafe { world.call(Instantiation::new(world, base, definition, arguments, Values::Borrowed(values)).drive()).await }
}

pub(crate) fn is_open(
    types: &syntax::List<wire::TypeParameterNode>,
    values: &syntax::List<wire::ValueParameterNode>,
) -> bool {
    unsafe {
        is_open_b(
            types.source_view().storage(),
            types.source_slice(),
            values.source_view().storage(),
            values.source_slice(),
        ) != 0
    }
}
