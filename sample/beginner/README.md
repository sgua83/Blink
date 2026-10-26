# 《B 语言编程》配套示例

[书稿](../B语言编程.md) · [手机 PDF](../pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](../pdf/B语言编程.pdf) · [运行记录](运行记录.md)。

前十四章直接编译 B 应用，然后运行 exe，在终端看到结果：

```powershell
cargo build --workspace --release
.\target\release\blc.exe build-exe `
    sample/beginner/start.b target/start.exe
.\target\start.exe
```

输出为 3。普通应用不需要 blrun、:count、输入／输出数据文件或另写 Rust/C++ 宿主。导入模块的 modules 示例在编译命令末尾使用 sample 作为模块根目录，其余命令见书稿。

独立应用示例：start、names、types、choice、loops、functions、borrow、records、arrays、vectors、modules、generic、grades。math 为共用函数模块。每个同名 .h/.b 提供接口与实现。

第十五章起的 numbers、stages、totals、shape、scores 保留并行 program，用于讲解数据域、阶段、集合、GPU 和同一成绩规则的并行实现。当前由正式 blrun 或宿主 API 执行，命令、数据与读取方法见对应章节。B 应用自身创建动态 dataset、组织 CPU/GPU program 的能力仍待 C-011。

十三个独立 exe 与七次 CPU/GPU 计算模块执行均已原生运行。详见应用运行记录.json。当前 println 支持 u32、i32、bool；f32 格式化、通用文本和输入／文件等能力尚未实现，书稿分别说明。
