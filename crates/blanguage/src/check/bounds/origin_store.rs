use super::ValueIdentity;
use crate::codec::{ReadStatus, contracts::{self, Native, Slot}};
use std::{ffi::c_void, mem::{self, ManuallyDrop}, ops::{Deref, DerefMut}, ptr, slice};

/// The native list preserves source order, including the primary origin and repetitions.
#[repr(C)]
pub(in crate::check) struct OriginList {
    records: *mut ValueIdentity,
    root: usize,
    count: usize,
    capacity: usize,
}

/// Each initialized row owns its B origin list. Moving a row transfers that allocation.
#[repr(C)]
pub(in crate::check) struct OriginRows {
    records: *mut OriginList,
    root: usize,
    count: usize,
    capacity: usize,
}

unsafe impl Send for OriginList {}
unsafe impl Sync for OriginList {}
unsafe impl Send for OriginRows {}
unsafe impl Sync for OriginRows {}

unsafe extern "C" {
    #[link_name = "bootstrap.proof_origins::origin_list_insert"]
    fn insert(list: &mut OriginList, index: usize, identity: &ValueIdentity);
    #[link_name = "bootstrap.proof_origins::origin_list_copy"]
    fn copy_list(source: &OriginList, output: &mut OriginList);
    #[link_name = "bootstrap.proof_origins::origin_list_truncate"]
    fn truncate(list: &mut OriginList, length: usize);
    #[link_name = "bootstrap.proof_origins::origin_list_retain"]
    fn retain(list: &mut OriginList, context: *mut c_void,
              accept: unsafe extern "C" fn(*mut c_void, &ValueIdentity) -> u32);
    #[link_name = "bootstrap.proof_origins::origin_rows_push"]
    fn push_row(rows: &mut OriginRows, list: &mut OriginList);
    #[link_name = "bootstrap.proof_origins::origin_rows_resize"]
    fn resize_rows(rows: &mut OriginRows, length: usize);
    #[link_name = "bootstrap.proof_origins::origin_rows_copy"]
    fn copy_rows(source: &OriginRows, output: &mut OriginRows);
    #[link_name = "bootstrap.proof_origins::origin_rows_clear"]
    fn clear_rows(rows: &mut OriginRows);
}

impl OriginList {
    pub(in crate::check) const fn new() -> Self {
        Self { records: ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(in crate::check) fn as_slice(&self) -> &[ValueIdentity] { self }

    pub(in crate::check) fn push(&mut self, value: ValueIdentity) {
        unsafe { insert(self, self.count, &value) }
    }

    pub(in crate::check) fn insert(&mut self, index: usize, value: ValueIdentity) {
        assert!(index <= self.count);
        unsafe { insert(self, index, &value) }
    }

    pub(in crate::check) fn truncate(&mut self, length: usize) {
        unsafe { truncate(self, length) }
    }

    pub(in crate::check) fn clear(&mut self) {
        self.truncate(0);
    }

    pub(in crate::check) fn retain<F: FnMut(&ValueIdentity) -> bool>(&mut self, mut keep: F) {
        unsafe extern "C" fn accept<F: FnMut(&ValueIdentity) -> bool>(
            context: *mut c_void, identity: &ValueIdentity,
        ) -> u32 {
            let keep = unsafe { &mut *context.cast::<F>() };
            u32::from(keep(identity))
        }
        unsafe { retain(self, ptr::from_mut(&mut keep).cast(), accept::<F>); }
    }
}

impl OriginRows {
    pub(in crate::check) const fn new() -> Self {
        Self { records: ptr::null_mut(), root: 0, count: 0, capacity: 0 }
    }

    pub(in crate::check) fn resize(&mut self, length: usize) {
        unsafe { resize_rows(self, length) }
    }

    pub(in crate::check) fn push(&mut self, list: OriginList) {
        let mut moving = ManuallyDrop::new(list);
        unsafe { push_row(self, &mut moving); }
    }
}

macro_rules! owned_sequence {
    ($table:ty, $element:ty, $copy:ident, $drop:expr) => {
        impl Default for $table {
            fn default() -> Self { Self::new() }
        }
        impl Clone for $table {
            fn clone(&self) -> Self {
                let mut output = Self::new();
                unsafe { $copy(self, &mut output); }
                output
            }
        }
        impl Drop for $table {
            fn drop(&mut self) { ($drop)(self) }
        }
        impl Deref for $table {
            type Target = [$element];
            fn deref(&self) -> &Self::Target {
                if self.count == 0 { return &[]; }
                unsafe { slice::from_raw_parts(self.records, self.count) }
            }
        }
        impl DerefMut for $table {
            fn deref_mut(&mut self) -> &mut Self::Target {
                if self.count == 0 { return &mut []; }
                unsafe { slice::from_raw_parts_mut(self.records, self.count) }
            }
        }
        impl PartialEq for $table {
            fn eq(&self, other: &Self) -> bool { **self == **other }
        }
        impl Eq for $table {}
        impl Extend<$element> for $table {
            fn extend<I: IntoIterator<Item = $element>>(&mut self, values: I) {
                for value in values { self.push(value); }
            }
        }
        impl FromIterator<$element> for $table {
            fn from_iter<I: IntoIterator<Item = $element>>(values: I) -> Self {
                let mut result = Self::new();
                result.extend(values);
                result
            }
        }
        impl<const N: usize> From<[$element; N]> for $table {
            fn from(values: [$element; N]) -> Self { values.into_iter().collect() }
        }
        impl<'a> IntoIterator for &'a $table {
            type Item = &'a $element;
            type IntoIter = slice::Iter<'a, $element>;
            fn into_iter(self) -> Self::IntoIter { self.iter() }
        }
        impl<'a> IntoIterator for &'a mut $table {
            type Item = &'a mut $element;
            type IntoIter = slice::IterMut<'a, $element>;
            fn into_iter(self) -> Self::IntoIter { self.iter_mut() }
        }
        impl Native for $table {
            fn empty() -> Self { Self::new() }
            unsafe fn write(&self, output: Slot) {
                unsafe { contracts::write_slice(self, output) }
            }
            unsafe fn read(input: Slot, output: &mut Self) -> ReadStatus {
                unsafe { contracts::read_elements(input, &mut |value| output.push(value)) }
            }
        }
    };
}

owned_sequence!(OriginList, ValueIdentity, copy_list, |list: &mut OriginList| unsafe {
    super::facts::clear_tree(contracts::slot(list));
});
owned_sequence!(OriginRows, OriginList, copy_rows, |rows: &mut OriginRows| unsafe {
    clear_rows(rows);
});

pub(in crate::check) struct OriginValues {
    list: OriginList,
    index: usize,
}
impl Iterator for OriginValues {
    type Item = ValueIdentity;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.list.get(self.index).copied()?;
        self.index += 1;
        Some(value)
    }
}
impl IntoIterator for OriginList {
    type Item = ValueIdentity;
    type IntoIter = OriginValues;
    fn into_iter(self) -> Self::IntoIter { OriginValues { list: self, index: 0 } }
}

pub(in crate::check) struct OwnedOriginRows {
    rows: OriginRows,
    index: usize,
}
impl Iterator for OwnedOriginRows {
    type Item = OriginList;
    fn next(&mut self) -> Option<Self::Item> {
        let row = self.rows.get_mut(self.index)?;
        self.index += 1;
        Some(mem::take(row))
    }
}
impl IntoIterator for OriginRows {
    type Item = OriginList;
    type IntoIter = OwnedOriginRows;
    fn into_iter(self) -> Self::IntoIter { OwnedOriginRows { rows: self, index: 0 } }
}
