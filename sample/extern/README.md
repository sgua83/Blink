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

.\target\release\bc.exe -c `
    sample/extern/start.b `
    -o target/extern/start.obj
.\target\release\blink.exe `
    target/extern/start.obj `
    -o target/extern/start.exe
.\target\extern\start.exe
$LASTEXITCODE
```

输出为两行 `15`、`21`，正常退出码 `0`。B 编译器生成 x64 目标文件，B 链接器生成 PE 应用，按 `.h` 中的 DLL 与符号名称导入函数。C++ 和 Rust 工具链只用于构建各自提供的外部库。

部署时把 `start.exe`、`blink_cpp.dll` 与 `blink_rust.dll` 放在同一目录；此例的 C++ 库使用 `/MD`，目标机需要相应的 MSVC 运行库。运行 B 应用无需 B 源码、brun 或编译工具链。既有 C/C++／Rust 调用 B 的用法仍见 [原生集成](../../Docs/NativeInterop.md)。

`extern "C" 返回类型 函数(参数) on cpu from "库.dll" as "符号";` 声明可信的外部 C ABI。`as` 可省略，此时符号名使用函数名；外部声明通常放在 `.h`，无需在 `.b` 写空包装。`read T`、`write T`、`inout T` 分别是只读、完整输出、读写位置指针；普通 `T` 按值传递，void 不返回值。B 保留类型、初始化与独占借用检查，借用直接指向调用方存储。

声明者负责匹配真实 DLL 的导出名称、布局、调用约定及读写行为。外部代码不能保留借用、越过所声明对象访问内存，或让异常／longjmp 跨过 C 边界；write 参数须在每次正常返回前完整初始化全部逻辑字段。这里的库都遵守这一约定。

本例使用Windows x64标准C ABI的u32、普通POD聚合及位置借用。当前支持的宽／窄标量、地址、函数指针与布局统一见[语言规范](../../Docs/BLanguage.md)和[原生集成](../../Docs/NativeInterop.md)，不在示例维护另一份能力清单。GPU／compute不能调用这些CPU DLL函数；目标、签名、实际存储与代码寿命继续按对应正式合同交接。
