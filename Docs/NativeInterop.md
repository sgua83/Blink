# C、C++ 与 Rust 集成

正式发行工具与源码SDK由GitHub Releases的Windows x64原生自解压EXE交付，双击或运维--quiet安装到%ProgramFiles%/Blink，并自动配置系统PATH；安装版读取该SDK，编译临时单元及框架输出写当前工程target。普通源码SDK构建仍沿原源码根与原target，BLEngine现有相邻源码依赖不变。安装权限、版本与完整操作统一见[版本说明](Versioning.md)和[发布安装入口](../Tools/README.md)。当前框架静态目标编译仍使用Rust MSVC／Windows构建工具；源码SDK不是运行生成应用的前置。

C++ 与 Rust 都是 Blink 的正式宿主。C/C++ 可直接链接 blink 生成的 DLL；Rust 可使用同一 DLL 的安全包装器，也可继续使用 blanguage、blcompute 原生 crate。原生 Rust 的编译、CPU 执行、GPU 驻留、异步提交和所有权接口不经过 C ABI，继续作为正式 SDK 提供。

B自身同样是系统接口的直接调用方：已有extern C声明可沿Windows x64实际ABI调用系统DLL，不要求Rust转发。接口声明以及承担类型、编码、错误和句柄寿命的B系统库具有真实职责，应复用已有实现；后续其他64位平台沿各自实际ABI建设，不因本段写成已经支持。只把相同参数再次转交系统API的Rust中转没有必要；独立且有实际价值的原生库可以保留Rust，明确其真实接口、算法和资源责任。

编译器主体由B实际构建并重建自身、调试／观察工具的主体接管，以及完整工具链脱离Rust构建分别记录，不以百分之百B或源码比例判定自举。B外壳后继续由Rust承担原核心算法不算B接管；必要系统接口封装也不能被视为无意义外壳。当前Rust运行库和资源包装的实际职责保持，不能把仍被消费的实现描述成已经退出。长期边界与具体迁移统一见[架构](Architecture.md#长期系统设计目标与语言边界)和C-032／C-034台账，不新增另一套ABI或生命周期权威。

## 宽标量、原声明布局与资源版本
u64／i64为8字节、8对齐的普通标量，C／C++对应uint64_t／int64_t，Rust对应u64／i64；u32／i32／f32／bool及原向量布局保持。外部C函数、生成算法记录和B模块按实际字段偏移交接完整宽值，不将逻辑分量数当成物理u32字数。B源代码使用u64后缀或显式数值构造，不因此获得裸指针、回调或任意地址权限。

B代码可用size_of<T>()／align_of<T>()取得原Ty值表示的usize编译期常量，例如将真实系统结构的大小显式转为u32后交给对应cb字段。大小、对齐与字段／数组步长仍取同一声明布局；float3的12字节范围与16字节对齐分别表达，不能据此假定任意C/C++同名类型兼容。dataset／view查询描述值本身，owned查询载荷表示，不表示物理分配量，也不扩大外部C可交接类型。查询不创建对象、运行释放或生成运行时反射；准确语义见[语言规范](BLanguage.md#类型值表示的编译期大小与对齐)。

原生Rust Value与模块C入口支持当前标量集合。共同Value为16字节、对齐8，标签位于偏移0、载荷位于偏移8，各活动载荷按真实宽度交接，填充不是值。当前对象／资源版本、完整标签及拒绝规则唯一见[语言规范](BLanguage.md#已编译模块的资源abi)。版本不更换Frame或复制所有权。

Rust的`Dataset::set_scalar(index,path,value)`从Dataset原Ty、实际记录下标及活动变体取得单标量字段的位置与类型，再写入完整值。嵌套路径沿既有field_at规则；路径、活动分支、标量类型或下标不符返回真实错误，错误调用保持存储。其他字段、相邻记录及ABI填充保持。此可写入口与words_mut具有相同的view绑定失效边界，实际view仍按原with_views重新绑定；不保存第二份schema或引擎参数表。

例如接口中有`struct Item { u64 identifier; i64 delta; };`，由该程序取得的相应Dataset可写：

```rust
data.set_scalar(0, "identifier", Value::U64(0xfedcba9876543210))?;
data.set_scalar(0, "delta", Value::I64(-(1i64 << 40)))?;
```

程序uniform继续按声明逻辑分量传入Value；原生C包装使用对应生成接口的类型与布局，不按裸字数猜测参数数量。CPU目标只考虑64位，当前机器后端仍是Windows x64；其他64位平台及完整模块演化、异步资源和稳定热更新分别按实际能力记录。

## CPU原生地址值

普通C调用中的address按Windows x64真实8字节指针值交接；usize／isize分别复用u64／i64，记录和输出位置保持原Ty偏移及连续位置借用。address是非拥有地址值，handle仍是不透明身份；地址形成和取位见[语言规范](BLanguage.md#原生地址值与指针宽整数)。系统返回值的有效期、可写性及释放责任由该API及实际调用者保持，不由B复制地址取得所有权。

address_of(place)取得连续CPU普通位置或CPU资源元素的真实地址，不先读取载荷。局部、字段、固定数组及连续位置参数沿原Position／Proof交接布局、权限、初始化与存储来源。unsafe_position<T>(address)直接形成原T位置；B自有来源保留原分量和只读／可写权限，普通读取、字段／固定数组写入及B／C的read、write、inout均消费同一存储，不因取址或可能写入就推定初始化。确定写入与条件输出沿原函数／模块合同交接；obj／lib不依赖提供方B实现。资源及指向它的地址可以同记录返回，拥有元数据移动不搬动载荷；局部地址或缺少相应拥有者的资源地址不能逃逸。静态释放函数的后续读取计入最后消费者，存活地址期间不得resize、迁移或释放真实存储，最后消费后由原拥有者继续操作。T仍是非空普通POD，不含Blink拥有资源或view；外部存储不登记为Frame资源，也不由位置表达式释放。外部来源的范围、对齐、权限、初始化及最后消费者前存活由调用者保证，不能与活跃B借用冲突；已知同址／重叠仍沿原静态借用规则处理。本地CPU地址不授予GPU或远程访问。

address_bits交出的普通整数不携带静态地址借用；重新unsafe_address后形成的External位置可能与原B局部存储相交。同一普通CPU函数对自身局部存储的直接读写沿原IR别名及值事实失效处理，含字段／数组及控制流；调用者仍保证实际布局、权限、初始化和存活期，数值相同不恢复拥有或只读升级许可。命名及间接C ABI调用通过原External内存效果交接同一函数内显式取址的局部存储，包含同步回调经整数userdata读写原位置；字段、固定数组和循环消费同一实际别名及事实失效。未知写入保留原来仍可能有效的地址借用来源并加入External可能性，不将可能覆盖当作确定覆盖或已初始化，已取出的普通值快照保持。普通B调用的实际内存效果已沿原函数图及obj／lib的Access合同交给调用IR，包含B包装内的同步C边界；类型位置和无固定布局的Memory效果分别表达，原存储保持真实别名。普通值调用后的旧常量／范围及循环预分析仍沿Issues C-020.2.7.2.3.2.2推进，C整数回调的完整存储及寿命范围沿.3.3；本函数局部效果不代替异步保留回调和最后消费者交接。

C ABI传入位置仍只允许在调用期间借用、不得保留。返回address的外部API只能按自己的真实外部存储合同使用，不能返回B借用地址后在调用结束继续访问来伪造外部来源。一般借用返回、需执行上下文的回调及保留寿命仍按规范中的实际缺口记录；C函数地址签名值及实际CPU调用见下节。生成算法数据接口仍按实际逻辑标量和公开布局交接载荷，地址字段的64位位模式不自动成为GPU／远端资源或延长其指向存储寿命。

## 原生C函数地址签名值

cfn<R,P...>保存真实Windows x64 C函数指针的完整8字节值；read<T>／write<T>／inout<T>描述该目标的普通位置参数，普通值参数及返回使用原C布局，void／never只作签名返回。资源／view不穿过该普通ABI。不同返回、参数类型、顺序和访问是不同签名，C实现须真实匹配；空值只表示空地址，不授予调用许可。

原生函数表与系统回调属于必要互操作边界。普通B复用采用具名函数、静态引用、泛型和特化，不以该C函数地址机制建设通用动态函数对象。存放、复制及签名检查不产生代码所有权，非零不证明外部实现或有效期；原模块／状态所有者保持到最后真实消费者。ABI和真实资源关系由已有所有者检查，不另加业务用途白名单。

unsafe_function<cfn<...>>(address)和function_address(value)仅作CPU显式签名／地址身份交接。函数值在B普通参数、返回、记录／数组、泛型和对象／归档中保留签名；普通C调用按函数指针值传参和返回，代码仍由实际DLL及原所有者保持到最后消费者。生成算法的通用数据字段按U64位载荷表示，不能据此跳过B接口签名或代码寿命责任。

记录可经函数签名自引用、互引用，签名也可按值接收或返回该记录；函数地址本体仍为8字节。签名保存已绑定的TypeName名义引用，原World完成真实布局后验证全部参数与返回，实际调用再取得完整布局，不把签名引用当成嵌入字段，也不伪造空记录。usize／isize继续与u64／i64采用相同签名身份。普通泛型可从签名返回、值／位置参数及其嵌套类型推导类型实参，保持原名义类型、访问和重复实参约束；签名内部const可从实际返回／参数中的数组形状和名义记录实参推导，嵌套签名沿原类型交接；重复值、允许值域和最终完整签名保持。B源码实际间接调用沿原CPU C ABI先保存目标、再求值实参；Proof必须已知非零，真实签名和代码寿命由外部来源负责。write／inout的实际位置、互斥借用、正常返回初始化和循环改写消费原C参数合同；签名后的requires nonzero(argument0)声明按值整数或函数指针及其字段的非零输入前提，ensures initialized(argument0,returned>=0)声明准确条件输出；argument0起按实际C参数编号，方法表中的This同样占一个位置，字段路径只交接所声明部分。原输入快照与实际返回结果进入同一输出条件图，不创建运行时初始化标志；外部实现必须履行该声明，无条件write仍承诺正常返回完整初始化。合同进入签名身份、对象与资源编码，复制、泛型和独立模块保留它。外部声明及函数值的具体合同在声明类型闭合时完整检查条件操作数与最终bool，不等到调用才发现类型错误；共用原运算类型规则，不在声明生成执行IR。声明闭合时已检查完整表达式；字面量沿原运算规则绑定真实类型和位模式，与同类型同值模块常量及const实参一致，不抹去符号、宽度、浮点精度或不同运算。f32／f64条件输出沿原Proof／Position交接同一实际谓词：命名和间接调用、普通B函数及对象／归档均使用真实输入快照、实际返回和已知常量位模式。输入或布尔条件改写后不继承旧许可；相等与不等互为否定，!(a<b)不能因NaN被当成a>=b，数值相等不代表位模式相同。不增加运行时初始化标志或浮点范围推导。头文件与实现仍在加载时匹配效果、ABI、泛型声明、参数名、访问、目标及函数自身合同；其中的函数值类型在普通具体声明或实际泛型实例闭合后，沿同一World绑定签名与数值身份再比较。只检查实际形成的实例，不将一个实例相等当成两个开放模板普遍等价；真实类型、值、运算与合同差异仍拒绝。Result与一般变体保留活动宽载荷的完整非零关系，零值、错误分支和改写后内容不继承原许可。B无隐藏Frame的C入口按下节直接交付；需执行上下文及可保留回调／上下文的最后消费者交接仍待实现；代码与资源保持原所有者。

## 查看原生CPU调用链栈需求

`bc -c ... --stack-report <report.txt>`查看本对象；`blink --stack-report <application.obj> <module.obj|library.lib>... -o <report.txt>`仅从原COFF生成已选调用图的报告，不输出PE。后者沿原独立链接器解析符号及按需成员，不读库.b或启动Cargo／rustc。模块外定义在单对象报告中保持unresolved，最终选择图才闭合；函数地址值不被误算为CALL。

Local为真实局部、保存及出参区；Known path还计入返回地址并在未报告目标处停止，不能当作完整执行上界。Upper仅在全部可达调用均被报告且无环时成立。Windows DLL、间接函数地址和原资源／计算运行库的内部栈未知；B的声明检查不证明这些目标代码的栈。递归环表示尚未得到有限上界，不断言某输入无限递归。CPU／GPU执行完成和代码寿命仍归原所有者。

可选`--stack-budget <字节数>`由真实调用者声明调用指令前、已准备入参／home区后的剩余容量，只用于报告。报告区分本帧连返回地址必需空间超过、闭合上界满足、保守上界超过但路径未证明以及未知；没有预算不作容量判定。PE reserve显示映像原容量政策，不自动代表线程剩余空间。Rust可调用`Object::stack_report(Option<u64>)`或`linker::stack_report_inputs(inputs,Option<u64>)`；当前格式与完整语义见[规范](BLanguage.md#cpu调用链的静态栈报告)。

CPU并行范围、延续及集合内核的已知局部帧尚未进入本报告，沿C-024.1.1.2继续；经过运行库的未知栈不被当作零栈。

## 编译算法，再链接为 DLL

当前目标为 Windows x64。使用源码 SDK，在 Visual Studio x64 Developer PowerShell 中构建 Blink，再执行：

```powershell
cargo build --workspace --release
.\target\release\bc.exe -c `
    sample\lessons\map.b `
    -o target\interop\map.obj `
    -I sample `
    --program scale
.\target\release\blink.exe `
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

bc 在构建时完成 B 的模块、类型、访问权限、阶段及入口检查，并用自研后端生成全部 CPU 机器码与 SPIR-V，包括集合尾部、扫描传播、排序和间接启动入口。DLL携带最终代码、重定位和执行布局，与应用包装共用resource编译映像编码；生成Rust包装只嵌入同一入口的映像字节，不重建类型／阶段对象源码。恢复映像保持原ProgramImage<P>入口类别及实际ABI，解码错误交回原FFI错误结果，不改变执行或资源所有者；不内嵌值IR。create 只装载 CPU 代码或 Vulkan pipeline；CPU 的机器码生成已移到构建时，GPU 驱动仍负责接收 SPIR-V 并建立实际管线。声明 on cpu／on gpu 的入口只生成对应后端，on compute 生成两端，无法编译的目标明确失败。

编译阶段由 bc 生成 B 算法原生代码，并使用 Rust 工具链把既有执行框架和 C ABI 编译成静态目标库，随 COFF 目标文件保存。链接阶段由 blink 读取目标文件，调用 MSVC 本机链接器生成 DLL，不调用 bc、Cargo 或 rustc，也不需要 B 源码。编译使用源码 SDK 与 Rust 工具链，链接使用 Visual Studio C++ 工具和 Windows SDK；消费已生成 DLL 的 C/C++ 项目无需安装 Rust、Blink 编译器或 Blink 源码。C 导出及生成的 C++／Rust 调用签名保持。Rust SDK 提供整程序编译映像与装载入口，数据创建接口消费只读的 ProgramInterface；已装载程序的 program() 返回不含 IR 的 ExecutionPlan，数据布局消费者使用该接口。

编译期根包装通过Cargo清单直接引用原SDK中实际需要的blanguage／blcompute包，关闭默认compiler功能；这条运行依赖不装入源码前端和对象／链接工具。运行库与包装在一次Cargo依赖图中编译，使用原target/dll及既有Release包装配置（优化级别3、lto=false、单代码生成单元、panic=abort），不单独先构建完整SDK或手工选择顶层rlib。完整源码SDK和工具的默认compiler功能、正式thin LTO配置及公开Rust编译接口保持。实际依赖由Cargo管理，首次运行角色的真实输入变化会编译必要SDK，后续新包装复用同一运行依赖产物；生成单元清理不删除这些共享产物。

公开 ABI 前缀只决定外部接口，不再作为编译暂存静态库的身份。同一 SDK 中两个相同前缀的编译各自消费本编译单元的私有库，目标文件归档后清除该单元的生成源码与 Cargo package 产物，共享框架依赖缓存保留。应用编译同样沿此路径，不再为私有 Cargo 库单独计算源码散列前缀；应用执行 DLL 的既有内容身份继续由链接器使用。编译失败如实返回并结束暂存生命周期，链接和最终 DLL 无需保留这些临时产物。

需要从原生入口构建缓存单独交付计算映像时，可用`bc image <entry.blbi> -o <deployment.blri>`或Rust `build::execution_image`取得原BLRI image字节，原BLBI构建记录与缓存保持。原运行SDK关闭compiler功能后，通过resource::from_compiler_output及原CpuTemplate／GpuTemplate装入，CPU／GPU代码、展开、布局与真实条件合同不变；不把原构建身份作为运行前置，也不把移除身份解释为取消prefix等前提。这是原执行映像交接，区别于COFF链接生成EXE／算法DLL及其C ABI工厂。compiler SDK可在原生成结果显式导出独立CPU范围材料，装入后按同代码恢复到原映像；bc code可在无源码时查询，运行角色不消费材料，详见[保存／恢复范围](BLanguage.md#保存和恢复可选cpu范围材料)。符号裁剪和正式安装仍沿原职责。完整接口与输出所有权见[部署执行映像](BLanguage.md#构建记录与可部署执行映像)。 原COFF可由`bc code --object`或`object::code_facts`直接查询物理节、符号／辅助记录、重定位及.pdata已有代码范围，不读源码或装入代码；原Object可显式捕获独立COFF范围材料，`--observations <ranges.blcf>`及object::code_facts_with_observations匹配后查询完整生成分区；无材料仍明确未分类，最终PE／内嵌归档分别交付，见[COFF查询](BLanguage.md#已保存coff的物理结构与代码查询)。

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

零尺寸普通值在B原生ABI中没有机器值／引用参数，源类型身份、借用权限与名义类型仍由原接口检查。普通C ABI当前只交接有实际字节的普通值；零尺寸字段不增加数据，包含它们的非空记录仍按原字段偏移、对齐和整体步长交接。这里的B零值不是void，亦不为C接口补虚构字段或伪造存储地址；取址支持沿原地址章节记录。

## 数据、完成与释放

- DLL接受不含资源的普通dataset和uniform：当前整数、浮点、bool、handle、现有向量／矩阵、编号枚举、带载荷变体／Result、具体泛型记录和固定数组。布局取自Ty真实字段偏移、对齐及步长；编号枚举保持其声明的8／16／32／64位有符号或无符号整数布局与标量交接，带载荷变体仍用原32位判别字。bool字段为32位整数，B写出0／1，宿主输入按零／非零解码。
- 向量、矩阵、嵌套记录、数组和结果展开为标量成员，填充与字节偏移显式保留。每个记录字段段使用 `_f<原 ASCII 长度>_<编码名称>`；名称中的 `z` 编为 `z7a`、`_` 编为 `z5f`，其余字符保持。例如字段 `a_b` 生成 `v_f3_az5fb`，路径 `a.b` 生成 `v_f1_a_f1_b`；名称中的下划线不会丢失层级身份或产生 C++ 保留的双下划线。数组下标、向量分量及结果的 tag／payload 继续使用原后缀，例如 `v_f4_tags_0`、`v_f8_position_x`、`v_f8_response_tag`；矩阵的行也保留自己的字段段。不要直接强转宿主自有结构；按生成类型填充字段。执行层只复制真实字段，不读取或改写宿主结构体的填充。
- 字段路径编码调整的是生成的 C／Rust 成员名称，原字节偏移、对齐、步长、C 导出符号及调用协议保持。重新生成记录接口后，宿主源码按对应头文件／绑定的新名称访问成员；标量别名及根向量名称保持，根数组的下标后缀保持，内部记录段遵守同一编码。已有 DLL 与配套旧绑定仍共同消费原布局，不混配不同版本生成接口。
- 每个数据指针配一个 u32 元素数，单位是完整记录。只读参数生成 const 指针／Rust 共享切片；可写参数生成可写指针／Rust 独占切片。各可写区间不得与其他参数重叠；调用期间宿主不能同时访问相应内存。
- run 是一次同步调用：准备本次存储、复制输入、执行、等待实际完成，再复制输出。GPU 路径包含上传与输出读回；DLL 不在返回后借用调用方数组。数组始终由宿主分配和释放，handle 由 DLL 创建并由 DLL 的 destroy 释放。所有 handle 销毁后才能卸载 DLL。
- create 的 backend 为 0（CPU）或 1（GPU）；目标限制与设备能力错误直接返回。相同 handle 可反复运行；调用及释放必须在创建线程串行执行。Rust 包装器以独占方法、Drop 和非 Send／Sync 类型表达这一所有权；C/C++ 遵守头文件中的相同约定。不同线程可分别创建自己的实例。
- view、纹理、加速结构和有外部 prefix 前置条件的程序不在当前 DLL 数据 ABI 中；算法目标文件编译明确拒绝，不改写成其他算法。C++ 可以调用包含内部多阶段、集合、排序及内部有效前缀的完整 计算函数。

## Rust SDK的CPU布局与view来源

`Dataset::layout()`借用原Ty；`view_bindings()`从同一原Views枚举`DatasetView { record, offset, source }`，`offset`是记录内字节，`source`是原Dataset共享借用。`view_source(record, field)`按原活动字段路径查询实际来源，支持记录／固定数组及活动变体；未知、非view、非活动字段或原绑定撤回均为None。空来源与无来源分别表达；同源多引用返回同一个Dataset，来源可以继续查询。不从header地址推导拥有关系、不克隆Arc或建立第二图；原count／stride／words／memory继续描述布局与直接存储。

返回值服从原父数据借用，不能保留到父存储释放后，不能在活动观察时改写父存储，也不授予来源写权限。CPU执行沿原同步生命周期；本接口不等待／下载GPU。完整语义见[CPU来源借用](BLanguage.md#cpu数据的布局与view来源借用)。当前CPU与GPU Rust SDK来源查询分别按原存储交付；Frame完整观察根与B／C实际入口仍沿[资源查询台账](Issues.md#c-021-7-9-22)。

## Rust SDK的CPU存储与代码区段观察

`Dataset::memory()`返回同一数据字存储的逻辑记录跨度、当前word长度和可用容量；`Frame::cpu_data_memory()`读取原当前CPU数据槽，并按查询借用中的实际Dataset分组返回原句柄与字节快照。GPU槽不参与，也不发生等待或下载。容量不等同OS承诺、工作集或分配器开销。

`cpu::CpuProgram::code_memory()`和`cpu::CpuTemplate::code_memory()`返回`Result<Vec<cpu::CpuCodeMemory>>`，按原阶段和已有代码入口同步查询。`stage`是原ExecutionPlan阶段索引，`entry_address`是该代码入口；`runtime_owned`直接表示原Executable是否持有Allocation。其余字段对应Windows当前查询：`allocation_address`为AllocationBase，`region_address`／`region_bytes`为从入口所在页开始的BaseAddress／RegionSize，`allocation_protection`／`protection`／`state`／`kind`分别为AllocationProtect／Protect／State／Type原值；失败交回原OS错误。

主代码、集合续段、Scan前缀及排序的各实际代码入口分别返回观察结果；一个阶段可以有多项。SDK自有VM及外部已链接／宿主代码保持各自原所有者，不凭VM类型推定归属。区段含页粒度和其他内容，不能当单函数机器码长度或整个模块大小；多个外部入口可能共享同一分配／区段，不能把每项字节直接相加作为独占分配。查询不保留代码、改变页保护或产生访问许可，结果只是当时元数据，地址在原所有者结束后可能复用。

当前是Rust SDK查询，未因此增加B／C观察ABI、完整资源图或全进程堆统计。详细职责与剩余范围见[CPU内存观察](Architecture.md#tool-23)。

## Rust SDK的GPU分页与分配区间观察

`vulkan::GpuDataset::memory()`及`vulkan::ReadDataset::memory()`返回`GpuDatasetMemory`，从同一原存储交付逻辑字节和各直接buffer的`GpuBufferMemory`。角色区分绑定头／页表、数据页的逻辑覆盖与实际实例指针；buffer请求大小、Range绑定偏移／大小和其所依附的VkDeviceMemory整块大小分别返回，内存类型及堆属性来自原设备。冻结别名不生成新页或分配；原`allocated_bytes()`仍为各buffer需求区间之和，不能当共享块去重总额。

返回的device／buffer／memory仅描述本次存活对象，多个buffer可以共享整块分配，多个只读别名还可以是同一个buffer。报告不保活资源、不授予读写权限，也不创建GPU工作、等待或下载；不把整块大小逐项相加为独占显存或驻留。完整字段、单位、直接buffer范围及其与Device::memory_budget的区别见[设备内存观察](DeviceMemory.md)。GPU来源由`GpuDataset::layout()`／`stride()`／`view_bindings()`直接借用，GpuDatasetView交付原record、记录内字节offset和原来源GpuDataset；same_storage比较当前同一原GpuStorage，ReadDataset沿原Deref使用这些查询。只交付实际绑定边，不读回活动tag或从静态字段路径猜活动来源。

`GpuDataset::last_use()`返回`Result<Option<GpuDatasetUse>>`，device／sequence来自本绑定原dependency的Completion，complete来自原Execution::is_complete。序号0合法；None仅无保留点，旧源点完成不能证明后续只读别名工作完成。同存储不同绑定可以持有不同点；显式wait清除本次点后仍可能看到冻结来源点。查询不等待／提交／下载；原Execution清理已完成资源、原生错误和原生命周期保持，快照不保活资源。字段和完整语义见[GPU来源与绑定保留点](BLanguage.md#gpu布局view来源与绑定保留点)。Frame完整观察根与B／C实际入口仍未交付。

## 性能的完整口径

sample/interop/fractal.* 是一百万像素的 Mandelbrot 计算，C++ 和 Rust 分别保留开启优化的串行实现，再调用同一个 DLL。每次计时只执行一次完整算法，包含 DLL 调用、本次分配、必要复制、GPU 提交／等待与输出读回。create 单独报告，不混入已复用实例的每次调用；首次只运行一回时应相加比较。重复测量使用两次预热、九次单次计时的中位数。

GPU与CPU浮点结果不保证逐位相等；分形递推会放大边界差异，消费者应按算法要求选择明确后端并观察实际数值。历史测量只在双台账记录，样例输出本次差异与耗时，不能仅凭加速比判断替换是否适合。

## B 主流程调用 C++ 或 Rust

B 的普通 CPU main、函数和 CPU 计算函数 已支持调用 Windows x64 DLL 的 C 导出。例如 `.h` 声明：

```c
extern "C" Pair add_values(Pair value, u32 amount) on cpu
    from "blink_cpp.dll";
extern "C" void scale_values(inout Pair value, u32 amount) on cpu
    from "blink_rust.dll";
```

`.b` 主流程可以取得 C++ 返回的 Pair，再将同一位置借给 Rust 修改。`as "真实符号"` 可指定与 B 函数不同的导出名称；省略时使用函数名。完整 B、C++、Rust 源码与构建部署命令见 [sample/extern](../sample/extern/README.md)。bc 生成机器码和导入重定位，blink 生成 PE 和导入表；外部 C++／Rust 工具链只构建各自的库。部署时携带实际 DLL 及其依赖，不需要 B 源码或编译器。

普通值按 Windows x64 C ABI 传递；read／write／inout 分别是实际存储的只读／完整输出／读写指针。前端保持类型、初始化和重叠借用限制。这里的普通值使用 B 的偏移、对齐、填充和数组步长；bool 使用 32 位 `uint32_t／u32`，向量是 POD 布局，不等同于 C++ 向量内建类型。C++ 的导出使用 extern C，Rust 使用 extern C 与 repr(C) 等实际匹配布局；普通 Rust ABI 和 C++ 类／模板 ABI 不属于该接口。

声明是可信 ABI 边界，DLL 必须匹配真实符号、类型布局与访问权限，指针只在调用期间有效，不得逃逸、越界或让异常／longjmp 跨边界。write参数在正常返回时满足完整或ensures initialized声明的条件输出范围。外部写入撤回旧值的静态证明；编译器不会检查 DLL 机器签名或推测失败状态对应的写入结果。只读临时值按必要生命周期保留，位置借用不维护另一份写回副本。

独立应用由 Windows 按实际导入装载库；原生 CpuProgram 用 LoadLibraryW／GetProcAddress 解析，原生代码所有者持有模块到最后一个消费者释放。装载／符号错误保留原生错误，不替换库或实现。外部调用的副作用不因返回值未使用而删除；parallel 实例的调用顺序不作保证。生成的 B 算法 DLL 也可在 CPU 后端调用其依赖库，GPU 后端明确拒绝宿主调用。

外部声明接收无泛型的on cpu普通函数和支持的普通值／void／never返回；requires nonzero可声明按值输入或字段的非零前提，ensures initialized可声明准确条件输出；其他requires／ensures仍未开放。dataset、tile、view、纹理、AS及拥有资源值不能跨普通C ABI；其合法CPU普通元素仍可借用。address值与指针宽整数按语言规范交接；外部地址位置读写与调用借用见原生地址章节；B自有位置取址及普通B／模块的来源交接见原生地址章节；无需Frame的B C回调见下节；资源型回调、完整可保留寿命协议、变参、静态对象链接及其他平台仍待实现。B应用的动态dataset和CPU／GPU组织已按规范支持，原生Rust计算API继续保留。

参考：[Microsoft DLL 使用指南](https://learn.microsoft.com/en-us/cpp/build/walkthrough-creating-and-using-a-dynamic-link-library-cpp)、[Rust extern 块](https://doc.rust-lang.org/reference/items/external-blocks.html)、[Rust 链接规则](https://doc.rust-lang.org/reference/linkage.html)。

## B代码直接作为C回调

不带from的extern "C"声明由B正文实现；c_function(Name)或c_function(Name<T,...,N>)从实际具体实例取得匹配的非零cfn，类型／u32 const按既有静态引用与实例规则绑定，泛型定义不等于C ABI含未定类型。独立模块／archive只交付已明确导出的实例，取址不在链接期展开模板；源文件及模块接口保留同一C标记与输入／输出合同；非零输入前提进入完整cfn签名并在间接调用点由实际实参证明，C／Rust调用者按真实外部接口履行前提，具体写法见[语言规范](BLanguage.md#b定义的原生c入口)。Windows x64的标量、栈参数、普通聚合返回及连续位置参数沿原C ABI交接，bool仍为4字节。符号地址来自原已编译B函数，不经Rust逐次转发，也不将普通B向量的分量指针当成C连续指针。

这类C入口没有隐藏Frame，其真实调用闭包必须无需Frame。普通B内部调用继续使用原Frame参数，C入口若直接或经辅助函数分配B资源、执行CPU／GPU程序，沿原需求闭包报B263；源语言不公开Frame身份或取得当前Frame的入口，也不按某个普通参数猜测活动运行库。根运行库与B模块资源C ABI仍通过其实际不透明Frame指针交接，但这不成为普通C函数可自行取得的权限。bc生成代码与地址重定位，blink只消费COFF／archive；原公开合同和非零关系交给调用者，链接不重读实现。需要Frame的资源型回调仍待真实代码／状态／访问／最后消费者关系设计，不另建Frame或增加宿主资源转发。

注册到Windows或其他库的函数地址及状态，必须由真实注册方保持至注销及最后一次回调完成。当前直接入口不提供动态代码／上下文保留协议或模块热卸载保证，也不表示BLEngine窗口与消息业务已迁移。平台合同见[WNDPROC](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wndproc)与[RegisterClassW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerclassw)。

## 原SPIR-V的只读结构接口

compiler Rust SDK可用`spirv::module_json(&[u32])`取得原模块全部指令、原字、类型／结果／操作数ID、入口和精确字面量位模式的独立JSON；`bc spirv <module.spv>`查询同一单文件，不读源码、创建Compiler或运行GPU。输出和临时解码归调用者，原代码与资源完成不变；格式错误通过DecodeError及原位置交接。依赖仅用于编译工具的标准数据格式解码，不是Shader编译器，普通运行角色不引入它。SPIR-V仍是中间表示；原ShaderCode／StageCode／ProgramImage的code_json及bc build的.spv.json已交接全部主／辅助模块，原bc code可无源码查询保存GPU映像，不要求CPU范围材料。SDK的可选CPU材料只配对真实CPU代码；来源与驱动最终ISA继续分别交付。精确字段和未支持范围见[语言规范](BLanguage.md#原spir-v单模块的完整结构查询)。

## Windows系统信息的原生布局

标准库`blink.windows.system`直接声明Kernel32的GetNativeSystemInfo及其SystemInfo输出记录。Windows x64真实结构的两个u16、u32、两个address、usize及后续窄／普通整数按原Ty布局交接，大小48、对齐8；接口来源、全部字段偏移与实际B消费者见[标准库](../library/README.md#blinkwindowssystem)及[系统信息应用](../sample/applications/README.md#读取windows系统信息)。纯类型与extern声明模块使用.h，不需要空.b或Rust转发包装。

本次同步write借用只在OS调用期间有效，输出仍归原B位置；系统报告的最小／最大应用地址不是新分配的资源。数量和掩码的处理器组语义遵守原Windows合同。其他系统结构、地址保留／释放、回调及引擎业务采用按各自真实接口继续建设，不把这个具体模块视为完整Windows SDK。

## Windows子进程的原生位置与完成

`blink.windows.process`的StartupInfo对应基础STARTUPINFOW：大小104、对齐8；ProcessInformation大小24、对齐8，两个handle后接两个u32 ID。CreateProcessW的startup是read位置，info是write位置，只有原i32 BOOL非零才交付已初始化输出；GetExitCodeProcess同样只在非零BOOL后交付u32 code。创建的十个参数沿原Windows x64寄存器／栈和连续记录指针ABI传递，不通过Rust或另一进程服务。具体字段及输入寿命见[标准库](../library/README.md#windows原生子进程)。

命令行是可写NUL结尾UTF-16，环境与目录按真实地址和创建标志交接；B拥有存储／借用保持到该次同步调用返回。两个输出句柄由调用者分别保持到最后真实消费者后CloseHandle，原关闭i32结果及失败OS错误继续由调用方消费；句柄复制不复制拥有权，关闭不替代进程结束。owned可沿原静态F组合，进入C ABI的仍是实际普通handle载荷。

WaitForSingleObject保留原u32状态，超时不终止工作；只有实际结束等待后，GetExitCodeProcess的u32才用作该进程的退出码。259既可能是结束前STILL_ACTIVE，也可能是已经结束进程的实际退出码，不能仅据这一值反推寿命。接口没有状态镜像、轮询、隐式取消、结果截断或错误重放。基础StartupInfo保持104字节读取；blink.windows.process_extended的StartupInfoEx为112字节／8对齐，attributes偏移104，同模块限定名CreateProcessW按完整read StartupInfoEx调用同一Kernel32导出。扩展cb使用sizeof扩展结构并设置EXTENDED_STARTUPINFO_PRESENT，不向基础前缀伪装完整扩展对象。InitializeProcThreadAttributeList的inout usize保留实际字节查询与原122错误，UpdateProcThreadAttribute以usize键／字节数和原地址交接句柄集合，Delete只销毁属性而不释放调用方存储。属性值及其实际存储保持到Delete，原始地址边界的范围、对齐、保留关系与寿命由调用方负责，不自动推导为B异步借用。SetHandleInformation直接修改原句柄继承位，原BOOL／GetLastError及拥有者保持；句柄列表只选择实际可继承的真实句柄，inherit_handles=1，不包含伪句柄。完整构建进程／I/O捕获及编译器采用继续按具体缺口建设，完整接口与寿命见上述标准库。

## Windows文件字节与同步完成

blink.windows.file以普通B实现原始字节读取、严格UTF-8输入及同步完整字节输出。原生CreateFileW／ReadFile／WriteFile／CloseHandle只交接实际Windows x64指针、u32计数与i32状态；普通库入口的overlapped为零，数据和句柄保持到真实同步返回。读写计数不能用BOOL成功代替，非零成功短写继续剩余部分，失败立即保存原OS错误；成功但0进展交NoProgress，不当成OS错误或循环等待。

read_binary复用唯一原文件句柄，read_utf8在同一读取／关闭后校验编码；成功资源归原Frame，错误不交付部分源码。write_bytes只借用CPU dataset及既有同步句柄，不替调用方关闭；write_binary／create_new_binary分别用真实CREATE_ALWAYS／CREATE_NEW，完整输出后关闭本次原句柄。原失败、此前完成前缀与关闭失败原身份分别交回，排他创建不以先查存在替代，不重试、回滚或自动重新关闭。原address调用者仍负责外部范围、权限与异步保留关系；rename在唯一名称编码入口拒绝NUL并交回UTF-16单元下标，源尚未打开／改名；完整名称长度不被原生NUL字符串解释截断。它以实际源句柄和8对齐的完整FILE_RENAME_INFO名称缓冲直接交FileRenameInfoEx，remove_file以实际文件句柄和4字节flags交FileDispositionInfoEx；操作／关闭错误共用FileChangeError，准备及外部缓冲保持原Frame。原变更已成功但关闭失败不能当作未发生，既有旧目标／文件消费者仍使用其原OS对象；不支持的信息类或文件系统返回原错误，不另改路径或分配器。更高层缓存发布、目录创建／枚举不是本接口的完成结果。精确返回类型和责任见[文件I/O标准库](../library/README.md#blinkwindowsfile)。

## B 业务结果与调用状态

B 的 `result<T,E>` 是业务数据，和生成 DLL 的 create／run 返回状态分别处理。C ABI 的0／-1、C++异常及Rust包装器Result报告装载／设备／执行／接口调用；一个合法执行可以在输出记录中产生业务Err，不因此变成DLL调用失败。B的try只传播显式业务结果，不自动捕获平台失败、外部异常或回滚已经发生的写入。

结果布局以偏移0处4字节B bool判别（零为Ok，非零为Err，B构造为0／1），载荷从两类载荷与4的最大自然对齐开始，共用最大载荷大小，整体按同一对齐补齐。生成记录中的 `*_tag`、`*_payload_序号` 及显式填充反映这个真实布局；载荷字按活动分支的原类型解释。对齐填充不作为业务值保证，宿主不能仅因总大小相同就假设字段偏移相同。原生声明仍是可信边界，宿主提供有效活动载荷，不引入动态类型对象或第二份所有权状态。

普通POD结果可用于外部C参数、返回和位置借用，也可由生成算法接口交给C/C++／Rust；按真实大小、对齐和Windows x64分类交接。

普通C结果载荷支持不含view或拥有资源的普通值及void；带载荷enum同样按实际布局交接。B内部dataset／拥有聚合结果、CPU自包含view聚合及条件输出见规范，不扩大此POD边界。结果入口适配、一般失败资源阶段和异步仍待实现。

Rust SDK的Ty明确区分记录、结果和变体类别；消费者通过当前类型构造接口及真实布局读取，不根据大小猜测类别。

## 拥有记录中的普通字段位置

B拥有聚合中的已初始化普通字段可按read／write／inout借给外部C函数；local_slots按Ty字节偏移保留自然对齐及填充。外部写入撤回旧内容事实，借用只在真实调用期间有效，不保留指针或建立资源所有者。B内部整份拥有聚合write／inout已实现，普通C仍不交接整份拥有资源。

## 嵌套表达式退出与外部效果

普通函数的try错误退出可发生在C ABI实参的数值表达式或实际位置索引内；该路径不再求后续实参、不调用外部函数，也不交付相应write初始化。此前已发生的外部效果保留。正常路径仍传递原位置、遵守既有布局及借用，真实错误返回仍满足声明的完整或条件输出契约；没有新增调用状态、ABI包装或回滚。

## 统一函数与算法产物

源函数统一使用返回类型定义，整域计算可写 void 和显式 parallel；--program 仍选择生成算法对象的公开具体函数。内部 Program／ExecutionPlan 与生成的 C/C++／Rust ABI 名称保持原职责。该产物以借用数据及普通统一值为C接口；CPU普通B函数已有独立原生调用、递归、模块对象与普通位置借用，但这不等于普通函数的通用C资源ABI已经完成。完整资源阶段合同、借用返回、版本演化与热更新仍在研发。只有类型／外部接口的.h可以独立存在，实际DLL符号与布局责任不变。B的target为同步执行位置选择，低层SDK异步提交仍使用原完成对象。

## 普通B借用与C指针布局
普通B向量借用按原分量地址通过B模块调用，允许离散及重排；记录／矩阵／数组保留声明布局。这个B函数ABI与外部C的单个连续指针分别定义，不能把B逻辑向量的位置直接当作C连续对象。B包装函数把符号向量区域交给C时，在原接口写`requires contiguous(value)`或选中的字段／swizzle；调用点按实际位置静态满足，库导出该条件，递归和多层调用承接相同声明。直接且布局已知的C位置继续沿原规则，不增加条件许可。

contiguous包含对应类型的顺序、全部字段偏移及对齐。例如uint2需要8字节对齐；uint4的xy／zw满足，xz／yx／yz分别因离散／重排／对齐不能传给该C接口。C仍只收到同一实际存储的一个指针；不复制载荷、不生成运行时布局检查或另一份写回状态。标量子位置可以直接转交，普通B函数的合法重排不受C的表示要求限制。具体语法见BLanguage.md的“普通位置借用与contiguous”。

C只读参数不得读取尚未初始化的B位置；B／C实际写入按原Position交接初始化范围。外部write合同由可信导出实现履行，编译器不检查DLL内部机器码。

## 固定拥有数组的B模块交接
普通CPU应用可通过独立B目标文件及归档按值传递／返回固定拥有数组，元素可为dataset、拥有记录／Result或嵌套数组；资源位置与部分移动沿原资源树和模块合同交接。同一Frame持有真实资源，数组元素布局仍按Ty的字节偏移、对齐与步长组织。动态元素的dataset借用可以调用已编译CPU函数和CPU／GPU整域入口；实际候选来源仅参与编译期别名和存活期检查，不进入C函数签名或运行时资源表。

这些拥有值按B模块合同交接，普通C ABI不接收拥有数组、view图或任意资源载荷。C++／Rust普通值接口、原生Rust宿主API与合法普通字段借用分别遵守各自边界。

## 一般带载荷变体的布局与模块交接
普通带载荷enum与原result使用共同的判别字／重叠载荷布局；声明的u32／i32编号、活动载荷、自然对齐及最大存储共同组成真实ABI。生成的C++／Rust绑定按实际偏移导出tag与payload字，超过四个字的载荷仍按实际布局生成名称，不作为向量分量解释。原生接口的声明者／调用者须交付符合声明类型的活动值；编译器自身的分配及B源码写入沿原初始化关系维持该事实，不以零存储代替任意分支构造。

普通带载荷enum可以作为不含资源的POD跨C接口；含拥有资源的变体、数组及Result按B模块合同交接，同一Frame持有实际资源，不扩大普通C资源ABI。

对象与资源版本按本文开头的唯一规范引用处理；链接只消费已有代码和合同。完整资源阶段、动态拥有元素容器、任意初始化覆盖、一般引用返回、版本迁移和热更新继续开放。


### B模块的CPU借用view返回

CPU应用以view<T>(dataset位置)取得现有CPU存储的不可变借用。普通view、含view记录／数组、返回及write／inout值输出沿模块合同交接真实来源和条件。view为16字节／对齐8，Frame和原dataset持有实际存储；借用不得越过最后消费者，局部存储及临时实参不能逃逸。

CPU最后使用期、循环及自递归／互递归的view来源在原函数图内闭合后导出，调用点代入实际参数及输出；无正常来源的纯递归环报B271。CPU自包含拥有聚合和宿主持久图见相应章节，不扩大extern C／算法DLL或完整异步借用。


源模块可以通过源闭包推导跨模块互递归的合同，再分别生成各自的对象及静态库；没有实际目标文件依赖的模块编译使用这一源模式。提供产物依赖后按原接口导入，不因某个函数未提供目标文件而读取现存库.b。模块所有者只交付自己的代码，外部模块的私有泛型／helper不会变成额外导入；Frame需要沿完整函数关系形成后再裁剪代码。

公开合同保持的私有计算可只重编所属模块，再重链原调用对象；接口或来源合同改变则重新编译实际依赖者，不以静态重链宣称稳定热更新。


普通CPU函数的拥有记录、固定数组、变体／Result及嵌套组合现可通过read参数跨B模块只读借用。原生调用交接已有Ty布局的元数据和句柄，原Frame仍持有实际存储；同源只读参数可以重合，源资源及实际元数据位置的独占冲突继续在原编译所有者处拒绝。临时资源复用原实参作用域，view返回保留实际输入来源，借用变体的匹配不取得拥有权。

read借用不转移拥有权；B模块的整份拥有聚合write／inout和view组合按后节当前规则使用。普通C资源ABI及稳定热更新仍未实现。

### view载荷与拥有聚合的模块合同
独立B对象／归档交接enum／Result中的view及CPU拥有聚合内外来源。资源来源归Data资源叶，view来源归类型化值Proof；按声明类型的资源叶顺序编码，不能与变体重叠机器槽混用身份。不增加机器参数、运行时注册或复制资源。

活动view期间仍拒绝来源写入、移动或传输；普通CPU返回条件可在真实分支中精化可能来源，不按来源候选集合猜测独立。

CPU自包含拥有view聚合、整份拥有聚合write／inout及固定数组动态位置均按规范支持。持久dataset图的B构造、应用内GPU图、动态拥有元素容器、完整资源阶段、异步／远端与版本迁移仍待实现；普通extern C和算法DLL保持各自边界。

### 宿主持久变体view图
原生Rust的Dataset::for_view、with_views／freeze及Device::upload／upload_with_views／download现可交接enum／Result及记录／固定数组中的实际活动view和间接子图。字段路径包含声明的分支名称，例如item.Some、entries[0].Some.rows及answer.Ok；声明路径只确定类型，实际绑定同时消费记录中的真实判别字。无view分支不要求子资源，未激活载荷不绑定，重叠的数值载荷不被地址覆盖。

实际类型没有view字段时，Dataset::with_views与Device::upload_with_views接受空绑定，保持原数据、记录数及声明的字节步长；u8／u16窄元素和零字节类型不被扩大为u32记录，也不产生子资源。非空绑定仍须对应实际活动view字段，字段数量、类型、记录范围和重复绑定继续按同一资源合同检查。

原Views继续持有实际共享子资源，bindings保存实际排序字偏移及原source索引，不增加图注册、活动表或Frame。CPU／GPU地址仅在真实引用位置编码，分页上传和下载沿同一表交接；显式传输及真实完成仍归原运行库。编译器候选字段与宿主活动字段均来自Ty，机器view和普通C／算法DLL边界保持。

普通extern C／算法DLL不传递持久资源图；CPU自包含拥有聚合使用B模块合同，持久dataset图的B构造与GPU应用内图仍未实现。

## 拥有聚合的独占引用与模块输出

B模块接口现可交接普通CPU拥有记录、固定数组、变体／Result及嵌套组合的write／inout。调用传递原Reference／Indexed位置，运行期直接读写该聚合的实际元数据；载荷及完成仍由原Frame持有。独占期间可以移动或替换拥有内容，正常及显式错误返回须归还完整输出；write在实参求值后、实际调用前释放旧内容，入口不能读取旧状态。

returned和written共用类型化Returned合同，普通事实、资源初始化、驻留及view来源一起导出。view可引用仍在实际独占输出中存活的新资源；出口与公共编号沿原图交接，自递归／互递归和泛型不会丢失该来源。局部存储和临时实参仍不能借返回值逃逸。引用容器及其交还内容不由被调用函数在退出时释放，真实覆盖和最后消费者完成后的释放仍归原所有者。

written和返回合同按真实类型输出，不编码私有位置；链接只消费已编译产物，不读取B实现或生成代码。CPU自包含拥有view聚合按规范支持，临时／局部来源仍不得逃逸。

B内部合同不开放普通extern C的整份拥有资源接口。完整资源阶段、动态拥有元素容器、任意初始化覆盖、异步／远端和版本迁移继续开放。

## 不透明原生句柄
源语言`handle`已提供原生不透明身份，当前Windows x64实际大小／对齐均为8字节。C绑定使用`void *`，Rust绑定使用`*mut core::ffi::c_void`；记录和固定数组中的句柄叶保持声明偏移及对齐，不向B公开uint2载荷。普通B函数和独立B模块也保持精确类型身份及完整位值。

`handle()`产生空值，`handle(value)`复制同类型身份，`==`／`!=`返回完整身份比较的单个bool。`unsafe_handle(usize_value)`直接解释完整64位外部身份，接受编译期常量或动态usize／u64，保持原类型布局及ABI；不经过address、读取存储或取得对象权限。普通数值构造、取位、分量访问及指针解引用仍不开放。句柄不是语言自动管理的外部资源拥有者：原调用者仍须遵守真实API的使用／完成／关闭合同，复制句柄不复制对象，不会隐式Close或登记另一份资源权威。

普通外部声明仍是信任边界；必须按实际C签名及输出语义声明。`write handle`承诺每条正常返回完整初始化，可能失败且保留旧输出的系统API应使用已初始化的inout位置或其真实接口合同，不能因正常C返回就假定成功写出句柄。

真实句柄由对应资源所有者在最后消费者完成后关闭，GPU仅消费身份值，不访问外部OS对象。owned<T,F>可表达CPU静态释放，B自有取址及静态释放F的地址消费者保留原资源寿命；需执行上下文的回调及保留寿命、通用拥有字符串、借用字符切片及完整系统库仍未实现，不外推为完整窗口或COM支持。


## 用户拥有包装与原生互操作

CPU的`owned<T,F>`把载荷T与静态释放函数F作为B类型关系交给原普通函数／模块机制；它不存放运行时回调地址，不能据此声明C函数地址签名值、实际间接调用或系统回调已经实现。F可以在准确的普通外部C声明上执行真实关闭操作，也可以是资源库中的普通B函数。handle本身仍是可复制身份；包装的移动、into_inner及退出清理见语言规范“CPU用户拥有值与静态释放”。

拥有包装仍不进入普通C ABI；C函数可按其真实合同消费已合法取得的普通载荷／字段。复制身份或函数返回不是外部关闭成功证明。独立B对象／归档需在.h声明其静态释放函数，调用者不读取库.b即可在正确出口调用；Frame及物理存储继续由根运行库唯一解释、持有和释放。


## 共同Frame C ABI的错误与所有权

根运行库唯一创建、解释和销毁Frame；外部C／C++／Rust及非根模块只交接不透明指针。以下是BLRI22的实际Windows x64资源入口，区别于生成算法DLL自身的create／run／destroy接口。该ABI身份同时涵盖执行映像中CPU主函数末端与展开字节的编码；共同Frame入口使用下列C签名。类型与程序编号来自本次已编译模块工厂及实际绑定，不能按另一映像猜测；Binding和Value完整标签／布局见[已编译模块的资源ABI](BLanguage.md#已编译模块的资源abi)。

```c
typedef struct BlinkFrame Frame;
typedef void (*BlinkErrorReceiver)(void *context,const uint8_t *data,size_t bytes);
typedef struct {uint32_t handle;uint32_t offset;} BlinkDatasetBinding;
typedef struct {uint32_t kind;uint64_t bits;} BlinkValue;
Frame *blink_create(BlinkErrorReceiver on_error,void *context);
uint32_t blink_allocate(Frame *frame,uint32_t ty,uint32_t count,
                       uint32_t *output,BlinkErrorReceiver on_error,void *context);
uint32_t blink_resize(Frame *frame,uint32_t handle,uint32_t count,const uint32_t *fill,
                     uint32_t *output,BlinkErrorReceiver on_error,void *context);
uint32_t blink_run(Frame *frame,uint32_t program,const BlinkDatasetBinding *data,
                  const BlinkValue *values,BlinkErrorReceiver on_error,void *context);
uint32_t blink_upload(Frame *frame,uint32_t handle,BlinkErrorReceiver on_error,void *context);
uint32_t blink_download(Frame *frame,uint32_t handle,BlinkErrorReceiver on_error,void *context);
uint32_t blink_address(Frame *frame,uint32_t handle,uint64_t byte_offset,uint32_t write_bytes,
                      uint32_t **output,BlinkErrorReceiver on_error,void *context);
void blink_release(Frame *frame,uint32_t handle);
void blink_destroy(Frame *frame);
```

六个返回uint32_t的操作成功为1、原Result错误为0，只有成功才写output；调用者不能在错误后读取失败输出。失败在返回前调用本次on_error一次，借出原UTF-8消息及不含NUL的字节数；消息仅到接收函数返回前有效。接收函数和context保持到本次调用返回，运行库不存入Frame或登记到全局状态，保留文本副本由调用者负责，异常不得跨C ABI展开。实际指针、完整参数数组、类型／程序身份及独占访问仍按可信原生边界履行，不以状态返回证明任意无效指针安全。

共同操作不打印、结束宿主或替宿主释放Frame；失败后的原存储／程序和已发生效果继续按原Rust操作合同交接，不回滚或自动重放，不承诺设备故障后数据可用。宿主选择后续合法调用、继续持有或显式销毁。release释放指定槽位，destroy结束根Frame；创建解码错误仍在同次接收函数交回原文本后返回NULL，没有可销毁的Frame。

生成B应用的私有适配消费上述状态，成功才读取输出；错误由应用接收者报告，应用根显式销毁同一Frame并退出1。非根模块调用同一根政策，只借用opaque Frame，不建立第二释放链。此终端政策属于该B应用，完整普通B可失败返回／业务result适配仍沿Issues C-020.5.2.4建设。算法DLL及原Rust宿主API继续遵守各自现行错误与生命周期接口，不能混用这些入口。

## CPU拥有dataset的显式变长

源语言语义以[语言规范同名章节](BLanguage.md#cpu拥有dataset的显式变长)为准。资源运行库的Windows x64 C入口为`uint32_t blink_resize(Frame *frame,uint32_t handle,uint32_t count,const uint32_t *fill,uint32_t *output,BlinkErrorReceiver on_error,void *context)`，成功返回1并把同一Frame槽位的实际句柄写入output；错误返回0、不写output，并在本次调用借出原错误文本，完整合同见[共同Frame C ABI](#共同frame-c-abi的错误与所有权)；Frame保持不透明，只由根运行库创建、解释和最终释放。fill在本次同步调用内按存储Ty的实际声明布局借用完整普通值，必须不含拥有资源或view；运行库只读取声明分量，不读取未初始化填充。调用者必须拥有可调整的CPU存储，先结束真实地址／view／位置消费者；该原生可信入口不替调用者验证任意外部指针。

非根模块通过同一不透明Frame调用共同入口，不移动Rust私有容器或解释Frame。普通C值接口不因此接收B拥有dataset；address值不代替指向位置访问；动态拥有元素、异步和完整版本迁移仍待实现。

## 普通值序列库与独立实例

`blink.sequence`用普通B接口交接拥有dataset及Result，源码使用与实例范围见[库说明](../library/README.md)。通过`bc -c --module --instance NAME<T>`导出实际实例，`blink --archive`生成标准COFF静态库；消费方用`--import`装入真实合同。没有导出的类型实例明确拒绝，不在对象消费或链接阶段读取库.b、重新生成模板或调用Rust编译器。

模块合同传递无符号上界距离、不回绕和及拼接偏移关系；数值事实与资源版本分开。普通C不接收拥有容器，库不建立第二Frame或独立运行时容器。

缺失的具体实例明确拒绝；一般资源阶段、版本迁移及二进制泛型模板分发仍未实现。

## 输入快照的精确常量偏移

输入快照的非回绕常量偏移沿原模块输出重命名及调用绑定交接，常量距离直接写入合同，不依赖库私有寄存器。一般while不变式仍未实现；链接不读取B实现或重新生成代码。

## 宽整数与不透明身份的交接

u64／i64普通值以真实8字节大小／对齐进入Windows x64调用和生成C++／Rust记录；handle仍按原void *／不透明指针身份交接。两种类型可以在同一普通聚合中保持各自声明偏移，不能用整数位宽替代句柄权限。外部声明继续信任准确C签名及实际初始化／借用合同，不引入对象模型、隐藏副本或跨边界Rust容器。

Value按当前规范标签和完整载荷宽度交接，资源不匹配由原ResourceBinding和链接入口拒绝。Frame、显式传输和最后消费者仍由原所有者持有，subgroup按实际设备能力执行。


## f64与原生double ABI

f64标量按Windows x64的C double／Rust f64传参和返回；仅真实浮点标量使用浮点参数／结果ABI，普通含f64记录仍按自身大小及布局分类。混合参数、第四个之后的栈实参、8字节记录与较大间接记录沿原调用槽及实际拥有者交接，不建立写回镜像。

生成C／C++和Rust算法记录分别使用double与f64，uniform及Dataset::set_scalar使用Value::F64。Value完整标签和布局集中于规范；类型化数学入口及Float64设备要求随实际编译产物交接，不靠布局猜测兼容。

SDK可显式选择具有所需能力的物理设备；原C绑定的backend=GPU仍使用现有默认设备，缺失Float64或64位浮点控制时返回真实错误。CPU、GPU、外部C／Rust、对象／归档及实际消费者采用分别记录，未实现的GPU f64超越函数、完整JSON／容器迁移及版本演化不由本节宣称完成。

## 打包存储的逻辑记录数量

Rust入口Dataset::from_words(program,binding,count,words)接收原生产者的实际记录数量及一份Vec<u32>，按值接收原存储，不复制数据。物理字数须等于ceil(stride*count/4)；末字中的非记录填充保持原字节，不从整字长度反推数量，也不要求填充为零。步长、类型来自原编译接口，逻辑数量存入原Dataset.count，不建立计数或布局副本。复制已有Dataset时交接其count；图像等外部生产者交接自己的实际元素数量。

本入口的Rust源调用须传递count；普通C、资源ABI、数据布局与GPU最后消费者规则保持。输入物理存储与声明数量不符仍报真实输入错误，不以截断、补齐、兼容包装或猜测数量接收。

## Windows高精度计时的原生输出

`blink.windows.time`以纯`.h`直接声明Kernel32的`QueryPerformanceCounter`与`QueryPerformanceFrequency`。Windows `BOOL`按`i32`原值返回，`LARGE_INTEGER`输出按大小8／对齐8的`i64`位置借用；`ensures initialized(output,returned != 0)`只在本次非零状态交付输出。接口不将可能失败的输出伪装为无条件初始化，也不增加包装DLL、运行时时钟服务或第二份输出。

计数是ticks，频率是ticks／秒；调用者决定错误处理与区间换算，原位置在调用期间借出，OS返回后仍归原调用者。签名、用法、时间语义与官方合同见[标准库计时接口](../library/README.md#blinkwindowstime)。它不提供日历时间或GPU计时，也不表示BLEngine的Rust时间消费者已迁移。

## Windows线程错误码的原生交接

`blink.windows.error`以纯`.h`直接声明Kernel32的`u32 GetLastError()`与`void SetLastError(u32 code)`，Windows DWORD按完整32位无符号值交接。唯一可变错误槽属于原OS线程，库不创建全局last-error、宿主转发或清除策略；读取得到的普通u32快照由原调用者保存和使用。

调用者只在原API规定的返回条件下立即读取，必须先于打印、释放或其他可能改写线程错误的调用。清零同样由真实API合同决定，不自动包住所有系统入口。模块不提供错误字符串或通用错误对象，不替应用退出进程；完整用法与官方合同见[标准库错误码接口](../library/README.md#blinkwindowserror)。B接口可用和BLEngine原Rust错误处理已迁移分别记录。

## Windows当前进程的命令行输入

`blink.windows.process::command_line() on cpu`以普通B函数交回独立拥有的`dataset<u16>`。私有GetCommandLineW只在真实Windows x64 C ABI上取得OS管理的地址，B按原unsafe_position<u16>同步只读至首个NUL，复制实际代码单元，不修改／释放系统字符串或将其地址／view交给调用者。结果不含终止符，不解析参数、不验证或替换孤立代理单元；需要其他编码由原Unicode库显式处理。

返回资源和模块调用沿原Frame、拥有结果及现行对象／资源ABI交接，没有新增C ABI或运行期注册。独立模块可在提供者.b缺席时编译调用方并链接运行。完整签名、实例用法、OS存储及程序名的合同见[标准库进程输入](../library/README.md#blinkwindowsprocess)；同模块parse_command_line／arguments已以B实现Windows参数序列：只读原始代码单元，返回独立拥有的Arguments{units,ranges}，参数范围索引自己的结果存储，不返回OS地址或依赖输入快照继续存活；argv[0]、连续引号、反斜线奇偶及空参数规则见上述标准库文档。按名字环境查询／写入、基础及扩展进程控制的当前接口与实际所有权见标准库对应章节；编译器CLI及Rust构建路径的B采用仍沿C-032推进。

## Windows环境块的原地址与最后消费者

`blink.windows.environment`直接声明`address GetEnvironmentStringsW()`与`i32 FreeEnvironmentStringsW(address block)`，分别保留LPWCH与BOOL的真实Windows x64 C ABI。成功取得的是本次只读UTF-16环境快照，B可用原unsafe_position逐条读取；本次调用方保持原地址及所有读取者，最后消费者结束后匹配释放一次，Free失败状态及其立即GetLastError由调用者处理。同模块GetEnvironmentVariableW保持address／address／u32→u32的真实C ABI，容量与返回值分别按原含NUL必要空间／不含NUL实际长度解释；直接调用者保持实际名称及缓冲区。普通B函数variable(address name)以原API交回result<dataset<u16>,VariableError>，成功只含实际代码单元，未定义203、空值、原系统失败及容量不足不混同；不足时不消费未定义输出或重新执行。返回资源沿原模块资源C ABI和Frame交接，不把外部环境快照变成B资源。SetEnvironmentVariableW保持address／address→i32的原LPCWSTR／可选LPCWSTR／BOOL接口：零value地址删除，NUL空串地址设置空值，非零成功／零失败及立即GetLastError由原调用方消费。OS只复制本次调用的输入，不持有调用方地址；原存储可在返回后修改／释放，旧快照和B拥有读取结果不被回写。本次进程写入不修改其他既有进程或注册表环境，不封装为额外状态或自动重放。

外部存储不进入Frame，address复制、游标推进和函数返回不改变原拥有者或延长释放后的有效期。模块不包装为可自动销毁的B字符串，不建立全局环境副本、释放队列或第二运行库。纯.h与其已编译对象／归档可直接消费，无需空.b；完整原型、枚举示例及官方合同见[环境接口](../library/README.md#blinkwindowsenvironment)。环境按名字查询／写入、B拥有文本集合及实际引擎／编译器采用分别建设。

## Windows当前进程标准句柄

blink.windows.process::GetStdHandle以原u32 DWORD选择值交给Kernel32，返回原8字节handle；三个STD选择值、失败的全一身份及没有标准句柄时的零值分别保持。返回的身份借自进程表，不因复制成为B拥有资源；标准端点的替换／关闭与余下读写仍归原所有者。ReadFile／WriteFile继续交接实际字节范围、i32状态、字节数和OS错误，缓冲只在本次真实同步调用完成后重用。具体借用及EOF语义见[标准句柄接口](../library/README.md#当前进程的标准句柄)。
