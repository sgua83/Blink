# 独立应用

在仓库根目录使用 Release 编译器构建，然后直接运行生成的 Windows x64 应用：

```powershell
cargo build --workspace --release
.\target\release\blc.exe build-exe sample/applications/start.b target/applications/start.exe
.\target\applications\start.exe
$LASTEXITCODE
```

程序在标准输出显示 `3`，正常退出码为 `0`；输出失败时返回 `1`。`println` 接受一个 `u32`、`i32` 或 `bool` 标量，附加 LF 换行，完整写出时返回 `true`。这个示例只有 B 源码，运行生成的 exe 无需 B 源码、编译器、运行器或另写的 Rust／C++ 宿主。

入口在 `.h` 中公开，在 `.b` 中实现，形式为无参数的 `u32 main() on cpu` 或 `i32 main() on cpu`。返回值按原始 32 位作为进程退出码。`build-exe` 的可选参数为模块根目录和具体入口，默认入口是 `main`；普通函数、条件、顺序循环和固定数组使用既有语言规则。

当前输出为 PE32+，目标是 Windows x64 控制台应用。启动和标准输出使用 Windows 系统接口，使用超越函数时导入系统 UCRT 数学入口；调用 C++／Rust DLL 见 [外部 C ABI 示例](../extern/README.md)。应用动态 dataset、CPU／GPU program 调用、字符串、输入、文件、命令行参数和源码调试仍待实现，不能把此示例作为完整应用开发能力已完成的依据。
