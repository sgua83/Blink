# Vulkan 数据分配与进程预算观察

## 数据分页与原分配区间

`blcompute::vulkan::GpuDataset::memory()`及`vulkan::ReadDataset::memory()`同步返回`GpuDatasetMemory`，只读取同一原GpuStorage的直接buffer及其Range／Block元数据。返回值不持有资源，不创建GPUbuffer／页表，不等待或读回载荷；被冻结的多个只读别名交付同一buffer，而不是另一份分配。

| 字段 | 来源与单位 |
|---|---|
| device | 原逻辑VkDevice地址，限定本次buffer／memory句柄作用域；不代表持久设备身份 |
| logical_bytes | 原count乘stride，含记录布局／步长填充，单位字节 |
| buffers | 原绑定表、全部实际数据页及存在的原生实例指针buffer；不递归遍历其他资源 |

每项`GpuBufferMemory`说明原buffer与其实际绑定：

| 字段 | 来源与语义 |
|---|---|
| role | BindingTable包含16字节绑定头与页地址；DataPage携带原页索引、逻辑字节偏移及覆盖；InstancePointers为原Vulkan实例地址数组 |
| buffer／buffer_bytes | VkBuffer及实际创建请求大小；字节数包含原分配器的末word填充，可能大于页的逻辑覆盖 |
| memory／memory_offset | 实际vkBindBufferMemory使用的VkDeviceMemory及字节偏移 |
| range_bytes | 原Range大小，即该buffer的VkMemoryRequirements.size；不同于创建请求大小及整块分配大小 |
| allocation_bytes | 同一原Block成功vkAllocateMemory时的allocationSize，不是当前空闲容量或本buffer独占的整块内存 |
| memory_type／memory_properties／heap_index | 该次分配的memoryTypeIndex及同一原设备的VkMemoryType属性／heapIndex |

原`bytes()`与`page_count()`继续描述逻辑记录跨度和实际数据页数量；空数据没有payload页，但原绑定表仍存在。原`allocated_bytes()`保持为所有直接buffer的需求区间大小之和，含绑定表与实际实例指针buffer；它不是VkDeviceMemory块的去重总额。逻辑页的最后几个字节、buffer请求末word填充、Vulkan需求对齐和原块中其他区间分别表达。

一个Block可被多个buffer／数据对象共享，冻结别名还可能重复引用同一buffer。跨对象分析须在相关原所有者仍存活的同一观察范围内，区分逻辑根、buffer、绑定区间与按device／memory识别的分配对象；不能逐项累加`allocation_bytes`当独占显存，也不能把这些句柄保存为长期身份。查询结果是当时元数据，原所有者结束后句柄可以复用，不授予地址访问或延长代码／数据寿命。

本查询不覆盖view引用的子图、实例持有的BLAS／TLAS、纹理、管线、临时传输／scratch或整逻辑设备的全部内存，也不证明物理驻留。最后真实GPU消费者完成前由原GpuDataset／Execution持有存储；Range及最后Block沿原释放链结束，报告没有第二份所有权或常驻统计。完整资源根和AI／B／C观察入口仍待实现。

标准字段分别依据[VkMemoryAllocateInfo](https://docs.vulkan.org/refpages/latest/refpages/source/VkMemoryAllocateInfo.html)与[VkMemoryRequirements](https://docs.vulkan.org/refpages/latest/refpages/source/VkMemoryRequirements.html)，原分配对象大小与资源需求不可互换。

## 原view来源与存储身份

GpuDataset的layout／stride借用原GpuStorage布局；view_bindings从原Views枚举record、记录内字节offset和原来源GpuDataset。来源可继续查询memory及其实际子边；查询不下载载荷、猜活动变体或建立另一份图。same_storage仅在当前借用内比较同一原GpuStorage，冻结别名仍是同一存储，不同对象即使共享VkDeviceMemory块也不会因此相同。多个边指向同一来源时，不能将其直接buffer重复当作多份分配；对象／buffer／Range／Block仍按真实层次分别解释。

ReadDataset沿原对象Deref使用来源接口；原memory快照及内存口径不变。绑定完成关系由last_use另行查询，只报告该绑定实际保留的原点，不表示全存储所有别名已经完成。它沿原Execution观察并清理已完成资源，不等待或维护内存／完成镜像，详细语义见[GPU来源与绑定保留点](BLanguage.md#gpu布局view来源与绑定保留点)。

## 进程堆使用估计与动态预算

`blcompute::vulkan::Device::memory_budget()` 返回 `Option<MemoryBudget>`。各 `MemoryHeapBudget` 包含 `device_local`、物理堆 `size`、当前 `budget` 和 `usage`，单位均为字节。设备所有者枚举并启用驱动实际支持的 `VK_EXT_memory_budget`，查询使用同一物理设备的 `vkGetPhysicalDeviceMemoryProperties2`；设备与 Vulkan 实例保持原有生命周期。

`usage` 是驱动对当前进程从该堆分配内存的估计，不是整卡占用、保证物理驻留或引擎逻辑资源容量。`budget` 是当前进程的动态使用预算，会受系统其他活动影响，不能当作恒定显存上限。来自 [Vulkan 的预算定义](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceMemoryBudgetPropertiesEXT.html)。

每次调用读取当前驱动信息，不增加资源分配账本、镜像计数或后台线程。消费者按真实观测需要查询，例如 BLEngine 原生窗口每约半秒汇总设备本地堆的 `usage`／`budget`。没有该驱动能力时显式返回 `None`，窗口显示不可用，不用逻辑资源容量代替。
