<div align="center">

<h1>Blink</h1>

<p><strong>原生并行 · 统一 CPU／GPU 编程 · 原生域编译与域运行</strong></p>
<p>面向 AI 超算时代的基础语言</p>

<p><strong>简体中文</strong> | <a href="README.en.md">English</a></p>
<p><a href="#快速开始">快速开始</a> · <a href="Docs/BLanguage.md">语言规范</a> · <a href="#学习与文档">学习资料</a> · <a href="Docs/Architecture.md">架构</a></p>

</div>

Blink，简称 **B**，是一门自研原生并行计算语言。B 也寓意 **Base**：让并行、数据关系与协作成为程序的基本表达，并逐步成长为完整的通用与系统编程语言。

接口写在 `.h`，实现写在 `.b`。自研编译器将 B 算法直接编译为 Windows x64 机器码和 SPIR-V，原生执行框架负责数据、设备、传输和真实完成。当前编译器与执行框架由 Rust 实现，无第三方编译器、Shader 编译器或并行运行时依赖。

> **项目状态：研发中，当前版本 0.1.0。** 已有单机 CPU／GPU 原生执行；域编译、域运行和稳定热更新是确定的设计目标，尚未实现。完整范围见[架构](Docs/Architecture.md)与[实现状态](Docs/Issues.md)。

## 三大核心特点

### 1. 原生并行

将并行作为语言的基本能力。并行域、数据访问权限、块内协作与阶段依赖直接进入编译器分析，再落实为原生执行。算法可以表达独占写入、共享只读、显式原子操作和集合计算。

### 2. 统一 CPU／GPU 编程

**同一套 B 源码，面向 CPU 与 GPU 原生执行。** 适用的算法无需为两类硬件分别实现；`on` 声明目标能力，`target` 选择本次执行位置，`upload`／`download` 显式交接数据。硬件能力差异与传输成本保持明确。

以下节选自 [map.b](sample/lessons/map.b)，对应接口在 [map.h](sample/lessons/map.h)：

```c
void scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}
```

`inout` 声明读写关系，`parallel` 为每个实例提供当前元素的独占访问，`on compute` 允许 CPU／GPU 后端执行。

### 3. 原生域编译与域运行

**把一组机器变成可直接使用的编译与执行资源。** 以计算域组织多台机器，原生完成域内编译与运行；编译域和运行域可以独立选择，分片、通信、依赖和完成具有明确语义。

这一能力目前处于设计阶段。一次构建或执行使用所选域内资源，跨域业务级联由上层应用组织。完整职责、部署与接口设计见[计算域方案](Docs/ComputeDomain.md)。

## 快速开始

准备 Windows x64、[Rust stable MSVC 工具链](https://rust-lang.org/tools/install)和 [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（x64 C++ 工具与 Windows SDK）。在 x64 Developer PowerShell 中执行：

```powershell
git clone https://github.com/sgua83/Blink.git
Set-Location Blink
cargo build --workspace --release

.\target\release\blc.exe -c sample/applications/start.b -o target/applications/start.obj
.\target\release\bllink.exe target/applications/start.obj -o target/applications/start.exe
.\target\applications\start.exe
```

程序输出 `3`，正常退出码为 `0`。这个独立 CPU 应用无需 GPU；生成的 exe 运行时无需 B 源码、编译器、运行器或另写的宿主。

再运行一个并行计算示例：

```powershell
.\target\release\blrun.exe cpu sample/lessons/map.b demo sample Saved/map-cpu :4
```

结果写入 `Saved/map-cpu/values.bldata`，四个值为 `3, 5, 7, 9`。将 `cpu` 改为 `gpu` 并选择另一输出目录，即可在满足程序能力要求的 Vulkan 设备上运行同一份源码。GPU 执行需要驱动，无需 Vulkan SDK 或外部 Shader 编译器。当前这个 f32 示例还要求设备支持 `DenormPreserve` 与 `SignedZeroInfNanPreserve` 浮点能力；不满足时直接报告错误。

## 工具与原生集成

| 工具／库 | 用途 |
|---|---|
| `blc` | 检查源码、编译 COFF 目标文件、导出 CPU／GPU 计算代码 |
| `bllink` | 链接已编译对象，生成 Windows x64 exe 或算法 DLL |
| `blrun` | 绑定数据并执行 CPU／GPU 计算模块 |
| `blanguage` | 前端、类型与访问检查、并行 IR、优化和原生后端 |
| `blcompute` | 数据存储、设备资源、传输、原生执行与完成后的释放 |

B 可以独立构建应用，也可以作为 **C、C++ 与 Rust** 的算法实现。算法 DLL 随同生成 C ABI 头文件、导入库、C++ 包装和 Rust 绑定；原生 Rust API 支持驻留资源和异步提交。

- [独立应用](sample/applications/README.md)：CPU 入口、动态数据与显式 CPU／GPU 计算。
- [C／C++／Rust 调用 B](sample/interop/README.md)：从四个数开始，构建并调用同一份算法 DLL。
- [B 调用外部 DLL](sample/extern/README.md)：C ABI、参数访问与真实资源边界。
- [原生集成与部署](Docs/NativeInterop.md)：产物、ABI、部署要求及当前限制。

## 能力与发展方向

| 范围 | 当前状态 |
|---|---|
| CPU／GPU 计算 | 已有自研 x64／SPIR-V、并行域、集合运算、显式原子、tile／subgroup 协作 |
| 数据与静态抽象 | 已有记录、数组、向量／矩阵、泛型、开放 `u32 const`、访问契约和有效前缀 |
| 应用与模块 | 已有独立 CPU 应用、普通原生调用／递归及有限的独立模块、多对象链接能力 |
| 平台 | 当前为 Windows x64 CPU 与 Vulkan GPU；其他平台和 CPU 架构待实现 |
| 计算域 | 原生域编译与域运行已确定设计边界，尚未实现 |
| 稳定热更新 | 版本绑定、状态迁移和资源交接已列入架构，尚未实现 |

B **不采用面向对象**，以数据、函数、模块和执行域组织程序。长期设计同时重视顶级效率与机器控制、可检查的接口，以及人、AI 代码生成工具和上层编译器的共同使用。当前子集、原生运行与性能结果分别记录于[语言规范](Docs/BLanguage.md)和[问题列表](Docs/Issues.md)。

## 学习与文档

| 从哪里开始 | 内容 |
|---|---|
| [《B 语言编程》](sample/B语言编程.md) | 从下载、第一个程序到函数、数据和并行；[完整示例](sample/beginner/README.md) · [手机 PDF](sample/pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](sample/pdf/B语言编程.pdf) |
| [《并行编程的艺术》](sample/并行编程的艺术.md) | 从 C／C++ 走向并行思考 |
| [《Blink 和 Rust 混合编程》](sample/Blink和Rust混合编程.md) | 从最小调用到模拟与连续 GPU 计算；[示例](sample/hybrid/README.md) |
| [《B 语言源码分析》](sample/B语言源码分析.md) | 阅读编译器、IR、后端及执行框架；[版本与阅读资料](sample/compiler/README.md) |
| [《BLEngine 源码分析》](sample/BLEngine源码分析.md) | 首个消费者如何使用语言、场景与光照；[阅读资料](sample/engine/README.md) |
| [语言规范](Docs/BLanguage.md) · [架构](Docs/Architecture.md) · [计算域](Docs/ComputeDomain.md) | 当前规则、长期边界和后续设计 |

技术文档与书籍目前以中文为主；英文首页提供相同的入门路径和能力说明。

## 参与开发

修改前请阅读[工程约定](AGENTS.md)和[实现状态](Docs/Issues.md)。反馈问题时提供 B 源码、实际命令、工具链与设备信息及原始错误；能力目标、实现、原生运行和性能结果分别说明。维护者的提交与发布流程见工程约定。

## 许可证

[MIT](LICENSE)。
