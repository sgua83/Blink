use super::IndexDomain;
use crate::codec::{ReadStatus, contracts::{self, Native, Slot}};
use std::{cmp::Ordering, iter::FusedIterator, ptr};

// B allocates the rows and owns their order. Keys have the original
// IndexDomain layout, and a borrowed cursor keeps the allocation stable.
#[repr(C)]
pub(in crate::check) struct Domains {
    records: Slot,
    root: usize,
    count: usize,
    capacity: usize,
}

unsafe impl Send for Domains {}
unsafe impl Sync for Domains {}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Cursor {
    front: usize,
    back: usize,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum MergeOperation { Union, Intersection }

unsafe extern "C" {
    #[link_name = "bootstrap.proof_facts::domain_tree_copy"]
    fn copy(original: &Domains, output: &mut Domains);
    #[link_name = "bootstrap.proof_facts::domain_tree_insert"]
    fn insert(tree: &mut Domains, key: &IndexDomain) -> u32;
    #[link_name = "bootstrap.proof_facts::domain_tree_contains"]
    fn contains(tree: &Domains, key: &IndexDomain) -> u32;
    #[link_name = "bootstrap.proof_facts::domain_tree_contains_range"]
    fn contains_range(tree: &Domains, start: &IndexDomain, end: &IndexDomain) -> u32;
    #[link_name = "bootstrap.proof_facts::domain_tree_append"]
    fn append(tree: &mut Domains, incoming: &mut Domains);
    #[link_name = "bootstrap.proof_facts::domain_tree_retain"]
    fn retain(tree: &mut Domains, context: Slot, keep: unsafe extern "C" fn(Slot, &IndexDomain) -> u32);
    #[link_name = "bootstrap.proof_facts::domain_tree_cursor"]
    fn cursor(tree: &Domains) -> Cursor;
    #[link_name = "bootstrap.proof_facts::domain_tree_next"]
    fn next(tree: &Domains, cursor: &mut Cursor, reverse: u32) -> *const IndexDomain;
    #[link_name = "bootstrap.proof_facts::domain_tree_merge"]
    fn merge(left: &Domains, right: &Domains, left_position: &mut Cursor, right_position: &mut Cursor,
             operation: MergeOperation) -> *const IndexDomain;
    #[link_name = "bootstrap.proof_facts::domain_tree_compare"]
    fn compare(left: &Domains, right: &Domains) -> i32;
}

impl Domains {
    pub(in crate::check) const fn new() -> Self {
        Self { records: ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(in crate::check) fn len(&self) -> usize { self.count }

    pub(in crate::check) fn is_empty(&self) -> bool { self.count == 0 }

    pub(in crate::check) fn clear(&mut self) {
        unsafe { super::facts::clear_tree(contracts::slot(self)) }
    }

    pub(in crate::check) fn insert(&mut self, domain: IndexDomain) -> bool {
        unsafe { insert(self, &domain) != 0 }
    }

    pub(in crate::check) fn contains(&self, domain: &IndexDomain) -> bool {
        unsafe { contains(self, domain) != 0 }
    }

    pub(super) fn contains_range(&self, start: &IndexDomain, end: &IndexDomain) -> bool {
        unsafe { contains_range(self, start, end) != 0 }
    }

    pub(in crate::check) fn append(&mut self, other: &mut Self) {
        unsafe { append(self, other) }
    }

    pub(in crate::check) fn retain<F: FnMut(&IndexDomain) -> bool>(&mut self, mut keep: F) {
        unsafe extern "C" fn accept<F: FnMut(&IndexDomain) -> bool>(context: Slot, domain: &IndexDomain) -> u32 {
            let keep = unsafe { &mut *context.cast::<F>() };
            u32::from(keep(domain))
        }
        unsafe { retain(self, contracts::slot(&mut keep), accept::<F>) }
    }

    pub(in crate::check) fn iter(&self) -> Iter<'_> {
        Iter { tree: Some(self), position: unsafe { cursor(self) } }
    }

    pub(in crate::check) fn union<'a>(&'a self, other: &'a Self) -> Merge<'a> {
        Merge::new(self, other, MergeOperation::Union)
    }

    pub(in crate::check) fn intersection<'a>(&'a self, other: &'a Self) -> Merge<'a> {
        Merge::new(self, other, MergeOperation::Intersection)
    }
}

impl Default for Domains {
    fn default() -> Self { Self::new() }
}

impl Drop for Domains {
    fn drop(&mut self) { self.clear(); }
}

impl Clone for Domains {
    fn clone(&self) -> Self {
        let mut result = Self::new();
        unsafe { copy(self, &mut result) };
        result
    }
}

impl PartialEq for Domains {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.cmp(other).is_eq()
    }
}
impl Eq for Domains {}
impl PartialOrd for Domains {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Domains {
    fn cmp(&self, other: &Self) -> Ordering {
        unsafe { compare(self, other).cmp(&0) }
    }
}

impl Extend<IndexDomain> for Domains {
    fn extend<I: IntoIterator<Item = IndexDomain>>(&mut self, values: I) {
        for value in values { self.insert(value); }
    }
}

impl FromIterator<IndexDomain> for Domains {
    fn from_iter<I: IntoIterator<Item = IndexDomain>>(values: I) -> Self {
        let mut result = Self::new();
        result.extend(values);
        result
    }
}

impl<const N: usize> From<[IndexDomain; N]> for Domains {
    fn from(values: [IndexDomain; N]) -> Self {
        values.into_iter().collect()
    }
}

impl Native for Domains {
    fn empty() -> Self { Self::new() }

    unsafe fn write(&self, output: Slot) {
        unsafe { contracts::write_iter(self.iter(), self.len(), output) }
    }

    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe { contracts::read_elements(context, &mut |domain| { output.insert(domain); }) }
    }
}

#[derive(Clone, Default)]
pub(in crate::check) struct Iter<'a> {
    tree: Option<&'a Domains>,
    position: Cursor,
}
impl<'a> Iterator for Iter<'a> {
    type Item = &'a IndexDomain;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe { next(self.tree?, &mut self.position, 0).as_ref() }
    }
}
impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        unsafe { next(self.tree?, &mut self.position, 1).as_ref() }
    }
}
impl FusedIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a Domains {
    type Item = &'a IndexDomain;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

pub(in crate::check) struct IntoIter {
    tree: Domains,
    position: Cursor,
}
impl Iterator for IntoIter {
    type Item = IndexDomain;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe { next(&self.tree, &mut self.position, 0).as_ref().copied() }
    }
}
impl IntoIterator for Domains {
    type Item = IndexDomain;
    type IntoIter = IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        let position = unsafe { cursor(&self) };
        IntoIter { tree: self, position }
    }
}

pub(in crate::check) struct Merge<'a> {
    left: &'a Domains,
    right: &'a Domains,
    positions: [Cursor; 2],
    operation: MergeOperation,
}
impl<'a> Merge<'a> {
    fn new(left: &'a Domains, right: &'a Domains, operation: MergeOperation) -> Self {
        Self { left, right, positions: unsafe { [cursor(left), cursor(right)] }, operation }
    }
}
impl<'a> Iterator for Merge<'a> {
    type Item = &'a IndexDomain;
    fn next(&mut self) -> Option<Self::Item> {
        let [left, right] = &mut self.positions;
        unsafe { merge(self.left, self.right, left, right, self.operation).as_ref() }
    }
}
