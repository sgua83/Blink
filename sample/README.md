# Blink 语言（B 语言）学习资料

## 从零开始：《B 语言编程》

[书稿](B语言编程.md) · [完整示例](beginner/README.md) · [手机 PDF](pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](pdf/B语言编程.pdf)。从第一个数字开始，讲解 B 语言的基本写法，包含直接运行命令、实际结果、图示、练习和参考答案；不要求先学习 C/C++ 或 Rust。

## 并行算法：《并行编程的艺术》

从 [《并行编程的艺术》](并行编程的艺术.md) 开始阅读。前半部分用小例子学习数据域、独占写、归约、扫描、筛选流水线和块内协作；后半部分摘读 BLEngine 的真实实现，把理论连接到实际计算。

中文版 PDF：[手机阅读版](pdf/并行编程的艺术-B语言-手机阅读版.pdf) · [标准版](pdf/并行编程的艺术-B语言.pdf)。两份正式 PDF 与书稿一同提交，便于直接下载阅读。

| 模块 | 主题 |
|---|---|
| [map](lessons/map.b) | 标量数组的最小逐元素变换 |
| [scan](lessons/scan.b) | 求和归约与独占前缀和 |
| [pipeline](lessons/pipeline.b) | 稳定筛选、有效域、排序、扫描与归约 |
| [tiles](lessons/tiles.b) | 输入快照、发布、协作函数与静态块宽度 |
| [algebra](lessons/algebra.b) | 保持输入顺序的自定义组合与前缀 |

每个模块的 `.h` 提供接口，`.b` 提供实现。各章直接使用 `target/release/blrun.exe` 编译并运行源码，给出 CPU/GPU 命令和预期结果。单独检查、导出代码与读取 BLDATA 的方法集中在附录 A；[BLEngine 摘录来源](BLEngine摘录来源.md) 记录原文件与摘录版本。

[性能示例](performance/README.md) 提供同算法的优化 C++ 串行对照和 B CPU／GPU 原生执行，覆盖 13 组计算。性能数据统一来自 Release 正式构建，附有[构建与原始运行记录](performance/运行记录.txt)。各章同时给出手算输入和用于体现效率优势的大规模输入，报告墙钟耗时、加速比与 GPU 数据往返成本。

正式中文版 PDF 保存在 `pdf`；性能原始结果随配套示例提供，排版中间文件和临时运行数据放在本地 `Saved`。

书中各个可运行案例附 CPU/GPU 执行命令及实际运行记录图；`figures` 保存正文使用的执行示意与运行记录。模块名 `lessons.map` 是示例命名空间，不是标准库 map。

## 第二本：《Blink 和 Rust 混合编程》

[书稿](Blink和Rust混合编程.md) · [配套项目](hybrid/README.md) · [手机 PDF](pdf/Blink和Rust混合编程-手机阅读版.pdf) · [标准 PDF](pdf/Blink和Rust混合编程.pdf)。从三个完整源码文件和两条编译命令开始，先跑通四个数的 Rust/B 调用；再逐步进入粒子、连续计算、共享模型、场景和材质图像。

## 第三本：《BLEngine 源码分析》

[书稿](BLEngine源码分析.md) · [阅读场景与命令](engine/README.md) · [手机 PDF](pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](pdf/BLEngine源码分析.pdf)。固定 BLEngine CL13031、Blink CL13046，以 25 章从数学、几何变换和相机逐步讲到场景、资产、材质光照、Stella 输运、GPU 执行与窗口；包含 35 处核心源码节选、执行图、完整源码结构索引及重新运行的真实 4K 场景，明确说明尚未实现的引擎能力。

## 编译器与执行框架：《B 语言源码分析》

[书稿](B语言源码分析.md) · [源码版本与阅读资料](compiler/README.md) · [手机 PDF](pdf/B语言源码分析-手机阅读版.pdf) · [标准 PDF](pdf/B语言源码分析.pdf)。固定 Blink CL13062，以 28 章、53 处真实源码节选和 16 幅图，从内存布局、语法树、类型与并行关系分析，逐步讲到寄存器、x64、SIMD、SPIR-V、Vulkan、集合算法及资源寿命。关联底层知识先于源码展开，首例使用同一版本的 Release 产物在 CPU/GPU 实际执行；独立应用与跨平台等未完成能力明确区分。
