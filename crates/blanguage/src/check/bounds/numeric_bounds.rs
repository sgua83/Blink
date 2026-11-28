//! Each numeric proof query owns the bounds folded from its original facts.
use super::{EqualityRoots, Facts, Scalar, ValueIdentity};
use crate::ir::scalar_ranges::BitRange;
use std::{ffi::c_void, ptr};

pub(super) struct Bounds(Facts);

unsafe extern "C" {
    #[link_name = "bootstrap.numeric_bounds::numeric_bounds"]
    fn fold(source: &Facts, roots: *const c_void, output: &mut Facts);
    #[link_name = "bootstrap.numeric_bounds::numeric_limits"]
    fn limits(bounds: &Facts, identity: &ValueIdentity, scalar: &Scalar) -> BitRange;
}

impl Bounds {
    pub(super) fn collect(facts: &Facts, roots: &EqualityRoots) -> Self {
        let mut result = Facts::new();
        unsafe { fold(facts, ptr::from_ref(roots.bindings()).cast(), &mut result) };
        Self(result)
    }

    pub(super) fn limits(&self, identity: ValueIdentity, scalar: Scalar) -> BitRange {
        unsafe { limits(&self.0, &identity, &scalar) }
    }
}
