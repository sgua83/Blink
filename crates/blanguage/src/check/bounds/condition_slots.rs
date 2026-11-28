use super::{Clause, Condition, SharedCondition, StoredClause, combination};
use std::{ffi::c_void, ops::Deref, ptr::NonNull, slice};

/// The B buffer owns the strong reference in each present slot.
#[repr(C)]
pub(super) struct ConditionSlots {
    records: *mut Option<SharedCondition>,
    root: usize,
    count: usize,
    capacity: usize,
}

unsafe impl Send for ConditionSlots {}
unsafe impl Sync for ConditionSlots {}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_slots::condition_slots_push"]
    fn push(owner: &mut ConditionSlots, payload: *mut c_void);
    #[link_name = "bootstrap.condition_slots::condition_slots_resize"]
    fn resize(owner: &mut ConditionSlots, width: usize);
    #[link_name = "bootstrap.condition_slots::condition_slots_clear"]
    fn clear(owner: &mut ConditionSlots);
    #[link_name = "bootstrap.condition_slots::condition_slots_copy"]
    fn copy(original: &ConditionSlots, output: &mut ConditionSlots);
    #[link_name = "bootstrap.condition_slots::condition_slots_set"]
    fn set(owner: &mut ConditionSlots, index: usize, payload: *mut c_void);
    #[link_name = "bootstrap.condition_slots::condition_slots_take"]
    fn take(owner: &mut ConditionSlots, index: usize) -> *mut StoredClause;
    #[link_name = "bootstrap.condition_slots::condition_slots_conjoin"]
    fn conjoin(owner: &mut ConditionSlots, index: usize, no: *mut c_void, yes: *mut c_void,
               conflict: unsafe extern "C" fn(*const c_void) -> u32);
    #[link_name = "bootstrap.condition_slots::condition_slots_deny"]
    fn deny(owner: &mut ConditionSlots, index: usize, branch: u32);
}

impl ConditionSlots {
    pub(super) const fn new() -> Self {
        Self { records: std::ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(super) fn resize(&mut self, width: usize) {
        unsafe { resize(self, width) }
    }

    pub(super) fn push(&mut self, condition: Option<SharedCondition>) {
        let payload = condition.map_or(std::ptr::null_mut(), SharedCondition::into_raw);
        unsafe { push(self, payload) }
    }

    pub(super) fn set(&mut self, index: usize, condition: Option<SharedCondition>) {
        assert!(index < self.count);
        let payload = condition.map_or(std::ptr::null_mut(), SharedCondition::into_raw);
        unsafe { set(self, index, payload) }
    }

    pub(super) fn take(&mut self, index: usize) -> Option<SharedCondition> {
        if index >= self.count { return None; }
        NonNull::new(unsafe { take(self, index) }).map(SharedCondition)
    }

    pub(super) fn merge(&mut self, index: usize, condition: &Condition) {
        self.conjoin(index, &condition.cases[0], &condition.cases[1]);
    }

    pub(super) fn constrain(&mut self, index: usize, path: &Clause) {
        self.conjoin(index, path, path);
    }

    fn conjoin(&mut self, index: usize, no: &Clause, yes: &Clause) {
        assert!(index < self.count);
        unsafe { conjoin(self, index, no.raw(), yes.raw(), combination::conflict) }
    }

    pub(super) fn deny(&mut self, index: usize, branch: u32) {
        assert!(index < self.count && branch < 2);
        unsafe { deny(self, index, branch) }
    }
}

impl Default for ConditionSlots {
    fn default() -> Self { Self::new() }
}

impl Drop for ConditionSlots {
    fn drop(&mut self) { unsafe { clear(self) }; }
}

impl Clone for ConditionSlots {
    fn clone(&self) -> Self {
        let mut copy_slots = Self::new();
        unsafe { copy(self, &mut copy_slots) };
        copy_slots
    }
}

impl Deref for ConditionSlots {
    type Target = [Option<SharedCondition>];

    fn deref(&self) -> &Self::Target {
        if self.count == 0 { return &[]; }
        // Option<SharedCondition> uses the nullable native payload pointer.
        unsafe { slice::from_raw_parts(self.records, self.count) }
    }
}

impl PartialEq for ConditionSlots {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| match (a, b) {
            (Some(a), Some(b)) => a.0 == b.0 || **a == **b,
            (None, None) => true,
            _ => false,
        })
    }
}
impl Eq for ConditionSlots {}

impl Extend<Option<SharedCondition>> for ConditionSlots {
    fn extend<I: IntoIterator<Item = Option<SharedCondition>>>(&mut self, source: I) {
        for condition in source { self.push(condition); }
    }
}
impl FromIterator<Option<SharedCondition>> for ConditionSlots {
    fn from_iter<I: IntoIterator<Item = Option<SharedCondition>>>(source: I) -> Self {
        let mut result = Self::new();
        result.extend(source);
        result
    }
}
impl<const N: usize> From<[Option<SharedCondition>; N]> for ConditionSlots {
    fn from(source: [Option<SharedCondition>; N]) -> Self { source.into_iter().collect() }
}

pub(super) struct IntoIter {
    slots: ConditionSlots,
    index: usize,
}
impl Iterator for IntoIter {
    type Item = Option<SharedCondition>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.slots.len() { return None; }
        let item = self.slots.take(self.index);
        self.index += 1;
        Some(item)
    }
}

impl IntoIterator for ConditionSlots {
    type Item = Option<SharedCondition>;
    type IntoIter = IntoIter;
    fn into_iter(self) -> IntoIter { IntoIter { slots: self, index: 0 } }
}
