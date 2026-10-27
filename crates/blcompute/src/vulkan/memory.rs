//! 原生物理缓冲尺寸与驱动内存堆估计；不以逻辑资源容量代替原生能力或驻留。
use super::*;

#[derive(Clone, Debug)]
pub struct MemoryHeapBudget {
    pub device_local: bool,
    pub size: u64,
    pub budget: u64,
    pub usage: u64,
}
#[derive(Clone, Debug)]
pub struct MemoryBudget {
    pub heaps: Vec<MemoryHeapBudget>,
}

#[repr(C)]
struct BufferProperties {
    kind: u32,
    next: *mut c_void,
    max_size: u64,
}

pub(super) fn max_buffer_size(
    owner: &InstanceOwner,
    physical: VkPhysicalDevice,
    api_version: u32,
) -> Result<Option<u64>> {
    if api_version < ((1 << 22) | (3 << 12)) {
        return Ok(None);
    }
    let mut buffer = BufferProperties {
        kind: 1000413001,
        next: null_mut(),
        max_size: 0,
    };
    let mut properties = VkPhysicalDeviceProperties2 {
        kind: 1000059001,
        next: (&mut buffer as *mut BufferProperties).cast(),
        base: zero(),
    };
    unsafe {
        let get: unsafe extern "system" fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties2) =
            owner
                .library
                .get(owner.instance, c"vkGetPhysicalDeviceProperties2")?;
        get(physical, &mut properties);
    }
    Ok(Some(buffer.max_size))
}

#[repr(C)]
struct ExtensionProperties {
    name: [c_char; 256],
    version: u32,
}
#[repr(C)]
struct MemoryProperties2 {
    kind: u32,
    next: *mut c_void,
    memory: VkPhysicalDeviceMemoryProperties,
}
#[repr(C)]
struct BudgetProperties {
    kind: u32,
    next: *mut c_void,
    budget: [u64; 16],
    usage: [u64; 16],
}
type GetMemoryProperties = unsafe extern "system" fn(VkPhysicalDevice, *mut MemoryProperties2);
pub(super) struct BudgetQuery(GetMemoryProperties);
impl BudgetQuery {
    pub(super) fn load(owner: &InstanceOwner, physical: VkPhysicalDevice) -> Result<Option<Self>> {
        unsafe {
            let enumerate: unsafe extern "system" fn(
                VkPhysicalDevice,
                *const c_char,
                *mut u32,
                *mut ExtensionProperties,
            ) -> i32 = owner
                .library
                .get(owner.instance, c"vkEnumerateDeviceExtensionProperties")?;
            let mut count = 0;
            check(
                enumerate(physical, null(), &mut count, null_mut()),
                "vkEnumerateDeviceExtensionProperties(count)",
            )?;
            let mut extensions: Vec<ExtensionProperties> = (0..count).map(|_| zero()).collect();
            check(
                enumerate(physical, null(), &mut count, extensions.as_mut_ptr()),
                "vkEnumerateDeviceExtensionProperties",
            )?;
            if !extensions[..count as usize]
                .iter()
                .any(|e| CStr::from_ptr(e.name.as_ptr()) == c"VK_EXT_memory_budget")
            {
                return Ok(None);
            }
            Ok(Some(Self(owner.library.get(
                owner.instance,
                c"vkGetPhysicalDeviceMemoryProperties2",
            )?)))
        }
    }
    fn read(&self, physical: VkPhysicalDevice) -> MemoryBudget {
        let mut budget = BudgetProperties {
            kind: 1000237000,
            next: null_mut(),
            budget: [0; 16],
            usage: [0; 16],
        };
        let mut properties = MemoryProperties2 {
            kind: 1000059006,
            next: (&mut budget as *mut BudgetProperties).cast(),
            memory: zero(),
        };
        unsafe { (self.0)(physical, &mut properties) };
        MemoryBudget {
            heaps: properties.memory.memoryHeaps[..properties.memory.memoryHeapCount as usize]
                .iter()
                .enumerate()
                .map(|(i, heap)| MemoryHeapBudget {
                    device_local: heap.flags & 1 != 0,
                    size: heap.size,
                    budget: budget.budget[i],
                    usage: budget.usage[i],
                })
                .collect(),
        }
    }
}
impl Device {
    /// VK_EXT_memory_budget 当前进程分配估计与动态预算；无此驱动能力时显式返回 None。
    /// usage 不保证物理驻留，也不是整张显卡或引擎逻辑资源容量。
    pub fn memory_budget(&self) -> Option<MemoryBudget> {
        self.inner
            .memory_budget
            .as_ref()
            .map(|query| query.read(self.inner.physical))
    }
}
