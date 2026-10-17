# Enouia Memory · Claude 交接（2026-10-17）

本轮按 Morii 的“完成手里的工作就停下来”收尾。本文与[启动提示词](CLAUDE_MV7_START_PROMPT_20261017.md)只供人工交接，没有启动其他会话或安排后台工作。旧的 `CLAUDE_START_PROMPT.md` 属于 MV-0 历史资料；10 月 10 日 MV-7 交接的基线和提交授权也不能替代本次状态。

## 当前工作与证据

Morii 的最终交付指令是“不需要做任何测试，快速交付即可”。已停止全量复核；此前五项契约回归、一项审核传播回归和双 API 原生场景通过，独立 Schema／Python／接入面检查通过。最终全量 Rust 被中断，最终严格 Clippy 未完成，不能称 required checks 全通过。接手若需继续检查，先遵守 Morii 届时的新指令，不把旧验证流程当作再次测试的授权。

实现分支为 `codex/history-context-dependencies`，基于 PR #32 的 `a2b77bda97c3aa613cf9fef852ee8839ff57cb61`。最终提交和远端 HEAD 接手时应现场核对；本文不把未来文档提交当作实现基线。前序 #29～#32 草稿依次覆盖人工比较材料、语义边界题集、相同输入的任务身份隔离及完整输入谱系格式一致性。历史报告见[质量接续](MV7_QUALITY_CONTINUATION.md)。这些工作不证明真实模型质量，也不表示该 PR 栈已合并。

当前修复已在独立合成来源上复现：模型读过两份资料、只引用一份，清除未引用来源后，派生候选仍能读取。共享导出原件会掩盖这个依赖缺口。[ADR-MEM-65](../adr/065-recorded-context-source-dependencies.md)先记录显式可选字段，再实施领域链路；[验证报告](../validation/MV-7-semantic-context-dependencies.md)记录最终结果和本地日志索引。

`context_source_refs` 是最多 128 个唯一来源修订；不是引文、佐证数量、权限或完整性承诺。原生语义提案保存全部选定来源，支持证据仍只含回答实际引用。提案敏感度、完整／增量依赖闭包、审核批准／合并／修订、Capsule 来源与新鲜度、候选历史和清除影响都考虑该字段。旧记录无字段可继续读写，序列化不凭空补空数组；未做旧资料扫描或回填。

清除测试覆盖两家 Fake Provider、独立来源、正式批准及已保存 Capsule。上下文来源被清除后派生候选历史与 Capsule 不可读取，引用的另一独立来源仍可读取。其他已批准记忆依既有 losing-provenance 策略保留内容；本例重新编译遇到已清除记录返回 `IntentionallyPurged`，不能宣称所有正式派生内容物理删除或普通使用已恢复。

## Runtime 与待办

接入面和同提交日志见 [RUNTIME.md](../integration/RUNTIME.md)。47 条 workspace IPC 命令未变。新读者兼容缺字段记录，但旧严格读者拒绝带新字段的记录；Runtime 需要升级固定 Core 修订及匹配契约，在自身宿主验证后才能采用。没有修改 Runtime checkout、实际 Vault、凭据或部署。

真正的 Provider／历史质量、完整历史初始化、Runtime 实际接入、长期备份恢复和 Morii 日常使用验收仍未完成。ADR-MEM-49 的云端单主目标已经形成设计，不能继续写成方向尚未决定；MV-8 仍未启动，自动接受范围、真实资料、服务信任和迁移激活仍待 Morii 决定。

另有既存 workspace 有序并发屏障故障尚未解释；通过的重跑或阶段诊断不证明它已修复。不要无证据增加超时、重复大规模压力测试，或把它塞入本次上下文修复。接续必须先由 Morii 指定工作包。

## 接手检查与停止点

先看 Git 状态，尤其用户自己的 `AGENTS.md` 改动；不要提交、覆盖或还原它。本轮另外按 Morii 请求准备了 Dashboard 标准快照，使用 [JSON](../status/project-status.json) 和[中文报告](../status/PROJECT_STATUS.md)，两份是同一时点元数据，未来更新仍须明确请求。快照流程仅阅读保存证据，不重跑测试、不改变实现或路线。

后续验证使用根 `AGENTS.md` 的 Rust 1.98.1 GNU、离线 Cargo、Python 3.12 固定环境；本次最后的免测试交付要求优先，未完成检查已明确保留。最新具体退出结果以本次验证报告为准；既有测试只含合成材料。PR 保持草稿，未合并；Codex 完成推送和交接后停止。
