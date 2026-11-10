# Codex 新 session 启动提示词：接续 ChatGPT 连接器路线与旧库重新初始化

下面正文可直接复制给 Codex。请在仓库主目录或从最新 main 新建的 worktree 启动。

---

你接手 Enouia Memory 的后续工作。Claude 已完成 ChatGPT 连接器服务端（#88–#93，已在 main）和
旧库重新初始化的工具（ADR135；#94–#96 已全部合并，main 为 `5c967c0`）。目标是把我的
旧记忆库重新初始化成新的 Owner Vault，放到 Linux VPS 上，让 ChatGPT 通过连接器读取。
回复我请用中文。

先读：

1. 当前 AGENTS.md。
2. `docs/handoff/CODEX_LEGACY_REINIT_HANDOFF_20261110.md`，这是主交接。
3. 本机说明 `.local/handoff/CODEX_LEGACY_REINIT_LOCAL_20261110.md`（仓库主目录）。缺失就说明缺失，
   不猜私人数据或工具路径。
4. ADR129–135、`docs/integration/INITIALIZED_VAULT.md`、`docs/deploy/REMOTE_MCP_LINUX.md`、
   `docs/validation/Legacy-reinitialization.md`，以及交接列出的实际源码，不要只读摘要。

开工前核对 cwd、branch、HEAD、origin、status、worktrees 和并行工作；确认 `5c967c0` 已在
origin/main，从最新 main 新建分支。保留他人修改，不 reset/clean，不 force-push。

我还没回答的问题：**新库建在本机还是直接建在 VPS**。在我回答前，不要建真实 Owner 根、
不要对真实数据跑 `enouia-memory-reinit run`。

不需要等决定、可以先做的（每片先写最小 ADR，再实现、按风险验证、提交、开 PR，等我合并）：

1. segmented Owner 根的独立备份/恢复（目前缺失，是删除旧库前和搬上 VPS 的硬条件），合成证据。
2. 修工作器兜底标题对多行引文的缺陷（会改集成面，同提交更新 runtime-surface.json 并登记 RUNTIME.md）。

硬边界：

- 合并、真实数据写入、部署、删除旧库都要我单独明确指示；删除旧库前列清单再问我一次。
- 主人数据目录（带 `DO_NOT_DELETE.USER_DATA` 或 `.enouia-owner-data.json` 标记）永不清理；
  只读旧库，所有导出都从工作副本做。真实数据、快照、凭据不进 Git。
- ChatGPT 能读的范围（策略、来源、敏感度）由我决定；重新初始化的 7 天形成策略不能当长期读取策略。
- 不接真实模型/Provider；写回/采集工具需要先修订 ADR102，等我决定。
- 不改 Runtime 仓库，不刷新手动项目状态快照，不开始新的 MV 阶段。

每片完成后检查公开内容（路径、个人信息、凭据形状的字符串），提交时使用 AGENTS 规定的
Codex trailer，保留既有 co-author。
