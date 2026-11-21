# Enouia Memory 项目状态

快照时间：2026-11-21T13:11:01+08:00 · 协议 1.1 · 项目 ID `enouia.memory` · 报告：Claude

代码基线：`main@ae1f859`。阶段：验收中（`acceptance`）。整体进度不估算。

11 月 18～19 日真实新库已原生形成、关闭重开并核对来源，主库迁入统一数据区，Morii 本人读取不再受期限限制；Runtime 已采用 f9ac21d 只读接入。剩下 Morii 亲自验收、独立介质备份（已暂缓），以及写入、模型、MCP 授权。

## 里程碑

| ID | 事项 | 状态 | 证据 | 人工验收 |
| --- | --- | --- | --- | --- |
| `local_storage` | 本地写入、存储与恢复基础（MV-1；合成数据范围） | 完成 | 已有测试 | 不需要 |
| `memory_management` | 本地检索、审核、更新与生命周期基础（MV-3/4；合成数据范围） | 完成 | 已有测试 | 不需要 |
| `local_conversation` | 本地对话选取相关记忆与重启接续（MV-5；模拟模型范围） | 完成 | 已有测试 | 不需要 |
| `real_provider` | 真实双模型调用与来源片段候选提炼验收（MV-7） | 进行中 | 已有测试 | 待验收 |
| `runtime_integration` | Runtime 已采用 Memory f9ac21d：登记只读入口读取全部敏感级别与全部历史 | 完成 | 仓库证据 | 待验收 |
| `long_running_safety` | 同盘原生导出（seq 440）已校验并逐文件核对；独立物理介质加密备份与恢复验收暂缓 | 进行中 | 已有测试 | 待验收 |
| `history_initialization` | 从完整原始聊天全新形成真实新库：原生形成、关闭重开、来源核对已完成 | 完成 | 已有测试 | 待验收 |
| `cloud_mcp` | 远程只读 MCP、Linux、OAuth 和独立读取授权已实现；真实部署及 ChatGPT 互通待验收 | 进行中 | 已有测试 | 待验收 |
| `daily_use_v1` | 真实新库在 Runtime 中只读查询与来源核对已接通；Morii 亲自验收和独立恢复未完成 | 进行中 | 已有测试 | 待验收 |
| `context_dependencies` | 上下文来源依赖记录与清除传播（旧记录不自动回填） | 进行中 | 已有测试 | 待验收 |
| `native_host_service` | 本地 Host 库、原生传输与参考外壳（既有合成成果） | 完成 | 已有测试 | 不需要 |
| `automated_memory_core` | Core-1～7：准入、持久任务、自动循环、策略、形成、召回、授权读模型 | 完成 | 已有测试 | 不需要 |
| `ui_v2_agent_drafts` | Owner 初始化、UI/Agent 读取与 Runtime 采集/后台形成（#56～#81 经 #82 合入主线） | 完成 | 仓库证据 | 不需要 |
| `agent_notes_corrections` | Agent 笔记、纠正报告持久化与 System 自动消费（ADR121/122，#83 已合并） | 完成 | 已有测试 | 不需要 |
| `mcp4_mcp5_transports` | MCP-4 纠正/取代/请求遗忘与 MCP-5 开场工具及资源已合并（实现与合成范围） | 完成 | 已有测试 | 不需要 |
| `storage_relocation` | 主库迁入统一数据区（ADR-MEM-154）与 Morii 全历史本机读取（ADR-MEM-153） | 完成 | 仓库证据 | 不需要 |
| `system_recall_preview` | 分段 Vault 的原生 System 召回预览（ADR-MEM-152，#125） | 完成 | 已有测试 | 不需要 |
| `r4_followups` | R4 后续能力（回看建议等，Morii 11-19 确定列入路线） | 未开始 | 仓库证据 | 不需要 |

## 当前卡点

- **只有一份同盘备份**：独立物理介质的加密备份与恢复验收已由 Morii 11-19 暂缓；主库和备份在同一块盘上，附件原件已删除，无法取回。
- **还没有 Morii 亲自验收和真实安装**：Runtime 候选 884c015 等一次性安装验收（门槛 G）；验收前只算本机代理验收。
- **写入、模型、MCP 仍是只读**：门槛 A～D 未授权：纠正、会话写入与形成、模型调用、外部 Agent 读写都不能用。

## 下一步（本次不执行）

1. **P0 配合 Runtime 候选 884c015 的一次性验收**：Morii 在本机装上候选，确认能读到全部历史并核对来源，记下验收结论。
2. **P1 按计划第 15 节实现 R4 后续能力**：回看建议等能力先以合成数据交 PR，合并后再接真实库。
3. **P2 恢复独立介质加密备份与恢复验收**：等 Morii 定下时间和介质后，做一次可恢复的独立备份并演练恢复。

## 需要 Morii 决定

- 何时装上 Runtime 候选 884c015 做一次性验收？
- 独立物理介质加密备份何时恢复推进？用什么介质？
- 写入真实库、模型调用、MCP 读写（门槛 A～D）是否授权？按 A→C→B→D 的顺序吗？
- 统一数据区根上 10-28 的那份旧登记怎么处理？

## 依据与验证范围

- 2026-11-21 生成快照：只读 Git、GitHub 和仓库文档；不重跑测试、构建或真实验收
- main@ae1f859（PR #128 合并提交），与 origin/main 一致
- 上一份快照（2026-11-16，Codex）之后合并的 #110～#128
- docs/integration/INITIALIZED_VAULT.md（2026-11-19 状态与存放变化）
- docs/design/IMPLEMENTATION_PLAN.md（2026-11-19 当前接续，第 15 节）
- docs/handoff/MEMORY_DELIVERY_ROUTE_2026-11-16.md
- enouia-runtime docs/integration/R4_AUTHORIZATION_GATES_2026-11-19.md
- 私人验收记录只在本机，本快照不引用其内容

证据位置见 [project-status.json](./project-status.json) 的 `evidence_refs`。本次只整理已有记录，`tests_rerun=false`；人工验收只转述有记录的结论，不把合并、分支或 CI 当作通过。
