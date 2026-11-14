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
#[repr(C)]
struct QueryBool {
    ready: u32,
    value: u32,
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
    result: Option<Instance>,
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
unsafe extern "C" fn explicit_type(raw: *mut c_void, index: u64, unit: u32) -> u32 {
    let syntax = unsafe { semantic(explicit_argument(raw, index)) };
    let f = unsafe { frame(raw) };
    let c = f.context;
    match if unit != 0 {
        Ok(Ty::unit())
    } else {
        c.world.ty(syntax, c.source, c.span, c.module, c.imports)
    } {
        Ok(value) => {
            f.current = Some(value);
            1
        }
        Err(error) => {
            f.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn explicit_dataset(raw: *mut c_void, index: u64) -> u32 {
    let syntax = unsafe { semantic(explicit_argument(raw, index)) };
    let f = unsafe { frame(raw) };
    let c = f.context;
    match c
        .world
        .shape(&syntax.count, c.source, c.span, c.module, c.imports)
    {
        Ok(count) => {
            f.current = Some(Ty::owned_dataset(f.current.take().unwrap().syntax(), count));
            1
        }
        Err(error) => {
            f.error = Some(error);
            0
        }
    }
}
fn family(c: Context<'_>, index: u64, value: &Ty, span: Span) -> Result<bool> {
    c.world.family_accepts(
        &c.callee.function.types().at(index as usize).family(),
        value,
        &c.callee.source,
        span,
        &c.callee.module,
        &c.callee.imports,
        &mut BTreeSet::new(),
    )
}
unsafe extern "C" fn family_current(raw: *mut c_void, index: u64) -> QueryBool {
    let f = unsafe { frame(raw) };
    match family(
        f.context,
        index,
        f.current.as_ref().unwrap(),
        f.context.callee.function.span(),
    ) {
        Ok(value) => QueryBool {
            ready: 1,
            value: u32::from(value),
        },
        Err(error) => {
            f.error = Some(error);
            QueryBool { ready: 0, value: 0 }
        }
    }
}
unsafe extern "C" fn family_actual(
    raw: *mut c_void,
    index: u64,
    value: *const c_void,
) -> QueryBool {
    let f = unsafe { frame(raw) };
    match family(f.context, index, unsafe { ty(value) }, f.context.span) {
        Ok(value) => QueryBool {
            ready: 1,
            value: u32::from(value),
        },
        Err(error) => {
            f.error = Some(error);
            QueryBool { ready: 0, value: 0 }
        }
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
unsafe extern "C" fn explicit_value(raw: *mut c_void, index: u64) -> wire::Optional<u32> {
    let f = unsafe { frame(raw) };
    let c = f.context;
    let shape = f.explicit.unwrap()[index as usize].shape().unwrap();
    match c
        .world
        .shape(&Some(shape), c.source, c.span, c.module, c.imports)
    {
        Ok(value) => optional(value),
        Err(error) => {
            f.error = Some(error);
            optional(None)
        }
    }
}
unsafe extern "C" fn push_value(raw: *mut c_void, value: u32) {
    unsafe { frame(raw) }.values.push(value);
}
unsafe extern "C" fn finish(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    let c = f.context;
    match instantiate(c.world, c.key, c.callee, mem::take(&mut f.types), &f.values) {
        Ok(value) => {
            f.result = Some(value);
            1
        }
        Err(error) => {
            f.error = Some(error);
            0
        }
    }
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
unsafe extern "C" fn type_dataset(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    syntax: SemanticInfo,
    index: u64,
    name: NameBytes,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match c.world.ty(
        unsafe { semantic(syntax) },
        c.source,
        c.span,
        &c.callee.module,
        &c.callee.imports,
    ) {
        Ok(value) => unsafe {
            type_after_dataset_b(host(), raw, formal, TypeInfo::of(&value), index, name)
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn type_cfn(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    syntax: SemanticInfo,
    index: u64,
    name: NameBytes,
    unit: u32,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match if unit != 0 {
        Ok(Ty::unit())
    } else {
        c.world.ty(
            unsafe { semantic(syntax) },
            c.source,
            c.span,
            &c.callee.module,
            &c.callee.imports,
        )
    } {
        Ok(value) => unsafe {
            type_mentioned_b(host(), raw, formal, TypeInfo::of(&value), index, name)
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn value_dataset(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    syntax: SemanticInfo,
    name: NameBytes,
    inferred: *mut wire::Optional<u32>,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match c.world.ty(
        unsafe { semantic(syntax) },
        c.source,
        c.span,
        &c.callee.module,
        &c.callee.imports,
    ) {
        Ok(value) => unsafe {
            value_after_dataset_b(host(), raw, formal, TypeInfo::of(&value), name, inferred)
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn value_cfn(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    syntax: SemanticInfo,
    name: NameBytes,
    inferred: *mut wire::Optional<u32>,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match c.world.ty(
        unsafe { semantic(syntax) },
        c.source,
        c.span,
        &c.callee.module,
        &c.callee.imports,
    ) {
        Ok(value) => unsafe {
            infer_value_b(host(), raw, formal, TypeInfo::of(&value), name, inferred)
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
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
unsafe extern "C" fn record_type(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    actual: TypeInfo,
    index: u64,
    name: NameBytes,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match record_base(c, formal, false) {
        Ok(base) => unsafe {
            type_record_b(
                host(),
                raw,
                formal,
                actual,
                index,
                name,
                NameBytes::new(&base),
            )
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn record_value(
    raw: *mut c_void,
    formal: wire::SignatureNode,
    actual: TypeInfo,
    name: NameBytes,
    inferred: *mut wire::Optional<u32>,
) -> u32 {
    let c = unsafe { frame(raw) }.context;
    match record_base(c, formal, true) {
        Ok(base) => unsafe {
            value_record_b(
                host(),
                raw,
                formal,
                actual,
                name,
                NameBytes::new(&base),
                inferred,
            )
        },
        Err(error) => {
            unsafe { frame(raw) }.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn record_matches(value: *const c_void, base: NameBytes) -> u32 {
    u32::from(record_key_matches(unsafe { text(base) }, unsafe {
        ty(value)
    }))
}

#[repr(C)]
struct Host {
    fail: unsafe extern "C" fn(*mut c_void, Problem, NameBytes, *const c_void, u32, u32),
    actual: unsafe extern "C" fn(*mut c_void, u64) -> ActualArgument,
    explicit_argument: unsafe extern "C" fn(*mut c_void, u64) -> SemanticInfo,
    explicit_type: unsafe extern "C" fn(*mut c_void, u64, u32) -> u32,
    explicit_dataset: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    family_current: unsafe extern "C" fn(*mut c_void, u64) -> QueryBool,
    family_actual: unsafe extern "C" fn(*mut c_void, u64, *const c_void) -> QueryBool,
    has_type: unsafe extern "C" fn(*mut c_void) -> u32,
    same_type: unsafe extern "C" fn(*mut c_void, *const c_void) -> u32,
    keep_type: unsafe extern "C" fn(*mut c_void, *const c_void),
    push_type: unsafe extern "C" fn(*mut c_void),
    explicit_value: unsafe extern "C" fn(*mut c_void, u64) -> wire::Optional<u32>,
    push_value: unsafe extern "C" fn(*mut c_void, u32),
    finish: unsafe extern "C" fn(*mut c_void) -> u32,
    child: unsafe extern "C" fn(*const c_void, u64) -> TypeInfo,
    element: unsafe extern "C" fn(*const c_void) -> TypeInfo,
    view: unsafe extern "C" fn(*const c_void) -> TypeInfo,
    dataset_syntax: unsafe extern "C" fn(*const c_void) -> SemanticInfo,
    cfn_slot: unsafe extern "C" fn(*const c_void, u64) -> CfnSlot,
    type_dataset:
        unsafe extern "C" fn(*mut c_void, wire::SignatureNode, SemanticInfo, u64, NameBytes) -> u32,
    type_cfn: unsafe extern "C" fn(
        *mut c_void,
        wire::SignatureNode,
        SemanticInfo,
        u64,
        NameBytes,
        u32,
    ) -> u32,
    value_dataset: unsafe extern "C" fn(
        *mut c_void,
        wire::SignatureNode,
        SemanticInfo,
        NameBytes,
        *mut wire::Optional<u32>,
    ) -> u32,
    value_cfn: unsafe extern "C" fn(
        *mut c_void,
        wire::SignatureNode,
        SemanticInfo,
        NameBytes,
        *mut wire::Optional<u32>,
    ) -> u32,
    record_type:
        unsafe extern "C" fn(*mut c_void, wire::SignatureNode, TypeInfo, u64, NameBytes) -> u32,
    record_value: unsafe extern "C" fn(
        *mut c_void,
        wire::SignatureNode,
        TypeInfo,
        NameBytes,
        *mut wire::Optional<u32>,
    ) -> u32,
    record_matches: unsafe extern "C" fn(*const c_void, NameBytes) -> u32,
}
fn host() -> Host {
    Host {
        fail,
        actual,
        explicit_argument,
        explicit_type,
        explicit_dataset,
        family_current,
        family_actual,
        has_type,
        same_type,
        keep_type,
        push_type,
        explicit_value,
        push_value,
        finish,
        child,
        element,
        view,
        dataset_syntax,
        cfn_slot,
        type_dataset,
        type_cfn,
        value_dataset,
        value_cfn,
        record_type,
        record_value,
        record_matches,
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.generic_instance::specialize"]
    fn specialize_b(
        function: wire::SignatureNode,
        actual_count: u64,
        explicit_count: wire::Optional<u64>,
        host: Host,
        context: *mut c_void,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::type_mentioned"]
    fn type_mentioned_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        index: u64,
        name: NameBytes,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::type_after_dataset"]
    fn type_after_dataset_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        index: u64,
        name: NameBytes,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::type_record"]
    fn type_record_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        index: u64,
        name: NameBytes,
        base: NameBytes,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::infer_value"]
    fn infer_value_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        name: NameBytes,
        inferred: *mut wire::Optional<u32>,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::value_after_dataset"]
    fn value_after_dataset_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        name: NameBytes,
        inferred: *mut wire::Optional<u32>,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::value_record"]
    fn value_record_b(
        host: Host,
        context: *mut c_void,
        formal: wire::SignatureNode,
        actual: TypeInfo,
        name: NameBytes,
        base: NameBytes,
        inferred: *mut wire::Optional<u32>,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::instantiate"]
    fn instantiate_b(
        function: wire::SignatureNode,
        tracking: u32,
        type_count: u64,
        value_count: u64,
        host: InstanceHost,
        context: *mut c_void,
    ) -> u32;
    #[link_name = "bootstrap.generic_instance::is_open"]
    fn is_open_b(
        type_storage: wire::StorageView,
        types: wire::Slice,
        value_storage: wire::StorageView,
        values: wire::Slice,
    ) -> u32;
}

pub(crate) fn specialize(
    world: &World,
    key: &str,
    actuals: &[Actual<'_>],
    explicit: Option<&[GenericArgument]>,
    (source, module, imports): (&Source, &str, &[String]),
    span: Span,
) -> Result<Instance> {
    let callee = &world.functions[key];
    let mut f = Frame {
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
        result: None,
        error: None,
    };
    let count = wire::Optional {
        present: u32::from(explicit.is_some()),
        value: explicit.map_or(0, |v| v.len() as u64),
    };
    let success = unsafe {
        specialize_b(
            callee.function.signature_node(),
            actuals.len() as u64,
            count,
            host(),
            (&mut f as *mut Frame<'_, '_>).cast(),
        )
    };
    if success != 0 {
        Ok(f.result.unwrap())
    } else {
        Err(f.error.unwrap())
    }
}

struct InstanceFrame<'a> {
    world: &'a World,
    base: &'a str,
    definition: &'a Arc<Definition>,
    arguments: std::vec::IntoIter<TypeArgument>,
    values: &'a [u32],
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
        f.values,
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
    f.bindings
        .values
        .insert(unsafe { text(name) }.into(), f.values[index as usize]);
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
unsafe extern "C" fn c_abi(raw: *mut c_void) -> u32 {
    let f = unsafe { instance_frame(raw) };
    match crate::check::external::check_declaration(f.world, f.result.as_ref().unwrap()) {
        Ok(()) => 1,
        Err(error) => {
            f.error = Some(error);
            0
        }
    }
}
unsafe extern "C" fn interface(raw: *mut c_void) -> u32 {
    let f = unsafe { instance_frame(raw) };
    match f
        .world
        .check_interface_types(f.definition, &f.result.as_ref().unwrap().bindings)
    {
        Ok(()) => 1,
        Err(error) => {
            f.error = Some(error);
            0
        }
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
    c_abi: unsafe extern "C" fn(*mut c_void) -> u32,
    interface: unsafe extern "C" fn(*mut c_void) -> u32,
}
pub(crate) fn instantiate(
    world: &World,
    base: &str,
    definition: &Arc<Definition>,
    arguments: Vec<TypeArgument>,
    values: &[u32],
) -> Result<Instance> {
    let type_count = arguments.len() as u64;
    let mut f = InstanceFrame {
        world,
        base,
        definition,
        arguments: arguments.into_iter(),
        values,
        name: None,
        bindings: Bindings::default(),
        result: None,
        error: None,
    };
    let host = InstanceHost {
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
        c_abi,
        interface,
    };
    let success = unsafe {
        instantiate_b(
            definition.function.signature_node(),
            u32::from(world.build_declarations.is_some()),
            type_count,
            values.len() as u64,
            host,
            (&mut f as *mut InstanceFrame<'_>).cast(),
        )
    };
    if success != 0 {
        Ok(f.result.unwrap())
    } else {
        Err(f.error.unwrap())
    }
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
