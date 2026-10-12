# MV-7-H2d · 带来源的语义变化与摘要审核

日期：2026-10-12。基线 main `18a1b52`；范围：[ADR-MEM-53](../adr/053-source-bound-semantic-history-review.md)。继续 MV-7，不启动 MV-8。

## 实现与采用方式

`history_semantic::create` 将主人选择的确切 TopicKey、既有完成响应的原始建议/响应 hash、全部选定来源修订/片段/hash 和上下文绑定为普通 source-linked Session 输入。去掉可变的候选应用状态和主人修改，重复原 key/options 得到同一任务；只读加载仍核对当前 SourceRead 权限、来源最新修订和原响应 hash。最多 16 个唯一来源片段、64 KiB 选定文字及 64 KiB 整个输入；超限拒绝，不省略来源。

`prepare` 复用 Extraction-purpose 编译及 native prepare，返回供主人检查的请求，不发送。宿主把 `AnalysisGuard { adapter, task }` 作为 Client.guard，VaultAdapter 作为 Client.journal；每次 guard 先重读任务，再走既有权限、exact body、凭据、额度和一次调用账本。额度由宿主 Quota/CallOptions 控制，不产生新的账户预算或调度器。失败/未知调用保留原状态，重新打开任务不能重发。

`preview` 整体验证已完成响应中的 changes 和 summary：新增/纠正/取消/冲突/不确定最多 16 条，摘要最多 8 点；每条有不超过 4 项精确来源引文，完整上下文、role、证据类别、原始时间和分支随引用保留。纠正/取消两侧都需用户证据，双方各自必须匹配选定图中的有向先后关系；不同分支不能当作替代。Preference 的确定提议不能仅由模型证据支持。没有生效时间或目标 Memory 字段；非法字段、坏引用或任何条目错误使整个响应失败。空结果仍列全部未引用来源。

结果是带来源的模型解释和派生摘要；它不发布候选，不批准 Canonical，也不覆盖主人现有修改。替代仍须主人指定目标和业务时间并走正常治理。精确引用和祖先关系不证明陈述被原文蕴含或重要信息无遗漏。

## 合成证据

七组新增定向测试通过，fixture 为 [受控语义响应](../../tests/fixtures/history/semantic-review-golden.json)，原文为 [初始化样例](../../tests/fixtures/history/initialization-synthetic.json)，所有传输及秘密均为测试替身：

- 两家假 Provider 返回偏好纠正、主分支计划取消、编辑分支冲突、未采纳助手建议、未知日期和摘要引用。时间不变成 effective_from，model_claim 不变成用户确认；没有新增候选或正式记忆。
- 坏摘要引文、越界索引、把不同分支当先后、助手建议作为旧决定、额外 effective_from、没有旧证据的冲突和摘要超限使整个响应拒绝；没有部分发布。
- 已完成和失败任务均用原 key 找回；同键改 model 拒绝；重新打开 Vault 仍找回原任务及响应状态，不新增 head、不重发。锁定和非主人拒绝；空响应保留两条未引用来源。
- 准备后，通过实际 importer 修订了原任务未引用的前文来源；AnalysisGuard 在假传输计数为 0 时拒绝旧请求，head 不增殖。
- 主人确认 WithDependents 清理来源后，语义输入对象明确读为 intentionally_purged，旧任务和已保存解释都不能返回，Vault 验证一致。
- 17 个唯一来源片段超过预算时不创建部分分析 Session；Preference 中仅凭助手建议提出确定偏好被拒绝。

开发中按仓库实际类型修正 Session policy 读取和 native guard 命名；来源修订测试最终采用 importer，不绕过 Vault record 规则。没有放宽既有 guard、secret/path 检查或正式记忆确认。

Rust 1.98.1、GNU Windows、离线全仓库验证退出码均为 0：`cargo fmt --all -- --check`、`cargo test --workspace --locked`（371 项通过，0 失败）和 `cargo clippy --workspace --all-targets --locked -- -D warnings`。完整测试及 Clippy 输出保存在忽略的本地验证日志中。

Python 3.12.10 / python-jsonschema 4.26.0 独立校验 OK：36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases。Runtime checker 通过且 aggregate 已登记；`git diff --check` 通过。13 个本片公开文件及 79 个本地文档链接核对通过，所查个人绝对路径/credential 形状未命中，两份状态快照 SHA-256 未变。未安装依赖。

## 集成与剩余门槛

Runtime surface 为 `0d48e30dc5645d1cd7b9e5a190788d932dff9d86c9603a251274de184c76c329`，采用要求已在同片 [日志](../integration/RUNTIME.md) 登记。47 条 workspace 命令、存储 Schema/字段和 ExtractionJob 版本不变；没有 Runtime 产品 UI。纯原生调用复用现有契约，仓库独立构建，不依赖 Runtime checkout。

受控假响应不是实际模型质量证据。主题摘要忠实性、重要信息漏提、真实双 Provider、Runtime 采用/主人验收、全量规模/费用和独立恢复仍 pending。没有私人缓存读取、真实凭据/外发、服务器部署、后续 MV 激活或状态快照更新。
