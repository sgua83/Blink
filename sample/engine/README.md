# 《BLEngine 源码分析》阅读材料

[书稿](../BLEngine源码分析.md) · [手机 PDF](../pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](../pdf/BLEngine源码分析.pdf)。全书从数学、几何变换和相机开始，依次解释裁剪与查询、场景森林、组件、资产、BLAS/TLAS、材质光照、采样、反射折射、介质与 Stella 输运，再进入设备执行、资源寿命、窗口、模拟和编辑器消费。25 章正文配有核心源码、数值推导、执行图及完整源码结构索引。

正文沿当前引擎与Blink职责阅读，现行源码位置见源码版本索引.json；历史构建和实际运行记录保存在P4双台账。

## 源码与目录

Blink 开源地址为 https://github.com/sgua83/Blink 。BLEngine 仍是独立的授权 P4 工程，当前未在这个仓库完整公开；下载 Blink 不包含完整引擎。具备对应引擎源码后，将两工程相邻放置，目录分别叫 BLEngine、BLanguage。

本目录的场景有七个节点：父节点、两个几何实例、两台相机和两盏灯。两个实例共享一份二十四顶点、十二三角形的立方体几何，分别选择棋盘和蓝色表面。格式为 BLSCENE_SOURCE 8、BLMAT 2，显式声明介质和相机起始介质。

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

计时区分设备程序跨度、传输／读回及完整程序墙钟，实际数值以本次Release输出为准，不写成窗口产品帧率。

## 模拟

```powershell
.\target\release\blengine.exe simulate `
  cpu 1000000 1200 `
  Saved/Scene/bodies-cpu.bldata
.\target\release\blengine.exe simulate `
  gpu 1000000 1200 `
  Saved/Scene/bodies-gpu.bldata
```

计时区分设备程序跨度、传输／读回及完整程序墙钟，实际数值以本次Release输出为准，不写成窗口产品帧率。

计时区分设备程序跨度、传输／读回及完整程序墙钟，实际数值以本次Release输出为准，不写成窗口产品帧率。

源码节选遵守 [BLEngine MIT License](BLEngine-LICENSE.txt)。这些文件是正式阅读材料和输入，不是构建后自动运行的测试关卡。当前未完成的动画、完整物理、音频、网络、虚拟几何、时序重建和流送等能力在书中明确说明。
