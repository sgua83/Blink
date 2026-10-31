# 《B 语言编程》配套示例

[书稿](../B语言编程.md) · [手机 PDF](../pdf/B语言编程-手机阅读版.pdf) · [标准 PDF](../pdf/B语言编程.pdf) · P4台账中的运行历史。

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


使用 import 的 modules、stages_gpu 和 scores_gpu 在编译命令中以 -I sample 指定模块根目录。使用动态数据或 计算函数 的应用链接后生成 exe 及同目录 blink_app_*.dll，部署时一起复制；运行无需编译器、SDK 或 B 源码。GPU 需要系统 Vulkan 驱动。

文档只保留当前源码、语法、接口及能力范围；具体修订、构建和实际运行历史在P4双台账记录。

println支持当前整数、f32／f64和bool标量。types 直接显示温度 21.5 和除法结果 3.5；vectors 直接显示移动后与旋转后的四个坐标分量。通用文本、输入／文件及其他平台尚在研发。

文档只保留当前源码、语法、接口及能力范围；具体修订、构建和实际运行历史在P4双台账记录。
