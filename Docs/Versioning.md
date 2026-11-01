# Blink 软件版本

根 `VERSION` 是 Blink 软件版本的唯一事实源，采用`用户段.主版本.次版本.补丁号`四个十进制整数；当前用户段为`0`。它是语言工具链与执行框架的软件版本，不使用 P4 全局 changelist 或 Git hash 充当版本号。源码主线持续研发，正式发行身份以GitHub Releases的实际发布记录为准。

## 提交公式

设P4最后成功提交版本为`(U,M,m,p)`，每次正式提交计算一次：

| 本次变更 | 下一版本 | 提交选项 |
|---|---|---|
| 修复、性能、内部重构、文档或台账 | `U.M.m.(p+1)` | 默认 `patch` |
| 向后兼容的新功能 | `U.M.(m+1).0` | `-Bump minor` |
| 不兼容语言规则或公共接口变化 | `U.(M+1).0.0` | `-Bump major` |

首段U由用户指定，普通提交原样继承已提交首段；patch、minor、major均不得改变它。当前固定为0，只有用户明确要求提升并指定新值时，才可由调用方传入`-UserStage <用户指定值>`；不得根据提交次数、不兼容变更或自判成熟度提升。后三段仍按本次Bump计算，提升用户段不隐式重置它们；用户另有要求时以用户指定版本为准。软件编号与实际稳定发布分别记录。

开发者根据本次实际语义选择升级级别，脚本不猜测描述；默认补丁自动加一，不逢十进位。其他项目提交不影响 Blink 版本。

将任务文件放进实际 pending changelist 后，在对应 P4 用户／client 的工作区运行：

```powershell
powershell -NoProfile -File Tools/Submit.ps1 -Change 12345
```

编号替换为本次实际编号；兼容功能增加 `-Bump minor`，不兼容变化增加 `-Bump major`。正式项目提交统一使用该入口。脚本从 P4 已提交 `VERSION#head` 计算，将版本与本次文件同一 changelist 提交，不读取本地候选值作为上次成功版本，不在提交后再补一笔版本提交。

提交被拒绝时没有新的已提交版本；候选与真实错误保留，解决原冲突后再次调用入口重新计算，不自动重试。并发由同一 VERSION 文件的正常 P4 revision／resolve 处理，不建立计数服务或额外锁。其他任务在本 client 打开的 VERSION 不被覆盖。直接运行 `p4 submit` 不经过脚本，工程约定禁止绕过正式入口；没有安装服务器 trigger 或后台监控。

任意节点成功提交后，由GUA／192.168.10.5的中央计划任务每天09:00和21:00调用 `Tools/PublishGit.ps1`，提交方不再启动Git发布，不需要GitHub凭据。每次读取GitHub远端main的P4-Change水位，覆盖其后所有节点的已提交变更；一次失败不推进远端水位，下次独立运行自然包含上次未发布范围。Git复制同一已提交VERSION，不再次递增，失败不回滚P4。两本台账只留P4。安装与操作见 [中央镜像发布](../Tools/README.md)。

## 构建、缓存与兼容身份

`bc --version`、`blink --version`、`brun --version` 及 `blanguage::VERSION`／`blcompute::VERSION` 读取同一构建版本。已发布 Git 源码可离线构建，不要求查询 P4、Git 或网络。Rust Cargo 包的既有 `0.1.0` 是内部包元数据，不作为 Blink 软件版本。

VERSION 更新工具内嵌显示，但不加入 BLINK_COMPILER_ID 或原生框架输入身份；仅版本或文档变化不使 B 对象／入口缓存全部失效。实际实现、源码依赖、公开合同、目标、工具链及配置继续按原规则失效。软件版本不替代 BLCO／BLRI 等实际格式和 ABI 身份，不承诺不同版本的产物一定兼容。

## 每周正式发行

只保留P4研发主线。中央机每周日北京时间21:00先执行当轮源码镜像，再固定远端已确认Git提交及其P4快照，Release编译通过后打包发布。首次计划2026-11-01；21:00是启动时间，不是构建、上传或节点安装完成时间。没有新提交不重复发行；小bug与功能修改累计到周发行，不按提交次数逐个发包，不建立候选池、评分或稳定维护分支。

正式发行沿用快照中的VERSION，不再次递增。编译失败如实结束，沿原研发流程修复并通过Submit.ps1提交；修复后再针对新快照构建。新功能未完成时说明当前限制，不以所有长期Issues清零作为发行条件，也不把Release构建成功写成所有能力已完成或无bug保证。

PublishRelease.ps1从实际P4 client Root及Cargo target_directory取得路径，不固定任何开发盘符。正式构建输入是本轮Git镜像的精确快照，临时源码在原target/dist/source，输出复用原target；包为target/dist/Blink-windows-x64.zip。该包包含同源的bc、blink、brun、源码SDK、标准库及安装入口。源码SDK编译运行框架仍需要Rust MSVC、C++构建工具和Windows SDK；GPU使用实际系统Vulkan驱动。发行包上传sgua83/Blink的GitHub Releases，标签为v<VERSION>，关联原Git提交。Git main源码镜像与正式发行分别记录。

-BuildOnly是研发打包入口，只构建当前工作区并标明working tree，不发布、不宣称已提交快照；正式定时入口不使用此选项。

## 节点安装与构建输入输出

各节点（包括中央机）统一从GitHub Releases下载同一个Windows x64发行包，通过现有运维底座的管理员入口安装到%ProgramFiles%/Blink。安装包读取节点自己的环境，不嵌入中央机源码绝对路径。只保留当前安装，新版覆盖旧版；安装器直接从压缩包写入正式目录，成功后删除本次下载包，历史发行包留在GitHub，不在节点积累版本目录。

bin持有三个正式工具，安装根中的VERSION、Cargo文件、crates和library组成实际SDK输入；源码由同一发行包交付。安装器只移除旧包清单内已经退出的新版本文件，不清理工程、target或用户资料。机器PATH增加正式bin，新终端可直接使用；原研发命令中的显式工具路径不变。

源码SDK的普通构建不启用installed特性，仍从原源码根读取SDK并使用原SDK target。正式发行启用blanguage/installed：SDK从%ProgramFiles%/Blink读取，临时单元和运行框架输出写当前工程target。正式Compiler会话持有安装VERSION的真实只读使用句柄，直到最后共享消费者结束；安装器取得同一文件的排他使用权后才覆盖。占用失败明确结束，不杀进程、不伪造安装成功，不在使用中覆盖输入。部分覆盖失败撤销安装VERSION，避免把残缺目录作为可用正式SDK。

中央发布与各节点安装分别输出PUBLISHED及INSTALLED；离线或安装失败不代表其他节点未发布，也不能宣称全体已采用。多节点安装沿已有BLSupervisor system-exec和BLAdmin权限边界，不增加服务或分发GitHub写凭据。消费者由Matrix真实节点登记与BLanguage P4工作区Owner／Host及项目关系取得，不固定节点清单；没有新发行时仍可使之前离线的消费者安装当前版本，已对应相同Git提交及VERSION的节点不重复下载。各节点独立执行，单节点失败不跳过其他节点，最终失败如实汇总。

## 正式工具与后续自举

正式发行（本项目采用的稳定发行身份）是按每周流程编译并发布的确切版本，不保证全部新功能可用。当前编译器仍由Rust实现；B自举及CPU/GPU协作编译各按真实实现记录。自举使用正式安装的工具与库作为本轮输入，研发源码是待编译内容，新工具写工程target；本轮输入消费者结束后，才由独立PowerShell安装动作覆盖正式目录。开发产物不能成为自己唯一的构建前提。
