# 独立应用入门示例

先按[安装指南](../../Docs/GettingStarted.md)安装稳定 Blink 种子、Rust MSVC 和 Windows C++ 构建工具，再从源码构建 Blink；或直接用安装目录中的正式工具编译以下应用。

```powershell
cargo build --workspace --release
.\target\release\bc.exe -c `
    sample/beginner/start.b `
    -o target/start.obj
.\target\release\blink.exe `
    target/start.obj `
    -o target/start.exe
.\target\start.exe
```

终端直接显示 3。每个动手例子都提供 main，直接运行生成的 exe，不需要另写 Rust／C++ 宿主或指定数据文件。


modules 专门讲解模块复用，编译命令以 -I sample 指定模块根目录。stages_gpu 和 scores_gpu 则在同一文件完整展示计算函数，不导入其他教学模块。CPU 普通调用保持CPU执行；numbers直接在main写parallel，totals显式初始化后调用同文件归约／扫描函数，stages直接调用完整展示的两个阶段，scores直接调用填值和求和。shape保留run<N>以讲解固定形状泛型，使用普通CPU调用，不套target。使用动态数据或 计算函数 的应用链接后生成 exe 及同目录 blink_app_*.dll，部署时一起复制；运行无需编译器、SDK 或 B 源码。GPU 需要系统 Vulkan 驱动。


println支持当前整数、f32／f64和bool标量。types 直接显示温度 21.5 和除法结果 3.5；vectors 直接显示移动后与旋转后的四个坐标分量。通用文本、输入／文件及其他平台尚在研发。

文档只保留当前源码、语法、接口及能力范围；具体修订、构建和实际运行历史在P4双台账记录。
