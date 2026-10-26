# 《BLEngine 源码分析》阅读材料

[书稿](../BLEngine源码分析.md) · [手机 PDF](../pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](../pdf/BLEngine源码分析.pdf)。全书从数学、几何变换和相机开始，依次解释裁剪与查询、场景森林、组件、资产、BLAS/TLAS、材质光照、采样、反射折射、介质与 Stella 输运，再进入设备执行、资源寿命、窗口、模拟和编辑器消费。25 章正文配有核心源码、数值推导、执行图及完整源码结构索引。

分析固定在 **BLEngine P4 CL13031**，原生构建使用 **Blink P4 CL13046**。当前工作区后续研发不纳入这份快照。`源码版本索引.json` 保存两个工程的构建源修订与 SHA256，以及正文 35 处源码节选的位置和摘要。

## 源码与目录

Blink 开源地址为 https://github.com/sgua83/Blink 。BLEngine 仍是独立的授权 P4 工程，当前未在这个仓库完整公开；下载 Blink 不包含完整引擎。具备对应引擎源码后，将两工程相邻放置，目录分别叫 BLEngine、BLanguage。

本目录的场景有七个节点：父节点、两个几何实例、两台相机和两盏灯。两个实例共享一份二十四顶点、十二三角形的立方体几何，分别选择棋盘和蓝色表面。格式为 BLSCENE_SOURCE 7、BLMAT 2，显式声明介质和相机起始介质。

## 编译与运行

Windows x64 下安装 Rust stable MSVC 和 Visual Studio C++ 桌面开发工具。打开 x64 Developer PowerShell，在 BLEngine 根目录直接执行；Cargo 同时构建所引用的 Blink 库：

```powershell
cargo build --release
.\target\release\blengine.exe device

New-Item -ItemType Directory `
  -Path Saved/Scene
Copy-Item `
  ../BLanguage/sample/engine/assets/* `
  Saved/Scene

.\target\release\blengine.exe cook `
  Saved/Scene/cube.obj `
  Saved/Scene/cube.blgeom
.\target\release\blengine.exe `
  cook-scene `
  Saved/Scene/reading.blscene.txt `
  Saved/Scene/reading.blscene
.\target\release\blengine.exe render `
  Saved/Scene/reading.blscene `
  Saved/Scene/frame.ppm
.\target\release\blengine.exe `
  render-views `
  Saved/Scene/reading.blscene `
  Saved/Scene/views
```

输出目录首次创建，后续沿用。纹理相对材质文件、模型和材质相对最终场景文件解析。运行需要显卡驱动实际支持当前程序用到的 Vulkan 加速结构和光线查询等能力，不需要 Vulkan SDK。输出完整 3840×2160，内部采样 7680×4320。

本次 AMD Radeon RX 9070 GRE 的单帧 GPU 程序跨度为 46.898 ms，两段 integrate/resolve 为 45.45448/1.41876 ms，绘制、读回和文件写入为 180.977 ms。两视图 GPU 跨度合计 98.290 ms，绘制、读回和写入为 342.771 ms。这些范围不等于窗口产品帧率，也不包含前面的全部加载与编译工作；精确起止位置见原始记录。

## 模拟

```powershell
.\target\release\blengine.exe simulate `
  cpu 1000000 1200 `
  Saved/Scene/bodies-cpu.bldata
.\target\release\blengine.exe simulate `
  gpu 1000000 1200 `
  Saved/Scene/bodies-gpu.bldata
```

一百万粒子以 1/120 秒步长演化十秒，共一千二百步。CPU 为原生多核，GPU 驻留推进。相同 total_ms 口径为 1228.396/369.908 ms，GPU 约快 3.32 倍；CPU 内部执行墙钟为 1214.745 ms，GPU 设备程序为 114.413 ms。后二者口径不同，不能替代端到端对比。总动能分别为 63477.54/63477.547，保留实际浮点差异。

[运行记录](运行记录.txt) 保存实际命令、输出、计时边界和完整图像哈希。正文两张 PNG 由本次原生完整 PPM 无损转换；运行卡片是日志排印，数学与执行图是示意。交互窗口、编辑器章节以固定源码分析为准，本次没有执行 view、view-views 或 editor。

源码节选遵守 [BLEngine MIT License](BLEngine-LICENSE.txt)。这些文件是正式阅读材料和输入，不是构建后自动运行的测试关卡。当前未完成的动画、完整物理、音频、网络、虚拟几何、时序重建和流送等能力在书中明确说明。
