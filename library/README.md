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

实际应用须导出其使用的各个实例，并通过`--import target/sequence.lib`编译，再与该库链接。当前对象与资源ABI版本见[语言规范](../Docs/BLanguage.md#已编译模块的资源abi)；缺失实例拒绝，链接不重读B源码。动态拥有元素、递归容器、持久view图、完整字符串、字符长度／切片和一般while回边不变式继续待实现；本库交付不等于BLEngine业务已迁移。

## blink.unicode

`blink.unicode`以纯B实现严格UTF-8／UTF-16标量编解码和整段转换。以`import blink.unicode;`导入，源码根仍为`-I library`。标量接口可在普通CPU函数和CPU并行域调用；拥有输出的整段转换属于普通CPU资源组织。

| 函数 | 成功与失败 |
|---|---|
| `decode_utf8(read dataset<u8> text,u32 offset)` | `Result<DecodedScalar,DecodeError>`；offset及next按字节计 |
| `decode_utf16(read dataset<u16> text,u32 offset)` | 同一Result；offset及next按u16代码单元计 |
| `encode_utf8(u32 value)` | `Result<Utf8Encoding,u32>`；成功为One(u8)、Two(array<u8,2>)、Three(array<u8,3>)或Four(array<u8,4>) |
| `encode_utf16(u32 value)` | `Result<Utf16Encoding,u32>`；成功为One(u16)或Two(array<u16,2>) |
| `to_utf16(read dataset<u8> text)` | `Result<dataset<u16>,TextError>`；严格解码整个UTF-8输入，Ok交回拥有UTF-16序列 |
| `to_utf8(read dataset<u16> text)` | `Result<dataset<u8>,TextError>`；严格解码整个UTF-16输入，Ok交回拥有UTF-8字节序列 |

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

完整文本值、字符长度／切片、源码字符串字面量和OS字符串地址／生命周期仍待实现；本模块不表示BLEngine JSON、窗口或IME业务已经迁移。

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

官方合同：[QueryPerformanceCounter](https://learn.microsoft.com/en-us/windows/win32/api/profileapi/nf-profileapi-queryperformancecounter)、[QueryPerformanceFrequency](https://learn.microsoft.com/en-us/windows/win32/api/profileapi/nf-profileapi-queryperformancefrequency)、[高精度时间戳](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)。日历时间、系统错误上下文、跨平台时间抽象及BLEngine计时职责的实际迁移仍未交付。
