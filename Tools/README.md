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

脚本从P4 client Root和Cargo metadata读取实际源码根／target，复用既有target缓存；GitHub写凭据只使用中央用户原有凭据管理器。当前Windows x64资产为target/dist/Blink-Setup-windows-x64.exe，是包含正式工具与源码SDK的原生自解压安装程序。上传完整资产后公开发行，版本不二次递增。失败日志为target/dist/release.log；成功清理打包临时源码、package目录及本机分发EXE，不清理可复用target。

普通研发打包用-BuildOnly：构建当前工作区，release.json的P4／Git身份为空，不上传。版本与平台资产分开，同一版本可以有多个架构／平台包；当前只交付Windows x64，不能把资产命名写成其他平台已支持。

Windows用户从GitHub Releases下载EXE并双击，由系统管理员授权后覆盖安装%ProgramFiles%/Blink。安装器自动配置机器PATH，保留已有项，通知桌面更新；新终端直接运行bc、blink、brun。用户不执行PS脚本，不手动解压或配置环境变量。只保留当前安装，原研发源码、target和BLEngine依赖保持。

现有运维底座直接支持同一EXE：

```text
BLSupervisor put <实际节点> <从GitHub下载的本机EXE> <节点实际临时路径>
BLSupervisor system-exec <实际节点> "<该节点实际下载路径> --quiet"
```

中央普通用户从GitHub下载已发布EXE一次，再通过现有BLSupervisor put分发到节点的实际普通用户TEMP；节点无需外网、代理或脚本环境。特权入口只执行本地EXE的--quiet，不从SYSTEM发起GitHub网络请求。静默安装真实返回退出码及安装／PATH结果，不弹UAC。随后普通用户执行同一EXE的--refresh-environment，向其桌面通知环境更新，再清理本次下载文件。

中央自动安装向PublishRelease.ps1传入-NodeRegistry <现有Matrix节点登记接口>，按真实BLanguage P4工作区Owner／Host及项目关系选取消费者。BLSupervisor从本机29083实际控制进程取得，默认使用原生维护身份配置；也可显式传入-Supervisor、-MaintenanceConfig及-MaintenanceTargets。不固定节点或源码盘符，不分发GitHub写凭据、不新增服务。

已发布同一版本时不重复构建或发包；各节点独立处理，一台失败继续其他节点，最后汇总真实失败，不重试。安装器遇到相同发行记录及VERSION时报告ALREADY_INSTALLED并确认PATH；源码发布、节点安装和实际命令可用分别记录。
