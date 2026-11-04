# 安装、应用与工具指南

[返回产品主页](../README.md) · [English guide](GettingStarted.en.md) · [Wiki 学习导航](https://github.com/sgua83/Blink/wiki)

本指南提供完整安装、CPU／GPU 应用、原生集成与研发工具说明。正式版与 main 分别记录，按所用版本选择配套源码。

## 快速开始

正式版0.14.1.1不接受在 `println` 中直接使用字符串字面量；当前 main 的该内建操作也只接受数值和 bool 标量。因此首例明确输出数字，不把未经验证的文本 Hello World 写成已支持。应用入口必须 `on cpu`；`on compute` 不是 GPU 应用主入口。

## 从一个完整程序开始

第一个程序输出 `3`。下面两份文件就是全部源码：`.h` 声明入口，`.b` 给出实现；`module hello` 声明本模块的名字。

**hello.h**

```c
module hello;

u32 main() on cpu;
```

**hello.b**

```c
module hello;

u32 main() on cpu {
    println(1 + 2);
    return 0;
}
```

### 同一个计算函数，先在 CPU 运行

下面两版都在各自文件中完整定义逐字相同的 `fill_numbers`，计算相同的四个值并输出 `3、5、7、9`。CPU 主流程直接调用这个函数；`on compute` 表示函数能面向两端编译，不会自动选择设备。

<details>
<summary>展开完整 CPU 并行程序</summary>

**parallel.h**

```c
module parallel;

u32 main() on cpu;
```

**parallel.b**

```c
module parallel;

void fill_numbers(write dataset<f32> values) on compute {
    parallel (value in values) {
        value = f32(index() + 1) * 2.0 + 1.0;
    }
}

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    fill_numbers(values);

    for (value in values) {
        println(value);
    }
    return 0;
}
```

</details>

### 同一段循环也能用于 GPU

GPU 版保留完全相同的函数、四条数据分配、公式和输出；主流程只在下面几行体现执行位置与传输的差异：

| CPU | GPU |
|---|---|
| `fill_numbers(values);` | `upload(values);`<br>`target(gpu, fill_numbers(values));`<br>`download(values);` |

<details>
<summary>展开完整 GPU 例子：计算与传输逐项可见</summary>

**gpu.h**

```c
module gpu;

u32 main() on cpu;
```

**gpu.b**

```c
module gpu;

void fill_numbers(write dataset<f32> values) on compute {
    parallel (value in values) {
        value = f32(index() + 1) * 2.0 + 1.0;
    }
}

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    upload(values);
    target(gpu, fill_numbers(values));
    download(values);

    for (value in values) {
        println(value);
    }
    return 0;
}
```

`upload` 建立 GPU 数据位置，`target(gpu, ...)` 执行同一个 `fill_numbers`，`download` 将结果交回 CPU 后再输出。CPU 版已经直接展示普通调用，无需 CPU target 包装。自动选设备与内联 GPU 块语法尚未实现。

</details>

这些例子仅使用内建操作；保存对应的两份完整文件后，按下节命令构建。

## 下载并运行

1. 下载并双击安装 [Windows x64 正式版 v0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1) 的 `Blink-Setup-windows-x64.exe`，新终端即可使用 `bc` 与 `blink`。
2. 准备 [Rust stable MSVC](https://rust-lang.org/tools/install) 和 [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（x64 C++ 工具与 Windows SDK），打开 x64 Developer PowerShell。这些工具用于构建，运行成品不需要 Rust 或源码。
3. 在空目录把上面的完整代码保存为 `hello.h` 与 `hello.b`，使用安装目录中的正式工具编译、链接并运行：

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c hello.b -o target/hello.obj
& "$env:ProgramFiles/Blink/bin/blink.exe" target/hello.obj -o target/hello.exe
.\target\hello.exe
```

程序输出 `3`。这些独立例子使用正式工具 **0.14.1.1** 核验，不需要克隆仓库或导入自定义教学模块。后续并行例子保存对应的两份文件，同样编译 `parallel.b` 或 `gpu.b` 即可；CPU 入门无需 GPU。[完整指南](GettingStarted.md#快速开始)说明 GPU 能力要求与生成 DLL 的交付方式。


### 4. 从已有 C／C++／Rust 工程调用

[DLL 集成示例](../sample/interop/README.md)和 [Rust 第一次调用](https://github.com/sgua83/Blink/blob/v0.14.1.1/sample/hybrid/first/README.md)在宿主代码中用 `Backend::Cpu`／`Backend::Gpu` 选择后端；工具生成 C ABI 头文件、导入库、C++ 包装与 Rust 绑定。普通宿主无需命令行设备参数；仅性能对照驱动保留这种参数。支持类型及责任见[正式版 ABI 文档](https://github.com/sgua83/Blink/blob/v0.14.1.1/Docs/NativeInterop.md)。

<a id="first-dll"></a>

### 用正式工具构建第一个 DLL

这个可选集成教程使用 Git 取得固定源码快照；在该仓库根目录、x64 Developer PowerShell 中执行。已安装的正式工具可直接生成 DLL；C、C++ 与 Rust 宿主都在各自源码中选择 CPU。下面生成同一算法库并分别运行三种宿主：

下面就是计算库入口的完整计算：输入由宿主数组 `[1, 2, 3, 4]` 提供，`scale` 是实际导出的算法，不是隐藏初始化的演示包装。

```c
void scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}
```

```powershell
git clone https://github.com/sgua83/Blink.git Blink
Set-Location Blink
git checkout cde16596f1532894a77490799783cdcf471ef172
```

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c sample/lessons/map.b -o target/interop/map.obj -I sample --program scale
& "$env:ProgramFiles/Blink/bin/blink.exe" target/interop/map.obj -o target/interop/map.dll
cl /nologo /utf-8 /EHsc /I target/interop sample/interop/map.cpp target/interop/map.lib /Fo:target/interop/map-cpp.obj /Fe:target/interop/map-cpp.exe
.\target\interop\map-cpp.exe
cl /nologo /utf-8 /I target/interop sample/interop/map.c target/interop/map.lib /Fo:target/interop/map-c.obj /Fe:target/interop/map-c.exe
.\target\interop\map-c.exe
rustc --edition=2024 -C opt-level=3 sample/interop/map.rs -L native=target/interop -o target/interop/map-rust.exe
.\target\interop\map-rust.exe
```

C／C++ 输出 `3 5 7 9`，Rust 输出 `[3.0, 5.0, 7.0, 9.0]`。将生成的宿主 EXE、`map.dll` 及链接器产生的配套 DLL 一起交付；资源和支持类型仍按 ABI 文档约定。

### 从 main 构建研发工具

`main` 持续研发，根 [VERSION](../VERSION) 是主线软件版本，可能领先正式版。下面保留主线源码 SDK 的构建方式；配套原书与相对文档链接随 main 演进，基础应用已按前述正式工具版本验证；旧标签中的书稿属于对应发行的历史快照，不含当前应用入口。

构建当前编译器前，先安装一份支持当前前端 B 源码的稳定 Blink 种子。默认 `compiler` 功能保留完整编译接口；原 `build.rs` 从 `%ProgramFiles%/Blink/bin` 调用 `bc` 与 `blink`，静态生成 B 词法及泛型前瞻模块，不以 target 中的新工具作为自己的构建前提。仅执行角色关闭 `compiler` 功能，不调用前端种子或装入源码前端。

准备 Windows x64、[Rust stable MSVC 工具链](https://rust-lang.org/tools/install)和 [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（x64 C++ 工具与 Windows SDK）。在 x64 Developer PowerShell 中执行：

```powershell
git clone https://github.com/sgua83/Blink.git Blink-main
Set-Location Blink-main
cargo build --workspace --release
```

后续命令均在 Blink 根目录执行。CPU 入门无需 GPU；GPU 版本需要满足程序能力要求的 Vulkan 驱动，无需 Vulkan SDK。当前 f32 示例还要求 `DenormPreserve` 与 `SignedZeroInfNanPreserve`，能力不足会明确报错。

正式发行计划于每周日北京时间 **21:00（UTC+8）启动**构建和发布，完成时间取决于构建与上传，实际已发布版本以 [Releases](https://github.com/sgua83/Blink/releases)为准。新版覆盖安装，只保留当前版，原研发根和 target 不改；同一安装程序支持管理员 `--quiet` 部署。发布、安装与稳定种子的输入／输出边界见[工具说明](../Tools/README.md)和[版本说明](Versioning.md)。`bc --version`、`blink --version`、`brun --version`显示各自构建时的版本。

## 工具与原生集成

| 工具／库 | 用途 |
|---|---|
| `bc` | 检查源码、编译 COFF 目标文件、导出 CPU／GPU 计算代码 |
| `blink` | 链接已编译对象，生成 Windows x64 exe 或计算库（DLL） |
| `brun` | 后端隔离测试／诊断使用的模块运行器；普通应用直接运行 EXE |
| `blanguage` | 前端、类型与访问检查、并行 IR、优化和原生后端 |
| `blcompute` | 数据存储、设备资源、传输、原生执行与完成后的释放 |

编译参数容量使用`bc --max-parameter-bytes N -c ...`显式选择，Rust调用方使用`Compiler::with_max_parameter_bytes`；默认128字节。实际布局要求随产物交给设备加载，配置、缓存及目标选择见[语言规范](BLanguage.md#abi-与代码生成)。

你可以用 B 语言构建独立应用，也可以把用 B 语言编写的计算函数编译成 DLL，供 **C、C++ 与 Rust** 程序调用。工具链同时生成 C ABI 头文件、导入库、C++ 包装和 Rust 绑定；原生 Rust API 支持驻留资源和异步提交。

- [独立应用](../sample/applications/README.md)：CPU 入口、动态数据与显式 CPU／GPU 计算。
- [C／C++／Rust 调用 B 语言程序](../sample/interop/README.md)：从四个数开始，构建并调用同一份计算库（DLL）。
- [B 语言调用外部 DLL](../sample/extern/README.md)：C ABI、参数访问与真实资源边界。
- [原生集成与部署](NativeInterop.md)：产物、ABI、部署要求及当前限制。

### 面向 AI 的工具与系统研发

编译、调试、观察与基础量化工具以AI为唯一直接使用者，架构师通过AI掌握真实代码、数据和执行。长期支持AI编写观察／分析程序、在数据附近计算、表达明确机器方案并组合真实输入实验；优化策略、调试信息和采集独立，AI负责分析与选择。总原则、逐项职责及当前基础见[面向AI的基础工具](Architecture.md#面向ai的基础工具与全流程观察)；这些方向仍在研发，不代表已有完整调试／实验工具，也不增加自动瓶颈判断、评分或另一运行状态权威。

B按真实职责推进编译器及调试／观察主体的自举与接管，并建设完整系统程序所需的接口与库。正常系统ABI声明和有职责的B库封装应复用，必要独立Rust支持可保留；不以百分之百B、源码比例或B外壳计完成。边界和当前状态见[架构设计](Architecture.md#长期系统设计目标与语言边界)。

可选CPU栈报告由`bc -c ... --stack-report <文件>`或`blink --stack-report <已编译输入>... -o <文件>`生成，展示实际帧、调用链上界及递归／外部／间接未知范围；可声明真实调用前预算，仅作报告。PE容量与真实线程剩余容量分别说明，工具不限制递归或改写执行。命令与范围见[静态栈报告](BLanguage.md#cpu调用链的静态栈报告)。

AI可用`bc inputs <compiled.obj|entry.blbi>`直接查询产物保存的实际构建请求与来源记录，输出JSON，不读取当前源码或重新编译。字段、摘要覆盖范围和无记录时的行为见[实际构建输入查询](BLanguage.md#实际构建输入查询)；该查询不代表完整调试工具已经交付。

AI可用`bc layout <module.h|module.b> <type> [-I module-root] [--field path]`查询本次具体类型或字段的JSON布局；`bc build`同时输出.layout.json，已有执行接口也可由SDK直接查询。值描述、元素存储及资源字覆盖的边界见[类型与字段布局查询](BLanguage.md#类型与字段布局的结构查询)。

AI可用`bc ir <module.b>`按原模块、具体并行入口或普通应用取得实际IR的JSON，并选择完整函数名或优化后阶段；SDK可直接查询同一编译返回值。函数／程序IR的source_origin仅保留真实源码定义的快照、范围与实例对应关系；合成函数及产物接口不伪造原定义，已知实例／后续真实内联来源分别保持；已取得IR在文件变化后仍可查原事实；每个原IR操作可沿source_site查询发射时的语法范围，缺失材料明确为null；同一来源还保留实际特化实例和从外到内的真实内联展开点；SourceSite::domain_path()及domain_path数组独立交接源码域展开和已编译域边界，真实调用者／目标与原范围随原IR保持，不把域路径当普通内联或运行期调用栈；Inst／Stage／ValueCode的generated_origin交接真实生成原因，语法与定义实例锚点按kind区分；程序入口／根Frame、dataset原分配Call、生成函数初始化、新绑定／重新赋值槽位、最终存储释放Call、拥有元素逆序清理循环、用户拥有值静态析构Call、条件拥有清理分支／条件准备、活动变体清理选择、resize填充值暂存／实际扩容Call、Frame地址请求常量／Call、存储地址查询Call、根地址表示准备、索引／步长寻址准备及CPU写区准备已有独立生成来源，原并行索引／空前缀语法触发保持；其余资源包装仍待实现；parallel及无普通Inst的集合调用可沿Stage::operation_source()及阶段JSON的operation_source查询原语句／调用标识符范围和实际实例；CPU应用提取保留原语句定义与原特化实例，生成执行名称不冒充源码函数；融合后的完整多来源仍待实现，各project／identity／combine可沿ValueCode::entry_origin()查询真实目标实例、静态引用及有序捕获参数来源，空代码值核也保留入口；优化变换与机器映射继续独立交付，见[操作语法来源](BLanguage.md#ir操作的语法来源)和[原定义来源](BLanguage.md#函数与程序ir的原定义来源)。实际阶段、逻辑引用和无IR产物的边界见[实际IR查询](BLanguage.md#实际ir的按需结构查询)。

AI可用`bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]`查询明确指令范围的原字节、操作数、寻址和静态控制流；SDK使用`native::instructions_json`。生成期映像可用NativeCode／StageCode／ProgramImage的`code_json`查询真实主／辅助代码、对齐和字面量；`bc build`同时导出`.x64.json`。原SDK可显式保存独立CPU范围材料，`bc code <entry.blbi> --observations <ranges.blcr>`及`bc code --image <deployment.blri> --observations <ranges.blcr>`可无源码恢复查询；执行编码不带材料，`bc code --object <compiled.obj> [--section number | --function exact-symbol]`查询原COFF物理节／符号／重定位及.pdata已声明指令范围，原Object可显式保存独立`.blcf`，`--observations <ranges.blcf>`匹配后交接普通函数／辅助代码／对齐／字面量完整分区；无材料保持未分类，最终PE另行交付，见[映像范围查询](BLanguage.md#生成期原生映像的范围与指令查询)。

AI可用`bc spirv <module.spv>`或compiler SDK的`spirv::module_json`只读取得原单模块全部指令、偏移、原字、类型／结果／操作数ID、入口与字面量位模式；不重编或执行GPU。原ShaderCode／StageCode／ProgramImage的code_json保留全部主／辅助模块，bc build为每份.spv导出配对.spv.json；纯GPU可用`bc code <entry.blbi>`或`bc code --image <deployment.blri>`无源码查询，不要求CPU范围材料。SPIR-V仍是中间表示，见[单模块结构查询](BLanguage.md#原spir-v单模块的完整结构查询)。

`bc image <entry.blbi> -o <deployment.blri>`直接导出原CPU／GPU执行映像，原构建输入记录与缓存保持；关闭compiler功能的Rust运行SDK可独立装入并执行，见[部署映像](BLanguage.md#构建记录与可部署执行映像)。

## 能力与发展方向

| 范围 | 当前状态 |
|---|---|
| CPU／GPU 计算 | 已有自研 x64／SPIR-V、并行域、集合运算、显式原子、tile／subgroup 协作 |
| 数据与静态抽象 | 已有记录、数组、向量／矩阵、泛型、开放 `u32 const`、访问契约和有效前缀 |
| 应用与模块 | 已有独立 CPU 应用、普通原生调用／递归及有限的独立模块、多对象链接能力 |
| 平台 | 当前为 Windows x64 CPU 与 Vulkan GPU；其他平台和 CPU 架构待实现 |
| 计算域 | 原生域编译与域运行已确定设计边界，尚未实现 |
| 稳定热更新 | 版本绑定、状态迁移和资源交接已列入架构，尚未实现 |

普通CPU应用可直接移动、补回固定拥有数组的动态位置，并沿实际分支、for／while和返回释放仍拥有的资源。 动态拥有元素可由`dataset<T>(length,F)`静态逐项构造，完整序列移动／返回与最后元素释放沿同一拥有树；能力及字段借用／追加／取出等剩余范围见[拥有元素序列](BLanguage.md#cpu-owned-dataset-elements)。普通CPU函数可用`requires nonzero(rows[*].value)`交接共同非零前提，也可用`requires nonempty(rows[*].value.values)`交接已初始化元素的共同子资源非空前提，同一元素的`requires index(rows[*].value.index,rows[*].value.values)`还可交接实际索引与子资源关系；范围见[共同值](BLanguage.md#common-element-nonzero)、[共同资源](BLanguage.md#common-element-nonempty)和[索引关系](BLanguage.md#common-element-index)。returned与write／inout的条件CPU／GPU输出位置可经普通函数、对象和归档交接，条件输入按实际使用路径交接，循环提前退出的跨函数输出位置也沿真实出口关系交接；当前对象与资源ABI见[唯一版本说明](BLanguage.md#已编译模块的资源abi)，精确范围见[语言规范](BLanguage.md#跨函数模块的条件资源输出位置)。

B 语言 **不采用面向对象**，以数据、函数、模块和执行域组织程序。长期设计同时重视顶级效率与机器控制、可检查的接口，以及人、AI 代码生成工具和上层编译器的共同使用。当前子集、原生运行与性能结果分别记录于[语言规范](BLanguage.md)和[现行架构](Architecture.md)。

## 学习与文档

**[进入 Wiki 学习与书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books)** · [按目标选书与安装入门](https://github.com/sgua83/Blink/wiki) · [English book index](https://github.com/sgua83/Blink/wiki/Blink-Books-English)。Wiki 提供四本书的完整在线正文；书稿跟随开发版 main，正式版使用与发行包匹配的文档和示例。

| 从哪里开始 | 内容 |
|---|---|
| [《并行编程的艺术》](../sample/并行编程的艺术.md) | 从 C／C++ 走向并行思考；[手机 PDF](../sample/pdf/并行编程的艺术-B语言-手机阅读版.pdf) · [标准 PDF](../sample/pdf/并行编程的艺术-B语言.pdf) |
| [《Blink 和 Rust 混合编程》](../sample/Blink和Rust混合编程.md) | 从最小调用到模拟与连续 GPU 计算；[示例](../sample/hybrid/README.md)；[手机 PDF](../sample/pdf/Blink和Rust混合编程-手机阅读版.pdf) · [标准 PDF](../sample/pdf/Blink和Rust混合编程.pdf) |
| [《B 语言源码分析》](../sample/B语言源码分析.md) | 阅读编译器、IR、后端及执行框架；[版本与阅读资料](../sample/compiler/README.md)；[手机 PDF](../sample/pdf/B语言源码分析-手机阅读版.pdf) · [标准 PDF](../sample/pdf/B语言源码分析.pdf) |
| [《BLEngine 源码分析》](../sample/BLEngine源码分析.md) | 首个消费者如何使用语言、场景与光照；[阅读资料](../sample/engine/README.md)；[手机 PDF](../sample/pdf/BLEngine源码分析-手机阅读版.pdf) · [标准 PDF](../sample/pdf/BLEngine源码分析.pdf) |
| [B标准库](../library/README.md) | 普通值动态序列、严格UTF-8／UTF-16编解码、整段转换与拥有切片，以及Windows系统信息、计时、线程错误码、原始命令行、环境读写、UTF-8文件输入、路径存在性、绝对路径、规范路径身份、扩展名替换及路径组合 |
| [语言规范](BLanguage.md) · [架构](Architecture.md) · [计算域](ComputeDomain.md) | 当前规则、长期边界和后续设计 |

技术文档与书籍目前以中文为主；英文首页提供相同的入门路径和能力说明。上表为 main 的现行书稿与配套示例；基础应用已按前述正式工具版本验证。上表及旧标签中的 PDF 为已有快照，最新内容以在线 Markdown 为准；标准版与手机版 PDF 在周日正式发行时从同次固定书稿统一生成，合集见对应 [Release](https://github.com/sgua83/Blink/releases) 的 `Blink-Books.zip` 资产。BLEngine 是独立消费者，尚未开源；本仓库提供其案例书稿与阅读资料，不提供引擎源码。
