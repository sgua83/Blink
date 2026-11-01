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

脚本从P4 client Root和Cargo metadata读取实际源码根／target，复用既有target缓存。GitHub写凭据只从中央用户现有Git凭据管理器取用，不打印、不落盘。完整包为target/dist/Blink-windows-x64.zip，GitHub Release同时提供Install.ps1。所有上传完成后才公开发行，正式发行版本不二次递增。失败日志在target/dist/release.log，本次未完成输入保留定位；成功后清理打包临时源码、展开目录和本机压缩包，不清理可复用target。

普通研发打包可用-BuildOnly：只编译当前工作区，release.json中P4和Git身份为空，不上传；不能将它当作正式已提交发行。

现有运维底座可向实际节点执行管理员安装：从对应GitHub Release下载Install.ps1，用Windows PowerShell运行，传入-Version <四段版本>和本次临时下载目录。无参数版本选择最新正式发行。中央自动安装向PublishRelease.ps1传入-NodeRegistry <现有Matrix节点登记接口>。入口按真实BLanguage P4工作区的Owner／Host及节点工作区关系选取消费者，用登记的节点身份与地址执行维护；既有BLSupervisor客户端从本机29083控制端口的实际进程取得，默认使用其原生维护身份配置。不在代码里写死机器清单或源码盘符。调用方也可显式传入-Supervisor、-MaintenanceConfig及-MaintenanceTargets以使用现有其他维护入口。

Install.ps1公开下载不需要GitHub写凭据。其-ApplyPackage入口由已有管理员维护者消费已经下载的本次包，是普通下载与ProgramFiles写入的真实权限交接；不会触发UAC或要求例行人工确认。普通用户无安装权时准确返回已下载包和所需维护入口，不假称安装成功。正式目录为%ProgramFiles%/Blink，只有当前版；研发源码、原target及BLEngine源码依赖不改。

已经发布同一版本时不再构建或发包，但仍沿现有维护入口检查消费者的当前安装；安装记录已经对应该Git提交和VERSION的节点直接报告ALREADY_INSTALLED。各节点独立处理，一台失败继续其他节点，最后汇总真实失败，不重试。
