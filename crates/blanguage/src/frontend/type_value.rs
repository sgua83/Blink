//! B controls ordinary type expansion. This file lends the original World,
//! declared source and single semantic containers to that query. B returns real
//! semantic child requests; those World queries run after native stepping returns.
use crate::{
    Diagnostic, Result, Source, World,
    ast::*,
    builtin_types::{Builtin, Kind},
    generics,
    ir::{self, Ty},
    syntax,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    mem, ptr,
    sync::Arc,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Text {
    bytes: *const u8,
    length: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct TypeInfo {
    value: *mut c_void,
    name: Text,
    arguments: u64,
    flags: u32,
    canonical: u32,
}
impl TypeInfo {
    fn of(value: &TypeName) -> Self {
        Self {
            value: (value as *const TypeName).cast_mut().cast(),
            name: Text {
                bytes: value.name.as_ptr(),
                length: value.name.len() as u64,
            },
            arguments: value.arguments.len() as u64,
            flags: u32::from(value.dataset)
                | (u32::from(value.view) << 1)
                | (u32::from(value.tile) << 2)
                | (u32::from(value.count.is_some()) << 3)
                | (u32::from(value.contracts.is_some()) << 4),
            canonical: u32::from(value.canonical),
        }
    }
    fn absent() -> Self {
        Self {
            value: ptr::null_mut(),
            name: Text {
                bytes: ptr::null(),
                length: 0,
            },
            arguments: 0,
            flags: 0,
            canonical: 0,
        }
    }
}
#[repr(C)]
struct Facts {
    numeric: u32,
    integer: u32,
    components: u64,
    name: Text,
}
impl Facts {
    fn of(value: &Ty) -> Self {
        Self {
            numeric: u32::from(value.is_numeric()),
            integer: u32::from(
                value
                    .components
                    .first()
                    .is_some_and(|(s, _)| s.is_integer()),
            ),
            components: value.components.len() as u64,
            name: Text {
                bytes: value.name.as_ptr(),
                length: value.name.len() as u64,
            },
        }
    }
}
#[repr(C)]
struct Count {
    present: u32,
    value: u32,
}
#[repr(C)]
struct RecordInfo {
    ready: u32,
    types: u64,
    values: u64,
    fields: u64,
    variants: u64,
    enumeration: u32,
    stride: u32,
}
#[repr(C)]
struct CleanupInfo {
    ready: u32,
    ordinary: u32,
    cpu: u32,
    parameters: u64,
    value_parameter: u32,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum Site {
    Root,
    Record,
    Field,
    Variant,
    CleanupParameter,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum Binding {
    Representation,
    Field,
    Variant,
    CleanupResult,
    CleanupParameter,
}
#[repr(u32)]
enum Problem {
    Contracts,
    Texture,
    Void,
    Never,
    Tile,
    TaskArguments,
    TaskOverflow,
    OwnedArguments,
    ReleaseForm,
    ReleaseShape,
    DatasetView,
    OwnedCycle,
    ReleaseResult,
    ReleasePayload,
    ResultArguments,
    ResultType,
    ResultTile,
    ResultOverflow,
    ArrayArguments,
    ArrayType,
    ArrayTile,
    ArrayLength,
    ArrayOverflow,
    BuiltinArguments,
    RecordArguments,
    RecordType,
    RecordTile,
    Family,
    RecordValue,
    RecordDomain,
    RecordCycle,
    EnumScalar,
    VariantScalar,
    DuplicateVariantName,
    DuplicateTag,
    VariantTile,
    VariantOverflow,
    DuplicateField,
    FieldOverflow,
    AlignedOverflow,
    Stride,
    DatasetViewSpaced,
}

#[derive(Clone, Copy)]
struct Context<'a> {
    world: &'a World,
    source: &'a Source,
    span: Span,
    module: &'a str,
    imports: &'a [String],
}
struct RecordState<'a> {
    source: &'a syntax::Record,
    context: Context<'a>,
    key: String,
    arguments: Vec<Ty>,
    values: Vec<u32>,
    replacements: BTreeMap<String, TypeName>,
    constants: BTreeMap<String, u32>,
    bindings: Option<Arc<generics::Bindings>>,
    ty: Option<Ty>,
    representation: Option<Ty>,
    variants: Vec<ir::Variant>,
    names: BTreeSet<String>,
    tags: BTreeSet<u32>,
}
struct ReleaseState {
    name: String,
    owner: Option<Ty>,
    identity: Option<String>,
    instance: Option<generics::Instance>,
}
enum InputOwner<'a> {Borrowed(&'a TypeName),Shared(Arc<TypeName>)}
// A direct view into the one actual input tree. Root calls borrow their input;
// bound inputs share their existing allocation with real child consumers.
struct TypeInput<'a> {owner:InputOwner<'a>,value:*const TypeName}
impl<'a> TypeInput<'a> {
    fn borrowed(value:&'a TypeName)->Self {Self{owner:InputOwner::Borrowed(value),value}}
    fn shared(value:&Arc<TypeName>)->Self {Self{owner:InputOwner::Shared(value.clone()),value:Arc::as_ptr(value)}}
    fn child(&self,value:TypeInfo)->Self {
        let owner=match &self.owner {
            InputOwner::Borrowed(root)=>InputOwner::Borrowed(root),
            InputOwner::Shared(root)=>InputOwner::Shared(root.clone()),
        };
        Self{owner,value:value.value.cast()}
    }
}
impl std::ops::Deref for TypeInput<'_> {
    type Target=TypeName;
    fn deref(&self)->&TypeName {unsafe{&*self.value}}
}

struct Frame<'a> {
    context: Context<'a>,
    input: TypeInput<'a>,
    builtin: Builtin,
    referenced: bool,
    stack: *mut BTreeSet<String>,
    current: Option<Ty>,
    record: Option<RecordState<'a>>,
    release: Option<ReleaseState>,
    payloads: Vec<Ty>,
    // One actual bound temporary, kept at a stable address for B's borrow.
    // It is moved here, not cloned into a source/type mirror.
    bound: Option<Arc<TypeName>>,
    field_count: Option<Shape>,
    reference: Option<TypeName>,
    error: Option<Diagnostic>,
}
impl Frame<'_> {
    fn referenced_layout(&self) -> bool {
        self.referenced || self.input.view
    }
}
// Only this active native call borrows data mutably. Moving the active-frame Vec
// cannot move the source tree or a shared bound input referenced by a cursor.
struct ActiveFrame<'a> {cursor:Cursor,data:Frame<'a>}
impl<'a> ActiveFrame<'a> {
    fn new(context:Context<'a>,input:TypeInput<'a>,stack:*mut BTreeSet<String>,referenced:bool)->Self {
        let descriptor=TypeInfo::of(&input);
        let builtin=Builtin::recognize(&input.name);
        let cursor=unsafe{type_start(descriptor,builtin.kind)};
        Self{cursor,data:Frame{context,input,builtin,referenced,stack,current:None,record:None,release:None,
            payloads:Vec::new(),bound:None,field_count:None,reference:None,error:None}}
    }
}

unsafe fn frame<'a>(raw: *mut c_void) -> &'a mut Frame<'a> {
    unsafe { &mut *raw.cast() }
}
unsafe fn input<'a>(value: TypeInfo) -> &'a TypeName {
    unsafe { &*value.value.cast::<TypeName>() }
}
fn scope<'a>(
    context: Context<'a>,
    record: &Option<RecordState<'a>>,
    release: &Option<ReleaseState>,
    site: Site,
    index: u64,
) -> Context<'a> {
    match site {
        Site::Root => context,
        Site::Record => record.as_ref().unwrap().context,
        Site::Field => {
            let r = record.as_ref().unwrap();
            Context {
                span: r.source.fields().at(index as usize).span(),
                ..r.context
            }
        }
        Site::Variant => {
            let r = record.as_ref().unwrap();
            Context {
                span: r.source.variants().at(index as usize).span(),
                ..r.context
            }
        }
        Site::CleanupParameter => {
            // Instance formation shares this World declaration's Definition Arc.
            let d=&context.world.functions[&release.as_ref().unwrap().name];
            Context {
                world: context.world,
                source: &d.source,
                span: d.function.params().at(0).span(),
                module: &d.module,
                imports: &d.imports,
            }
        }
    }
}
unsafe extern "C" fn failure(
    raw: *mut c_void,
    site: Site,
    index: u64,
    problem: Problem,
    detail: u64,
) {
    let f = unsafe { frame(raw) };
    let c = scope(f.context, &f.record, &f.release, site, index);
    let (code, message): (&'static str, String) = match problem {
        Problem::Contracts => ("B225", "输入／输出合同属于cfn函数签名".into()),
        Problem::Texture => (
            "B239",
            "texture2d／texture3d 只能作为 read 资源参数，不能作为普通值存储或返回".into(),
        ),
        Problem::Void => ("B228", "void 只能表示函数无返回值，不能作为数据类型".into()),
        Problem::Never => (
            "B228",
            "never 表示函数没有正常返回路径，不提供可存储的数据值".into(),
        ),
        Problem::Tile => (
            "B207",
            "tile<T> 只能作为 cooperative 函数的只读借用参数，不能存储或返回".into(),
        ),
        Problem::TaskArguments => ("B224", "任务类型需要一个类型实参".into()),
        Problem::TaskOverflow => ("B224", "任务错误载荷布局过大".into()),
        Problem::OwnedArguments => ("B224", "owned<T,F>需要载荷类型与静态释放函数".into()),
        Problem::ReleaseForm => ("B224", "释放函数由载荷推导泛型，声明处使用函数名称".into()),
        Problem::ReleaseShape => (
            "B224",
            "释放函数必须是接收一个按值载荷并返回void的普通on cpu函数".into(),
        ),
        Problem::DatasetView => ("B033", "dataset元素不能保存view借用".into()),
        Problem::DatasetViewSpaced => ("B033", "dataset 元素不能保存view借用".into()),
        Problem::OwnedCycle => ("B224", "释放函数的载荷合同不能递归要求同一拥有包装".into()),
        Problem::ReleaseResult => ("B224", "释放函数必须返回void".into()),
        Problem::ReleasePayload => {
            let r = f.release.as_ref().unwrap();
            let owner = r.owner.as_ref().unwrap();
            (
                "B124",
                format!(
                    "释放函数 {} 需要 {}，实际拥有载荷为 {}",
                    r.name,
                    f.current.as_ref().unwrap().name,
                    owner.arguments[0].name
                ),
            )
        }
        Problem::ResultArguments => ("B269", "result<T,E> 需要成功及错误两个类型".into()),
        Problem::ResultType => ("B269", "结果实参需要类型".into()),
        Problem::ResultTile => ("B269", "tile 是执行域借用，不能作为拥有结果载荷".into()),
        Problem::ResultOverflow => ("B269", "结果布局超过 u32 字节地址范围".into()),
        Problem::ArrayArguments => ("B238", "array<T,N> 需要元素类型和固定长度".into()),
        Problem::ArrayType => ("B238", "array 首参数需要值类型".into()),
        Problem::ArrayTile => ("B238", "tile 是执行域借用，不能作为拥有数组元素".into()),
        Problem::ArrayLength => ("B238", "array 长度需要 u32 编译期表达式".into()),
        Problem::ArrayOverflow => ("B238", "array 布局超过 u32 字节地址范围".into()),
        Problem::BuiltinArguments => ("B232", "内置数值类型不接受记录类型实参".into()),
        Problem::RecordArguments => {
            let r = f.record.as_ref().unwrap();
            (
                "B232",
                format!(
                    "类型 {} 需要 {} 个类型及 const 实参，实际为 {}",
                    r.key,
                    r.source.types().len() + r.source.values().len(),
                    f.input.arguments.len()
                ),
            )
        }
        Problem::RecordType => ("B232", "此记录位置需要普通值类型".into()),
        Problem::RecordTile => ("B232", "tile借用不能作为拥有类型实参".into()),
        Problem::Family => {
            let r = f.record.as_ref().unwrap();
            (
                "B232",
                format!(
                    "类型 {} 不属于 {} 的允许集合",
                    f.current.as_ref().unwrap().name,
                    r.source.types().at(index as usize).name()
                ),
            )
        }
        Problem::RecordValue => ("B235", "记录 const 实参需要 u32 编译期表达式".into()),
        Problem::RecordDomain => {
            let r = f.record.as_ref().unwrap();
            (
                "B235",
                format!(
                    "记录 const 实参 {detail} 不属于 {} 的允许值域",
                    r.source.values().at(index as usize).name()
                ),
            )
        }
        Problem::RecordCycle => ("B032", "结构体不能递归包含自身".into()),
        Problem::EnumScalar => ("B268", "编号枚举的表示类型需要整数标量".into()),
        Problem::VariantScalar => ("B268", "带载荷变体的表示类型需要 u32 或 i32".into()),
        Problem::DuplicateVariantName => ("B268", "同一变体名称不能重复".into()),
        Problem::DuplicateTag => ("B268", "带载荷变体的编号必须唯一，才能确定活动载荷".into()),
        Problem::VariantTile => ("B269", "tile是执行域借用，不能作为变体载荷".into()),
        Problem::VariantOverflow => ("B238", "变体布局超过u32字节地址范围".into()),
        Problem::DuplicateField => {
            let r = f.record.as_ref().unwrap();
            (
                "B034",
                format!("重复字段 {}", r.source.fields().at(index as usize).name()),
            )
        }
        Problem::FieldOverflow => ("B238", "记录布局超过 u32 字节地址范围".into()),
        Problem::AlignedOverflow => ("B238", "记录对齐后超过 u32 字节地址范围".into()),
        Problem::Stride => {
            let ty = f.record.as_ref().unwrap().ty.as_ref().unwrap();
            (
                "B241",
                format!(
                    "记录 stride {detail} 必须容纳自然布局 {} 且是对齐 {} 的倍数",
                    ty.size, ty.align
                ),
            )
        }
    };
    f.error = Some(Diagnostic {
        source: c.source.clone(),
        span: c.span,
        code,
        message,
    });
}
async fn function(f:&mut Frame<'_>)->Result<()> {
    let c=f.context;
    let value=unsafe { c.world.call(c.world.c_function_reference(&f.input,c.source,c.span,c.module,c.imports)).await }?;
    f.current=Some(value.into_ty());Ok(())
}

unsafe extern "C" fn builtin(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    f.current = Ty::from_builtin(f.builtin);
}
unsafe extern "C" fn unit(raw: *mut c_void) {
    unsafe { frame(raw) }.current = Some(Ty::unit());
}
unsafe extern "C" fn view(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    let element = f.current.take().unwrap();
    f.current = Some(Ty {
        name: format!("view<{}>", element.name),
        arguments: Vec::new(),
        values: Vec::new(),
        size: 24,
        align: 8,
        components: vec![
            (ir::Scalar::U32, 0),
            (ir::Scalar::U32, 4),
            (ir::Scalar::U32, 8),
            (ir::Scalar::U32, 12),
            (ir::Scalar::U32, 16),
        ],
        fields: Vec::new(),
        view: Some(Box::new(element)),
        array: None,
        enumeration: false,
        result: false,
        variants: Vec::new(),
        dataset: None,
        cleanup: None,
        function: None,
    });
}
unsafe extern "C" fn argument(parent: *mut c_void, index: u64) -> TypeInfo {
    match &unsafe { &*parent.cast::<TypeName>() }.arguments[index as usize] {
        GenericArgument::Type(value) => TypeInfo::of(value),
        GenericArgument::Value(_) => TypeInfo::absent(),
    }
}
async fn dataset(f:&mut Frame<'_>,value:TypeInfo,site:Site,index:u64)->Result<()> {
    let c=scope(f.context,&f.record,&f.release,site,index);
    let syntax=f.current.as_ref().unwrap().syntax();
    let count=unsafe { c.world.call(c.world.shape(&input(value).count,c.source,c.span,c.module,c.imports)).await }?;
    f.current=Some(Ty::owned_dataset(syntax,count));Ok(())
}

unsafe extern "C" fn cleanup(raw: *mut c_void, value: TypeInfo) -> CleanupInfo {
    let f = unsafe { frame(raw) };
    let name = unsafe { input(value) };
    let c = f.context;
    let key = if name.canonical && c.world.functions.contains_key(&name.name) {
        Some(name.name.clone())
    } else {
        c.world
            .resolve(&name.name, c.module, c.imports, true)
            .unique()
    };
    let Some(key) = key else {
        f.error = Some(Diagnostic {
            source: c.source.clone(),
            span: c.span,
            code: "B158",
            message: format!("未知或未导出的释放函数 {}", name.name),
        });
        return CleanupInfo {
            ready: 0,
            ordinary: 0,
            cpu: 0,
            parameters: 0,
            value_parameter: 0,
        };
    };
    let function = &c.world.functions[&key].function;
    let parameters = function.params();
    let info = CleanupInfo {
        ready: 1,
        ordinary: u32::from(function.kind() == FunctionKind::Value),
        cpu: u32::from(function.target() == Target::Cpu),
        parameters: parameters.len() as u64,
        value_parameter: u32::from(
            parameters.len() != 0 && parameters.at(0).access() == Access::Value,
        ),
    };
    f.release = Some(ReleaseState {
        name: key,
        owner: None,
        identity: None,
        instance: None,
    });
    info
}
unsafe extern "C" fn owner(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    let referenced = f.referenced_layout();
    let r = f.release.as_mut().unwrap();
    let mut ty = Ty::owned(f.current.take().unwrap(), r.name.clone());
    if referenced {
        ty.components.clear();
    }
    r.owner = Some(ty);
}
unsafe extern "C" fn enter_cleanup(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    let r = f.release.as_mut().unwrap();
    let identity = format!("owned contract:{}", r.owner.as_ref().unwrap().name);
    let entered = unsafe{&mut *f.stack}.insert(identity.clone());
    r.identity = Some(identity);
    u32::from(entered)
}
async fn specialize_cleanup(f:&mut Frame<'_>)->Result<()> {
    let r=f.release.as_mut().unwrap();
    let definition=&f.context.world.functions[&r.name];
    let instance=unsafe { f.context.world.call(crate::check::generics::specialize(f.context.world,&r.name,
        &[crate::check::generics::Actual::value(&r.owner.as_ref().unwrap().arguments[0])],None,
        (&definition.source,&definition.module,&definition.imports),definition.function.span())).await }
        .map_err(|error|Diagnostic{source:f.context.source.clone(),span:f.context.span,code:error.code,
            message:format!("释放函数 {}：{}",r.name,error.message)})?;
    r.instance=Some(instance);Ok(())
}

async fn bind(f:&mut Frame<'_>,which:Binding,index:u64)->Result<TypeInfo> {
    let value=match which {
        Binding::Representation|Binding::Field|Binding::Variant=>{
            let r=f.record.as_ref().unwrap();
            let(node,span)=match which {
                Binding::Representation=>(r.source.enumeration().unwrap(),r.source.span()),
                Binding::Field=>{let field=r.source.fields().at(index as usize);(field.ty(),field.span())},
                _=>{let variant=r.source.variants().at(index as usize);(variant.payload(),variant.span())},
            };
            unsafe { f.context.world.call(r.bindings.as_ref().unwrap().type_name(f.context.world,&node,
                (r.context.source,r.context.module,r.context.imports),span)).await }?
        }
        Binding::CleanupResult=>{
            let i=f.release.as_ref().unwrap().instance.as_ref().unwrap();let d=&i.definition;
            unsafe { f.context.world.call(i.bindings.type_name(f.context.world,&d.function.result(),
                (&d.source,&d.module,&d.imports),d.function.span())).await }?
        }
        Binding::CleanupParameter=>{
            let i=f.release.as_ref().unwrap().instance.as_ref().unwrap();let d=&i.definition;
            let parameter=d.function.params().at(0);
            unsafe { f.context.world.call(i.bindings.type_name(f.context.world,&parameter.ty(),
                (&d.source,&d.module,&d.imports),parameter.span())).await }?
        }
    };
    f.bound=Some(Arc::new(value));Ok(TypeInfo::of(f.bound.as_ref().unwrap()))
}

unsafe extern "C" fn leave_cleanup(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    unsafe{&mut *f.stack}
        .remove(f.release.as_ref().unwrap().identity.as_ref().unwrap());
}
unsafe extern "C" fn owner_matches(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    let expected = f.current.as_ref().unwrap();
    let payload = &f
        .release
        .as_ref()
        .unwrap()
        .owner
        .as_ref()
        .unwrap()
        .arguments[0];
    u32::from(if let (Some(expected), Some(actual)) = (&expected.dataset, &payload.dataset) {
        expected == actual
    } else {
        expected.same_layout(payload)
    })
}
unsafe extern "C" fn use_owner(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    f.current = f.release.as_mut().unwrap().owner.take();
}
unsafe extern "C" fn save_payload(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    f.payloads.push(f.current.take().unwrap());
}
unsafe extern "C" fn result(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    let error = f.payloads.pop().unwrap();
    let ok = f.payloads.pop().unwrap();
    f.current = if f.referenced_layout() { Ty::result_layout(ok, error) } else { Ty::result(ok, error) };
    u32::from(f.current.is_some())
}
async fn shape_argument(f:&mut Frame<'_>,index:u64)->Result<u32> {
    let c=f.context;let shape=f.input.arguments[index as usize].shape().unwrap();
    Ok(unsafe { c.world.call(c.world.shape(&Some(shape),c.source,c.span,c.module,c.imports)).await }?.unwrap())
}

unsafe extern "C" fn array_size(raw: *mut c_void, count: u32) -> u64 {
    let f = unsafe { frame(raw) };
    let element = f.current.as_ref().unwrap();
    unsafe { ir::fixed_array_layout(element.size, element.align, count) }.size
}
unsafe extern "C" fn array(raw: *mut c_void, count: u32, size: u32) {
    let f = unsafe { frame(raw) };
    let element = f.current.take().unwrap();
    let mut ty = Ty {
        name: format!("array<{},{}>", element.name, count),
        arguments: Vec::new(),
        values: vec![count],
        size,
        align: element.align,
        components: Vec::new(),
        fields: Vec::new(),
        view: None,
        array: Some(Box::new(element)),
        enumeration: false,
        result: false,
        variants: Vec::new(),
        dataset: None,
        cleanup: None,
        function: None,
    };
    if !f.referenced_layout() {
        ty.fill_components();
    }
    f.current = Some(ty);
}
#[repr(C)]
struct Cursor {
    value:TypeInfo,kind:Kind,stage:u32,wrap:u32,index:u64,
    temporary:TypeInfo,record:RecordInfo,scalar:u32,count:Count,
}
#[repr(u32)]
enum Query {Complete,Function,Child,Dataset,SpecializeCleanup,Bind,ShapeArgument,Family,VariantTag,Reference,FieldCount,RecordStride}
#[repr(C)]
struct Request {kind:Query,site:Site,binding:Binding,index:u64}

#[repr(C)]
struct Host {
    failure: unsafe extern "C" fn(*mut c_void, Site, u64, Problem, u64) -> (),
    builtin: unsafe extern "C" fn(*mut c_void) -> (),
    unit: unsafe extern "C" fn(*mut c_void) -> (),
    view: unsafe extern "C" fn(*mut c_void) -> (),
    argument: unsafe extern "C" fn(*mut c_void, u64) -> TypeInfo,
    cleanup: unsafe extern "C" fn(*mut c_void, TypeInfo) -> CleanupInfo,
    owner: unsafe extern "C" fn(*mut c_void) -> (),
    enter_cleanup: unsafe extern "C" fn(*mut c_void) -> u32,
    leave_cleanup: unsafe extern "C" fn(*mut c_void) -> (),
    owner_matches: unsafe extern "C" fn(*mut c_void) -> u32,
    use_owner: unsafe extern "C" fn(*mut c_void) -> (),
    save_payload: unsafe extern "C" fn(*mut c_void) -> (),
    result: unsafe extern "C" fn(*mut c_void) -> u32,
    array_size: unsafe extern "C" fn(*mut c_void, u32) -> u64,
    array: unsafe extern "C" fn(*mut c_void, u32, u32) -> (),
    record: unsafe extern "C" fn(*mut c_void) -> RecordInfo,
    type_argument: unsafe extern "C" fn(*mut c_void, u64) -> (),
    const_domain: unsafe extern "C" fn(*mut c_void, u64, u32) -> u32,
    const_argument: unsafe extern "C" fn(*mut c_void, u64, u32) -> (),
    arguments_done: unsafe extern "C" fn(*mut c_void) -> (),
    enter_record: unsafe extern "C" fn(*mut c_void) -> u32,
    leave_record: unsafe extern "C" fn(*mut c_void) -> (),
    simple_enum: unsafe extern "C" fn(*mut c_void) -> (),
    begin_variants: unsafe extern "C" fn(*mut c_void) -> (),
    unique_variant_name: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    unique_variant_tag: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    variant: unsafe extern "C" fn(*mut c_void, u64, u32) -> (),
    variants_done: unsafe extern "C" fn(*mut c_void) -> u32,
    begin_record: unsafe extern "C" fn(*mut c_void) -> (),
    unique_field: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    dataset_field: unsafe extern "C" fn(*mut c_void) -> TypeInfo,
    reference_dataset: unsafe extern "C" fn(*mut c_void, Count) -> (),
    field: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    record_done: unsafe extern "C" fn(*mut c_void) -> u32,
    stride_accepts: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    set_stride: unsafe extern "C" fn(*mut c_void, u32) -> (),
    use_record: unsafe extern "C" fn(*mut c_void) -> (),
    facts: unsafe extern "C" fn(*mut c_void) -> Facts,
    views: unsafe extern "C" fn(*mut c_void) -> u32,
    task: unsafe extern "C" fn(*mut c_void, u32) -> u32,
}
unsafe extern "C" {
    #[link_name="bootstrap.type_value::type_start"]
    fn type_start(value:TypeInfo,kind:Kind)->Cursor;
    #[link_name="bootstrap.type_value::type_step"]
    fn type_step(state:*mut Cursor,host:Host,context:*mut c_void)->Request;
}
fn host() -> Host {
    Host {
        failure,
        builtin,
        unit,
        view,
        argument,
        cleanup,
        owner,
        enter_cleanup,
        leave_cleanup,
        owner_matches,
        use_owner,
        save_payload,
        result,
        array_size,
        array,
        record,
        type_argument,
        const_domain,
        const_argument,
        arguments_done,
        enter_record,
        leave_record,
        simple_enum,
        begin_variants,
        unique_variant_name,
        unique_variant_tag,
        variant,
        variants_done,
        begin_record,
        unique_field,
        dataset_field,
        reference_dataset,
        field,
        record_done,
        stride_accepts,
        set_stride,
        use_record,
        facts,
        views,
        task,
    }
}


unsafe extern "C" fn record(raw: *mut c_void) -> RecordInfo {
    let f = unsafe { frame(raw) };
    let c = f.context;
    let key = if f.input.canonical {
        c.world
            .records
            .get_key_value(&f.input.name)
            .map(|(key, _)| key.clone())
    } else {
        c.world
            .resolve(&f.input.name, c.module, c.imports, false)
            .unique()
    };
    let Some(key) = key else {
        f.error = Some(Diagnostic {
            source: c.source.clone(),
            span: c.span,
            code: "B031",
            message: format!("未知或不明确的类型 {}", f.input.name),
        });
        return RecordInfo {
            ready: 0,
            types: 0,
            values: 0,
            fields: 0,
            variants: 0,
            enumeration: 0,
            stride: 0,
        };
    };
    let (source, s, m, i) = c.world.record(&key);
    let info = RecordInfo {
        ready: 1,
        types: source.types().len() as u64,
        values: source.values().len() as u64,
        fields: source.fields().len() as u64,
        variants: source.variants().len() as u64,
        enumeration: u32::from(source.enumeration().is_some()),
        stride: u32::from(source.stride().is_some()),
    };
    f.record = Some(RecordState {
        source,
        context: Context {
            world: c.world,
            source: s,
            span: source.span(),
            module: m,
            imports: i,
        },
        key,
        arguments: Vec::new(),
        values: Vec::new(),
        replacements: BTreeMap::new(),
        constants: BTreeMap::new(),
        bindings: None,
        ty: None,
        representation: None,
        variants: Vec::new(),
        names: BTreeSet::new(),
        tags: BTreeSet::new(),
    });
    info
}
async fn family(f:&mut Frame<'_>,index:u64)->Result<bool> {
    let r=f.record.as_ref().unwrap();let parameter=r.source.types().at(index as usize);let c=r.context;
    unsafe { c.world.call(c.world.family_accepts(&parameter.family(),f.current.as_ref().unwrap(),
        c.source,c.span,c.module,c.imports,&mut *f.stack)).await }
}

unsafe extern "C" fn type_argument(raw: *mut c_void, index: u64) {
    let f = unsafe { frame(raw) };
    let r = f.record.as_mut().unwrap();
    let actual = f.current.take().unwrap();
    r.replacements.insert(
        r.source.types().at(index as usize).name().to_owned(),
        actual.syntax(),
    );
    r.arguments.push(actual);
}
unsafe extern "C" fn const_domain(raw: *mut c_void, index: u64, value: u32) -> u32 {
    u32::from(unsafe { frame(raw) }
        .record
        .as_ref()
        .unwrap()
        .source
        .values()
        .at(index as usize)
        .accepts_const(value))
}
unsafe extern "C" fn const_argument(raw: *mut c_void, index: u64, value: u32) {
    let r = unsafe { frame(raw) }.record.as_mut().unwrap();
    r.values.push(value);
    r.constants.insert(
        r.source.values().at(index as usize).name().to_owned(),
        value,
    );
}
unsafe extern "C" fn arguments_done(raw: *mut c_void) {
    let r = unsafe { frame(raw) }.record.as_mut().unwrap();
    r.bindings = Some(Arc::new(generics::Bindings {
        types: mem::take(&mut r.replacements),
        values: mem::take(&mut r.constants),
    }));
    if !r.arguments.is_empty() || !r.values.is_empty() {
        r.key = generics::key(
            &r.key,
            r.arguments.iter().map(|ty| ty.name.as_str()),
            &r.values,
        );
    }
}
unsafe extern "C" fn enter_record(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    u32::from(unsafe{&mut *f.stack}.insert(f.record.as_ref().unwrap().key.clone()))
}
unsafe extern "C" fn leave_record(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    unsafe{&mut *f.stack}.remove(&f.record.as_ref().unwrap().key);
}
unsafe extern "C" fn simple_enum(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    let ty = f.current.as_mut().unwrap();
    ty.name = f.record.as_ref().unwrap().key.clone();
    ty.enumeration = true;
}
unsafe extern "C" fn begin_variants(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    f.record.as_mut().unwrap().representation = f.current.take();
}
unsafe extern "C" fn unique_variant_name(raw: *mut c_void, index: u64) -> u32 {
    let r = unsafe { frame(raw) }.record.as_mut().unwrap();
    u32::from(r.names
        .insert(r.source.variants().at(index as usize).name().to_owned()))
}
async fn variant_tag(f:&mut Frame<'_>,index:u64)->Result<u32> {
    let r=f.record.as_ref().unwrap();let variant=r.source.variants().at(index as usize);
    let expression=variant.discriminant();let c=r.context;
    unsafe { c.world.call(crate::check::variant_discriminant(c.world,&format!("{}::{}",r.key,variant.name()),
        c.source,c.module,c.imports,r.representation.as_ref().unwrap(),&expression,r.bindings.as_ref().unwrap())).await }
}

unsafe extern "C" fn unique_variant_tag(raw: *mut c_void, tag: u32) -> u32 {
    u32::from(unsafe { frame(raw) }
        .record
        .as_mut()
        .unwrap()
        .tags
        .insert(tag))
}
unsafe extern "C" fn variant(raw: *mut c_void, index: u64, tag: u32) {
    let f = unsafe { frame(raw) };
    let r = f.record.as_mut().unwrap();
    r.variants.push(ir::Variant {
        name: r.source.variants().at(index as usize).name().to_owned(),
        discriminant: tag,
        ty: Box::new(f.current.take().unwrap()),
    });
}
unsafe extern "C" fn variants_done(raw: *mut c_void) -> u32 {
    let f = unsafe { frame(raw) };
    let make = if f.referenced_layout() { Ty::variant_layout } else { Ty::variant };
    let r = f.record.as_mut().unwrap();
    let tag = r.representation.as_ref().unwrap().components[0].0;
    r.ty = make(
        r.key.clone(),
        mem::take(&mut r.arguments),
        mem::take(&mut r.values),
        tag,
        mem::take(&mut r.variants),
    );
    u32::from(r.ty.is_some())
}
unsafe extern "C" fn begin_record(raw: *mut c_void) {
    let r = unsafe { frame(raw) }.record.as_mut().unwrap();
    r.ty = Some(Ty {
        name: r.key.clone(),
        arguments: mem::take(&mut r.arguments),
        values: mem::take(&mut r.values),
        size: 0,
        align: 1,
        components: Vec::new(),
        fields: Vec::new(),
        view: None,
        array: None,
        enumeration: false,
        result: false,
        variants: Vec::new(),
        dataset: None,
        cleanup: None,
        function: None,
    });
}
unsafe extern "C" fn unique_field(raw: *mut c_void, index: u64) -> u32 {
    let r = unsafe { frame(raw) }.record.as_mut().unwrap();
    u32::from(r.names
        .insert(r.source.fields().at(index as usize).name().to_owned()))
}
unsafe extern "C" fn dataset_field(raw: *mut c_void) -> TypeInfo {
    let f = unsafe { frame(raw) };
    let value = Arc::get_mut(f.bound.as_mut().unwrap()).unwrap();
    value.dataset = false;
    f.field_count = value.count.take();
    TypeInfo::of(value)
}
async fn reference(f:&mut Frame<'_>,value:TypeInfo,site:Site,index:u64)->Result<()> {
    let c=scope(f.context,&f.record,&f.release,site,index);
    let value=unsafe { c.world.call(c.world.type_reference(input(value),c.source,c.span,c.module,c.imports)).await }?;
    f.reference=Some(value);Ok(())
}

async fn field_count(f:&mut Frame<'_>,index:u64)->Result<Count> {
    let c=scope(f.context,&f.record,&f.release,Site::Field,index);
    let value=unsafe { c.world.call(c.world.shape(&f.field_count,c.source,c.span,c.module,c.imports)).await }?;
    Ok(Count{present:u32::from(value.is_some()),value:value.unwrap_or(0)})
}

unsafe extern "C" fn reference_dataset(raw: *mut c_void, count: Count) {
    let f = unsafe { frame(raw) };
    f.current = Some(Ty::owned_dataset(
        f.reference.take().unwrap(),
        (count.present != 0).then_some(count.value),
    ));
}
unsafe extern "C" fn field(raw: *mut c_void, index: u64) -> u32 {
    let f = unsafe { frame(raw) };
    let field = f.current.take().unwrap();
    let referenced = f.referenced_layout();
    let r = f.record.as_mut().unwrap();
    let ty = r.ty.as_mut().unwrap();
    let offset = if referenced { ty.place_field_layout(&field) } else { ty.append_field_layout(&field) };
    match offset {
        Some(offset) => {
            ty.fields.push((
                r.source.fields().at(index as usize).name().to_owned(),
                offset,
                Box::new(field),
            ));
            1
        }
        None => 0,
    }
}
unsafe extern "C" fn record_done(raw: *mut c_void) -> u32 {
    u32::from(unsafe { frame(raw) }
        .record
        .as_mut()
        .unwrap()
        .ty
        .as_mut()
        .unwrap()
        .finish_record_layout()
        .is_some())
}
async fn record_stride(f:&mut Frame<'_>)->Result<u32> {
    let r=f.record.as_ref().unwrap();let c=r.context;
    unsafe { c.world.call(c.world.source_shape(&r.source.stride().unwrap(),r.bindings.as_ref().unwrap(),
        c.source,c.span,c.module,c.imports)).await }
}

unsafe extern "C" fn stride_accepts(raw: *mut c_void, stride: u32) -> u32 {
    let ty = unsafe { frame(raw) }
        .record
        .as_ref()
        .unwrap()
        .ty
        .as_ref()
        .unwrap();
    u32::from(unsafe { super::accepts_record_stride(ty.size, ty.align, stride) })
}
unsafe extern "C" fn set_stride(raw: *mut c_void, stride: u32) {
    unsafe { frame(raw) }
        .record
        .as_mut()
        .unwrap()
        .ty
        .as_mut()
        .unwrap()
        .size = stride;
}
unsafe extern "C" fn use_record(raw: *mut c_void) {
    let f = unsafe { frame(raw) };
    f.current = f.record.as_mut().unwrap().ty.take();
}
unsafe extern "C" fn facts(raw: *mut c_void) -> Facts {
    Facts::of(unsafe { frame(raw) }.current.as_ref().unwrap())
}
unsafe extern "C" fn views(raw: *mut c_void) -> u32 {
    u32::from(unsafe { frame(raw) }
        .current
        .as_ref()
        .unwrap()
        .contains_views())
}
unsafe extern "C" fn task(raw: *mut c_void, error: u32) -> u32 {
    let f = unsafe { frame(raw) };
    let payload = f.current.take().unwrap();
    f.current = if error != 0 {
        if f.referenced_layout() { Ty::task_start_error_layout(payload) } else { Ty::task_start_error(payload) }
    } else {
        Some(Ty::task(payload))
    };
    u32::from(f.current.is_some())
}

pub(super) async fn resolve(
    world:&World,value:&TypeName,source:&Source,span:Span,module:&str,imports:&[String],
    stack:&mut BTreeSet<String>,
)->Result<Ty> {
    let context=Context{world,source,span,module,imports};
    let mut frames=vec![ActiveFrame::new(context,TypeInput::borrowed(value),stack,false)];
    loop {
        let active=frames.last_mut().unwrap();
        let request=unsafe{type_step(&mut active.cursor,host(),(&mut active.data as *mut Frame<'_>).cast())};
        if let Some(error)=active.data.error.take(){return Err(error);}
        match request.kind {
            Query::Complete=>{
                let finished=frames.pop().unwrap();
                let ty=finished.data.current.unwrap();
                if let Some(parent)=frames.last_mut(){parent.data.current=Some(ty);}else{return Ok(ty);}
            }
            Query::Child=>{
                let context=scope(active.data.context,&active.data.record,&active.data.release,request.site,request.index);
                let input=match request.site {
                    Site::Root=>active.data.input.child(active.cursor.temporary),
                    _=>TypeInput::shared(active.data.bound.as_ref().unwrap()),
                };
                let child=ActiveFrame::new(context,input,active.data.stack,active.data.referenced_layout());
                frames.push(child);
            }
            Query::Function=>unsafe { world.call(function(&mut active.data)).await }?,
            Query::Dataset=>unsafe { world.call(dataset(&mut active.data,active.cursor.temporary,request.site,request.index)).await }?,
            Query::SpecializeCleanup=>unsafe { world.call(specialize_cleanup(&mut active.data)).await }?,
            Query::Bind=>active.cursor.temporary=unsafe { world.call(bind(&mut active.data,request.binding,request.index)).await }?,
            Query::ShapeArgument=>active.cursor.scalar=unsafe { world.call(shape_argument(&mut active.data,request.index)).await }?,
            Query::Family=>active.cursor.scalar=u32::from(unsafe { world.call(family(&mut active.data,request.index)).await }?),
            Query::VariantTag=>active.cursor.scalar=unsafe { world.call(variant_tag(&mut active.data,request.index)).await }?,
            Query::Reference=>unsafe { world.call(reference(&mut active.data,active.cursor.temporary,request.site,request.index)).await }?,
            Query::FieldCount=>active.cursor.count=unsafe { world.call(field_count(&mut active.data,request.index)).await }?,
            Query::RecordStride=>active.cursor.scalar=unsafe { world.call(record_stride(&mut active.data)).await }?,
        }
    }
}
