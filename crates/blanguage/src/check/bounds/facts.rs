use super::Fact;
use std::{
    cmp::Ordering,
    ffi::c_void,
    iter::FusedIterator,
    ops::{Bound, RangeBounds},
    ptr,
};

type Borrowed = *const c_void;
type Slot = *mut c_void;
type Keep = unsafe extern "C" fn(Slot, Borrowed) -> u32;

/// The node allocation and all order operations belong to the B fact tree.
/// The original C-layout fact is stored and compared in place.
#[repr(C)]
pub(super) struct Facts {
    records: Slot,
    root: usize,
    count: usize,
    capacity: usize,
}

// B only mutates the store under an exclusive borrow. Its payloads are Copy
// and share the original Fact type's lifetimes; neither addresses nor cursors
// are persisted in the store.
unsafe impl Send for Facts {}
unsafe impl Sync for Facts {}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Cursor {
    front: usize,
    back: usize,
}

#[repr(C)]
struct KeyBound {
    key: Borrowed,
    exclusive: u32,
}

impl From<Bound<&Fact>> for KeyBound {
    fn from(value: Bound<&Fact>) -> Self {
        match value {
            Bound::Included(key) => Self { key: ptr::from_ref(key).cast(), exclusive: 0 },
            Bound::Excluded(key) => Self { key: ptr::from_ref(key).cast(), exclusive: 1 },
            Bound::Unbounded => Self { key: ptr::null(), exclusive: 0 },
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum MergeOperation {
    Union,
    Difference = 2,
}

unsafe extern "C" {
    #[link_name = "bootstrap.proof_facts::fact_tree_clear"]
    pub(super) fn clear_tree(tree: Slot);
    #[link_name = "bootstrap.proof_facts::fact_tree_copy"]
    fn copy(original: &Facts, output: &mut Facts);
    #[link_name = "bootstrap.proof_facts::fact_tree_insert"]
    fn insert(tree: &mut Facts, key: Borrowed) -> u32;
    #[link_name = "bootstrap.proof_facts::fact_tree_remove"]
    fn remove(tree: &mut Facts, key: Borrowed) -> u32;
    #[link_name = "bootstrap.proof_facts::fact_tree_contains"]
    fn contains(tree: &Facts, key: Borrowed) -> u32;
    #[link_name = "bootstrap.proof_facts::fact_tree_retain"]
    fn retain(tree: &mut Facts, context: Slot, keep: Keep);
    #[link_name = "bootstrap.proof_facts::fact_tree_cursor"]
    fn cursor(tree: &Facts) -> Cursor;
    #[link_name = "bootstrap.proof_facts::fact_tree_range"]
    fn range(tree: &Facts, start: KeyBound, end: KeyBound) -> Cursor;
    #[link_name = "bootstrap.proof_facts::fact_tree_next"]
    fn next(tree: &Facts, cursor: &mut Cursor, reverse: u32) -> Borrowed;
    #[link_name = "bootstrap.proof_facts::fact_tree_merge"]
    fn merge(left: &Facts, right: &Facts, cursors: &mut [Cursor; 2],
        operation: MergeOperation) -> Borrowed;
    #[link_name = "bootstrap.proof_facts::fact_tree_collect_merge"]
    fn collect_merge(left: &Facts, right: &Facts, cursors: &mut [Cursor; 2],
        operation: MergeOperation, output: &mut Facts);
    #[link_name = "bootstrap.proof_facts::fact_tree_compare"]
    fn ordered(left: &Facts, right: &Facts) -> i32;
    #[link_name = "bootstrap.proof_facts::fact_tree_relation"]
    fn related(left: &Facts, right: &Facts, subset: u32) -> u32;
}

unsafe extern "C" fn keep<F: FnMut(&Fact) -> bool>(context: Slot, key: Borrowed) -> u32 {
    let predicate = unsafe { &mut *context.cast::<F>() };
    u32::from(predicate(unsafe { &*key.cast::<Fact>() }))
}

impl Facts {
    pub(super) const fn new() -> Self {
        Self { records: ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(super) fn len(&self) -> usize {
        self.count
    }

    pub(super) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(super) fn clear(&mut self) {
        unsafe { clear_tree(ptr::from_mut(self).cast()) }
    }

    pub(super) fn insert(&mut self, key: Fact) -> bool {
        unsafe { insert(self, ptr::from_ref(&key).cast()) != 0 }
    }

    pub(super) fn remove(&mut self, key: &Fact) -> bool {
        unsafe { remove(self, ptr::from_ref(key).cast()) != 0 }
    }

    pub(super) fn contains(&self, key: &Fact) -> bool {
        unsafe { contains(self, ptr::from_ref(key).cast()) != 0 }
    }

    pub(super) fn retain<F: FnMut(&Fact) -> bool>(&mut self, mut predicate: F) {
        unsafe { retain(self, ptr::from_mut(&mut predicate).cast(), keep::<F>) }
    }

    pub(super) fn iter(&self) -> Iter<'_> {
        Iter { tree: Some(self), cursor: unsafe { cursor(self) } }
    }

    pub(super) fn range(&self, bounds: impl RangeBounds<Fact>) -> Iter<'_> {
        let start = bounds.start_bound().into();
        let end = bounds.end_bound().into();
        Iter { tree: Some(self), cursor: unsafe { range(self, start, end) } }
    }

    pub(super) fn union<'a>(&'a self, other: &'a Self) -> Merge<'a> {
        Merge::new(self, other, MergeOperation::Union)
    }

    pub(super) fn difference<'a>(&'a self, other: &'a Self) -> Merge<'a> {
        Merge::new(self, other, MergeOperation::Difference)
    }

    pub(super) fn is_subset(&self, other: &Self) -> bool {
        unsafe { related(self, other, 1) != 0 }
    }

    pub(super) fn is_disjoint(&self, other: &Self) -> bool {
        unsafe { related(self, other, 0) != 0 }
    }
}

impl Default for Facts {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Facts {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Clone for Facts {
    fn clone(&self) -> Self {
        let mut result = Self::new();
        unsafe { copy(self, &mut result) }
        result
    }
}

impl PartialEq for Facts {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.cmp(other).is_eq()
    }
}

impl Eq for Facts {}

impl PartialOrd for Facts {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Facts {
    fn cmp(&self, other: &Self) -> Ordering {
        unsafe { ordered(self, other).cmp(&0) }
    }
}

impl Extend<Fact> for Facts {
    fn extend<I: IntoIterator<Item = Fact>>(&mut self, incoming: I) {
        for key in incoming {
            self.insert(key);
        }
    }
}

impl FromIterator<Fact> for Facts {
    fn from_iter<I: IntoIterator<Item = Fact>>(incoming: I) -> Self {
        let mut output = Self::new();
        output.extend(incoming);
        output
    }
}

impl<const N: usize> From<[Fact; N]> for Facts {
    fn from(keys: [Fact; N]) -> Self {
        keys.into_iter().collect()
    }
}

#[derive(Clone, Default)]
pub(super) struct Iter<'a> {
    tree: Option<&'a Facts>,
    cursor: Cursor,
}

impl<'a> Iter<'a> {
    pub(super) fn is_empty(&self) -> bool {
        self.cursor.front == 0
    }

    fn step(&mut self, reverse: bool) -> Option<&'a Fact> {
        let tree = self.tree?;
        // The borrow prevents relocation while a cursor or yielded fact lives.
        unsafe { next(tree, &mut self.cursor, u32::from(reverse)).cast::<Fact>().as_ref() }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Fact;
    fn next(&mut self) -> Option<Self::Item> {
        self.step(false)
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.step(true)
    }
}

impl FusedIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a Facts {
    type Item = &'a Fact;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(super) struct IntoIter {
    tree: Facts,
    cursor: Cursor,
}

impl Iterator for IntoIter {
    type Item = Fact;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe { next(&self.tree, &mut self.cursor, 0).cast::<Fact>().as_ref().copied() }
    }
}

impl IntoIterator for Facts {
    type Item = Fact;
    type IntoIter = IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        let cursor = unsafe { cursor(&self) };
        IntoIter { tree: self, cursor }
    }
}

pub(super) struct Merge<'a> {
    left: &'a Facts,
    right: &'a Facts,
    positions: [Cursor; 2],
    operation: MergeOperation,
}

impl<'a> Merge<'a> {
    fn new(left: &'a Facts, right: &'a Facts, operation: MergeOperation) -> Self {
        let positions = unsafe { [cursor(left), cursor(right)] };
        Self { left, right, positions, operation }
    }

    pub(super) fn into_facts(mut self) -> Facts {
        let mut output = Facts::new();
        unsafe { collect_merge(self.left, self.right, &mut self.positions, self.operation, &mut output) };
        output
    }
}

impl<'a> Iterator for Merge<'a> {
    type Item = &'a Fact;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe { merge(self.left, self.right, &mut self.positions, self.operation)
            .cast::<Fact>().as_ref() }
    }
}
