# 将 B 算法接入 C、C++ 和 Rust

在 Blink 根目录、Visual Studio x64 Developer PowerShell 中执行。先按仓库 README 安装 Git、Rust MSVC、C++ 工具并运行 cargo build --workspace --release。这里的 DLL 和头文件均由 blc 生成，宿主不需要自己编写 Rust DLL 工程。

## 从四个数开始

```powershell
$blc = '.\target\release\blc.exe'
$out = 'target/interop'
& $blc build-dll sample/lessons/map.b `
    "$out/map.dll" sample scale

cl /nologo /utf-8 /EHsc /I $out `
    sample/interop/map.cpp "$out/map.lib" `
    /Fo:target/interop/map-cpp.obj `
    /Fe:target/interop/map-cpp.exe
& "$out/map-cpp.exe"

cl /nologo /utf-8 /I $out `
    sample/interop/map.c "$out/map.lib" `
    /Fo:target/interop/map-c.obj `
    /Fe:target/interop/map-c.exe
& "$out/map-c.exe"

rustc --edition=2024 -C opt-level=3 `
    sample/interop/map.rs `
    -L native=target/interop `
    -o target/interop/map-rust.exe
& "$out/map-rust.exe"
```

C 和 C++ 输出 3 5 7 9；Rust 输出 [3.0, 5.0, 7.0, 9.0]。C++ 析构与 Rust Drop 释放 DLL handle，数组仍属于各自的宿主。

## 一百万像素的分形：完整调用计时

```powershell
& $blc build-dll sample/interop/fractal.b `
    "$out/fractal.dll" sample render

cl /nologo /utf-8 /std:c++17 `
    /O2 /arch:AVX2 /fp:precise /EHsc `
    /I $out sample/interop/cpp.cpp `
    "$out/fractal.lib" `
    /Fo:target/interop/cpp.obj `
    /Fe:target/interop/cpp.exe

rustc --edition=2024 -C opt-level=3 `
    -C target-cpu=native `
    sample/interop/rust.rs `
    -L native=target/interop `
    --out-dir target/interop

& "$out/cpp.exe" cpu
& "$out/cpp.exe" gpu
& "$out/rust.exe" cpu
& "$out/rust.exe" gpu
```

编译器、执行框架及生成的 DLL 均为 Release；C++ 使用 /O2，Rust 使用 -C opt-level=3。2026-10-26 的重新实测见 [完整运行记录](../performance/运行记录.txt)。

每次处理一百万像素；limit=256 是每个像素的最大递推次数。程序分别报告 create 的 setup_ms、各宿主串行的 serial_ms、完整 DLL 调用的 blink_call_ms、加速比、差异像素数与校验和。算法不包含文件输出的计时。两次预热后测量九次单次调用，取中位数，方法与全书其他案例一致。

CPU 与两个串行实现逐项相同；当前 GPU 分形边界的 f32 迭代计数有差异，程序会如实输出。create 复用时只发生一次；冷启动只调用一次，应将 setup_ms 与 blink_call_ms 相加。C++ 另输出 target/interop/fractal.pgm（1000×1000 灰度图），图像工具可直接读取。

## Rust 原生 API

```powershell
cargo run --release `
    --manifest-path sample/interop/Cargo.toml `
    -- cpu
cargo run --release `
    --manifest-path sample/interop/Cargo.toml `
    -- gpu
```

两个命令均输出 [3.0, 5.0, 7.0, 9.0]。native.rs 直接消费既有 blanguage、blcompute；Rust 没有被改成只能调用 C ABI。关于数据布局、同步完成、部署、支持类型及 B 反向调用的当前边界，见 [原生集成文档](../../Docs/NativeInterop.md)。
