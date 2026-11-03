# 《Blink 和 Rust 混合编程》配套示例

从 [第一个 DLL 调用例子](first/README.md) 开始。自己写 numbers.h、numbers.b、main.rs，使用 bc -c 编译目标文件，再由 blink 生成算法库，再用 rustc 编译 Rust 调用方，直接运行 numbers.exe。B 完成乘二加一，Rust 提供输入并打印结果。

第一、二章完整解释这次调用；第三章再使用 blanguage、blcompute 原生 Rust 接口，逐步准备粒子、执行 B 并读取结果。书稿见 [《Blink 和 Rust 混合编程》](../Blink和Rust混合编程.md)。

普通应用在 Rust 源码中选择设备，不读取命令行设备开关。`particles` 与 `material-samples` 仅为性能对照驱动，后者的一千万点 CPU／GPU 测量命令单独保留。

## 构建原生 Rust 示例

在 Blink 根目录、x64 Developer PowerShell 中执行：

```powershell
cargo build --release `
    --manifest-path sample/hybrid/Cargo.toml `
    --target-dir target

.\target\release\particle-cpu.exe
.\target\release\particle-gpu.exe
.\target\release\particle-steps.exe
.\target\release\particle-flow.exe
.\target\release\scene-evaluate.exe
.\target\release\material-image.exe
```


仅供性能对照的测量命令（与上面的普通应用分开）：

```powershell
.\target\release\particles.exe cpu 1000000
.\target\release\particles.exe gpu 1000000
.\target\release\material-samples.exe cpu
.\target\release\material-samples.exe gpu
```

设备参数属于这两个对照驱动的实验控制项，不是 B 语言或普通 Rust 宿主的运行要求。

| 程序 | 完成的事情 | 主要 Rust 源码 |
|---|---|---|
| particle-cpu | 完整展开的一次三粒子 CPU 模拟 | src/bin/particle-cpu.rs |
| particle-gpu | 宿主源码选定 GPU 的三粒子完整应用 | src/bin/particle-gpu.rs |
| particles | 性能对照专用：控制 CPU/GPU、数量并打印耗时 | src/particles.rs |
| particle-steps | 连续推进三个 0.01 秒的物理时间步 | src/particles.rs |
| particle-flow | 两组粒子分别求动能，再由 B 相加 | src/particles.rs |
| scene-evaluate | 三节点世界变换及相机、灯光位置 | src/scene.rs |
| material-samples | 一千万点的 GGX 采样，比较 CPU/GPU 时间 | src/sheet.rs |
| material-image | 一百万像素图片，从 B 加载一直到文件保存 | src/bin/material-image.rs |

前述两个完整程序直接写出加载、数据创建、执行和结果处理。其余示例的独立入口也位于 src/bin，各完成一个明确用途。

GpuProgram／GpuTemplate默认不采集时间戳。particles的GPU计时、material-samples及material-image在原程序上显式调用with_timestamps(true)?，因此这些测量示例还要求计算队列支持时间戳；无样本显示not_collected，不把未采集写成零。particle-steps／particle-flow继续使用默认普通执行。

GPU 例子需要驱动提供所需 Vulkan 能力。失败会直接返回；CPU 不创建设备。点记录约 1.12 GB，全量下载与设备计算分别计时。material-image 只下载最后的四字节显示像素，输出 Saved/HybridBook/materials.ppm。

Programs 下是按当前 Blink 语法维护的引擎算法教学模块及所需导入。Programs/book 只准备示例输入和连接步骤；实际粒子、变换、GGX 与显示转换使用引擎实现。[来源清单](BLEngine摘录来源.md) 与 [MIT 许可](BLEngine-LICENSE.txt) 随源码保留。下载 Blink 后可运行这些独立例子，不要求 BLEngine 工作区或 P4。

P4台账中的运行历史 保留正文图表和新版完整示例的输出。
