//! A task owns its work Frame until completion and successful result transfer.
use super::{CpuBorrow, Frame, ResidentData, abi};
use crate::{
    Result,
    cpu::{CpuDataset, WorkHandle},
    error,
};
use blanguage::{ast::Access, ir::{Scalar, Ty, align_up}};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    ptr,
};

pub(super) type Entry = unsafe extern "C" fn(*mut Frame, *const u8, *mut u8) -> *mut c_void;
pub(super) type Cleanup = unsafe extern "C" fn(*mut Frame, *const u8, *const u8, *mut c_void, u32) -> *mut c_void;

// A reference may have padding. Read only initialized tags and handle words.
unsafe fn offsets(ty: &Ty, value: *const u8, base: usize, output: &mut Vec<usize>, status: *const c_void) {
    if unsafe { abi::blink_error_consumed(status, value.wrapping_add(base) as usize, ty.size as usize) } != 0 {
        return;
    }
    if ty.dataset.is_some() {
        output.push(base);
    } else if let Some(element) = &ty.array {
        let stride = align_up(element.size, element.align) as usize;
        for index in 0..ty.values[0] as usize {
            unsafe { offsets(element, value, base + index * stride, output, status) };
        }
    } else if !ty.variants.is_empty() {
        let bytes = ty.components[0].0.size() as usize;
        let mut tag = [0u8; 8];
        unsafe { ptr::copy_nonoverlapping(value.add(base), tag.as_mut_ptr(), bytes) };
        let tag = u64::from_le_bytes(tag);
        let variant = ty.variants.iter().find(|variant| u64::from(variant.discriminant) == tag).unwrap();
        unsafe { offsets(&variant.ty, value, base + align_up(4, ty.align) as usize, output, status) };
    } else {
        for (name, offset, field) in &ty.fields {
            if !name.starts_with(Ty::TASK_READ_FIELD_PREFIX) && field.contains_dataset() {
                unsafe { offsets(field, value, base + *offset as usize, output, status) };
            }
        }
    }
}

// A root word belongs to the caller position or the task packet. Element
// words are addressed through their owned source slot at the actual handoff.
#[derive(Clone, Copy)]
enum Word {
    Root(*mut u32),
    Element { slot: usize, byte: usize },
}

#[derive(Default)]
struct Selection {
    slots: BTreeSet<usize>,
    words: Vec<(Word, usize)>,
}

fn positions<'a>(
    types: &'a [Ty],
    packet: &'a Ty,
    bytes: &'a [u8],
) -> impl Iterator<Item = (&'a Ty, *mut u8, bool, Option<usize>)> + 'a {
    packet.fields.iter().filter_map(move |(name, offset, _)| {
        let (access, ty) = name.strip_prefix(Ty::TASK_OWNED_REFERENCE_PREFIX)?.split_once(':').unwrap();
        let (ty, ready) = ty.split_once(':').map_or((ty, None), |(ty, ready)| (ty, Some(ready.parse::<usize>().unwrap())));
        let offset = *offset as usize;
        let address = usize::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()) as *mut u8;
        Some((&types[ty.parse::<usize>().unwrap()], address, access == "inout", ready))
    })
}

fn completed_positions<'a>(
    types:&'a [Ty], input_type:&'a Ty, input:&'a [u8], output:&'a [u8],
) -> impl Iterator<Item=(&'a Ty,*mut u8)>+'a {
    positions(types,input_type,input)
        .flat_map(move |(ty,address,_,ready)| completed_owners(ty, address, ready, output))
}

fn completed_owners<'a>(ty: &'a Ty, address: *mut u8, ready: Option<usize>, output: &'a [u8])
    -> impl Iterator<Item = (&'a Ty, *mut u8)> + 'a
{
    let whole = ready.is_none().then_some((ty, address));
    let selected = ready.into_iter().flat_map(move |base| {
        ty.task_output_owners().into_iter().enumerate().filter_map(move |(index,(offset,owner))| {
            (output[base+index*Scalar::Bool.size() as usize]!=0)
                .then(|| (owner, address.wrapping_add(offset as usize)))
        })
    });
    whole.into_iter().chain(selected)
}

pub(super) struct InputCompletionWrites {
    input_type:usize,
    output_base:usize,
    positions:Vec<(Ty,*mut u8)>,
}
impl InputCompletionWrites {
    fn consumed(&self,error:*mut c_void) {
        for (ty,address) in &self.positions {
            unsafe { abi::consumed(error,*address as usize,ty.size as usize) };
        }
    }
}

fn cpu_storage(data: &ResidentData) -> Option<(&Ty, u32, u32, *const u32)> {
    match data {
        ResidentData::Cpu(data) => Some((&data.ty, data.stride, data.count, data.words.as_ptr())),
        ResidentData::CpuBorrow(data) => Some((&data.ty, data.stride, data.count, data.pointer.cast_const())),
        ResidentData::Gpu(_) => None,
    }
}

unsafe fn gather(frame: &Frame, ty: &Ty, value: *const u8, selected: &mut Selection, shared: bool) -> Result<()> {
    unsafe { gather_value(frame,ty,value,selected,shared,None,ptr::null()) }
}

unsafe fn gather_value(
    frame: &Frame,
    ty: &Ty,
    value: *const u8,
    selected: &mut Selection,
    shared: bool,
    element: Option<(usize, usize)>,
    status: *const c_void,
) -> Result<()> {
    if !ty.contains_dataset() {
        return Ok(());
    }
    let mut fields = Vec::new();
    unsafe { offsets(ty, value, 0, &mut fields, status) };
    for field in fields {
        let word = unsafe { value.add(field).cast::<u32>() };
        let slot = u32::from_le(unsafe { word.read_unaligned() }) as usize;
        if !shared {
            let location=match element {
                Some((owner,base))=>Word::Element{slot:owner,byte:base+field},
                None=>Word::Root(word.cast_mut()),
            };
            selected.words.push((location,slot));
        }
        if !selected.slots.insert(slot) {
            continue;
        }
        let storage = frame.data.datasets[slot].as_ref();
        if !shared && !matches!(storage, Some(ResidentData::Cpu(CpuDataset::Owned(_)))) {
            return Err(error("CPU task transfer requires owned CPU storage"));
        }
        let (element, stride, count, pointer) = storage.and_then(cpu_storage)
            .ok_or_else(|| error("CPU task read requires CPU storage"))?;
        if element.contains_borrows() {
            return Err(error("CPU task datasets cannot carry cross-task borrows"));
        }
        if element.contains_dataset() {
            for index in 0..count as usize {
                let byte=stride as usize*index;
                let element_ptr=unsafe { pointer.cast::<u8>().add(byte) };
                unsafe { gather_value(frame,element,element_ptr,selected,shared,Some((slot,byte)),status)? };
            }
        }
    }
    Ok(())
}

enum DatasetBase {
    Address(*const u8),
    Frame(usize),
}

struct DatasetLoan<'a> {
    base: DatasetBase,
    element: &'a Ty,
    access: Access,
    ready: Option<usize>,
    count: usize,
    stride: usize,
    offset: usize,
}

impl DatasetLoan<'_> {
    fn elements(&self, frame: &mut Frame) -> Result<impl Iterator<Item = *const u8> + '_> {
        let base = match self.base {
            DatasetBase::Address(address) => address,
            DatasetBase::Frame(slot) => {
                let (_, _, _, pointer) = frame.data.datasets[slot].as_ref().and_then(cpu_storage)
                    .ok_or_else(|| error("CPU task read requires CPU storage"))?;
                if self.access.writes() {
                    frame.storage_address(slot as u32) as usize as *const u8
                } else {
                    pointer.cast::<u8>()
                }
            }
        };
        Ok((0..self.count).map(move |index| base.wrapping_add(self.offset + index * self.stride)))
    }
}

fn dataset_loans<'a>(packet: &'a Ty, bytes: &'a [u8]) -> impl Iterator<Item = DatasetLoan<'a>> + 'a {
    packet.fields.iter().filter_map(move |(name, offset, field)| {
        let (mode, layout) = name.strip_prefix(Ty::TASK_BORROWED_DATASET_PREFIX)?.split_once(':').unwrap();
        let (element, ready) = layout.split_once(':')
            .map_or((layout, None), |(element, ready)| (element, Some(ready.parse().unwrap())));
        let access = match mode {
            "read" => Access::Read,
            "write" => Access::Write,
            "inout" => Access::InOut,
            _ => unreachable!(),
        };
        let word = |index: usize| {
            let (scalar, at) = field.components[index];
            let start = (*offset + at) as usize;
            let width = scalar.size() as usize;
            let mut value = [0; 8];
            value[..width].copy_from_slice(&bytes[start..start + width]);
            u64::from_le_bytes(value) as usize
        };
        let root = word(0);
        let base = match field.components[0].0 {
            Scalar::U64 => DatasetBase::Address(root as *const u8),
            _ => DatasetBase::Frame(root),
        };
        Some(DatasetLoan {
            base, element: &packet.arguments[element.parse::<usize>().unwrap()], access, ready,
            count: word(1), stride: word(2), offset: word(3),
        })
    })
}

fn bind_borrowed_inputs(
    source: &mut Frame, destination: &mut Frame, layout: &Ty, bytes: &[u8], owned: &mut Selection,
) -> Result<()> {
    let mut borrowed = Selection::default();
    let mut writable = BTreeSet::new();
    for loan in dataset_loans(layout, bytes) {
        let elements = loan.elements(source)?;
        if let DatasetBase::Frame(root) = loan.base {
            borrowed.slots.insert(root);
            if loan.access.writes() {
                writable.insert(root);
            }
        }
        if loan.access.reads() && loan.element.contains_dataset() {
            let shared = loan.access == Access::Read;
            let selection = if shared { &mut borrowed } else { &mut *owned };
            for address in elements {
                // The outer rows stay with the caller. Their child handle words
                // are stable addresses for the original Frame transfer.
                unsafe { gather(source, loan.element, address, selection, shared)? };
            }
        }
    }
    for (name, offset, field) in &layout.fields {
        if name.starts_with(Ty::TASK_READ_FIELD_PREFIX) {
            unsafe { gather(source, field, bytes.as_ptr().add(*offset as usize), &mut borrowed, true)? };
        }
    }
    let selected=borrowed.slots;
    if let Some(&last)=selected.last() {
        // Borrowed element storage keeps the caller's nested handle numbers.
        destination.data.datasets.try_reserve(last+1)?;
        destination.data.datasets.resize_with(last+1,||None);
        for &slot in &selected {
            let (ty, stride, count, pointer) = source.data.datasets[slot].as_ref().and_then(cpu_storage).unwrap();
            let ty = ty.clone();
            let writable = writable.contains(&slot);
            let pointer = if writable {
                source.storage_address(slot as u32) as usize as *mut u32
            } else {
                pointer.cast_mut()
            };
            destination.data.datasets[slot] = Some(ResidentData::CpuBorrow(CpuBorrow {
                ty, stride, count, pointer, writable,
            }));
        }
        destination.data.domains.extend(source.data.domains.iter().filter_map(|relation|
            relation.mapped(|slot|selected.contains(&slot).then_some(slot))
        ));
    }
    Ok(())
}

/// Every recorded word stays live through this Frame handoff. Reserve before rewriting any word.
unsafe fn transfer(source: &mut Frame, destination: &mut Frame, selected: Selection) -> Result<()> {
    let Selection{slots:selected,words}=selected;
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

    for (word, old) in words {
        let word=match word {
            Word::Root(word)=>word,
            Word::Element{slot,byte}=>{
                let Some(ResidentData::Cpu(CpuDataset::Owned(data)))=source.data.datasets[slot].as_mut() else {
                    unreachable!("gathered element belongs to the original CPU owner");
                };
                unsafe { data.words.as_mut_ptr().cast::<u8>().add(byte).cast::<u32>() }
            }
        };
        unsafe { word.write_unaligned(slots[&old].to_le()) };
    }
    for (&old, &new) in &slots {
        let storage = source.data.datasets[old].take().unwrap();
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
    cleanup: Option<Cleanup>,
    input: Vec<u8>,
    result_type: usize,
    output: Vec<u8>,
    completion: Option<std::result::Result<Completion, *mut c_void>>,
}

struct Completion {
    selected: Selection,
    status: *mut c_void,
}

impl Work {
    fn abandon_result(&mut self, error: Box<dyn std::error::Error + Send + Sync>, status:*mut c_void) -> *mut c_void {
        let error = abi::into_error(error);
        let error=if status.is_null(){error}else{unsafe {abi::blink_error_join(status,error)}};
        let frame = self.frame.as_mut().unwrap();
        let error = self.cleanup.map_or(error, |cleanup| unsafe {
            cleanup(frame, self.output.as_ptr(), self.input.as_ptr(), error, u32::from(status.is_null()))
        });
        let types = &frame.definitions.types;
        let input_type = &types[self.result_type - 1];
        for (ty, address, _, _) in positions(types, input_type, &self.input) {
            if ty.contains_dataset() {
                unsafe { abi::consumed(error, address as usize, ty.size as usize) };
            }
        }
        if let Some(completions)=&frame.task_completion_writes {
            completions.consumed(error);
        }
        self.frame = None;
        self.output.fill(0);
        error
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
    let definitions = frame.definitions.clone();
    let types = &definitions.types;
    let input_type=&types[work.result_type-1];
    if !status.is_null() {
        for (ty,address,_,_) in positions(types,input_type,&work.input) {
            unsafe { abi::consumed(status,address as usize,ty.size as usize) };
        }
    }
    let mut selected = Selection::default();
    let result_type = &types[work.result_type];
    let collected:Result<()>=(|| {
        if status.is_null() {
            unsafe { gather(frame,result_type,work.output.as_mut_ptr(),&mut selected,false)?; }
            for (ty,address) in completed_positions(types,input_type,&work.input,&work.output) {
                unsafe { gather(frame,ty,address,&mut selected,false)?; }
            }
        }
        for loan in dataset_loans(input_type, &work.input) {
            if loan.access.writes() && (status.is_null() || loan.access.reads()) && loan.element.contains_dataset() {
                for address in loan.elements(frame)? {
                    for (ty, position) in completed_owners(loan.element, address.cast_mut(), loan.ready, &work.output) {
                        unsafe { gather_value(frame, ty, position, &mut selected, false, None, status)? };
                    }
                }
            }
        }
        if let Some(completions)=&frame.task_completion_writes {
            for (ty,address) in &completions.positions {
                unsafe { gather(frame,ty,*address,&mut selected,false)?; }
            }
        }
        Ok(())
    })();
    work.completion = Some(
        match collected {
            Ok(()) => {
                // Only owned CPU results leave the work thread. Device consumers end here.
                frame.gpu_programs.clear();
                frame.device = None;
                Ok(Completion{selected,status})
            }
            Err(error) => Err(work.abandon_result(error,status)),
        },
    );
}

/// The input and output storage are the layouts bound by the originating module.
pub(super) unsafe fn start(
    frame: &mut Frame,
    index: usize,
    entry: Entry,
    cleanup: Option<Cleanup>,
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
    let mut worker = unsafe { Frame::from_definitions(definitions.clone(), &[], &[]) };
    let layout=input_type.arguments.last().unwrap();
    let mut work_output=vec![0;layout.size as usize];
    for (_,offset,completion) in &layout.fields {
        let (payload,result)=completion.task_completion_payload();
        for (word,value) in [(Ty::TASK_COMPLETION_EXTENT,result.size),(Ty::TASK_COMPLETION_PAYLOAD,payload)] {
            let start=(*offset+word) as usize;
            work_output[start..start+Scalar::U32.size() as usize].copy_from_slice(&value.to_le_bytes());
        }
    }
    if !layout.fields.is_empty() {
        worker.task_completion_writes=Some(InputCompletionWrites{
            input_type:index,output_base:work_output.as_ptr() as usize,positions:Vec::new(),
        });
    }
    let task = Box::into_raw(Box::new(Task {
        handle: None,
        work: Work {
            frame: Some(worker),
            entry,
            cleanup,
            input: bytes,
            output: work_output,
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
            drop(unsafe { Box::from_raw(task) });
            let failure = &start_type.variants[1];
            let code = payload + failure.ty.fields[0].1 as usize;
            result[..4].copy_from_slice(&failure.discriminant.to_le_bytes());
            result[code..code + 4]
                .copy_from_slice(&(error.raw_os_error().unwrap() as u32).to_le_bytes());
            return Ok(());
        }
    };
    unsafe { ptr::addr_of_mut!((*task).handle).write(Some(handle)); }
    result[..4].copy_from_slice(&start_type.variants[0].discriminant.to_le_bytes());
    result[payload..payload + 8].copy_from_slice(&(task as u64).to_le_bytes());
    Ok(())
}

/// An accepted work object stays with the caller through its original write cleanup.
pub(super) unsafe fn submit(frame:&mut Frame,owner:*mut c_void,preparation:*mut c_void)->Result<()> {
    let mut task=unsafe { Box::from_raw(owner.cast::<Task>()) };
    if !preparation.is_null() {
        return Ok(());
    }
    let definitions=frame.definitions.clone();
    let input_type=&definitions.types[task.work.result_type-1];
    let work=&mut task.work;
    let worker=work.frame.as_mut().unwrap();
    let mut selected=Selection::default();
    bind_borrowed_inputs(frame,worker,input_type,&work.input,&mut selected)?;
    unsafe {
        gather(frame,input_type,work.input.as_mut_ptr(),&mut selected,false)?;
        for (ty,address,inout,_) in positions(&definitions.types,input_type,&work.input) {
            if inout { gather(frame,ty,address,&mut selected,false)?; }
        }
        transfer(frame,worker,selected)?;
    }
    let task=Box::into_raw(task);
    unsafe { (*ptr::addr_of!((*task).handle)).as_ref().unwrap().submit() };
    Ok(())
}

/// The task pointer is uniquely owned. Only its native handle is read while work is active.
pub(super) unsafe fn wait(frame: &mut Frame, owner: *mut c_void, completion: *mut u8) -> *mut c_void {
    let task = owner.cast::<Task>();
    drop(unsafe { (&mut *ptr::addr_of_mut!((*task).handle)).take().unwrap() });
    let mut task = unsafe { Box::from_raw(task) };
    let work = &mut task.work;
    let status=match work.completion.take().unwrap() {
        Ok(Completion{selected,status})=>{
            let worker=work.frame.as_mut().unwrap();
            match unsafe { transfer(worker,frame,selected) } {
                Ok(())=>{
                    if let Some(completions)=&mut frame.task_completion_writes {
                        let layout=frame.definitions.types[completions.input_type].arguments.last().unwrap();
                        if layout.fields.iter().any(|(_,offset,_)|
                            completions.output_base+*offset as usize==completion as usize) {
                            if status.is_null() {
                                let types=&worker.definitions.types;
                                completions.positions.extend(completed_positions(types,&types[work.result_type-1],&work.input,&work.output)
                                    .filter(|(ty,_)|ty.contains_dataset()).map(|(ty,address)|(ty.clone(),address)));
                            }
                            if let Some(nested)=worker.task_completion_writes.take() {
                                completions.positions.extend(nested.positions);
                            }
                        }
                    }
                    status
                }
                Err(error)=>work.abandon_result(error,status),
            }
        }
        Err(error)=>error,
    };
    let read=|word:u32| u32::from_le(unsafe {completion.add(word as usize).cast::<u32>().read_unaligned()}) as usize;
    let extent=read(Ty::TASK_COMPLETION_EXTENT);
    let payload=read(Ty::TASK_COMPLETION_PAYLOAD);
    let state=if status.is_null(){Ty::TASK_COMPLETION_SUCCESS}else{Ty::TASK_COMPLETION_FAILED};
    unsafe {
        ptr::copy_nonoverlapping(work.output.as_ptr(),completion.add(payload),extent.min(work.output.len()));
        completion.add(Ty::TASK_COMPLETION_STATUS as usize).cast::<u32>().write_unaligned(state.to_le());
    }
    status
}
