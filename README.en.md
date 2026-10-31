<div align="center">

<h1>Blink Programming Language</h1>

<p><strong>Native parallelism · Unified CPU/GPU programming · Native domain compilation and execution</strong></p>
<p>A native parallel programming language for the AI supercomputing era</p>

<p><a href="README.md">简体中文</a> | <strong>English</strong></p>
<p><a href="#quick-start">Quick start</a> · <a href="#parallel-programming">Read chapters 1–3</a> · <a href="Docs/BLanguage.md">Language reference</a> · <a href="#learn-and-explore">Learn</a> · <a href="Docs/Architecture.md">Architecture</a></p>

</div>

**Blink is a native parallel programming language, also known as the B language.** Use it to write standalone applications, multicore CPU programs, and GPU compute programs. The same compute source can be compiled to native CPU and GPU code.

The B in the name also stands for **Base**. Blink aims to become a foundational programming language for the AI supercomputing era, with parallelism, data relationships, and cooperation built into the language, while developing complete general-purpose and systems programming capabilities.

This repository contains the B-language compiler, linker, native execution framework, language reference, and examples. B-language interfaces live in `.h` files and implementations in `.b` files. The compiler generates Windows x64 machine code or SPIR-V for the selected target. The execution framework owns data, devices, transfers, and completion. The compiler and framework are currently implemented in Rust, without third-party compilers, shader compilers, or parallel runtimes.

> **In development — software version in [VERSION](VERSION).** Native execution on a single machine's CPU and GPU is available. Domain compilation, domain execution, and stable hot updates are established design goals and are not implemented yet. See the [architecture and current implementation scope](Docs/Architecture.md).

## Three core features

### 1. Native parallelism

Parallelism is a language capability. Parallel domains, access permissions, cooperation within a block, and stage dependencies enter compiler analysis and become native execution. Algorithms can express exclusive writes, shared reads, explicit atomics, and collective operations.

### 2. Unified CPU/GPU programming

**One set of B-language sources, native execution on CPUs and GPUs.** Applicable algorithms share an implementation across both targets. `on` declares target capabilities, `target` chooses an execution location, and `upload` / `download` make data transfers explicit. Hardware differences and transfer costs remain visible.

### 3. Native domain compilation and execution

**Use a group of machines as compilation and execution resources.** A compute domain organizes machines for native compilation and execution. Build and execution domains can be selected independently, with explicit semantics for partitioning, communication, dependencies, and completion.

This capability is currently in design. Each build or execution uses resources within its selected domain; applications orchestrate workflows between domains. See the [compute-domain design](Docs/ComputeDomain.md) for responsibilities, deployment, and interfaces.

## Quick start

Use Windows x64, the [Rust stable MSVC toolchain](https://rust-lang.org/tools/install), and [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with x64 C++ tools and the Windows SDK. Open an x64 Developer PowerShell:

```powershell
git clone https://github.com/sgua83/Blink.git
Set-Location Blink
cargo build --workspace --release
```

Run all subsequent commands from the Blink root. CPU examples need no GPU. GPU examples need a Vulkan driver supporting each program's requirements, without a separate Vulkan SDK. The f32 example currently requires `DenormPreserve` and `SignedZeroInfNanPreserve`; unsupported devices receive an explicit error.

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

Run this command in PowerShell:

```powershell
.\target\release\blrun.exe cpu `
    sample/lessons/map.b demo `
    sample Saved/Book/map :4
```

This invokes `blrun.exe` on the CPU and selects the `demo` entry in `sample/lessons/map.b`. `sample` is the module search root; `Saved/Book/map` is the output directory; `:4` allocates four zero-initialized records. `demo` first fills them with `[1,2,3,4]`, then performs the multiply-and-add transformation.

The resulting `values.bldata` contains four numbers:

```text
[3, 5, 7, 9]
```

`blrun` prints program information and execution timing; computed values are written to BLDATA files. The numbers in the figure were read from that file. To read it yourself, see [Appendix A.3 of the book](sample/并行编程的艺术.md).

Run the same sources on a GPU by selecting `gpu` and another output directory:

```powershell
.\target\release\blrun.exe gpu `
    sample/lessons/map.b demo `
    sample Saved/Book/map-gpu :4
```

In the book's recorded runs, both `values.bldata` outputs contain `[3,5,7,9]`, with one f32 per record. The complete implementation is in [map.h](sample/lessons/map.h) and [map.b](sample/lessons/map.b).


*Execution record: excerpts of the program entry and actual output, with values read from BLDATA.*

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

`write` declares write access; the algorithm or host handles initialization. In the example command, `:4` asks the runner to allocate four zero-initialized records.

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

Run the `demo_sum` entry on the CPU:

```powershell
.\target\release\blrun.exe cpu `
    sample/lessons/scan.b demo_sum `
    sample Saved/Book/reduce :4 :1
```

`:4` allocates four input records and `:1` allocates one total record. Run on the GPU with:

```powershell
.\target\release\blrun.exe gpu `
    sample/lessons/scan.b demo_sum `
    sample Saved/Book/reduce-gpu :4 :1
```

`demo_sum` produces `[3,1,4,2]` and sums it. The single u32 in `total.bldata` is 10. This entry shares its input generator with the next chapter's scan example.


*Execution record: excerpts of the program entry and actual output, with values read from BLDATA.*

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
| `blc` | Check sources, compile COFF objects, and export native CPU/GPU compute code |
| `bllink` | Link compiled objects into Windows x64 executables or compute libraries (DLLs) |
| `blrun` | Bind data and execute CPU/GPU compute modules |
| `blanguage` | Frontend, type/access checking, parallel IR, optimization, and native backends |
| `blcompute` | Data, device resources, transfers, native execution, and release after completion |

Write standalone applications in the B language, or compile compute functions written in the B language into DLLs callable from **C, C++, and Rust**. The toolchain also generates C ABI headers, import libraries, C++ wrappers, and Rust bindings. The native Rust API supports resident resources and asynchronous submissions.

- [Standalone applications](sample/applications/README.md): CPU entry points, dynamic data, and explicit CPU/GPU computation.
- [Call B-language programs from C/C++/Rust](sample/interop/README.md): build and call the same algorithm DLL, starting with four numbers.
- [Call external DLLs from the B language](sample/extern/README.md): C ABI, parameter access, and resource boundaries.
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

The B language **does not adopt object-oriented programming**. Programs are organized around data, functions, modules, and execution domains. The design values top-tier efficiency, direct control over machine execution, checkable interfaces, and use by people, AI code generators, and higher-level compilers. Current rules, native runs, and performance results are recorded separately in the [language reference](Docs/BLanguage.md) and [current architecture](Docs/Architecture.md).

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

P4 is the development source of truth. Each developer submits through their own account and client. GUA (192.168.10.5) publishes the GitHub mirror at 09:00 and 21:00 China time, continuing from the P4 watermark confirmed by the remote branch. The next invocation includes every change left unpublished by a failed run. Developer nodes do not need GitHub credentials. See [central mirror publication](Tools/README.md).

Read the [project conventions](AGENTS.md) and [current implementation scope](Docs/Architecture.md) before making changes. Include B-language source code, exact commands, toolchain/device details, and original errors when reporting a problem. Distinguish design goals, implementation, native execution, and measured performance. Maintainer submission and publishing procedures are documented in the project conventions.

## License

[MIT](LICENSE).
