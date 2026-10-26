# Vulkan 进程内存观测

`blcompute::vulkan::Device::memory_budget()` 返回 `Option<MemoryBudget>`。各 `MemoryHeapBudget` 包含 `device_local`、物理堆 `size`、当前 `budget` 和 `usage`，单位均为字节。设备所有者枚举并启用驱动实际支持的 `VK_EXT_memory_budget`，查询使用同一物理设备的 `vkGetPhysicalDeviceMemoryProperties2`；设备与 Vulkan 实例保持原有生命周期。

`usage` 是驱动对当前进程从该堆分配内存的估计，不是整卡占用、保证物理驻留或引擎逻辑资源容量。`budget` 是当前进程的动态使用预算，会受系统其他活动影响，不能当作恒定显存上限。来自 [Vulkan 的预算定义](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceMemoryBudgetPropertiesEXT.html)。

每次调用读取当前驱动信息，不增加资源分配账本、镜像计数或后台线程。消费者按真实观测需要查询，例如 BLEngine 原生窗口每约半秒汇总设备本地堆的 `usage`／`budget`。没有该驱动能力时显式返回 `None`，窗口显示不可用，不用逻辑资源容量代替。

2026-10-26，本机 AMD Radeon RX 9070 GRE 的正式 Release BLEngine 窗口已实际消费：3840×2160 输出、内部 7680×4320，显示进程设备本地堆估计约 0.75 GiB、动态预算约 11.08 GiB。此数值属于当时的独立单字场景，不外推为完整编辑器、其他场景或性能目标。关联能力记录 C-018；原生进程、构建档位和日志在 `E:/BLEngineMeasurements/Reservoir/visible-hud-process.json`、`visible-hud.log`。
