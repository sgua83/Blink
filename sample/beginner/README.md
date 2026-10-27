# 《B 语言编程》配套示例

[书稿](../B语言编程.md) · [手机 PDF](../pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](../pdf/B语言编程.pdf) · [运行记录](运行记录.md)。

先构建 Blink，再编译、链接并运行第一个 B 应用：

```powershell
cargo build --workspace --release
.\target\release\blc.exe -c `
    sample/beginner/start.b `
    -o target/start.obj
.\target\release\bllink.exe `
    target/start.obj `
    -o target/start.exe
.\target\start.exe
```

终端直接显示 3。每个动手例子都提供 main，直接运行生成的 exe，不需要另写 Rust／C++ 宿主或指定数据文件。

前十四章：start、names、types、choice、loops、functions、borrow、records、arrays、vectors、modules、generic、grades。math 是共用函数模块。这些应用原有的 CL13085 Release 原生运行记录保留；types、vectors 已使用 CL13181 Release 补充直接小数输出，最新结果见 floating_applications。

第十五至十九章：numbers_serial 是顺序对照；numbers、stages、totals、shape、scores 创建自己的 dataset 并调用 CPU 计算函数；stages_gpu、scores_gpu 复用算法并显式组织 upload、run_gpu、download。八个应用均使用 CL13172 Release 编译器真实编译和直接运行，CPU／GPU 完整输出一致。命令见书稿。

使用 import 的 modules、stages_gpu 和 scores_gpu 在编译命令中以 -I sample 指定模块根目录。使用动态数据或 计算函数 的应用链接后生成 exe 及同目录 blink_app_*.dll，部署时一起复制；运行无需编译器、SDK 或 B 源码。GPU 需要系统 Vulkan 驱动。

应用运行记录.json 保存原基础应用、旧计算模块和本轮独立并行应用的版本、源码／exe／执行库 SHA256、完整标准输出与退出码。旧 blrun 记录只是历史证据，不是本版书稿的运行步骤。

println 当前支持 u32、i32、f32、bool。types 直接显示温度 21.5 和除法结果 3.5；vectors 直接显示移动后与旋转后的四个坐标分量。通用文本、输入／文件及其他平台尚在研发。

本版已使用分离后的 Release blc／bllink 重新编译、链接并运行全部二十一个应用；compile_link_applications 保存本轮原始记录。此前版本段落和 JSON 字段继续作为历史结果保留。
