# P4提交与中央Git镜像

P4 `//BLanguage/main` 是研发权威。开发者使用本人P4账号及独立stream client，通过 `Submit.ps1 -Change <编号>` 提交；VERSION随同一次提交递增。GitHub `sgua83/Blink` 的main是已提交源码的单向镜像，不将GitHub修改自动导入P4。

## 中央发布

GitHub凭据只保留在GUA／192.168.10.5的Gua用户中。各研发节点不再启动发布，也不需要登录GitHub。中央Windows计划任务 `Blink.P4GitMirror` 每天北京时间09:00和21:00运行PowerShell 7的 `PublishGit.ps1 -P4Client BLanguage_Gua`，任务不配置失败重启；错过时间由StartWhenAvailable在可运行时补执行。运行身份为现有Gua交互式用户，机器及其用户会话、P4与GitHub网络是实际运行条件；任务不保存或分发账户密码。

所有者需要即时发布时，在GUA运行相同入口：

```powershell
pwsh -NoProfile -File B:/BLanguage/Tools/PublishGit.ps1 -P4Client BLanguage_Gua
```

脚本默认持有 `B:/BLanguage/Saved/GitPublish/Mirror.git` 独立裸仓库；该仓库只保存发布对象和引用，不使用研发工作树及其Git索引。可通过 `-MirrorDirectory <目录>` 指定发布仓库。OS文件句柄在本次调用期间拥有同一发布目录，结束即释放；不留下阻止下一次调用的锁状态。任务也设置为已有实例运行时不启动第二实例。

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
| 王岩／192.168.10.54／DESKTOP-75P7CSU | wangyan／BLanguage_wangyan | D:/BLanguage | //BLanguage/main/...的write |

项目级 `.p4config` 选择本人的账号和client，不改变其他项目或全局P4设置。账号票据留在各自用户原生凭据文件，GitHub凭据不复制到这些节点。源码同步不自动表示Rust工具链、Release构建或CPU／GPU实际执行已经完成。
