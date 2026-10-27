<div align="center">

<h1>Blink</h1>

<p><strong>Native parallelism · Unified CPU/GPU programming · Native domain compilation and execution</strong></p>
<p>A foundational language for the AI supercomputing era</p>

<p><a href="README.md">简体中文</a> | <strong>English</strong></p>
<p><a href="#quick-start">Quick start</a> · <a href="Docs/BLanguage.md">Language reference</a> · <a href="#learn-and-explore">Learn</a> · <a href="Docs/Architecture.md">Architecture</a></p>

</div>

Blink, or **B**, is a native parallel programming language developed from the ground up. B also stands for **Base**: parallelism, data relationships, and cooperation belong in the language itself. The long-term goal is a complete general-purpose and systems language.

Interfaces live in `.h` files; implementations live in `.b` files. The compiler generates Windows x64 machine code and SPIR-V directly. The native execution framework owns data, devices, transfers, and completion. The compiler and framework are currently implemented in Rust, without third-party compilers, shader compilers, or parallel runtimes.

> **In development — version 0.1.0.** Native execution on a single machine's CPU and GPU is available. Domain compilation, domain execution, and stable hot updates are established design goals and are not implemented yet. See the [architecture](Docs/Architecture.md) and [implementation status](Docs/Issues.md).

## Three core features

### 1. Native parallelism

Parallelism is a language capability. Parallel domains, access permissions, cooperation within a block, and stage dependencies enter compiler analysis and become native execution. Algorithms can express exclusive writes, shared reads, explicit atomics, and collective operations.

### 2. Unified CPU/GPU programming

**One set of B sources, native execution on CPUs and GPUs.** Applicable algorithms share an implementation across both targets. `on` declares target capabilities, `target` chooses an execution location, and `upload` / `download` make data transfers explicit. Hardware differences and transfer costs remain visible.

This excerpt comes from [map.b](sample/lessons/map.b); its interface is in [map.h](sample/lessons/map.h):

```c
void scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}
```

`inout` declares read/write access, `parallel` gives each invocation exclusive access to its current element, and `on compute` permits CPU/GPU execution.

### 3. Native domain compilation and execution

**Use a group of machines as compilation and execution resources.** A compute domain organizes machines for native compilation and execution. Build and execution domains can be selected independently, with explicit semantics for partitioning, communication, dependencies, and completion.

This capability is currently in design. Each build or execution uses resources within its selected domain; applications orchestrate workflows between domains. See the [compute-domain design](Docs/ComputeDomain.md) for responsibilities, deployment, and interfaces.

## Quick start

Use Windows x64, the [Rust stable MSVC toolchain](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open an x64 Developer PowerShell:

```powershell
git clone https://github.com/sgua83/Blink.git
Set-Location Blink
cargo build --workspace --release

.\target\release\blc.exe -c sample/applications/start.b -o target/applications/start.obj
.\target\release\bllink.exe target/applications/start.obj -o target/applications/start.exe
.\target\applications\start.exe
```

The program prints `3` and exits with code `0`. This standalone CPU application needs no GPU. The generated executable runs without B sources, the compiler, a runner, or a separate host application.

Next, run a parallel example:

```powershell
.\target\release\blrun.exe cpu sample/lessons/map.b demo sample Saved/map-cpu :4
```

The result is saved to `Saved/map-cpu/values.bldata` with four values: `3, 5, 7, 9`. Replace `cpu` with `gpu` and choose another output directory to run the same sources on a Vulkan device that supports the program's requirements. GPU execution needs a driver; it does not require the Vulkan SDK or an external shader compiler. This f32 example currently also requires `DenormPreserve` and `SignedZeroInfNanPreserve` support; unsupported devices receive an explicit error.

## Tools and native integration

| Tool / library | Purpose |
|---|---|
| `blc` | Check sources, compile COFF objects, and export native CPU/GPU compute code |
| `bllink` | Link compiled objects into Windows x64 executables or algorithm DLLs |
| `blrun` | Bind data and execute CPU/GPU compute modules |
| `blanguage` | Frontend, type/access checking, parallel IR, optimization, and native backends |
| `blcompute` | Data, device resources, transfers, native execution, and release after completion |

Build standalone applications in B or use B algorithms from **C, C++, and Rust**. Algorithm DLLs are accompanied by C ABI headers, import libraries, C++ wrappers, and Rust bindings. The native Rust API also supports resident resources and asynchronous submissions.

- [Standalone applications](sample/applications/README.md): CPU entry points, dynamic data, and explicit CPU/GPU computation.
- [Call B from C/C++/Rust](sample/interop/README.md): build and call the same algorithm DLL, starting with four numbers.
- [Call external DLLs from B](sample/extern/README.md): C ABI, parameter access, and resource boundaries.
- [Native integration and deployment](Docs/NativeInterop.md): artifacts, ABI, deployment, and current limitations.

## Capabilities and direction

| Area | Current status |
|---|---|
| CPU/GPU computation | Native x64/SPIR-V backends, parallel domains, collectives, explicit atomics, and tile/subgroup cooperation |
| Data and static abstraction | Records, arrays, vectors/matrices, generics, open `u32 const` parameters, access contracts, and valid prefixes |
| Applications and modules | Standalone CPU applications, native function calls/recursion, and a limited implementation of independent modules and multi-object linking |
| Platforms | Windows x64 CPU and Vulkan GPU today; other platforms and CPU architectures remain future work |
| Compute domains | Domain compilation and execution have a defined architectural scope; not implemented |
| Stable hot updates | Version binding, state migration, and resource handover are architectural goals; not implemented |

B **does not adopt object-oriented programming**. Programs are organized around data, functions, modules, and execution domains. The design values top-tier efficiency, direct control over machine execution, checkable interfaces, and use by people, AI code generators, and higher-level compilers. Current rules, native runs, and performance results are recorded separately in the [language reference](Docs/BLanguage.md) and [issue register](Docs/Issues.md).

## Learn and explore

| Start here | What it covers |
|---|---|
| [B Language Programming](sample/B语言编程.md) | Installation through functions, data, and parallelism; [examples](sample/beginner/README.md) · [mobile PDF](sample/pdf/B语言编程-手机阅读版.pdf) · [standard PDF](sample/pdf/B语言编程.pdf) |
| [The Art of Parallel Programming](sample/并行编程的艺术.md) | Moving from C/C++ to parallel thinking |
| [Blink and Rust](sample/Blink和Rust混合编程.md) | Minimal integration through simulation and repeated GPU execution; [examples](sample/hybrid/README.md) |
| [B Language Source Analysis](sample/B语言源码分析.md) | Compiler, IR, backends, and execution framework; [version and reading notes](sample/compiler/README.md) |
| [BLEngine Source Analysis](sample/BLEngine源码分析.md) | How the first consumer uses the language, scenes, and lighting; [reading notes](sample/engine/README.md) |
| [Language reference](Docs/BLanguage.md) · [Architecture](Docs/Architecture.md) · [Compute domains](Docs/ComputeDomain.md) | Current rules, long-term boundaries, and future design |

Technical documentation and books are currently primarily in Chinese. Both README editions provide the same starting points and capability descriptions.

## Contributing

Read the [project conventions](AGENTS.md) and [implementation status](Docs/Issues.md) before making changes. Include B sources, exact commands, toolchain/device details, and original errors when reporting a problem. Distinguish design goals, implementation, native execution, and measured performance. Maintainer submission and publishing procedures are documented in the project conventions.

## License

[MIT](LICENSE).
