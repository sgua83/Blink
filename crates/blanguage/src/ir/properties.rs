//! Synchronously borrow the original Ty containers for B layout queries.
use super::Ty;
use std::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Query { Dataset, Resources, Cleanup, Arrays, Views, Borrows }
#[derive(Clone, Copy)]
#[repr(u32)]
enum Child { Variant, Array, Field }
#[repr(C)]
struct Node {
    task: u32, cleanup: u32, dataset: u32, view: u32, array: u32, address: u32,
    variants: usize, fields: usize, count: u32, alignment: u32,
}
#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(*const Ty) -> Node,
    child: unsafe extern "C" fn(*const Ty, Child, usize) -> *const Ty,
}
#[repr(C)]
struct Part { value: *const Ty, offset: u32 }
#[repr(C)]
struct BorrowHost {
    types: Host,
    part: unsafe extern "C" fn(*const Ty, usize) -> Part,
}
unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::contains"]
    fn b_contains(value: *const Ty, query: Query, host: Host) -> u32;
    #[link_name = "bootstrap.type_layout::borrow_fields"]
    fn b_borrow_fields(value: *const Ty, base: u32, host: BorrowHost, context: *mut c_void,
        emit: unsafe extern "C" fn(*mut c_void, u32, *const Ty));
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
        count: if ty.array.is_some() { ty.values[0] } else { 0 }, alignment: ty.align,
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

pub(super) fn borrows(value: &Ty) -> Vec<(u32, &Ty)> {
    let mut output: Vec<(u32, &Ty)> = Vec::new();
    unsafe {
        b_borrow_fields(value, 0, BorrowHost { types: Host { inspect, child }, part },
            ptr::from_mut(&mut output).cast(), emit);
    }
    output
}
unsafe extern "C" fn part(value: *const Ty, index: usize) -> Part {
    let (offset, value) = unsafe { &*value }.aggregate_part(index);
    Part { value: ptr::from_ref(value), offset }
}
// B emits only the original value or its owned layout children. The result
// remains borrowed from the input Ty; no reference escapes that ownership.
unsafe extern "C" fn emit(context: *mut c_void, offset: u32, value: *const Ty) {
    let output = unsafe { &mut *context.cast::<Vec<(u32, &Ty)>>() };
    output.push((offset, unsafe { &*value }));
}
