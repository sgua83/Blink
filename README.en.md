<div align="center">

<img src="assets/blink-banner.svg" alt="Blink: shared compute source, native CPU and GPU targets" width="1040" />

# Blink Programming Language

**Put parallel computation in your program. Bring B into your existing project.**

A native parallel language for C/C++/Rust programmers and AI, also called **B**.

[Download](https://github.com/sgua83/Blink/releases/latest) · [Run your first program](#quick-start) · [Integrate B](#tools-and-native-integration) · [Docs and four books](#learn-and-explore) · [Discussions](https://github.com/sgua83/Blink/discussions)

[简体中文](README.md) | [English](README.en.md)

</div>

**Available today: Windows x64 CPUs and Vulkan GPUs.** Standalone B applications, native parallel computation, and C/C++/Rust DLL integration are available. Formal releases and development [main](VERSION) are tracked separately. Multi-machine domain compilation, domain execution, and stable hot updates remain in design.

## Start with a complete program

The first program prints `3`. These two files are the complete source: `.h` declares the entry point and `.b` contains its implementation. `module hello` identifies this module.

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

### The same compute function on the CPU

Both versions below define the complete, identical `fill_numbers` function in their own file. It computes the same four values and prints `3, 5, 7, 9`. The CPU main calls it normally; `on compute` makes the function available to either backend, without choosing a device automatically.

<details>
<summary>Complete CPU parallel program</summary>

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

### The same loop can run on the GPU

The GPU version keeps exactly the same function, four-element allocation, formula and output. Only the execution statements differ:

| CPU | GPU |
|---|---|
| `fill_numbers(values);` | `upload(values);`<br>`target(gpu, fill_numbers(values));`<br>`download(values);` |

<details>
<summary>Complete GPU example, with every computation and transfer visible</summary>

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

`upload` establishes the GPU data location, `target(gpu, ...)` executes the same `fill_numbers` function, and `download` returns the result to the CPU before printing. The CPU version already shows the ordinary call, with no CPU target wrapper. Automatic device selection and inline GPU blocks are not implemented.

</details>

The [step-by-step guide](Open/Docs/GettingStarted.en.md#quick-start) explains the files, output, and GPU requirements. These examples use built-in operations and show their full logic.

<a id="quick-start"></a>

## Download and run

1. Download `Blink-Setup-windows-x64.exe` from the [Windows x64 release v0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1) and double-click it. Open a new terminal to use `bc` and `blink`.
2. Install [Rust stable MSVC](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open x64 Developer PowerShell. These tools are used for building; the resulting application needs neither Rust nor sources.
3. Save the two complete files above as `hello.h` and `hello.b` in an empty folder, then compile, link, and run with the installed formal tools:

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c hello.b -o target/hello.obj
& "$env:ProgramFiles/Blink/bin/blink.exe" target/hello.obj -o target/hello.exe
.\target\hello.exe
```

It prints `3`. These standalone examples are checked with formal tools **0.14.1.1**, without a repository checkout or custom imports. For the parallel examples, save their two files and compile `parallel.b` or `gpu.b` in the same way. CPU needs no GPU. The [complete guide](Open/Docs/GettingStarted.en.md#quick-start) covers GPU features and deployment of generated companion DLLs.

<a id="tools-and-native-integration"></a>

## Integrate with C/C++/Rust

Give B a computation while retaining the existing application and project structure. `bc` compiles the compute module; `blink` generates the DLL, import library, and calling interfaces.

| Your project | Integration | Start here |
|---|---|---|
| C | Generated C ABI header and DLL | [A complete four-number call](Open/Docs/GettingStarted.en.md#first-dll) |
| C++ | Generated wrapper, RAII, and source-level `Backend` selection | [C++ host source](https://github.com/sgua83/Blink/blob/cde16596f1532894a77490799783cdcf471ef172/sample/interop/map.cpp) |
| Rust | Generated bindings or the native Rust API | [DLL host source](https://github.com/sgua83/Blink/blob/cde16596f1532894a77490799783cdcf471ef172/sample/interop/map.rs) · [Native API](sample/interop/README.md#rust-原生-api) |

Ordinary hosts select CPU/GPU in code. See the [formal-release ABI reference](https://github.com/sgua83/Blink/blob/v0.14.1.1/Docs/NativeInterop.md) for supported types, synchronous completion, resource lifetimes, and deployment; the [current integration reference](Open/Docs/NativeInterop.md) tracks newer main capabilities. B can also [call external DLLs through the C ABI](sample/extern/README.md).

<a id="three-core-features"></a>
<a id="capabilities-and-direction"></a>

## Three designs, with explicit implementation status

| Design | Value | Current status |
|---|---|---|
| **Native parallelism** | Express independent work and required cooperation through data domains, access contracts, and collectives | Multicore CPU and GPU execution on a single machine: parallel domains, reductions, scans, explicit atomics, and cooperation within blocks |
| **Shared CPU/GPU source** | Share applicable algorithms, selecting targets and transfers explicitly | In-house native x64/SPIR-V backends; required GPU features depend on the program and driver |
| **Native domain compilation and execution** | Organize machines into build and execution resources; select build and execution domains independently | **In design, not implemented**; 内部研发资料（不公开） |

<a id="ai-facing-tools-and-systems-development"></a>

B organizes programs around data, functions, modules, and execution domains, without object-oriented programming. B also stands for **Base**: its long-term aim is to become a foundational language for the AI supercomputing era. AI-facing compilation and inspection interfaces continue to develop; see the [tool guide](Open/Docs/GettingStarted.en.md#ai-facing-tools-and-systems-development) for available tools and capabilities.

The current platform is Windows x64. Other platforms, complete module capabilities, and stable hot updates have unfinished scope. GPU execution needs a Vulkan driver supporting the program, without a Vulkan SDK. The [language reference](Open/Docs/BLanguage.md) specifies exact support; [capabilities and direction](Open/Docs/GettingStarted.en.md#capabilities-and-direction) gives more detail.

<a id="parallel-programming"></a>
<a id="map"></a>
<a id="ownership"></a>
<a id="reduction"></a>
<a id="begin-with-the-art-of-parallel-programming"></a>

## Learn and explore

Use the **[Wiki learning hub](https://github.com/sgua83/Blink/wiki)** to choose a book. The **[English book index](https://github.com/sgua83/Blink/wiki/Blink-Books-English)** links all complete manuscripts and PDF editions.

| Goal | Read online |
|---|---|
| From C/C++ to parallel thinking: map, access ownership, reduction, scan, and cooperation | [The Art of Parallel Programming](https://github.com/sgua83/Blink/wiki/Book-Parallel-Programming) |
| Bring B into Rust: layouts, calls, and repeated computation | [Blink and Rust Mixed Programming](https://github.com/sgua83/Blink/wiki/Book-Blink-and-Rust) |
| Understand the compiler, IR, CPU/GPU backends, and execution framework | [B Language Source Analysis](https://github.com/sgua83/Blink/wiki/Book-B-Source-Analysis) |
| Study scenes, lighting, and resource handover through an engine case | [BLEngine Source Analysis](https://github.com/sgua83/Blink/wiki/Book-BLEngine-Source-Analysis) |

The [English first three chapters](Open/Docs/ParallelPrimer.en.md) retain the complete introduction, diagrams, and performance methodology. The [installation and tool guide](Open/Docs/GettingStarted.en.md) covers full setup, source builds, integration, and AI queries. Also explore [standalone applications](sample/applications/README.md), the [B standard library](library/README.md), and the [language reference](Open/Docs/BLanguage.md).

Complete books are currently primarily in Chinese. The Wiki follows main; existing PDFs are snapshots, and new PDFs are generated from fixed manuscripts during the Sunday formal release. **BLEngine is not yet open source**; its case-study book and reading notes are available here.

<a id="build-development-tools-from-main"></a>

### Want to study or build the compiler?

Install a stable Blink seed first, then follow the [main build guide](Open/Docs/GettingStarted.en.md#build-development-tools-from-main). The default build consumes stable inputs in `%ProgramFiles%/Blink`. Published versions are available in [Releases](https://github.com/sgua83/Blink/releases).

<a id="feedback"></a>
<a id="feedback-and-technical-discussion"></a>
<a id="contributing"></a>

## Community and contribution

- **Usage, experience, and technical discussion:** [GitHub Discussions](https://github.com/sgua83/Blink/discussions).
- **Reproducible defects and feature requests:** [GitHub Issues](https://github.com/sgua83/Blink/issues) · [Open an issue](https://github.com/sgua83/Blink/issues/new). Include minimal source, tool versions, source commit, exact commands, CPU/GPU/driver details, and original errors. For performance, include input size, build profile, timing scope, and transfer costs.
- **Code and documentation contributions:** coordinate in an Issue or Discussion, then read the 内部研发资料（不公开） and 内部研发资料（不公开）. P4 is the development authority; GitHub is a one-way mirror and maintainers integrate changes through the mainline.

## License

[MIT](LICENSE).

## Plugins and source availability

The base language, compiler and single-machine runtime sources are in crates and library. Public specifications and guides are in Open. Distributed compilation, distributed execution and their administration and supervision foundation are developed as separate closed-source packages. Their public distribution layout is documented in [Plugins](Plugins/README.md). Domain binaries are not available yet.
