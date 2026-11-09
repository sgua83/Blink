# P4提交、中央源码镜像与每周正式发行

P4 `//BLanguage/main` 是研发权威。开发者使用本人P4账号及独立stream client，通过 `Submit.ps1 -Change <编号>` 提交；VERSION随同一次提交递增。GitHub `sgua83/Blink` 的main是已提交源码的单向镜像，不将GitHub修改自动导入P4。

## 中央发布

GitHub凭据只保留在GUA／192.168.10.5的Gua用户中。各研发节点不再启动发布，也不需要登录GitHub。中央Windows计划任务每天北京时间09:00和21:00运行PowerShell 7的源码镜像入口：`Blink.P4GitMirror`负责每日09:00及周一至周六21:00，`Blink.WeeklyRelease`负责周日21:00先调用同一`PublishGit.ps1`再构建正式发行，任务不配置失败重启；错过时间由StartWhenAvailable在可运行时补执行。运行身份为现有Gua交互式用户，机器及其用户会话、P4与GitHub网络是实际运行条件；任务不保存或分发账户密码。

所有者需要即时发布时，在GUA运行相同入口：

```powershell
pwsh -NoProfile -File ./Tools/PublishGit.ps1 -P4Client <本机实际client>
```

源码镜像脚本默认持有工程下 `Saved/GitPublish/Mirror.git` 独立裸仓库；该仓库只保存发布对象和引用，不使用研发工作树及其Git索引。可通过 `-MirrorDirectory <目录>` 指定发布仓库。OS文件句柄在本次调用期间拥有同一发布目录，结束即释放；不留下阻止下一次调用的锁状态。任务也设置为已有实例运行时不启动第二实例。

## 一次调用的实际交接

1. fetch远端main，读取该提交的 `P4-Change` 和 `P4-Depot`；远端已确认提交是唯一已发布水位。
2. 固定本轮P4最新已提交changelist，读取水位之后的所有BLanguage提交，不区分来自PA1、王岩或其他节点。
3. 按P4提交顺序，用各文件的精确depot revision生产Git blob与tree。每个Git提交标明原P4编号与用户，VERSION直接来自P4；两本台账精确排除，已有Git树中的台账条目也移除。未提交文件、暂存文件和本地构建物没有发布入口。
4. 全部候选形成后，按正常快进规则一次push；读取远端引用确认实际结果。脚本不强推，不执行P4 sync／revert／submit，不改变研发工作区。

一次失败记录真实错误并返回非零，不循环重试、不创建失败队列，也不推进远端水位。下一次定时或手动调用重新读取远端main，因此会覆盖上次以及此后全部未发布的P4提交；即使上次push已完成但确认响应失败，也依据远端事实避免重复发布。本地候选对象与日志不代表GitHub已经发布。

日志为 `Saved/GitPublish/Mirror.git/publish.log`。`PREPARED`只代表候选，`PUBLISHED`才表示本次远端确认；`UNCHANGED`表示本轮远端水位已经等于P4快照。计划任务的LastTaskResult记录本次进程退出码。P4成功、Git发布与消费者采用分别成立。

## 四本书的 Wiki 阅读入口

`PublishBooksWiki.py` 只将 `sample/并行编程的艺术.md`、`sample/Blink和Rust混合编程.md`、`sample/B语言源码分析.md`、`sample/BLEngine源码分析.md` 的完整正文发布到 [GitHub Wiki](https://github.com/sgua83/Blink/wiki)。中文／英文书籍目录与侧栏提供阅读入口；书稿仍为中文。图片引用固定 Git 快照的公开资源，PDF 链接指向同一快照中原有成品，不向 Wiki 复制其他文档、规范、架构或内部双台账。书稿中的其他公开源码／文档链接仍指向原仓库；内部台账链接明确注明不公开，并提供公开 Issues 反馈入口。

同步源是中央 `Mirror.git` 中远端 main 已确认的公开历史，选择最近涉及书稿、导航源、README、图片、PDF 及正文所引资源的提交，标明该快照的 VERSION、P4 编号与固定 Git 来源；无关代码提交不重复生成同一书稿。Wiki 是开发版书稿入口，不等同于正式发行 SDK；目录另外提供正式 Releases 链接，MD、配图与源码索引随实现实时维护；PDF只在周日正式发行时从固定的已提交书稿集中生成，不随日常Wiki或源码提交重建。

Wiki Home 的学习导航权威源为 `sample/wiki/Home.md`，与四本书同样从已公开快照读取，相对链接转为固定源码资源或原 Wiki 书页。修改该源后，下一次现有同步更新 Home 标记块；经用户授权移除初始默认欢迎语。Home、B语言书籍及侧栏只有 `BEGIN Blink books navigation`／`END Blink books navigation` 包围的导航由脚本管理，其他用户内容保留。原 Home／B语言书籍和四本书的地址、Home 原“B语言书籍／Blink books”标题锚点保持可用。完整生成书页带有来源标记，编辑应回到 P4 书稿；遇到同名用户书页时停止，不替换其正文。

GUA 所有者在中央工作区采用已提交脚本后运行一次：

```powershell
pwsh -NoProfile -File ./Tools/InstallBooksWikiSync.ps1 -P4Client BLanguage_Gua -TaskName Blink.P4GitMirror
```

安装器把指定原任务的发布命令和Wiki收口保留在一个ID为`BlinkPublishWithBooks`的原生动作中：try执行原命令，finally继续Wiki，原发布异常不会被Wiki成功覆盖，Wiki非零也使本次任务失败。保留原命令、参数、工作目录、触发器、身份和任务设置，不创建新任务或保存凭据；重新采用读取原发布体，不重复包裹。两个既有任务继续使用原计划：前者覆盖每日北京时间 09:00 及周一至周六 21:00，在原源码镜像结束后同步书籍；后者于周日 21:00 启动，在原镜像／软件发行步骤结束后同步书籍，因此实际 Wiki 更新时间可能晚于 21:00。两个任务的Wiki收口使用相同入口，沿用原Gua用户和原运行级别；安装时的管理员授权不改变计划任务的运行权限。维护周日动作时同一入口指定 `-TaskName Blink.WeeklyRelease`，只采用该原任务。

Wiki 故障独立返回失败，不回滚源码镜像或软件发行。后续运行从实际远端状态继续，不循环重试、不维护失败队列。机器、Gua 会话、P4/GitHub 网络及现有 Git 凭据仍是运行条件；动作每次从 P4 client 解析实际工程根。

所有者即时同步使用相同书籍入口（先运行原源码镜像以取得最新公开对象）：

```powershell
python ./Tools/PublishBooksWiki.py
```

默认 Wiki 裸仓库为 `Saved/GitPublish/Wiki.git`，可用 `--source-mirror`／`--wiki-mirror` 指定实际目录。脚本读取 Wiki 远端当前分支，以临时索引只更新四本书、两份目录及标记导航，正常快进推送并回读远端确认；并发用户修改导致非快进时如实失败，不强推。相同输入生成相同树，第二次报告 `WIKI_UNCHANGED`，不产生空提交。日志 `Saved/GitPublish/Wiki.git/publish-books.log` 记录 `WIKI_PUBLISHED`、`WIKI_UNCHANGED` 或 `WIKI_FAILED`。任务的LastTaskResult对应同一工作流进程，任一真实步骤失败均非零；源码、软件已发布、节点采用与Wiki仍分别以原日志和实际消费者为准，非零不回滚已经公开的发行。GitHub 实际页面、计划任务采用和日志分别确认。编辑权限沿仓库 Settings → General → Features → Wikis 的现有设置，本工具不改变权限。

## 已配置的研发工作区

| 节点 | P4用户／client | 源码路径 | 权限 |
|---|---|---|---|
| PA1／192.168.10.11／DESKTOP-R18H06O | PA1／BLanguage_PA1 | D:/BLanguage | //BLanguage/main/...的write |
| 王岩／192.168.10.54／DESKTOP-75P7CSU | wangyan／BLanguage_wangyan | C:/ProgramData/BLMatrix/Workspaces/BLanguage | //BLanguage/main/...的write |

项目级 `.p4config` 选择本人的账号和client，不改变全局P4USER／P4CLIENT或其他项目client。王岩原未配置查找文件名，本轮设置标准P4CONFIG=.p4config；项目P4IGNORE使用既有.gitignore。账号票据留在各自用户原生凭据文件，GitHub凭据不复制到这些节点。源码同步不自动表示Rust工具链、Release构建或CPU／GPU实际执行已经完成。

共享节点的新增源码放在实际Node／运维安装根的Workspaces子目录，由实际普通用户持有；不向磁盘根散放项目，也不占用Machine、版本化Release或运行态State目录。迁移更新原client Root，原目录只在新工作区验证后清除。

## 每周正式发行与节点安装

每周日北京时间21:00启动正式发行，先镜像源码，再固定远端Git提交和P4编号，沿同一VERSION构建、打包、上传；不根据主线是否正在开发新功能挑选历史候选。流程和自举边界的唯一规则见[版本说明](../Docs/Versioning.md)。

中央入口在实际工程根执行，不固定开发盘符：

```powershell
pwsh -NoProfile -File ./Tools/PublishRelease.ps1 -P4Client <本机实际client>
```

脚本从P4 client Root和Cargo metadata读取实际源码根／target，复用既有target缓存；GitHub写凭据只使用中央用户原有凭据管理器。当前Windows x64软件资产为target/dist/Blink-Setup-windows-x64.exe。周发行另由同一固定Git快照的四本MD、sample/figures、Tools/BuildBooks.ps1及Books.css生成标准版和手机版PDF，八份PDF压为target/dist/Blink-Books.zip；书籍不装入运行SDK。生成使用原PowerShell 7的Markdown转换、Windows中文／代码字体及Edge无窗口原生打印，不依赖临时开发脚本、Python包或已有PDF。版式／浏览器临时文件归本轮target/dist/books，由原发行所有者在最后消费者结束后清理；ZIP只收八份实际PDF。两个资产全部上传后才公开同一发行，VERSION不再次递增。生成或上传失败如实结束并保留定位材料，不拿旧PDF替代。新发行直接把刚生成并已上传的本机安装EXE交给原节点安装消费者，全部实际安装结束后再由发行所有者清理临时源码、package、books、源码ZIP与本机发行资产，不先删再从GitHub下载同份文件；失败保留本轮材料，不清理可复用target。日志仍为target/dist/release.log。

源码生产构建继续使用独立稳定种子。包内源码SDK附带本次Cargo生产的固定B引导静态库，发行者按Cargo package_id与build-script-executed.out_dir取得它，不按mtime选择。包的Cargo构建入口为build-sdk.rs：compiler消费者把同包固定库捕获到本次OUT_DIR后链接，不调用种子重编；关闭compiler不读取该库。源码生产与SDK消费角色由各自包入口明确，installed仅选择原运行SDK位置。原生库归本次发行文件集合，缺少输入直接交回真实I/O错误。

普通研发打包用-BuildOnly：构建当前工作区，release.json的P4／Git身份为空，不上传。版本与平台资产分开，同一版本可以有多个架构／平台包；当前只交付Windows x64，不能把资产命名写成其他平台已支持。

Windows用户从GitHub Releases下载EXE并双击，由系统管理员授权后覆盖安装%ProgramFiles%/Blink。安装器自动配置机器PATH，保留已有项，通知桌面更新；新终端直接运行bc、blink、brun。用户不执行PS脚本，不手动解压或配置环境变量。只保留当前安装，原研发源码、target和BLEngine依赖保持。

现有运维底座直接支持同一EXE：

```text
BLSupervisor put <实际节点> <从GitHub下载的本机EXE> <节点实际临时路径>
BLSupervisor system-exec <实际节点> "<该节点实际下载路径> --quiet"
```

新发行复用仍由原发布者持有的本机安装EXE；独立安装既有发行时，中央普通用户从GitHub取得该已发布EXE一次。原InstallNodes只消费调用者交入的本地包，通过现有BLSupervisor put分发到节点的实际普通用户TEMP；节点无需外网、代理或脚本环境。特权入口只执行本地EXE的--quiet，不从SYSTEM发起GitHub网络请求。静默安装真实返回退出码及安装／PATH结果，不弹UAC。随后普通用户执行同一EXE的--refresh-environment，向其桌面通知环境更新，再清理本次下载文件。

中央自动安装向PublishRelease.ps1传入-NodeRegistry <现有Matrix节点登记接口>，按真实BLanguage P4工作区Owner／Host及项目关系选取消费者。BLSupervisor从本机29083实际控制进程取得，默认使用原生维护身份配置；也可显式传入-Supervisor、-MaintenanceConfig及-MaintenanceTargets。不固定节点或源码盘符，不分发GitHub写凭据、不新增服务。

已发布同一版本时不重复构建或发包；各节点独立处理，一台失败继续其他节点，最后汇总真实失败，不重试。安装器遇到相同发行记录及VERSION时报告ALREADY_INSTALLED并确认PATH；源码发布、节点安装和实际命令可用分别记录。

## 已结束批次的中间文件清理

在实际源码工作区运行 Tools/Clean.ps1 -Path <工程相对路径>。默认工程根来自脚本所在目录，也可用 -Root 明确指定；路径必须位于该工程的Saved子目录，或Cargo metadata声明的target_directory及其子目录。输入、WhatIf预览及返回的Paths均使用工程相对路径，返回的Root为`.`，表示这些路径共同采用的工程根；不将盘符或机器目录写入结果。使用原样路径，不展开通配符。只有选择Saved以外的构建输出时才读取Cargo metadata；清理Saved不依赖Cargo启动。先加 -WhatIf 查看将删除的文件数、逻辑字节及具体路径，再去掉该参数执行。返回值分别记录已选／已删文件数与逻辑字节，以及所在卷实际空闲前后值；压缩文件的逻辑大小不等于实际回收空间。

    ./Tools/Clean.ps1 -Path Saved/Development/FinishedBatch/Output -WhatIf
    ./Tools/Clean.ps1 -Path Saved/Development/FinishedBatch/Output

没有可清理对象时，显式传入空集合；脚本返回零选择、零删除和空Paths，不执行删除或读取Cargo metadata：

    ./Tools/Clean.ps1 -Path @()

已确认无消费者的项目构建输出同样明确选择，不连带删除其他构建档位或仍有效的缓存：

    ./Tools/Clean.ps1 -Path target/debug -WhatIf

研发批次中混合保存源码输入、日志和编译产物时，只传入已确认可回收的精确输出文件或目录，不因目录体积大而删除整批输入。

由原任务所有者确认整批实际验证已成功、最后消费者已结束，再明确选择对应临时脚本／输入／中间产物。可以传入多个明确文件或目录；目录才递归删除。脚本不按日期、后缀或文件大小推测生命周期，不维护另一份批次状态。未完成或失败调查的材料、必要原始日志和仍有效的构建／依赖／对象缓存按真实用途保留；不清空Saved根，不选择Git发布目录、正式产物或仍有消费者的工作区。

Windows深层生成文件通过原生长路径名称取得同一个文件或目录，预览与结果仍使用工程相对路径，不因传统路径长度限制将存在的文件误报为不存在。删除使用LiteralPath，不默认Force；重解析点不能将删除重定向到所选生成目录范围之外，错误直接终止并保留真实错误，不重试或假称成功。此入口不停止进程、不改源码／P4状态／权限，不运行发行、安装、Git镜像或PDF生成。
