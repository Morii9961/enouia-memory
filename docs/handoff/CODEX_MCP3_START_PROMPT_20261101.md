# Codex MCP-3 启动提示词

用法：Morii 在 MCP 实现工作树开启新的 Codex Session，发送下面整段。本文件本身不启动实现，也不授权任何工作。

```text
你接手 Enouia Memory 的 MCP-3 后端开发。日常记忆目标是全自动，人工纠正只是可选控制，不能把不确定结果变成必须由 Morii 逐条处理的队列。

授权范围：按顺序完成 docs/handoff/CODEX_MCP3_HANDOFF_20261101.md 第 4 节的片 A（Owner Runtime wire）和片 B（worker 托管）。每片先冻结最小 ADR 和合同，再在同一片编码、验证、提交并推送。片内可自主处理必要的兼容和测试修复，完成片 A 后直接进入片 B，不用再问。片 B 完成后报告并停止，不进入其他分片。

先读：
1. 当前 AGENTS.md。工作树里是已提交版本，主目录可能有 Morii 尚未提交的更新版，以本机说明指向的为准，不要改它。
2. docs/handoff/CODEX_MCP3_HANDOFF_20261101.md，这是本任务的主文档。
3. .local/handoff/MCP3_LOCAL_20261101.md，里面有本机路径和工具位置。没有时先问 Morii，不要猜路径。
4. ADR-102、111、112，docs/contracts/ 下的 CAPTURED_FORMATION_QUEUE_V1、OWNER_RUNTIME_CAPTURE_NATIVE_V1、RUNTIME_CAPTURE_PIPE_V1，以及相关验证报告。
5. 相关源码：crates/enouia-memory-host/src/automation/runtime_capture.rs、runtime_capture_transport.rs、worker.rs，crates/enouia-memory-vault/src/automation/segmented/formation_queue.rs、formation_worker.rs。

开工前：先 fetch，核对 branch、HEAD、status、worktree 和并行工作，比对交接文档的基线，以实际代码为准。从 claude/mcp-captured-formation-queue 的最新提交建 codex/ 前缀分支。列出本片 owned-file allowlist。保留所有他人改动、Morii 的 AGENTS 和手动项目状态快照，不 reset、clean 或覆盖。

补 PR：68ca1cc 和 claude/mcp-captured-formation-queue 目前没有 PR。先按现有栈式约定开草稿 PR，分别以 #77 和前一分支为 base。之后每片也开草稿 PR，不合并。

硬边界：
- 只做 Runtime 采集。不开发外部 Agent 宿主 hook，不开放 conversation_capture，不做 memory_note、MCP-4、MCP-5。
- 不接真实模型、Provider 或 embedding，只用假分析器。真实分析器需要 Morii 决定模型、预算和外发范围。
- 不打开私人 Vault、聊天导出、备份或凭据。只用新建的临时合成根和自有测试进程。
- 不修改 Runtime 仓库、真实 MCP 客户端配置或共享外壳。Runtime 需要采用的内容登记在 docs/integration/RUNTIME.md。
- 不读取 Activity 或 Usage 数据。
- 不合并、不部署、不升 Runtime pin，不做旧主迁移或双写，不创建自动化或关机任务，不刷新项目状态快照。
- 结果未知时保留原包原键，不自动重发、重连或换键。成功回执只能来自合法提交。

验证：按 AGENTS 和本片修改面选择检查。片 A 和片 B 都改跨 crate 接口时跑工作区全量，用 --no-fail-fast，报告里写清实际结果、失败过的尝试和没跑的检查。集成面变化时在同一提交重新生成 runtime-surface.json 并登记 RUNTIME.md。片 B 引入线程后补多线程竞态测试。

提交：公开内容检查后，只提交本片 owned files，空行后加：
Co-authored-by: Codex <267193182+codex@users.noreply.github.com>

每片报告 commit、分支、PR、具体变更、版本与拒绝矩阵、实际检查、未交付项。如果遇到确实只有 Morii 能决定的问题就停下来问，其余自己判断并继续。
```
