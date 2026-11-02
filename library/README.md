# Blink 标准库

`library`是正式B模块的源码根。应用以`-I library`装入，用普通泛型、模块和资源规则编译；库代码不反向依赖BLEngine。

## blink.sequence

导入`blink.sequence`后，序列直接使用`dataset<T>`，空序列直接构造`dataset<T>(0)`，清空直接调用`truncate(data,0)`。当前元素为可复制且不含拥有资源或view的普通存储值，包括标量、向量、矩阵、记录、固定数组和带载荷变体。原dataset长度表示逻辑项数，底层Vec容量仍由Frame／blcompute拥有；截断允许后续复用容量，最终释放沿原资源生命周期。

| 函数 | 输入与结果 |
|---|---|
| `get(read data,index)` | `Result<T,SequenceIndexError>`；错误含实际index和length |
| `set(inout data,index,value)` | `Result<void,SequenceIndexError>`；错误时内容不变 |
| `push(data,value)` | 返回新增末项的拥有dataset，长度溢出时返回原输入 |
| `pop(data)` | 返回`SequenceRemoved<T>{data,value}`；空输入返回原数据及Empty |
| `insert(data,index,value)` | 允许`index <= length`；返回新序列或原数据及错误 |
| `remove(data,index)` | 允许`index < length`；返回删除项和新序列，非法下标返回原数据 |
| `extend(data,read tail)` | 拼接尾序列；tail仍归调用者，长度溢出返回原data |
| `truncate(data,length)` | 保留前length项；length不小时保持原长度，不增长 |

拥有修改函数消费输入，Result成功载荷交回新数据；错误载荷`SequenceFailure<T>`交回原`data`及`reason`。原因是`Empty`、`IndexOutOfRange`、`LengthOverflow`；`remove`空输入属于非法下标。分配或执行失败按原运行库处理，不属于这些库错误。全部操作要求CPU数据，GPU数据先显式download；get只读借用、set借用内容可写，其他修改需要拥有权。真实游标、view及释放函数消费者仍须在移动／调整前结束，extend的尾输入不能与被消费的输入别名。

```b
module main;
import blink.sequence;

u32 main() on cpu {
    dataset<u32> data=dataset<u32>(0);
    match (push(data,17)) {
        Ok(next) { data=next; }
        Err(failure) { data=failure.data; return 1; }
    }
    match (pop(data)) {
        Ok(removed) { println(removed.value); data=removed.data; }
        Err(failure) { data=failure.data; return 1; }
    }
    return 0;
}
```

配套`main.h`声明`module main; u32 main() on cpu;`。在工程根使用正式工具：

```powershell
target/release/bc.exe -c main.b -I library -o target/main.obj
target/release/blink.exe target/main.obj -o target/main.exe
target/main.exe
```

也可用`blink.sequence::push`等限定名。开放泛型按装入的B源码实例化；独立对象／归档只导出明确请求的实例，例如：

```powershell
target/release/bc.exe -c library/blink/sequence.b -I library --module --instance 'push<u32>' -o target/sequence.obj
target/release/blink.exe --archive target/sequence.obj -o target/sequence.lib
```

实际应用须导出其使用的各个实例，并通过`--import target/sequence.lib`编译，再与该库链接。当前对象与资源ABI版本见[语言规范](../Docs/BLanguage.md#已编译模块的资源abi)；缺失实例拒绝，链接不重读B源码。动态拥有元素、递归容器、持久view图、完整字符串、借用字符切片和一般while回边不变式继续待实现；本库交付不等于BLEngine业务已迁移。

## blink.unicode

`blink.unicode`以纯B实现严格UTF-8／UTF-16标量编解码、标量长度、拥有切片和整段转换。以`import blink.unicode;`导入，源码根仍为`-I library`。标量接口可在普通CPU函数和CPU并行域调用；拥有输出的切片与整段转换属于普通CPU资源组织。

| 函数 | 成功与失败 |
|---|---|
| `decode_utf8(read dataset<u8> text,u32 offset)` | `Result<DecodedScalar,DecodeError>`；offset及next按字节计 |
| `decode_utf16(read dataset<u16> text,u32 offset)` | 同一Result；offset及next按u16代码单元计 |
| `scalar_length_utf8(read dataset<u8> text)` | `Result<u32,DecodeError>`；严格扫描整个UTF-8输入，交回Unicode标量数 |
| `scalar_length_utf16(read dataset<u16> text)` | 同一Result；合法UTF-16代理对计一个标量 |
| `slice_utf8(read dataset<u8> text,u32 start,u32 end)` | `Result<dataset<u8>,SliceError>`；按Unicode标量半开区间[start,end)交回独立拥有UTF-8序列 |
| `slice_utf16(read dataset<u16> text,u32 start,u32 end)` | 同一错误类型，成功为拥有dataset<u16>；代理对不会被拆开 |
| `encode_utf8(u32 value)` | `Result<Utf8Encoding,u32>`；成功为One(u8)、Two(array<u8,2>)、Three(array<u8,3>)或Four(array<u8,4>) |
| `encode_utf16(u32 value)` | `Result<Utf16Encoding,u32>`；成功为One(u16)或Two(array<u16,2>) |
| `to_utf16(read dataset<u8> text)` | `Result<dataset<u16>,TextError>`；严格解码整个UTF-8输入，Ok交回拥有UTF-16序列 |
| `to_utf8(read dataset<u16> text)` | `Result<dataset<u8>,TextError>`；严格解码整个UTF-16输入，Ok交回拥有UTF-8字节序列 |

标量长度接口只读借用原CPU序列，以原decoder交回的next前进；空输入返回Ok(0)，输入内容、长度与拥有者保持，不分配文本副本或保存计数缓存。NUL和BOM各计一个标量，组合标记独立计数；补充平面字符在UTF-8的四字节或UTF-16的代理对均计一个标量。它们不报告字节数、UTF-16代码单元数或用户感知的字素簇数，不规范化或合并字符。第一个非法编码原样返回DecodeError的绝对index和reason，不交付部分长度、不替换或跳过错误；已有dataset.length仍表示该序列的单元数。

拥有切片要求`start <= end <= 标量长度`，区间按Unicode标量计数，输出保留原代码单元。合法空区间（含文本末尾）返回空拥有序列。NUL、BOM及组合标记仍分别计一个标量，不按字素簇切片；不追加NUL或变换编码。两个函数严格解码**整个输入**，第一个编码错误优先返回`SliceError::Decode(DecodeError)`，即使错误在所选区间之外或请求区间本身非法；位置与原因仍以原输入为准。完整输入合法后，反向或越界区间返回`SliceError::Range(ScalarRangeError)`，其中start／end保留原请求，length为实际标量数。半开端点避免以start加count计算末尾；空文本仅[0,0)成功。

输入只读借用，成功输出是独立拥有dataset：修改或释放其中一份不改变另一份。实现沿原decoder的next选择原始代码单元，由resize增长同一输出的原Vec容量；不重新编码、不保存边界表、计数缓存或另一资源所有者。输出单元数不超过输入，沿原u32长度，不增加溢出错误分支或人为上限。解码或范围失败时不交付部分输出，本次存储由原Frame退出释放；分配／执行失败仍遵循原运行库。切片不返回指向输入的view，不能据此认为借用文本切片或完整字符串类型已经实现。

`DecodedScalar`含`value`与第一个尚未消费位置`next`。解码只借用原CPU数据，调用者在Ok分支自行采用next，输入和原offset仍由调用者持有。编码成功载荷的变体决定实际单位数；Err保留原非法u32值。UTF-16接口交接逻辑u16单位，字节序列化由实际输入／输出接口处理。

`DecodeError`含`index`与`DecodeReason`。offset不小于length时为EndOfInput，index保留所请求offset；合法前缀后缺单位为Truncated，index是第一个缺失位置。InvalidLead及InvalidContinuation指向实际非法首／续字节；InvalidSequence指向违反UTF-8最短形式、代理区间或最大标量限制的第二字节。UTF-16孤立低代理或错误代理对用InvalidSurrogate，分别定位该低代理或高代理后不合法的第二单位。

整段转换从offset零顺序扫描至输入末尾，空输入返回空拥有序列。`TextError::Decode(DecodeError)`保留第一个解码错误及其原输入中的绝对位置；`TextError::Sequence(SequenceError)`保留序列操作的原原因，当前push的预期失败为u32输出长度溢出。失败时本次部分输出沿原Frame退出释放；输入仍归调用者，只有Ok交接完整输出。存储增长复用`blink.sequence`、resize及原Vec容量，长度与释放继续只有原所有者；分配／执行失败按原运行库语义处理。

合法标量为U+0000至U+10FFFF，排除U+D800至U+DFFF。NUL、BOM、未分配码点及noncharacter原样保留，输出长度按实际代码单元计。接口不追加终止符、不剥离BOM、不做替换、规范化、大小写折叠或错误后的自动跳过；UTF-8拒绝非最短形式、代理编码及越界序列，UTF-16严格按高／低代理配对。规则来自[Unicode编码形式](https://www.unicode.org/versions/Unicode18.0.0/core-spec/chapter-3/)。

```b
import blink.unicode;

result<dataset<u8>,TextError> roundtrip(read dataset<u8> text) on cpu {
    dataset<u16> wide=try(to_utf16(text));
    return to_utf8(wide);
}
```

同名.h按原模块规则声明上述函数；wide在调用期间只读借用，返回后沿原退出释放。独立模块按实际依赖分别编译序列和Unicode，再组成标准COFF静态库：

```powershell
target/release/bc.exe -c library/blink/sequence.b -I library --module --instance 'push<u8>' --instance 'push<u16>' -o target/sequence.obj
target/release/bc.exe -c library/blink/unicode.b -I library --module --import target/sequence.obj -o target/unicode.obj
target/release/blink.exe --archive target/unicode.obj target/sequence.obj -o target/unicode.lib
```

调用方用`--import target/unicode.lib`编译，并与同一库链接；使用分立对象时同时导入并链接unicode.obj及sequence.obj。两份提供者.b无需进入调用方编译或链接，公开.h继续提供接口。标量公开输入检查与转换共用同一私有编码载荷生产者；转换只将成功解码的标量交给它，不复制编码算法或另造宿主执行路径。

完整文本值、借用字符切片、源码字符串字面量和OS字符串地址／生命周期仍待实现；本模块不表示BLEngine JSON、窗口或IME业务已经迁移。

## blink.windows.system

`blink.windows.system`是Windows x64的原生系统信息接口。它只有一份`.h`，公开普通记录`SystemInfo`和`extern "C" void GetNativeSystemInfo(write SystemInfo info) on cpu`；直接调用Kernel32的同名函数，没有B转发实现或常驻服务。记录未初始化时可作为这个完整write输出；同步返回后记录仍由原B调用者持有。

```b
import blink.windows.system;

SystemInfo info;
GetNativeSystemInfo(info);
println(info.page_size);
println(info.number_of_processors);
```

这些语句写在普通CPU函数内。`number_of_processors`及`active_processor_mask`描述当前处理器组；不据此推算所有处理器组的总量。实际数值随系统而变。

| B字段 | Windows SYSTEM_INFO成员 | x64字节偏移／类型 |
|---|---|---|
| processor_architecture | wProcessorArchitecture | 0／u16 |
| reserved | wReserved | 2／u16 |
| page_size | dwPageSize | 4／u32 |
| minimum_application_address | lpMinimumApplicationAddress | 8／address |
| maximum_application_address | lpMaximumApplicationAddress | 16／address |
| active_processor_mask | dwActiveProcessorMask | 24／usize |
| number_of_processors | dwNumberOfProcessors | 32／u32 |
| processor_type | dwProcessorType | 36／u32 |
| allocation_granularity | dwAllocationGranularity | 40／u32 |
| processor_level | wProcessorLevel | 44／u16 |
| processor_revision | wProcessorRevision | 46／u16 |

该记录大小48、对齐8；第一个原生union使用官方建议的架构分支。`reserved`没有应用语义，`processor_type`是原生ABI保留的过时成员；判断架构使用`processor_architecture`。地址字段是系统报告的应用地址空间边界，不拥有所指存储；取`address_bits`只观察完整64位地址，不授予访问或释放权限。API返回void，信息由输出记录交付。

完整B程序与构建命令见[系统信息应用](../sample/applications/README.md#读取windows系统信息)。当前验证目标为Windows x64；模块接口不表示其他64位平台或完整Windows系统库已经交付。正式源码发行包沿既有library目录包含此模块。

原生合同来源：[SYSTEM_INFO](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ns-sysinfoapi-system_info)、[GetNativeSystemInfo](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getnativesysteminfo)。

## blink.windows.time

Windows x64的高精度区间计时接口只有`time.h`，直接导入Kernel32的原生函数，无B转发实现：

| 函数 | 输出与单位 | 返回及初始化 |
|---|---|---|
| `QueryPerformanceCounter(write i64 count)` | 当前计数，单位为ticks | 原生`i32`状态非零时count已初始化 |
| `QueryPerformanceFrequency(write i64 frequency)` | 每秒计数，单位为ticks／秒 | 原生`i32`状态非零时frequency已初始化 |

两个入口按Windows x64 C ABI交接`BOOL`与`LARGE_INTEGER`；B分别使用32位有符号状态和64位有符号计数。调用者先检查本次状态，再读取对应输出；接口保留原始状态，不在库中退出进程或改写单位。输出由调用者的原普通位置持有，借用仅持续到同步OS调用返回。纯接口模块不需要空`.b`。

在导入`blink.windows.time`的普通CPU main中可写：

```c
i64 frequency;
if (QueryPerformanceFrequency(frequency) == 0) { return 1; }
i64 begin;
if (QueryPerformanceCounter(begin) == 0) { return 1; }
// 在这里执行需要计时的CPU工作。
i64 end;
if (QueryPerformanceCounter(end) == 0) { return 1; }
f64 seconds = f64(end - begin) / f64(frequency);
if (!println(seconds)) { return 2; }
```

先在整数域求计数差，再转换为f64并除以实际频率，避免先转换两个大绝对计数造成精度损失。Windows保证同次系统启动的频率固定且跨处理器一致；本模块不持有频率缓存，应用可按自身生命周期保存返回值。QPC适合测量本机时间区间，不是UTC／日历时间，不能直接比较不同机器的绝对计数，也不表示GPU实际执行时间。具体GPU完成仍由原设备／执行消费者负责。

官方合同：[QueryPerformanceCounter](https://learn.microsoft.com/en-us/windows/win32/api/profileapi/nf-profileapi-queryperformancecounter)、[QueryPerformanceFrequency](https://learn.microsoft.com/en-us/windows/win32/api/profileapi/nf-profileapi-queryperformancefrequency)、[高精度时间戳](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)。日历时间、错误文本与调用上下文、跨平台时间抽象及BLEngine计时职责的实际迁移仍未交付；原线程错误码接口见下节。

## blink.windows.error

Windows x64的`blink.windows.error`只有`error.h`，通过`-I library`装入，直接按原生C ABI调用Kernel32：

| 函数 | 原生合同 |
|---|---|
| `u32 GetLastError()` | 读取本调用线程的32位last-error原值 |
| `void SetLastError(u32 code)` | 将本调用线程的last-error设为code，没有返回值 |

错误槽由Windows按线程持有；B模块不缓存、不复制为全局状态，也不包装系统调用。不同线程的错误码互不覆盖。只有原API文档说明的返回条件才赋予该值本次调用的错误意义；有些成功调用也会改写它，未声明设置错误的函数不能据此判断成功或失败。

在原API返回表明应读取错误后，立即调用GetLastError，再打印、关闭资源或调用其他可能改写线程错误的函数。取得的u32是原调用者可保存、返回或放入自身业务结果的普通快照。SetLastError(0)只在实际API合同需要先清除错误时由调用者明确使用；设置或清零错误槽不执行、重放或证明任何系统工作成功。

```b
import blink.windows.error;

// 在原API规定读取last-error的失败分支中，先保存，再报告。
u32 code = GetLastError();
println(code);
```

这些语句放在普通CPU函数中。模块不提供错误文本、自动Result转换或进程退出政策；外部句柄、缓冲区及其最后消费者仍归原调用者。其他平台和BLEngine窗口／等待等Rust职责的实际迁移继续按对应问题推进。

官方合同：[GetLastError](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-getlasterror)、[SetLastError](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-setlasterror)。

## blink.windows.process

`command_line() on cpu`返回`dataset<u16>`，内容是本次Windows进程的原始命令行，不含末尾NUL。以`import blink.windows.process;`和`-I library`使用；返回值是调用者独立拥有的CPU序列，按UTF-16代码单元计数。引号、空格、制表、反斜线、BOM、补充平面字符的代理对和孤立代理单元原样保留，不按参数拆分、去引号、规范化或替换编码。需要严格UTF-8时由调用者显式使用`blink.unicode::to_utf8`并处理其真实解码结果；命令行代码单元不被自动认定为合法Unicode标量序列。

```b
module main;
import blink.windows.process;

u32 main() on cpu {
    dataset<u16> line=command_line();
    println(line.length);
    return 0;
}
```

配套`main.h`声明`module main; u32 main() on cpu;`。上例输出UTF-16代码单元数量，不是参数个数。原存储由Windows进程管理，库只同步读取、不修改或释放；纯B实现沿原resize增长同一输出Vec，最终由Frame及调用者的实际资源寿命释放。两次调用取得独立拥有序列，修改或释放一份不改变另一份或OS原存储；没有全局命令行缓存、外部地址泄漏或第二份资源权威。分配／执行失败仍遵循原运行库处理，GetCommandLineW本身没有定义额外失败状态，本接口不猜空指针错误或添加长度限制。

独立对象与静态库仍沿正式阶段交付：

```powershell
target/release/bc.exe -c library/blink/windows/process.b -I library --module -o target/process.obj
target/release/blink.exe --archive target/process.obj -o target/process.lib
target/release/bc.exe -c main.b -I library --import target/process.lib -o target/main.obj
target/release/blink.exe target/main.obj target/process.lib -o target/main.exe
```

调用方以`--import`读取归档中的实际模块接口，提供者.h／.b可以不在交付机器上；原资源C ABI、Frame及对象／归档选择保持。程序名采用OS实际提供的形式，可能与启动器传入名称不同；启动器传空命令行也不保证进程收到空串。平台合同见[GetCommandLineW](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-getcommandlinew)及[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)。Windows参数序列接口见下文；本模块的原生子进程接口见下节；编译器CLI及原std::process构建工作尚未迁移，继续沿C-032推进。

### Windows参数序列

同一模块的`parse_command_line(read dataset<u16> line) on cpu`以B解析不含终止NUL的原始命令行内容，返回`Arguments`；`arguments() on cpu`读取本次进程快照，再调用同一解析器。`command_line()`仍只交回原始代码单元。参数库不调用Rust解析、Shell32或命令解释器，不展开环境变量、通配符或shell符号。

```b
struct ArgumentRange { u32 start; u32 length; };
struct Arguments { dataset<u16> units; dataset<ArgumentRange> ranges; };
```

`units`按参数顺序持有去引号／转义后的代码单元，`ranges`逐项保存同一存储中的start与length。第0项是argv[0]；合法空参数的length为0，可以与后一项共用起点。范围不是指针或view，不持有OS字符串地址；调用者保持这两份数据至最后实际使用，最终释放仍归原Frame。解析先统计代码单元与参数数目，再精确分配两份结果并写入，不为每个字符扩容。解析器只读借用line，返回不引用它，修改或释放原快照不改变结果；两次调用的拥有结果互相独立。当前输入与序列长度沿既有u32 dataset范围，不增设字符／参数数量上限；分配／执行失败仍遵循原运行库处理，未改成业务Result。

规则采用[Microsoft C参数解析](https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments)：argv[0]单独处理，引号改变是否把空格／制表当作结束，反斜线在此不转义引号；其余参数在引号外由空格或制表分隔。引号前连续反斜线按奇偶交接，其他反斜线原样保留；引号内的两个连续引号产生一个字面引号。未闭合引号把已读内容交给最后参数，`""`保留空参数，换行／回车及其他代码单元不是分隔符。前导空白形成空argv[0]；纯解析空序列也保留一个空argv[0]，不查询或补入可执行文件名。实际`arguments()`只消费OS提供的快照，启动器的空lpCommandLine不保证该快照为空。

只解析上述ASCII语法单元，不验证／替换代理单元、不改变大小写或Unicode规范形；需要严格Unicode转换时由调用者显式调用原Unicode库。现有编译器Rust CLI尚未接入这些B函数，完整文本值、一般拥有容器、参数选项业务及其他平台仍按各自台账推进。

```b
module main;
import blink.windows.process;
import blink.sequence;

u32 main() on cpu {
    Arguments args=arguments();
    println(args.ranges.length);
    for (range in args.ranges) {
        println(range.length);
        for (index in 0..range.length) {
            match(get(args.units,range.start+index)) {
                Ok(unit) { println(unit); }
                Err(error) { println(error.index);return 1; }
            }
        }
    }
    return 0;
}
```

配套main.h沿上节，入口仍为u32 main() on cpu。示例先输出含argv[0]的参数数目，再逐项输出长度与代码单元，适用于空参数及未配对代理。使用归档接口时，两个实际模块依赖都需编译、导入并链接：

```powershell
target/release/bc.exe -c library/blink/windows/process.b -I library --module -o target/process.obj
target/release/blink.exe --archive target/process.obj -o target/process.lib
target/release/bc.exe -c library/blink/sequence.b -I library --module --instance 'get<u16>' -o target/sequence-u16.obj
target/release/blink.exe --archive target/sequence-u16.obj -o target/sequence-u16.lib
target/release/bc.exe -c main.b -I . --import target/process.lib --import target/sequence-u16.lib -o target/main.obj
target/release/blink.exe target/main.obj target/process.lib target/sequence-u16.lib -o target/main.exe
```

调用方通过归档接口取得两个模块，不再补读提供方.h／.b；返回结构、资源ABI及编译／链接职责保持。

### Windows原生子进程

本模块直接声明Kernel32入口，不经过命令解释器或Rust转发；创建、等待、查询与关闭分别交接原OS结果：

| 入口 | 返回及输出 |
|---|---|
| `CreateProcessW` | i32 BOOL，非零时初始化`ProcessInformation`；零时立即读取`blink.windows.error::GetLastError()` |
| `WaitForSingleObject` | 原u32等待状态；进程`WAIT_OBJECT_0`表示已结束，`WAIT_TIMEOUT`保持原工作，`WAIT_FAILED`后立即保存OS错误 |
| `GetExitCodeProcess` | i32 BOOL，非零时初始化write u32退出状态；失败时输出不可读 |
| `CloseHandle` | 原i32 BOOL，调用者消费实际关闭结果；失败后立即保存OS错误 |
| `SetHandleInformation` | 原i32 BOOL，按mask修改原句柄flags；HANDLE_FLAG_INHERIT为继承位，设置和清除都保持原拥有权 |

`StartupInfo`按基础STARTUPINFOW布局保存cb、三个UTF-16地址、八个u32、两个u16、reserved2地址及三个标准句柄；Windows x64大小104、对齐8，std_input／std_output／std_error偏移80／88／96。cb由`u32(size_of<StartupInfo>())`取得，其余字段按实际标志初始化，保留字段遵守OS合同。`ProcessInformation`为24字节、对齐8，process／thread两个句柄偏移0／8，process_id／thread_id两个u32偏移16／20。read／write沿同一声明位置与C指针ABI，不复制OS对象。

application、可写command_line、可选安全属性、environment与directory均为原CPU地址；字符串必须是实际NUL结尾UTF-16，完整输入在同步创建期间保持。命令行允许OS原地修改，不能传只读文本；程序选择、引用、路径搜索、句柄继承和权限由调用者按真实平台合同决定。environment零地址继承本次父环境；显式UTF-16块使用`CREATE_UNICODE_ENVIRONMENT`并按OS要求排序／双NUL结束，双NUL空块与继承不同。directory零地址继承父目录。`CREATE_NO_WINDOW`和`STARTF_USESTDHANDLES`直接交接对应平台标志，不自动改写标志或重建输入。

成功创建只交回原进程／主线程句柄与ID，不表示初始化或业务已成功，也不隐式等待。两个句柄各有一份实际关闭责任；数值复制不产生新拥有者。调用方保持句柄到最后等待／查询／用户结束，再各关闭一次；`CloseHandle`本身不终止对应进程或线程。可按原`owned<T,F>`组合明确静态释放，但库不提供丢失CloseHandle结果的自动关闭包装，不增加状态表、后台监控或释放链。

`INFINITE`为0xffffffff，零等待立即观察本次状态；原等待函数亦可服务其他合法可等待对象，`WAIT_ABANDONED`等原状态不改写为进程完成。退出查询在进程未结束时可返回`STILL_ACTIVE`（259），真实等待完成后查询到259则仍是该进程的真实退出码。完整u32状态不截断，不把非零业务退出码改成创建失败，不以魔数循环判断进程寿命。等待期间不得关闭对应句柄。平台依据：[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)、[等待](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)、[退出查询](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess)及[关闭句柄](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-closehandle)。

已编译消费者须提供实际模块依赖：process.obj／lib交付本模块的完整接口和普通B函数，依赖它的模块与应用分别用`--import`导入，并把对应对象／归档交给blink链接；不能只交桥接模块而漏掉其process依赖。基础记录只能交给基础创建接口；扩展结构及选择性继承由下节的准确入口交接，不向基础记录交付扩展启动标志。更高层命令构造、I/O捕获、异步任务及编译器实际迁移仍按原具体Issues推进。

## blink.windows.process_extended

本模块是纯接口，导入blink.windows.process的原结构与句柄规则；无需空.b。`StartupInfoEx { StartupInfo startup; address attributes; }`对应STARTUPINFOEXW，Windows x64大小112、对齐8，attributes偏移104。使用`u32(size_of<StartupInfoEx>())`设置startup.cb，creation_flags包含`EXTENDED_STARTUPINFO_PRESENT`。模块内CreateProcessW以read StartupInfoEx读取完整扩展结构，以原BOOL非零保证write ProcessInformation初始化；基础模块的同名函数保持read StartupInfo。两者都是同一个Kernel32原生导出，B用原模块限定名区分，例如`blink.windows.process_extended::CreateProcessW(...)`，没有包装函数、动态分发或截断指针。

`InitializeProcThreadAttributeList(address attributes,u32 count,u32 flags,inout usize bytes)`交接不透明属性列表。先以零attributes和flags=0查询实际字节数；该查询按OS合同返回零，并由GetLastError及bytes交付ERROR_INSUFFICIENT_BUFFER（122）和所需空间，不是重试或初始化成功。调用方分配实际足量、正确对齐的存储，再以该地址和容量执行一次初始化；原i32 BOOL和即时错误分别保留。

`UpdateProcThreadAttribute(address attributes,u32 flags,usize attribute,address value,usize bytes,address previous_value,address return_size)`的flags为0，后两个保留地址为零。`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`为原指针宽整数键；value指向实际handle数组，bytes为该数组的字节数。列表中的每个真实句柄必须可继承，创建的inherit_handles必须为1；不得列入伪句柄。调用方可通过基础模块SetHandleInformation及HANDLE_FLAG_INHERIT设置／清除原句柄的继承位。清除继承位与设置本次子进程的继承列表是不同操作；并行创建各自以实际列表选择自己的集合，不能用继承全部句柄代替。

属性值及其存储必须保持到`DeleteProcThreadAttributeList(address attributes)`；已初始化的列表存储保持到Delete结束，然后仍由原分配者释放。Delete返回void，只删除列表内部属性，不释放调用方缓冲、不关闭列表中的句柄或进程。创建后可删除该次列表，父进程中的输入／输出句柄按最后实际消费者各自关闭；子进程实际继承的句柄是它自己的OS引用。原始address互操作不为外部API所保留的地址自动建立B借用，调用方负责真实范围、对齐、权限和上述寿命；不能在最后原生消费者前移动、resize或释放存储。没有第二份拥有表、隐式关闭、等待或取消。

纯头接口可直接作为模块编译输入，不需要空实现：

```powershell
bc -c library/blink/windows/process.b -I library --module -o target/process.obj
blink --archive target/process.obj -o target/process.lib
bc -c library/blink/windows/process_extended.h --import target/process.lib --module -o target/process_extended.obj
blink --archive target/process_extended.obj -o target/process_extended.lib
```

独立模块及最终应用分别以--import导入实际接口依赖，链接时交付对应对象／归档；仅有桥接模块不代替process.lib与process_extended.lib。消费这些已编译接口不需要提供者.h／.b或library搜索路径。此处提供原生启动能力，编译器Rust构建代码尚未迁移。依据：[STARTUPINFOEXW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-startupinfoexw)、[初始化](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-initializeprocthreadattributelist)、[更新属性及值寿命](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)、[删除列表](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-deleteprocthreadattributelist)及[句柄属性](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-sethandleinformation)。

## blink.windows.environment

本模块直接交接Windows x64的环境原生接口，`environment.b`以普通B函数提供单个变量的拥有值读取。`GetEnvironmentStringsW()`返回`address`，零地址表示未取得快照；成功后可按UTF-16代码单元读取NUL分隔的条目，空条目结束。它提供本次进程环境的独立快照，条目及其顺序、空值、隐藏驱动器项和原始代码单元由OS提供；枚举时不按字符标量验证、大小写归一化、过滤或另建环境镜像。

本次调用方负责保持快照至最后实际读取者结束，然后把**取得时的原始地址**交给`FreeEnvironmentStringsW(address block)`一次。不能把逐项推进的游标、其他分配器地址或原命令行地址交给它；不得修改快照、重复释放或在释放后读取任何地址副本。原OS快照不进入B的Frame拥有树，复制address不新增拥有权；接口不自动释放或替应用决定进程寿命。已取得快照可以和后续进程环境变化分开使用。

释放入口按原Windows BOOL返回`i32`，非零表示成功、零表示失败，状态不被吞掉或改成B bool。失败时按原API约定紧接读取`blink.windows.error::GetLastError()`，先于打印或其他系统调用；Getter只按其官方合同声明NULL失败，不虚构额外错误码或初始化保证。两个接口不增加重试、释放队列、空指针探测、块长度上限或隐式状态。

下面的普通B程序枚举条目数，最后读取完成后释放原块：

```b
module main;
import blink.windows.environment;
import blink.windows.error;

u32 main() on cpu {
    address block=GetEnvironmentStringsW();
    if (block == address()) { return 1; }
    usize cursor=address_bits(block);
    usize count=0u64;
    u16 unit=unsafe_position<u16>(unsafe_address(cursor));
    while (unit != u16(0)) {
        while (unit != u16(0)) {
            cursor+=2;
            unit=unsafe_position<u16>(unsafe_address(cursor));
        }
        count+=1u64; cursor+=2;
        unit=unsafe_position<u16>(unsafe_address(cursor));
    }
    i32 released=FreeEnvironmentStringsW(block);
    if (released == 0) {
        u32 error=GetLastError();
        println(error); return 1;
    }
    println(count);
    return 0;
}
```

配套`main.h`声明`module main; u32 main() on cpu;`，使用`bc -c main.b -I library -o target/main.obj`再独立链接。这里的unsafe_position只解释OS已提供且仍存活的实际CPU地址；不能借此访问GPU／远端资源。Getter失败时不读空地址；成功后原块一直保持至枚举结束，释放前的cursor不取代原block身份。

纯声明模块也可交付独立对象／静态库：

```powershell
target/release/bc.exe -c library/blink/windows/environment.b -I library --module -o target/environment.obj
target/release/blink.exe --archive target/environment.obj -o target/environment.lib
target/release/bc.exe -c main.b -I library --import target/environment.lib -o target/main.obj
target/release/blink.exe target/main.obj target/environment.lib -o target/main.exe
```

提供方environment.h／environment.b不可读取时，调用方仍可通过已编译对象／归档消费原生声明与普通B读取函数；编译和链接保持各自职责。完整B文本集合及编译器／BLEngine实际迁移继续按对应台账建设。平台合同：[取得快照](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-getenvironmentstringsw)、[匹配释放](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-freeenvironmentstringsw)、[环境块格式](https://learn.microsoft.com/en-us/windows/win32/procthread/environment-variables)。

### 按名称读取拥有值

`result<dataset<u16>,VariableError> variable(address name) on cpu`读取当前进程的指定变量。name由调用者保持为本次真实调用期间有效、NUL结尾的UTF-16名称；原OS负责按名查找，B不改名称大小写或另建环境镜像。成功交回独立、唯一拥有的UTF-16代码单元，不含终止NUL，允许零长度及孤立代理单元，不转码或替换内容；返回后资源仍归原Frame及真实调用方。

`VariableError::System(code)`交回零返回时立即取得的原32位Windows错误码；未定义变量保留`ERROR_ENVVAR_NOT_FOUND=203`，与成功的空值分别表达。函数先向GetEnvironmentVariableW查询含NUL所需容量，再分配一次输出并读取一次；长度查询的零返回直接失败，实际读取前用SetLastError(0)区分零长度成功和失败，不保存／恢复旧错误状态。

两次调用之间进程环境可以变化。若实际读取报告缓冲区不足，`VariableError::BufferTooSmall(required)`保留本次原API报告的、含NUL的必要容量；该分支的缓冲内容未定义，函数不会返回它、截断或重新读取。若变为空值、删除、缩短或等长替换，交回实际读取的值或错误。接口不承诺原子环境快照，不添加固定大小上限；需要独立快照时使用原GetEnvironmentStringsW及匹配释放。

底层`u32 GetEnvironmentVariableW(address name,address buffer,u32 capacity)`同时保留原C ABI：capacity按UTF-16代码单元计且包含NUL空间，成功返回实际长度且不含NUL，空间不足返回含NUL所需容量。调用方直接使用该入口时自行保持名称／缓冲区、权限及其实际最后消费者。具体返回合同见[GetEnvironmentVariableW](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-getenvironmentvariablew)。编译器CLI采用是独立迁移工作，不能由本库交付代替。

### 设置与删除当前进程变量

`i32 SetEnvironmentVariableW(address name,address value) on cpu`直接保留Windows x64的LPCWSTR／LPCWSTR／BOOL接口。name是本次调用期间实际有效、NUL结尾的UTF-16名称；value为零地址时删除变量，非零地址指向NUL空串时设置空值，其他内容按原UTF-16代码单元写入。名称查找及合法性由OS处理，不改变B的名称大小写规则，不加入固定长度限制或复制环境状态。

调用方消费原i32返回：非零成功，零失败后立即从blink.windows.error::GetLastError取得原u32错误码；成功不解释残留错误。接口没有另一层B包装、错误转换或自动重放。OS在同步调用期间复制输入，返回后调用方可继续修改、释放自己的名称／值存储；输入地址不被登记为环境所有者，原Frame与OS各保持自己的资源。

后续查询及新取得的环境快照观察本次进程的新环境；已经取得的独立环境快照和B拥有读取结果保持原值。后续子进程在使用默认继承环境时获得创建时的环境，显式环境块由实际创建者决定；本调用不更新既有其他进程、用户／机器注册表环境或旧快照。子进程基础／扩展创建、等待及句柄的现行接口见本标准库process及process_extended；编译器原Rust生产者采用仍沿C-032推进。平台合同见[SetEnvironmentVariableW](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-setenvironmentvariablew)。

## blink.windows.file

本模块以普通B代码提供Windows x64文件I/O，使用`import blink.windows.file;`和`-I library`。路径为调用期间有效、NUL结尾的UTF-16地址，名称、相对基准与代码单元由调用者和OS解释；库不改大小写、不规范化路径、不建立文件缓存或第二份来源。

### 读取原始字节或UTF-8

`result<dataset<u8>,SourceError> read_binary(address path) on cpu`以GENERIC_READ、OPEN_EXISTING和读／写／删除共享打开，沿原读取循环连续至真实EOF，再关闭原句柄，交回唯一拥有的原始字节。所有字节包括NUL与非法UTF-8均保持，不追加终止符、转码或验证编码；空文件返回空dataset。

`read_utf8(address path)`保持同一返回类型，直接复用read_binary的打开／读取／关闭结果，再由Unicode库严格校验整个内容。BOM、NUL、换行及非ASCII原字节保持，合法标量可以跨读取块；解码失败不返回部分源码。打开共享允许其他进程修改内容，两入口都不承诺原子文件快照，固定本次模块输入仍归原装入器。

| SourceError载荷 | 实际含义 |
|---|---|
| `Open(u32)` | CreateFileW返回INVALID_HANDLE_VALUE后立即取得的原OS错误码 |
| `Read(ReadError)` | 原读取错误或现有序列的LengthOverflow；原句柄已关闭 |
| `Close(CloseError)` | 内容读取完成但关闭失败，保留原handle及code，不宣称已释放 |
| `ReadAndClose(ReadCloseError)` | 同时保留原读取错误与关闭的原句柄／错误码 |
| `Decode(DecodeError)` | 仅read_utf8在原句柄关闭后交回首个非法UTF-8绝对字节位置／原因 |

ReadError继续为System(u32)或Sequence(SequenceError)，分配与执行错误沿原Frame／运行库处理，不规定另一文件大小上限。成功dataset归原Frame及最后实际消费者，不保留文件句柄或路径借用；关闭失败不自动再次关闭或重读。按DecodeReason或SequenceError判断时直接导入blink.unicode或blink.sequence。

### 同步完整写入与文件创建

`result<u32,WriteError> write_bytes(address file,read dataset<u8> data) on cpu`在调用方已有的同步可写句柄上完整输出CPU字节，成功返回data.length。空数据返回0而不发起零长度WriteFile；原句柄、文件位置及关闭仍归调用方。每次WriteFile只写当前剩余部分，只有非零成功进展才推进原游标；成功短写继续其未写尾部，失败立即返回，不重放已完成前缀或重试失败。

WriteError::System(WriteSystemError{completed,code})保留此前成功完成的前缀长度及该次失败后立即保存的原GetLastError；失败调用可能发生部分效果，completed不是失败回滚或完整文件长度。WriteError::NoProgress(u32)保留成功返回但写入数为0时此前完成的前缀：它是完整写入未完成的库结果，不伪造OS错误、等待、重试或声称成功。Win32允许成功短写，例如非阻塞字节管道；同步库不会用BOOL非零代替实际完成数。

`result<u32,OutputError> write_binary(address path,read dataset<u8> data) on cpu`以GENERIC_WRITE、CREATE_ALWAYS和读／写／删除共享打开，完整写入后关闭。目标存在则截断，空数据同样创建／截断为0字节。`create_new_binary`参数和返回类型相同，使用CREATE_NEW，已有目标即原错误80，不先查询、不覆盖或自动换名；空数据也保持排他创建。两入口复用同一打开／写入／关闭实现。

| OutputError载荷 | 实际含义 |
|---|---|
| `Open(u32)` | 原生创建失败后的即时OS错误；未获得本次句柄 |
| `Write(WriteError)` | 原写入未完成，原句柄已关闭，目标可能已改变 |
| `Close(CloseError)` | 写入已完成但关闭失败；保留本次原handle与code |
| `WriteAndClose(WriteCloseError)` | 同时保留实际write错误及close句柄／错误 |

源dataset只读借用，字节不经文本校验；在同步调用返回前不得修改、resize、迁移或释放载荷。已打开文件的句柄由同一调用保持到真实写入结束，关闭只执行一次；关闭失败由结果交回真实身份，不隐藏再关或复制拥有权。输出不承诺事务、原子替换、失败恢复或磁盘持久化；部分写入和截断不会自动撤销，最终产物发布仍归原消费者。

底层CreateFileW、ReadFile、WriteFile和CloseHandle直接使用Windows C ABI。ReadFile／WriteFile的计数为原write u32输出；直接原生调用者按实际同步／异步模式负责缓冲、句柄、位置及最后完成，不能把原生address接口当成自动建立的异步B借用。普通库入口仅以零overlapped作同步I/O。平台合同见[CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)、[ReadFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-readfile)、[WriteFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-writefile)及[CloseHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-closehandle)。

独立模块先交付实际泛型实例，再分别编译其消费者：

```powershell
target/release/bc.exe -c library/blink/sequence.b -I library --module --instance 'push<u8>' --instance 'push<u16>' --instance 'extend<u8>' -o target/sequence.obj
target/release/blink.exe --archive target/sequence.obj -o target/sequence.lib
target/release/bc.exe -c library/blink/unicode.b -I library --module --import target/sequence.lib -o target/unicode.obj
target/release/blink.exe --archive target/unicode.obj -o target/unicode.lib
target/release/bc.exe -c library/blink/windows/file.b -I library --module --import target/sequence.lib --import target/unicode.lib -o target/file.obj
target/release/blink.exe --archive target/file.obj -o target/file.lib
```

调用方分别--import这三份实际归档并链接，不需要提供者.h／.b或-I；也可以按同一正式archive入口将实际对象组合成一个库。开放sequence声明不等于已交付其所有类型实例，未导出的实例仍按原规则拒绝；链接不重新编译源码。编译器Rust装入、对象输出和缓存发布尚未迁移；目录创建／枚举、同平台替换及删除继续按Issues各自具体子项推进。

## blink.windows.path

`result<bool,ExistsError> try_exists(address path) on cpu`查询Windows x64目标是否存在。path由调用者在同步调用期间保持为有效、NUL结尾的UTF-16路径；文件、目录、原名称和相对基准由OS解释。接口跟随实际符号链接或junction目标：目标不存在及中间路径不存在返回Ok(false)，断开的链接也返回false，不能由链接条目自身存在推定目标存在。

实现通过一次零desired-access的CreateFileW元数据打开，允许读／写／删除共享，以FILE_FLAG_BACKUP_SEMANTICS打开目录且不使用FILE_FLAG_OPEN_REPARSE_POINT。打开成功后关闭本次原句柄再交回Ok(true)；不要求内容可读，不保存路径缓存、存在性镜像或目录状态，也不保证后续读取时目标仍未改变。只有原OS错误2／3交接为真实不存在；非法名称、访问失败、链接环等其他结果均保留原错误，不能被压成false。

`ExistsError::Query(u32)`保留打开失败后立即取得的原GetLastError。关闭失败交回`ExistsError::Close(ExistsCloseError)`，其中handle是本次原句柄、code是原OS错误；结果不宣称该句柄已释放，不自动再次关闭或重新查询。正常结果不把任何句柄或路径借用留给调用者。公开接口不依赖文件内容、Unicode或序列模块，查询所需的C ABI声明留在私有实现。

模块以`import blink.windows.path;`及`-I library`装入，也可将path.b编译为模块并归档；仅消费try_exists的调用方只需path.lib，无需提供者.h／.b、文件内容库、序列代码或运行库sidecar。绝对路径的实际代码依赖见下节。当前Rust编译器Inputs::file尚未采用；规范路径接口见下节，扩展名词法替换见后节，路径组合接口见后节。平台调用见[CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)，当前真实消费者语义见[Path::try_exists](https://doc.rust-lang.org/std/path/struct.Path.html#method.try_exists)。

### 取得绝对路径

`result<dataset<u16>,AbsoluteError> absolute(address path) on cpu`交回独立拥有的Windows绝对路径UTF-16代码单元，不含终止NUL。输入仍由调用者在同步调用期间保持为有效、NUL结尾的UTF-16路径。输出由原Frame及实际调用者持有，返回后不保留输入地址；原输入可以修改、截断或释放。函数不查询目标是否存在，不打开文件、不解析符号链接，也不验证／替换Unicode代理单元。

普通Windows路径由GetFullPathNameW解释本次进程当前目录、盘符相对路径、根相对路径、UNC及点目录语义。先查询含NUL的必要容量，再取得一次实际结果并按不含NUL的返回长度交接；原OS零返回立即保存GetLastError。进程当前目录是OS共享状态，两次调用间可能改变：若本次结果需要更大容量，BufferTooSmall保留该次含NUL的必要容量，不返回部分输出或重新查询；本函数不保存当前目录镜像或承诺原子快照。调用者取得结果后应沿该实际绝对路径继续本次工作。

精确的反斜线verbatim前缀`\\?\`按平台语义原样交回拥有副本，不再送入普通Win32路径展开；盘符、UNC、点目录、分隔符、尾部点／空格和UTF-16代码单元均不改写。前缀识别按实际NUL顺序读取，短输入不会为识别前缀读取终止符以后的单元。拥有副本通过原sequence::push增长，不建立文本运行时、路径缓存或第二资源所有者。

| AbsoluteError载荷 | 实际含义 |
|---|---|
| `System(u32)` | GetFullPathNameW失败后立即取得的原OS错误码；空路径按原OS结果失败 |
| `BufferTooSmall(u32)` | 本次取得结果所需容量，包含终止NUL；原缓冲内容不作为结果交付 |
| `LengthOverflow()` | verbatim副本增长超过当前dataset可表达长度，沿原sequence::push交接；不是另一个路径长度阈值 |

绝对路径的归档消费编译时导入path.lib，链接时同时提供实际使用的`push<u16>`序列实例；在absolute中它服务verbatim拥有副本，不成为纯值try_exists调用的链接前置。提供者源码不可读取时仍按已编译接口组合，根Frame和最后消费者规则保持。当前Rust构建图尚未采用本B函数；规范路径接口见下节，扩展名词法替换见后节，路径组合接口见后节。平台规则见[GetFullPathNameW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfullpathnamew)及[本次真实Rust消费者的绝对路径实现](https://github.com/rust-lang/rust/blob/1.97.1/library/std/src/sys/path/windows.rs#L169)。

### 取得规范路径身份

`result<dataset<u16>,CanonicalError> canonicalize(address path) on cpu`通过真实文件／目录句柄解析实际目标，交回独立拥有、没有终止NUL的UTF-16规范路径。输入是同步调用期间有效、NUL结尾的UTF-16路径；输出不借用输入，保留实际OS代码单元及名称大小写，不要求代理单元可转换为Unicode标量，也不以字符串转小写代替目标身份。

普通输入先按本次GetFullPathNameW规则取得绝对名称，再将DOS／UNC／设备名称交接为Windows扩展命名空间路径，支持超过传统MAX_PATH的实际文件；精确verbatim、NT前缀及空输入按自身原生语义直接交OS。转换是打开之前的名称交接，不在打开失败后另选路径。输入准备失败由Input(AbsoluteError)返回。当前目录与名字解析不构成原子文件系统快照，调用者仍负责本次构建输入与实际内容的关系。

函数以零访问、读／写／删除共享及目录备份语义打开一次原目标，不指定OPEN_REPARSE_POINT，因此跟随实际符号链接／连接点。GetFinalPathNameByHandleW按normalized／DOS模式先查询容量再读取一次，成功输出保留OS实际返回的扩展前缀；查询失败立即保存原OS错误，容量增长由BufferTooSmall保留实际所需容量。查询完成后关闭原句柄一次，成功关闭后才交付拥有结果；不重查、不替换标志或交付部分输出。

```b
enum CanonicalQueryError : u32 { System(u32)=1; BufferTooSmall(u32)=2; };
struct CanonicalCloseError { address handle; u32 code; };
struct CanonicalQueryCloseError { CanonicalQueryError query; CanonicalCloseError close; };
enum CanonicalError : u32 {
    Open(u32)=1;
    Query(CanonicalQueryError)=2;
    Close(CanonicalCloseError)=3;
    QueryAndClose(CanonicalQueryCloseError)=4;
    Input(AbsoluteError)=5;
};
```

Open交回真实打开错误；Query表示查询失败且句柄已成功关闭。Close与QueryAndClose保留关闭失败时仍由调用者持有的原句柄及原错误，后者同时保留查询错误，不伪称资源已经释放。临时名称、查询缓冲与成功拥有结果继续由原Frame和最后消费者管理，不新增路径缓存或释放链。

归档消费编译只导入path.lib；链接提供path.lib及实际使用的`push<u16>`序列实例，提供者.h／.b不必可读取。当前Rust编译器的build/object.rs与build/mod.rs仍使用原Rust实现，本B库原生可用与编译器迁移、正式周发行及采用分别记录。平台契约见[GetFinalPathNameByHandleW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew)及[Rust当前Windows规范路径消费者](https://github.com/rust-lang/rust/blob/1.97.1/library/std/src/sys/fs/windows.rs)。

### 替换路径扩展名

`result<dataset<u16>,ExtensionError> with_extension(read dataset<u16> path,read dataset<u16> extension) on cpu`是Windows路径的纯词法操作。两个输入均为按实际长度借用的UTF-16代码单元，不需要终止NUL；输出是独立拥有的数据集，返回后不保留输入借用。原大小写、非ASCII代码单元、未配对代理项及内部NUL原样参与词法组合；它不验证OS是否接受最终名称，不查询文件存在、不解析链接、不读取当前目录。

函数识别普通盘符、UNC、设备和精确verbatim前缀，保留原前缀及文件名之前的实际代码单元。普通路径按反斜线与斜线分隔，verbatim主体只按反斜线分隔。最后的普通文件名按最后一个非首位点分成stem与旧扩展名；点文件名的首位点不作为扩展名分隔。普通末尾空组件及点组件不成为文件名，verbatim末尾点组件与末尾父目录组件没有可替换文件名；无文件名时交回原路径的独立副本。

存在文件名时，结果截到stem末尾，再按需附加一个点及完整新扩展名；空扩展名只移去旧扩展名，包含多个点的新扩展名整体追加。路径末尾被忽略的分隔符／点组件随实际stem截断，不恢复为另一份规范化路径。

```b
enum ExtensionError : u32 { Separator(u32)=1; LengthOverflow=2; };
```

新扩展名中的第一个斜线或反斜线由Separator交回其UTF-16单元索引，即使原路径没有文件名也不把它当成有效扩展名。LengthOverflow仅表示最终拥有数据集无法用当前u32长度表达，没有MAX_PATH或其他固定路径长度阈值。输入只读、输出修改及资源释放沿原dataset／Frame与最后消费者，不建立路径对象缓存或第二拥有者。

归档调用方编译只导入path.lib，链接只需实际路径归档及生成物的原运行库代码，不要求sequence.lib或提供者.h／.b。当前Rust装入器与构建输入仍使用原Rust路径调用；本B库能力、消费者迁移与正式周发行分别记录。直接替换规则见[Rust PathBuf::set_extension](https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.set_extension)。

### 组合路径

`result<dataset<u16>,JoinError> join(read dataset<u16> base,read dataset<u16> path) on cpu`按Windows词法规则组合基路径与右路径，交回独立拥有、按实际长度存放的UTF-16代码单元。输入不需要终止NUL，原大小写、非ASCII、未配对代理项及内部NUL均保持。两个只读参数可以借用同一数据集；返回后输入可修改、截断或释放，结果由原Frame及最后消费者持有。

右路径带有已识别的盘符、UNC、设备或精确verbatim前缀时替换整个基路径，盘符相对路径也按其自身前缀处理。普通基路径遇到无前缀的根路径时保留原基前缀，再交接右路径；普通相对路径在需要时加入反斜线。单独的普通盘符前缀如`C:`直接接右相对路径，保持盘符相对含义。空右路径沿同一分隔符规则组合。

基路径为verbatim且右路径非空时按组件组合：右根路径保留基前缀并从根重新交接，右点组件忽略，右父目录组件只移去紧邻的普通组件。基路径已有的verbatim组件按其实际含义保留；重组的组件间使用反斜线，组件内真实代码单元保持。精确verbatim主体只按反斜线分隔，普通右路径按两类斜线分隔。组合是纯词法计算，当前目录、文件存在和链接目标仍由各自调用的所有者解释。

```b
enum JoinError : u32 { LengthOverflow=1; };
```

LengthOverflow表示实际拥有数据集在本次组合中无法用当前u32长度继续表达，结果及必要的临时组件数据均沿原dataset／resize规则交接；没有MAX_PATH或预设组件数上限。临时组件是本次算法的普通数据，结束后由原Frame释放，不形成路径缓存或新的运行时拥有表。

源码消费使用`import blink.windows.path;`与`-I library`；归档消费只导入path.lib，链接只提供本次实际路径归档及原运行库代码，join不要求sequence.lib或提供者.h／.b。当前Rust模块装入／构建图仍使用原Rust路径调用，本B接口的原生可用、编译器迁移、中央镜像及正式周发行分别记录。平台组合规则见[Rust PathBuf::push](https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.push)。
