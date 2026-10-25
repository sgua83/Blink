# BLEngine 教学摘录来源

取自 //BLEngine/main 截至 P4 CL12965 的已提交版本。下列 B 文件按字节保留；Programs/book 是本书的教学组合，不属于引擎源文件。Rust 宿主为配套教学实现；书中 Rust 引擎摘录另标原路径。

| 文件 | P4 版本 | SHA256 |
|---|---|---|
| Programs/lighting/lights.b | //BLEngine/main/Programs/lighting/lights.b#1 | 8a251fdf7f30d0e3c00cc364ef32abab7b28bbf9651d8040b9eb6826c3e03941 |
| Programs/lighting/lights.h | //BLEngine/main/Programs/lighting/lights.h#1 | e36794a70b75d692226580bb3cca4acf736ba5ac9eef6f3893a2d0093df850bd |
| Programs/material/palette.b | //BLEngine/main/Programs/material/palette.b#1 | b6a285ac50946563436f7e334992bed07b26193b608623d96a34d3bdea67047c |
| Programs/material/palette.h | //BLEngine/main/Programs/material/palette.h#2 | 74167813ec5b809813e73ff1803bceb091d18f21e4ea67788eddc4b2332d9cdf |
| Programs/material/surface.b | //BLEngine/main/Programs/material/surface.b#3 | a97372bce86718340e628565dfbd2dbf6d463de5ae9a1284a51b43232a0c8d7a |
| Programs/material/surface.h | //BLEngine/main/Programs/material/surface.h#1 | 960645b0b47041621fb67d4c92b94b6fdb1e3d1f3ab6e856ae15f64fa315a5de |
| Programs/material/textured.b | //BLEngine/main/Programs/material/textured.b#4 | f628f644c5c54c954e1aaff148d1a8b10892a1bb77550275b657b18f0ea19db3 |
| Programs/material/textured.h | //BLEngine/main/Programs/material/textured.h#4 | 9f1daefe804c15f812fc2a0cb9cc7d3dc1fa4ec4c55fc790e5326e747657f351 |
| Programs/math/field.b | //BLEngine/main/Programs/math/field.b#3 | fc74a15454d685d2badfb5f15b168a8a9c0a7f39fe834b5b3cf4dc3cb7f586f1 |
| Programs/math/field.h | //BLEngine/main/Programs/math/field.h#3 | a0b82319874b7512ae6b72607e5e76c00494628436638b2feb9df4a7a2af62c7 |
| Programs/math/transform.b | //BLEngine/main/Programs/math/transform.b#2 | f919f691e84ffa426a656540182f27ee00bd77c5633cc17d55e5afb262ed12b2 |
| Programs/math/transform.h | //BLEngine/main/Programs/math/transform.h#2 | f6224686eead1786a89f242de3a3377446b8d79a62db649c0dcd545ecaaba439 |
| Programs/render/frame.b | //BLEngine/main/Programs/render/frame.b#11 | 65c1615be25bfa13fa7928a89ed95a28669053e5ee12ffde7a592047034c7d75 |
| Programs/render/frame.h | //BLEngine/main/Programs/render/frame.h#11 | 11b2b1313a79d4c796827be8d43773e4264d6c76498e9388c724c4a25d61aefd |
| Programs/render/resolve.b | //BLEngine/main/Programs/render/resolve.b#4 | 550b5e3b97d90f4c149354d44d92952e542c66743989a37c938400c877c91d2f |
| Programs/render/resolve.h | //BLEngine/main/Programs/render/resolve.h#4 | 00f54850e405ddf97f809651fe722264dd27c5d0112631242c783af69eafce4e |
| Programs/render/trace.b | //BLEngine/main/Programs/render/trace.b#16 | 3610c8de0819b28ebe3fd3fe3224ca18c518f68b88c559658fcbd0f76d933b6e |
| Programs/render/trace.h | //BLEngine/main/Programs/render/trace.h#16 | a29bc33824aee2626e2a3ee77c5507ce7362f8c80828ffb05e9cc0fa477cc2e2 |
| Programs/scene/camera.b | //BLEngine/main/Programs/scene/camera.b#1 | 66c9bad19037a499a1e2641306c553c74f329f2324d2d580ec59e402e86b4b8e |
| Programs/scene/camera.h | //BLEngine/main/Programs/scene/camera.h#1 | b7bf03ae4e985e24bc8068fd6defd3e1d983f203c9e852ca4bfec5877518b7bb |
| Programs/scene/components.b | //BLEngine/main/Programs/scene/components.b#1 | f33691b08a194d06ad4d74c347b2ed7b93370670929e2861113e144d50904c00 |
| Programs/scene/components.h | //BLEngine/main/Programs/scene/components.h#1 | dbb3053f1a62a8494aa3e3d2a0b3473852837266028291df84bad8bc4eec2059 |
| Programs/scene/hierarchy.b | //BLEngine/main/Programs/scene/hierarchy.b#1 | 48f7791608f856af6cce9722be303f03f66263f21e59c65e7a8dffef3ed08cd4 |
| Programs/scene/hierarchy.h | //BLEngine/main/Programs/scene/hierarchy.h#1 | 346fc399d4766cdd7da24fcc7008f6454c3cf64dd05bf70ee85d3eaadab16a8d |
| Programs/scene/transforms.b | //BLEngine/main/Programs/scene/transforms.b#3 | 4d47a6a95ee892d7fa640a77ab4bd5a13141517671f9139529cf178c802eaa1b |
| Programs/scene/transforms.h | //BLEngine/main/Programs/scene/transforms.h#3 | 5c2b42e32364e02396459cd8e42c18cb939ee057c28165fcc1d332629c9a86f3 |
| Programs/simulation/bodies.b | //BLEngine/main/Programs/simulation/bodies.b#9 | 60613260d125db91662d6a001c0c7956af4304e0e442d4fb14dea1fd190f0509 |
| Programs/simulation/bodies.h | //BLEngine/main/Programs/simulation/bodies.h#8 | d2beb6318438743d497de4fd89d5154646ccb399640c891bb19555e5a12c00c4 |
| Programs/simulation/statistics.b | //BLEngine/main/Programs/simulation/statistics.b#4 | a13cdee4ffcc442eadb5ead6e83cf0d3aecb14b3b2527a7149cc8e81524c3d73 |
| Programs/simulation/statistics.h | //BLEngine/main/Programs/simulation/statistics.h#4 | 8eebb295e6206585e5dcfc82c32d14aab79e14fcd50b9f40e81c4f91b9e47f3d |
