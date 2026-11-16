# Install Blink and run your first program

[Product home](../../README.en.md) · [中文指南](GettingStarted.md) · [Wiki](https://github.com/sgua83/Blink/wiki)

Start with a standalone B application, then try parallel computing or choose a tutorial for your existing language. The current platform is Windows x64.

## Quick start

### Install

1. Download `Blink-Setup-windows-x64.exe` from the [Windows x64 release](https://github.com/sgua83/Blink/releases/latest) and run the installer.
2. The installer adds Blink's `bin` directory to the system PATH. Open a new PowerShell window and check the tools:

```powershell
bc --version
blink --version
```

Commands below use the installed tool names directly. An existing terminal may retain its old environment; close it and open a new one.

The parallel programs and compute DLLs below also need [Rust stable MSVC](https://rust-lang.org/tools/install) and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the x64 C++ tools and Windows SDK. Run those commands in **x64 Developer PowerShell**. These are build dependencies of the current runtime framework; you do not need to write Rust. Running a built application does not require these build tools.

### Write a program

Create these two files in an empty folder. `hello.h` declares the entry point; `hello.b` implements it. The program prints `3`.

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

Compile, link, and run in that folder:

```powershell
bc -c hello.b -o hello.obj
blink hello.obj -o hello.exe
.\hello.exe
```

`bc` compiles the source; `blink` links the object into an executable. You can write this example without cloning the repository.

## Run a parallel computation

### The same compute function on the CPU

Save these two files in the same folder. The program computes four numbers and prints `3`, `5`, `7`, `9`, one per line.

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

`parallel` describes the per-element computation. `on compute` makes the function available to CPU/GPU compilation; the ordinary call here executes on the CPU.

### The same loop can run on the GPU

The formula stays the same. Replace `fill_numbers(values);` with these three lines:

```c
upload(values);
target(gpu, fill_numbers(values));
download(values);
```

Upload the data, execute on the GPU, then download the result before CPU output.

<details>
<summary>Complete GPU program</summary>

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

It prints the same four values. A GPU run needs a compatible Vulkan driver, but no Vulkan SDK. This f32 example requires the floating-point capabilities listed in the [GPU reference](BLanguage.md#gpu-f32-运算的保留模式); unsupported devices report an error. CPU examples do not need a GPU.

<a id="first-dll"></a>

## Call B from your language

Choose one independent tutorial. Each includes the B module, host source, build commands, and expected output; no repository checkout is needed.

| Your language | Tutorial |
|---|---|
| C | [Call B from C](CInterop.en.md) |
| C++ | [Call B from C++](CppInterop.en.md) |
| Rust | [Call B from Rust](RustInterop.en.md) |

For the reverse direction, see [B calling external DLLs](../../sample/extern/README.md). See the [integration reference](NativeInterop.md) for supported types and deployment.

## Tools and native integration

| Tool | Purpose |
|---|---|
| `bc` | Compile B source into object files |
| `blink` | Link objects and libraries into EXEs or compute DLLs |
| `brun` | Run a compute entry directly for backend diagnostics |

Applications run their generated EXEs directly. Native SDKs and resource lifetimes are described in the [integration reference](NativeInterop.md).

### AI-facing tools and systems development

The [language reference](BLanguage.md) documents the existing inspection commands and their limits. Jump to [IR](BLanguage.md#实际ir的按需结构查询), [type layout](BLanguage.md#类型与字段布局的结构查询), [build inputs](BLanguage.md#实际构建输入查询), or [stack reports](BLanguage.md#cpu调用链的静态栈报告) when needed.

### Build development tools from main

This optional path is for toolchain development. Install an independent stable Blink seed supporting the current bootstrap sources, then prepare Rust MSVC and the Windows build tools. In x64 Developer PowerShell:

```powershell
git clone https://github.com/sgua83/Blink.git Blink-main
Set-Location Blink-main
cargo build --workspace --release
```

Use `target/release/bc.exe` and `target/release/blink.exe` explicitly when working on development binaries. Source builds use the installed seed by default; `BLINK_BOOTSTRAP_SDK` selects an independent previous SDK. Keep that seed available until compilation completes. `main` may be ahead of the installed release; check [VERSION](../../VERSION) and use sources matching your tools.

## Capabilities and direction

Standalone CPU applications and CPU/GPU computation have implementations. Other platforms, complete module capabilities, compute domains, and stable hot updates still have unfinished scope. The [language reference](BLanguage.md) records current support and limits.

## Learning and documentation

- [Parallel programming introduction](ParallelPrimer.en.md): the computation model and parallel syntax.
- [More standalone applications](../../sample/applications/README.md): dynamic data and CPU/GPU programs.
- [Language reference](BLanguage.md) and [standard library](../../library/README.md): current syntax, APIs, and limits.
- [Wiki and books](https://github.com/sgua83/Blink/wiki/Blink-Books): learning paths and four books.
- [Releases](https://github.com/sgua83/Blink/releases): installers and book PDFs matching each release.

The online documentation follows `main`. Use the matching sources and documentation when working with that release.
