# Claude 交接：MCP-3 已接通合成通路，MCP-4 完成原生纠正草稿

日期：2026-11-03，Asia/Shanghai。Morii 要求当前 Codex 完成手头任务后停止，
由 Claude 新 session 接手。本文件交接工程事实和后续顺序；不替代新 session 的主人指令。
配套：[启动提示词](CLAUDE_MCP4_START_PROMPT_20261103.md)。本机路径与工具位置在
被忽略的 `.local/handoff/CLAUDE_MCP4_LOCAL_20261103.md`。

## 1. 从哪里接

| 项目 | 本次核实结果 |
| --- | --- |
| 仓库 | `Morii9961/enouia-memory` |
| 主线基线 | `0d338f12f51a859363e0725071bcf43874b492c7`，已整合 #56–#81（PR #82） |
| 实现分支 | `codex/captured-source-lifecycle` |
| 最新实现提交 | `ccbf1b1a96c03360e5124434b0d14597af8c64e7` |
| 草稿 PR | [#83](https://github.com/Morii9961/enouia-memory/pull/83)，相对上述 main 的接续工作；尚未合并 |
| 当前集成面 | `d74ae1000024e580d04bbacd4824cd996984d9856f4d3770fb1b46c7e6e498a7` |

本交接文档另有文档提交，其父提交包含上述实现。启动时重新核对 HEAD、origin、status 和
worktree；不能从 main 直接开工而遗漏 #83。旧的 2026-11-01 交接是历史输入，里面
“Owner wire/worker/memory_note 尚未实现”和旧 PR 链状态已经过时。

## 2. 已完成并逐项提交的工作

| 提交 | 完成内容 | 保存的验证证据 |
| --- | --- | --- |
| `310c206` | 删除来源后，已分析的形成作业能结算；保留实际回执，拒绝未发布语句，后续轮次可继续 | [删除结算](../validation/MCP-3-deleted-capture-jobs.md)，当时 13 项 |
| `17e6df2` | 删除来源时在下次 poll 前取消并 drop 待分析句柄；含真实双线程删除/apply 检查 | 同报告 follow-up，当时 16 项 |
| `237c60d` | ADR115：独立 AgentNotePermit、精确来源/原话准入、无写入草稿 | [提示准入](../validation/MCP-3-agent-note-native.md) |
| `c6ff0cd` | ADR116：提示进入 Owner witnessed commit chain；原键 hint_saved 回执、容量、未知结果与恢复 | [提示持久化](../validation/MCP-3-agent-note-intake.md) |
| `277165e` | ADR117：CaptureAnalyzer native v3；首次分析前持久固定提示键，最多 8 条 / 64 KiB | [提示消费](../validation/MCP-3-captured-agent-hints.md)，41 项定向检查 |
| `06b0e3e` | ADR118：五个初始化只读工具加 memory_note 的独立服务；同 Agent、同已租约 Vault、独立提示授权 | [MCP 服务](../validation/MCP-3-agent-note-server.md)，19 项及 66 schemas |
| `cb1a5e6` | ADR119：pipe-6、保留完整包的 stdio、automation-notes-v1 显式启动器 | [管道和启动器](../validation/MCP-3-agent-note-pipe.md)，24 项定向检查 |
| `ccbf1b1` | ADR120：独立纠正报告授权；精确当前目标和独立授权的 Runtime 用户原话；只准备草稿 | [纠正准入](../validation/MCP-4-agent-correction-native.md)，12 项定向检查 |

Owner Runtime pipe-2、专用可取消 worker 已在基线中，不必重新实现。
本轮还通过实际自有 Agent 进程、pipe-6 和原生捕获队列，把已保存提示交给假 System
分析器，形成一条真正提交的合成记忆；原始 UserStatement 证据保留。这个结果证明本地
合成通路，不能称真实模型、实际产品客户端或私人库已可日用。

## 3. 当前能力的精确含义

- `memory_note` 保存的是可选提示，成功仅为 `hint_saved`。它不采集新来源，不创建形成
  作业，不直接写记忆。既有 pipe-5 仍只有五个只读工具。
- 首次 dispatch 只选精确来源修订和当前 policy 的提示，按 note_key 排序限额。晚到提示
  不改变 pending/terminal 分析，也不会重启模型。完整历史需求里的结构化 title/time/entities
  提示和晚到后重分析，不在本次交付中。
- AgentNote delta 改变闭合提交格式：旧二进制不能打开含该 delta 的根，不要降级。部分
  read hello 含 catalog schema 摘要，需匹配客户端/服务端版本。实际读取 DTO 含义未扩大。
- `CaptureAnalyzer::start(job, source, notes, stop)` 是明确的 native v3 编译接口变化；没有
  静默忽略 notes 的兼容默认方法。真实适配器仍须满足有界、非阻塞、可取消。
- `AgentCorrectionReporter::prepare` 只接受独立授权的当前目标修订和真正 Runtime 捕获的
  User/UserStatement 来源。Agent 的 correction 文本仍是不可信解释。它不持久化报告，
  不回执“已保存/已纠正”，不执行 NativeMemoryOwner，也不在 MCP 上广告新工具。
- `OutcomeUnknown` 保留原包原键，停止该连接/授权；原生显式恢复后才能重新设置并查询/
  重放原请求。服务须实际 disconnect，不能只从借用管道的函数返回 Err。新 stdio 保留完整
  MCP Value，旧桥保留原来的 arguments-only 格式；不保证原始 JSON 空白排版。

## 4. Claude 下一项应做什么

建议先做 **MCP-4 纠正报告的原生持久化与原键回执**，沿用已完成的草稿准入，先冻结最小
ADR/合同，再编码、按风险验证、提交。后续顺序是 System 修订消费、独立 MCP 服务/版本化
传输、supersede 和 forget 请求，不能将当前准备 API 当作完整 MCP-4。

第一项必须解决：

1. 原始请求重新准入后才发布；不能把可复制的 Draft 当 capability。独立纠正授权不能借用
   NotePermit，也不能从 read scope 或 MemoryPropose 推导修改权。
2. 稳定报告键和完整请求 hash；同包同键返回真实原回执，改包冲突。成功只表示报告已保存。
   通过现有单写者链、CURRENT/witness 和恢复机制发布，避免新建独立真相日志。
3. **目标修订变化后的回执重放。** 当前 prepare 要求目标恰好等于当前版本；System 消费后
   目标会变化。因此持久化实现不能每次无条件先走“当前目标相等”而使已保存回执无法找回。
   必须区分新请求准入与原键查询，在当前授权下核实原目标历史，再返回原回执。删除/撤权仍
   优先，不能凭旧回执泄漏已撤销目标。`read_agent` 已有 selected history，可据代码评估复用。
4. 保存报告 ≠ 修正记忆。后续消费者仍需 System evaluator、精确目标和合法提交；保留实际
   source evidence、冲突和修订历史。日常修订应自动完成，人工纠正是可选控制，不能变成逐条
   主人审核积压。当前 CaptureStatement 没有 target，formation_seed 仍设 target: None。
5. 记录未知发布、锁/撤权/过期/目标与来源删除、容量、跨实例与并发重放的实际证据。

这是一份后续实施建议。合并、部署、真实模型、私人根和 Runtime 采用仍以 Morii 新指令为准。

## 5. 待 Morii 决定和仍未完成的事项

- **forget-hide 尚未决定。** [正式 MCP 合同](../contracts/MCP_AGENT_READ_V2.md) 明确将
  forget-hide 留待 MCP-4 决定。此前已向 Morii 提出“先只保存且召回可见”或“立即可逆隐藏”
  的问题，交接时尚未收到答案。不要从历史需求的措辞推定立即隐藏获批。若选择隐藏，还要
  冻结范围、期限、恢复和撤权规则。该问题不阻碍纠正报告持久化。
- `memory_supersede_report` 表示以前正确但现在变化，与错误纠正不同；尚未实现。
- `memory_forget_request` 尚未实现；实际原生 G08 Owner 动作已经存在，但 MCP 不持有
  Owner factory/token/gesture/Consent。不能借它处理 Agent 报告。
- 真实模型/Provider/embedding、Owner 根的持续模型策略与预算/外发、日常实体写入、多来源
  上下文、analysis_unknown 的可选补分析、作业日志容量及 MCP-5 均未交付。
- 私人 Vault、真实客户端配置、生产部署、云端主权/迁主和 Runtime adoption 都未由本轮验证。

## 6. 源码入口与阅读顺序

先读当前 AGENTS，再读 ADR102、111–120 和相关合同。纠正相关优先读：

- `crates/enouia-memory-vault/src/automation/segmented/agent.rs`：独立 permit、草稿准入、
  read_agent/target closure；`mod.rs` 为导出。
- `crates/enouia-memory-host/src/automation/qualified_agent.rs`：Host 生命周期、两类独立
  writer/reporter 和共享的 pinned quote locator。
- `crates/enouia-memory-host/tests/agent_correction.rs` 与 `agent_note.rs`；共享合成夹具
  `tests/support/agent_note_fixture.rs`。
- 后续持久化参考 Vault `note_intake.rs`、`state.rs`、contract `next/agent_note.rs` 和
  `next/segmented.rs`。不要简单复制当前 prepare 的目标相等检查来处理历史回执。
- 形成消费参考 Vault `formation_worker.rs`，Host `automation/captured_worker.rs`。
- wire 参考 Host `mcp/notes/{mod,native}.rs`、`mcp/v2/native.rs` 与实际 `agent_note_pipe.rs`。
- 设计总路线 `docs/design/IMPLEMENTATION_PLAN.md`、`CLOUD_PRIMARY_AND_HISTORY.md`。
  早期 UI backend 需求在本机交接包，含被 ADR102 取代的 Agent 采集内容，只能作历史输入。

## 7. 验证证据如何使用

第四片 `c6ff0cd` 的完整 workspace 检查为 **887 passed、0 failed、4 ignored、107 targets**；
fmt、strict workspace/all-target Clippy、65-schema 检查也通过。第六片 schema 增至 66，
含 15 个 MCP note cases。完整 suite 并非在最新 HEAD 重跑，不要称最新 HEAD 全量通过。

此后的定向记录：第五片 41 项，第六片 19 项，第七片 24 项，第八片 12 项，均通过。
第八片运行的是 `cargo test -p enouia-memory-host --test agent_correction --test agent_note
--locked`，以及 Vault/Host 两包 all-target strict Clippy、fmt、surface、whitespace。
最后一片未改 schema/validator，未跑 Python schema 或完整 suite。文档交接提交只需格式/
链接/一致性检查，不应为它重跑构建。

各次检查不能直接相加作为独立测试总数。测试均使用新建自有合成根、假时钟/假分析器。
早期 sandbox 根夹具因 Windows ACL 报 StorageFailed，尚未进入行为测试；普通权限下通过。
测试工具版本、失败过的编译尝试及未验证内容详见各片 report。本次纠正草稿的额外跨 session
只读审阅因该 session 额度用尽未完成，不能作为已获独立审查的证据。

## 8. 并行工作和停机交接

原 Implement MCP-3 runtime capture session 已因额度限制停在 systemError，最后完成的
工作是只读核对 MCP-4 需求，没有并行实现。本次发给它的草稿审阅请求未完成，不需等待。
它原来的 pipe-2/worker 交付已进入主线基线。

Runtime 的另一 session 已报告收尾：#116（`bbdea44`）采用 Memory `0d338f1` 的兼容性，
#117（`73ca965`）提供新建 SegmentedV1 合成根/read-2，#118（`3a5dc33`）提供 page/stats
前端与授权到期观察，原生 21/21；它也正在交接后停止。这是对方报告，Memory 未独立重验；
以 Runtime 自己的交接和 Git 为准。它明确未采用 #83，也没有 Owner adoption、真实
Provider 或持续 formation。不得把 read-2 采用说成已采用 pipe-6/分析器 v3/Owner worker。
两边本机 session ID 在私有本机说明中，勿写入 fixtures 或业务代码。

Codex 在 `ccbf1b1` 后不再开展功能实现，只提交交接文档。主目录 main、AGENTS 和两个手动
项目状态文件均未改；没有 merge、部署、真实模型、真实客户端配置或私人根操作。已完成
检查均已结束，没有需要保活的测试进程。保留现有 worktree、target 中验证日志和被忽略的
交接目录，不能因 ignored 就清理。旧 preparation snippets 不再是待应用补丁。

新 session 每片先列 owned-file allowlist，保留并行修改，按风险检查并提交。集成面改动在
同提交更新 surface 和 RUNTIME；schema/validator 变化才跑 Python cross-check。手动
PROJECT_STATUS 两文件只能在 Morii 明说“生成项目状态快照”时更新。
