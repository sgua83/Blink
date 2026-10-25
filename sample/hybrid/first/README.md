# Rust 调用 B：第一个完整例子

自己编写的源码是 numbers.h、numbers.b、main.rs。B 将数组中每个数乘二再加一，Rust 准备四个数，调用后打印结果。gpu.rs 使用相同算法，只把后端改为 GPU。

先按仓库 README 安装 Git、Rust MSVC 和 Visual Studio C++ 桌面开发工具。打开 x64 Developer PowerShell，在 Blink 根目录执行：

```powershell
cargo build --workspace --release
.\target\release\blc.exe build-dll `
    sample/hybrid/first/numbers.b `
    target/hybrid/numbers.dll `
    sample/hybrid/first transform

rustc --edition=2024 `
    sample/hybrid/first/main.rs `
    -L native=target/hybrid `
    -o target/hybrid/numbers.exe
.\target\hybrid\numbers.exe
```

输出：

```text
before: [1.0, 2.0, 3.0, 4.0]
after:  [3.0, 5.0, 7.0, 9.0]
```

blc 生成 numbers.dll、numbers.lib、numbers.rs 和 numbers.abi.h。生成的 numbers.rs 是 Rust 的调用接口，由 main.rs 的 mod numbers 引入；它与自己编写的 main.rs 是两个文件。rustc 编译 main.rs，并链接 numbers.lib；最终 numbers.exe 在运行时加载同目录的 numbers.dll。

GPU 版继续使用这些生成文件：

```powershell
rustc --edition=2024 `
    sample/hybrid/first/gpu.rs `
    -L native=target/hybrid `
    -o target/hybrid/numbers-gpu.exe
.\target\hybrid\numbers-gpu.exe
```

输出的四个数相同。GPU 版本要求驱动提供所需 Vulkan 能力，无需 Vulkan SDK。这个小输入用来理解调用，较大规模的效率比较在书中第十章。

[完整讲解](../../Blink和Rust混合编程.md) 的第一、二章逐步解释文件、命令和调用过程。
