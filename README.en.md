<div align="center">

<h1>Blink Programming Language</h1>

<p><strong>Native parallelism · Shared CPU/GPU source · C/C++/Rust integration</strong></p>
<p>A native parallel programming language for the AI supercomputing era</p>

<p><a href="README.md">简体中文</a> | <strong>English</strong></p>
<p><a href="https://github.com/sgua83/Blink/releases/latest">Download</a> · <a href="#quick-start">Quick start</a> · <a href="#tools-and-native-integration">C++/Rust integration</a> · <a href="#learn-and-explore">Books and docs</a> · <a href="#feedback">Feedback</a></p>

</div>

**Blink is a native parallel programming language, also known as the B language.** Use it to write standalone applications, multicore CPU programs, and GPU compute programs. The same compute source can be compiled to native CPU and GPU code.

For **C/C++/Rust programmers and AI** working with native parallelism: express suitable computation once in B, run it on a CPU or GPU, or compile it into a dynamic library for an existing project.

The B in the name also stands for **Base**. Blink aims to become a foundational programming language for the AI supercomputing era, with parallelism, data relationships, and cooperation built into the language, while developing complete general-purpose and systems programming capabilities.

This repository contains the B-language compiler, linker, native execution framework, language reference, and examples. B-language interfaces live in `.h` files and implementations in `.b` files. The compiler generates Windows x64 machine code or SPIR-V for the selected target. The execution framework owns data, devices, transfers, and completion. The current division of compiler and runtime responsibilities between B and Rust is recorded in the architecture; source proportions do not establish self-hosting. Blink uses no third-party compiler, shader compiler, or parallel runtime.

> **In development.** [Formal releases](https://github.com/sgua83/Blink/releases/latest) and the [main software version](VERSION) are tracked separately. Native execution on a single machine's CPU and GPU is available. Domain compilation, domain execution, and stable hot updates are established design goals and are not implemented yet. See the [architecture and current implementation scope](Docs/Architecture.md).

## Three core features

### 1. Native parallelism

Parallelism is a language capability. Parallel domains, access permissions, cooperation within a block, and stage dependencies enter compiler analysis and become native execution. Algorithms can express exclusive writes, shared reads, explicit atomics, and collective operations.

### 2. Unified CPU/GPU programming

**One set of B-language sources, native execution on CPUs and GPUs.** Applicable algorithms share an implementation across both targets. `on` declares target capabilities, `target` chooses an execution location, and `upload` / `download` make data transfers explicit. Hardware differences and transfer costs remain visible.

### 3. Native domain compilation and execution (in design)

**Use a group of machines as compilation and execution resources.** A compute domain organizes machines for native compilation and execution. Build and execution domains can be selected independently, with explicit semantics for partitioning, communication, dependencies, and completion.

This capability is currently in design. Each build or execution uses resources within its selected domain; applications orchestrate workflows between domains. See the [compute-domain design](Docs/ComputeDomain.md) for responsibilities, deployment, and interfaces.

## Quick start

### 1. Install the tools and obtain the examples

Download `Blink-Setup-windows-x64.exe` from the [Windows x64 release v0.14.1.1](https://github.com/sgua83/Blink/releases/tag/v0.14.1.1) and double-click it. The installer configures PATH; open a new terminal to use `bc` and `blink`. See [latest releases](https://github.com/sgua83/Blink/releases/latest) for updates.

Install Git, [Rust stable MSVC](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. These build tools generate execution-framework code at compile time. Open a new x64 Developer PowerShell:

```powershell
bc --version
blink --version
git clone https://github.com/sgua83/Blink.git Blink
Set-Location Blink
```

The complete applications below are part of this documentation update on main and have been run with formal tools `0.14.1.1`; they are separate from the older tutorial in that release tag. Both the algorithm and main are B code. Running the resulting application needs neither the compiler, Rust toolchain, nor sources.

### 2. Select the device in B code

The CPU application [map_cpu.b](sample/applications/map_cpu.b):

```c
module applications.map_cpu;
import lessons.map;

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    target(cpu, demo(values));

    for (value in values) { if (!println(value)) { return 1; } }
    return 0;
}
```

[map_cpu.h](sample/applications/map_cpu.h) exports `u32 main() on cpu;`. Compile, link independently, and run:

```powershell
bc -c sample/applications/map_cpu.b -o target/book-apps/map_cpu.obj -I sample
blink target/book-apps/map_cpu.obj -o target/book-apps/map_cpu.exe
.\target\book-apps\map_cpu.exe
```

It prints `3`, `5`, `7`, `9`, one per line. No GPU is needed.

### 3. Run the same algorithm on the GPU

[map_gpu.b](sample/applications/map_gpu.b) calls the same `lessons.map::demo`. Its main explicitly selects execution and transfers:

```c
upload(values);
target(gpu, demo(values));
download(values);
```

```powershell
bc -c sample/applications/map_gpu.b -o target/book-apps/map_gpu.obj -I sample
blink target/book-apps/map_gpu.obj -o target/book-apps/map_gpu.exe
.\target\book-apps\map_gpu.exe
```

Both EXEs run without a runtime device argument. Deploy the EXE together with the linker-generated `blink_app_*.dll` files. GPU execution needs a Vulkan driver supporting the example, without a separate Vulkan SDK; this f32 example requires `DenormPreserve` and `SignedZeroInfNanPreserve`.

`on compute` declares target capabilities; `target(cpu, ...)` / `target(gpu, ...)` choose this execution. `upload` / `download` explicitly transfer storage. There is currently no `target(auto, ...)`, implicit automatic device selection, or implicit migration.

### 4. Call B from an existing C/C++/Rust project

The [DLL integration example](sample/interop/README.md) and [first Rust integration](https://github.com/sgua83/Blink/blob/v0.14.1.1/sample/hybrid/first/README.md) select `Backend::Cpu` / `Backend::Gpu` in host code. The tools generate C ABI headers, import libraries, C++ wrappers, and Rust bindings. Ordinary hosts need no device argument; dedicated comparison drivers retain it for measurement. See the [release ABI reference](https://github.com/sgua83/Blink/blob/v0.14.1.1/Docs/NativeInterop.md) for supported types and responsibilities.

### Build development tools from main

`main` is under active development. Its [VERSION](VERSION) can be newer than the formal release. The source SDK instructions below, the three chapters later on this page, and relative documentation links follow main. The basic applications above have been verified with the stated formal tools; books in older tags are historical release snapshots and do not contain these new application entries.

Before building the current compiler, install a stable Blink seed supporting its current B frontend sources. The default `compiler` feature retains the complete compiler APIs. The original `build.rs` invokes installed `bc` and `blink` from `%ProgramFiles%/Blink/bin` to statically build the B lexer and generic-lookahead modules; newly built target tools are not their own bootstrap prerequisite. Runtime-only consumers disable `compiler`, avoiding the frontend seed and source frontend.

Use Windows x64, the [Rust stable MSVC toolchain](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open an x64 Developer PowerShell:

```powershell
git clone https://github.com/sgua83/Blink.git Blink-main
Set-Location Blink-main
cargo build --workspace --release
```

Run all subsequent commands from the Blink root. CPU examples need no GPU. GPU examples need a Vulkan driver supporting each program's requirements, without a separate Vulkan SDK. The f32 example currently requires `DenormPreserve` and `SignedZeroInfNanPreserve`; unsupported devices receive an explicit error.

Formal release builds and publication are scheduled to **start every Sunday at 21:00 Beijing time (UTC+8)**. Completion depends on building and uploading; [Releases](https://github.com/sgua83/Blink/releases) records what is actually published. New installations replace the current version; development roots and target directories remain unchanged. The same installer supports administrator `--quiet` deployment. See [tooling](Tools/README.md) and [versioning](Docs/Versioning.md) for installation and seed input/output boundaries. `bc --version`, `blink --version`, and `brun --version` report each tool's build-time version.

<a id="parallel-programming"></a>

## Begin with The Art of Parallel Programming

The first three chapters of [The Art of Parallel Programming](sample/并行编程的艺术.md) are reproduced in full below, including explanations, C and B-language comparisons, figures, commands, and the book's measurements. Links and outdated function/borrowing descriptions are aligned with the current implementation. Figures are retained from the Chinese edition, with translated captions here.

[Chapter 1: element-wise parallelism](#map) · [Chapter 2: ownership of reads and writes](#ownership) · [Chapter 3: sum reduction](#reduction)

The examples use the current language rules and formal Release tools. Historical build, hardware and measurement records are kept in the P4 ledgers; run the supplied programs to obtain measurements for the current build.

To reproduce them, build the additional measurement program as described in [Appendix A.5](sample/并行编程的艺术.md). A normal workspace build does not produce `bperf.exe`:

```powershell
cargo build --release --manifest-path sample/performance/Cargo.toml --target-dir target
```

<a id="map"></a>

## 1 · The first beauty: many things can happen together

### 1.1 · Start with a loop you already know

The array `values` contains four numbers, `[1,2,3,4]`. Multiplying each by two and adding one takes only a short C loop:

```cpp
for (size_t i = 0; i < n; ++i) {
    values[i] = values[i] * 2.0f + 1.0f;
}
```

The computation written in the B language is just as short:

```c
parallel (value in values) {
    value = value * 2.0 + 1.0;
}
```

`values` denotes all the numbers. `value` is an exclusive position for the current element, corresponding to C's `values[i]`; assigning to it updates that element directly. `parallel` describes what each element does and permits the invocations to advance independently.

Both forms produce `[3,5,7,9]`. Each update depends only on its own previous value, so the updates can be computed separately. The serial order comes from the loop's form; the problem itself does not require it.

### 1.2 · Look at dependencies before hardware

![Figure 1-1: serial order and a parallel element domain for the same formula. Steps represent logical dependencies, not measured time.](sample/figures/map-order.svg)

*Figure 1-1: serial order and a parallel element domain for the same formula. Steps represent logical dependencies, not measured time.*

In a serial execution, the processor reads one number, multiplies, adds, writes it back, and advances to the next item. The parallel form exposes four independent read-modify-write operations. Within each invocation, reading still precedes calculation, which precedes writing.

This pattern, with one input corresponding to one output, is called a map: an element-wise transformation.

![Figure 1-2: logical elements, CPU work ranges, and GPU workgroups are different levels.](sample/figures/map-hardware.svg)

*Figure 1-2: logical elements, CPU work ranges, and GPU workgroups are different levels.*

The current CPU framework assigns contiguous index ranges to worker threads. One thread processes many elements. Suitable straight-line computations can also use four SSE2 lanes at once; vector execution and work across multiple CPU cores are two levels of parallelism.

On a GPU, element instances map to compute invocations, which are grouped into workgroups. Workgroups are scheduled onto hardware in batches. Four logical elements are not guaranteed to execute at precisely the same instant, and one million elements do not mean one million operating-system threads.

C/C++ compilers may also vectorize simple loops automatically. B expresses independence between elements directly through the work domain.

### 1.3 · Concurrency and parallelism are different

Concurrency concerns how multiple activities remain in progress, such as handling another request while waiting for a file read. Parallelism concerns whether computations can advance simultaneously on multiple execution resources.

Concurrent tasks can take turns on one core. A parallel array computation uses multiple execution resources to divide the work. B primarily develops data parallelism and connects stages through producer-consumer relationships.

### 1.4 · Ask three questions first

When looking at a loop, ask: what does each iteration read? What does it write? Does it need a result just written by another iteration?

If the answers are shared read-only inputs, exclusive outputs, and no waiting for another element, you have found a shape well suited to parallelism. Finding that shape before discussing core or thread counts makes the problem easier to understand.

### 1.5 · Run it and inspect the result

This is a complete B application. Its `.h` declares `u32 main() on cpu;`; the implementation imports the existing `lessons.map` algorithm:

```c
module applications.map_cpu;
import lessons.map;

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    target(cpu, demo(values));

    for (value in values) { if (!println(value)) { return 1; } }
    return 0;
}
```

The application allocates four values, chooses `target(cpu, ...)` in source, and prints `3`, `5`, `7`, `9`, one per line. Build once, then run its EXE without a device argument:

```powershell
bc -c sample/applications/map_cpu.b -o target/book-apps/map_cpu.obj -I sample
blink target/book-apps/map_cpu.obj -o target/book-apps/map_cpu.exe
.\target\book-apps\map_cpu.exe
```

The [GPU application](sample/applications/map_gpu.b) calls `upload(values)`, `target(gpu, demo(values))`, and `download(values)` in source before printing. Its allocation and algorithm are the same:

```powershell
bc -c sample/applications/map_gpu.b -o target/book-apps/map_gpu.obj -I sample
blink target/book-apps/map_gpu.obj -o target/book-apps/map_gpu.exe
.\target\book-apps\map_gpu.exe
```

Both programs print the same four numbers. Copy the EXE and the linker-generated `blink_app_*.dll` files together when deploying; running them requires neither B sources nor a compiler or module runner. GPU execution still requires the documented Vulkan capabilities. Device choice is application code, rather than a runtime command-line switch.

### 1.6 · Let scale expose the benefit

Four numbers are easy to check by hand. To compare efficiency, apply the same formula to one hundred million numbers. Both serial and parallel versions perform one array transformation with the same formula, data type, and amount of work. A sufficiently large input gives parallel resources room to contribute.

The book's performance comparisons use Release builds throughout. The Blink compiler, CPU/GPU execution framework, and measurement program use Rust release optimizations. The compute library written in the B language in Chapter 14 is also built in Release mode. Debug builds are used for investigation; their timings are excluded from these comparisons.

Three real implementations are compared: single-threaded C++ optimized with `/O2 /arch:AVX2`, native CPU range parallelism from B-language programs, and native GPU execution of the same B-language program. C++ retains compiler auto-vectorization. The B-language CPU version uses the machine's available worker threads.

To reduce incidental variation, the program warms up, repeats measurements, and reports the median. Each measurement executes the algorithm once. C++ and B-language CPU timing starts before the algorithm call and ends after completion. B-language GPU timing starts before submission and ends after actual device completion, including command organization, submission, and waiting. Speedup is serial C++ time divided by B-language parallel execution time.

Code generation, initial allocation, and data preparation are outside the compute timing. In the main GPU comparison, code and data are already resident, and one submission performs one algorithm execution. Upload-execute-download costs are measured separately so that execution can be chosen according to where the data lives.

Build the measurement program as described in [Appendix A.5](sample/并行编程的艺术.md), then run from the Blink root:

```powershell
.\target\release\bperf.exe `
    map 100000000
```


### 1.7 · Put computation where it belongs


*Figure 1-3: compare equivalent computation first, then include the real cost of delivering the data.*


<a id="ownership"></a>

## 2 · The second beauty: give reads and writes clear ownership

### 2.1 · A syntax bridge for C/C++ readers

| Familiar concept | B-language syntax | Important difference |
|---|---|---|
| `float`, 32-bit integers | `f32`, `u32`, `i32` | Do not carry assumptions about C's `int` size into the program |
| Three floating-point components | `float3` | A native vector value, not three system threads |
| A structure | `struct Cell { f32 value; };` | Organizes ordinary values |
| Array plus length | `dataset<T>` | Also carries resource identity, access mode, and an execution domain |
| A read-only parameter | `read dataset<T>` | Explicitly restricts shared mutation |
| An output parameter | `write T` | Must be written before reading; does not imply automatic zeroing |
| Updating an existing value | `inout T` | Borrows an exclusive position, rather than an arbitrary raw pointer |
| An ordinary function | `f32 f(f32 x)` | CPU applications have native calls/recursion; compute-target calls remain subject to implementation limits |
| An entry with compute stages | An ordinary definition such as `void f(...)` | Can contain parallel domains and collectives; no separate `program` category is needed |
| CPU/GPU versions of one algorithm | `on compute` | Permits both backends; the caller explicitly selects the execution device |

`.h` describes a module's types, signatures, access permissions, and execution targets; `.b` provides implementations. Module declarations locate both files without C-style `#include` preprocessing.

### 2.2 · Put the computation in an entry point

The first chapter's compute entry is in [map.b](sample/lessons/map.b):

```c
void scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}
```

`dataset<f32>` denotes a collection of 32-bit floating-point values. `inout` permits reading and updating them. The compute entry uses an ordinary `void` function definition. `on compute` permits CPU and GPU code generation; the caller explicitly chooses the execution device.

`demo` produces `[1,2,3,4]` and calls `scale`. Initialization and the algorithm are separate, making the first run easy to inspect. See [Appendix A](sample/并行编程的艺术.md) for module declarations and interface files.

### 2.3 · Why exclusive writes matter

A common mistake when parallelizing C/C++ is to let many workers execute the following operation. `total` has just one output record, so they all write to `total[0].value`:

```cpp
// Conceptual parallel body: this is a data race.
total[0].value += input[i].value;
```

Several workers read the old value, each adds its own number, and their final writes overwrite one another. Even when addition is simple, the whole read-compute-write sequence is not indivisible.

An ordinary `parallel` domain in the B language gives each invocation exclusive write access to its current element. Shared mutation uses explicit atomics; collective summation uses reduction. Independent updates and shared modification are therefore distinguishable in the source.

Access permissions constrain conflicting reads and writes. The algorithm still determines the formula, numerical precision, and business rules.

![Figure 2-1: conflicting writes lose updates; exclusive outputs separate writing responsibilities.](sample/figures/ownership.svg)

*Figure 2-1: conflicting writes lose updates; exclusive outputs separate writing responsibilities.*

Both workers in the figure read 0, calculate independently, and write back, overwriting an update. A reduction first forms independent summaries and then combines them, resolving shared accumulation for a collective sum.

### 2.4 · What write, inout, and read actually mean

Think of `write` as producing a new result, `inout` as reading and updating an existing value exclusively, and `read` as allowing shared reading.

`write` declares write access; the algorithm or host handles initialization. The application uses `dataset<f32>(4)` to allocate four zero-initialized records in main.

CPU application functions now support whole-dataset read/write/inout borrowing. Explicit atomic resources use atomic access. Individual exclusive positions can be borrowed through `inout T` or `write T`. Compute functions pass the required permissions between stages, but none of these capabilities permits one parallel invocation to arbitrarily access the entire dataset being modified by other invocations.

### 2.5 · Different jobs for parallel and for

`parallel (value in values)` establishes a set of element invocations. `for (item in data)` performs a bounded sequential traversal inside the current invocation; it does not create another layer of parallel tasks.

If each of N outputs traverses N inputs, the result is O(N²) work. Parallelism does not remove redundant computation. Eliminate unnecessary work first.

<a id="reduction"></a>

## 3 · The third beauty: many results can become one

### 3.1 · Turn an accumulation chain into a tree

Consider the sum of `[3,1,4,2]`. To make the result available to later stages, `total` stores the number in one `Count` record. Inputs also use `Count { u32 value; }`.

The serial C core is:

```cpp
total[0].value = 0;
for (size_t i = 0; i < n; ++i) {
    total[0].value += input[i].value;
}
```

The parallel version written in the B language is:

```c
reduce_sum(input.value, total.value);
```

`input.value` denotes the value fields of all input records. `total.value` is the result field in a dataset of length one. This expresses the collective intent; the compiler expands the collective algorithm.

![Figure 3-1: a linear accumulation chain and a hierarchical reduction. Summaries at the same level can be computed independently.](sample/figures/reduce-tree.svg)

*Figure 3-1: a linear accumulation chain and a hierarchical reduction. Summaries at the same level can be computed independently.*

Each serial accumulation step uses the preceding result. A tree computes `3+1=4` and `4+2=6` first, then `4+6=10`. Three necessary binary combinations occupy two dependency levels. A balanced tree over n inputs has O(log n) levels but still performs O(n) work.

### 3.2 · One line of source, several execution levels

The accompanying [scan.b](sample/lessons/scan.b) provides a complete entry:

```c
void sum(read dataset<Count> input,
            write dataset<Count, 1> total) on compute {
    reduce_sum(input.value, total.value);
}
```

The CPU first computes partial sums independently over ranges and then combines the range summaries. The GPU forms partial sums in workgroup shared storage. For larger inputs, it continues reducing workgroup summaries until one result remains. Synchronization within a workgroup coordinates that group only; the execution framework organizes stages between groups.

From this reduction statement, the compiler can organize local sums and hierarchical combination. Collective intent, resource permissions, and stage dependencies determine the CPU/GPU execution structure together.

### 3.3 · Understand associativity first

For ideal integer addition, `(a+b)+c` and `a+(b+c)` agree, so parentheses can be rearranged into a tree. Subtraction does not have this property. An interface that appears to combine two values is not sufficient justification for an arbitrary tree reduction.

Associativity permits regrouping; commutativity permits exchanging left and right operands. They are different properties. Later chapters show parallel reductions and scans that preserve operand order.

The B language's u32 addition retains the low 32 bits, giving overflow defined modular semantics. A business count that must not overflow still needs an appropriate scale; a truncated result is not an arbitrary-precision integer.

### 3.4 · Floating-point values are not mathematical real numbers

f32 has finite precision. Different groupings can change rounding. A small value can disappear when added to a large one; canceling large terms first may preserve it.

CPU and GPU reductions can group operations differently and therefore have rounding differences. Evaluate floating-point computation in terms of input scale, acceptable error, special values, and algorithmic stability.

### 3.5 · Run it and inspect the result

The B application allocates four input records and one total, chooses the device in its target call, and prints `10`. The GPU source explicitly uploads and downloads the resources. Build and run without runtime device arguments:

```powershell
bc -c sample/applications/reduce_cpu.b -o target/book-apps/reduce_cpu.obj -I sample
blink target/book-apps/reduce_cpu.obj -o target/book-apps/reduce_cpu.exe
.\target\book-apps\reduce_cpu.exe
```

```powershell
bc -c sample/applications/reduce_gpu.b -o target/book-apps/reduce_gpu.obj -I sample
blink target/book-apps/reduce_gpu.obj -o target/book-apps/reduce_gpu.exe
.\target\book-apps\reduce_gpu.exe
```

Both variants use the same `lessons.scan::demo_sum` algorithm on `[3, 1, 4, 2]`.

### 3.6 · Turn many summaries into throughput

The performance input contains ten million elements. The comparison measures one complete algorithm execution, using the scope defined in Section 1.6.

```powershell
.\target\release\bperf.exe `
    reduce 10000000
```


Each input contributes one integer addition, and the optimized serial implementation is already compact. B-language CPU divides ten million items into parallel ranges and combines their summaries. GPU submission and waiting account for a larger share of this lightweight computation. Inputs repeat `[3,1,4,2]`; the sum is twenty-five million, and all three implementations produce the same integer result.


## Tools and native integration

| Tool / library | Purpose |
|---|---|
| `bc` | Check sources, compile COFF objects, and export native CPU/GPU compute code |
| `blink` | Link compiled objects into Windows x64 executables or compute libraries (DLLs) |
| `brun` | Module runner for backend isolation tests and diagnostics; ordinary applications run their EXEs directly |
| `blanguage` | Frontend, type/access checking, parallel IR, optimization, and native backends |
| `blcompute` | Data, device resources, transfers, native execution, and release after completion |

Write standalone applications in the B language, or compile compute functions written in the B language into DLLs callable from **C, C++, and Rust**. The toolchain also generates C ABI headers, import libraries, C++ wrappers, and Rust bindings. The native Rust API supports resident resources and asynchronous submissions.

- [Standalone applications](sample/applications/README.md): CPU entry points, dynamic data, and explicit CPU/GPU computation.
- [Call B-language programs from C/C++/Rust](sample/interop/README.md): build and call the same algorithm DLL, starting with four numbers.
- [Call external DLLs from the B language](sample/extern/README.md): C ABI, parameter access, and resource boundaries.
- [Native integration and deployment](Docs/NativeInterop.md): artifacts, ABI, deployment, and current limitations.

### AI-facing tools and systems development

Compilation, debugging, inspection, and basic measurement tools serve AI as their sole direct user; architects use AI to inspect actual code, data, and execution. The long-term design lets AI write observation and analysis programs, compute near the data, express concrete machine implementations, and combine experiments on real inputs. Optimization, debug information, and runtime collection are independent choices; analysis and implementation selection remain with AI or the user. See the [AI-facing tool principles and individual responsibilities](Docs/Architecture.md#面向ai的基础工具与全流程观察) for current foundations and planned capabilities. Complete debugging and experiment interfaces are not yet delivered; the design adds no automatic bottleneck judgments, scoring, or second authority over runtime state.

B develops self-hosting compiler and debugging/inspection responsibilities alongside the interfaces and libraries required for complete systems programs. System ABI declarations and meaningful B library wrappers are valid capabilities; independent native support may remain in Rust. Completion is based on actual ownership and rebuilding, rather than a source percentage or a B shell around the original implementation. See the [architecture](Docs/Architecture.md#长期系统设计目标与语言边界) for boundaries and current status.

AI can inspect an artifact's stored build request and source identities with `bc inputs <compiled.obj|entry.blbi>`. This JSON query neither reads current B sources nor recompiles the artifact. See [build input inspection](Docs/BLanguage.md#实际构建输入查询) for fields and identity scope; this command does not imply that complete debugging tools are available.

Use `bc layout <module.h|module.b> <type> [-I module-root] [--field path]` for a JSON query of a concrete source type or static field. `bc build` also exports `.layout.json`; the SDK can inspect an existing execution interface without source compilation. See [type and field layout inspection](Docs/BLanguage.md#类型与字段布局的结构查询) for descriptor, element-storage, and resource-word boundaries.

Use `bc ir <module.b>` to query the actual module, selected parallel entry, or application IR as JSON, with function or optimized-stage selection. The SDK can inspect the same compilation result without another compilation. Function/program IR also exposes source_origin with the original definition snapshot, range and owning instance identity; existing IR retains that snapshot after files change. Each IR node can expose its emission-time syntax range through source_site, with null for missing material. The same facts also retain the actual specialized instance and inline expansion sites in outermost-first order. Collective calls without ordinary instructions expose their original call-token range and concrete instance through Stage::operation_source() and stage JSON operation_source; value-kernel entry origins remain separate. Optimization provenance and machine-code mappings remain separate. See [operation syntax](Docs/BLanguage.md#ir操作的语法来源). See [definition origins](Docs/BLanguage.md#函数与程序ir的原定义来源). See [IR inspection](Docs/BLanguage.md#实际ir的按需结构查询) for phases, logical references, and artifacts that no longer retain IR.

Use `bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]` or `native::instructions_json` for structured decoding of an explicitly selected x64 instruction range. Compiler-produced NativeCode, StageCode and ProgramImage values expose `code_json` with the actual main/helper code, alignment and literals; `bc build` also exports `.x64.json`. The SDK can explicitly save separate CPU range material. Query a saved BLBI with `bc code <entry.blbi> --observations <ranges.blcr>` or a BLRI with `bc code --image <deployment.blri> --observations <ranges.blcr>` without source files. Execution bytes exclude the material; Use `bc code --object <compiled.obj> [--section number | --function exact-symbol]` for original physical COFF records and instructions in resolved `.pdata` ranges. The original Object can explicitly save separate `.blcf` material; `--observations <ranges.blcf>` pairs it with actual code and relocations to expose complete generated code, helper, alignment and literal regions. Without material, other bytes remain unclassified. Final PE inspection remains separate. See [native image queries](Docs/BLanguage.md#生成期原生映像的范围与指令查询).

Use `bc spirv <module.spv>` or the compiler SDK’s `spirv::module_json` to inspect every original instruction, offset, word, type/result/operand ID, entry point and literal bit pattern in one module, without recompilation or GPU execution. ShaderCode, StageCode and ProgramImage expose code_json for every original main/helper module; bc build writes a paired .spv.json for each .spv. Pure GPU artifacts can be queried without sources or CPU range material using `bc code <entry.blbi>` or `bc code --image <deployment.blri>`. SPIR-V remains an intermediate representation. See [single-module queries](Docs/BLanguage.md#原spir-v单模块的完整结构查询).

`bc image <entry.blbi> -o <deployment.blri>` exports the original CPU/GPU execution image while retaining the build cache and input record. The Rust runtime SDK can load it with compiler features disabled. See [deployment images](Docs/BLanguage.md#构建记录与可部署执行映像).

Optional CPU stack reports are available through `bc -c ... --stack-report <file>` and `blink --stack-report <compiled-inputs>... -o <file>`. They expose actual frames, closed call-chain bounds, and unknown recursive, external, or indirect ranges. A caller-supplied remaining stack budget is used only for reporting; PE reserve is not assumed to be a thread’s remaining capacity. See the [stack report specification](Docs/BLanguage.md#cpu调用链的静态栈报告).

## Capabilities and direction

| Area | Current status |
|---|---|
| CPU/GPU computation | Native x64/SPIR-V backends, parallel domains, collectives, explicit atomics, and tile/subgroup cooperation |
| Data and static abstraction | Records, arrays, vectors/matrices, generics, open `u32 const` parameters, access contracts, and valid prefixes |
| Applications and modules | Standalone CPU applications, native function calls/recursion, and a limited implementation of independent modules and multi-object linking |
| Platforms | Windows x64 CPU and Vulkan GPU today; other platforms and CPU architectures remain future work |
| Compute domains | Domain compilation and execution have a defined architectural scope; not implemented |
| Stable hot updates | Version binding, state migration, and resource handover are architectural goals; not implemented |

CPU applications can construct datasets of owning elements with `dataset<T>(length,F)`, using a static factory for each initialized element. Whole-dataset moves, returns and final element cleanup retain the original resource ownership. CPU functions can declare `requires nonzero(rows[*].value)` and `requires nonempty(rows[*].value.values)` for common integer and child-resource facts about initialized elements. The `requires index(rows[*].value.index,rows[*].value.values)` contract also carries each element’s index relation to its actual child resource; see the [value](Docs/BLanguage.md#common-element-nonzero), [resource](Docs/BLanguage.md#common-element-nonempty) and [index](Docs/BLanguage.md#common-element-index) contracts. See the [implemented scope and remaining element borrowing and container operations](Docs/BLanguage.md#cpu-owned-dataset-elements).

CPU applications can also move and refill dynamically indexed positions in fixed owning arrays, releasing the resources still owned along actual branches, loops, and returns. Conditional CPU/GPU output locations for returned values and write/inout parameters can cross ordinary functions, compiled objects, and archives; conditional inputs follow the actual use path, including loop early exits. See the [compiled-module resource ABI](Docs/BLanguage.md#已编译模块的资源abi) and [conditional resource outputs](Docs/BLanguage.md#跨函数模块的条件资源输出位置).

The B language **does not adopt object-oriented programming**. Programs are organized around data, functions, modules, and execution domains. The design values top-tier efficiency, direct control over machine execution, checkable interfaces, and use by people, AI code generators, and higher-level compilers. Current rules, native runs, and performance results are recorded separately in the [language reference](Docs/BLanguage.md) and [current architecture](Docs/Architecture.md).

## Learn and explore

| Start here | What it covers |
|---|---|
| [The Art of Parallel Programming](sample/并行编程的艺术.md) | Moving from C/C++ to parallel thinking; [mobile PDF](sample/pdf/并行编程的艺术-B语言-手机阅读版.pdf) · [standard PDF](sample/pdf/并行编程的艺术-B语言.pdf) |
| [Blink and Rust](sample/Blink和Rust混合编程.md) | Minimal integration through simulation and repeated GPU execution; [examples](sample/hybrid/README.md); [mobile PDF](sample/pdf/Blink和Rust混合编程-手机阅读版.pdf) · [standard PDF](sample/pdf/Blink和Rust混合编程.pdf) |
| [B Language Source Analysis](sample/B语言源码分析.md) | Compiler, IR, backends, and execution framework; [version and reading notes](sample/compiler/README.md); [mobile PDF](sample/pdf/B语言源码分析-手机阅读版.pdf) · [standard PDF](sample/pdf/B语言源码分析.pdf) |
| [BLEngine Source Analysis](sample/BLEngine源码分析.md) | How the first consumer uses the language, scenes, and lighting; [reading notes](sample/engine/README.md); [mobile PDF](sample/pdf/BLEngine源码分析-手机阅读版.pdf) · [standard PDF](sample/pdf/BLEngine源码分析.pdf) |
| [B standard library](library/README.md) | Dynamic value sequences, strict UTF-8/UTF-16 codecs, whole-text conversions and owned slices, plus Windows system information, timing, thread error codes, raw command lines, environment access, UTF-8 file input, path existence, absolute and canonical path identities, extension replacement, and path composition |
| [Language reference](Docs/BLanguage.md) · [Architecture](Docs/Architecture.md) · [Compute domains](Docs/ComputeDomain.md) | Current rules, long-term boundaries, and future design |

Technical documentation and books are currently primarily in Chinese. Both README editions provide the same starting points and capability descriptions. The table follows main and its companion examples; basic applications have been verified with the formal tools stated above. PDFs in older tags are historical snapshots without this teaching update. BLEngine is an independent consumer and is not yet open source; this repository provides its case-study book and reading notes, rather than the engine source.

<a id="feedback"></a>

## Feedback and technical discussion

Use [GitHub Issues](https://github.com/sgua83/Blink/issues) for questions, defects, and feature requests. When [opening an issue](https://github.com/sgua83/Blink/issues/new), include minimal B source, exact commands, `bc --version` / `blink --version` / `brun --version`, the source tag or commit, Windows and CPU/GPU/driver details, and original errors. For performance comparisons, include input size, build profile, timing scope, and data-transfer costs so others can reproduce the result.

## Contributing

P4 is the development source of truth. Each developer submits through their own account and client. GUA (192.168.10.5) publishes the GitHub mirror at 09:00 and 21:00 China time, continuing from the P4 watermark confirmed by the remote branch. The next invocation includes every change left unpublished by a failed run. Developer nodes do not need GitHub credentials. See [central mirror publication](Tools/README.md).

Read the [project conventions](AGENTS.md) and [current implementation scope](Docs/Architecture.md) before making changes. Include B-language source code, exact commands, toolchain/device details, and original errors when reporting a problem. Distinguish design goals, implementation, native execution, and measured performance. Maintainer submission and publishing procedures are documented in the project conventions.

## License

[MIT](LICENSE).
