# BLEngine 摘录来源

本文为《并行编程的艺术》的源码引用定位，更新于 2026-10-25。引用以相邻 BLEngine 工作区对应的已提交 P4 修订为准，SHA-256 对这些修订的实际导出字节计算；本目录不复制整套引擎实现。

普通教学模块位于 lessons。引擎函数与函数体摘录标明原文件；显示转换一节将当前通用实现的 S=2 取样循环展开，便于与四项读取的串行版本逐行比较。性能工具直接编译引擎完整入口，未使用这个讲解展开片段代替测量。

| 原文件 | P4 修订 | SHA-256 |
|---|---|---|
| [Programs/simulation/bodies.h](../../BLEngine/Programs/simulation/bodies.h) | #8 | `d2beb6318438743d497de4fd89d5154646ccb399640c891bb19555e5a12c00c4` |
| [Programs/simulation/bodies.b](../../BLEngine/Programs/simulation/bodies.b) | #9 | `60613260d125db91662d6a001c0c7956af4304e0e442d4fb14dea1fd190f0509` |
| [Programs/simulation/statistics.h](../../BLEngine/Programs/simulation/statistics.h) | #4 | `8eebb295e6206585e5dcfc82c32d14aab79e14fcd50b9f40e81c4f91b9e47f3d` |
| [Programs/simulation/statistics.b](../../BLEngine/Programs/simulation/statistics.b) | #4 | `a13cdee4ffcc442eadb5ead6e83cf0d3aecb14b3b2527a7149cc8e81524c3d73` |
| [Programs/math/field.h](../../BLEngine/Programs/math/field.h) | #3 | `a0b82319874b7512ae6b72607e5e76c00494628436638b2feb9df4a7a2af62c7` |
| [Programs/math/field.b](../../BLEngine/Programs/math/field.b) | #3 | `fc74a15454d685d2badfb5f15b168a8a9c0a7f39fe834b5b3cf4dc3cb7f586f1` |
| [Programs/scene/transforms.h](../../BLEngine/Programs/scene/transforms.h) | #3 | `5c2b42e32364e02396459cd8e42c18cb939ee057c28165fcc1d332629c9a86f3` |
| [Programs/scene/transforms.b](../../BLEngine/Programs/scene/transforms.b) | #3 | `4d47a6a95ee892d7fa640a77ab4bd5a13141517671f9139529cf178c802eaa1b` |
| [Programs/render/resolve.h](../../BLEngine/Programs/render/resolve.h) | #4 | `00f54850e405ddf97f809651fe722264dd27c5d0112631242c783af69eafce4e` |
| [Programs/render/resolve.b](../../BLEngine/Programs/render/resolve.b) | #4 | `550b5e3b97d90f4c149354d44d92952e542c66743989a37c938400c877c91d2f` |
| [Programs/render/trace.h](../../BLEngine/Programs/render/trace.h) | #15 | `846276e208c8fe1296ef74f67b2c12c4622dbdd9c79c261ee2aad0d829ace918` |
| [Programs/render/trace.b](../../BLEngine/Programs/render/trace.b) | #15 | `3ce54f15fb602910b00d9e017d3755b190f69d185aa4f6e52a4ea167cce6cfde` |
| [Programs/material/surface.h](../../BLEngine/Programs/material/surface.h) | #1 | `960645b0b47041621fb67d4c92b94b6fdb1e3d1f3ab6e856ae15f64fa315a5de` |
| [Programs/material/surface.b](../../BLEngine/Programs/material/surface.b) | #3 | `a97372bce86718340e628565dfbd2dbf6d463de5ae9a1284a51b43232a0c8d7a` |
| [Programs/material/textured.h](../../BLEngine/Programs/material/textured.h) | #4 | `9f1daefe804c15f812fc2a0cb9cc7d3dc1fa4ec4c55fc790e5326e747657f351` |
| [Programs/material/textured.b](../../BLEngine/Programs/material/textured.b) | #4 | `f628f644c5c54c954e1aaff148d1a8b10892a1bb77550275b657b18f0ea19db3` |
| [Programs/render/frame.h](../../BLEngine/Programs/render/frame.h) | #11 | `11b2b1313a79d4c796827be8d43773e4264d6c76498e9388c724c4a25d61aefd` |
| [Programs/render/frame.b](../../BLEngine/Programs/render/frame.b) | #11 | `65c1615be25bfa13fa7928a89ed95a28669053e5ee12ffde7a592047034c7d75` |

性能测量直接调用 simulation.bodies::step、simulation.statistics::step_and_total、scene.transforms::prepare、material.textured::shade 与 render.resolve::resolve；本书测量的显示入口为 1000×1000 输出（一百万个像素）、2×2 内部采样。C++ 完整对照位于 performance/serial.cpp，调用与计时位于 performance/src/main.rs。

语言规则以 Blink 的 Docs/BLanguage.md 为准；执行与资源所有权以 Docs/Architecture.md 和实际框架为准。更新中的完整帧与求交源码属于持续研发；本书各项性能结论仅对应明确列出的入口与输入，不外推为整帧 FPS。
