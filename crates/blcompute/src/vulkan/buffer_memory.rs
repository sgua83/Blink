//! 同一逻辑设备的缓冲区物理内存；区间随实际 Allocation 释放，不保留空闲块。
use super::*;
use std::{cell::RefCell, collections::btree_map::Entry, ops::Range as ByteRange, rc::Weak};

// 小缓冲区的物理分块政策，不是资源容量或语言边界。
const BLOCK_BYTES: u64 = 1024 * 1024;

#[repr(C)]
pub(super) struct BufferInfo {
    kind: u32,
    next: *const c_void,
    buffer: VkBuffer,
}
#[repr(C)]
pub(super) struct Requirements2 {
    kind: u32,
    next: *mut c_void,
    memory: VkMemoryRequirements,
}
#[repr(C)]
struct DedicatedRequirements {
    kind: u32,
    next: *mut c_void,
    preferred: u32,
    required: u32,
}
#[repr(C)]
struct DedicatedInfo {
    kind: u32,
    next: *const c_void,
    image: u64,
    buffer: VkBuffer,
}
pub(super) fn requirements(inner: &Inner, buffer: VkBuffer) -> (VkMemoryRequirements, bool) {
    let input = BufferInfo {
        kind: 1000146000,
        next: null(),
        buffer,
    };
    let mut dedicated = DedicatedRequirements {
        kind: 1000127000,
        next: null_mut(),
        preferred: 0,
        required: 0,
    };
    let mut output = Requirements2 {
        kind: 1000146003,
        next: (&mut dedicated as *mut DedicatedRequirements).cast(),
        memory: zero(),
    };
    unsafe {
        (inner.functions.vkGetBufferMemoryRequirements2)(inner.owner.device, &input, &mut output);
    }
    (output.memory, dedicated.required != 0)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Class {
    memory_type: u32,
    address: bool,
    mapped: bool,
}
#[derive(Default)]
pub(super) struct Pool {
    blocks: BTreeMap<Class, Vec<Weak<Block>>>,
}
impl Pool {
    fn remove(&mut self, block: &Block) {
        if let Entry::Occupied(mut class) = self.blocks.entry(block.class) {
            let blocks = class.get_mut();
            if let Some(index) = blocks.iter().position(|entry| std::ptr::eq(entry.as_ptr(), block)) {
                blocks.remove(index);
            }
            if blocks.is_empty() {
                class.remove();
            }
        }
    }
}
struct Block {
    owner: Rc<DeviceOwner>,
    pool_owner: Weak<Inner>,
    memory: VkDeviceMemory,
    size: u64,
    class: Class,
    mapped: *mut u8,
    free: RefCell<Vec<ByteRange<u64>>>,
    free_memory: unsafe extern "system" fn(VkDevice, VkDeviceMemory, *const c_void),
    unmap: unsafe extern "system" fn(VkDevice, VkDeviceMemory),
}
impl Block {
    fn new(inner: &Inner, class: Class, size: u64, dedicated_buffer: VkBuffer) -> Result<Self> {
        let dedicated = DedicatedInfo {
            kind: 1000127001,
            next: null(),
            image: 0,
            buffer: dedicated_buffer,
        };
        let mut address = VkMemoryAllocateFlagsInfo {
            sType: 1000060000,
            pNext: if dedicated_buffer != 0 {
                (&dedicated as *const DedicatedInfo).cast()
            } else {
                null()
            },
            flags: 2,
            deviceMask: 0,
        };
        let next = if class.address {
            (&mut address as *mut VkMemoryAllocateFlagsInfo).cast()
        } else if dedicated_buffer != 0 {
            (&dedicated as *const DedicatedInfo).cast()
        } else {
            null()
        };
        let info = VkMemoryAllocateInfo {
            sType: 5,
            pNext: next,
            allocationSize: size,
            memoryTypeIndex: class.memory_type,
        };
        let mut memory = 0;
        let result = unsafe {
            (inner.functions.vkAllocateMemory)(inner.owner.device, &info, null(), &mut memory)
        };
        if result != 0 {
            let ty = inner.memory.memoryTypes[class.memory_type as usize];
            return Err(error(format!(
                "vkAllocateMemory 返回 VkResult {result}：block_bytes={size} memory_type={} properties={:#x} heap={} heap_bytes={}",
                class.memory_type,
                ty.propertyFlags,
                ty.heapIndex,
                inner.memory.memoryHeaps[ty.heapIndex as usize].size
            )));
        }
        let mut block = Self {
            owner: inner.owner.clone(),
            pool_owner: Weak::new(),
            memory,
            size: info.allocationSize,
            class,
            mapped: null_mut(),
            free: RefCell::default(),
            free_memory: inner.functions.vkFreeMemory,
            unmap: inner.functions.vkUnmapMemory,
        };
        if class.mapped {
            let mut pointer = null_mut();
            unsafe {
                check(
                    (inner.functions.vkMapMemory)(
                        inner.owner.device,
                        memory,
                        0,
                        size,
                        0,
                        &mut pointer,
                    ),
                    "vkMapMemory(buffer block)",
                )?;
            }
            block.mapped = pointer.cast();
        }
        Ok(block)
    }
    fn first_range(mut self, size: u64) -> Range {
        if size < self.size {
            *self.free.get_mut() = vec![size..self.size];
        }
        Range {
            block: Rc::new(self),
            offset: 0,
            size,
        }
    }
    fn take(self: &Rc<Self>, size: u64, alignment: u64) -> Option<Range> {
        let mut free = self.free.borrow_mut();
        let (index, offset) = free.iter().enumerate().find_map(|(index, span)| {
            let offset = span.start.div_ceil(alignment) * alignment;
            (offset <= span.end && size <= span.end - offset).then_some((index, offset))
        })?;
        let end = offset + size;
        let span = &mut free[index];
        match (span.start < offset, end < span.end) {
            (true, true) => {
                let right = end..span.end;
                span.end = offset;
                free.insert(index + 1, right);
            }
            (true, false) => span.end = offset,
            (false, true) => span.start = end,
            (false, false) => {
                free.remove(index);
            }
        }
        Some(Range {
            block: self.clone(),
            offset,
            size,
        })
    }
}
impl Drop for Block {
    fn drop(&mut self) {
        if let Some(inner) = self.pool_owner.upgrade() {
            inner.buffer_memory.borrow_mut().remove(self);
        }
        unsafe {
            if !self.mapped.is_null() {
                (self.unmap)(self.owner.device, self.memory);
            }
            (self.free_memory)(self.owner.device, self.memory, null());
        }
    }
}
pub(super) struct Range {
    block: Rc<Block>,
    pub offset: u64,
    size: u64,
}
impl Range {
    pub fn bytes(&self) -> u64 {
        self.size
    }
    pub fn allocation_bytes(&self) -> u64 {
        self.block.size
    }
    pub fn memory_type(&self) -> u32 {
        self.block.class.memory_type
    }
    pub fn memory(&self) -> VkDeviceMemory {
        self.block.memory
    }
    pub fn pointer(&self) -> Option<*mut u8> {
        std::ptr::NonNull::new(self.block.mapped)
            .map(|base| unsafe { base.as_ptr().add(self.offset as usize) })
    }
}
impl Drop for Range {
    fn drop(&mut self) {
        // Only a block with another live range can reuse this interval.
        if Rc::strong_count(&self.block) == 1 {
            return;
        }
        let mut free = self.block.free.borrow_mut();
        let start = self.offset;
        let end = start + self.size;
        let index = free.partition_point(|span| span.start < start);
        let joins_right = free.get(index).is_some_and(|span| span.start == end);
        if index != 0 && free[index - 1].end == start {
            let end = if joins_right { free.remove(index).end } else { end };
            free[index - 1].end = end;
        } else if joins_right {
            free[index].start = start;
        } else {
            free.insert(index, start..end);
        }
    }
}
pub(super) fn allocate(
    inner: &Rc<Inner>,
    required: VkMemoryRequirements,
    dedicated: bool,
    buffer: VkBuffer,
    usage: u32,
    flags: u32,
    alignment: u64,
) -> Result<Range> {
    let alignment = alignment.max(required.alignment);
    let memory = &inner.memory;
    let host_read = usage == 2 && flags & 2 != 0;
    let index = (0..memory.memoryTypeCount)
        .filter(|i| {
            required.memoryTypeBits & (1 << i) != 0
                && memory.memoryTypes[*i as usize].propertyFlags & flags == flags
        })
        .min_by_key(|i| host_read && memory.memoryTypes[*i as usize].propertyFlags & 8 == 0)
        .ok_or_else(|| error(format!("目标设备缺少内存类型 flags={flags:#x}")))?;
    let class = Class {
        memory_type: index,
        address: usage & 0x20000 != 0,
        mapped: flags & 2 != 0,
    };
    if dedicated || required.size >= BLOCK_BYTES {
        let block = Block::new(
            inner,
            class,
            required.size,
            if dedicated { buffer } else { 0 },
        )?;
        return Ok(block.first_range(required.size));
    }
    if let Some(blocks) = inner.buffer_memory.borrow().blocks.get(&class) {
        for block in blocks.iter().filter_map(Weak::upgrade) {
            if let Some(range) = block.take(required.size, alignment) {
                return Ok(range);
            }
        }
    }
    let mut block = Block::new(inner, class, BLOCK_BYTES, 0)?;
    block.pool_owner = Rc::downgrade(inner);
    let range = block.first_range(required.size);
    inner
        .buffer_memory
        .borrow_mut()
        .blocks
        .entry(class)
        .or_default()
        .push(Rc::downgrade(&range.block));
    Ok(range)
}
