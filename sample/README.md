# Blink 语言（B 语言）并行编程示例

从 [《并行编程的艺术》](并行编程的艺术.md) 开始阅读。前半部分用小例子学习数据域、独占写、归约、扫描、筛选流水线和块内协作；后半部分摘读 BLEngine 的真实实现，把理论连接到实际计算。

中文版 PDF：[手机阅读版](pdf/并行编程的艺术-B语言-手机阅读版.pdf) · [标准版](pdf/并行编程的艺术-B语言.pdf)。两份正式 PDF 与书稿一同提交，便于直接下载阅读。

| 模块 | 主题 |
|---|---|
| [map](lessons/map.b) | 标量数组的最小逐元素变换 |
| [scan](lessons/scan.b) | 求和归约与独占前缀和 |
| [pipeline](lessons/pipeline.b) | 稳定筛选、有效域、排序、扫描与归约 |
| [tiles](lessons/tiles.b) | 输入快照、发布、协作函数与静态块宽度 |
| [algebra](lessons/algebra.b) | 保持输入顺序的自定义组合与前缀 |

每个模块的 `.h` 提供接口，`.b` 提供实现。书中附录 A 包含可直接执行的 CPU/GPU 命令和预期结果；[BLEngine 摘录来源](BLEngine摘录来源.md) 记录原文件与摘录版本。

[性能示例](performance/README.md) 提供同算法的优化 C++ 串行对照和 B CPU／GPU 原生执行，覆盖 13 组计算。性能数据统一来自 Release 正式构建，附有[构建与原始运行记录](performance/运行记录.txt)。各章同时给出手算输入和用于体现效率优势的大规模输入，报告墙钟耗时、加速比与 GPU 数据往返成本。

正式中文版 PDF 保存在 `pdf`；性能原始结果随配套示例提供，排版中间文件和临时运行数据放在本地 `Saved`。

书中各个可运行案例附编译、CPU/GPU 执行命令及实际运行记录图；`figures` 保存正文使用的执行示意与运行记录。模块名 `lessons.map` 是示例命名空间，不是标准库 map。

## 第二本：《Blink 和 Rust 混合编程》

[书稿](Blink和Rust混合编程.md) · [配套项目](hybrid/README.md) · [手机 PDF](pdf/Blink和Rust混合编程-手机阅读版.pdf) · [标准 PDF](pdf/Blink和Rust混合编程.pdf)。从三个完整源码文件和两条编译命令开始，先跑通四个数的 Rust/B 调用；再逐步进入粒子、连续计算、共享模型、场景和材质图像。

## 第三本：《BLEngine 源码分析》

[书稿](BLEngine源码分析.md) · [阅读场景与命令](engine/README.md) · [手机 PDF](pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](pdf/BLEngine源码分析.pdf)。固定 BLEngine CL12969，沿同一个双实例场景分析架构、数据生产消费与资源寿命，包含真实 4K 输出、编译阶段和原生运行记录。
