# Codex 新 session 启动提示词：接续 Memory MCP-4

下面正文可直接复制给 Codex。请在仓库主目录或从最新 main 新建的 worktree 启动。

---

你接手 Enouia Memory 的 MCP 后端接续工作。Claude 已完成 MCP-4 纠正报告的持久化
（ADR121）和 System 自动消费（ADR122），PR #83 已按我的要求合并进 main。目标仍是自动
日常记忆，人工纠正只是可选控制；不要把不确定结果变成必须由我逐条处理的队列。

先读：

1. 当前 AGENTS.md；保留原文件。
2. `docs/handoff/CODEX_MCP4_HANDOFF_20261104.md`，这是本次主交接。
3. 本机说明 `.local/handoff/CODEX_MCP4_LOCAL_20261104.md`（仓库主目录）。缺失就说明缺失，
   不猜私人数据或工具路径。
4. ADR115–122，特别是 ADR118、119、120、121、122；合同 AGENT_CORRECTION_NATIVE_V1、
   AGENT_CORRECTION_INTAKE_V1、AGENT_CORRECTION_CONSUMPTION_V1、AGENT_NOTE_MCP_V1、
   AGENT_NOTE_PIPE_V1、CAPTURE_ANALYSIS_V3、MCP_AGENT_READ_V2。
5. 对应验证报告和交接列出的实际源码，不要只读摘要。

开工前核对 cwd、branch、HEAD、origin、status、worktrees 和并行工作；确认 PR #83 的合并
提交已在 origin/main（含 `f42f3b1`）。从最新 main 新建接续分支，不要从旧 `0d338f1` 或
已合并的旧分支继续堆叠。先列本片 owned-file allowlist，保留他人修改，不 reset/clean。
不要等额度用尽才写交接。

首项任务（等我说开始后进行）：**纠正报告的显式 MCP 服务/版本化传输**
（`memory_correction_report`）。先冻结最小 ADR/合同，再在同一片编码、按风险验证、提交。
参照 ADR118/119：独立 AgentCorrectionPermit，与 read 身份配对在同一租约 Vault 和同一
Agent；新 capability/profile，不改变 pipe-5（只读）和 pipe-6（notes）的含义；成功只回
`report_saved`；未知发布不回工具结果、实际断开并保留完整原始 MCP Value。不要从
NotePermit/read/MemoryPropose 推导纠正权，不调用 NativeMemoryOwner。

后续路线：`memory_supersede_report`（以前为真、现在变化，区别于纠正），然后
`memory_forget_request`（只记录请求）。以我的接续指令为准，不自动跨到 MCP-5 或新 MV 阶段。

硬边界：

- 只做 Runtime 采集，不开放 conversation_capture 或外部 Agent 宿主 hook。
- memory_note 是可选提示，不是来源；纠正报告是不可信解释，必须追到真实 Runtime 捕获的
  UserStatement 原话；Agent 文字从不成为内容或证据。
- forget-hide 尚未获我选择，不实现立即隐藏，不把历史需求中的提案当批准；需要时只就
  范围/期限/恢复/撤权规则提具体问题。
- 不打开私人 Vault、聊天导出、备份、恢复材料或凭据；只用新建合成根和自有进程。
- 不接真实模型/Provider/embedding；模型、预算和外发范围需要我的独立决定。
- 不改 Runtime 仓库、pin、真实 MCP 配置或共享外壳，不读 Activity/Usage。
- 不部署、不迁主、不双写、不创建自动化、不刷新手动项目状态快照；合并需我单独指示。
- OutcomeUnknown 保留原包原键，停止连接/授权，显式恢复；不自动重发、重连或换键。
- 不把旧二进制用于含 AgentNote/AgentCorrection delta 的根。

验证遵循当前 AGENTS：按变更面选择定向测试；schema/validator 改动才跑 Python schema
cross-check；集成面变化同提交更新 runtime-surface.json 并登记 RUNTIME.md。合并基线最近
一次全量为 913 passed / 0 failed / 4 ignored（f42f3b1）；你的改动之后须重新取证，不要
沿用。合成测试不等于真实模型/私人库/Runtime 产品采用或主人验收。

每片完成后检查公开内容，提交并推送到你的接续分支，开或更新草稿 PR。保留既有
co-author；你新贡献的提交使用 AGENTS 规定的 Codex trailer，不把他人实现误署名。
Runtime 的并行工作以它自己的交接为准。
