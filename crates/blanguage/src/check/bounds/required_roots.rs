use super::{Clause, Facts, RequiredFacts};
use std::{cell::RefCell, ffi::c_void, mem::MaybeUninit};

#[repr(C)]
#[derive(Default)]
struct Results {
    records: *mut c_void,
    root: usize,
    count: usize,
    capacity: usize,
}

#[repr(C)]
struct CachedResult {
    // B owns this key and initializes the result tag before returning the slot.
    _environment: Facts,
    possible: u64,
    value: MaybeUninit<RequiredFacts>,
}

#[repr(C)]
struct CachedRow {
    // The native OrderedNode links precede the actual cache entry.
    _left: u64,
    _right: u64,
    _parent: u64,
    _height: u64,
    _entry: CachedResult,
}

const ENTRY_STRIDE: usize = size_of::<CachedRow>();

unsafe extern "C" {
    #[link_name = "bootstrap.clause_storage::required_cache_lookup"]
    fn lookup(cache: &Results, root: *mut c_void, environment: &Facts, stride: usize) -> *const CachedResult;
    #[link_name = "bootstrap.clause_storage::required_cache_reserve"]
    fn reserve(cache: &mut Results, root: *mut c_void, environment: &Facts, stride: usize,
        drop_result: unsafe extern "C" fn(*mut CachedResult)) -> *mut CachedResult;
    #[link_name = "bootstrap.clause_storage::required_cache_clear"]
    fn clear(cache: &mut Results, stride: usize, drop_result: unsafe extern "C" fn(*mut CachedResult));
}

unsafe extern "C" fn drop_result(entry: *mut CachedResult) {
    unsafe { (*entry).value.assume_init_drop(); }
}

impl Drop for Results {
    fn drop(&mut self) { unsafe { clear(self, ENTRY_STRIDE, drop_result) }; }
}

thread_local! {
    static RESULTS: RefCell<Option<Results>> = const { RefCell::new(None) };
}

pub(super) fn scope() -> impl Drop {
    struct Scope(Option<Results>);
    impl Drop for Scope {
        fn drop(&mut self) {
            RESULTS.replace(self.0.take());
        }
    }
    Scope(RESULTS.replace(Some(Results::default())))
}

fn result<R>(
    root: &Clause,
    environment: &Facts,
    read: impl FnOnce(Option<&RequiredFacts>) -> R,
) -> Option<R> {
    RESULTS.with_borrow(|active| {
        let cache = active.as_ref()?;
        let entry = unsafe { lookup(cache, root.raw(), environment, ENTRY_STRIDE).as_ref()? };
        let value = (entry.possible != 0).then(|| unsafe { entry.value.assume_init_ref() });
        Some(read(value))
    })
}

fn remember(
    root: &Clause,
    environment: &Facts,
    value: impl FnOnce() -> Option<RequiredFacts>,
) {
    RESULTS.with_borrow_mut(|active| {
        if let Some(cache) = active {
            let value = value();
            let entry = unsafe { &mut *reserve(cache, root.raw(), environment, ENTRY_STRIDE, drop_result) };
            if let Some(value) = value {
                entry.value.write(value);
                entry.possible = 1;
            }
        }
    });
}

pub(super) fn possible(
    root: &Clause,
    environment: &Facts,
    evaluate: impl FnOnce() -> Option<RequiredFacts>,
) -> bool {
    if let Some(known) = result(root, environment, |known| known.is_some()) {
        return known;
    }
    let value = evaluate();
    let possible = value.is_some();
    remember(root, environment, || value);
    possible
}

pub(super) fn required(
    root: &Clause,
    environment: &Facts,
    evaluate: impl FnOnce() -> Option<RequiredFacts>,
) -> Option<RequiredFacts> {
    if let Some(known) = result(root, environment, |known| known.cloned()) {
        return known;
    }
    let value = evaluate();
    remember(root, environment, || value.clone());
    value
}
