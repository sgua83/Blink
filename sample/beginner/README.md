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


使用import的modules、stages_gpu和scores_gpu在编译命令中以-I sample指定模块根目录。使用动态数据或计算函数的应用将B代码与已编译运行库一次链接到同一个exe，部署复制该exe及实际声明的外部DLL；运行无需编译器、SDK或B源码。GPU需要系统Vulkan驱动。


println支持当前整数、f32／f64和bool标量。types 直接显示温度 21.5 和除法结果 3.5；vectors 直接显示移动后与旋转后的四个坐标分量。通用文本、输入／文件及其他平台尚在研发。

文档只保留当前源码、语法、接口及能力范围；具体修订、构建和实际运行历史在P4双台账记录。
