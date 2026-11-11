# Blink 学习导航 / Start learning Blink

从一个能运行的小程序开始，再按你的目标选择书籍。四本书都提供完整在线正文，目录分别有[中文](https://github.com/sgua83/Blink/wiki/Blink-Books)和 [English navigation](https://github.com/sgua83/Blink/wiki/Blink-Books-English)；正文目前以中文为主。

[安装与第一个程序](#安装与第一个程序) · [按目标选书](#b-语言书籍--blink-books) · [PDF 下载](#pdf-下载) · [提问与交流](#提问与交流) · [返回项目主页](../../README.md)

## 安装与第一个程序

1. 从 [GitHub Releases](https://github.com/sgua83/Blink/releases/latest) 下载 Windows x64 安装程序。正式版的文档和示例与该发行版本配套使用。
2. 按[中文快速开始](../../README.md#快速开始)或 [English quick start](../../README.en.md#quick-start) 准备构建工具、取得示例并运行第一个 CPU 程序，再阅读代码内选择 GPU 的示例。
3. 若要从源码构建研发工具，先安装可独立取得的稳定 Blink 种子，再执行书稿中的 Cargo 构建。默认构建脚本直接消费 `%ProgramFiles%/Blink` 内的 `bc`、`blink` 与 `VERSION`；书稿会在构建命令之前说明前提。

Wiki 跟随开发版 main，可能包含正式发行尚未具备的能力。各书原始源码链接和书籍目录标明对应快照，安装的正式版与 main 分别记录。

## B 语言书籍 / Blink books

| 你的目标 | 从哪里开始 | 在线正文 |
|---|---|---|
| 从 C／C++ 进入 CPU／GPU 原生并行，理解数据域、归约、扫描与协作，并学习 C／C++ 集成 | 《并行编程的艺术》 | [在线阅读](../并行编程的艺术.md) |
| 把 B 算法作为动态库接入 Rust，理解数据布局、调用和连续计算 | 《Blink 和 Rust 混合编程》 | [在线阅读](../Blink和Rust混合编程.md) |
| 阅读编译器、类型与访问分析、IR、CPU／GPU 后端和执行框架 | 《B 语言源码分析》 | [在线阅读](../B语言源码分析.md) |
| 通过引擎案例学习场景、几何、光照与资源交接 | 《BLEngine 源码分析》 | [在线阅读](../BLEngine源码分析.md) |

刚接触 B，可以先读《并行编程的艺术》；接入 C／C++ 时读该书第十四章，Rust 场景转到《Blink 和 Rust 混合编程》，研究实现时再读源码分析。引擎案例书可以独立阅读；BLEngine 尚未开源，完整引擎的构建需要授权取得源码及匹配的 Blink SDK。

## PDF 下载

标准版与手机阅读版请从 [正式发行资产](https://github.com/sgua83/Blink/releases/latest) 下载 `Blink-Books.zip` 合集；选择其他版本时使用该版本的 Release 页面及实际资产。公开源码树不提供旧 PDF 快照下载。PDF 只在正式发行打包时从同次固定的四本书稿生成，版本随该次发行；最新开发内容以在线 Markdown 为准。

## 提问与交流

- 使用问题与技术讨论：[GitHub Discussions](https://github.com/sgua83/Blink/discussions)。
- 可复现缺陷或功能建议：[GitHub Issues](https://github.com/sgua83/Blink/issues)。附上最小源码、实际工具版本、命令、CPU／GPU 与驱动信息及原始错误，便于核对。
- 项目、安装和 C++／Rust 集成入口：[返回 GitHub 项目主页](../../README.md)。

书稿和本页导航的权威源由 P4 维护，经现有 GitHub 镜像流程自动同步到 Wiki。后续阅读可收藏本页或[书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books)，既有书页地址保持可用。
