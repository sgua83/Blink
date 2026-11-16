# C++ 调用 B

[返回入门指南](GettingStarted.md#first-dll) · [English](CppInterop.en.md)

把 C++ 中的四个数交给 B 函数计算 `value * 2 + 1`，结果写回原数组。本篇只介绍 C++ 调用代码。

## 准备环境

先完成[安装](GettingStarted.md#安装)，包括生成计算 DLL 所需的 Rust MSVC 与 Windows 构建工具。打开 **x64 Developer PowerShell**，新建一个空目录，在该目录完成下面步骤；不需要克隆仓库。

## 编写 B 计算函数

在同一目录保存两份文件：

**map.h**

```c
module map;

void scale(inout dataset<f32> values) on compute;
```

**map.b**

```c
module map;

void scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}
```

## 生成 DLL

```powershell
bc -c map.b --program scale -o map.obj
blink map.obj -o map.dll
```

生成物包含 `map.dll`、导入库 `map.lib` 和下面使用的调用接口。

## 编写调用程序

**main.cpp**

```cpp
#include "map.abi.h"
#include <cstdio>

int main() try {
    float values[] = {1, 2, 3, 4};
    blink_map::Program program(blink_map::Backend::Cpu);
    program.run(values, 4);
    for (float value : values) std::printf("%.0f ", value);
    std::puts("");
    return 0;
} catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
}
```

`Backend::Cpu` 选择 CPU，`Program` 在析构时释放句柄；数组始终由调用程序拥有。

## 编译并运行

```powershell
cl /nologo /utf-8 /EHsc main.cpp map.lib /Fe:main.exe
.\main.exe
```

预期输出：

```text
3 5 7 9
```

部署时将 `main.exe`、`map.dll` 及链接器生成的配套 DLL 放在一起。支持类型、系统运行依赖和 GPU 使用方式见[原生集成文档](NativeInterop.md)。
