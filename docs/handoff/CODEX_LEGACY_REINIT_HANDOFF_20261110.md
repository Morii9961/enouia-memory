# Codex 交接：ChatGPT 连接器路线 + 旧库重新初始化

日期：2026-11-10，Asia/Shanghai。Morii 要求 Claude 停在当前片，把后续工作全部交给 Codex。
本文件交接工程事实、开放决定和后续顺序，不替代 Morii 的新指令。
配套：[启动提示词](CODEX_LEGACY_REINIT_START_PROMPT_20261110.md)。本机路径、数据目录和工具位置
写在被忽略的主目录 `.local/handoff/CODEX_LEGACY_REINIT_LOCAL_20261110.md`，公开文档只用占位符。

## 1. 从哪里接

| 项目 | 交接时状态 |
| --- | --- |
| 仓库 | `Morii9961/enouia-memory` |
| main | `5c967c0`（#96 合并；之前 #95 `2581e9c`、#94 `20ded79`） |
| 已合并 PR | #94 ADR135 方案；#95 `legacy-snapshot` 导出；#96 生产初始化入口与 `enouia-memory-reinit`。两者合并后的树与测试过的 head `229e842` 完全一致 |
| 待合并 PR | 无（交接文档 PR 除外） |
| 集成面聚合 | `31fc7e44…`（已登记；#95/#96 未改集成面） |
| 合并规则 | 只在 Morii 明确说"合并"时合并。需要 main 时把 main merge 进分支，不 force-push |

启动时用 `git log origin/main` 核对 `5c967c0` 已在 main，从最新 main 新建分支。

## 2. 已完成的路线

**ChatGPT 连接器服务端（#88–#93，已在 main）**：ADR129 远程 MCP 端点（Streamable HTTP、
bearer + `memory.read`、RFC 9728），ADR130 仅回环 HTTP/1.1 驱动（只收 Content-Length，
反代需 `request_buffers`），ADR131 Linux Vault 平台层，ADR132 OAuth 授权服务（DCR、PKCE
S256、refresh 轮换、只存摘要），ADR133 可运行服务 `enouia-memory-remote serve|approve`
（permit 5 分钟、4 分钟续期），ADR134 Linux 主人初始化（有效 uid 绑定 machine-id）。
部署参考：[REMOTE_MCP_LINUX](../deploy/REMOTE_MCP_LINUX.md)。与 ChatGPT 的真实连接**从未测过**。

**旧库重新初始化（ADR135）**：

- 背景：真实数据不在 2026-10-30 建的空 Owner 根里，而在一个 v1 旧库（一次 ChatGPT 导出的
  来源与附件 + 种子阶段模型提取的候选 + 原件存档），旧库没有正式记忆。新 `SegmentedOwnerV1`
  根在创世提交里绑定主人主体，旧根不能升级，只能重新初始化。
- Morii 的决定（2026-11-10）：候选直接成为正式记忆（不调模型、不建待审队列）；时区
  Asia/Shanghai；目标 Linux VPS；新库验收并有已验证独立备份后可删除旧库，**删除前再问一次**。
  见 [INITIALIZED_VAULT](../integration/INITIALIZED_VAULT.md) 的"主人已确认的迁移决定"。
- #95（已合并）：`enouia-memory legacy-snapshot <vault> <empty-dir>`，从旧库**工作副本**只读导出
  `snapshot.json`（全部修订，校验为 RecordSet）、`objects/<sha256>`（导入原件和附件）、
  `manifest.json`。
- #96（已合并）：`Initializer::adopt_bootstrapped`（生产入口，只接管刚由主人初始化工具建好的空根），
  `legacy::legacy_batches`（旧来源按消息 id/内容哈希对回重新解析的导出消息；每条候选成为
  一条 ClearSelfReport 语句，引用候选所引的主人原话），`Statement.context_sources`，
  驱动 `enouia-memory-reinit plan|run`（plan 在内存里建出全部批次，不写入；run 可断点续跑）。
- 证据：`crates/enouia-memory-init/tests/legacy.rs` 两个合成端到端测试；
  [验证报告](../validation/Legacy-reinitialization.md)。真实只读演练（旧库副本）：731/731
  来源对上，185 条候选 → 185 条采纳记忆，16 批，0 待审，0 写入。

## 3. 演练中学到的硬约束（改代码前先知道）

- 引文最多 500 字（`QuotedEvidence::valid_shape`）；超长消息由 `excerpt` 截取最贴近候选的句子。
- 标题不能含控制字符（换行、Tab）；工作器在"引文不含标题"时用引文前 60 字兜底，多行引文会被拒。
  legacy 模块已绕开；**工作器本身的兜底仍有此缺陷**，日常/远程写入遇到多行原话也会失败（见第 5 节）。
- 采纳（Adopted）只给 ClearSelfReport，而它要求证据是归属主人主体的 User 原话；助手文字只能待审。
- 一个计划里同一会话最多 32 条分支。旧库只存主人轮次，缺失的助手父消息让每条都成独立分支，
  所以一个会话每批最多放一个 32 条的块；会话跨批拆分已验证可被 Vault 接受。
- `Plan::build` 每批最多 64 条消息、128 条语句；同一 watermark 重复 enqueue 返回原批次（续跑依赖它）。
- ProjectState 需要阶段（state），候选没有，所以 project_state 候选落为 Fact。
- 107 处候选的附加引用因不在同批被丢弃（只少了旁证链接）；若要改进，在容量允许时合并引用相关的块。

## 4. 剩余工作与决定门槛

按顺序；标 **[Morii]** 的必须先得到 Morii 明确决定，不要把建议当授权。

1. **[Morii] 新库建在哪里**：本机（Windows/NTFS）先建、验收后再搬 VPS，或直接在 VPS（Linux）建。
   当前这个问题已经问过 Morii，还没有答复。
2. **独立备份/恢复（缺口，可先做合成片）**：INITIALIZED_VAULT 要求新 segmented 布局有经验证的
   独立备份和恢复；旧布局的 CLI `export/restore` 不算。仓库里目前**没有** segmented 根的
   export/restore。这是删除旧库前的硬条件，也是搬上 VPS 的手段。先写 ADR 再实现，合成证据。
3. **真实写入**（第 1 项决定后，并得到 Morii 开跑的确认）：
   1. 用 `enouia-memory-owner-bootstrap prepare|commit`（ADR110/134）在主人数据目录建空 Owner 根；
   2. 对旧库**工作副本**跑 `legacy-snapshot`（已有一份快照，见本机附录），再跑
      `enouia-memory-reinit plan` 复核，然后 `run <snapshot> <新根>`；
   3. 核对 ledger：来源/语句/提交数、`model_invocations` 为 0、`outcome_unknown` 为 0；抽查来源跳转；
   4. 做备份并在临时位置恢复验证（依赖第 2 项）。
4. **[Morii] 暴露范围**：远程服务配置里的 `policy_ids/source_ids/max_sensitivity/source_read`
   就是 ChatGPT 能读到的范围，Vault 只认有效策略。重新初始化用的形成策略只有 7 天有效期，
   **不能拿来当长期读取策略**；需要 Morii 决定读取范围，再由主人设置长期策略（可能需要一个小工具）。
5. **[Morii] VPS 与域名**，按 REMOTE_MCP_LINUX 部署（systemd、Caddy、服务账号拥有 0700 根），
   然后做第一次真实 ChatGPT 连接测试（开发者模式连接器 + approve 码）。
6. **旧库删除**：新根验收且有已验证备份后，向 Morii 列出要删的内容并再次确认，才删除。
7. **[Morii] 写入/采集工具**：让 ChatGPT 写回记忆需要修订 ADR102（目前外部 Agent 采集未授权）。

## 5. 可直接做的后续（不需要新决定，仍按片提交、等 Morii 合并）

- 修工作器兜底标题：`enouia-memory-vault/src/automation/segmented/formation_worker.rs` 中
  `quote.trim().chars().take(60)` 改为跳过控制字符（例如取第一段非空行）。这改动 vault automation，
  属于集成面：同提交 `python tools/integration/runtime_surface.py --write` 并在 RUNTIME.md 登记。
- 第 2 项 segmented 备份/恢复的 ADR 与合成实现。
- 附加引用保留率改进（可选）。

## 6. 工作约定

- 遵守 AGENTS.md：阶段只在 Morii 明确要求时开始；不提交真实数据；主人数据目录永不清理；
  集成面变化同提交更新 surface 与 RUNTIME.md；按变更面选择测试。
- 回复 Morii 用中文。合并、删除、部署、真实数据写入都要 Morii 的明确指示。
- 测试：Windows 上 `cargo` 在 `%USERPROFILE%\.cargo\bin`；离线 `CARGO_NET_OFFLINE=true`。
  Linux 验证用 WSL Ubuntu-24.04（见本机附录），跑大型 Windows 构建前先 `wsl --shutdown` 防内存不足。
  已知负载敏感：workspace 并发测试在 Linux 32 线程超时（≤8 线程通过），Windows 有 2 个 host 测试负载下偶发失败。
- 本片证据：init 全部套件通过、clippy（init、cli）`-D warnings` 干净、fmt 通过、集成面未变。
  你的改动之后须重新取证。
