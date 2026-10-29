# Blink 标准库

`library`是正式B模块的源码根。应用以`-I library`装入，用普通泛型、模块和资源规则编译；库代码不反向依赖BLEngine。

## blink.sequence

导入`blink.sequence`后，序列直接使用`dataset<T>`。当前元素为可复制且不含拥有资源或view的普通存储值，包括标量、向量、矩阵、记录、固定数组和带载荷变体。原dataset长度表示逻辑项数，底层Vec容量仍由Frame／blcompute拥有；缩短和clear允许后续复用容量，最终释放沿原资源生命周期。

| 函数 | 输入与结果 |
|---|---|
| `empty<T>()` | 返回长度零的拥有dataset |
| `get(read data,index)` | `Result<T,SequenceIndexError>`；错误含实际index和length |
| `set(inout data,index,value)` | `Result<void,SequenceIndexError>`；错误时内容不变 |
| `push(data,value)` | 返回新增末项的拥有dataset，长度溢出时返回原输入 |
| `pop(data)` | 返回`SequenceRemoved<T>{data,value}`；空输入返回原数据及Empty |
| `insert(data,index,value)` | 允许`index <= length`；返回新序列或原数据及错误 |
| `remove(data,index)` | 允许`index < length`；返回删除项和新序列，非法下标返回原数据 |
| `extend(data,read tail)` | 拼接尾序列；tail仍归调用者，长度溢出返回原data |
| `truncate(data,length)` | 保留前length项；length不小时保持原长度，不增长 |
| `clear(data)` | 返回同一资源所有者下的零长度序列 |

拥有修改函数消费输入，Result成功载荷交回新数据；错误载荷`SequenceFailure<T>`交回原`data`及`reason`。原因是`Empty`、`IndexOutOfRange`、`LengthOverflow`；`remove`空输入属于非法下标。分配或执行失败按原运行库处理，不属于这些库错误。全部操作要求CPU数据，GPU数据先显式download；get只读借用、set借用内容可写，其他修改需要拥有权。真实游标、view及释放函数消费者仍须在移动／调整前结束，extend的尾输入不能与被消费的输入别名。

```b
module main;
import blink.sequence;

u32 main() on cpu {
    dataset<u32> data=empty<u32>();
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
target/release/blc.exe -c main.b -I library -o target/main.obj
target/release/bllink.exe target/main.obj -o target/main.exe
target/main.exe
```

也可用`blink.sequence::push`等限定名。开放泛型按装入的B源码实例化；独立对象／归档只导出明确请求的实例，例如：

```powershell
target/release/blc.exe -c library/blink/sequence.b -I library --module --instance 'push<u32>' -o target/sequence.obj
target/release/bllink.exe --archive target/sequence.obj -o target/sequence.lib
```

实际应用须导出其使用的各个实例，并通过`--import target/sequence.lib`编译，再与该库链接。对象合同为BLCO19，资源ABI为BLRI5；缺失实例拒绝，链接不重读B源码。动态拥有元素、递归容器、持久view图、字符串／字节库和一般while回边不变式继续待实现；本库交付不等于BLEngine业务已迁移。
