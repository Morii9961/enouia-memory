# Codex 交接：MCP-4 纠正报告已持久化并由 System 自动消费

日期：2026-11-04，Asia/Shanghai。Morii 要求 Claude 完成当前片后停止，推送并合并
PR #83，再交给 Codex。本文件交接工程事实和后续顺序，不替代 Morii 的新指令。
配套：[启动提示词](CODEX_MCP4_START_PROMPT_20261104.md)。本机路径和工具位置在
被忽略的主目录 `.local/handoff/CODEX_MCP4_LOCAL_20261104.md`。

## 1. 从哪里接

| 项目 | 交接时状态 |
| --- | --- |
| 仓库 | `Morii9961/enouia-memory` |
| 合并前分支 | `codex/captured-source-lifecycle`，PR #83 |
| 最新实现提交 | `f42f3b1`（ADR122），前一片 `5f6c147`（ADR121） |
| 合并 | Morii 于 2026-11-04 明确要求合并；PR #83 合并进 main 后，main 即新基线 |
| 当前集成面 | `f866137f3eb3150aec183d4bf2a976f2bc7383acec27bad70dafc1c35be2cbd2` |

启动时用 `git log origin/main` 核对 PR #83 的合并提交是否已在 main。若在，从最新 main
新建分支；若不在，说明情况并停止，不要从旧 `0d338f1` 重做。旧交接
[CLAUDE_MCP4_HANDOFF_20261103](CLAUDE_MCP4_HANDOFF_20261103.md) 是历史输入；Claude
期间的逐片记录在 [CLAUDE_MCP4_CONTINUATION_20261103](CLAUDE_MCP4_CONTINUATION_20261103.md)。

## 2. Claude 本轮完成的两片

| 提交 | 内容 | 证据 |
| --- | --- | --- |
| `5f6c147` | ADR121：`agent_correction` closed delta 进入既有 witnessed Owner chain；`report_saved` 原键回执；先授权后查回执；目标被 System 修订后，按当前授权的目标历史返回原回执；改包冲突；每目标 32 条；OutcomeUnknown 锁存原包 | [报告](../validation/MCP-4-agent-correction-intake.md)：全量 909 passed / 0 failed / 4 ignored，strict Clippy，67 schemas |
| `f42f3b1` | ADR122：captured worker 通过独立 `CorrectionAnalyzer` 端口把每条已保存报告分析一次；接受的用户原话成为普通 target-bound `correction` 修订，由既有 evaluator/`apply_batch` 决定；Host 新增可选 `start_worker_with_corrections`；修复 target-bound 工作遇 Stale 时的无限重试 | [报告](../validation/MCP-4-correction-consumption.md)：全量 913 passed / 0 failed / 4 ignored（分三段跑完，原因见报告），strict Clippy |

## 3. 当前能力的准确含义

- `AgentCorrectionReporter::submit` 成功只表示报告已保存（`report_saved`），不是已纠正、
  已排队或已交主人审核。报告里的 Agent 文字始终是不可信解释，从不成为内容或证据。
- 原包重放：新请求要求 exact current target；已保存的键在目标被修订后，仍可在当前授权
  的 selected history 中找到原修订并返回原回执。删除、撤权、过期、策略变化、范围排除
  先于回执查找。未保存的键对非当前目标返回 Stale。
- System 消费：报告键序选择，每条至多 dispatch 一次；目标/来源被删或策略失效则不分析
  直接 Rejected；Paused 可重试；Unknown/panic/超时/重启终态，不重发；多条语句、引文
  不在来源、change 不是 `correction` 均拒绝。种子绑定 dispatch 时的当前修订（revision+1），
  目标在分析期间被修订则拒绝，不改绑。capture 与 correction 的 dispatch 互不重叠。
- 报告与修订的关联只记在私有 dispatch journal（v1 同路径，新增可选 `report_key`）。链上
  有修订本身和用户原话证据，没有 report_key 引用。
- CaptureAnalyzer v3、`start_worker`、pipe-2/5/6、producer status 含义不变。含
  `agent_correction` delta 的根不能用旧二进制打开。
- 没有新的 MCP 工具或传输；没有真实纠正模型/适配器；没有 forget-hide。

## 4. Codex 下一步建议（按顺序，等 Morii 指令开始）

1. **纠正报告的显式 MCP 服务/版本化传输**（`memory_correction_report`）。参照 ADR118/119：
   独立的 AgentCorrectionPermit，与 read 身份配对在同一租约 Vault/同一 Agent；新
   capability/profile，不改变 pipe-5（只读）和 pipe-6（notes）的含义；成功只回
   `report_saved`；未知结果不回工具结果、断开并保留完整 MCP Value。需决定是新 pipe 还是
   在 notes 服务上做版本化扩展，先冻结 ADR/合同。
2. **`memory_supersede_report`**：以前为真、现在变化，区别于纠正（预计 change=`update`）。
   可复用 ADR121/122 的结构，但键空间、capability、回执状态和 analyzer 判定要独立。
3. **`memory_forget_request`**：只记录请求。forget-hide（立即可逆隐藏）仍待 Morii 决定，
   不要实现隐藏；如需推进，只就范围/期限/恢复/撤权规则向 Morii 提具体问题。原生 G08
   Owner 动作不能被 Agent 报告借用。

## 5. 未完成和待决定

- 待 Morii 决定：forget-hide；真实纠正模型/Provider、预算与外发范围；Runtime 是否以及何时
  采用 `start_worker_with_corrections`。
- 未实现：报告→修订的链上关联；journal 丢失后可能重复分析（与 captured 作业同样风险）；
  supersede/forget；MCP 传输；MCP-5。
- 已知不稳定：`automatic_loop::continuation_cases::revocation_cancels_inflight_and_prevents_late_publication_or_refunds`
  在一次满载全量运行中失败（OutcomeUnknown vs Revoked），单独运行 3/3 及之后全量均通过。
  属旧 continuation worker，未修改，未证明已修复。
- 私人 Vault、真实客户端配置、生产部署、云端主权/迁主和 Runtime adoption 均未验证。

## 6. 工作方式提示

- `cargo test --workspace` 在首个失败目标处停止，计数请用 `--no-fail-fast`；会话暂停会
  杀掉后台长测，被杀时日志无 `EXIT` 行，需补跑剩余包与 doc-tests。
- 改动 surface glob 下的 Rust 后，运行 `python tools/integration/runtime_surface.py --write`
  并在同提交登记 `docs/integration/RUNTIME.md`。schema/validator 改动才跑 Python 检查。
- 测试夹具：`crates/enouia-memory-host/tests/agent_correction.rs` 有 `revise`（合法 System
  修订）、`Corrections`（脚本化纠正分析器）；端到端见 `owner_runtime_capture.rs` 末尾。
- 手动项目状态两文件只在 Morii 明说“生成项目状态快照”时更新。
