//! Synchronously borrow the original Ty containers for B layout queries.
use super::Ty;
use std::ptr;

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Query { Dataset, Resources, Cleanup, Arrays, Views, Borrows }
#[derive(Clone, Copy)]
#[repr(u32)]
enum Child { Variant, Array, Field }
#[repr(C)]
struct Node {
    task: u32, cleanup: u32, dataset: u32, view: u32, array: u32, address: u32,
    variants: usize, fields: usize, count: u32,
}
#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(*const Ty) -> Node,
    child: unsafe extern "C" fn(*const Ty, Child, usize) -> *const Ty,
}
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::contains"]
    fn b_contains(value: *const Ty, query: Query, host: Host) -> u32;
}

pub(super) fn contains(value: &Ty, query: Query) -> bool {
    unsafe { b_contains(value, query, Host { inspect, child }) != 0 }
}
unsafe extern "C" fn inspect(value: *const Ty) -> Node {
    let ty = unsafe { &*value };
    Node {
        task: u32::from(ty.task_result().is_some()),
        cleanup: u32::from(ty.cleanup.is_some()),
        dataset: u32::from(ty.dataset.is_some()),
        view: u32::from(ty.view.is_some()),
        array: u32::from(ty.array.is_some()),
        address: u32::from(ty.is_address()),
        variants: ty.variants.len(), fields: ty.fields.len(),
        count: if ty.array.is_some() { ty.values[0] } else { 0 },
    }
}
unsafe extern "C" fn child(value: *const Ty, kind: Child, index: usize) -> *const Ty {
    let ty = unsafe { &*value };
    ptr::from_ref(match kind {
        Child::Variant => ty.variants[index].ty.as_ref(),
        Child::Array => ty.array.as_deref().unwrap(),
        Child::Field => ty.fields[index].2.as_ref(),
    })
}
