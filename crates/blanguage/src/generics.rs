//! 从声明绑定具体类型与静态值；具体实例沿原入口检查和 IR 执行。
use crate::{World, ast::*, ir::Ty};
use std::{collections::BTreeSet, ffi::c_void, fmt, marker::PhantomData};

mod source_bind;
mod instance;
pub(crate) use instance::specialize;

pub(crate) use instance::is_open;
#[repr(C)]
#[derive(Clone, Copy)]
struct NameBytes { bytes: *const u8, length: u64 }
impl NameBytes {
    fn new(text: &str) -> Self { Self { bytes: text.as_ptr(), length: text.len() as u64 } }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct InstanceName {
    base: NameBytes,
    types: *const NameBytes,
    type_count: u64,
    values: *const u32,
    value_count: u64,
}
unsafe extern "C" {
    #[link_name = "bootstrap.instance_domain::key_size"]
    fn instance_key_size(name: InstanceName) -> u64;
    #[link_name = "bootstrap.instance_domain::write_key"]
    fn write_instance_key(name: InstanceName, output: *mut u8);
    #[link_name = "bootstrap.instance_domain::matches_key"]
    fn matches_instance_key(name: InstanceName, actual: NameBytes) -> bool;
}

fn with_instance_name<'a, T>(
    base: &str,
    types: impl IntoIterator<Item = &'a str>,
    values: &[u32],
    consume: impl FnOnce(InstanceName) -> T,
) -> T {
    let types = types.into_iter().map(NameBytes::new).collect::<Vec<_>>();
    consume(InstanceName {
        base: NameBytes::new(base),
        types: types.as_ptr(),
        type_count: types.len() as u64,
        values: values.as_ptr(),
        value_count: values.len() as u64,
    })
}

pub(crate) fn key<'a>(base: &str, types: impl IntoIterator<Item = &'a str>, values: &[u32]) -> String {
    with_instance_name(base, types, values, |name| {
        // B initializes the final UTF-8 bytes while the input is still borrowed.
        let length = unsafe { instance_key_size(name) } as usize;
        let mut bytes = Vec::with_capacity(length);
        unsafe {
            write_instance_key(name, bytes.as_mut_ptr());
            bytes.set_len(length);
            String::from_utf8_unchecked(bytes)
        }
    })
}

pub(crate) fn record_key_matches(base: &str, actual: &Ty) -> bool {
    with_instance_name(base, actual.arguments.iter().map(|ty| ty.name.as_str()), &actual.values, |name| {
        // B compares against the existing name without producing another string.
        unsafe { matches_instance_key(name, NameBytes::new(&actual.name)) }
    })
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct BindingView<'a> {
    rows: *mut c_void,
    count: u64,
    owner: PhantomData<&'a Bindings>,
}

#[repr(C)]
#[derive(Default)]
struct BindingTable {
    view: BindingView<'static>,
    capacity: u64,
}

#[repr(C)]
struct BindingEntry {
    name: NameBytes,
    type_value: *const c_void,
    const_value: u32,
    kind: u32,
}

#[derive(Default)]
pub(crate) struct Bindings(BindingTable);

// Insertion requires exclusive ownership. Published instances share immutable
// B rows and TypeName values, and the last owner releases both.
unsafe impl Send for Bindings {}
unsafe impl Sync for Bindings {}

unsafe extern "C" {
    #[link_name = "bootstrap.generic_bindings::bind_type"]
    fn bind_type(table: &mut BindingTable, name: NameBytes, value: *mut c_void, wrapped: u32,
        release: unsafe extern "C" fn(*mut c_void)) -> bool;
    #[link_name = "bootstrap.generic_bindings::bind_value"]
    fn bind_value(table: &mut BindingTable, name: NameBytes, value: u32) -> bool;
    #[link_name = "bootstrap.generic_bindings::bound_type"]
    fn bound_type(table: BindingView<'_>, name: NameBytes) -> *const c_void;
    #[link_name = "bootstrap.generic_bindings::bound_value"]
    fn bound_value(table: BindingView<'_>, name: NameBytes) -> crate::syntax::wire::Optional<u32>;
    #[link_name = "bootstrap.generic_bindings::entry_at"]
    fn binding_entry(table: BindingView<'_>, index: u64) -> BindingEntry;
    #[link_name = "bootstrap.generic_bindings::release"]
    fn release_bindings(table: BindingView<'_>, release: unsafe extern "C" fn(*mut c_void));
}

unsafe extern "C" fn release_type(value: *mut c_void) {
    drop(unsafe { Box::from_raw(value.cast::<TypeName>()) });
}

impl Drop for Bindings {
    fn drop(&mut self) {
        if !self.0.view.rows.is_null() {
            unsafe { release_bindings(self.view(), release_type) };
        }
    }
}

// A bound call reads the original instance; it does not rewrite its source node.
pub(crate) enum TypeCall<'a> { Builtin(&'a str), Construct(&'a TypeName) }
impl Bindings {
    pub(crate) fn view(&self) -> BindingView<'_> { self.0.view }
    pub(crate) fn add_type(&mut self, name: &str, value: TypeName) {
        let wrapped = u32::from(value.dataset || value.view || value.tile);
        let value = Box::into_raw(Box::new(value)).cast();
        if !unsafe { bind_type(&mut self.0, NameBytes::new(name), value, wrapped, release_type) } {
            std::process::abort();
        }
    }
    pub(crate) fn add_value(&mut self, name: &str, value: u32) {
        if !unsafe { bind_value(&mut self.0, NameBytes::new(name), value) } {
            std::process::abort();
        }
    }
    pub(crate) fn value(&self, name: &str) -> Option<u32> {
        let value = unsafe { bound_value(self.view(), NameBytes::new(name)) };
        (value.present != 0).then_some(value.value)
    }
    pub(crate) fn shape_identity(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(|format| {
            self.format_entries(format, 1)?;
            format.write_str(":")?;
            self.format_entries(format, 2)
        })
    }
    fn format_entries(&self, format: &mut fmt::Formatter<'_>, kind: u32) -> fmt::Result {
        let mut map = format.debug_map();
        for index in 0..self.0.view.count {
            let entry = unsafe { binding_entry(self.view(), index) };
            if entry.kind != kind { continue; }
            let name = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                    entry.name.bytes, entry.name.length as usize,
                ))
            };
            if kind == 1 {
                map.entry(&name, unsafe { &*entry.type_value.cast::<TypeName>() });
            } else {
                map.entry(&name, &entry.const_value);
            }
        }
        map.finish()
    }
    pub(crate) fn type_call(&self,name:&str)->Option<TypeCall<'_>> {
        let actual = unsafe { bound_type(self.view(), NameBytes::new(name)).cast::<TypeName>().as_ref()? };
        Some(if actual.arguments.is_empty()&&crate::builtin_types::Builtin::recognize(&actual.name).is_scalar() {
            TypeCall::Builtin(&actual.name)
        }else{TypeCall::Construct(actual)})
    }
    fn check_uses(
        &self, source: &Source,
        inspect: impl FnOnce(BindingView<'_>) -> crate::syntax::wire::UseError,
    ) -> Result<()> {
        if self.0.view.count == 0 { return Ok(()); }
        let error = inspect(self.view());
        if error.code == 0 { return Ok(()); }
        let (code, message) = [
            ("B232", "类型参数已代表完整具体类型，不能再附加类型实参"),
            ("B232", "资源包装的元素需要普通存储类型，不能再次包装资源"),
            ("B232", "类型参数已代表完整类型，不能再附加实参"),
            ("B235", "const 参数不是可调用函数"),
            ("B235", "局部绑定不能覆盖本函数的 const 参数"),
        ][error.code as usize - 1];
        source.fail(Span { start: error.span.start as usize, end: error.span.end as usize }, code, message)
    }
    fn check_type(&self, ty: &crate::syntax::Type, source: &Source, span: Span) -> Result<()> {
        let span = crate::syntax::wire::Span { start: span.start as u64, end: span.end as u64 };
        self.check_uses(source, |bindings| ty.bound_uses(bindings, span))
    }
    fn check_function(&self, function: &crate::syntax::Function, source: &Source) -> Result<()> {
        self.check_uses(source, |bindings| function.bound_uses(bindings))
    }
    pub(crate) fn check_expression(&self, expression: &crate::syntax::Expr, source: &Source) -> Result<()> {
        self.check_uses(source, |bindings| expression.bound_uses(bindings))
    }
    pub(crate) async fn type_name(
        self: &std::sync::Arc<Self>, world: &World, source_type: &crate::syntax::Type,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<TypeName> {
        self.check_type(source_type, context.0, span)?;
        unsafe { world.call(source_bind::type_name(self, world, source_type, context, span)) .await }
    }
    pub(crate) async fn arguments(
        self: &std::sync::Arc<Self>, world: &World,
        arguments: &crate::syntax::List<crate::syntax::wire::ArgumentNode>,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<Vec<GenericArgument>> {
        unsafe { world.call(source_bind::arguments(self, world, arguments, context, span)) .await }
    }

}

#[derive(Clone)]
pub(crate) struct Instance {
    pub name:String,
    pub definition:std::sync::Arc<crate::Definition>,
    pub bindings:std::sync::Arc<Bindings>,
}
impl Instance {
    pub(crate) async fn parameter(&self, world:&World, parameter:crate::syntax::Param)->Result<Param>{
        let definition=&self.definition;
        Ok(Param{name:parameter.name().into(),access:parameter.access(),span:parameter.span(),
            ty:unsafe { world.call(self.bindings.type_name(world,&parameter.ty(),
                (&definition.source,&definition.module,&definition.imports),parameter.span())) .await }?})
    }
    pub(crate) fn parameters<'a>(&'a self,world:&'a World)->impl Iterator<Item=impl std::future::Future<Output=Result<Param>>+'a>+'a{
        self.definition.function.params().into_iter().map(move|parameter|self.parameter(world,parameter))
    }
    pub(crate) fn contracts(&self) -> impl Iterator<Item = FunctionContract> + '_ {
        self.definition.function.contracts().iter()
            .filter_map(|contract| source_bind::contract(contract, self.bindings.view()))
    }
}
#[derive(Clone)]
pub(crate) struct TypeArgument {
    pub name: String,
    pub syntax: TypeName,
}

pub(crate) use instance::instantiate;
pub(crate) fn validate_declaration(definition: &crate::Definition) -> Result<()> {
    let f = &definition.function;
    crate::check::validate_signature(definition)?;
    let error = unsafe { crate::syntax::wire::function_parameter_error(f.signature_node()) };
    parameter_diagnostic(error, &definition.source, f.span())
}
pub(crate) async fn for_each_instance(
    world: &World,
    base: &str,
    definition: &std::sync::Arc<crate::Definition>,
    mut consume: impl FnMut(Instance),
) -> Result<bool> {
    let f = &definition.function;
    unsafe { world.call(for_each_combination(
        world,
        &f.types(),
        &f.values(),
        (&definition.source, f.span(), &definition.module, &definition.imports),
        async |arguments, values| {
            let instance = unsafe { world.call(instantiate(world, base, definition, arguments, values)).await }?;
            consume(instance);
            Ok(())
        },
    )).await }
}
pub(crate) fn validate_record_parameters(record: &crate::syntax::Record, source: &Source) -> Result<()> {
    let error = unsafe { crate::syntax::wire::record_parameter_error(record.signature_node()) };
    parameter_diagnostic(error, source, record.span())
}

fn parameter_diagnostic(error: crate::syntax::wire::ParameterError, source: &Source, span: Span) -> Result<()> {
    use crate::syntax::wire::ParameterError;
    match error {
        ParameterError::Valid => Ok(()),
        ParameterError::TypeName => source.fail(span, "B231", "类型参数不能覆盖内置类型或当前声明名"),
        ParameterError::EmptyConstDomain => source.fail(span, "B235", "const 参数至少需要一个允许的 u32 值"),
        ParameterError::ConstName => source.fail(span, "B235", "const 参数不能覆盖内置名称、当前声明名或普通参数"),
    }
}

pub(crate) async fn for_each_combination(
    world: &World,
    types: &crate::syntax::List<crate::syntax::wire::TypeParameterNode>,
    values: &crate::syntax::List<crate::syntax::wire::ValueParameterNode>,
    (source, span, module, imports): (&Source, Span, &str, &[String]),
    mut consume: impl std::ops::AsyncFnMut(Vec<TypeArgument>, &[u32]) -> Result<()>,
) -> Result<bool> {
    let families = types.into_iter().filter_map(|parameter| parameter.family());
    if is_open(types, values) {
        for family in families {
            if let crate::syntax::FamilyKind::Named(name) = family.kind() {
                world.validate_named_family(
                    name, (source, span, module, imports), &mut BTreeSet::new(),
                ).await?;
            }
        }
        return Ok(true);
    }

    let mut domains = Vec::with_capacity(types.len());
    for family in families {
        let members = unsafe {
            world.call(world.family_arguments(&family, source, span, module, imports, &mut BTreeSet::new())).await
        }?;
        domains.push(members);
    }
    let sizes: Vec<_> = domains.iter().map(|domain| domain.len() as u64).collect();
    let mut combinations=values.instance_combinations(&sizes);
    let mut first=true;
    while let Some((indices,row))=combinations.advance(first) {
        let arguments=domains.iter().zip(indices)
            .map(|(domain,&index)|domain[index as usize].clone()).collect();
        consume(arguments,row).await?;
        first=false;
    }
    Ok(false)
}
