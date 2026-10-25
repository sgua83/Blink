# 《BLEngine 源码分析》阅读案例

[书稿](../BLEngine源码分析.md) · [手机 PDF](../pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](../pdf/BLEngine源码分析.pdf)。本书分析 BLEngine P4 CL12969，执行基础为 Blink CL12971。

本目录提供一份立方体几何、棋盘纹理、材质库和场景文本。场景由七个节点组成：一个父节点、两个几何实例、两台相机、两盏灯。两个实例共享一份几何，分别使用棋盘与蓝色材质，便于追踪资产、节点、变换、实际命中与资源寿命。

## 编译与运行

需要 BLEngine 对应源码检出；它目前使用 P4 //BLEngine/main，未在此仓库中发布。Blink 可从 https://github.com/sgua83/Blink 下载。两工程相邻放置，目录名分别为 BLEngine、BLanguage。

Windows x64 下安装 Rust stable MSVC 和 Visual Studio C++ 桌面开发工具。打开 x64 Developer PowerShell，在 BLEngine 根目录执行；GPU 渲染要求显卡驱动提供 Vulkan 加速结构与光线查询能力，无需 Vulkan SDK：

```powershell
cargo build --manifest-path ../BLanguage/Cargo.toml --workspace --release
cargo build --workspace --release
.\target\release\blengine.exe device

New-Item -ItemType Directory -Path Saved/SourceReading | Out-Null
Copy-Item ../BLanguage/sample/engine/assets/* Saved/SourceReading

.\target\release\blengine.exe cook `
    Saved/SourceReading/cube.obj `
    Saved/SourceReading/cube.blgeom
.\target\release\blengine.exe cook-scene `
    Saved/SourceReading/reading.blscene.txt `
    Saved/SourceReading/reading.blscene
.\target\release\blengine.exe render `
    Saved/SourceReading/reading.blscene `
    Saved/SourceReading/frame.ppm
.\target\release\blengine.exe render-views `
    Saved/SourceReading/reading.blscene `
    Saved/SourceReading/views
```

输出目录首次创建，后续沿用。场景引用相对最终 .blscene 所在目录，材质纹理相对 .blmat 所在目录。PPM 用支持该格式的图像工具打开。完整输出为 3840×2160，内部采样为 7680×4320。

## 同一真实模拟的 CPU/GPU 执行

```powershell
.\target\release\blengine.exe simulate `
    cpu 1000000 1200 Saved/SourceReading/bodies-cpu.bldata
.\target\release\blengine.exe simulate `
    gpu 1000000 1200 Saved/SourceReading/bodies-gpu.bldata
```

一百万粒子以每步 1/120 秒演化十秒，共一千二百个物理时间步。CPU 为原生多核，GPU 为设备驻留执行。total_ms 计入后端准备、执行、必要传输和输出文件写入，不含两条路径共同的初值创建；本次分别为 896.911、344.046 ms，约 2.61 倍。设备程序 127.532 ms 是其中的一部分。浮点结果保留实际差异，不能将这些范围混为一个内核计时。

[运行记录](运行记录.txt) 保留实际终端输出；[源码版本索引](源码版本索引.json) 列出固定源路径、修订与 SHA256。终端卡片按日志排印，两张场景 PNG 由完整原生 PPM 转换，保留原分辨率。窗口和编辑器章节以固定源码分析为准，本次重新执行的是离屏渲染、双视图、设备查询、资产转换和模拟。

源代码节选遵守 [BLEngine MIT License](BLEngine-LICENSE.txt)。本目录是正式阅读材料与输入，不是构建后自动运行的测试关卡。
