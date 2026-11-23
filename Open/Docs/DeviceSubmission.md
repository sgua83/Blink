# 异步结果与原生完成观测

`Submission::detach()`不等待本次GPU执行，返回原 `DomainBindings<GpuDataset>`、原AS绑定和 `GpuCompletion`。没有复制数据、资源或完成事实；结果继续携带原来的真实最后使用点，后续程序、下载和释放沿原执行依赖处理。计算提交没有派发或时间戳命令时，以原队列的零批次提交取得真实fence。该fence覆盖同队列前序，原输入保留各自的最后使用点。需要写时间戳的零容量阶段仍录制这些命令。[Vulkan队列提交](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html#devsandqueues-queue-submission) [Fence信号顺序](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-fences-signaling)

`GpuCompletion::is_complete()`查询同一原生执行的实际完成情况。原Completion的vkWaitForFences等待与vkGetFenceStatus查询只记录0或DEVICE_LOST；查询NOT_READY返回false并保持未知。其他等待／查询错误原样返回，不记录终态，继续由原执行持有资源；调用者下一次显式wait／query沿原fence观察，不自动重试。等待返回范围见[Khronos vkWaitForFences](https://docs.vulkan.org/refpages/latest/refpages/source/vkWaitForFences.html)。已记录的终态查询不再次访问原fence，原错误继续交回，DEVICE_LOST不作为true；`wait()`等待完成并返回`Option<GpuTimings>`。`GpuProgram`及`GpuTemplate`默认不采集，通过`with_timestamps(true)?`显式开启，通过`with_timestamps(false)?`关闭。没有采集或没有执行阶段返回`None`；`Some`中的零值仍是实际查询结果。开启时计算队列必须真实支持时间戳，未开启的计算不以计时能力为前提。已请求的查询池在执行期间由原`ExecutionResources`持有，观察者也持有原池，因此后一个提交完成并回收此前命令资源后，前一个观察者仍可读取自己的时间戳。此接口不是引擎帧号、计数镜像、另一个调度器或新的完成服务。

`GpuCompletion::wait_timestamps()`使用同一完成点及原查询池，显式请求采集后返回`Option<GpuTimestamps>`。`device`和`sequence`采用原逻辑设备和提交序号，作用域与`GpuDatasetUse`相同。`program()`借用本次实际执行计划；`stages()`按原录制顺序迭代，每项提供从零开始的`step`、`stage`及原始`start_ticks`、`end_ticks`。`stage`索引同一计划的`stages()`。`period_ns`和`valid_bits`来自原设备周期和计算队列，调用者可据此解释tick和回绕。阶段区间包括该阶段原录制的辅助分派；时间基准属于实际设备提交。原`wait()`与`GpuTimings`继续交付毫秒结果。[Vulkan设备时钟周期](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceLimits.html) [Vulkan队列时间戳有效位](https://docs.vulkan.org/refpages/latest/refpages/source/VkQueueFamilyProperties.html)

观察者可以先于结果退出。结果和AS的物理使用期仍由原资源／Execution管理，不以观察者退出或CPU tick结束作为GPU完成。最后真实资源所有者退出时仍履行原有完成后释放职责；异步不等于允许提前销毁设备资源。`Submission::wait()`借用同一完成点与查询池等待，成功交回Completed，并仅清除本次提交为实际绑定记录的使用点；失败交回(error, submission)，同一提交继续持有原绑定、AS、查询池与真实最后使用点。调用者决定继续持有或明确结束它，错误不保证设备数据可用。原Execution::wait／is_complete在同一Completion已记录0或VK_ERROR_DEVICE_LOST时，沿原release_known结束本执行的命令、scratch、绑定参数和依赖持有，再原样交回原Result；已终止执行的中间资源不因失败观察者继续存在而延长。DEVICE_LOST仍作为错误返回，不发布成功，也不改变原绑定、AS及查询池的观察所有权。GpuDataset或Structure交出原存储时，已知原执行终止则直接释放，仍在途的存储交回真实消费者。ReadDataset、AS及其BLAS来源通过原Rc维持真实拥有关系，最后一个拥有者退出前底层资源保持；查询仍持有AS、其他TLAS或调用者仍持有BLAS时，不以某个构建点结束提前释放共享资源，不添加等待或第二释放链。其他未确认终止的错误不提前释放在途资源。终态资源规则见[Khronos设备丢失语义](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html#devsandqueues-lost-device)。执行器不自动重放。`into_bindings()`及原按绑定交接继续存在。

`vkQueueSubmit`、Fence等待或查询、GPU时间戳查询及交换链创建、获取、呈现报告`VK_ERROR_DEVICE_LOST`时，原逻辑设备先结束原生资源使用，再返回首次报告丢失的API错误。设备等待本身报告丢失时也关闭同一提交序号。后继提交、数据读回、尚未读取的时间戳及交换链创建和获取返回设备丢失。已确认的Fence保留原观察结果，未确认的点从已退役设备取得设备丢失结果。普通Fence查询和未就绪结果保持原非等待路径。[Vulkan设备丢失语义](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html#devsandqueues-lost-device)

计算、显式汇合、GPU数据到图像复制和AS构建按同队列提交序号观察实际前序。查询遇到第一个未就绪点后停止原生轮询，仍读取其余前序已经记录的终态。任何依赖报告失败时返回该错误。已接受的未知完成继续由原执行持有，沿实际队列和Fence结束。[Vulkan Fence状态](https://docs.vulkan.org/refpages/latest/refpages/source/vkGetFenceStatus.html)

从 GPU 数据集复制到纹理或交换链时，原执行直接持有每个实际录入 `vkCmdCopyBufferToImage` 的源页。同一复制记录多次引用一页时共用原页。逻辑绑定的最后使用点更新为本次复制，供后续读回和改写沿用。绑定退出时，绑定表、未复制页和视图来源只沿较早尚在途的实际绑定执行及原共享所有权存活。仅剩本次复制消费的页在复制完成时释放。提交拒绝时源保持原使用点。[Khronos缓冲到图像复制](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdCopyBufferToImage.html)

`Device::download`、`download_domains`和二维／三维纹理下载是同步接口。`download_domains`先核对所有根的逻辑设备归属，再按绑定顺序读回。各源在读取子视图、分配暂存或录制复制之前，直接交回自身已经观察到的同队列前序错误。零页数据集也读取这一终态。多个只读绑定可以复用同一冻结存储的CPU载荷，各绑定仍消费自己的最后使用点。实际拷贝提交成功后等待本次原生fence，将终态交给尚未确认的前序并释放相关资源。CPU数据在读回成功后交回。拥有式域组逐根接手独占存储的主机布局和视图绑定索引，原GPU页、绑定表和设备视图依赖沿实际完成点释放。借用根由调用方继续持有，冻结来源继续共享CPU读回结果。

BLEngine的 `submit_to` 直接消费这些绑定，将同一工作图像和输出交回原所有者；下一次提交沿原依赖使用它们，无需CPU等待，也不为每个在途执行复制一份内部8K工作图像。渲染算法及阶段仍由B表达，实际执行、Vulkan内存依赖与寿命仍归Blink。


通用完成交接与B应用语言能力分别定义；B主流程的动态dataset和同步CPU／GPU组织已在规范中支持，设备自主调度、多队列及完整引擎能力仍未完成。

## 交换链呈现与原显示存储
`Swapchain::try_present(&mut GpuDataset)`返回`Poll<PresentStatus>`。交换链自己的上一份acquire信号量消费尚未完成，或零超时`vkAcquireNextImageKHR`报告NOT_READY／TIMEOUT时，返回Pending，不提交拷贝、不修改输入最后使用点。消费者保留当前完整显示数据，继续处理输入或其他视图；Pending是原生可用性，不是错误或异常重试。

取得实际图像后，沿原Commands记录源生产者到传输的依赖、完整分页面拷贝及呈现布局转换，提交后交给原呈现执行`ExecutionResources::Present`。复制一经队列接受，`try_present`立即将其交给源GpuDataset作为最后使用点，再调用`vkQueuePresentKHR`。呈现报错也保留该点，后续计算改写、下载和退出沿原只读依赖处理。冻结数据的只读binding仍保持原物理所有者。复制提交被拒绝时，源保留此前的最后使用点。

Swapchain持有本次已取得的图像和acquire信号，拷贝由队列接受后接手实际信号等待。正常呈现的拷贝完成后才再次获取图像，呈现等待信号量仍按实际交换链图像分配。Ready表示拷贝和呈现已排入队列，返回Optimal／Suboptimal事实，不表示源拷贝已完成或显示器已扫描输出。信号量消费与上屏完成是不同事实，参见[Khronos交换链信号量复用](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html)。

`present(&GpuDataset)`与`try_present`可在同一交换链交替使用。同步入口在返回前沿原执行结束已提交复制对源的借用，即使呈现失败或首次等待返回非终态错误。呈现失败时交回原呈现错误，否则交回复制等待的结果。异步入口交回普通呈现错误时无需等待源复制；驱动的呈现调用本身可能阻塞，设备丢失仍沿原逻辑设备退役。[Khronos队列呈现](https://docs.vulkan.org/refpages/latest/refpages/source/vkQueuePresentKHR.html)

获取前的输入拒绝或获取返回错误时，交回原诊断。取得图像之后录制、拷贝提交或队列呈现失败时，调用者结束当前链并显式重建。析构先结束该链原生资源的使用。首次成功获取前可直接销毁已创建的句柄。从未入队呈现时，若拷贝未获队列接受，析构以带 Fence 的获取等待结束原信号的使用。若拷贝已获队列接受，析构沿原复制执行确认交换链图像的使用结束。设备丢失时复用原逻辑设备退役结果。[Khronos图像获取](https://docs.vulkan.org/refpages/latest/refpages/source/vkAcquireNextImageKHR.html)

设备按实际实例扩展与物理设备特性启用 KHR 或 EXT 的 swapchain maintenance1。支持该特性时，Swapchain 持有每份已入队呈现的 Fence，轮询确认后复用，正常 `try_present` 无需为持有或复用 Fence 增设同步等待。呈现引擎报告图像过期或表面丢失时，已经入队的呈现仍交接原 Fence。关闭时以同一链、同一队列上尚未确认的最后一份呈现 Fence 收齐呈现资源的使用，并结束尚未交给呈现的获取或复制。驱动继续拥有上屏过程及图像载荷。[Khronos呈现 Fence](https://docs.vulkan.org/refpages/latest/refpages/source/VkSwapchainPresentFenceInfoKHR.html)

无扩展的设备在交换链曾实际入队呈现时沿基础兼容路径调用 `vkDeviceWaitIdle` 后销毁资源。基础 Vulkan 缺少等价的呈现资源完成点，该路径依赖驱动的兼容行为。多队列、动态extent及完整引擎调度仍未完成。[Khronos交换链资源退出](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html#_vk_ext_swapchain_maintenance1_extension)
