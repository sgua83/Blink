//! B reads the two borrowed fact trees through their original native layout.
use super::Facts;

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::cross_conflict"]
    fn evaluate(left: &Facts, right: &Facts, may_share: u32) -> u32;
}

pub(super) fn conflict(left: &Facts, right: &Facts, may_share: bool) -> bool {
    unsafe { evaluate(left, right, u32::from(may_share)) != 0 }
}
