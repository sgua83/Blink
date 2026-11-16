//! Typed synchronous construction at the existing Rust/B contract boundary.
//! The fixed B functions own byte order, tags and traversal. This module only
//! borrows original values and moves actual Rust values into their containers.
pub(crate) use super::types::Bytes;
use super::types::reading;
use super::{ReadStatus, Reader, Result, Writer};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    ptr,
    sync::Arc,
};
pub(crate) type Address = *const c_void;
pub(crate) type Slot = *mut c_void;
pub(crate) type Write = unsafe extern "C" fn(Address, Slot);
pub(crate) type Read = unsafe extern "C" fn(Slot, Slot) -> ReadStatus;
pub(crate) fn address<T>(v: &T) -> Address {
    ptr::from_ref(v).cast()
}
pub(crate) fn slot<T>(v: &mut T) -> Slot {
    ptr::from_mut(v).cast()
}
pub(crate) trait Native: Sized {
    fn empty() -> Self;
    unsafe fn write(&self, output: Slot);
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus;
}
pub(crate) fn empty<T: Native>() -> T {
    T::empty()
}
pub(crate) fn encode<T: Native>(value: &T, writer: &mut Writer) {
    unsafe { value.write(slot(writer)) }
}
pub(crate) fn decode<T: Native>(reader: &mut Reader<'_>) -> Result<T> {
    reading::with_context(reader, |context| {
        let mut value = T::empty();
        let status = unsafe { T::read(context, &mut value) };
        (status, value)
    })
}
pub(crate) unsafe extern "C" fn write_value<T: Native>(value: Address, output: Slot) {
    unsafe { (&*value.cast::<T>()).write(output) }
}
pub(crate) unsafe extern "C" fn read_value<T: Native>(context: Slot, output: Slot) -> ReadStatus {
    unsafe { T::read(context, &mut *output.cast::<T>()) }
}
pub(crate) unsafe extern "C" fn append(output: Slot, bytes: *const u8, count: usize) {
    unsafe { &mut *output.cast::<Writer>() }
        .0
        .extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, count) });
}
pub(crate) unsafe fn input(context: Slot) -> (Bytes, *mut usize) {
    unsafe { reading::input(context) }
}
pub(crate) unsafe fn fail(
    context: Slot,
    message: impl Into<Box<dyn std::error::Error>>,
) -> ReadStatus {
    unsafe { reading::fail(context, message.into()) }
}
pub(crate) unsafe fn finish<T>(context: Slot, result: Result<T>, output: &mut T) -> ReadStatus {
    match result {
        Ok(value) => {
            *output = value;
            ReadStatus::Success
        }
        Err(error) => unsafe { reading::fail(context, error) },
    }
}
macro_rules! codec {
    ($ty:ty) => {
        impl $crate::codec::Encode for $ty {
            fn encode(&self, w: &mut $crate::codec::Writer) {
                $crate::codec::contracts::encode(self, w)
            }
        }
        impl $crate::codec::Decode for $ty {
            fn decode(r: &mut $crate::codec::Reader<'_>) -> $crate::codec::Result<Self> {
                $crate::codec::contracts::decode(r)
            }
        }
    };
}
pub(crate) use codec;
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PairFields {
    pub(crate) first: Slot,
    pub(crate) second: Slot,
}
#[repr(C)]
pub(crate) struct PairWriter {
    pub(crate) first: Write,
    pub(crate) second: Write,
}
#[repr(C)]
pub(crate) struct PairReader {
    pub(crate) first: Read,
    pub(crate) second: Read,
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::contract_write_sequence"]
    fn write_sequence(
        count: usize,
        iterator: Slot,
        output: Slot,
        next: unsafe extern "C" fn(Slot) -> Address,
        element: Write,
        append: unsafe extern "C" fn(Slot, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::contract_read_sequence"]
    fn read_sequence(
        input: Bytes,
        position: *mut usize,
        context: Slot,
        output: Slot,
        push: Read,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::contract_write_option"]
    fn write_option(
        value: Address,
        output: Slot,
        element: Write,
        append: unsafe extern "C" fn(Slot, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::contract_read_option"]
    fn read_option(
        input: Bytes,
        position: *mut usize,
        context: Slot,
        output: Slot,
        some: Read,
        none: unsafe extern "C" fn(Slot),
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::contract_write_pair"]
    fn write_pair(output: Slot, fields: PairFields, host: PairWriter);
    #[link_name = "bootstrap.contract_codec::contract_read_pair"]
    fn read_pair(context: Slot, fields: PairFields, host: PairReader) -> ReadStatus;
}
unsafe extern "C" fn next<'a, T: 'a>(iterator: Slot) -> Address {
    unsafe { &mut *iterator.cast::<std::slice::Iter<'a, T>>() }
        .next()
        .map_or(ptr::null(), address)
}
unsafe fn write_slice<T: Native>(values: &[T], output: Slot) {
    let mut iterator = values.iter();
    unsafe {
        write_sequence(
            values.len(),
            slot(&mut iterator),
            output,
            next::<T>,
            write_value::<T>,
            append,
        )
    }
}
unsafe extern "C" fn push<T: Native>(context: Slot, output: Slot) -> ReadStatus {
    let mut value = T::empty();
    let status = unsafe { T::read(context, &mut value) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<T>>() }.push(value);
    }
    status
}
impl<T: Native> Native for Vec<T> {
    fn empty() -> Self {
        Vec::new()
    }
    unsafe fn write(&self, output: Slot) {
        unsafe { write_slice(self, output) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { input(context) };
        unsafe { read_sequence(bytes, position, context, slot(output), push::<T>) }
    }
}
unsafe extern "C" fn some<T: Native>(context: Slot, output: Slot) -> ReadStatus {
    let mut value = T::empty();
    let status = unsafe { T::read(context, &mut value) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<T>>() = Some(value) }
    }
    status
}
unsafe extern "C" fn none<T>(output: Slot) {
    unsafe { *output.cast::<Option<T>>() = None }
}
impl<T: Native> Native for Option<T> {
    fn empty() -> Self {
        None
    }
    unsafe fn write(&self, output: Slot) {
        unsafe {
            write_option(
                self.as_ref().map_or(ptr::null(), address),
                output,
                write_value::<T>,
                append,
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { input(context) };
        unsafe { read_option(bytes, position, context, slot(output), some::<T>, none::<T>) }
    }
}
impl<T: Native> Native for Box<T> {
    fn empty() -> Self {
        Box::new(T::empty())
    }
    unsafe fn write(&self, output: Slot) {
        unsafe { (**self).write(output) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe { T::read(context, output.as_mut()) }
    }
}
impl<T: Native + Clone> Native for Arc<T> {
    fn empty() -> Self {
        Arc::new(T::empty())
    }
    unsafe fn write(&self, output: Slot) {
        unsafe { (**self).write(output) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe { T::read(context, Arc::make_mut(output)) }
    }
}
impl<A: Native, B: Native> Native for (A, B) {
    fn empty() -> Self {
        (A::empty(), B::empty())
    }
    unsafe fn write(&self, output: Slot) {
        unsafe {
            write_pair(
                output,
                PairFields {
                    first: address(&self.0).cast_mut(),
                    second: address(&self.1).cast_mut(),
                },
                PairWriter {
                    first: write_value::<A>,
                    second: write_value::<B>,
                },
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe {
            read_pair(
                context,
                PairFields {
                    first: slot(&mut output.0),
                    second: slot(&mut output.1),
                },
                PairReader {
                    first: read_value::<A>,
                    second: read_value::<B>,
                },
            )
        }
    }
}
unsafe extern "C" fn next_set<'a, T: Ord + 'a>(iterator: Slot) -> Address {
    unsafe { &mut *iterator.cast::<std::collections::btree_set::Iter<'a, T>>() }
        .next()
        .map_or(ptr::null(), address)
}
unsafe extern "C" fn insert_set<T: Native + Ord>(context: Slot, output: Slot) -> ReadStatus {
    let mut value = T::empty();
    let status = unsafe { T::read(context, &mut value) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<BTreeSet<T>>() }.insert(value);
    }
    status
}
impl<T: Native + Ord> Native for BTreeSet<T> {
    fn empty() -> Self {
        BTreeSet::new()
    }
    unsafe fn write(&self, output: Slot) {
        let mut iterator = self.iter();
        unsafe {
            write_sequence(
                self.len(),
                slot(&mut iterator),
                output,
                next_set::<T>,
                write_value::<T>,
                append,
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { input(context) };
        unsafe { read_sequence(bytes, position, context, slot(output), insert_set::<T>) }
    }
}
struct MapIterator<'a, K, V> {
    iterator: std::collections::btree_map::Iter<'a, K, V>,
    current: Option<(&'a K, &'a V)>,
}
unsafe extern "C" fn next_map<'a, K: Ord + 'a, V: 'a>(iterator: Slot) -> Address {
    let iterator = unsafe { &mut *iterator.cast::<MapIterator<'a, K, V>>() };
    iterator.current = iterator.iterator.next();
    iterator.current.as_ref().map_or(ptr::null(), address)
}
unsafe extern "C" fn write_entry<K: Native, V: Native>(source: Address, output: Slot) {
    let (key, value) = unsafe { &*source.cast::<(&K, &V)>() };
    unsafe {
        write_pair(
            output,
            PairFields {
                first: address(*key).cast_mut(),
                second: address(*value).cast_mut(),
            },
            PairWriter {
                first: write_value::<K>,
                second: write_value::<V>,
            },
        )
    }
}
unsafe extern "C" fn insert_map<K: Native + Ord, V: Native>(
    context: Slot,
    output: Slot,
) -> ReadStatus {
    let mut value = <(K, V)>::empty();
    let status = unsafe { <(K, V)>::read(context, &mut value) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<BTreeMap<K, V>>() }.insert(value.0, value.1);
    }
    status
}
impl<K: Native + Ord, V: Native> Native for BTreeMap<K, V> {
    fn empty() -> Self {
        BTreeMap::new()
    }
    unsafe fn write(&self, output: Slot) {
        let mut iterator = MapIterator {
            iterator: self.iter(),
            current: None,
        };
        unsafe {
            write_sequence(
                self.len(),
                slot(&mut iterator),
                output,
                next_map::<K, V>,
                write_entry::<K, V>,
                append,
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { input(context) };
        unsafe { read_sequence(bytes, position, context, slot(output), insert_map::<K, V>) }
    }
}

impl<T: Native> Native for Arc<[T]> {
    fn empty() -> Self {
        Arc::from([])
    }
    unsafe fn write(&self, output: Slot) {
        unsafe { write_slice(self, output) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let mut value = Vec::<T>::new();
        let status = unsafe { Vec::<T>::read(context, &mut value) };
        if status == ReadStatus::Success {
            *output = value.into();
        }
        status
    }
}
impl Native for u8 {
    fn empty() -> Self {
        0
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_u8"]
            fn write(value: u8, output: Slot, append: unsafe extern "C" fn(Slot, *const u8, usize));
        }
        unsafe { write(*self, output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_u8"]
            fn read(input: Bytes, position: *mut usize, output: *mut u8) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, output) }
    }
}
impl Native for u16 {
    fn empty() -> Self {
        0
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_u16"]
            fn write(
                value: u16,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe { write(*self, output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_u16"]
            fn read(input: Bytes, position: *mut usize, output: *mut u16) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, output) }
    }
}
impl Native for u32 {
    fn empty() -> Self {
        0
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_u32"]
            fn write(
                value: u32,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe { write(*self, output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_u32"]
            fn read(input: Bytes, position: *mut usize, output: *mut u32) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, output) }
    }
}
impl Native for u64 {
    fn empty() -> Self {
        0
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_u64"]
            fn write(
                value: u64,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe { write(*self, output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_u64"]
            fn read(input: Bytes, position: *mut usize, output: *mut u64) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, output) }
    }
}
impl Native for usize {
    fn empty() -> Self {
        0
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_usize"]
            fn write(
                value: usize,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe { write(*self, output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_usize"]
            fn read(input: Bytes, position: *mut usize, output: *mut usize) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, output) }
    }
}
impl Native for bool {
    fn empty() -> Self {
        false
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_bool"]
            fn write(
                value: u32,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe { write(u32::from(*self), output, append) }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_bool"]
            fn read(input: Bytes, position: *mut usize, output: *mut u32) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        let mut value = 0u32;
        let status = unsafe { read(bytes, position, &mut value) };
        if status == ReadStatus::Success {
            *output = value != 0;
        }
        status
    }
}
impl Native for String {
    fn empty() -> Self {
        String::new()
    }
    unsafe fn write(&self, output: Slot) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_text"]
            fn write(
                value: Bytes,
                output: Slot,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe {
            write(
                Bytes {
                    data: self.as_ptr(),
                    count: self.len(),
                },
                output,
                append,
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_text"]
            fn read(
                input: Bytes,
                position: *mut usize,
                context: Slot,
                output: Slot,
                text: unsafe extern "C" fn(Slot, Slot, Bytes) -> ReadStatus,
            ) -> ReadStatus;
        }
        let (bytes, position) = unsafe { input(context) };
        unsafe { read(bytes, position, context, slot(output), reading::native_text) }
    }
}
pub(crate) unsafe fn write_map_with<K: Ord, V>(
    values: &BTreeMap<K, V>,
    output: Slot,
    element: Write,
) {
    let mut iterator = MapIterator {
        iterator: values.iter(),
        current: None,
    };
    unsafe {
        write_sequence(
            values.len(),
            slot(&mut iterator),
            output,
            next_map::<K, V>,
            element,
            append,
        )
    }
}
pub(crate) unsafe fn write_pair_with(
    first: Address,
    second: Address,
    output: Slot,
    a: Write,
    b: Write,
) {
    unsafe {
        write_pair(
            output,
            PairFields {
                first: first.cast_mut(),
                second: second.cast_mut(),
            },
            PairWriter {
                first: a,
                second: b,
            },
        )
    }
}
#[repr(C)]
struct NativeSpan {
    data: Address,
    count: usize,
    stride: usize,
}
pub(crate) unsafe fn write_trimmed<T: Native>(
    values: &[T],
    output: Slot,
    is_empty: unsafe extern "C" fn(Address) -> u32,
) {
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_trimmed"]
        fn write(
            values: NativeSpan,
            output: Slot,
            is_empty: unsafe extern "C" fn(Address) -> u32,
            element: Write,
            append: unsafe extern "C" fn(Slot, *const u8, usize),
        );
    }
    unsafe {
        write(
            NativeSpan {
                data: values.as_ptr().cast(),
                count: values.len(),
                stride: size_of::<T>(),
            },
            output,
            is_empty,
            write_value::<T>,
            append,
        )
    }
}
