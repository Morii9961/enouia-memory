# Enouia Memory · 数据契约

版本：设计包 v1.0 / 2026-09-28。本文是字段语义规范，不含实现代码或可执行 Schema；MV-0 将它转为机器契约与合成 fixtures。未列出的未来业务字段放入有命名空间的 extensions，不静默改写既有字段。

## 1. 统一规则

内部 ID 使用带类型前缀的随机 UUID，不把时间、标题、路径或来源账号编码进 ID。相同对象的重试靠幂等键识别，不靠随机 ID 碰巧一致。原始平台 ID 保留为不透明字符串，与 provider、account_scope、conversation_id 组成命名空间；不同账号同名 message ID 不合并。

`schema_version` 是记录结构主版本；`revision` 是同一逻辑记录的正整数修订；`commit_id` 是整体提交点。这三者不能互相代替。修订单调增加，单写者分配；时间由 Clock 注入，不作为唯一排序或去重依据。新文件的 created_at ≤ updated_at；未来的有效时间是合法业务数据，系统时钟回退则暂停产生伪造顺序并报告 clock_regression。

未知主版本只允许保存原始字节和只读诊断，不允许重写；未知非安全扩展在往返时保留。未知权限、状态、敏感级别一律不按“普通”处理。升级前固定快照，迁移输出新修订，校验后整体切换；旧版本程序不得向新格式写入。

## 2. 来源对象 SourceRecord

| 字段 | 必需性 / 含义 |
|---|---|
| schema_version, source_id, revision | 必需；内部身份与版本 |
| source_kind | 必需；export_message / runtime_event / imported_document / manual_assertion / external_event / agent_submission |
| provider, account_scope | 外部来源必需；账号使用本地不透明别名，不存登录凭据 |
| import_id, raw_object_hash | 导入来源必需；指向收到的原始字节和导入清单 |
| original_conversation_id, original_message_id | 上游存在则原样保留；缺失显式为 null，不制造平台 ID |
| parent_source_ids, branch_id | 保留分支关系；无法恢复时标 unknown |
| locator | JSON Pointer、原始文件 byte range 或 Runtime event ID；压缩包另含安全 member 名、member hash 与包内定位链；必须适用对应对象版本 |
| original_time, occurred_at, captured_at, time_precision | 原始时间、可解析 UTC 时间、落盘时间、精度；未知不填虚构日期 |
| speaker_role, author_label | user / assistant / tool / system / unknown；角色是来源数据，不等于权限 |
| evidence_class | user_statement / user_confirmation / external_observation / model_claim / summary / unknown |
| completeness | complete / partial / metadata_only / unavailable；只描述收到的范围 |
| sensitivity, access_policy_id | 强制安全元数据；导入默认 private |
| attachment_refs | 零个或多个 AttachmentRecord 引用 |
| parser_version, parse_warnings | 解析来源与局限；不把解析警告藏在日志里 |

一个 source_id 指向一个稳定证据位置；若再次导出的同一上游消息内容变化，新建来源修订并保留前版，Canonical 的 source_ref 固定到具体 revision/hash。由多句拼成的陈述必须引用多项 evidence，而非整个对话标题。

`manual_assertion` 保存用户实际输入、操作者、时间、确认方式与内容哈希。它证明 Morii 确认过此陈述，不证明外部世界已独立验证。Agent 自报“用户已同意”只能是 agent_submission，不能充当 manual_assertion。

### 附件

AttachmentRecord 至少包括 attachment_id、source_id、original_name、claimed_media_type、detected_media_type、size_bytes、object_hash、availability、sensitivity。availability 为 present / missing / external_reference / quarantined / unsupported；只有 present 有已验证对象哈希。

附件 URL 只是引用，不能自动抓取内网、带令牌 URL 或过期资源。文本抽取/OCR 为派生对象，标明 parser/model/version、来源页码/区域与错误；OCR 文本不能冒充附件原始字节。外部发布去除原名和路径。

## 3. CanonicalMemory 共通字段

所有五种类型保持 Runtime M0 的必需字段：schema_version、memory_id、type、content、source_id、created_at、updated_at、status。content 是人类可读的具体陈述，不是指令包装或任意模型提示词。以下字段在本设计中新建记录时同样必需，nullable 项必须显式表达未知。

| 字段 | 语义与约束 |
|---|---|
| revision, title | 正整数修订；短标题只用于展示 |
| subject_ids, project_id, category, tags | 主体稳定 ID；project_id 可空；标签为受控或经过验证的字符串 |
| source_id | 主证据来源；必须属于 evidence 列表且可定位 |
| evidence[] | source_id、source_revision、locator、object_hash、evidence_class、支持哪一 claim；至少一项 |
| epistemic_status | asserted / corroborated / uncertain / disputed；用户审核不自动等于 corroborated |
| confidence | 可空 0～1；模型估计只作提示，不作为批准、安全等级或真实性保证 |
| valid_from, valid_until | 可空半开区间 [from, until)；未知起点不填入导入时间 |
| observed_at, last_verified_at, review_after | 可空；观测、复核和下一次应检查时间彼此独立 |
| volatility | stable / changing / live；当前性风险，与优先级无关 |
| priority | P0 / P1 / P2；身份核心、当前长期项目、相关历史的相对排序 |
| sensitivity | public / normal / private / highly_sensitive |
| access_policy_id, egress_policy_id | 指向当前可检查的权限；无策略时拒绝外发 |
| status | active / superseded / archived；保留 M0 枚举 |
| supersedes[] | 被此记忆替代的 memory ID 与精确 revision；不能形成环 |
| review_id, approved_by, approved_at | 可验证的人类确认记录；不由模型填写后直接信任 |
| created_at, updated_at | 系统记录时间，与事实发生时间不同 |

当前修订的字节哈希存放在 commit 清单中，避免文件内含自身 checksum 的循环定义。`superseded_by` 是可重建反向关系，不让两个方向成为独立可写权威。

删除不扩充 M0 status；使用 tombstone 强制覆盖所有状态。`archived` 表示不参与日常当前检索，仍可显式历史查看；`superseded` 表示被新陈述替代，通常只在历史或解释变更时返回。`active` 仍可能过期、冲突或未经近期复核，不等于 current truth。

## 4. 五种类型的附加约束

| type | 必需的专用内容 | 约束 |
|---|---|---|
| fact | subject_ids、claim_key | 单条尽量只有一个可判断的事实；健康/财务陈述不由模型自动补全 |
| preference | subject_ids、scope、strength | strength 为 explicit / tentative；偏好适用情境不可被抹掉 |
| episode | occurred interval、participants、summary | 一段已发生经历；不知道准确日期时保留精度 |
| project_state | project_id、state、decisions[]、open_loops[] | 决策、计划、实现、测试、发布状态分项表达 |
| session_checkpoint | checkpoint_id、session_id、covered_events、last_state、open_loops[] | 只能来自审核过的 SessionCheckpoint 工件 |

ProjectState 每个 decision / state item 都有 item_id、claim、evidence_refs、state_kind（planned / decided / implemented / tested / released / unknown）、as_of。若不同项有不同敏感度、时效或需独立替代，拆成多个记录，不用“整个项目已完成”覆盖局部事实。

项目实体采用稳定 project_id + display_name + aliases。同名项目不能靠字符串相似度自动合并；重命名增加别名。MoriMeta 与 Moriium 必须是不同实体。

## 5. 修订、替代、冲突和历史查询

**修订 revision**：同一事实的拼写、来源补充、分类修正或撤回状态改变，保留旧修订。改变实质含义时创建新 memory_id，经 supersedes 连接。

**替代 supersedes**：相同主体与命题范围下，新决定取代旧决定。必须显式指出适用范围；不可因同属一个项目就把全部记录替代。接受事务同时保存新 active 记录、旧记录 superseded 修订及 review。

**冲突**：多个证据对同一命题存在不兼容说法，保存 conflict_group 和待处理关系；自动检索展示冲突或返回 needs_review，不按最新时间、最高 confidence 或模型投票决定谁真。已批准旧事实可以继续显示，但必须标明存在新冲突。

替代关系同时带 `effective_from`（明确值或 unknown）；不知道新事实何时生效时不能擅自截断旧事实的有效区间。未来生效的决定可以预先保存，但当前查询仍使用尚未被有效替代的旧事实。`superseded` 表示存在替代关系，不是脱离时间的全局禁用：历史/未来查询根据 as_of 解析有效链，必要时读取旧修订。应用不得仅凭 status 过滤便把未来才失效的旧状态提前抹掉。

支持两个时间轴：`as_of` 问“当时的业务事实是什么”，`known_at` 问“系统在那一时刻知道什么”。默认 known_at 为当前可验证提交；历史审计选择历史 commit。2026-10 才导入的一条 2026-08 决定，不能假装系统在 8 月已经知道它。现行权限和删除屏障对所有历史查询仍生效。

P0 不绕过敏感过滤；原材料里的 P3 敏感与 P4 易过期分别映射到 sensitivity 与 volatility。不会把“健康”排成低重要性，也不会因“身份”是高优先级就无条件上传。

## 6. 候选与审核

CandidateRecord 必需：candidate_id、revision、proposal_kind、proposed_type、proposed_content、evidence[]、source_id、reason、origin_actor、created_at、sensitivity、status、target_memory_id（更新时）、expected_revision（更新时）、conflicts[]、dedupe_fingerprint、extraction_run_id（自动抽取时）。

proposal_kind 为 create / revise / supersede / archive / identity_change；普通 MCP 不提供永久删除提议的执行权限。默认 status 流转：

```text
pending -> accepted
        -> rejected
        -> merged
        -> withdrawn
```

编辑 pending 候选产生新 revision；accepted/rejected/merged/withdrawn 为终态，不原地重开。需要重议时创建新候选并引用原候选。merged 必须指向目标候选或正式记忆并有审核记录。

ReviewRecord 必需：review_id、actor_id、actor_type=owner、trusted_surface、action、candidate_id/revision、final_content_hash、evidence_refs、target_expected_revisions、approval_nonce、created_at、commit_id。nonce 由可信客户端的审核流程产生，短时、一次性、绑定具体 diff；普通工具参数不能声称自己是 owner。

批量审核必须逐条校验目标 revision，并展示每条变化。存在任何过期批准时整个批次拒绝，或由用户明确拆分成独立事务；不能只提交半份却显示全部完成。

“记住……”可以走 trusted owner 的快捷保存：显示将保存的确切文本并确认，生成 manual_assertion 与 review。模型只能提出待确认内容；如果未来实现自然语言一键保存，也必须记录来源 user turn、精确内容及可信确认，不用一句被导入文档引用的“记住”触发写入。

## 7. Session 与 checkpoint

SessionRecord：session_id、origin_surface、provider_binding（可变历史）、branch_id、parent_session_id、participants、created_at、last_event_seq、status（open / closed / interrupted）、sensitivity、policy_id。一个 session 可有多个独立分支，接续需明确 branch_id。

SessionEvent：event_id、session_id、branch_id、sequence、parent_event_id、turn_id、kind、actor、occurred_at、captured_at、content_ref、source_refs、request_id、delivery_state。kind 至少区分 user_message、assistant_chunk、assistant_completed、tool_request、tool_result、turn_cancelled、turn_failed、checkpoint_created、provider_switched。

用户输入先持久化再调用模型。流式块按有界批次落盘，末尾以 assistant_completed 明确封口；崩溃时未提交的显示文本属于未保存，不得伪造完成回答。默认展示“生成中 / 已保存 / 中断”；承诺“已保存”的文本必须已进入提交。工具仅存实际可取得结果，不保存或索取模型隐藏推理。

自动 SessionCheckpoint：checkpoint_id、revision、session_id、branch_id、covered_event_ids 或连续范围、coverage_hash、base_vault_commit_id、summary、decisions_with_sources、open_loops_with_sources、last_completed_turn_id、generated_by、created_at、sensitivity、status（provisional / reviewed / stale）。

恢复时比对覆盖范围之后的事件和当前记忆修订。checkpoint 中的 open loop 是“当时未完成”，可能已在另一会话解决；不得照旧执行。长会话滚动摘要引用原事件，不能摘要套摘要后丢掉根证据。初版在显式保存/结束会话时生成，自动触发频率留到真实日用阶段配置。

## 8. 提交、审计与删除记录

CommitManifest 至少包含 commit_id、parent_commit_id、sequence、vault_id、format_version、writer_device_id、operation_id、idempotency_key_hash、request_payload_hash、created_at、记录映射、对象哈希、review/receipt 引用、policy_epoch、deletion_epoch。幂等键在同主体同操作作用域内相同而 payload 不同必须冲突，不接受覆盖。

AuditEvent 记录 actor、operation、object IDs/revisions、purpose、destination、policy version、allow/deny、timestamp、result、request_id；不复制正文、查询原句、密钥或文件路径。未知主体不能通过填写 actor 字段冒名；身份由传输认证上下文给出。

Tombstone 记录 delete_id、target IDs/对象、scope、requested_by、review_id、deletion_epoch、created_at；PurgeReceipt 记录受影响存储清单、各副本确认和未能清除的位置。删除是有意的数据生命周期操作，优先于 Raw 的默认不可变约定；具体步骤见 [隐私与恢复](PRIVACY_RECOVERY.md)。

## 9. 类型间引用必须闭合

Canonical → Review → Candidate/ManualAssertion → Source → 原始对象可解；临时 checkpoint → SessionEvent 可解；Capsule → 确切记忆修订与来源可解。来源缺失时允许保留陈述供用户修复，但必须标 broken_provenance 并排除自动外发。

主动删除来源后，相关记忆一并删除或经用户重新陈述建立新来源；不得留下看似可验证的悬空证据。需要保留历史删除事实时只能展示不含已删除正文的 tombstone。
