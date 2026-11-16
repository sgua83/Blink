# 从现有工程调用 B

先选择自己的工程语言。下面三篇各自提供完整源码、构建命令和输出，只需安装 Blink 及其构建依赖，不需要先从源码构建 Blink：

| 你的工程语言 | 独立教程 |
|---|---|
| C | [C 调用 B](../../Open/Docs/CInterop.md) |
| C++ | [C++ 调用 B](../../Open/Docs/CppInterop.md) |
| Rust | [Rust 调用 B](../../Open/Docs/RustInterop.md) |

本目录的 `map.c`、`map.cpp` 和 `map.rs` 保留对应的仓库示例；使用这些源码时，在仓库根目录生成 `target/interop/map.dll` 及配套接口，再构建所选宿主即可。完整 ABI 与部署规则见[原生集成文档](../../Open/Docs/NativeInterop.md)。

以下保留已有工程的高级示例。在仓库根目录的 x64 Developer PowerShell 中，先按[源码构建说明](../../Open/Docs/GettingStarted.md#从-main-构建研发工具)构建同源 Release 工具，再执行这些研发命令。

## 性能对照专用：一百万像素分形的完整调用计时

```powershell
.\target\release\bc.exe -c `
    sample/interop/fractal.b `
    -o target/interop/fractal.obj `
    -I sample `
    --program render
.\target\release\blink.exe `
    target/interop/fractal.obj `
    -o target/interop/fractal.dll

cl /nologo /utf-8 /std:c++17 `
    /O2 /arch:AVX2 /fp:precise /EHsc `
    /I target/interop `
    sample/interop/cpp.cpp `
    target/interop/fractal.lib `
    /Fo:target/interop/cpp.obj `
    /Fe:target/interop/cpp.exe

rustc --edition=2024 -C opt-level=3 `
    -C target-cpu=native `
    sample/interop/rust.rs `
    -L native=target/interop `
    --out-dir target/interop

.\target\interop\cpp.exe cpu
.\target\interop\cpp.exe gpu
.\target\interop\rust.exe cpu
.\target\interop\rust.exe gpu
```


每次处理一百万像素；limit=256 是每个像素的最大递推次数。程序分别报告 create 的 setup_ms、各宿主串行的 serial_ms、完整 DLL 调用的 blink_call_ms、加速比、差异像素数与校验和。算法不包含文件输出的计时。两次预热后测量九次单次调用，取中位数，方法与全书其他案例一致。

CPU 与两个串行实现逐项相同；当前 GPU 分形边界的 f32 迭代计数有差异，程序会如实输出。create 复用时只发生一次；冷启动只调用一次，应将 setup_ms 与 blink_call_ms 相加。C++ 另输出 target/interop/fractal.pgm（1000×1000 灰度图），图像工具可直接读取。

这四条带 cpu／gpu 参数的命令只用于同一驱动的性能对照；普通 DLL 宿主在 C++／Rust 源码的 Backend 中选择设备。

## Rust 原生 API

```powershell
cargo run --release --manifest-path sample/interop/Cargo.toml
```

native.rs 在源码中以 `let use_gpu = false;` 选择 CPU；改为 true 后走显式设备、上传、提交与下载路径。两种选择均应输出 [3.0, 5.0, 7.0, 9.0]。native.rs 直接消费既有 blanguage、blcompute；Rust 没有被改成只能调用 C ABI。关于数据布局、同步完成、部署、支持类型及 B 反向调用的当前边界，见 [原生集成文档](../../Open/Docs/NativeInterop.md)。
