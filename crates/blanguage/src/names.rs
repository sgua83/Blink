//! B chooses names and visibility; the original declaration maps keep their storage.
use crate::{frontend::NameResolution, syntax::{List, wire::FunctionNode}};
use std::{cell::RefCell, ffi::c_void, marker::PhantomData};
#[derive(Clone,Copy)]
#[repr(C)]
struct Bytes { bytes:*const u8, length:u64 }
impl Bytes {
    fn new(value:&str)->Self {Self{bytes:value.as_ptr(),length:value.len()as u64}}
    unsafe fn text<'a>(self)->&'a str {
        unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes,self.length as usize))}
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub(crate) struct ModuleName<'a>(Bytes, PhantomData<&'a str>);

impl<'a> ModuleName<'a> {
    fn new(value: &'a str) -> Self {
        Self(Bytes::new(value), PhantomData)
    }

    pub(crate) fn as_str(&self) -> &'a str {
        // The original image owns the UTF-8, and B only reorders these views.
        unsafe { self.0.text() }
    }
}

#[repr(C)]
struct FunctionName {
    name: Bytes,
    ordinal: u64,
}
#[repr(C)]
struct Hit { name:Bytes, public:u64, present:u64 }
#[repr(C)]
struct Resolution { name:Bytes, count:u64 }
pub(crate) struct Declaration<'a> { pub name:&'a str, pub public:bool }
unsafe extern "C" {
    #[link_name = "bootstrap.names::collect_external_module_names"]
    fn collect_external_module_names(
        local: *const ModuleName<'_>, local_count: usize,
        references: *mut c_void, next: unsafe extern "C" fn(*mut c_void) -> Bytes,
        external: *mut c_void, push: unsafe extern "C" fn(*mut c_void, Bytes),
    );
    #[link_name = "bootstrap.names::dependency_active"]
    fn dependency_active(path: *const Bytes, count: u64, key: Bytes) -> bool;
    #[link_name = "bootstrap.names::order_imports"]
    fn order_imports(names: *mut Bytes, count: u64) -> u64;
    #[link_name = "bootstrap.names::pair_functions"]
    fn pair_functions(names: *mut FunctionName, declarations: u64, implementations: u64, output: *mut u64);
    #[link_name="bootstrap.names::resolve"]
    fn choose(
        name: Bytes, module: Bytes, count: u64, context: *mut c_void,
        import_name: unsafe extern "C" fn(*const c_void, u64) -> Bytes,
        lookup: unsafe extern "C" fn(*mut c_void, Bytes, Bytes) -> Hit,
    ) -> Resolution;
}

#[derive(Default)]
pub(crate) struct ActiveDependencies {
    path: RefCell<Vec<Bytes>>,
}

struct DependencyScope<'a>(&'a ActiveDependencies);

impl Drop for DependencyScope<'_> {
    fn drop(&mut self) {
        self.0.path.borrow_mut().pop();
    }
}

impl ActiveDependencies {
    pub(crate) async fn evaluate<T>(&self, key: &str, evaluate: impl std::ops::AsyncFnOnce() -> T) -> Option<T> {
        let key = Bytes::new(key);
        {
            let path = self.path.borrow();
            if unsafe { dependency_active(path.as_ptr(), path.len() as u64, key) } {
                return None;
            }
        }
        self.path.borrow_mut().push(key);
        // The key stays borrowed through this call; remove its descriptor before returning or unwinding.
        let _scope = DependencyScope(self);
        Some(evaluate().await)
    }
}
struct Resolver<'a, F> {
    imports: &'a [String],
    declarations: F,
    candidate: String,
}

unsafe extern "C" fn import_name<F>(context: *const c_void, index: u64) -> Bytes {
    let resolver = unsafe { &*context.cast::<Resolver<'_, F>>() };
    Bytes::new(&resolver.imports[index as usize])
}

unsafe extern "C" fn lookup<'a, F: Fn(&str) -> Option<Declaration<'a>>>(
    context: *mut c_void, prefix: Bytes, name: Bytes,
) -> Hit {
    let resolver = unsafe { &mut *context.cast::<Resolver<'_, F>>() };
    let name = unsafe { name.text() };
    let key = if prefix.bytes.is_null() {
        name
    } else {
        let prefix = unsafe { prefix.text() };
        let key = &mut resolver.candidate;
        key.clear();
        key.reserve(prefix.len() + 2 + name.len());
        key.push_str(prefix);
        key.push_str("::");
        key.push_str(name);
        key.as_str()
    };
    match (resolver.declarations)(key) {
        Some(declaration) => Hit { name: Bytes::new(declaration.name), public: u64::from(declaration.public), present: 1 },
        None => Hit { name: Bytes::new(""), public: 0, present: 0 },
    }
}

pub(crate) fn resolve<'a, F: Fn(&str) -> Option<Declaration<'a>>>(
    name: &str, module: &str, imports: &[String], declarations: F,
) -> NameResolution {
    let mut resolver = Resolver { imports, declarations, candidate: String::new() };
    let result = unsafe {
        choose(Bytes::new(name), Bytes::new(module), imports.len() as u64,
            std::ptr::from_mut(&mut resolver).cast(), import_name::<F>, lookup::<F>)
    };
    match result.count {
        0=>NameResolution::Missing,
        1=>NameResolution::Unique(unsafe{result.name.text()}.to_owned()),
        _=>NameResolution::Ambiguous,
    }
}


pub(crate) fn module_imports(
    interface: &crate::syntax::Module,
    implementation: Option<&crate::syntax::Module>,
) -> std::sync::Arc<[String]> {
    let mut names = interface.imports()
        .chain(implementation.into_iter().flat_map(crate::syntax::Module::imports))
        .map(Bytes::new)
        .collect::<Vec<_>>();
    // B rearranges borrowed descriptors in this call. Both syntax owners stay live.
    let count = unsafe { order_imports(names.as_mut_ptr(), names.len() as u64) };
    names.truncate(count as usize);
    names.into_iter().map(|name| unsafe { name.text() }.to_owned()).collect()
}


pub(crate) fn function_pairs(
    declarations: &List<FunctionNode>,
    implementations: Option<&List<FunctionNode>>,
) -> Option<Vec<u64>> {
    let implementations = implementations?;
    if declarations.is_empty() || implementations.is_empty() {
        return None;
    }
    let mut names = declarations.iter().chain(implementations.iter()).enumerate()
        .map(|(ordinal, function)| FunctionName { name: Bytes::new(function.name()), ordinal: ordinal as u64 })
        .collect::<Vec<_>>();
    // The two lists retain the source storage through this synchronous call.
    // Each row receives the first matching index in the other list, or the length of that list.
    let mut pairs = vec![0; names.len()];
    unsafe {
        pair_functions(names.as_mut_ptr(), declarations.len() as u64, implementations.len() as u64, pairs.as_mut_ptr());
    }
    Some(pairs)
}

unsafe extern "C" fn next_module_reference<'a, I: Iterator<Item = &'a str>>(source: *mut c_void) -> Bytes {
    let references = unsafe { &mut *source.cast::<I>() };
    references.next().map_or(Bytes { bytes: std::ptr::null(), length: 0 }, Bytes::new)
}

unsafe extern "C" fn push_module_reference(output: *mut c_void, name: Bytes) {
    let external = unsafe { &mut *output.cast::<Vec<ModuleName<'_>>>() };
    external.push(ModuleName(name, PhantomData));
}

pub(crate) fn module_names<'a, R: IntoIterator<Item = &'a str>>(
    local: impl IntoIterator<Item = &'a str>, references: R,
) -> (Vec<ModuleName<'a>>, Vec<ModuleName<'a>>) {
    let mut local: Vec<_> = local.into_iter().map(ModuleName::new).collect();
    let defined = unsafe { order_imports(local.as_mut_ptr().cast(), local.len() as u64) };
    local.truncate(defined as usize);

    let mut references = references.into_iter();
    let mut external: Vec<ModuleName<'a>> = Vec::new();
    // B borrows source names for this call; the original image outlives both outputs.
    unsafe {
        collect_external_module_names(local.as_ptr(), local.len(),
            std::ptr::from_mut(&mut references).cast(), next_module_reference::<R::IntoIter>,
            std::ptr::from_mut(&mut external).cast(), push_module_reference);
    }
    let used = unsafe { order_imports(external.as_mut_ptr().cast(), external.len() as u64) };
    external.truncate(used as usize);
    (local, external)
}
