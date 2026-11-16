# Rust 调用 B

[返回入门指南](GettingStarted.md#first-dll) · [English](RustInterop.en.md)

把 Rust 中的四个数交给 B 函数计算 `value * 2 + 1`，结果写回原数组。本篇只介绍 Rust 调用代码。

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

**main.rs**

```rust
#[allow(non_camel_case_types, dead_code)]
#[path = "map.rs"]
mod map;

fn main() -> Result<(), String> {
    let mut values = [1.0, 2.0, 3.0, 4.0];
    let mut program = map::Program::new(map::Backend::Cpu)?;
    program.run(&mut values)?;
    println!("{values:?}");
    Ok(())
}
```

`Backend::Cpu` 选择 CPU；`run` 同步借用可变切片，`Program` 在 Drop 时释放句柄。

## 编译并运行

```powershell
rustc --edition=2024 -C opt-level=3 main.rs -L native=. -o main.exe
.\main.exe
```

预期输出：

```text
[3.0, 5.0, 7.0, 9.0]
```

部署时将 `main.exe`、`map.dll` 及链接器生成的配套 DLL 放在一起。支持类型、系统运行依赖和 GPU 使用方式见[原生集成文档](NativeInterop.md)。

如需直接使用原生 Rust SDK 和驻留资源，继续看[原生 API 示例](../../sample/interop/README.md#rust-原生-api)。
