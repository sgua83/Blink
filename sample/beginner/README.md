# 《B 语言编程》配套示例

[独立书稿](../B语言编程.md) · [手机 PDF](../pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](../pdf/B语言编程.pdf) · [实际运行记录](运行记录.md)。

本目录只依赖 Blink 自己的编译器与执行框架。先按仓库首页安装工具，在 Blink 根目录执行 `cargo build --workspace --release`。随后直接运行例子：

```powershell
.\target\release\blrun.exe cpu `
    sample/beginner/start.b run `
    sample Saved/Learn/start :1
```

结果目录的 numbers.bldata 保存 u32 数字 3。blrun 在命令行显示程序信息与时间，数值写入文件；书中附录 A 给出读取方法。选择 gpu 可执行 GPU 版本，需要显卡驱动提供 Vulkan。

| 文件 | 讲解内容 |
|---|---|
| start.h / start.b | 第一个完整程序，1+2 |
| names.h / names.b | 变量、let、赋值 |
| types.h / types.b | u32、f32、i32、bool 与除法 |
| numbers.h / numbers.b | 一排数据、index、parallel |
| choice.h / choice.b | if/else 与等级选择 |
| loops.h / loops.b | for、while、break |
| records.h / records.b | struct 与字段 |
| functions.h / functions.b | 参数、返回值和提前返回 |
| borrow.h / borrow.b | void、inout 与 write 输出 |
| arrays.h / arrays.b | 固定数组、length 与下标 |
| vectors.h / vectors.b | float2、坐标计算与 float2x2 |
| math.h / math.b | 模块常量与共用函数 |
| modules.h / modules.b | import 与命令行普通值参数 |
| stages.h / stages.b | 多个 program 的组合与文件输入 |
| totals.h / totals.b | 求和与前缀 |
| generic.h / generic.b | 同一做法用于几种小数类型 |
| shape.h / shape.b | const 参数与固定个数 |
| scores.h / scores.b | 完整成绩处理练习 |

每个 `.h` 列出本模块的数据与函数清单，对应 `.b` 写具体做法。运行命令中的模块根目录为 sample；普通模块都以 beginner 开头。没有本书专用命令、启动器或路径别名。

全部示例的命令、输入个数和预期结果在书稿各章中给出。例子使用少量数据来理解写法；大规模效率对照见《并行编程的艺术》及 sample/performance。
