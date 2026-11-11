//! A task owns its work Frame until completion and successful result transfer.
use super::{Frame, ResidentData, abi};
use crate::{
    Result,
    cpu::{CpuDataset, WorkHandle},
    error,
};
use blanguage::ir::{Ty, align_up};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    ptr,
};

pub(super) type Entry = unsafe extern "C" fn(*mut Frame, *const u8, *mut u8) -> *mut c_void;

fn read_handle(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn offsets(ty: &Ty, value: &[u8], base: usize, output: &mut Vec<usize>) {
    if ty.dataset.is_some() {
        output.push(base);
    } else if let Some(element) = &ty.array {
        let stride = align_up(element.size, element.align) as usize;
        for index in 0..ty.values[0] as usize {
            offsets(element, value, base + index * stride, output);
        }
    } else if !ty.variants.is_empty() {
        let bytes = ty.components[0].0.size() as usize;
        let mut tag = [0u8; 8];
        tag[..bytes].copy_from_slice(&value[base..base + bytes]);
        let tag = u64::from_le_bytes(tag);
        let variant = ty
            .variants
            .iter()
            .find(|variant| u64::from(variant.discriminant) == tag)
            .unwrap();
        offsets(
            &variant.ty,
            value,
            base + align_up(4, ty.align) as usize,
            output,
        );
    } else {
        for (_, offset, field) in &ty.fields {
            if field.owns_resources() {
                offsets(field, value, base + *offset as usize, output);
            }
        }
    }
}

fn gather(frame: &Frame, ty: &Ty, bytes: &[u8], selected: &mut BTreeSet<usize>) -> Result<()> {
    let mut fields = Vec::new();
    offsets(ty, bytes, 0, &mut fields);
    for field in fields {
        let slot = read_handle(bytes, field) as usize;
        if !selected.insert(slot) {
            continue;
        }
        let Some(ResidentData::Cpu(CpuDataset::Owned(data))) = frame.data.datasets[slot].as_ref()
        else {
            return Err(error("CPU task transfer requires owned CPU storage"));
        };
        if data.ty.contains_borrows() {
            return Err(error("CPU task datasets cannot carry cross-task borrows"));
        }
        if data.ty.owns_resources() {
            let bytes = unsafe {
                std::slice::from_raw_parts(data.words.as_ptr().cast::<u8>(), data.words.len() * 4)
            };
            let stride = data.stride as usize;
            for value in bytes[..stride * data.count as usize].chunks_exact(stride) {
                gather(frame, &data.ty, value, selected)?;
            }
        }
    }
    Ok(())
}

fn rewrite(ty: &Ty, value: &mut [u8], slots: &BTreeMap<usize, u32>, fields: &mut Vec<usize>) {
    fields.clear();
    offsets(ty, value, 0, fields);
    for &field in fields.iter() {
        let handle = slots[&(read_handle(value, field) as usize)];
        value[field..field + 4].copy_from_slice(&handle.to_le_bytes());
    }
}

/// Reserve every destination before changing either Frame or the value.
fn transfer(
    source: &mut Frame,
    destination: &mut Frame,
    ty: &Ty,
    value: &mut [u8],
    selected: BTreeSet<usize>,
) -> Result<()> {
    let mut destinations: Vec<_> = destination
        .data
        .datasets
        .iter()
        .enumerate()
        .filter_map(|(slot, data)| data.is_none().then_some(slot))
        .take(selected.len())
        .collect();
    let additional = selected.len() - destinations.len();
    let start = destination.data.datasets.len();
    destinations.extend(start..start + additional);
    let slots: BTreeMap<_, _> = selected
        .into_iter()
        .zip(destinations)
        .map(|(source, target)| Ok((source, u32::try_from(target)?)))
        .collect::<Result<_>>()?;
    let domains: Vec<_> = source
        .data
        .domains
        .iter()
        .filter_map(|relation| relation.mapped(|old| slots.get(&old).map(|&slot| slot as usize)))
        .collect();
    destination.data.datasets.try_reserve(additional)?;
    destination.data.domains.try_reserve(domains.len())?;

    let mut fields = Vec::new();
    rewrite(ty, value, &slots, &mut fields);
    for (&old, &new) in &slots {
        let mut storage = source.data.datasets[old].take().unwrap();
        if let ResidentData::Cpu(CpuDataset::Owned(data)) = &mut storage
            && data.ty.owns_resources()
        {
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(
                    data.words.as_mut_ptr().cast::<u8>(),
                    data.words.len() * 4,
                )
            };
            let stride = data.stride as usize;
            for value in bytes[..stride * data.count as usize].chunks_exact_mut(stride) {
                rewrite(&data.ty, value, &slots, &mut fields);
            }
        }
        if new as usize == destination.data.datasets.len() {
            destination.data.datasets.push(Some(storage));
        } else {
            destination.data.datasets[new as usize] = Some(storage);
        }
    }
    destination.data.domains.extend(domains);
    source
        .data
        .domains
        .retain(|relation| !slots.keys().any(|&slot| relation.references(slot)));
    Ok(())
}

struct Work {
    frame: Option<Frame>,
    entry: Entry,
    cleanup: Option<Entry>,
    input: Vec<u8>,
    result_type: usize,
    output: Vec<u8>,
    completion: Option<std::result::Result<BTreeSet<usize>, *mut c_void>>,
}

impl Work {
    fn abandon_result(&mut self, error: Box<dyn std::error::Error + Send + Sync>) -> *mut c_void {
        let error = abi::into_error(error);
        let cleanup = self.cleanup.map_or(ptr::null_mut(), |cleanup| unsafe {
            cleanup(
                self.frame.as_mut().unwrap(),
                self.output.as_ptr(),
                ptr::null_mut(),
            )
        });
        self.frame = None;
        if cleanup.is_null() {
            error
        } else {
            unsafe { abi::blink_error_join(error, cleanup) }
        }
    }
}

struct Task {
    handle: Option<WorkHandle>,
    work: Work,
}

unsafe extern "system" fn execute(_: *mut c_void, context: *mut c_void, _: *mut c_void) {
    let work = unsafe { &mut *context.cast::<Work>() };
    let frame = work.frame.as_mut().unwrap();
    let status = unsafe { (work.entry)(frame, work.input.as_ptr(), work.output.as_mut_ptr()) };
    if !status.is_null() {
        work.frame = None;
        work.completion = Some(Err(status));
        return;
    }
    let mut selected = BTreeSet::new();
    let result_type = &frame.definitions.types[work.result_type];
    work.completion = Some(
        match gather(frame, result_type, &work.output, &mut selected) {
            Ok(()) => {
                // Only owned CPU results leave the work thread. Device consumers end here.
                frame.gpu_programs.clear();
                frame.device = None;
                Ok(selected)
            }
            Err(error) => Err(work.abandon_result(error)),
        },
    );
}

/// The input and output storage are the layouts bound by the originating module.
pub(super) unsafe fn start(
    frame: &mut Frame,
    index: usize,
    entry: Entry,
    cleanup: Option<Entry>,
    input: *const u8,
    output: *mut u8,
) -> Result<()> {
    let definitions = frame.definitions.clone();
    let input_type = &definitions.types[index];
    let result_type = index + 1;
    let start_type = &definitions.types[index + 2];
    let mut bytes = vec![0; input_type.size as usize];
    for &(scalar, offset) in &input_type.components {
        let offset = offset as usize;
        unsafe {
            ptr::copy_nonoverlapping(
                input.add(offset),
                bytes.as_mut_ptr().add(offset),
                scalar.size() as usize,
            )
        };
    }
    let worker = unsafe { Frame::from_definitions(definitions.clone(), &[], &[]) };
    let task = Box::into_raw(Box::new(Task {
        handle: None,
        work: Work {
            frame: Some(worker),
            entry,
            cleanup,
            input: bytes,
            output: vec![0; definitions.types[result_type].size as usize],
            result_type,
            completion: None,
        },
    }));
    let context = unsafe { ptr::addr_of_mut!((*task).work) };
    let result = unsafe {
        ptr::write_bytes(output, 0, start_type.size as usize);
        std::slice::from_raw_parts_mut(output, start_type.size as usize)
    };
    let payload = align_up(4, start_type.align) as usize;
    let handle = match unsafe { WorkHandle::new(execute, context.cast()) } {
        Ok(handle) => handle,
        Err(error) => {
            let task = unsafe { Box::from_raw(task) };
            let failure = &start_type.variants[1];
            let code = payload + failure.ty.fields[0].1 as usize;
            let input = payload + failure.ty.fields[1].1 as usize;
            result[..4].copy_from_slice(&failure.discriminant.to_le_bytes());
            result[code..code + 4]
                .copy_from_slice(&(error.raw_os_error().unwrap() as u32).to_le_bytes());
            result[input..input + task.work.input.len()].copy_from_slice(&task.work.input);
            return Ok(());
        }
    };
    let work = unsafe { &mut *context };
    let mut selected = BTreeSet::new();
    let moved = gather(frame, input_type, &work.input, &mut selected).and_then(|()| {
        transfer(
            frame,
            work.frame.as_mut().unwrap(),
            input_type,
            &mut work.input,
            selected,
        )
    });
    if let Err(error) = moved {
        drop(handle);
        drop(unsafe { Box::from_raw(task) });
        return Err(error);
    }
    unsafe {
        ptr::addr_of_mut!((*task).handle).write(Some(handle));
        (*ptr::addr_of!((*task).handle)).as_ref().unwrap().submit();
    }
    result[..4].copy_from_slice(&start_type.variants[0].discriminant.to_le_bytes());
    result[payload..payload + 8].copy_from_slice(&(task as u64).to_le_bytes());
    Ok(())
}

/// The task pointer is uniquely owned. Only its native handle is read while work is active.
pub(super) unsafe fn wait(frame: &mut Frame, owner: *mut c_void, output: *mut u8) -> *mut c_void {
    let task = owner.cast::<Task>();
    drop(unsafe { (&mut *ptr::addr_of_mut!((*task).handle)).take().unwrap() });
    let mut task = unsafe { Box::from_raw(task) };
    let work = &mut task.work;
    let selected = match work.completion.take().unwrap() {
        Ok(selected) => selected,
        Err(error) => return error,
    };
    let worker = work.frame.as_mut().unwrap();
    let definitions = worker.definitions.clone();
    match transfer(
        worker,
        frame,
        &definitions.types[work.result_type],
        &mut work.output,
        selected,
    ) {
        Ok(()) => {
            unsafe { ptr::copy_nonoverlapping(work.output.as_ptr(), output, work.output.len()) };
            ptr::null_mut()
        }
        Err(error) => work.abandon_result(error),
    }
}
