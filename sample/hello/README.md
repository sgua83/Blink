# 完整的最小程序 / Complete minimal programs

[中文入门](../../Open/Docs/GettingStarted.md) · [English guide](../../Open/Docs/GettingStarted.en.md)

每组 `.h`／`.b` 都是完整独立程序，不依赖自定义模块；文件名和 `module` 名必须对应。
Each header/source pair is a complete application with no custom imports. Keep its filenames and module name aligned.

| 文件 / Files | 行为 / Behavior | 输出 / Output |
|---|---|---|
| `hello.h`, `hello.b` | CPU 标量计算 / Scalar CPU calculation | `3` |
| `parallel.h`, `parallel.b` | 普通调用同文件完整计算函数 / Ordinary CPU call to the fully shown compute function | `3`, `5`, `7`, `9` |
| `gpu.h`, `gpu.b` | 同文件展示完整计算、上传、GPU 调用与下载 / Visible computation and GPU transfers in one file | `3`, `5`, `7`, `9` |

先安装 [Windows x64 正式版 0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1)、Rust stable MSVC 与 Visual Studio x64 C++ Build Tools（含 Windows SDK）；在 x64 Developer PowerShell 中运行。下面命令从本目录执行，将 `hello` 换为 `parallel` 或 `gpu` 即构建其他例子。
Install the release, Rust stable MSVC and Visual Studio x64 C++ Build Tools with the Windows SDK first. In x64 Developer PowerShell, run these commands from this directory; replace `hello` with `parallel` or `gpu` for the other programs.

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c hello.b -o target/hello.obj
& "$env:ProgramFiles/Blink/bin/blink.exe" target/hello.obj -o target/hello.exe
.\target\hello.exe
```

应用入口必须是 `on cpu`。`on compute` 表示函数能面向两端编译；CPU 普通调用保持 CPU 执行，无需 CPU target。CPU／GPU 两版在各自文件中完整定义逐字相同的 `fill_numbers`，分配四条数据并打印相同输出。CPU 使用 `fill_numbers(values);`；GPU 仅将这一行换为 `upload`、`target(gpu, ...)`、`download` 三行。自动选设备与内联 GPU 块语法尚未实现。
Application entry points must be `on cpu`. A normal CPU call to an `on compute` function stays on the CPU. Both files contain the complete, identical `fill_numbers`, allocate four elements and print the same output. The CPU calls `fill_numbers(values);`; the GPU replaces only that statement with upload/target/download. Automatic device selection and inline GPU blocks are not implemented.

GPU 例子需要 Vulkan 驱动以及 f32 的 `DenormPreserve`、`SignedZeroInfNanPreserve` 能力，无需 Vulkan SDK。正式工具 0.14.1.1 已分别编译、链接并运行以上三个程序及普通 CPU 调用对照，结果如表所示。运行成品时带上链接器生成的配套 DLL。
The GPU example requires a Vulkan driver with those f32 capabilities, but no Vulkan SDK. All three programs and the ordinary CPU-call variant were compiled, linked and run with release tools 0.14.1.1. Ship generated companion DLLs with the executable.
