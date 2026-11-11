# 异步结果与原生完成观测

`Submission::detach()`不等待本次GPU执行，返回原 `DomainBindings<GpuDataset>`、原AS绑定和 `GpuCompletion`。没有复制数据、资源或完成事实；结果继续携带原来的真实最后使用点，后续程序、下载和释放沿原执行依赖处理。

`GpuCompletion::is_complete()`查询同一原生执行的实际完成情况。原Completion的vkWaitForFences等待与vkGetFenceStatus查询只记录0或DEVICE_LOST；查询NOT_READY返回false并保持未知。其他等待／查询错误原样返回，不记录终态，继续由原执行持有资源；调用者下一次显式wait／query沿原fence观察，不自动重试。等待返回范围见[Khronos vkWaitForFences](https://docs.vulkan.org/refpages/latest/refpages/source/vkWaitForFences.html)。已记录的终态查询不再次访问原fence，原错误继续交回，DEVICE_LOST不作为true；`wait()`等待完成并返回`Option<GpuTimings>`。`GpuProgram`及`GpuTemplate`默认不采集，通过`with_timestamps(true)?`显式开启，通过`with_timestamps(false)?`关闭。没有采集或没有执行阶段返回`None`；`Some`中的零值仍是实际查询结果。开启时计算队列必须真实支持时间戳，未开启的计算不以计时能力为前提。已请求的查询池在执行期间由原`ExecutionResources`持有，观察者也持有原池，因此后一个提交完成并回收此前命令资源后，前一个观察者仍可读取自己的时间戳。此接口不是引擎帧号、计数镜像、另一个调度器或新的完成服务。

观察者可以先于结果退出。结果和AS的物理使用期仍由原资源／Execution管理，不以观察者退出或CPU tick结束作为GPU完成。最后真实资源所有者退出时仍履行原有完成后释放职责；异步不等于允许提前销毁设备资源。`Submission::wait()`借用同一完成点与查询池等待，成功交回Completed并清除数据的未决使用标记；失败交回(error, submission)，同一提交继续持有原绑定、AS、查询池与真实最后使用点。调用者决定继续持有或明确结束它，错误不保证设备数据可用。原Execution::wait／is_complete在同一Completion已记录0或VK_ERROR_DEVICE_LOST时，沿原release_known结束本执行的命令、scratch、绑定参数和依赖持有，再原样交回原Result；已终止执行的中间资源不因失败观察者继续存在而延长。DEVICE_LOST仍作为错误返回，不发布成功，也不改变原绑定、AS及查询池的观察所有权。GpuDataset或Structure明确交出原存储时，Execution::discard／discard_structure在同一Completion已缓存0或DEVICE_LOST后直接drop原DatasetStorage／StructureStorage；尚未确认终止时仍由原discarded／discarded_structures保留。ReadDataset、AS及其BLAS来源通过原Rc维持真实拥有关系，最后一个拥有者退出前底层资源保持；查询仍持有AS、其他TLAS或调用者仍持有BLAS时，不以某个构建点结束提前释放共享资源，不添加等待或第二释放链。其他未确认终止的错误不提前释放在途资源。终态资源规则见[Khronos设备丢失语义](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html#devsandqueues-lost-device)。执行器不自动重放。`into_bindings()`及原按绑定交接继续存在。

BLEngine的 `submit_to` 直接消费这些绑定，将同一工作图像和输出交回原所有者；下一次提交沿原依赖使用它们，无需CPU等待，也不为每个在途执行复制一份内部8K工作图像。渲染算法及阶段仍由B表达，实际执行、Vulkan内存依赖与寿命仍归Blink。


通用完成交接与B应用语言能力分别定义；B主流程的动态dataset和同步CPU／GPU组织已在规范中支持，设备自主调度、多队列及完整引擎能力仍未完成。

## 交换链呈现与原显示存储
`Swapchain::try_present(&mut GpuDataset)`返回`Poll<PresentStatus>`。交换链自己的上一份acquire信号量消费尚未完成，或零超时`vkAcquireNextImageKHR`报告NOT_READY／TIMEOUT时，返回Pending，不提交拷贝、不修改输入最后使用点。消费者保留当前完整显示数据，继续处理输入或其他视图；Pending是原生可用性，不是错误或异常重试。

取得实际图像后，沿原Commands记录源生产者到传输的依赖、完整分页面拷贝及呈现布局转换，提交后交给原ExecutionResources::Transfer。输入GpuDataset的最后使用点更新为这份真实只读拷贝执行；后续计算改写、下载和退出复用既有依赖与资源释放，无需复制显示图或在CPU等待拷贝。冻结数据的只读binding同样保持原物理所有者；不能因调用返回提前释放它。

Swapchain只持有当前acquire信号量的实际消费执行，完成后才再次获取图像；呈现等待信号量仍按实际交换链图像分配。Ready表示拷贝和呈现已排入队列，返回Optimal／Suboptimal事实，不表示源拷贝已完成或显示器已扫描输出。信号量消费与上屏完成是不同事实，参见[Khronos交换链信号量复用](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html)。

原`present(&GpuDataset)`继续是明确的同步接口，使用同一提交实现并等待源拷贝完成；可与try_present在同一交换链交替使用。正常可轮询入口没有无限acquire等待或显式CPU拷贝等待，但驱动的vkQueuePresentKHR调用本身仍可能有宿主成本，不能承诺任何平台调用绝对不阻塞。实际错误照常报告，不重建交换链、降档或吞掉OutOfDate。当前交换链销毁继续履行原WSI完成边界；多队列、动态extent及完整引擎调度仍未完成。
