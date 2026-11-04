# Claude 新 session 启动提示词：接续 Memory MCP-4

下面正文可直接复制给 Claude。请在本机说明指定的实现 checkout 启动。

---

你接手 Enouia Memory 的 MCP 后端接续工作。上个 Codex 已按我的要求完成当前纠正报告
草稿任务并停止，接下来由你继续。目标仍是自动日常记忆，人工纠正只是可选控制；不要把
不确定结果变成必须由我逐条处理的队列。

先读：

1. 当前 AGENTS.md，并比对仓库主目录是否有我的更新指令；保留原文件。
2. `docs/handoff/CLAUDE_MCP4_HANDOFF_20261103.md`，这是本次主交接。
3. 本机说明 `.local/handoff/CLAUDE_MCP4_LOCAL_20261103.md`。若当前 checkout 无该文件，
   查看仓库主目录同一路径；仍缺失就说明缺失，不猜私人数据或工具路径。
4. ADR102、111–120，特别是 ADR116、117、120；合同 AGENT_CORRECTION_NATIVE_V1、
   AGENT_NOTE_INTAKE_V1、CAPTURE_ANALYSIS_V3、MCP_AGENT_READ_V2、UI_V2_OWNER_ACTIONS。
5. 对应验证报告和交接列出的实际源码，不要只读摘要。

开工前核对 cwd、branch、HEAD、origin、status、worktrees 和并行工作。当前实现分支是
`codex/captured-source-lifecycle`，最新实现为 `ccbf1b1a96c03360e5124434b0d14597af8c64e7`，
其后有交接文档提交，草稿 PR #83；主线 `0d338f1` 缺本次八项接续。不要直接从旧 main
重做。确认已有分支/PR 和远端关系后选择最小接续方式；不要改写既有历史或遗漏未合并提交。
先列本片 owned-file allowlist，保留他人修改，不 reset/clean。不要等额度用尽才写交接。

首项任务：**完成 MCP-4 纠正报告的原生持久化、原键回执和恢复**。先冻结最小 ADR/合同，
在同一片编码并按风险验证、提交。它接续已有独立纠正 permit 与 prepare，不是重做草稿。
成功只能表示报告已保存，不能说已纠正。报告仍是 untrusted Agent interpretation，
必须追到真实 Runtime 已捕获的 UserStatement 原话和当前授权的目标证据闭包。

必须解决目标修订变化后原回执查询的问题：新请求要求当前 exact target；但 System 消费
会改变目标，因此原包重放不能无条件先调用只接受当前版本的 prepare。应在当前授权下
验证原目标历史，再按原键/原 hash 返回真实回执；撤权/删除优先。改包必须冲突。沿用已有
单写者 witnessed commit chain、CURRENT 和恢复规则，避免独立真相日志。不要把 Draft
当写权限，不能从 NotePermit/read/MemoryPropose 暗增纠正权，也不能调用 NativeMemoryOwner。

该片完成后报告提交、PR、实际检查和剩余工作。后续路线为 System 自动修订消费、显式
MCP 服务/版本化传输、supersede 与 forget 请求；以我的接续指令为准，不自动跨到 MCP-5
或新 MV 阶段。System 修订应走既有 evaluator/合法提交，区别于原生 Owner G08 动作。

硬边界：

- 只做 Runtime 采集，不开放 conversation_capture 或外部 Agent 宿主 hook。
- memory_note 是可选提示，不是来源；晚到提示不会自动重启已 dispatch 的分析。
- forget-hide 尚未获我选择。可继续独立的纠正工作；不要实现立即隐藏，也不要把历史需求
  中的提案当批准。需要时只就范围/期限/恢复规则向我提出具体问题。
- 不打开私人 Vault、聊天导出、备份、恢复材料或凭据；只用新建合成根和自有进程。
- 不接真实模型/Provider/embedding；模型、预算和外发范围需要我的独立决定。
- 不改 Runtime 仓库、pin、真实 MCP 配置或共享外壳，不读 Activity/Usage。
- 不合并、不部署、不迁主、不双写、不创建自动化、不刷新手动项目状态快照。
- OutcomeUnknown 保留原包原键，停止连接/授权，显式恢复；不自动重发、重连或换键。
- 不把旧二进制用于含 AgentNote delta 的根；native v3 分析器适配必须明确处理 notes。

验证遵循当前 AGENTS：按变更面选择定向测试；schema/validator 改动才跑 Python schema
cross-check；集成面变化同提交更新 runtime-surface.json 并登记 RUNTIME.md。最新实现只有
12 项定向检查和两包 strict Clippy 等结果；887 项全量属于较早提交，不能冒充当前 HEAD
全量证据。合成测试不等于真实模型/私人库/Runtime 产品采用或主人验收。

每片完成后检查公开内容，提交并推送到同一仓库的接续分支，更新草稿 PR，不合并。
保留既有 co-author；你新贡献的提交使用你 session 规定的 Claude attribution，不把你的
新实现误署名为 Codex。协同问题可向既有 session 查证；原 Implement MCP-3 session 已
因限额停止，不需等它的未完成只读审阅。Runtime 的并行工作以它自己的 Claude 交接为准。
