use super::{OriginRows, Reg, ValueIdentity};
use std::{ffi::c_void, ptr};

#[repr(C)]
pub(in crate::check) struct OutputBindings {
    entries: *mut c_void,
    root: usize,
    count: usize,
    capacity: usize,
}

// B mutates the allocation through the unique owner. Queries borrow it immutably.
unsafe impl Send for OutputBindings {}
unsafe impl Sync for OutputBindings {}

#[repr(C)]
struct Entry {
    source: *const ValueIdentity,
    target: *const ValueIdentity,
}

unsafe extern "C" {
    #[link_name = "bootstrap.output_bindings::copy_output_bindings"]
    fn copy(original: &OutputBindings, output: &mut OutputBindings);
    #[link_name = "bootstrap.output_bindings::same_output_bindings"]
    fn same(left: &OutputBindings, right: &OutputBindings) -> u32;
    #[link_name = "bootstrap.output_bindings::write_output_binding"]
    fn insert(owner: &mut OutputBindings, source: &ValueIdentity, target: &ValueIdentity);
    #[link_name = "bootstrap.output_bindings::collect_output_bindings"]
    fn collect(owner: &mut OutputBindings, origins: &OriginRows, registers: *const Reg, count: usize, entry: usize);
    #[link_name = "bootstrap.output_bindings::find_output_binding"]
    fn find(owner: &OutputBindings, source: &ValueIdentity) -> *const ValueIdentity;
    #[link_name = "bootstrap.output_bindings::next_output_binding"]
    fn next(owner: &OutputBindings, cursor: &mut usize) -> Entry;
}

impl OutputBindings {
    pub(in crate::check) const fn new() -> Self {
        Self { entries: ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(in crate::check) fn is_empty(&self) -> bool { self.count == 0 }

    pub(in crate::check) fn get(&self, source: &ValueIdentity) -> Option<&ValueIdentity> {
        unsafe { find(self, source).as_ref() }
    }

    pub(in crate::check) fn insert(&mut self, source: ValueIdentity, target: ValueIdentity) {
        unsafe { insert(self, &source, &target); }
    }

    pub(super) fn bind_origins(&mut self, origins: &OriginRows, registers: &[Reg], entry: usize) {
        unsafe { collect(self, origins, registers.as_ptr(), registers.len(), entry); }
    }

    pub(in crate::check) fn iter(&self) -> Entries<'_> {
        Entries { owner: self, remaining: self.count, cursor: 0 }
    }

    pub(in crate::check) fn keys(&self) -> impl Iterator<Item = &ValueIdentity> {
        self.iter().map(|(source, _)| source)
    }

    pub(in crate::check) fn values(&self) -> impl Iterator<Item = &ValueIdentity> {
        self.iter().map(|(_, target)| target)
    }
}

pub(in crate::check) struct Entries<'a> {
    owner: &'a OutputBindings,
    remaining: usize,
    cursor: usize,
}

impl<'a> Iterator for Entries<'a> {
    type Item = (&'a ValueIdentity, &'a ValueIdentity);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        let pair = unsafe { next(self.owner, &mut self.cursor) };
        Some(unsafe { (&*pair.source, &*pair.target) })
    }
}

impl Clone for OutputBindings {
    fn clone(&self) -> Self {
        let mut result = Self::new();
        unsafe { copy(self, &mut result) };
        result
    }
}

impl PartialEq for OutputBindings {
    fn eq(&self, other: &Self) -> bool {
        unsafe { same(self, other) != 0 }
    }
}

impl Eq for OutputBindings {}

impl Default for OutputBindings {
    fn default() -> Self { Self::new() }
}

impl Drop for OutputBindings {
    fn drop(&mut self) { unsafe { super::facts::clear_tree(ptr::from_mut(self).cast()); } }
}

impl FromIterator<(ValueIdentity, ValueIdentity)> for OutputBindings {
    fn from_iter<T: IntoIterator<Item = (ValueIdentity, ValueIdentity)>>(items: T) -> Self {
        let mut output = Self::new();
        for (source, target) in items { output.insert(source, target); }
        output
    }
}

impl<const N: usize> From<[(ValueIdentity, ValueIdentity); N]> for OutputBindings {
    fn from(items: [(ValueIdentity, ValueIdentity); N]) -> Self { items.into_iter().collect() }
}
