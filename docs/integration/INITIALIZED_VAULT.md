# 正式初始化后的接入与私人数据保留

日期：2026-10-29。本文是接入步骤和出口要求，不宣称真实库已经建立或 Runtime
已经采用新布局。当前 ADR104 实现与验证只覆盖临时合成根。

## 同一份数据，两个职责

Memory 拥有原件、来源、记录、形成规则、权限、回执、恢复和投影。
Runtime 是产品客户端，通过采用的 Memory 原生接口使用同一个被明确选择的根；
不复制第二套记忆，不直接读磁盘 JSON，不把初始化检查 bundle 当查询数据库。
初始化与日常新增都经过同一形成通道。

## 长期目录与清理规则

实际目录由主人选择，只在本地配置/交接中保存；公开文档使用占位符：

```text
<OWNER_DATA_HOME>/
  DO_NOT_DELETE.USER_DATA
  .enouia-owner-data.json
  initialization-inputs/manifest.json
  initialization-inputs/user-supplied/
  vaults/<vault-id>/                 # 后续由原生入口创建
  backups/                          # 后续按已验证的恢复契约建立
```

数据区本身不是 Vault，标识也不会自动开库、导入、调用模型或外发。
`vaults`、`backups` 在本图中是规划，不代表已创建或已有可恢复副本。

- 与 Git 工作树、`target`、`.local`、系统临时目录和合成夹具根分开。
- 真实原件、记忆正文、会话、模型响应、回执、私人运行日志及备份均不上传 GitHub。
  仓库只保存代码、Schema、契约、无私人数据的报告和合成夹具。
- 两种保留标识表示用户数据。开发清理不得删除该目录、子目录或包含它的上级目录；
  仅清理明确列入白名单的自有构建/测试产物。忽略文件不是垃圾清理许可。
- 原件、来源和初始化输入同样需要保留；不能因尚未完成形成就当临时资料清掉。
  主人明确选入的独立记忆随正常初始化直接形成，不额外建立待审队列。
- Memory 管理的投影虽可重算，也不能由开发清理脚本在活动 Vault 内随意删除。
- 受保护 ACL 限制其他账户访问；标识和清理约定不是不可删除锁，也不替代独立备份。

## 真正接入的顺序

### 已核对的并行依赖

[PR68](https://github.com/Morii9961/enouia-memory/pull/68)、提交
`34709b9d2abcb60188c0e3488b15f40c33d6ee16` 已提供 G08 原生合成实现；
[ADR103](https://github.com/Morii9961/enouia-memory/blob/34709b9d2abcb60188c0e3488b15f40c33d6ee16/docs/adr/103-owner-correction-subject-binding-proposal.md)
要求新建空 `automation-segmented-owner-v1` 根，显式提供 `owner_subject_id`，
创世提交见证 `descriptor_hash`。不能改头升级旧根，也不能重新绑定主体。
记录业务字段仍沿用 ADR93；读取 selection 为 v4，process headers 为 v2。

初始化 PR67 已整合 MCP 的 `14125f5cae8df2277d16c167a165b7eae3c1974c`
（包含上述 Owner 依赖），只接受新建 Owner 根。导入/附件入口保留，初始化日志
绑定描述符哈希，拒绝不匹配的主体；初始化、重开、原键回执恢复和七投影删除均在
Owner 合成根验证。不能把现有合成根改名交付为正式根。

G08 的 correction 需要已存在的目标，并限于绑定的主人主体；它不是初始化时
新建独立条目的入口。主人明确提供的初始化条目应经正常初始化的原生来源入口
直接形成；兼容片的显式 `OwnerProvided` 解析模式已覆盖此来源形状，不能伪造纠正目标或
额外建立逐条待审队列。主人主体与其他人物/角色实体必须分开，不能从名字猜 ID。

### 实施步骤

1. **先具备生产入口和恢复出口。** 当前 initializer 入口明确为 `create_synthetic` /
   `open_synthetic`，分析是脚本输入。正式入口须绑定真实原生 owner/System 身份、
   明确来源范围、形成分析及可恢复运行；不能只把函数名改成 production。
   主人提供的事实和聊天引文作为初始化来源保全；引文原说话人仍是原说话人。
   原生主人输入/动作所需的能力不能由 System 伪造；G08 后端已有合成实现，
   本片已验证初始化之后可执行独立的原生合成 Owner 动作，Runtime 采用仍另行完成。
   新 segmented 布局还须验证独立备份/恢复；旧布局的 CLI restore 不构成该证明。
2. **明确真实处理参数。** 选择来源/账号/时间范围、主体/项目/敏感度，明确模型、
   目的地/外发、附件处理和默认时区。数据目录的选择不等于这些处理权限。
3. **Memory 创建单独正式新根并初始化。** 不升级旧根；先预检，再按来源时间形成，
   保存原件、输入水位、批次与原键回执。已明确选入的独立记忆也在这一步直接写入。
   未知结果只查原操作，不能换键重发。核对实际覆盖率、来源/字符引文与删除屏障。
4. **Runtime 采用确定版本。** 匹配 Memory pin、锁文件、surface manifest、布局与
   native read capability。正式 Owner 根采用对应 segmented 原生入口、v4 selection
   和 v2 process headers，并重启旧游标；旧 AutomationVault/旧 Workspace 路径
   不能替代该入口，不能绕过版本门槛。
   [目录读取契约](../contracts/UI_V2_CATALOG.md)、
   [投影契约](../contracts/UI_V2_PROJECTIONS.md)和
   [过程读取契约](../contracts/UI_V2_PROCESS.md)定义 Runtime 应采用的输出。
5. **先验收同一根上的六视图。** Formation、记忆格、时间线、Constellation、星图、
   Recall lab 经原生读取显示；再验来源跳转、过滤、锁定/权限变化、删除后刷新、
   Context/Sessions 和日常形成。保留截图与实际根/版本证据，合成结果和真实验收分别报告。
6. **Core-8 明确切换。** 停止旧根的写入和相关作业，处理未决回执，关闭旧 Host，
   再由兼容的新 Host 持有新根的唯一写入租约。Runtime 原生选择具体 Vault 子目录，
   页面只持有句柄/token。沿用当前显式开根契约，不自动猜路径或自动记住/打开根。
   旧库只读保留；不能两根同时充当主库。回退也须先关闭新写入，保全新根新增记录。

当前 PR67 的合成管线通过不意味着上述生产入口、独立恢复、Runtime 采用和
Core-8 已完成。它们必须由对应切片交付，禁止用改目录或复制检查 JSON 代替接入。

对于日常已采集的 Runtime 轮次，形成必须复用原 `SourceRevisionRef` 与采集键。
当前 initializer 尚未提供仅接收已采集修订并入队的 API；这是后续明确接口增量，
不能通过重新包装为导入来源来补洞。MCP 的 Owner 根读取也须采用它自己发布的
兼容接口和版本门槛；本片不把 System 读取入口开放给 Agent。

参见[初始化实际覆盖报告](../validation/Memory-init-pipeline.md)与
[Runtime 接入职责](RUNTIME.md)。
