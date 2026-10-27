# 异步结果与原生完成观测

`Submission::detach()`不等待本次GPU执行，返回原 `DomainBindings<GpuDataset>`、原AS绑定和 `GpuCompletion`。没有复制数据、资源或完成事实；结果继续携带原来的真实最后使用点，后续程序、下载和释放沿原执行依赖处理。

`GpuCompletion::is_complete()`查询同一原生执行的实际完成情况；`wait()`等待完成并返回该提交的 `GpuTimings`。时间戳查询池在执行期间由原 `ExecutionResources` 持有，观察者也持有原池，因此后一个提交完成并回收此前命令资源后，前一个观察者仍可读取自己的时间戳。此接口不是引擎帧号、计数镜像、另一个调度器或新的完成服务。

观察者可以先于结果退出。结果和AS的物理使用期仍由原资源／Execution管理，不以观察者退出或CPU tick结束作为GPU完成。最后真实资源所有者退出时仍履行原有完成后释放职责；异步不等于允许提前销毁设备资源。旧 `Submission::wait()`复用同一完成／时间戳实现，保持完成后返回结果的接口；`into_bindings()`及原按绑定交接继续存在。

BLEngine的 `submit_to` 直接消费这些绑定，将同一工作图像和输出交回原所有者；下一次提交沿原依赖使用它们，无需CPU等待，也不为每个在途执行复制一份内部8K工作图像。渲染算法及阶段仍由B表达，实际执行、Vulkan内存依赖与寿命仍归Blink。

2026-10-26，Release原生消费者PID3716已执行：3840×2160／内部7680×4320，两次提交返回时均尚未完成；在其间替换照明、继续提交第三次执行并退出观察者，再释放渲染器／场景。反向等待前两次提交、读取原时间戳及直接下载无观察者输出均成功，退出码0。原光源异步完整帧与同步基线逐像素一致，后续空光源帧改变8037604个像素；没有把新光源错误用于旧提交。三次都使用原两阶段B程序，实际相机为20个u32字、显式可见性33177600条。数据、完整帧、进程、构建和原始日志在 `E:/BLEngineMeasurements/AsyncFrame`。与可见窗口同时运行的耗时不作吞吐或性能改善结论。

此项C-019补齐通用执行交接，不代表B主流程动态dataset／CPU-GPU program组织、设备自主调度、并行多队列或完整引擎已完成。

## 交换链呈现与原显示存储（2026-10-27）

`Swapchain::try_present(&mut GpuDataset)`返回`Poll<PresentStatus>`。交换链自己的上一份acquire信号量消费尚未完成，或零超时`vkAcquireNextImageKHR`报告NOT_READY／TIMEOUT时，返回Pending，不提交拷贝、不修改输入最后使用点。消费者保留当前完整显示数据，继续处理输入或其他视图；Pending是原生可用性，不是错误或异常重试。

取得实际图像后，沿原Commands记录源生产者到传输的依赖、完整分页面拷贝及呈现布局转换，提交后交给原ExecutionResources::Transfer。输入GpuDataset的最后使用点更新为这份真实只读拷贝执行；后续计算改写、下载和退出复用既有依赖与资源释放，无需复制显示图或在CPU等待拷贝。冻结数据的只读binding同样保持原物理所有者；不能因调用返回提前释放它。

Swapchain只持有当前acquire信号量的实际消费执行，完成后才再次获取图像；呈现等待信号量仍按实际交换链图像分配。Ready表示拷贝和呈现已排入队列，返回Optimal／Suboptimal事实，不表示源拷贝已完成或显示器已扫描输出。信号量消费与上屏完成是不同事实，参见[Khronos交换链信号量复用](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html)。

原`present(&GpuDataset)`继续是明确的同步接口，使用同一提交实现并等待源拷贝完成；可与try_present在同一交换链交替使用。正常可轮询入口没有无限acquire等待或显式CPU拷贝等待，但驱动的vkQueuePresentKHR调用本身仍可能有宿主成本，不能承诺任何平台调用绝对不阻塞。实际错误照常报告，不重建交换链、降档或吞掉OutOfDate。当前交换链销毁继续履行原WSI完成边界；多队列、动态extent及完整引擎调度仍未完成。
