# 独立应用

在仓库根目录使用 Release 编译器构建，然后直接运行生成的 Windows x64 应用：

```powershell
cargo build --workspace --release
.\target\release\blc.exe -c `
    sample/applications/start.b `
    -o target/applications/start.obj
.\target\release\bllink.exe `
    target/applications/start.obj `
    -o target/applications/start.exe
.\target\applications\start.exe
$LASTEXITCODE
```

程序在标准输出显示 `3`，正常退出码为 `0`；输出失败时返回 `1`。`println` 接受一个 `u32`、`i32`、`f32` 或 `bool` 标量，附加 LF 换行，完整写出时返回 `true`。这个示例只有 B 源码，运行生成的 exe 无需 B 源码、编译器、运行器或另写的 Rust／C++ 宿主。

入口在 `.h` 中公开，在 `.b` 中实现，形式为无参数的 `u32 main() on cpu` 或 `i32 main() on cpu`。返回值按原始 32 位作为进程退出码。`blc -c` 的 `-I` 指定模块根目录，`--entry` 选择具体入口，默认入口是 `main`；`-o` 指定输出目标文件，再由 bllink 单独链接；普通函数、条件、顺序循环和固定数组使用既有语言规则。

当前输出为 PE32+，目标是 Windows x64 控制台应用。启动和标准输出使用 Windows 系统接口，使用超越函数时导入系统 UCRT 数学入口；调用 C++／Rust DLL 见 [外部 C ABI 示例](../extern/README.md)。字符串、输入、文件、命令行参数、其他平台和源码调试继续研发。

## 浮点结果直接输出

```powershell
.\target\release\blc.exe -c `
    sample/applications/floating.b `
    -o target/applications/floating.obj
.\target\release\bllink.exe `
    target/applications/floating.obj `
    -o target/applications/floating.exe
.\target\applications\floating.exe
```

`floating.b` 直接输出 `0.333333343`、`1.41421354` 和 `-0`，正常退出码为 `0`。f32 使用 C 数值区域的 `%.9g`，最多 9 位有效数字，以普通或科学计数法显示当前单精度值；小数点固定为 `.`，保留负零。这不是把 1／3 或平方根变成精确实数。普通值应用仍只部署 exe，系统 UCRT 负责浮点格式化。

## 由 B 组织动态数据和 CPU／GPU 计算

```powershell
.\target\release\blc.exe -c `
    sample/applications/datasets.b `
    -o target/applications/datasets.obj `
    -I sample
.\target\release\bllink.exe `
    target/applications/datasets.obj `
    -o target/applications/datasets.exe
.\target\applications\datasets.exe
```

`datasets.b` 先声明尚无资源的 input，再把零长度拥有存储交给 CPU 函数 `create_input`；函数按请求长度保留或重赋值 storage，调用 `seed` 初始化一百万条记录，再返回实际 dataset 给 main；main 随后显式上传输入、筛选输出、排序输出和计数，再调用 GPU `select／order`。GPU 两次提交之间保留数据与有效域，没有读回 Count；随后显式下载排序结果及计数，调用 CPU `summarize`。正常输出 `500000` 和 `891896832`，退出码为 `0`；第二个数是 B 的 u32 回绕结果。

`dataset<T>(length)` 创建全零、由当前 CPU 应用拥有的存储，`let rows = dataset<T>(length)` 可推导资源类型；固定形状可写 `dataset<T,N>` 或 `dataset<T,N>(N)`。CPU 下标与有界遍历沿用原类型和范围规则，普通函数可以 `read dataset<T>` 借用。`upload(rows)`、`download(rows)` 明确转移这份资源的物理存储；`run_cpu(program(args))`、`run_gpu(program(args))` 只接受对应存储，CPU 读取 GPU 数据前也须下载。资源可由普通 CPU 函数以 dataset<T[,N]> 返回并由调用方接收；返回资源交接所有权，其余本地资源在作用域结束、函数返回、break、continue 或 seek 时释放，GPU 物理内存继续由实际最后使用点持有。借用参数不能当作拥有资源返回；局部初始化和 CPU 拥有形参可以移动资源，原绑定随移动失效。

这个应用部署为 exe 加链接器生成的 `blink_app_*.dll`，应一起复制同目录。源码 SDK 构建时，Rust 工具链只在编译阶段生成执行框架与平台 ABI 的静态目标代码，链接阶段不调用源码编译器；main 和 program 均为 B 的原生编译结果。部署无需 SDK、Rust 工具链、B 源码或 blrun；GPU 部分需要系统 Vulkan 驱动。仅使用普通值的 `start` 仍只产生 exe。
## 整份数据借用给普通 CPU 函数

```powershell
.\target\release\blc.exe -c `
    sample/applications/borrow.b `
    -o target/applications/borrow.obj
.\target\release\bllink.exe `
    target/applications/borrow.obj `
    -o target/applications/borrow.exe
.\target\applications\borrow.exe
```

`borrow.b` 由 main 创建一百万条 Entry，再将同一存储以 `write dataset<Entry>` 交给 prepare 完整初始化，以 `inout dataset<Entry>` 交给 advance 调用 CPU program，以 `read dataset<Entry>` 交给 total 求和。正常输出 `1784293664` 和 `1000000`，退出码为 `0`；求和按 u32 回绕。

inout 函数可在范围内读写和转借，返回后数据仍由 main 拥有；函数只释放自己的本地资源，不能转移借来的 CPU／GPU 存储。同次调用中的独占借用不得与同一存储的其他整份或元素借用重叠，只读参数可以共享。参数类型／固定形状和 nonempty／index 条件继续沿原检查器证明，不复制数据，不增加引用 ABI 或运行时别名检查。

CPU 应用中已初始化存储的整份 inout、拥有 dataset 的函数返回、局部初始化移动、拥有形参、资源重赋值与重新初始化已接通；CPU write dataset 的完整遍历／已知域输出已接通；资源聚合、一般动态子域／数组初始化推导与其他资源构造仍待完成。program 的每个并行实例仍只拥有当前独占元素。
