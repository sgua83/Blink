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
| Device::instance_data_from(data,models) | 以已有 B 布局 Dataset 的 CPU 矩阵创建同一原生输入，记录数量必须对应实际模型；原生尾部仍只由平台填写 |
| unsafe Device::top_level_device_read(data) | 以冻结输入的原生只读绑定进入同一top_level_device构建；存储可供其他真实计算消费者引用 |
| unsafe Device::top_level_device(data) | 消费GpuDataset所有权，读取实际GPU页异步提交TLAS构建；输入须属于该设备并持有原生实例所有权 |
| Device::download(data) | 返回矩阵等逻辑数据；原生实例的 48..64 字节清零，不将设备地址序列化为可执行资源 |

实例输入以已有 GpuDataset 持有数据页、原生地址数组及唯一 BLAS 引用集合。框架填入 mask／flags／BLAS 引用，B 只写 matrix。48 字节矩阵与 16 字节原生尾部对应 [Vulkan 实例布局](https://docs.vulkan.org/refpages/latest/refpages/source/VkAccelerationStructureInstanceKHR.html)。构建使用 [arrayOfPointers](https://docs.vulkan.org/refpages/latest/refpages/source/VkAccelerationStructureGeometryInstancesDataKHR.html)，地址数组按真实数据页生成，允许实例跨多个数据页，无需逐记录复制命令或矩阵读回。

唯一输入入口 top_level_device 沿用 GpuDataset 的真实最后使用点，并消费其唯一所有权。页表、实例页、地址数组交给本次构建完成点；不在函数内等待GPU，也不下载或重新打包矩阵。原队列记录Compute／Transfer到构建的访问依赖，后续查询记录构建到计算的访问依赖。API返回不表示本次构建已经完成，GPU自主调度与refit仍未实现。

TLAS自提交起持有唯一BLAS引用集合，构建持有的输入引用与scratch在真实构建完成后释放；TLAS随真正查询它的执行存活。不维护第二份模型计数或跨操作注册表。普通数据集的原始填充不能伪造原生所有权，跨设备模型／输入在资源边界直接报告。

## unsafe 的实际含义

最终 3×4 矩阵必须有限、线性部分可逆；可逆条件来自 [Vulkan 实例矩阵要求](https://docs.vulkan.org/refpages/latest/refpages/source/VkTransformMatrixKHR.html)。调用者还须保持原生尾部与所持有模型对应，只修改 B 逻辑矩阵字段，不能经 bulk copy／sort／compact 等换入未持有模型的尾部。

这是原生使用前提，不是新增每帧校验或读回关卡。接口目前明确为 unsafe，不宣称编译器已证明任意运行数据的有限、可逆或尾部关系，也不自动切换引擎已有安全入口。

## 原生执行记录（2026-10-25）

AMD Radeon RX 9070 GRE 上，0／37／65537 条普通填充记录在 x64 与 GPU 实际计算，结果相同且尾部哨兵保持；嵌套泛型记录字段更新也相同。stride=32／52 在前端报 B241，268435472 字节单条记录在 GPU 目标生成时拒绝。

0／37／4194305 个简单三角形实例经过 GPU 写矩阵、直接 TLAS 构建和硬件查询。最大输入两页，输入自身 Vulkan 分配 301989976 字节，含页表与地址数组，不含 BLAS／TLAS。外部模型及输入释放后，首／中／末仍命中正确实例和距离；空 TLAS 无命中。普通输入与异设备模型被资源入口拒绝，原生下载未泄出尾部地址。

这不是同等数量独特复杂模型、完整流送或性能优势的证明。原始日志、逻辑输出及引擎全帧结果位于 E:/BLEngineMeasurements/DeviceInstancesDevelopment。临时消费者和产物完成后清除，不成为常驻测试。

## 异步传输与构建接口

Device::upload／upload_with_views／upload_domains返回已提交的真实GpuDataset；Device::zeroed用一次提交填充实际数据页。每个上传操作持有本次全部分块暂存到设备复制完成，冻结view仍持有实际子资源；同一队列后续program直接消费这些真实依赖，调用方无需在中间加入CPU等待。

unsafe bottom_level复制CPU几何到自有传输资源后提交构建；unsafe top_level同样复制原生实例输入，并持有去重后的BLAS。BottomLevel／Acceleration提供is_complete与wait，含义为该结构自身的构建完成，不代表未来的全部查询已经结束。没有消费者的最后析构会等待尚未结束的真实工作；显式download也等待实际结果。纹理上传仍同步，本能力只覆盖已有单设备／单队列缓冲区和AS执行。

本轮原生执行在AMD Radeon RX 9070 GRE的真实队列前放置一个临时未释放事件：两份BLAS、CPU TLAS、GPU矩阵程序、设备TLAS及射线查询均在事件释放前返回，完成观察为pending。原始BLAS／TLAS持有者和查询输入提前退出后，放行队列，CPU／GPU实例的三个命中和一个未命中完全相同，命中实例为0／1／1、primitive为0／0／1、距离为1／1.5／1.5。

同一受控队列中，16777219条float4记录的上传与清零各跨两个物理页；原始叶、父view和清零数据持有者在完成前退出，最终四个跨页位置的实际读取与预期严格相同。空TLAS的wait／is_complete及最后设备所有者释放也已确认。原日志在E:/BLEngineMeasurements/AsyncResourcesDevelopment/async-native.out.log；临时事件入口与消费者均在验证后删除，不成为正式API或固定关卡。

## 多程序的同一设备与BLAS记录范围（2026-10-26）

Device::for_programs(programs)在创建一个实际逻辑设备时，合并各B程序所需的光线查询、排序、maximal reconvergence、纹理数组及命中顶点读取能力。with_presentation_programs额外声明原生呈现；原for_program／with_presentation复用同一实现。能力来自真实程序，不在执行时更换设备或增加另一套调度。后续程序仍由既有逻辑设备／能力边界负责；这不把单队列变成多队列或设备自主任务。

BottomLevel::primitive_count返回该BLAS实际构建输入的u32 primitive范围。数值由平台构建所有者保存，用于原AS的primitive号与外部属性记录之间的寻址关系；不是GPU Count、原生构建完成或查询完成。它不读回设备计数，不增加引擎侧的计数镜像、扫描或等待，is_complete／wait仍保持原来含义。

本轮由BLEngine独立GPU场景与渲染消费者推动：同一for_programs设备上的网格查询与两张纹理的完整4K／内部8K渲染已实际执行；原with_presentation入口也经原生4K窗口与转动使用。源材质／法线／UV按真实消费者补充时，直接用驻留BLAS的primitive范围交接。原始日志、完整图及数据位于E:/BLEngineMeasurements/GpuScene。当前没有新增B语法、第三方依赖或常驻测试，不称全部引擎或多队列并行已经完成。

## 共享不可变的实际构建输入（2026-10-26）

BLEngine 的刚体表面对应需要此前实例矩阵。instance_data_from 接收由真实 B 类型创建并填好逻辑矩阵的 CPU Dataset，复用 instance_data 的原生尾部、模型检查、分页地址及传输；instance_data 原入口直接委托它，没有第二份设备矩阵。

调用者可 freeze 同一原生输入，再交给 unsafe top_level_device_read。只读绑定在真实构建完成点持有该 ReadDataset；显式场景或计算消费者按自己的使用期限继续持有同一存储。冻结输入保持其原生产完成点，不反向改成当前构建的最后使用点，因此构建对输入的持有不会形成引用环。后续 view 提交仍由原执行框架持有真实读取资源，不新增全局历史、资源注册表或额外完成状态。

未请求其他消费者时，原 top_level_device 仍在构建完成后释放唯一输入。64 字节原生记录与矩阵／模型／设备前提不变；只读共享不授予修改原生尾部、伪造模型引用、跨设备使用或任意变换的安全证明。此项为执行框架能力补齐，语言语法、IR 和数学算法没有改动。

## 冻结dataset的直接原生绑定（2026-10-26）

ReadDataset::binding返回可直接放入原vulkan::Bindings.datasets的GpuDataset句柄。它只引用原冻结存储，不创建GPU记录、页表、view头或传输；同一来源可以在多个read参数／程序中使用。原生提交检查B声明的真实权限，冻结来源只接受Access::Read，不能因句柄类型仍为GpuDataset而改写底层页。

绑定未消费时没有本次最后使用点；提交与下载从原来源取得生产依赖，提交后才由绑定记录本次实际执行。退出沿原Execution.discarded存储的Owned／Read两种所有权形态交接，不增加第二个执行或完成服务。已完成的共享输入可以直接再次使用，binding再freeze取得原冻结来源，不累积句柄包装或令生产执行持有自身。

top_level_device_read也复用此路径，AS不再保留独立的_read_inputs持有字段和私有共享构建链。存储只在最后实际消费者完成后释放；普通唯一输入仍按同一构建寿命退出。原生matrix／BLAS／device约束与unsafe前提保持。

BLEngine据此删除仅为共享绑定存在的相机、实例元数据和运动输入引用包装。视图引用依然用于真正的嵌套数据关系；本项不是禁止view，也没有复制BLCamera／世界矩阵或增加逐帧校验。Release原生边界、跨页与生命周期记录见[问题列表](Issues.md#blengine提出的原生只读dataset直接绑定缺口2026-10-26)。

## 显式可更新构建与新版本 refit（2026-10-27）

| 构建入口 | 对应更新入口 | 输入与所有权 |
|---|---|---|
| unsafe bottom_level_updatable(positions,indices) | unsafe refit_bottom_level(&source,positions,indices) | CPU 顶点与索引复制到本次原生输入；索引内容／顺序和活动状态保持，顶点有限 |
| unsafe top_level_updatable(instances) | unsafe refit_top_level(&source,instances) | 本次实际 RayInstance 变换和 BLAS；矩阵有限可逆，实例活动状态保持 |
| unsafe top_level_device_updatable(data) | unsafe refit_top_level_device(&source,data) | 原 B 原生实例数据、页内地址及 BLAS；维持 top_level_device 的矩阵及原生尾部前提 |

原 bottom_level／top_level／top_level_device／top_level_device_read 仍为原静态构建，未声明 ALLOW_UPDATE 的来源不能进入 refit。同一逻辑设备、原数量和三角形 maxVertex 由唯一原生描述负责；不同数量或顶点范围属于不同构建形状。GPU 只读实例输入直接用 `frozen.binding()` 交给可更新构建／refit，不再增加专用读取包装；同形 CPU 与 GPU 输入可在更新中交接。

新入口复用 build_acceleration，source 为原 AS，target 为独立新 AS，选择真实 UPDATE 与 updateScratchSize；原静态入口继续 BUILD 和 buildScratchSize，build flags 在后续更新中保持。source 本体而非仅其旧完成点由本次原 Execution 持有，更新完成后归还；新 TLAS 唯一存储持有本次真实 BLAS 集合，旧查询保留旧集合。没有主机等待才能开始更新的额外前置，也没有原地改写共享资源、全局版本表、自动重建或后台回收。

`is_complete`／`wait` 表示该结果自身的构建／更新完成；查询的使用期限沿原提交持有，源 CPU 数组在 API 已复制后可退出。构建完成后不再保留顶点／索引的 CPU 镜像。实现不自动证明所有输入的有限性、矩阵可逆性或相同索引内容；这些 unsafe 条件与真正原生形状／设备边界分别负责。

原生记录见 Issues 的 C-003／C-022.4 refit：分别记录旧新快照、连续更新、CPU／GPU 输入交接、实际 pending 中的来源退出及查询结果；构建与遍历性能、峰值资源、完整引擎采用不由接口实现代替。GPU 几何输入见下节，原地更新、通用拓扑修改、压缩／重排和完整流送继续开放。

## GPU 顶点与索引直接构建／更新 BLAS（2026-10-27）

| 静态构建 | 可更新构建 | 新版本更新 |
|---|---|---|
| unsafe bottom_level_device(vertices,triangles) | unsafe bottom_level_device_updatable(vertices,triangles) | unsafe refit_bottom_level_device(&source,vertices,triangles) |

两个输入都是本逻辑设备的 GpuDataset，可直接传冻结来源的 binding()。顶点前三个分量为 offset 0／4／8 的 f32，按真实记录 stride 读取，允许后续字段；三角形为紧密三个 u32，stride 12。`struct Triangle {array<u32,3> indices;};` 可表达该索引布局，uint3 的 stride 16 不等价。入口消费完整记录数，不隐式读取有效前缀；全部记录已初始化、位置有限及索引有效是 unsafe 前提。更新还要求原索引内容／顺序、活动状态、原数量及 maxVertex；同形 CPU／GPU 输入可以交叉更新，顶点 stride 可以不同。

GPU 计算结果按真实物理页沿原传输提交复制到具备 AS 输入用途的连续存储，保留逻辑顺序及 primitive 编号。没有 CPU 几何读回、逐三角形主机调用、额外调度或所有普通分配用途扩张。源数据通过原最后使用点持有到复制完成，输入／scratch 与 refit source 通过原 AS Execution 持有到构建／更新完成；旧新查询各消费自己的真实快照。输出仍为原 BottomLevel，不增加另一份几何或场景权威。

两 GPU 常规构建／交叉 refit、65 次连续更新、共享来源退出、真正 pending 中的资源交接与 256 MiB 顶点分页已有本轮原生记录；RX 9070 GRE 还实际读取命中顶点。两千多万索引压力的后续诊断已确认原 AS 请求超出实际 maxBufferSize；此前独显创建／查询成功只保留为原始观测，不构成合法容量证明。容量内的核显大 AS 仍有设备丢失，完整压力项保持开放，见下节及 Issues。

## 原生物理缓冲尺寸（2026-10-27）

逻辑设备取得其实际可用的 maxBufferSize，原 Allocation 入口在创建物理缓冲前报告超出该属性的请求，返回请求字节、设备值及 usage。此处使用 Vulkan 对 VkBuffer 的尺寸要求，不以总显存、预算、记录数或固定 2 GiB 替代。Vulkan 1.2 未取得该核心属性时保留未知，不增加替代属性或提升最低版本。CPU 存储和 GPU 多页逻辑域继续沿原所有权运行，详见 [架构中的物理缓冲与逻辑域](Architecture.md#原生缓冲尺寸与逻辑数据域2026-10-27)。

AS 原生大小查询若要求超出实际缓冲能力的连续存储／输入／scratch，当前接口明确不支持该形状；不会拆分 geometry、缩小规模或更换 primitive 编号。本轮 22369623 三角形在两 GPU 上均明确报告不支持，错误后同设备继续构建和查询正常；其完整 AS 能力没有交付。合法尺寸范围的独显八百万三角形构建／更新／旧新查询通过，核显同规模 CPU 和 GPU 输入仍有设备丢失与 Watchdog 141，单独保留。两 GPU 的 4362076160 字节／17 页数据完整逐记录核对通过，不能把 AS 的单缓冲限制扩大成 B 数据域限制。
