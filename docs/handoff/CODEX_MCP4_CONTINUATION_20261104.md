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

## 后续两片

- `13c57de` / ADR124：独立 supersede permit/intake/delta、System SupersedeAnalyzer、
  start_worker_with_reports、pipe-8 和 launcher。只有真实用户原话和 update 可以
  进入普通 evaluator；三个 dispatch 互斥，无主人队列。完整离线 workspace：
  948 passed / 0 failed / 4 ignored，114 个目标含全部 doc-tests；workspace/all-target
  strict Clippy、70 schemas、fmt/surface/空白检查通过。
  [报告](../validation/MCP-4-agent-supersede.md)。首次 push 遇 GitHub 远端内部错误，
  只读确认远端仍旧后普通重试成功，未改规则或权限。
- ADR125（本片提交）：memory_forget_request 独立授权和请求记录，pipe-9/stdio；
  成功只有 forget_request_saved，不隐藏、删除、改 recall、发模型或生成主人待办。
  52 个 Host 定向/回归测试、6 个合同测试、72 schemas 和 scoped strict Clippy 通过。
  忘记请求加入后没有重跑全量；不得将前一片 948 当作本片完整测试结果。
  [报告](../validation/MCP-4-agent-forget-request.md)。

## 路线边界与恢复

三项已授权路线到 request-only forget 收束。草稿 PR #84 未合并，main 未改。
没有开始 MCP-5、新 MV、forget-hide、Runtime 采用、真实模型/Provider/embedding、
外发、部署、迁主或双写。AGENTS、手动项目状态两文件、真实 MCP 配置和私人数据
均未改；未读 Runtime Activity/Usage。后续路线需要 Morii 新指令。

保存回执不表示已接受或已修订。纠正/更新均由独立 System 端口决定，Agent 文字
从不成为内容或证据；不确定分析不变成主人必须逐条处理的队列。forget-hide 的
范围/期限/恢复/撤权规则、真实模型与预算/外发、Runtime 采用仍由 Morii 决定。
当前 System 嵌入采用 start_worker_with_reports 才会显式消费两类报告；不替 Runtime
选择 pin 或接入真实适配器。

所有实际检查只用新合成根、脚本分析器和自有进程。合成证据不等于真实模型质量、
产品采用或主人验收。旧 continuation 撤权时序测试本次全量通过，源码未改，仍不
宣称已修复历史偶发。journal 丢失后的重复分析风险、链上报告到修订关联仍未扩展。
含 AgentNote/Correction/Supersede/Forget delta 的根不能交给旧二进制。

OutcomeUnknown 必须保留完整原始 MCP Value 和原生原包原键，停止连接/授权，
显式恢复和新原生 setup 后才可原键恢复回执；绝不自动重发、重连或换键。
所有新集成面均同片更新 runtime-surface.json 和 RUNTIME.md；采用需匹配兼容指纹。
完整日志见各片报告，未完成日志不能当作通过。
