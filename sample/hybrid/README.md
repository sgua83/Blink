# 《Blink 和 Rust 混合编程》配套项目

在 Blink 仓库根目录、Visual Studio x64 Developer PowerShell 中执行。安装与语言介绍见 [书稿](../Blink和Rust混合编程.md)。本项目独立依赖本仓库的 blanguage 与 blcompute，不要求 BLEngine 工作区或 P4。

```powershell
cargo build --workspace --release
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target'
cargo build --release --manifest-path sample/hybrid/Cargo.toml
$book = '.\target\release\blink-rust-book.exe'
& $book particles cpu 3
& $book particles gpu 3
& $book steps
& $book flow
& $book scene cpu
& $book scene gpu
& $book sheet cpu
& $book sheet gpu
& $book image
```

- particles：真实引擎 step_and_total，三粒子用于手算；可另给十进制数量。
- steps：三个 0.01 秒的物理时间步，无中间主机等待。
- flow：两组真实粒子模拟，通过 submit_join 汇合能量。
- scene：真实引擎 evaluate，世界变换、相机和点光源。
- sheet：一千万采样点的真实 GGX，分别显示首次执行和资源复用后的执行；GPU 另列全量下载。点记录约 1.12 GB，传输另需实际资源。
- image：一百万像素完整 B 流水，只下载四字节显示像素，输出 Saved/HybridBook/materials.ppm。支持 PPM 的图像查看器可以直接打开。

GPU 入口要求当前驱动提供相应 Vulkan 能力，出错直接返回；CPU 不创建设备。sheet 的 CPU 已是原生多核，不是串行参考；setup、主机执行等待、设备时间戳、读回采用不同字段，不能混成一个内核加速比。两种后端存在浮点差异，运行记录显示实际中心点，未宣称全部逐位一致。

Programs/book 是本书的输入生成与组合代码，其他 Programs 模块保留 BLEngine 截至 CL12965 的已提交原文及导入闭包；[来源清单](BLEngine摘录来源.md) 记录精确版本与 SHA256，[MIT 许可](BLEngine-LICENSE.txt) 随摘录保留。Rust src 是完整教学宿主；书中的引擎 Rust 节选另标原文件。

[原生运行记录](运行记录.txt) 保存书中图表对应的实际输出。这个项目是正式教学示例，不是构建后自动执行的测试关卡。
