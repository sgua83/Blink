# Installation, applications, and tools

[Product home](../README.en.md) · [中文指南](GettingStarted.md) · [Wiki learning hub](https://github.com/sgua83/Blink/wiki)

Complete installation, CPU/GPU applications, native integration, and development-tool references. Formal releases and main are tracked separately; use the matching sources for your version.

## Quick start

Formal 0.14.1.1 does not accept a string literal in `println`; the current main builtin also accepts only scalar numbers and bool. The first example deliberately prints a number, rather than presenting an unverified text Hello World. `main` must be `on cpu`; compute capability is not a GPU application entry.

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

The [step-by-step guide](GettingStarted.en.md#quick-start) explains the files, output, and GPU requirements. These examples use built-in operations and show their full logic.

## Download and run

1. Download `Blink-Setup-windows-x64.exe` from the [Windows x64 release v0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1) and double-click it. Open a new terminal to use `bc` and `blink`.
2. Install [Rust stable MSVC](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open x64 Developer PowerShell. These tools are used for building; the resulting application needs neither Rust nor sources.
3. Save the two complete files above as `hello.h` and `hello.b` in an empty folder, then compile, link, and run with the installed formal tools:

```powershell
& "$env:ProgramFiles/Blink/bin/bc.exe" -c hello.b -o target/hello.obj
& "$env:ProgramFiles/Blink/bin/blink.exe" target/hello.obj -o target/hello.exe
.\target\hello.exe
```

It prints `3`. These standalone examples are checked with formal tools **0.14.1.1**, without a repository checkout or custom imports. For the parallel examples, save their two files and compile `parallel.b` or `gpu.b` in the same way. CPU needs no GPU. The [complete guide](GettingStarted.en.md#quick-start) covers GPU features and deployment of generated companion DLLs.


### 4. Call B from an existing C/C++/Rust project

The [DLL integration example](../sample/interop/README.md) and [first Rust integration](https://github.com/sgua83/Blink/blob/v0.14.1.1/sample/hybrid/first/README.md) select `Backend::Cpu` / `Backend::Gpu` in host code. The tools generate C ABI headers, import libraries, C++ wrappers, and Rust bindings. Ordinary hosts need no device argument; dedicated comparison drivers retain it for measurement. See the [release ABI reference](https://github.com/sgua83/Blink/blob/v0.14.1.1/Docs/NativeInterop.md) for supported types and responsibilities.

<a id="first-dll"></a>

### Build your first DLL with the formal tools

This optional integration lesson uses Git to obtain a fixed source snapshot. Run the commands from that checkout's root in x64 Developer PowerShell. The installed tools can generate the DLL directly; each C, C++, and Rust host selects CPU in its own source. Build one algorithm library and run the three hosts:

The library entry below performs the complete computation. Its input is the host array `[1, 2, 3, 4]`; `scale` is the actual exported algorithm, not a demonstration wrapper.

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

C/C++ print `3 5 7 9`; Rust prints `[3.0, 5.0, 7.0, 9.0]`. Deploy the host EXE with `map.dll` and any companion DLLs produced by the linker. Resource and type support follow the ABI reference.

### Build development tools from main

`main` is under active development. Its [VERSION](../VERSION) can be newer than the formal release. The source SDK instructions below and relative reference links follow main. The introduction above uses a fixed example snapshot with the stated formal tools; books in older tags are historical release snapshots.

Before building the current compiler, install a stable Blink seed supporting its current B frontend sources. The default `compiler` feature retains the complete compiler APIs. The original `build.rs` invokes installed `bc` and `blink` from `%ProgramFiles%/Blink/bin` to statically build the B lexer and parser-helper modules (generic lookahead and C ABI/PE link-string decoding); newly built target tools are not their own bootstrap prerequisite. Runtime-only consumers disable `compiler`, avoiding the frontend seed and source frontend.

Use Windows x64, the [Rust stable MSVC toolchain](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open an x64 Developer PowerShell:

```powershell
git clone https://github.com/sgua83/Blink.git Blink-main
Set-Location Blink-main
cargo build --workspace --release
```

Run all subsequent commands from the Blink root. CPU examples need no GPU. GPU examples need a Vulkan driver supporting each program's requirements, without a separate Vulkan SDK. The f32 example currently requires `DenormPreserve` and `SignedZeroInfNanPreserve`; unsupported devices receive an explicit error.

Formal release builds and publication are scheduled to **start every Sunday at 21:00 Beijing time (UTC+8)**. Completion depends on building and uploading; [Releases](https://github.com/sgua83/Blink/releases) records what is actually published. New installations replace the current version; development roots and target directories remain unchanged. The same installer supports administrator `--quiet` deployment. See [tooling](../Tools/README.md) and [versioning](Versioning.md) for installation and seed input/output boundaries. `bc --version`, `blink --version`, and `brun --version` report each tool's build-time version.

## Tools and native integration

| Tool / library | Purpose |
|---|---|
| `bc` | Check sources, compile COFF objects, and export native CPU/GPU compute code |
| `blink` | Link compiled objects into Windows x64 executables or compute libraries (DLLs) |
| `brun` | Module runner for backend isolation tests and diagnostics; ordinary applications run their EXEs directly |
| `blanguage` | Frontend, type/access checking, parallel IR, optimization, and native backends |
| `blcompute` | Data, device resources, transfers, native execution, and release after completion |

Write standalone applications in the B language, or compile compute functions written in the B language into DLLs callable from **C, C++, and Rust**. The toolchain also generates C ABI headers, import libraries, C++ wrappers, and Rust bindings. The native Rust API supports resident resources and asynchronous submissions.

- [Standalone applications](../sample/applications/README.md): CPU entry points, dynamic data, and explicit CPU/GPU computation.
- [Call B-language programs from C/C++/Rust](../sample/interop/README.md): build and call the same algorithm DLL, starting with four numbers.
- [Call external DLLs from the B language](../sample/extern/README.md): C ABI, parameter access, and resource boundaries.
- [Native integration and deployment](NativeInterop.md): artifacts, ABI, deployment, and current limitations.

### AI-facing tools and systems development

Compilation, debugging, inspection, and basic measurement tools serve AI as their sole direct user; architects use AI to inspect actual code, data, and execution. The long-term design lets AI write observation and analysis programs, compute near the data, express concrete machine implementations, and combine experiments on real inputs. Optimization, debug information, and runtime collection are independent choices; analysis and implementation selection remain with AI or the user. See the [AI-facing tool principles and individual responsibilities](Architecture.md#面向ai的基础工具与全流程观察) for current foundations and planned capabilities. Complete debugging and experiment interfaces are not yet delivered; the design adds no automatic bottleneck judgments, scoring, or second authority over runtime state.

B develops self-hosting compiler and debugging/inspection responsibilities alongside the interfaces and libraries required for complete systems programs. System ABI declarations and meaningful B library wrappers are valid capabilities; independent native support may remain in Rust. Completion is based on actual ownership and rebuilding, rather than a source percentage or a B shell around the original implementation. See the [architecture](Architecture.md#长期系统设计目标与语言边界) for boundaries and current status.

AI can inspect an artifact's stored build request and source identities with `bc inputs <compiled.obj|entry.blbi>`. This JSON query neither reads current B sources nor recompiles the artifact. See [build input inspection](BLanguage.md#实际构建输入查询) for fields and identity scope; this command does not imply that complete debugging tools are available.

Use `bc layout <module.h|module.b> <type> [-I module-root] [--field path]` for a JSON query of a concrete source type or static field. `bc build` also exports `.layout.json`; the SDK can inspect an existing execution interface without source compilation. See [type and field layout inspection](BLanguage.md#类型与字段布局的结构查询) for descriptor, element-storage, and resource-word boundaries.

Use `bc ir <module.b>` to query the actual module, selected parallel entry, or application IR as JSON, with function or optimized-stage selection. The SDK can inspect the same compilation result without another compilation. Function/program IR exposes source_origin only for actual source definitions; generated functions and compiled interfaces do not fabricate a definition, while known instances and later real inline origins remain available; existing IR retains that snapshot after files change. Each IR node can expose its emission-time syntax range through source_site, with null for missing material. The same facts also retain the actual specialized instance and inline expansion sites in outermost-first order. SourceSite::domain_path() and domain_path independently retain source-domain expansion and compiled-domain boundaries, with original callers, actual targets and source ranges; these are separate from ordinary inlining and runtime stacks. Inst/Stage/ValueCode generated_origin exposes actual generation reasons with distinct syntax and definition-instance anchors. Application entry, root Frame lifecycle, original dataset allocation calls, factory initialization, new local dataset bindings, dataset reassignment stores, final dataset storage release calls, reverse cleanup of owned dataset elements, static destructor calls for user-owned values, conditional-ownership cleanup branches and condition preparation, cleanup selection for active variants, resize fill temporaries/calls, Frame address-request constants/calls, storage-address query calls, root-address preparation, address offset preparation, CPU write-area preparation, access-layout/read-area preparation, field-layout preparation, the projection zero index for dataset-to-view construction, actual upload/download transfer calls, and application calls into compiled domains, compiled-domain wrapper body entries, literal argument preparation for compiled-domain calls, actual domain-part run calls, Stage Dataset call-argument preparation, CPU borrow-descriptor stride casts, and array-parameter materialization stores expose their generation origins; existing parallel-index and empty-prefix syntax triggers are preserved. Other resource wrappers remain pending. Parallel statements and collective calls without ordinary instructions expose their original statement/call-token range and concrete instance through Stage::operation_source() and stage JSON operation_source. CPU application extraction retains the original statement definition and specialized caller; generated execution names do not become source definitions. Complete provenance across stage fusion remains pending; each project/identity/combine ValueCode exposes its actual target, static reference and ordered capture parameter origins through entry_origin(), including empty value bodies. Optimization provenance and machine-code mappings remain separate. See [operation syntax](BLanguage.md#ir操作的语法来源). See [definition origins](BLanguage.md#函数与程序ir的原定义来源). See [IR inspection](BLanguage.md#实际ir的按需结构查询) for phases, logical references, and artifacts that no longer retain IR.

Use `bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]` or `native::instructions_json` for structured decoding of an explicitly selected x64 instruction range. Compiler-produced NativeCode, StageCode and ProgramImage values expose `code_json` with the actual main/helper code, alignment and literals; `bc build` also exports `.x64.json`. The SDK can explicitly save separate CPU range material. Query a saved BLBI with `bc code <entry.blbi> --observations <ranges.blcr>` or a BLRI with `bc code --image <deployment.blri> --observations <ranges.blcr>` without source files. Execution bytes exclude the material; Use `bc code --object <compiled.obj> [--section number | --function exact-symbol]` for original physical COFF records and instructions in resolved `.pdata` ranges. The original Object can explicitly save separate `.blcf` material; `--observations <ranges.blcf>` pairs it with actual code and relocations to expose complete generated code, helper, alignment and literal regions. Without material, other bytes remain unclassified. Final PE inspection remains separate. See [native image queries](BLanguage.md#生成期原生映像的范围与指令查询). Add --source-range start:end to query original source records for a section-relative half-open byte range; object::source_facts provides the SDK entry. It retains every intersecting record and its context without instruction decoding; null means unavailable, an empty captured sequence means no intersection, and no hit does not establish elimination. Source-to-code and CPU-image range queries remain separate unfinished capabilities.

Use `bc spirv <module.spv>` or the compiler SDK’s `spirv::module_json` to inspect every original instruction, offset, word, type/result/operand ID, entry point and literal bit pattern in one module, without recompilation or GPU execution. ShaderCode, StageCode and ProgramImage expose code_json for every original main/helper module; bc build writes a paired .spv.json for each .spv. Pure GPU artifacts can be queried without sources or CPU range material using `bc code <entry.blbi>` or `bc code --image <deployment.blri>`. SPIR-V remains an intermediate representation. See [single-module queries](BLanguage.md#原spir-v单模块的完整结构查询).

`bc image <entry.blbi> -o <deployment.blri>` exports the original CPU/GPU execution image while retaining the build cache and input record. The Rust runtime SDK can load it with compiler features disabled. See [deployment images](BLanguage.md#构建记录与可部署执行映像).

Optional CPU stack reports are available through `bc -c ... --stack-report <file>` and `blink --stack-report <compiled-inputs>... -o <file>`. They expose actual frames, closed call-chain bounds, and unknown recursive, external, or indirect ranges. A caller-supplied remaining stack budget is used only for reporting; PE reserve is not assumed to be a thread’s remaining capacity. See the [stack report specification](BLanguage.md#cpu调用链的静态栈报告).

## Capabilities and direction

| Area | Current status |
|---|---|
| CPU/GPU computation | Native x64/SPIR-V backends, parallel domains, collectives, explicit atomics, and tile/subgroup cooperation |
| Data and static abstraction | Records, arrays, vectors/matrices, generics, open `u32 const` parameters, access contracts, and valid prefixes |
| Applications and modules | Standalone CPU applications, native function calls/recursion, and a limited implementation of independent modules and multi-object linking |
| Platforms | Windows x64 CPU and Vulkan GPU today; other platforms and CPU architectures remain future work |
| Compute domains | Domain compilation and execution have a defined architectural scope; not implemented |
| Stable hot updates | Version binding, state migration, and resource handover are architectural goals; not implemented |

CPU applications can construct datasets of owning elements with `dataset<T>(length,F)`, using a static factory for each initialized element. Whole-dataset moves, returns and final element cleanup retain the original resource ownership. CPU functions can declare `requires nonzero(rows[*].value)` and `requires nonempty(rows[*].value.values)` for common integer and child-resource facts about initialized elements. The `requires index(rows[*].value.index,rows[*].value.values)` contract also carries each element’s index relation to its actual child resource; see the [value](BLanguage.md#common-element-nonzero), [resource](BLanguage.md#common-element-nonempty) and [index](BLanguage.md#common-element-index) contracts. See the [implemented scope and remaining element borrowing and container operations](BLanguage.md#cpu-owned-dataset-elements).

CPU applications can also move and refill dynamically indexed positions in fixed owning arrays, releasing the resources still owned along actual branches, loops, and returns. Conditional CPU/GPU output locations for returned values and write/inout parameters can cross ordinary functions, compiled objects, and archives; conditional inputs follow the actual use path, including loop early exits. See the [compiled-module resource ABI](BLanguage.md#已编译模块的资源abi) and [conditional resource outputs](BLanguage.md#跨函数模块的条件资源输出位置).

The B language **does not adopt object-oriented programming**. Programs are organized around data, functions, modules, and execution domains. The design values top-tier efficiency, direct control over machine execution, checkable interfaces, and use by people, AI code generators, and higher-level compilers. Current rules, native runs, and performance results are recorded separately in the [language reference](BLanguage.md) and [current architecture](Architecture.md).

## Learn and explore

**[Open the Wiki book index](https://github.com/sgua83/Blink/wiki/Blink-Books-English)** · [Choose a book and get started](https://github.com/sgua83/Blink/wiki) · [中文书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books). The Wiki provides all four complete manuscripts. It follows development main; use documentation and examples matching your formal release.

| Start here | What it covers |
|---|---|
| [The Art of Parallel Programming](../sample/并行编程的艺术.md) | Moving from C/C++ to parallel thinking; [mobile PDF](../sample/pdf/并行编程的艺术-B语言-手机阅读版.pdf) · [standard PDF](../sample/pdf/并行编程的艺术-B语言.pdf) |
| [Blink and Rust](../sample/Blink和Rust混合编程.md) | Minimal integration through simulation and repeated GPU execution; [examples](../sample/hybrid/README.md); [mobile PDF](../sample/pdf/Blink和Rust混合编程-手机阅读版.pdf) · [standard PDF](../sample/pdf/Blink和Rust混合编程.pdf) |
| [B Language Source Analysis](../sample/B语言源码分析.md) | Compiler, IR, backends, and execution framework; [version and reading notes](../sample/compiler/README.md); [mobile PDF](../sample/pdf/B语言源码分析-手机阅读版.pdf) · [standard PDF](../sample/pdf/B语言源码分析.pdf) |
| [BLEngine Source Analysis](../sample/BLEngine源码分析.md) | How the first consumer uses the language, scenes, and lighting; [reading notes](../sample/engine/README.md); [mobile PDF](../sample/pdf/BLEngine源码分析-手机阅读版.pdf) · [standard PDF](../sample/pdf/BLEngine源码分析.pdf) |
| [B standard library](../library/README.md) | Dynamic value sequences, strict UTF-8/UTF-16 codecs, whole-text conversions and owned slices, plus Windows system information, timing, thread error codes, raw command lines, environment access, UTF-8 file input, path existence, absolute and canonical path identities, extension replacement, and path composition |
| [Language reference](BLanguage.md) · [Architecture](Architecture.md) · [Compute domains](ComputeDomain.md) | Current rules, long-term boundaries, and future design |

Technical documentation and books are currently primarily in Chinese. Both README editions provide the same starting points and capability descriptions. The table follows main and its companion examples; basic applications have been verified with the formal tools stated above. PDFs linked in the table and older tags are existing snapshots; online Markdown contains the latest text. Standard and mobile PDFs are generated from the fixed manuscripts during the Sunday formal release; look for `Blink-Books.zip` in the corresponding [Release assets](https://github.com/sgua83/Blink/releases). BLEngine is an independent consumer and is not yet open source; this repository provides its case-study book and reading notes, rather than the engine source.
