<div align="center">

<img src="assets/blink-banner.svg" alt="Blink：同一份计算源码，面向原生 CPU 与 GPU" width="1040" />

# Blink 编程语言

**把并行计算写进程序，把 B 接入你已有的工程。**

面向 C／C++／Rust 程序员与 AI 的原生并行语言，简称 **B**。

[下载正式版](https://github.com/sgua83/Blink/releases/latest) · [运行第一个程序](#快速开始) · [接入现有项目](#工具与原生集成) · [文档与四本书](#学习与文档) · [Discussions](https://github.com/sgua83/Blink/discussions)

[简体中文](README.md) | [English](README.en.md)

</div>

**当前可用：Windows x64 CPU 与 Vulkan GPU。** 已有独立 B 应用、原生并行计算和 C／C++／Rust DLL 集成。正式发行与持续研发的 [main](VERSION) 分别记录；跨机器域编译、域运行和稳定热更新仍在设计中。

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

[逐行上手指南](Open/Docs/GettingStarted.md#快速开始)解释文件、输出与 GPU 要求。这些例子使用内建操作，全部计算逻辑均在这里可见。

<a id="快速开始"></a>

## 下载并运行

1. 下载并双击安装 [Windows x64 正式版 v0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1) 的 `Blink-Setup-windows-x64.exe`，新终端即可使用 `bc` 与 `blink`。
2. 准备 [Rust stable MSVC](https://rust-lang.org/tools/install) 和 [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（x64 C++ 工具与 Windows SDK），打开 x64 Developer PowerShell。这些工具用于构建，运行成品不需要 Rust 或源码。
3. 在空目录把上面的完整代码保存为 `hello.h` 与 `hello.b`，使用安装目录中的正式工具编译、链接并运行：

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c hello.b -o target/hello.obj
& "$env:ProgramFiles/Blink/bin/blink.exe" target/hello.obj -o target/hello.exe
.\target\hello.exe
```

程序输出 `3`。这些独立例子使用正式工具 **0.14.1.1** 核验，不需要克隆仓库或导入自定义教学模块。后续并行例子保存对应的两份文件，同样编译 `parallel.b` 或 `gpu.b` 即可；CPU 入门无需 GPU。[完整指南](Open/Docs/GettingStarted.md#快速开始)说明 GPU 能力要求与生成 DLL 的交付方式。

<a id="工具与原生集成"></a>

## 接入现有 C／C++／Rust 项目

让 B 负责一段计算，保留已有应用和工程组织。`bc` 编译计算模块，`blink` 输出 DLL、导入库及调用接口。

| 你的工程 | 接入方式 | 从这里开始 |
|---|---|---|
| C | 生成的 C ABI 头文件与 DLL | [四个数的完整调用](Open/Docs/GettingStarted.md#first-dll) |
| C++ | 生成的包装、RAII 和代码内 `Backend` 选择 | [C++ 宿主源码](https://github.com/sgua83/Blink/blob/cde16596f1532894a77490799783cdcf471ef172/sample/interop/map.cpp) |
| Rust | 生成的绑定，或直接使用原生 Rust API | [DLL 宿主源码](https://github.com/sgua83/Blink/blob/cde16596f1532894a77490799783cdcf471ef172/sample/interop/map.rs) · [原生 API](sample/interop/README.md#rust-原生-api) |

普通宿主在代码中选择 CPU／GPU。支持类型、同步完成、资源寿命与部署要求见[正式版 ABI 文档](https://github.com/sgua83/Blink/blob/v0.14.1.1/Docs/NativeInterop.md)；主线的新能力见[现行集成文档](Open/Docs/NativeInterop.md)。B 也支持[通过 C ABI 调用外部 DLL](sample/extern/README.md)。

<a id="三大核心特点"></a>
<a id="能力与发展方向"></a>

## 三大设计，清楚区分实现与方向

| 设计 | 带来的价值 | 当前状态 |
|---|---|---|
| **原生并行** | 用数据域、访问权限和集合运算表达独立工作与必要协作 | 已有单机多核 CPU 与 GPU 执行；并行域、归约、扫描、显式原子与块内协作 |
| **同源 CPU／GPU** | 适用的计算共享一份算法，显式选择目标和数据传输 | 自研原生 x64／SPIR-V 后端；具体 GPU 能力由程序与驱动决定 |
| **原生域编译与域运行** | 将所选机器的资源用于原生编译与分片计算，编译域和运行域可独立选择 | **设计中，尚未实现** |

普通函数、条件和循环保持原生执行；并行域及域协作由源算法和访问合同表达，三大特性不要求整个程序进入通用自动任务DAG。编译器实施合法优化，架构师和AI仍可通过B算法、布局、分块及通信实现控制关键成本；完整明确机器方案和域内多机能力仍待实现。

<a id="面向-ai-的工具与系统研发"></a>

B 以数据、函数、模块和执行域组织程序，不采用面向对象。B 也寓意 **Base**：长期希望成为 AI 超算时代的基础语言。面向 AI 的编译与观察接口持续完善，已有工具及能力范围见[工具指南](Open/Docs/GettingStarted.md#面向-ai-的工具与系统研发)。

当前平台为 Windows x64；其他平台、完整模块能力和稳定热更新仍有未完成范围。GPU 需要匹配程序能力的 Vulkan 驱动，无需 Vulkan SDK。[语言规范](Open/Docs/BLanguage.md)记录精确支持范围，[能力与发展方向](Open/Docs/GettingStarted.md#能力与发展方向)提供详细说明。

<a id="parallel-programming"></a>
<a id="map"></a>
<a id="ownership"></a>
<a id="reduction"></a>
<a id="从并行编程的艺术开始"></a>

## 学习与文档

**[Wiki 学习导航](https://github.com/sgua83/Blink/wiki)** 帮你选书；**[中文书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books)** 提供完整在线正文与 PDF 入口。

| 目标 | 在线阅读 |
|---|---|
| 从 C／C++ 进入并行思考：map、读写归属、归约、扫描与协作 | [《并行编程的艺术》](https://github.com/sgua83/Blink/wiki/Book-Parallel-Programming) |
| 将 B 接入 Rust，理解布局、调用与连续计算 | [《Blink 和 Rust 混合编程》](https://github.com/sgua83/Blink/wiki/Book-Blink-and-Rust) |
| 深入编译器、IR、CPU／GPU 后端和执行框架 | [《B 语言源码分析》](https://github.com/sgua83/Blink/wiki/Book-B-Source-Analysis) |
| 从实际引擎案例理解场景、光照和资源交接 | [《BLEngine 源码分析》](https://github.com/sgua83/Blink/wiki/Book-BLEngine-Source-Analysis) |

[前三章导读](Open/Docs/ParallelPrimer.md)完整保留入门正文、图示和既有性能测量记录，并可继续阅读完整书稿；[上手与工具指南](Open/Docs/GettingStarted.md)承接安装、源码构建、集成和 AI 查询说明。另有[独立应用示例](sample/applications/README.md)、[B 标准库](library/README.md)与[语言规范](Open/Docs/BLanguage.md)。

书籍正文目前以中文为主。[English book index](https://github.com/sgua83/Blink/wiki/Blink-Books-English)与[英文入门三章](Open/Docs/ParallelPrimer.en.md)也可直接阅读。Wiki 跟随 main；现有 PDF 是快照，新 PDF 在周日正式发行时从固定书稿统一生成。**BLEngine 尚未开源**，此处提供其案例书稿与阅读资料。

<a id="从-main-构建研发工具"></a>

### 想研究编译器或从源码构建？

先安装稳定 Blink 种子，再按[主线构建指南](Open/Docs/GettingStarted.md#从-main-构建研发工具)构建研发工具。默认构建直接消费 `%ProgramFiles%/Blink` 中的稳定输入；已发布版本见[Releases](https://github.com/sgua83/Blink/releases)。

<a id="feedback"></a>
<a id="反馈与技术交流"></a>
<a id="参与开发"></a>

## 社区与参与

- **使用问题、经验和技术讨论：** [GitHub Discussions](https://github.com/sgua83/Blink/discussions)。
- **可复现缺陷与功能建议：** [GitHub Issues](https://github.com/sgua83/Blink/issues) · [新建反馈](https://github.com/sgua83/Blink/issues/new)。请附最小源码、工具版本、所用源码提交、完整命令、CPU／GPU／驱动信息与原始错误；性能问题同时说明输入规模、构建档位和计时／传输范围。
- **贡献代码或文档：** 在 Issue 或 Discussion 说明改动及其依据。语言规则见[公开规范](Open/Docs/BLanguage.md)。维护者在 P4 主线集成改动，GitHub 同步公开内容。

## 许可证

基础语言、编译器、标准库和单机运行库按 [MIT](LICENSE) 开放。闭源插件的分发条款随各插件正式制品交付。

## 插件与源码范围

基础语言、编译器和单机运行库的源码位于crates与library，公开规范及教程位于Open。域编译、域运算和域维护底座按独立闭源插件包研发，公开分发目录为[Plugins](Plugins/README.md)。各包采用Binaries/<平台>-<架构>布局，域插件当前尚无可用二进制。
