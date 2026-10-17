# MV-7 · 语义提案的未引用上下文依赖

日期：2026-10-17。基线 `a2b77bd`（PR #32），分支 `codex/history-context-dependencies`。范围见 [ADR-MEM-65](../adr/065-recorded-context-source-dependencies.md)。Morii 最后要求“不需要做任何测试，快速交付即可”，已中断进行中的全量复核，不再运行测试。下文保留此前已取得的证据，不宣称全套检查通过。

## 核查目标

语义任务读取所有选定上下文，而回答只引用其中一部分。提案敏感度已取全部选定来源的最高等级；清除未引用上下文时，仍需确认已保存的派生提案不会继续保留依赖该上下文的内容。使用两份独立手动合成来源，避免同一导出共享原件造成连带清除，从而掩盖依赖是否真正记录。

只调用本地 Fake Provider，在临时 Vault 创建合成提案与删除确认。修正前候选仍能读取的失败已保存在忽略目录 `mv7-context-dependencies-before.log` 和 `before-exit.json`，exit 101。

## 修正

CandidateRecord、CanonicalMemory 增加显式可选 `context_source_refs`，最多 128 个唯一来源修订。缺字段的旧记录保持原样；缺失不证明没有依赖，不自动扫描或补录旧资料。原生语义提案记录全部选定上下文，支持证据仍只含实际引用。提案敏感度取引用与上下文的最高等级；完整／增量校验、来源变更的反向依赖、批准／合并／修订、Capsule 来源和资源新鲜度、候选历史清除均处理该字段。Capsule 不能省略上下文来源，也不把它追加为 MemoryItem 的支持证据。

两家 Fake Provider 的独立来源回归覆盖 Public 引文、Normal 未引用上下文、派生提案批准和保存 Capsule。清除未引用来源后，派生候选历史、任务输入和 Capsule 不可读取，另一独立引用来源仍可读取。其他正式记忆按既有 losing-provenance 策略保留原文；本例重新编译返回 `IntentionallyPurged`，没有成功返回可复用 Capsule。不得将它表述成所有派生正式内容已物理抹除，或清除后的正常编译已恢复。

## 已保存证据与交付限制

在最后停止测试指令之前，定向原生回归通过（一个测试内覆盖两家 API）；五项契约回归及一项审核传播回归通过，覆盖旧 JSON 往返、敏感度／依赖闭包、来源变更反向依赖、Capsule 必须列出上下文来源、预算／重复／Identity 拒绝、候选与记忆合并及修订保留依赖。日志分别为 `.local/mv7-context-dependencies-targeted.log`、`contract.log`、`review.log`（后两者同前缀），对应 exit 文件均为 0。

独立 python-jsonschema 4.26.0 检查通过：36 schemas、55 valid records、119 record cases；历史质量 Python 65 项通过；Runtime surface 检查通过，aggregate 为 `2c1c064082dc14ea9d14387f982c50cd31855b301d534dfa78908f02a1cfd9d1`。相应保存日志为同前缀 `schemas.log`、`python.log`、`surface.log`。fmt 的已保存检查 exit 0。

首次全量运行发现旧“无关候选”夹具复制草案、因而继承了新依赖，已改为从独立合成来源新建；Clippy 提示切片引用可免 clone，也已修改。一次后续编译发现新增构造器缺限定名，已修正。最后全量运行已编译并执行部分测试，收到主人指令后中断；没有最终全量结果，最终严格 Clippy 未执行完成。首次失败日志保存在同前缀 `first-workspace.log`、`first-clippy.log`、`first-checks.json`；`checks.json` 的旧失败不能当作最终状态，以本报告与 `delivery-state.json` 为准。此次交付不满足“全部必需检查已通过”的声明条件。

Runtime 接入面已再生成并在 [同提交兼容日志](../integration/RUNTIME.md)记录。47 条命令未变；旧严格读者不兼容带新字段的记录，Runtime 采用相应 Core／契约并完成宿主验证后才能读取。未修改 Runtime 或真实 Vault，未调用实际 Provider、读取凭据、进行迁移／部署或启动 MV-8。交付后的状态快照仅引用上述已保存证据，不重跑检查；主人验收、真实质量与遗留依赖恢复仍待独立决定。
