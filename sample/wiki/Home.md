# Blink 学习导航 / Start learning Blink

从一个能运行的小程序开始，再按你的目标选择书籍。四本书都提供完整在线正文，目录分别有[中文](https://github.com/sgua83/Blink/wiki/Blink-Books)和 [English navigation](https://github.com/sgua83/Blink/wiki/Blink-Books-English)；正文目前以中文为主。

[安装与第一个程序](#安装与第一个程序) · [按目标选书](#b-语言书籍--blink-books) · [PDF 下载](#pdf-下载) · [提问与交流](#提问与交流) · [返回项目主页](../../README.md)

## 安装与第一个程序

1. 从 [GitHub Releases](https://github.com/sgua83/Blink/releases/latest) 下载 Windows x64 安装程序。正式版的文档和示例与该发行版本配套使用。
2. 按[中文快速开始](../../README.md#快速开始)或 [English quick start](../../README.en.md#quick-start) 准备构建工具、取得示例并运行第一个 CPU 程序，再阅读代码内选择 GPU 的示例。
3. 若要从源码构建研发工具，先安装可独立取得的稳定 Blink 种子，再执行书稿中的 Cargo 构建。默认构建脚本直接消费 `%ProgramFiles%/Blink` 内的 `bc`、`blink` 与 `VERSION`；书稿会在构建命令之前说明前提。

书籍按各书注明的源码快照阅读，历史性能记录保留当时版本；四本书仅按用户明确要求更新，与每周软件发行分开。

## B 语言书籍 / Blink books

| 你的目标 | 从哪里开始 | 在线正文 |
|---|---|---|
| 从 C／C++ 进入 CPU／GPU 原生并行，理解数据域、归约、扫描与协作，并学习 C／C++ 集成 | 《并行编程的艺术》 | [在线阅读](../并行编程的艺术.md) |
| 把用 B 语言编写的程序作为动态库接入 Rust，理解数据布局、调用和连续计算 | 《Blink 和 Rust 混合编程》 | [在线阅读](../Blink和Rust混合编程.md) |
| 阅读编译器、类型与访问分析、IR、CPU／GPU 后端和执行框架 | 《B 语言源码分析》 | [在线阅读](../B语言源码分析.md) |
| 通过引擎案例学习场景、几何、光照与资源交接 | 《BLEngine 源码分析》 | [在线阅读](../BLEngine源码分析.md) |

刚接触 B，可以先读《并行编程的艺术》；接入 C／C++ 时读该书第十四章，Rust 场景转到《Blink 和 Rust 混合编程》，研究实现时再读源码分析。引擎案例书可以独立阅读；BLEngine 尚未开源，完整引擎的构建需要授权取得源码及匹配的 Blink SDK。

## PDF 下载

每本 PDF 均可直接下载：

| 书籍 | 标准版 | 手机阅读版 |
|---|---|---|
| 并行编程的艺术 | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/%E5%B9%B6%E8%A1%8C%E7%BC%96%E7%A8%8B%E7%9A%84%E8%89%BA%E6%9C%AF-B%E8%AF%AD%E8%A8%80.pdf) | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/%E5%B9%B6%E8%A1%8C%E7%BC%96%E7%A8%8B%E7%9A%84%E8%89%BA%E6%9C%AF-B%E8%AF%AD%E8%A8%80-%E6%89%8B%E6%9C%BA%E9%98%85%E8%AF%BB%E7%89%88.pdf) |
| Blink 和 Rust 混合编程 | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/Blink%E5%92%8CRust%E6%B7%B7%E5%90%88%E7%BC%96%E7%A8%8B.pdf) | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/Blink%E5%92%8CRust%E6%B7%B7%E5%90%88%E7%BC%96%E7%A8%8B-%E6%89%8B%E6%9C%BA%E9%98%85%E8%AF%BB%E7%89%88.pdf) |
| B 语言源码分析 | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/B%E8%AF%AD%E8%A8%80%E6%BA%90%E7%A0%81%E5%88%86%E6%9E%90.pdf) | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/B%E8%AF%AD%E8%A8%80%E6%BA%90%E7%A0%81%E5%88%86%E6%9E%90-%E6%89%8B%E6%9C%BA%E9%98%85%E8%AF%BB%E7%89%88.pdf) |
| BLEngine 源码分析 | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/BLEngine%E6%BA%90%E7%A0%81%E5%88%86%E6%9E%90.pdf) | [PDF](https://raw.githubusercontent.com/sgua83/Blink/main/sample/pdf/BLEngine%E6%BA%90%E7%A0%81%E5%88%86%E6%9E%90-%E6%89%8B%E6%9C%BA%E9%98%85%E8%AF%BB%E7%89%88.pdf) |

## 提问与交流

- 使用问题与技术讨论：[GitHub Discussions](https://github.com/sgua83/Blink/discussions)。
- 可复现缺陷或功能建议：[GitHub Issues](https://github.com/sgua83/Blink/issues)。附上最小源码、实际工具版本、命令、CPU／GPU 与驱动信息及原始错误，便于核对。
- 项目、安装和 C++／Rust 集成入口：[返回 GitHub 项目主页](../../README.md)。

书稿和本页导航的权威源由 P4 维护，经现有 GitHub 镜像流程自动同步到 Wiki。后续阅读可收藏本页或[书籍目录](https://github.com/sgua83/Blink/wiki/Blink-Books)，既有书页地址保持可用。
