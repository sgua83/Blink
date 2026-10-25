# BLEngine 摘录来源

本文为《并行编程的艺术》的引用定位，摘录时点为 2026-10-25。原文在相邻 BLEngine 项目；本目录不复制整套引擎实现。SHA-256 对实际文件字节计算，P4 版本表示本机 have 版本。

摘录直接取自下列源码；函数体片段在正文注明，摘录未改写算法。正文中普通教学模块在 lessons，二者分开标识。

| 原文件 | P4 have 版本 | SHA-256 |
|---|---|---|
| [Programs/simulation/bodies.h](../../BLEngine/Programs/simulation/bodies.h) | #8 | `d2beb6318438743d497de4fd89d5154646ccb399640c891bb19555e5a12c00c4` |
| [Programs/simulation/bodies.b](../../BLEngine/Programs/simulation/bodies.b) | #9 | `60613260d125db91662d6a001c0c7956af4304e0e442d4fb14dea1fd190f0509` |
| [Programs/simulation/statistics.h](../../BLEngine/Programs/simulation/statistics.h) | #4 | `8eebb295e6206585e5dcfc82c32d14aab79e14fcd50b9f40e81c4f91b9e47f3d` |
| [Programs/simulation/statistics.b](../../BLEngine/Programs/simulation/statistics.b) | #4 | `a13cdee4ffcc442eadb5ead6e83cf0d3aecb14b3b2527a7149cc8e81524c3d73` |
| [Programs/math/field.h](../../BLEngine/Programs/math/field.h) | #3 | `a0b82319874b7512ae6b72607e5e76c00494628436638b2feb9df4a7a2af62c7` |
| [Programs/math/field.b](../../BLEngine/Programs/math/field.b) | #3 | `fc74a15454d685d2badfb5f15b168a8a9c0a7f39fe834b5b3cf4dc3cb7f586f1` |
| [Programs/scene/transforms.h](../../BLEngine/Programs/scene/transforms.h) | #2 | `00fe9c322838a6519678af79384e50b945cd6420b0cdd5f00747244f104b150e` |
| [Programs/scene/transforms.b](../../BLEngine/Programs/scene/transforms.b) | #2 | `6e14185a4544f62b5ede19a693dddf025274b2e91450133c30a342536ad1e6a5` |
| [Programs/render/resolve.h](../../BLEngine/Programs/render/resolve.h) | #3 | `60db292357d304252dde76dc2b07f096529f9c8f7432dc91384ec66b375aa9d0` |
| [Programs/render/resolve.b](../../BLEngine/Programs/render/resolve.b) | #3 | `a339513df0a2ff4bba4b43e42cef64c705b443b8304d6f3f5b51d44d6c8d82d6` |
| [Programs/render/trace.h](../../BLEngine/Programs/render/trace.h) | #13 | `d9d027b0af0f2b0c86111e3ccf0d3c174942318587f8404eca232600a2a658e5` |
| [Programs/render/trace.b](../../BLEngine/Programs/render/trace.b) | #13 | `50bcbddeb02a90cf34a0ee1a368f694a2cb47bda45bcfeded79575e35d94cfdd` |
| [Programs/material/surface.h](../../BLEngine/Programs/material/surface.h) | #1 | `960645b0b47041621fb67d4c92b94b6fdb1e3d1f3ab6e856ae15f64fa315a5de` |
| [Programs/material/surface.b](../../BLEngine/Programs/material/surface.b) | #2 | `6424c09781c3ce144e17b7c439ae852714f309a68d382c4c256257b8340b616b` |
| [Programs/material/textured.h](../../BLEngine/Programs/material/textured.h) | #4 | `9f1daefe804c15f812fc2a0cb9cc7d3dc1fa4ec4c55fc790e5326e747657f351` |
| [Programs/material/textured.b](../../BLEngine/Programs/material/textured.b) | #4 | `f628f644c5c54c954e1aaff148d1a8b10892a1bb77550275b657b18f0ea19db3` |
| [Programs/render/frame.h](../../BLEngine/Programs/render/frame.h) | #9 | `b19d8eb734749019a737a983c8222c9f3900213fc8483435f859f31fa068008e` |
| [Programs/render/frame.b](../../BLEngine/Programs/render/frame.b) | #9 | `062e24d70923344a4d31dc9d4393f1745f5f8f410781ccf27324042740ac52b8` |
| [Programs/render/dimensions.h](../../BLEngine/Programs/render/dimensions.h) | #1 | `843287995e9c8e260c774d57ebbff72f7bbecb590b5fea631be8789a5ed3b6c0` |

语言规则以 Blink 的 Docs/BLanguage.md 为准；运行时所有权以 Docs/Architecture.md 和实际框架为准。资料存在研发进展时，正文明确当前子集与限制，不将旧摘录当作永久语言规格。
