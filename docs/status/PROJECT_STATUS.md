# Enouia Memory 项目状态

快照时间：2026-11-07T13:26:23+08:00 · 协议 1.1 · 项目 ID `enouia.memory` · 报告：Claude

代码基线：`main@2a6310e`。阶段：集成中（`integration`）。整体进度不估算。

#56～#81 新管道已经 #82 合入主线，#83 又加入 Agent 笔记、纠正报告持久化与 System 自动消费。MCP-4 传输（#84）和 MCP-5 开场资源（#85）仍是草稿。Runtime 只采用 v1 58 命令和合成 read-2；真实分析器、个人迁移与云端均未验收。

## 里程碑

| ID | 事项 | 状态 | 证据 | 人工验收 |
| --- | --- | --- | --- | --- |
| `local_storage` | 本地写入、存储与恢复基础（MV-1；合成数据范围） | 完成 | 已有测试 | 不需要 |
| `memory_management` | 本地检索、审核、更新与生命周期基础（MV-3/4；合成数据范围） | 完成 | 已有测试 | 不需要 |
| `local_conversation` | 本地对话选取相关记忆与重启接续（MV-5；模拟模型范围） | 完成 | 已有测试 | 不需要 |
| `real_provider` | 真实双模型调用与来源片段候选提炼验收（MV-7） | 进行中 | 已有测试 | 待验收 |
| `runtime_integration` | Runtime 主线采用 Memory 精确 pin 0d338f1（Workspace v1 58 命令；read-2 仅合成入口） | 完成 | 仓库证据 | 不需要 |
| `long_running_safety` | 长期运行与真实数据的独立备份、恢复和故障安全验收 | 进行中 | 已有测试 | 待验收 |
| `history_initialization` | 历史规划、批次准备、响应预览与逐步候选入队（真实质量待验收） | 进行中 | 已有测试 | 待验收 |
| `cloud_mcp` | 受限 Host/MCP 主线基础；纠正/取代/遗忘请求传输与开场资源仍在草稿，云端待接入 | 进行中 | 仓库证据 | 待验收 |
| `daily_use_v1` | Morii 日常对话中可可靠使用的 v1 记忆系统 | 受阻 | 仓库证据 | 待验收 |
| `context_dependencies` | 上下文来源依赖记录与清除传播（旧记录不自动回填） | 进行中 | 已有测试 | 待验收 |
| `native_host_service` | 本地 Host 库、原生传输与参考外壳（既有合成成果） | 完成 | 已有测试 | 不需要 |
| `automated_memory_core` | Core-1～7：准入、持久任务、自动循环、策略、形成、召回、授权读模型 | 完成 | 已有测试 | 不需要 |
| `ui_v2_agent_drafts` | Owner 初始化、UI/Agent 读取与 Runtime 采集/后台形成（#56～#81 经 #82 合入主线） | 完成 | 仓库证据 | 不需要 |
| `agent_notes_corrections` | Agent 笔记、纠正报告持久化与 System 自动消费（ADR121/122，#83 已合并） | 完成 | 已有测试 | 不需要 |
| `mcp4_mcp5_transports` | MCP-4 纠正/取代/仅请求遗忘传输（#84）与 MCP-5 开场工具和资源（#85，叠在 #84 上） | 进行中 | 已有测试 | 待验收 |

## 当前卡点

- **新管道已入主线，Runtime 尚未采用**：Runtime 只用 0d338f1 的 v1 58 命令和合成 read-2；Owner 初始化、采集 pipe-2、后台形成、纠正消费都没有在产品里接入。
- **真实质量与个人使用验收不足**：只有假分析器和合成根；真实模型/embedding、私人初始化、独立恢复、旧主隔离与云端均未证明可用。

## 下一步（本次不执行）

1. **P1 评审草稿链 #84 → #85**：确认验证范围后由 Morii 决定是否按顺序合并；合并后 main 成为新基线。
2. **P1 安排 Runtime 采用所选精确契约**：明确旧 v1 与新 Owner 入口边界，先用合成数据接线，保持现有个人库。
3. **P1 确定第一轮真实质量验收**：由 Morii 明确资料、分析器、预算、恢复和迁移范围。

## 需要 Morii 决定

- 是否按顺序评审并合并草稿 PR #84（MCP-4）和 #85（MCP-5）？
- forget-hide（立即可逆隐藏）是否实现？范围、期限、恢复和撤权规则是什么？
- 首轮真实分析器/纠正模型、预算与外发范围，以及个人资料和恢复验收允许什么范围？
- Runtime 何时采用新 Owner 管道（含 start_worker_with_corrections），先接哪一条？

## 依据与验证范围

- 2026-11-07 生成快照：只读 Git、GitHub PR 和仓库文档；不重跑测试、构建或真实验收
- main@2a6310e（PR #83 合并提交），与 origin/main 一致
- PR #82（#56～#81 合入主线）、PR #83
- docs/handoff/CODEX_MCP4_HANDOFF_20261104.md
- 开启的草稿 PR #84（MCP-4）、#85（MCP-5，叠在 #84 上），未合并
- PR #83 报告：913 passed / 0 failed / 4 ignored；PR #84 报告：948 passed / 0 failed / 4 ignored。均为保存结果，本次未重跑

证据位置见 [project-status.json](./project-status.json) 的 `evidence_refs`。本次只整理已有记录，`tests_rerun=false`；人工验收只转述有记录的结论，不把合并、分支或 CI 当作通过。
