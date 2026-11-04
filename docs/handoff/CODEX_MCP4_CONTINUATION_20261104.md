# MCP-4 Codex 接续记录

2026-11-04，Asia/Shanghai。主交接仍是
[CODEX_MCP4_HANDOFF_20261104](CODEX_MCP4_HANDOFF_20261104.md)。

## 基线与授权

已 fetch 并确认 PR #83 合并提交 `2a6310e` 在 origin/main，包含 `f42f3b1`。
从该 main 新建 `codex/mcp4-correction-transport`，未从旧分支堆叠。
开工时本 worktree 及另两个 Memory worktree clean，既有 worktree 保留。
当前 AGENTS 和主目录本机交接均已读取，AGENTS 未修改。

Morii 随后明确要求开始，并在完成一项后自动接续既定路线。顺序是纠正报告
MCP 传输 → supersede report → request-only forget。允许逐片提交、推送并维护
草稿 PR；没有合并、forget-hide、MCP-5、新 MV、真实模型或 Runtime 采用授权。
目标保持自动日常记忆；不确定分析不变成主人必须逐条处理的队列。

## 已完成

- `07626b4` / ADR123：memory_correction_report、独立 pipe-7/profile、原生连接
  检查和 explicit stdio attach；成功只有 report_saved。未知发布不回工具结果或
  Reply，实际断开、撤销配对授权，完整保留原始 MCP Value，显式原键恢复。
  [验证](../validation/MCP-4-agent-correction-transport.md)：44 个 Host 定向测试、
  2 个合同测试、68 schemas、定向 strict Clippy、fmt/surface/空白检查通过。
- 草稿 [PR #84](https://github.com/Morii9961/enouia-memory/pull/84)，未合并。

## 当前片

ADR124/AGENT_SUPERSEDE_V1 已冻结，原生独立授权/intake、agent_supersede delta、
独立 SupersedeAnalyzer、显式 start_worker_with_reports、pipe-8 和 launcher 已实现。
每条报告至多 dispatch 一次，只有真实用户原话和 update 可进入普通 evaluator。
原有 correction/capture 路径保持类型与语义，三路 dispatch 互斥；无主人审核队列。
第一轮 59 个 Host 定向测试与 70 schemas 通过；另加的 pending supersede 阻止
capture/correction dispatch 测试也通过。完整离线 workspace：948 passed /
0 failed / 4 ignored，114 个目标，含全部 doc-tests，退出 0；workspace/all-target
strict Clippy 通过。源码在完整运行开始后未变更。
详细证据：[本片报告](../validation/MCP-4-agent-supersede.md)。

完整日志是 `target/mcp4-supersede-workspace-tests.log`，随后 Clippy 日志是
`target/mcp4-supersede-workspace-clippy.log`，均已完成。
ADR125 和 AGENT_FORGET_REQUEST_V1 已提前冻结为下一片的两份文档，尚未实现，
须留到下一片编码/验证后一起提交；不要在 supersede 提交中仅广告未交付的工具。

本片通过后先公开内容检查、提交/推送/更新草稿 PR，再接 request-only
memory_forget_request；该请求只记录，不隐藏、删除或调用 NativeMemoryOwner。
含新 Agent delta 的根不交给旧二进制打开。所有检查只用新合成根、脚本分析器
和自有进程；未访问私人 Vault、真实配置、Provider、Runtime 或 Activity/Usage。
合成证据不代表真实模型、产品采用或主人验收。
