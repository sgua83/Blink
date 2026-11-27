//! Rust callers borrow the same C-layout keys compared by the B fact store.
use super::*;
use std::{cmp::Ordering, ffi::c_void, ptr};

type Key = *const c_void;

// The B store uses 144-byte keys and reads their explicitly laid-out fields.
const _: () = {
    assert!(size_of::<Fact>() == 144 && align_of::<Fact>() == 8);
    assert!(size_of::<ValueIdentity>() == 32 && align_of::<ValueIdentity>() == 8);
    assert!(size_of::<IndexDomain>() == 104 && align_of::<IndexDomain>() == 8);
    assert!(size_of::<Operand>() == 40 && align_of::<Operand>() == 8);
    assert!(size_of::<Selector>() == 80 && align_of::<Selector>() == 16);
    assert!(size_of::<IntegerExpression>() == 88 && align_of::<IntegerExpression>() == 8);
    assert!(size_of::<ScalarPredicate>() == 88 && align_of::<ScalarPredicate>() == 8);
    assert!(size_of::<FloatingBound>() == 80 && align_of::<FloatingBound>() == 8);
};

unsafe extern "C" {
    #[link_name = "bootstrap.fact_key::order_identity"]
    pub(super) fn order_identity(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_domain"]
    fn order_domain(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_operand"]
    fn order_operand(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_selector"]
    fn order_selector(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_predicate"]
    fn order_predicate(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_expression"]
    fn order_expression(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_floating"]
    fn order_floating(left: Key, right: Key) -> i32;
    #[link_name = "bootstrap.fact_key::order_fact<0>"]
    fn order_fact(left: Key, right: Key) -> i32;
}

macro_rules! order {
    ($key:ty, $native:ident) => {
        impl Ord for $key {
            fn cmp(&self, other: &Self) -> Ordering {
                unsafe { $native(ptr::from_ref(self).cast(), ptr::from_ref(other).cast()).cmp(&0) }
            }
        }

        impl PartialOrd for $key {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
    };
}

order!(ValueIdentity, order_identity);
order!(IndexDomain, order_domain);
order!(Operand, order_operand);
order!(Selector, order_selector);
order!(ScalarPredicate, order_predicate);
order!(IntegerExpression, order_expression);
order!(FloatingBound, order_floating);
order!(Fact, order_fact);
