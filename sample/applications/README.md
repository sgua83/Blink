# 独立应用

在仓库根目录使用 Release 编译器构建，然后直接运行生成的 Windows x64 应用：

```powershell
cargo build --workspace --release
.\target\release\blc.exe build-exe sample/applications/start.b target/applications/start.exe
.\target\applications\start.exe
$LASTEXITCODE
```

程序在标准输出显示 `3`，正常退出码为 `0`；输出失败时返回 `1`。`println` 接受一个 `u32`、`i32`、`f32` 或 `bool` 标量，附加 LF 换行，完整写出时返回 `true`。这个示例只有 B 源码，运行生成的 exe 无需 B 源码、编译器、运行器或另写的 Rust／C++ 宿主。

入口在 `.h` 中公开，在 `.b` 中实现，形式为无参数的 `u32 main() on cpu` 或 `i32 main() on cpu`。返回值按原始 32 位作为进程退出码。`build-exe` 的可选参数为模块根目录和具体入口，默认入口是 `main`；普通函数、条件、顺序循环和固定数组使用既有语言规则。

当前输出为 PE32+，目标是 Windows x64 控制台应用。启动和标准输出使用 Windows 系统接口，使用超越函数时导入系统 UCRT 数学入口；调用 C++／Rust DLL 见 [外部 C ABI 示例](../extern/README.md)。字符串、输入、文件、命令行参数、其他平台和源码调试继续研发。

## 浮点结果直接输出

```powershell
.\target\release\blc.exe build-exe sample/applications/floating.b target/applications/floating.exe
.\target\applications\floating.exe
```

`floating.b` 直接输出 `0.333333343`、`1.41421354` 和 `-0`，正常退出码为 `0`。f32 使用 C 数值区域的 `%.9g`，最多 9 位有效数字，以普通或科学计数法显示当前单精度值；小数点固定为 `.`，保留负零。这不是把 1／3 或平方根变成精确实数。普通值应用仍只部署 exe，系统 UCRT 负责浮点格式化。

## 由 B 组织动态数据和 CPU／GPU 计算

```powershell
.\target\release\blc.exe build-exe sample/applications/datasets.b target/applications/datasets.exe sample
.\target\applications\datasets.exe
```

`datasets.b` 创建一百万条记录，调用 CPU `seed`，显式上传输入、筛选输出、排序输出和计数，再调用 GPU `select／order`。GPU 两次提交之间保留数据与有效域，没有读回 Count；随后显式下载排序结果及计数，调用 CPU `summarize`。正常输出 `500000` 和 `891896832`，退出码为 `0`；第二个数是 B 的 u32 回绕结果。

`dataset<T>(length)` 创建全零、由当前 CPU 应用拥有的存储，`let rows = dataset<T>(length)` 可推导资源类型；固定形状可写 `dataset<T,N>` 或 `dataset<T,N>(N)`。CPU 下标与有界遍历沿用原类型和范围规则，普通函数可以 `read dataset<T>` 借用。`upload(rows)`、`download(rows)` 明确转移这份资源的物理存储；`run_cpu(program(args))`、`run_gpu(program(args))` 只接受对应存储，CPU 读取 GPU 数据前也须下载。资源离开作用域、函数返回、break 或 seek 时释放，GPU 物理内存继续由实际最后使用点持有。

这个应用部署为 exe 加编译器生成的 `blink_app_*.dll`，应一起复制同目录。源码 SDK 构建时，Rust 工具链只链接执行框架与平台 ABI；main 和 program 均为 B 的原生编译结果。部署无需 SDK、Rust 工具链、B 源码或 blrun；GPU 部分需要系统 Vulkan 驱动。仅使用普通值的 `start` 仍只产生 exe。
