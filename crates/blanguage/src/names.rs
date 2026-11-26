//! Shared name spans and the original ordered name and dependency operations.
use crate::syntax::{List, wire::FunctionNode};
use std::{cell::UnsafeCell, ffi::c_void, marker::PhantomData};
#[derive(Clone,Copy)]
#[repr(C)]
pub(crate) struct Bytes { bytes:*const u8, length:u64 }
impl Bytes {
    pub(crate) fn new(value:&str)->Self {Self{bytes:value.as_ptr(),length:value.len()as u64}}
    pub(crate) fn absent()->Self {Self{bytes:std::ptr::null(),length:0}}
    pub(crate) unsafe fn text<'a>(self)->&'a str {
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
unsafe extern "C" {
    #[link_name = "bootstrap.names::collect_external_module_names"]
    fn collect_external_module_names(
        local: *const ModuleName<'_>, local_count: usize,
        references: *mut c_void, next: unsafe extern "C" fn(*mut c_void) -> Bytes,
        external: *mut c_void, push: unsafe extern "C" fn(*mut c_void, Bytes),
    );
    #[link_name = "bootstrap.names::enter_dependency"]
    fn enter_dependency(path: *mut DependencyPath, key: Bytes) -> DependencyEntry;
    #[link_name = "bootstrap.names::leave_dependency"]
    fn leave_dependency(path: *mut DependencyPath);
    #[link_name = "bootstrap.names::release_dependencies"]
    fn release_dependencies(path: *mut DependencyPath);
    #[link_name = "bootstrap.names::order_imports"]
    fn order_imports(names: *mut Bytes, count: u64) -> u64;
    #[link_name = "bootstrap.names::pair_functions"]
    fn pair_functions(names: *mut FunctionName, declarations: u64, implementations: u64, output: *mut u64);
}

#[repr(C)]
struct DependencyPath {
    rows: *mut Bytes,
    count: u64,
    capacity: u64,
}

#[allow(dead_code)] // Produced by the B stack.
#[repr(u32)]
enum DependencyStatus { Entered, Recursive, Allocation, Overflow }

#[repr(C)]
struct DependencyEntry { status: DependencyStatus, bytes: u64 }

pub(crate) struct ActiveDependencies {
    path: UnsafeCell<DependencyPath>,
}

impl Default for ActiveDependencies {
    fn default() -> Self {
        Self { path: UnsafeCell::new(DependencyPath { rows: std::ptr::null_mut(), count: 0, capacity: 0 }) }
    }
}

impl Drop for ActiveDependencies {
    fn drop(&mut self) {
        unsafe { release_dependencies(self.path.get_mut()); }
    }
}

struct DependencyScope<'a>(&'a ActiveDependencies);

impl Drop for DependencyScope<'_> {
    fn drop(&mut self) {
        unsafe { leave_dependency(self.0.path.get()); }
    }
}

impl ActiveDependencies {
    pub(crate) async fn evaluate<T>(&self, key: &str, evaluate: impl std::ops::AsyncFnOnce() -> T) -> Option<T> {
        let entered = unsafe { enter_dependency(self.path.get(), Bytes::new(key)) };
        match entered.status {
            DependencyStatus::Entered => {}
            DependencyStatus::Recursive => return None,
            DependencyStatus::Allocation => std::alloc::handle_alloc_error(
                std::alloc::Layout::from_size_align(entered.bytes as usize, std::mem::align_of::<Bytes>()).unwrap(),
            ),
            DependencyStatus::Overflow => panic!("capacity overflow"),
        }
        // The original key remains borrowed until this evaluation leaves the B stack.
        let _scope = DependencyScope(self);
        Some(evaluate().await)
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
