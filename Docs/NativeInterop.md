# C、C++ 与 Rust 集成

C++ 与 Rust 都是 Blink 的正式宿主。C/C++ 可直接链接 blc 生成的 DLL；Rust 可使用同一 DLL 的安全包装器，也可继续使用 blanguage、blcompute 原生 crate。原生 Rust 的编译、CPU 执行、GPU 驻留、异步提交和所有权接口不经过 C ABI，已有签名保持不变。

## 由 blc 输出 DLL

当前目标为 Windows x64。使用源码 SDK，在 Visual Studio x64 Developer PowerShell 中构建 Blink，再执行：

```powershell
cargo build --workspace --release
.\target\release\blc.exe build-dll `
    sample\lessons\map.b `
    target\interop\map.dll sample scale
```

最后一个参数选择具体公开 program；开放 const 入口可以用 'run<float3,1000000>' 等实例。每个 DLL 导出一个无 requires 前置条件的具体入口；条件模板应先在 B 中组合成能够自行生产所需关系的 program。不会把裸指针中的 Count 当作编译器已经证明的 prefix 关系。

| 输出 | 用途 |
|---|---|
| map.dll | 独立运行的算法库；部署到宿主可执行文件旁 |
| map.lib | C、C++、Rust 在 Windows MSVC 下链接使用的导入库 |
| map.abi.h | C ABI、数据布局及 C++ RAII 包装器 |
| map.rs | 相同布局、extern C 声明及 Rust 安全包装器 |

B 的源码接口 map.h 和生成的 C 头文件 map.abi.h 是不同文件。导出前缀来自 DLL 文件名，只接受 ASCII 字母、下划线和非首位数字；不同算法库应使用不同名字。生成接口与对应 DLL 一起发布，不混用其他版本的绑定。

blc 在构建时完成 B 的模块、类型、访问权限、阶段及入口检查，并将已检查 IR 内嵌到 DLL。create 按选择的后端准备原生 CPU 代码或 Vulkan pipeline；该一次性准备仍使用 Blink 自研 x64／SPIR-V 后端。Rust 工具链负责既有 Rust 执行框架、C ABI 和 PE DLL 链接，不负责翻译 B 算法。当前 build-dll 使用构建 blc 的源码 SDK 与 Rust/MSVC 工具链；消费已生成 DLL 的 C/C++ 项目无需安装 Rust、Blink 编译器或 Blink 源码。

部署机器需要 Windows x64 与 [Microsoft Visual C++ x64 运行库](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。GPU 后端另需支持程序所需 Vulkan 能力的显卡驱动；不要求安装 Vulkan SDK。

## 三种宿主

完整、可运行的工程命令见 [sample/interop](../sample/interop/README.md)。最短调用如下。

C++ 使用头文件中的 RAII 类：

```cpp
#include "map.abi.h"
float values[] = {1, 2, 3, 4};
map::Program program(map::Backend::Cpu);
program.run(values, 4);
// values: 3, 5, 7, 9
```

Rust 使用同一个 DLL，包装器公开切片、Result 与 Drop：

```rust
let mut values = [1.0, 2.0, 3.0, 4.0];
let mut program = map::Program::new(map::Backend::Cpu)?;
program.run(&mut values)?;
// values: [3.0, 5.0, 7.0, 9.0]
```

纯 C 使用 map_create、map_run、map_destroy，返回值 0 表示成功、-1 表示失败；错误文本写入调用方提供的 UTF-8 缓冲区。见 map.c。C++ 包装器把错误转为 std::runtime_error，Rust 包装器转为 Result。没有跨 DLL 传递 Rust Vec、String、C++ 对象或其分配器。

Rust 原生入口继续使用 compile_entry、Dataset、CpuProgram、Device、GpuProgram 等类型。native.rs 已分别执行 CPU 和 GPU；需要跨提交保留设备数据、view／纹理／加速结构与真实资源所有权时，继续使用原生 API。

## 数据、完成与释放

- DLL 当前接受纯值 dataset 和 uniform：f32／u32／i32／bool、向量、矩阵、普通或具体泛型记录、固定内联数组。生成布局直接取自编译器的字段偏移、对齐与步长；布尔字段使用 32 位整数，值为 0 或 1。
- 向量、矩阵、嵌套记录和数组展开为 v_position_x、v_tags_0 等标量成员，填充与字节偏移显式保留。不要直接强转宿主自有结构；按生成的类型填充字段。执行层只复制真实字段，不读取或改写宿主结构体的填充。
- 每个数据指针配一个 u32 元素数，单位是完整记录。只读参数生成 const 指针／Rust 共享切片；可写参数生成可写指针／Rust 独占切片。各可写区间不得与其他参数重叠；调用期间宿主不能同时访问相应内存。
- run 是一次同步调用：准备本次存储、复制输入、执行、等待实际完成，再复制输出。GPU 路径包含上传与输出读回；DLL 不在返回后借用调用方数组。数组始终由宿主分配和释放，handle 由 DLL 创建并由 DLL 的 destroy 释放。所有 handle 销毁后才能卸载 DLL。
- create 的 backend 为 0（CPU）或 1（GPU）；目标限制与设备能力错误直接返回。相同 handle 可反复运行；调用及释放必须在创建线程串行执行。Rust 包装器以独占方法、Drop 和非 Send／Sync 类型表达这一所有权；C/C++ 遵守头文件中的相同约定。不同线程可分别创建自己的实例。
- view、纹理、加速结构和有外部 prefix 前置条件的程序不在当前 DLL 数据 ABI 中；build-dll 明确拒绝，不改写成其他算法。C++ 可以调用包含内部多阶段、集合、排序及内部有效前缀的完整 program。

## 性能的完整口径

sample/interop/fractal.* 是一百万像素的 Mandelbrot 计算，C++ 和 Rust 分别保留开启优化的串行实现，再调用同一个 DLL。每次计时只执行一次完整算法，包含 DLL 调用、本次分配、必要复制、GPU 提交／等待与输出读回。create 单独报告，不混入已复用实例的每次调用；首次只运行一回时应相加比较。重复测量使用两次预热、九次单次计时的中位数。

CPU 结果与两个串行实现逐项相同。当前 GPU 的 f32 结果在分形边界有差异，具体数量与最大迭代差由样例输出，不能将设备结果称为逐位等价。分形的递推会放大浮点差异；要求精确迭代计数的消费者应选用已核对的 CPU 路径或另行设计数值算法。不要只用加速比判断替换是否适合应用。

## B 源码能否直接调用 C++ 或 Rust

当前没有任意 extern 函数、C++ 类／模板或 Rust 函数的导入语法。DLL 输出解决的是 C/C++／Rust 调用 B 的具体算法，不代表 B 已能反向调用任意宿主函数。

现有应用由宿主组织：C++ 或 Rust 准备数据、调用 B、消费结果，再调用既有库。C++ 与 Rust 自己的相互调用使用双方的 C ABI；普通 Rust ABI 不作为跨语言稳定接口。GPU 内核也不能把任意 CPU 函数直接当设备函数。此边界不会删减或替换既有 Rust 原生计算 API。

参考：[Microsoft DLL 使用指南](https://learn.microsoft.com/en-us/cpp/build/walkthrough-creating-and-using-a-dynamic-link-library-cpp)、[Rust extern 块](https://doc.rust-lang.org/reference/items/external-blocks.html)、[Rust 链接规则](https://doc.rust-lang.org/reference/linkage.html)。
