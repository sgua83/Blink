# BLEngine 教学摘录来源

这些教学模块最初取自 //BLEngine/main 的 P4 CL12965，现以仓库中的当前教学实现为准，使用统一返回类型函数语法。本轮以 Blink CL13553 的 Release 编译器检查并构建 Rust 宿主。下表保留原始来源修订，SHA256 对应当前随书文件；Programs/book 是教学组合。纯接口模块只列真实存在的文件。当前引擎的源码分析和原生执行另见《BLEngine 源码分析》。

| 文件 | 原始来源 P4 版本 | 当前文件 SHA256 |
|---|---|---|
| Programs/lighting/lights.b | //BLEngine/main/Programs/lighting/lights.b#1 | 8a251fdf7f30d0e3c00cc364ef32abab7b28bbf9651d8040b9eb6826c3e03941 |
| Programs/lighting/lights.h | //BLEngine/main/Programs/lighting/lights.h#1 | e36794a70b75d692226580bb3cca4acf736ba5ac9eef6f3893a2d0093df850bd |
| Programs/material/palette.h | //BLEngine/main/Programs/material/palette.h#2 | 74167813ec5b809813e73ff1803bceb091d18f21e4ea67788eddc4b2332d9cdf |
| Programs/material/surface.b | //BLEngine/main/Programs/material/surface.b#3 | a97372bce86718340e628565dfbd2dbf6d463de5ae9a1284a51b43232a0c8d7a |
| Programs/material/surface.h | //BLEngine/main/Programs/material/surface.h#1 | 960645b0b47041621fb67d4c92b94b6fdb1e3d1f3ab6e856ae15f64fa315a5de |
| Programs/material/textured.b | //BLEngine/main/Programs/material/textured.b#4 | ef10aa45625270a48fc820d4bab4590bdb23be9be52505b7de4f51016a1f6b01 |
| Programs/material/textured.h | //BLEngine/main/Programs/material/textured.h#4 | 290d75f89aadf1f60fa610478a9f5faa9a5c092bf879fb920162156cea115e8c |
| Programs/math/field.b | //BLEngine/main/Programs/math/field.b#3 | fc74a15454d685d2badfb5f15b168a8a9c0a7f39fe834b5b3cf4dc3cb7f586f1 |
| Programs/math/field.h | //BLEngine/main/Programs/math/field.h#3 | a0b82319874b7512ae6b72607e5e76c00494628436638b2feb9df4a7a2af62c7 |
| Programs/math/transform.b | //BLEngine/main/Programs/math/transform.b#2 | f919f691e84ffa426a656540182f27ee00bd77c5633cc17d55e5afb262ed12b2 |
| Programs/math/transform.h | //BLEngine/main/Programs/math/transform.h#2 | f6224686eead1786a89f242de3a3377446b8d79a62db649c0dcd545ecaaba439 |
| Programs/render/frame.b | //BLEngine/main/Programs/render/frame.b#11 | 40aa565890083b07ee5abfb4381b9cbf68b98f36c1617f5453089fb5d9c2de01 |
| Programs/render/frame.h | //BLEngine/main/Programs/render/frame.h#11 | 6f6da4ecc00253fa6d471c7b4116fc384551c7969ea2debc1ca5fd30422d65e5 |
| Programs/render/resolve.b | //BLEngine/main/Programs/render/resolve.b#4 | f2cecb6370ac9c531d2af909f04c3b62884963cee36f2b656e97b4d271891d2d |
| Programs/render/resolve.h | //BLEngine/main/Programs/render/resolve.h#4 | 10a10b0e4e64bc2622a6acf81c5d8e7a6803849a8e4353995840346b5173ea03 |
| Programs/render/trace.b | //BLEngine/main/Programs/render/trace.b#16 | 5ba263d26df38f7c0bc2184eeaa925a471b72248870ef398eceb63d889c85147 |
| Programs/render/trace.h | //BLEngine/main/Programs/render/trace.h#16 | dc9f68da32441c3834b519d1f308efabb8f54abff64ec72afbdff67b3640cb21 |
| Programs/scene/camera.b | //BLEngine/main/Programs/scene/camera.b#1 | bb41089666e564be32634e7eb457580ef61d8e03d58c0099322e920f5161fa68 |
| Programs/scene/camera.h | //BLEngine/main/Programs/scene/camera.h#1 | b76a87c50686cf4211a3ec4664b8a2bd4e74488c0916ce702cdbb71ed9fd6d68 |
| Programs/scene/components.b | //BLEngine/main/Programs/scene/components.b#1 | 8616e8fc6854cbaff933a85a45a026a871198b15369cbd8f5f8d34323c2f1422 |
| Programs/scene/components.h | //BLEngine/main/Programs/scene/components.h#1 | f4dd714f5be3d604fe3f61278654bbcc5cb051dc91f52c155ba31a815f3b0fc0 |
| Programs/scene/hierarchy.b | //BLEngine/main/Programs/scene/hierarchy.b#1 | 520131abb1bed595346864fcc7bd2f6bf5a0d05d8c874e90cfc11a97eda2b164 |
| Programs/scene/hierarchy.h | //BLEngine/main/Programs/scene/hierarchy.h#1 | 5ba2c83db5f54cc4cf4c9bcf9e42aa6ffd89ce151d0e21729f2e3f467c366c65 |
| Programs/scene/transforms.b | //BLEngine/main/Programs/scene/transforms.b#3 | 8efa8a6a7de1316fba6101b68985fa7f7d87dd6213860322b15ebef51580afc7 |
| Programs/scene/transforms.h | //BLEngine/main/Programs/scene/transforms.h#3 | e6deab7cec8ba2960720b6a01e3d74ac80b729f44b251fabe56f3f7e29f8c9c8 |
| Programs/simulation/bodies.b | //BLEngine/main/Programs/simulation/bodies.b#9 | 61474960f9ff3117095dbbea90090ad0b0b40cf6783b04b7438be5ffa0492787 |
| Programs/simulation/bodies.h | //BLEngine/main/Programs/simulation/bodies.h#8 | 94da4d4b8068e4928b65e67be0009d3a885e089cd2d66eb6d8a2de0d27b2c30d |
| Programs/simulation/statistics.b | //BLEngine/main/Programs/simulation/statistics.b#4 | 44bdea6071418d49e50e1a731c840acd31cc80fef762042e7b8153b6c02b5ff6 |
| Programs/simulation/statistics.h | //BLEngine/main/Programs/simulation/statistics.h#4 | 9b5eae49782b2bf845bd42adc97f1a0b193978d074eb4de74032d3066941a65d |
