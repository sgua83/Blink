# 安装 Blink，运行第一个程序

[产品主页](../../README.md) · [English guide](GettingStarted.en.md) · [Wiki 学习导航](https://github.com/sgua83/Blink/wiki)

先运行一个独立 B 程序，再选择并行计算或你所用语言的混合编程教程。当前支持 Windows x64。

## 快速开始

### 安装

1. 从 [Releases](https://github.com/sgua83/Blink/releases/latest) 下载 Windows x64 安装包 `Blink-Setup-windows-x64.exe`，双击安装。
2. 安装器会把 Blink 的 `bin` 目录加入系统 PATH。安装完成后重新打开 PowerShell，查看工具版本：

```powershell
bc --version
blink --version
```

本页命令直接使用安装后的工具名。已有终端可能仍保留安装前的环境，关闭后重新打开即可。

后面的并行程序和计算 DLL 还需要 [Rust stable MSVC 工具链](https://rust-lang.org/tools/install)及 [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（x64 C++ 工具与 Windows SDK）。安装后使用 **x64 Developer PowerShell** 执行命令。这些是当前运行框架的构建依赖，不要求你编写 Rust；运行编译好的程序不需要这些构建工具。

### 写一个程序

在空目录中保存下面两份文件。`hello.h` 声明入口，`hello.b` 实现程序；这个程序会输出 `3`。

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

在同一目录编译、链接并运行：

```powershell
bc -c hello.b -o hello.obj
blink hello.obj -o hello.exe
.\hello.exe
```

`bc` 把源码编译成目标文件，`blink` 把目标文件链接成可执行程序。这个例子不需要克隆仓库。

## 运行并行计算

### 同一个计算函数，先在 CPU 运行

保存下面两份文件。程序计算四个数，逐行输出 `3、5、7、9`。

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

```powershell
bc -c parallel.b -o parallel.obj
blink parallel.obj -o parallel.exe
.\parallel.exe
```

`parallel` 描述每个元素的计算，`on compute` 表示函数可以面向 CPU／GPU 编译。这里由普通调用在 CPU 执行。

### 同一段循环也能用于 GPU

计算公式保持不变，把 `fill_numbers(values);` 换成下面三行：

```c
upload(values);
target(gpu, fill_numbers(values));
download(values);
```

先上传数据，再执行 GPU 计算，最后把结果下载回 CPU 输出。

<details>
<summary>展开完整 GPU 程序</summary>

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

</details>

```powershell
bc -c gpu.b -o gpu.obj
blink gpu.obj -o gpu.exe
.\gpu.exe
```

输出仍为相同的四个数。GPU 运行需要匹配程序能力的 Vulkan 驱动，无需 Vulkan SDK；本例的 f32 要求见[GPU 浮点能力](BLanguage.md#gpu-f32-运算的保留模式)，设备不支持时会明确报错。CPU 示例不需要 GPU。

<a id="first-dll"></a>

## 从你的语言调用 B

按你的工程语言选择一篇独立教程。每篇都有完整的 B 模块、调用代码、构建命令和预期输出，不需要克隆仓库。

| 你的工程语言 | 入门教程 |
|---|---|
| C | [C 调用 B](CInterop.md) |
| C++ | [C++ 调用 B](CppInterop.md) |
| Rust | [Rust 调用 B](RustInterop.md) |

反向调用见 [B 调用外部 DLL](../../sample/extern/README.md)。支持类型和部署要求见[原生集成文档](NativeInterop.md)。

## 工具与原生集成

| 工具 | 用途 |
|---|---|
| `bc` | 把 B 源码编译成目标文件 |
| `blink` | 把目标文件和库链接成 EXE 或计算 DLL |
| `brun` | 直接运行计算入口，用于后端诊断 |

普通应用直接运行生成的 EXE。原生 SDK 与资源生命周期见[原生集成文档](NativeInterop.md)。

### 面向 AI 的工具与系统研发

已有观察命令及其限制统一见[语言规范](BLanguage.md)。需要时可直接查阅 [IR 查询](BLanguage.md#实际ir的按需结构查询)、[类型布局](BLanguage.md#类型与字段布局的结构查询)、[构建输入](BLanguage.md#实际构建输入查询)和[栈报告](BLanguage.md#cpu调用链的静态栈报告)。

### 从 main 构建研发工具

这条可选路径用于工具链研发。先安装一份支持当前 B 引导源码的独立稳定 Blink 种子，并准备 Rust MSVC 与 Windows 构建工具，在 x64 Developer PowerShell 中执行：

```powershell
git clone https://github.com/sgua83/Blink.git Blink-main
Set-Location Blink-main
cargo build --workspace --release
```

研发工具使用 `target/release/bc.exe` 与 `target/release/blink.exe` 显式调用。源码构建默认使用已安装种子；`BLINK_BOOTSTRAP_SDK` 可选择独立上一代 SDK，构建完成前保持该种子可用。`main` 可能领先已安装正式版，使用时核对 [VERSION](../../VERSION) 与配套源码。

## 能力与发展方向

独立 CPU 应用及 CPU／GPU 计算已有实现；其他平台、完整模块能力、计算域和稳定热更新仍有未完成范围。当前支持与限制统一见[语言规范](BLanguage.md)。

## 学习与文档

- [并行编程入门](ParallelPrimer.md)：计算模型与并行语法。
- [更多独立应用](../../sample/applications/README.md)：动态数据与 CPU／GPU 程序。
- [语言规范](BLanguage.md)与[标准库](../../library/README.md)：现行语法、接口与限制。
- [Wiki 与书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books)：按目标选书与四本书正文。
- [正式发行](https://github.com/sgua83/Blink/releases)：安装包与同次发行的书籍 PDF。

在线文档随 `main` 更新；使用正式版时，选择与工具版本匹配的源码和文档。
