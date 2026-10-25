# Blink 记录步长与设备实例输入

本能力由 BLEngine 的 GPU 场景世界变换直接进入 Vulkan 实例构建这一消费者推动。B 表达算法与逻辑字段；blcompute 持有原生元数据、设备存储、BLAS 引用及实际完成后的释放，不将平台地址变成普通 B 数值字段。

## 记录 stride

```c
module scene.device;
struct BLDeviceTransform stride(64) {float3x4 matrix;};
const u32 Bytes=64;
struct Transform<const S:u32> stride(S) {float3x4 matrix;};
struct Nested {Transform<Bytes> item;u32 tag;};
```

stride 放在记录名称及泛型参数之后，接受 u32 字面量、模块 const 名称或记录 const 参数名，不能在括号里直接写任意表达式。自然字段偏移和自然对齐保持不变，stride 指定记录总字节大小及数组／dataset 元素步长；必须容纳自然布局，且为自然对齐的整数倍，否则前端报 B241。没有 stride 时沿用原布局，不改变既有类型 ABI。

float3x4 自然大小为 48、对齐为 16；Transform<64> 大小为 64、对齐仍为 16，Nested 的 tag 位于 64，整体大小为 80。stride(64) 不意味着基地址需要 64 字节对齐。填充没有字段名，不参与普通值或类型化字段赋值；现有 x64／SPIR-V 按逻辑分量写入，不覆盖尾部填充。原始传输、sort／compact 等整条物理记录操作仍有各自的存储语义，不能把填充当作可移植值。

普通记录布局仍使用既有 u32 字节范围。GPU 后端当前按 PAGE_BYTES=256 MiB 的物理页寻址，每条记录完整位于一页，要求单条实际元素能放入一页；SPIR-V 生成与实际 GPU 分配直接报告超出该寻址布局的记录，不以分配循环或无效除数继续执行。该限制来自当前后端 ABI，不能写成 CPU 语言通用容量限制。

## 原生接口与所有权

| API | 语义 |
|---|---|
| Device::instance_data(program,binding,models) | 按具体 B 绑定创建原生实例输入；要求 stride=64、12 个 f32 分量位于 offset 0..44，模型均属于该设备 |
| unsafe Device::top_level_device(data) | 读取实际 GPU 页直接构建 TLAS；输入须为该设备持有原生实例所有权的数据集 |
| Device::download(data) | 返回矩阵等逻辑数据；原生实例的 48..64 字节清零，不将设备地址序列化为可执行资源 |

实例输入以已有 GpuDataset 持有数据页、原生地址数组及唯一 BLAS 引用集合。框架填入 mask／flags／BLAS 引用，B 只写 matrix。48 字节矩阵与 16 字节原生尾部对应 [Vulkan 实例布局](https://docs.vulkan.org/refpages/latest/refpages/source/VkAccelerationStructureInstanceKHR.html)。构建使用 [arrayOfPointers](https://docs.vulkan.org/refpages/latest/refpages/source/VkAccelerationStructureGeometryInstancesDataKHR.html)，地址数组按真实数据页生成，允许实例跨多个数据页，无需逐记录复制命令或矩阵读回。

提交沿用 GpuDataset 的真实最后使用点；释放输入不会提前释放待执行的页面或模型。构建在同一实际队列中消费生产者的 Compute／Transfer 访问依赖，构建后仍等待本次 AS 完成。去掉矩阵 GPU→CPU→GPU 搬运不等于实现全部异步 AS、GPU 自主调度或 refit。

TLAS 构建成功后持有唯一 BLAS 引用集合，源输入可释放；TLAS 随真正查询它的执行存活。不维护第二份模型计数或跨操作注册表。普通数据集的原始填充不能伪造原生所有权，跨设备模型／输入在资源边界直接报告。

## unsafe 的实际含义

最终 3×4 矩阵必须有限、线性部分可逆；可逆条件来自 [Vulkan 实例矩阵要求](https://docs.vulkan.org/refpages/latest/refpages/source/VkTransformMatrixKHR.html)。调用者还须保持原生尾部与所持有模型对应，只修改 B 逻辑矩阵字段，不能经 bulk copy／sort／compact 等换入未持有模型的尾部。

这是原生使用前提，不是新增每帧校验或读回关卡。接口目前明确为 unsafe，不宣称编译器已证明任意运行数据的有限、可逆或尾部关系，也不自动切换引擎已有安全入口。

## 原生执行记录（2026-10-25）

AMD Radeon RX 9070 GRE 上，0／37／65537 条普通填充记录在 x64 与 GPU 实际计算，结果相同且尾部哨兵保持；嵌套泛型记录字段更新也相同。stride=32／52 在前端报 B241，268435472 字节单条记录在 GPU 目标生成时拒绝。

0／37／4194305 个简单三角形实例经过 GPU 写矩阵、直接 TLAS 构建和硬件查询。最大输入两页，输入自身 Vulkan 分配 301989976 字节，含页表与地址数组，不含 BLAS／TLAS。外部模型及输入释放后，首／中／末仍命中正确实例和距离；空 TLAS 无命中。普通输入与异设备模型被资源入口拒绝，原生下载未泄出尾部地址。

这不是同等数量独特复杂模型、完整流送或性能优势的证明。原始日志、逻辑输出及引擎全帧结果位于 E:/BLEngineMeasurements/DeviceInstancesDevelopment。临时消费者和产物完成后清除，不成为常驻测试。
