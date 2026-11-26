//! The B catalog holds the keys, ordering, and owning locations of native values.
use crate::{frontend::{Definition, NameResolution}, names::Bytes};
use std::{ffi::c_void, marker::PhantomData, ops::{Deref, Index}, sync::Arc};

type Address = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct Catalog {
    slots: Address,
    capacity: u64,
    count: u64,
    ordered: Address,
}

#[repr(C)]
struct Entry {
    name: Bytes,
    value: Address,
}

#[repr(C)]
struct Insertion {
    replaced: Address,
    success: u32,
}

#[repr(C)]
struct Resolution {
    name: Bytes,
    count: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.declarations::insert"]
    fn insert(catalog: &mut Catalog, name: Bytes, value: Address) -> Insertion;
    #[link_name = "bootstrap.declarations::find"]
    fn find(catalog: &Catalog, name: Bytes) -> Address;
    #[link_name = "bootstrap.declarations::entry"]
    fn entry(node: Address) -> Entry;
    #[link_name = "bootstrap.declarations::seal"]
    fn seal(catalog: &mut Catalog) -> bool;
    #[link_name = "bootstrap.declarations::ordered_entry"]
    fn ordered_entry(ordered: Address, index: u64) -> Entry;
    #[link_name = "bootstrap.declarations::release"]
    fn release(catalog: &Catalog, drop_value: unsafe extern "C" fn(Address));
    #[link_name = "bootstrap.declarations::resolve"]
    fn resolve(catalog: &Catalog, name: Bytes, module: Bytes, imports: *const c_void, count: u64,
        import_name: unsafe extern "C" fn(*const c_void, u64) -> Bytes,
        visible: unsafe extern "C" fn(Address) -> u32) -> Resolution;
}

pub(crate) struct Directory<T> {
    catalog: Catalog,
    values: PhantomData<Box<T>>,
}

// The borrowed rows and values follow the same shared/exclusive ownership as T.
unsafe impl<T: Send> Send for Directory<T> {}
unsafe impl<T: Sync> Sync for Directory<T> {}

impl<T> Default for Directory<T> {
    fn default() -> Self {
        Self { catalog: Catalog::default(), values: PhantomData }
    }
}

unsafe extern "C" fn drop_value<T>(value: Address) {
    drop(unsafe { Box::from_raw(value.cast::<T>()) });
}

impl<T> Drop for Directory<T> {
    fn drop(&mut self) {
        unsafe { release(&self.catalog, drop_value::<T>) };
    }
}

impl<T> Directory<T> {
    pub(crate) fn insert(&mut self, name: &str, value: T) {
        let value = Box::into_raw(Box::new(value)).cast();
        let inserted = unsafe { insert(&mut self.catalog, Bytes::new(name), value) };
        if inserted.success == 0 { std::process::abort(); }
        if !inserted.replaced.is_null() {
            unsafe { drop_value::<T>(inserted.replaced) };
        }
    }

    pub(crate) fn contains_key(&self, name: &str) -> bool {
        !unsafe { find(&self.catalog, Bytes::new(name)) }.is_null()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&T> {
        self.get_key_value(name).map(|(_, value)| value)
    }

    pub(crate) fn get_key_value(&self, name: &str) -> Option<(&str, &T)> {
        let node = unsafe { find(&self.catalog, Bytes::new(name)) };
        if node.is_null() { return None; }
        let row = unsafe { entry(node) };
        Some(unsafe { (row.name.text(), &*row.value.cast::<T>()) })
    }

    pub(crate) fn publish(mut self) -> OrderedDirectory<T> {
        if !unsafe { seal(&mut self.catalog) } { std::process::abort(); }
        OrderedDirectory(self)
    }
}

pub(crate) struct OrderedDirectory<T>(Directory<T>);

impl<T> Deref for OrderedDirectory<T> {
    type Target = Directory<T>;

    fn deref(&self) -> &Directory<T> { &self.0 }
}

impl<T> OrderedDirectory<T> {
    pub(crate) fn len(&self) -> usize { self.catalog.count as usize }

    pub(crate) fn iter(&self) -> Iter<'_, T> {
        Iter { ordered: self.catalog.ordered, indices: 0..self.catalog.count, values: PhantomData }
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(name, _)| name)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, value)| value)
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let ordered = self.catalog.ordered;
        (0..self.catalog.count).map(move |index| {
            let row = unsafe { ordered_entry(ordered, index) };
            unsafe { &mut *row.value.cast::<T>() }
        })
    }
}

impl<T, K: AsRef<str> + ?Sized> Index<&K> for Directory<T> {
    type Output = T;

    fn index(&self, name: &K) -> &T {
        self.get(name.as_ref()).expect("no entry found for key")
    }
}

pub(crate) struct Iter<'a, T> {
    ordered: Address,
    indices: std::ops::Range<u64>,
    values: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (&'a str, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let row = unsafe { ordered_entry(self.ordered, self.indices.next()?) };
        Some(unsafe { (row.name.text(), &*row.value.cast::<T>()) })
    }
}

impl<'a, T> IntoIterator for &'a OrderedDirectory<T> {
    type Item = (&'a str, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

pub(crate) trait Visibility {
    fn is_public(&self) -> bool;
}

impl Visibility for Arc<Definition> {
    fn is_public(&self) -> bool { self.public }
}

impl<T> Visibility for (T, crate::ast::Source, String, Arc<[String]>) {
    fn is_public(&self) -> bool { self.1.path.extension().is_some_and(|extension| extension == "h") }
}

unsafe extern "C" fn import_name(imports: *const c_void, index: u64) -> Bytes {
    Bytes::new(unsafe { &*imports.cast::<String>().add(index as usize) })
}

unsafe extern "C" fn visible<T: Visibility>(value: Address) -> u32 {
    u32::from(unsafe { &*value.cast::<T>() }.is_public())
}

impl<T: Visibility> Directory<T> {
    pub(crate) fn resolve(&self, name: &str, module: &str, imports: &[String]) -> NameResolution {
        let result = unsafe {
            resolve(&self.catalog, Bytes::new(name), Bytes::new(module), imports.as_ptr().cast(),
                imports.len() as u64, import_name, visible::<T>)
        };
        match result.count {
            0 => NameResolution::Missing,
            1 => NameResolution::Unique(unsafe { result.name.text() }.to_owned()),
            _ => NameResolution::Ambiguous,
        }
    }
}
