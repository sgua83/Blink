# B 应用调用 C++ 和 Rust 库

这个独立应用从 B 的 main 调用 C++ 返回普通结构体，再把自己的同一份结构体借给 Rust 修改，最后显示 `15`、`21`。主流程由 B 拥有。

在安装 Rust MSVC、Visual Studio C++ 工具和 Windows SDK 的机器上，打开 x64 Developer PowerShell，在仓库根目录执行：

```powershell
cargo build --workspace --release
New-Item -ItemType Directory target/extern

cl /nologo /LD /MD /O2 /EHsc sample/extern/cpp.cpp `
    /Fo:target/extern/cpp.obj /Fe:target/extern/blink_cpp.dll

rustc --edition=2024 --crate-type cdylib -C opt-level=3 -C panic=abort `
    sample/extern/rust.rs -o target/extern/blink_rust.dll

.\target\release\blc.exe build-exe sample/extern/start.b target/extern/start.exe
.\target\extern\start.exe
$LASTEXITCODE
```

输出为两行 `15`、`21`，正常退出码 `0`。B 编译器直接生成并链接 x64／PE 机器码，按 `.h` 中的 DLL 与符号名称导入函数。C++ 和 Rust 工具链只用于构建各自提供的外部库。

部署时把 `start.exe`、`blink_cpp.dll` 与 `blink_rust.dll` 放在同一目录；此例的 C++ 库使用 `/MD`，目标机需要相应的 MSVC 运行库。运行 B 应用无需 B 源码、blrun 或编译工具链。既有 C/C++／Rust 调用 B 的用法仍见 [原生集成](../../Docs/NativeInterop.md)。

`extern "C" 返回类型 函数(参数) on cpu from "库.dll" as "符号";` 声明可信的外部 C ABI。`as` 可省略，此时符号名使用函数名；外部声明通常放在 `.h`，无需在 `.b` 写空包装。`read T`、`write T`、`inout T` 分别是只读、完整输出、读写位置指针；普通 `T` 按值传递，void 不返回值。B 保留类型、初始化与独占借用检查，借用直接指向调用方存储。

声明者负责匹配真实 DLL 的导出名称、布局、调用约定及读写行为。外部代码不能保留借用、越过所声明对象访问内存，或让异常／longjmp 跨过 C 边界；write 参数须在每次正常返回前完整初始化全部逻辑字段。这里的库都遵守这一约定。

当前目标为 Windows x64 标准 C ABI，普通 32 位标量、按值 POD 结构体／向量／固定数组与位置借用已接入。bool 在此数据 ABI 中使用 32 位的 `uint32_t／u32`，不能直接替换为 C++／Rust 的单字节 bool。GPU／compute 不能调用这些 CPU 函数；其他平台、裸指针值／通用 u64、回调、变参、C++ 类 ABI 与静态对象链接仍待实现。完整规则见 [语言规范](../../Docs/BLanguage.md)。
