# C、C++ 与 Rust 集成

C++ 与 Rust 都是 Blink 的正式宿主。C/C++ 可直接链接 bllink 生成的 DLL；Rust 可使用同一 DLL 的安全包装器，也可继续使用 blanguage、blcompute 原生 crate。原生 Rust 的编译、CPU 执行、GPU 驻留、异步提交和所有权接口不经过 C ABI，继续作为正式 SDK 提供。

## 编译算法，再链接为 DLL

当前目标为 Windows x64。使用源码 SDK，在 Visual Studio x64 Developer PowerShell 中构建 Blink，再执行：

```powershell
cargo build --workspace --release
.\target\release\blc.exe -c `
    sample\lessons\map.b `
    -o target\interop\map.obj `
    -I sample `
    --program scale
.\target\release\bllink.exe `
    target\interop\map.obj `
    -o target\interop\map.dll
```

`--program` 选择具体公开 计算函数；开放 const 入口可以用 'run<float3,1000000>' 等实例。每个 DLL 导出一个无 requires 前置条件的具体入口；条件模板应先在 B 中组合成能够自行生产所需关系的 计算函数。不会把裸指针中的 Count 当作编译器已经证明的 prefix 关系。

| 输出 | 用途 |
|---|---|
| map.dll | 独立运行的算法库；部署到宿主可执行文件旁 |
| map.lib | C、C++、Rust 在 Windows MSVC 下链接使用的导入库 |
| map.abi.h | C ABI、数据布局及 C++ RAII 包装器 |
| map.rs | 相同布局、extern C 声明及 Rust 安全包装器 |

B 的源码接口 map.h 和生成的 C 头文件 map.abi.h 是不同文件。导出前缀来自编译目标文件名，只接受 ASCII 字母、下划线和非首位数字；不同算法库应使用不同名字。生成接口与对应 DLL 一起发布，不混用其他版本的绑定。

固定长度的 `COUNT` 常量与 C 头文件保护宏保留名称的原大小写身份：小写 `a` 至 `y` 写成对应大写，数字保持；小写 `z`、原大写字母及下划线写成 `Z` 加原 ASCII 字节的两位大写十六进制。`a`／`A` 因而分别编码为 `A`／`Z41`，`z41`／`Z41` 为 `Z7A41`／`Z5A41`，`input_data` 为 `INPUTZ5FDATA`。Rust 使用 `<编码参数名>_COUNT`，C 使用 `<编码导出前缀>_<编码参数名>_COUNT`；保护宏使用同一前缀编码。按实际生成接口读取常量，不将不同参数或库的大小写折叠；原 `fractal`／`pixels` 仍生成 `FRACTAL_PIXELS_COUNT` 和 `PIXELS_COUNT`。C 导出符号、类型及参数名称保留原规则，实际数据布局与调用协议保持。

blc 在构建时完成 B 的模块、类型、访问权限、阶段及入口检查，并用自研后端生成全部 CPU 机器码与 SPIR-V，包括集合尾部、扫描传播、排序和间接启动入口。DLL 携带最终代码、重定位和执行布局，不内嵌值 IR。create 只装载 CPU 代码或 Vulkan pipeline；CPU 的机器码生成已移到构建时，GPU 驱动仍负责接收 SPIR-V 并建立实际管线。声明 on cpu／on gpu 的入口只生成对应后端，on compute 生成两端，无法编译的目标明确失败。

编译阶段由 blc 生成 B 算法原生代码，并使用 Rust 工具链把既有执行框架和 C ABI 编译成静态目标库，随 COFF 目标文件保存。链接阶段由 bllink 读取目标文件，调用 MSVC 本机链接器生成 DLL，不调用 blc、Cargo 或 rustc，也不需要 B 源码。编译使用源码 SDK 与 Rust 工具链，链接使用 Visual Studio C++ 工具和 Windows SDK；消费已生成 DLL 的 C/C++ 项目无需安装 Rust、Blink 编译器或 Blink 源码。C 导出及生成的 C++／Rust 调用签名保持。Rust SDK 新增整程序编译映像与装载入口，数据创建接口消费只读的 ProgramInterface；已装载程序的 program() 返回不含 IR 的 ExecutionPlan，直接标注旧 ir::Program 类型的数据布局消费者须改用该接口。

公开 ABI 前缀只决定外部接口，不再作为编译暂存静态库的身份。同一 SDK 中两个相同前缀的编译各自消费本编译单元的私有库，目标文件归档后清除该单元的生成源码与 Cargo package 产物，共享框架依赖缓存保留。应用编译同样沿此路径，不再为私有 Cargo 库单独计算源码散列前缀；应用执行 DLL 的既有内容身份继续由链接器使用。编译失败如实返回并结束暂存生命周期，链接和最终 DLL 无需保留这些临时产物。

部署机器需要 Windows x64 与 [Microsoft Visual C++ x64 运行库](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。GPU 后端另需支持程序所需 Vulkan 能力的显卡驱动；不要求安装 Vulkan SDK。

## 三种宿主

完整、可运行的工程命令见 [sample/interop](../sample/interop/README.md)。最短调用如下。

C++ 使用头文件中的 RAII 类：

```cpp
#include "map.abi.h"
float values[] = {1, 2, 3, 4};
blink_map::Program program(blink_map::Backend::Cpu);
program.run(values, 4);
// values: 3, 5, 7, 9
```

Rust 使用同一个 DLL，包装器公开切片、Result 与 Drop：

```rust
let mut values = [1.0, 2.0, 3.0, 4.0];
let mut program = map::Program::new(map::Backend::Cpu)?;
program.run(&mut values)?;
// values: [3.0, 5.0, 7.0, 9.0]
```

纯 C 使用 map_create、map_run、map_destroy，返回值 0 表示成功、-1 表示失败；错误文本写入调用方提供的 UTF-8 缓冲区。见 map.c。C++ 包装器把错误转为 std::runtime_error，Rust 包装器转为 Result。没有跨 DLL 传递 Rust Vec、String、C++ 对象或其分配器。

Rust 原生入口继续使用 compile_entry、Dataset、CpuProgram、Device、GpuProgram 等类型。native.rs 已分别执行 CPU 和 GPU；需要跨提交保留设备数据、view／纹理／加速结构与真实资源所有权时，继续使用原生 API。

## 数据、完成与释放

- DLL 当前接受纯值 dataset 和 uniform：f32／u32／i32／bool、向量、矩阵、编号枚举、普通值／void result、普通或具体泛型记录、固定内联数组。生成布局直接取自编译器的字段偏移、对齐与步长；布尔字段使用32位整数，B写出为0或1，原生传入按零／非零解码。
- 向量、矩阵、嵌套记录、数组和结果展开为标量成员，填充与字节偏移显式保留。每个记录字段段使用 `_f<原 ASCII 长度>_<编码名称>`；名称中的 `z` 编为 `z7a`、`_` 编为 `z5f`，其余字符保持。例如字段 `a_b` 生成 `v_f3_az5fb`，路径 `a.b` 生成 `v_f1_a_f1_b`；名称中的下划线不会丢失层级身份或产生 C++ 保留的双下划线。数组下标、向量分量及结果的 tag／payload 继续使用原后缀，例如 `v_f4_tags_0`、`v_f8_position_x`、`v_f8_response_tag`；矩阵的行也保留自己的字段段。不要直接强转宿主自有结构；按生成类型填充字段。执行层只复制真实字段，不读取或改写宿主结构体的填充。
- 字段路径编码调整的是生成的 C／Rust 成员名称，原字节偏移、对齐、步长、C 导出符号及调用协议保持。重新生成记录接口后，宿主源码按对应头文件／绑定的新名称访问成员；标量别名及根向量名称保持，根数组的下标后缀保持，内部记录段遵守同一编码。已有 DLL 与配套旧绑定仍共同消费原布局，不混配不同版本生成接口。
- 每个数据指针配一个 u32 元素数，单位是完整记录。只读参数生成 const 指针／Rust 共享切片；可写参数生成可写指针／Rust 独占切片。各可写区间不得与其他参数重叠；调用期间宿主不能同时访问相应内存。
- run 是一次同步调用：准备本次存储、复制输入、执行、等待实际完成，再复制输出。GPU 路径包含上传与输出读回；DLL 不在返回后借用调用方数组。数组始终由宿主分配和释放，handle 由 DLL 创建并由 DLL 的 destroy 释放。所有 handle 销毁后才能卸载 DLL。
- create 的 backend 为 0（CPU）或 1（GPU）；目标限制与设备能力错误直接返回。相同 handle 可反复运行；调用及释放必须在创建线程串行执行。Rust 包装器以独占方法、Drop 和非 Send／Sync 类型表达这一所有权；C/C++ 遵守头文件中的相同约定。不同线程可分别创建自己的实例。
- view、纹理、加速结构和有外部 prefix 前置条件的程序不在当前 DLL 数据 ABI 中；算法目标文件编译明确拒绝，不改写成其他算法。C++ 可以调用包含内部多阶段、集合、排序及内部有效前缀的完整 计算函数。

## 性能的完整口径

sample/interop/fractal.* 是一百万像素的 Mandelbrot 计算，C++ 和 Rust 分别保留开启优化的串行实现，再调用同一个 DLL。每次计时只执行一次完整算法，包含 DLL 调用、本次分配、必要复制、GPU 提交／等待与输出读回。create 单独报告，不混入已复用实例的每次调用；首次只运行一回时应相加比较。重复测量使用两次预热、九次单次计时的中位数。

CPU 结果与两个串行实现逐项相同。当前 GPU 的 f32 结果在分形边界有差异，具体数量与最大迭代差由样例输出，不能将设备结果称为逐位等价。分形的递推会放大浮点差异；要求精确迭代计数的消费者应选用已核对的 CPU 路径或另行设计数值算法。不要只用加速比判断替换是否适合应用。

## B 主流程调用 C++ 或 Rust

B 的普通 CPU main、函数和 CPU 计算函数 已支持调用 Windows x64 DLL 的 C 导出。例如 `.h` 声明：

```c
extern "C" Pair add_values(Pair value, u32 amount) on cpu
    from "blink_cpp.dll";
extern "C" void scale_values(inout Pair value, u32 amount) on cpu
    from "blink_rust.dll";
```

`.b` 主流程可以取得 C++ 返回的 Pair，再将同一位置借给 Rust 修改。`as "真实符号"` 可指定与 B 函数不同的导出名称；省略时使用函数名。完整 B、C++、Rust 源码与构建部署命令见 [sample/extern](../sample/extern/README.md)，已实际运行并输出 `15`、`21`。blc 生成机器码和导入重定位，bllink 生成 PE 和导入表；外部 C++／Rust 工具链只构建各自的库。部署时携带实际 DLL 及其依赖，不需要 B 源码或编译器。

普通值按 Windows x64 C ABI 传递；read／write／inout 分别是实际存储的只读／完整输出／读写指针。前端保持类型、初始化和重叠借用限制。这里的普通值使用 B 的偏移、对齐、填充和数组步长；bool 使用 32 位 `uint32_t／u32`，向量是 POD 布局，不等同于 C++ 向量内建类型。C++ 的导出使用 extern C，Rust 使用 extern C 与 repr(C) 等实际匹配布局；普通 Rust ABI 和 C++ 类／模板 ABI 不属于该接口。

声明是可信 ABI 边界，DLL 必须匹配真实符号、类型布局与访问权限，指针只在调用期间有效，不得逃逸、越界或让异常／longjmp 跨边界。write 参数在每次正常返回前完整初始化全部逻辑字段。外部写入撤回旧值的静态证明；编译器不会检查 DLL 机器签名或推测失败状态对应的写入结果。只读临时值按必要生命周期保留，位置借用不维护另一份写回副本。

独立应用由 Windows 按实际导入装载库；原生 CpuProgram 用 LoadLibraryW／GetProcAddress 解析，原生代码所有者持有模块到最后一个消费者释放。装载／符号错误保留原生错误，不替换库或实现。外部调用的副作用不因返回值未使用而删除；parallel 实例的调用顺序不作保证。生成的 B 算法 DLL 也可在 CPU 后端调用其依赖库，GPU 后端明确拒绝宿主调用。

当前只接受无泛型／契约的 on cpu 普通外部函数和非空普通值／void 返回。dataset、tile、view、纹理、加速结构、拥有 dataset 的结果及含资源记录不能跨此 ABI；CPU 端已合法取得的普通元素仍可借用。裸指针值、u64、opaque handle、回调、变参、静态对象链接及其他平台仍待实现。B 应用创建动态数据与组织 CPU／GPU 计算函数 仍归 C-011，不以外部调用或宿主已有能力冒充完成；原生 Rust 计算 API 保留。

参考：[Microsoft DLL 使用指南](https://learn.microsoft.com/en-us/cpp/build/walkthrough-creating-and-using-a-dynamic-link-library-cpp)、[Rust extern 块](https://doc.rust-lang.org/reference/items/external-blocks.html)、[Rust 链接规则](https://doc.rust-lang.org/reference/linkage.html)。

## B 业务结果与调用状态

B 的 `result<T,E>` 是业务数据，和生成 DLL 的 create／run 返回状态分别处理。C ABI 的0／-1、C++异常及Rust包装器Result报告装载／设备／执行／接口调用；一个合法执行可以在输出记录中产生业务Err，不因此变成DLL调用失败。B的try只传播显式业务结果，不自动捕获平台失败、外部异常或回滚已经发生的写入。

结果布局以偏移0处4字节B bool判别（零为Ok，非零为Err，B构造为0／1），载荷从两类载荷与4的最大自然对齐开始，共用最大载荷大小，整体按同一对齐补齐。生成记录中的 `*_tag`、`*_payload_序号` 及显式填充反映这个真实布局；载荷字按活动分支的原类型解释。对齐填充不作为业务值保证，宿主不能仅因总大小相同就假设字段偏移相同。原生声明仍是可信边界，宿主提供有效活动载荷，不引入动态类型对象或第二份所有权状态。

结果作为普通POD可用于B导入的外部C函数值、返回及位置借用；生成算法接口继续同时服务C／C++和Rust。Windows x64的8字节寄存器结果和80字节间接结果已有PA3双向原生记录；含80字节结果的96字节记录由C++及Rust宿主在CPU／GPU各65条完整核对，包含非规范的非零判别字、NaN原始位及嵌套数组。实际验证使用正式Release、C++ /O2、Rust opt-level=3与panic=abort。

当前普通C ABI结果载荷支持不含view的普通值及void。B内部CPU应用已支持dataset／拥有记录结果，两者使用同一Frame所有权，不属于该ABI的普通POD。view资源图、一般变体、按错误分支的部分输出契约和结果形式的应用入口仍开放。接口语法、分支及显式传播见[语言规范](BLanguage.md#有载荷结果分支与显式传播2026-10-27)，资源按原最后真实消费者完成后释放。

Rust SDK的ir::Ty新增结果类别；直接使用该结构体字面量的消费者需补齐result字段，已有类型构造器自动保持相应类别。此项不改变生成C函数及C++／Rust包装器的调用签名。

## 拥有记录中的普通字段位置

B内部拥有记录组合dataset、拥有结果与嵌套记录；整份拥有记录及资源载荷尚不跨普通C ABI。其已初始化普通字段仍可作为read／write／inout位置交给实际外部函数，CPU dataset字段的普通元素同样沿原借用规则。原local_slots按Ty真实字节偏移保留自然对齐、内部及尾部填充，不能将逻辑分量紧排后冒充原生字段地址；外部写入沿原位置撤回旧数值事实。PA3 Release原生观察已确认标量字段的同一地址，及含float4的48字节、16字节对齐POD字段的实际写入。原生声明仍是可信边界，仅在该次调用期间借用，不保留指针或新建资源所有者；整份拥有聚合write／inout与完整资源ABI继续开放。

## 嵌套表达式退出与外部效果

普通函数的try错误退出可发生在C ABI实参的数值表达式或实际位置索引内；该路径不再求后续实参、不调用外部函数，也不交付相应write初始化。此前已发生的外部效果保留。正常路径仍传递原位置、遵守既有布局及借用，真实错误返回仍满足write完整输出契约；没有新增调用状态、ABI包装或回滚。PA3 Release原生观察已核对先前临时拥有资源释放、后续调用顺序以及实际外部写入是否发生，完整CPU／GPU与SDK范围记录于Issues。

## 统一函数与算法产物

源函数统一使用返回类型定义，整域计算可写 void 和显式 parallel；--program 仍选择生成算法对象的公开具体函数。内部 Program／ExecutionPlan 与生成的 C/C++／Rust ABI 名称保持原职责。该产物以借用数据及普通统一值为C接口；CPU普通B函数已有独立原生调用、递归、模块对象与普通位置借用，但这不等于普通函数的通用C资源ABI已经完成。完整资源阶段合同、借用返回、版本演化与热更新仍在研发。只有类型／外部接口的.h可以独立存在，实际DLL符号与布局责任不变。B的target为同步执行位置选择，低层SDK异步提交仍使用原完成对象。

## 普通B借用与C指针布局（2026-10-27）

普通B向量借用按原分量地址通过B模块调用，允许离散及重排；记录／矩阵／数组保留声明布局。这个B函数ABI与外部C的单个连续指针分别定义，不能把B逻辑向量的位置直接当作C连续对象。B包装函数把符号向量区域交给C时，在原接口写`requires contiguous(value)`或选中的字段／swizzle；调用点按实际位置静态满足，库导出该条件，递归和多层调用承接相同声明。直接且布局已知的C位置继续沿原规则，不增加条件许可。

contiguous包含对应类型的顺序、全部字段偏移及对齐。例如uint2需要8字节对齐；uint4的xy／zw满足，xz／yx／yz分别因离散／重排／对齐不能传给该C接口。C仍只收到同一实际存储的一个指针；不复制载荷、不生成运行时布局检查或另一份写回状态。标量子位置可以直接转交，普通B函数的合法重排不受C的表示要求限制。具体语法见BLanguage.md的“普通位置借用与contiguous”。

C只读参数同样不得读取未初始化的B原生位置；先由B或C的write完整初始化后再read／inout，原初始化事实沿真实参数位置保留。实际PA3 Release已拒绝旧版曾接受的只写原生参数提前C读取及部分初始化读取，同时验证C完成输出后的读取、递归／子区域合同与原C++／Rust接口。C的write完整性仍由可信导出实现负责，不能把一个声明当作检查了DLL内部机器码。

## 固定拥有数组的B模块交接（2026-10-27）

普通CPU应用可通过独立B目标文件及归档按值传递／返回固定拥有数组，元素可为dataset、拥有记录／Result或嵌套数组；资源位置与部分移动沿原资源树和模块合同交接。同一Frame持有真实资源，数组元素布局仍按Ty的字节偏移、对齐与步长组织。动态元素的dataset借用可以调用已编译CPU函数和CPU／GPU整域入口；实际候选来源仅参与编译期别名和存活期检查，不进入C函数签名或运行时资源表。

本次PA3 Release验证包含六个移除模块实现源码与头文件后的CPU／GPU消费者，资源返回、递归转交、部分移动和错误结果均实际运行；只重编库私有实现后，保留原应用目标文件重新链接并运行。此处为B模块ABI能力，不表示普通C ABI已接收拥有数组、view图或任意资源载荷。原C++／Rust外部POD调用、原生宿主API与普通字段借用继续沿原边界。

## 一般带载荷变体的布局与模块交接（2026-10-28）

普通带载荷enum与原result使用共同的判别字／重叠载荷布局；声明的u32／i32编号、活动载荷、自然对齐及最大存储共同组成真实ABI。生成的C++／Rust绑定按实际偏移导出tag与payload字，超过四个字的载荷仍按实际布局生成名称，不作为向量分量解释。原生接口的声明者／调用者须交付符合声明类型的活动值；编译器自身的分配及B源码写入沿原初始化关系维持该事实，不以零存储代替任意分支构造。

PA3 Release实际验证了含uint3／int3／float3及空分支的32字节、16字节对齐变体和48字节外层记录。C++／Rust算法宿主在CPU／GPU逐项核对到百万条并保留NaN与负零的原始位；普通C ABI的按值参数、返回与write原地址也已原生核对。拥有记录、固定拥有数组、嵌套变体及result／try经无库源码的B归档交接；拥有资源仍属于原Frame，未扩展为普通C资源ABI。

当前对象元数据为BLCO6，模块资源映像为BLRI2；前述BLCO4／BLRI1说明对应各增量交付时状态。实际旧对象由版本入口拒绝，须重新编译；链接仍消费已编译对象及合同，不读取B实现、不做IR／CPU／GPU生成或调用Rust编译器。仅改变库私有实现后已保持原调用对象重链接并完成CPU／GPU核对。view图、动态部分移动／一般初始化覆盖、借用返回、完整资源阶段及版本迁移／热更新继续开放。


### B模块的CPU借用view返回

CPU应用`view<T>(dataset位置)`沿原Frame取得现有CPU存储的不可变借用。普通view、含view记录／固定数组的值输入、返回及write／inout值输出，在BLCO6原生函数合同中交接编译期来源；调用点代入实际参数及聚合分量，分支合流保持可能来源。机器view仍为16字节／对齐8，资源ABI仍为BLRI2；没有第二Frame、运行时借用表或载荷复制。实际dataset所有者保持到借用结束，局部存储与临时实参不能通过返回地址逃逸。

PA3已用移走库.h／.b的对象和静态库交接view、记录、数组及输出位置；仅重编库私有计算后，原调用对象直接重链得到新结果。CPU应用的最后使用点与循环来源在编译期闭合后导出，调用者沿原编号代入实际参数及输出位置；普通CPU自递归／互递归及泛型、字段／数组与write／inout输出的来源，已在原函数与回边关系中闭合后交付；无正常返回来源的纯递归环仍报B271，拥有图／GPU构造的能力范围见规范；这不扩大普通extern C或算法DLL的资源接口，也不宣称完整借用／异步／热更新完成。


源模块可以通过源闭包推导跨模块互递归的合同，再分别生成各自的对象及静态库；没有实际目标文件依赖的模块编译使用这一源模式。提供产物依赖后按原接口导入，不因某个函数未提供目标文件而读取现存库.b。模块所有者只交付自己的代码，外部模块的私有泛型／helper不会变成额外导入；Frame需要沿完整函数关系形成后再裁剪代码。

PA3 Release已在两个互递归模块中交接普通view、字段、递归write及inout输出，并验证私有泛型仍由其原模块拥有。移走所有库.h／.b后，对象和归档消费者均完成完整记录核对及最后使用后的实际传输；公开合同保持的私有计算变化后，原调用对象直接重链得到新结果。实际范围及原始记录见Issues，不外推为任意接口变化或稳定热更新。


普通CPU函数的拥有记录、固定数组、变体／Result及嵌套组合现可通过read参数跨B模块只读借用。原生调用交接已有Ty布局的元数据和句柄，原Frame仍持有实际存储；同源只读参数可以重合，源资源及实际元数据位置的独占冲突继续在原编译所有者处拒绝。临时资源复用原实参作用域，view返回保留实际输入来源，借用变体的匹配不取得拥有权。

PA3 Release已验证无库.h／.b的对象及归档消费，CPU／GPU完整双字段记录覆盖0／1／65／1000003条。四次原生均完成128轮各1000003条CPU拥有作用域，原调用对象在公开合同保持的私有计算更新后直接重链得到新结果。BLCO6／BLRI2、机器布局、根运行库与Frame所有者保持；本轮没有开放普通C资源ABI、整份拥有聚合write／inout、拥有值与view图的组合或稳定热更新。原始记录与交付状态见Issues。
