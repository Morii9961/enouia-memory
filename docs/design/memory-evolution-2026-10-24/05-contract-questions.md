# 下一轮最小契约问题与后端启动材料

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

日期：2026-10-24（Asia/Shanghai）。源码基线：`9286cd8f89669d9ec3b92f943fd74072457536e7`。状态：**待 Morii 选择切片的设计问题，不是已冻结 API、JSON Schema、枚举、发布格式或新 MV 阶段授权。** 本轮只写文档，没有新领域实现、测试、构建、真实资料或 Provider 调用。

**产品目标已修正为全自动，先读 [06](06-automation-and-ombre-migration.md)。** 本页现有接口限制仍是代码事实；旧目标中机器只能提议、Morii 必须逐条批准已被替代。后续首先需要冻结真实系统 actor 的自动准入与持续提炼策略，不是让脚本模拟 owner 点击。

本页把 [生命周期提案](02-lifecycle-design.md)、[开发路线](03-development-route.md)、[补全评估](04-evaluation-and-backfill.md) 和 [前端数据需要](../../handoff/memory-ui-2026-10-24/CONTRACT_READINESS.md) 落到目前可复用的代码与明确缺口。以下推荐用于下一次获准设计，不供前端直接拼请求。

## 1. 先定“展示哪一层、什么时间”，再做全库时间轴

### 已可复用

- [`CanonicalMemory`](../../../crates/enouia-memory-contract/src/memory.rs#L189)：身份/修订、主体/项目/分类/标签、有效期、观察/复核时间、状态、证据、上下文依赖。
- [`EpisodeFields`](../../../crates/enouia-memory-contract/src/memory.rs#L131)：发生起止、精度、参与者、摘要。已有 [`TimePrecision`](../../../crates/enouia-memory-contract/src/common.rs#L67) 包含月、年、未知，无需先发明“月份精度”新枚举。
- [`SourceRecord`](../../../crates/enouia-memory-contract/src/source.rs#L75)：发生/捕获时间、原始时间/时区、说话角色、父来源、分支、完整度、附件。
- [`ProjectEntity`](../../../crates/enouia-memory-contract/src/memory.rs#L480)：稳定项目 ID、名字及别名；[`TopicKey`](../../../crates/enouia-memory-provider/src/history_review.rs#L35) 仅是 fact/preference 的主题比较键，不能直接当通用人物/地点/故事身份。
- [`temporal::effect/currency`](../../../crates/enouia-memory-contract/src/temporal.rs#L32) 与 [`canonical_memories`](../../../crates/enouia-memory-govern/src/view.rs#L22)：已有业务有效性和被选提交的正式记忆视图。

### 必须先冻结的具体问题

1. **首个投影的对象集合。** 是只读“已批准记忆”，还是同时含 Source/Candidate/Checkpoint？后者的读取、计数和批准状态不同。推荐首片只投影有界正式记忆和已存在关系；候选/来源层作为另一个显式选择，避免用同一个总量说“已形成多少记忆”。
2. **一条时间点/区间到底来自哪一字段。** episode 的发生区间、事实有效期、Source 的消息发生时间、捕获时间、批准时间、修订时间、known-at 提交分别表示不同事情。没有 episode 发生时间时不回退 created_at 画“发生日”；允许未知专组。用哪个时区分桶、跨日区间如何计一次、仅月/年精度如何画范围，都须给合成例子。
3. **known-at 的证据。** `approved_at` 不等于系统第一次收到 Source；`captured_at` 也不能独自证明某个旧提交当时已包含这条记录。历史系统视图应基于固定 commit 的记录闭包；前端不能从日期排序复原历史权限或系统知识。
4. **“当前”“历史”“归档”不是一个枚举轴。** 当前 `memory_list` 的 `includeInactive` 仅选择 status Active 或 AllStatuses；它不是 `current_supported` 过滤器。`memory_search(includeHistorical)` 才经过 currency，但仍排除 Archived。归档内容可经管理列表/直接详情查看，不等于可通过 includeHistorical 搜到。[当前列表/搜索](../../../crates/enouia-memory-workspace/src/lib.rs#L936)。
5. **状态不能只看 summary。** [`memory_row`](../../../crates/enouia-memory-workspace/src/views.rs#L21) 的 expired/notYetEffective/conflicted/sourceMissing 是局部 flags；它不执行完整替代链、活跃冲突双方或 review_after 判断。future supersession 的旧记录可能存储 status superseded 但此刻业务仍有效。若产品要“现在可用”，应由后端计算并冻结该含义，不能用 active 字符串代替。

**最小设计交付**：选定一种对象层、两种明确时间口径、未知/区间展示、状态与当前性映射，以及按同一范围计算的可见计数例子。先不定义全库人生叙事、关系年龄或自动事件合并。

## 2. 关联线先带语义和出处，不先建通用图数据库

### 可复用的现成关系

| 已有结构 | 可正确画成什么 | 不能推导什么 |
| --- | --- | --- |
| `EvidenceRef` 的 source/revision/locator/hash/class/supports | 一条记忆或项目 item 的支持证据引用 | 来源提到同一词就彼此印证 |
| `context_source_refs` | 生成内容曾依赖的来源上下文 | 这些来源都支持正文 |
| `supersedes` 的目标修订、生效时间与 scope | 精确有向替代关系 | 任意相似旧节点均失效，或审核时间就是生效日 |
| `conflict_group_id` | 已记录的待处理竞争陈述组 | 已知哪条真，或任何同组节点此刻都冲突 |
| `Source.parent_source_ids` 与 branch_id | 上游对话的父链/分支 | 同一日/分支 ID 足以证明直接后代或生活因果 |
| project_id、category、tags、subject_ids | 同属性/同范围关联线索 | 人际关系、共同经历、因果或价值判断 |

依据：[`EvidenceRef`](../../../crates/enouia-memory-contract/src/common.rs#L191)、[`Supersedes`](../../../crates/enouia-memory-contract/src/memory.rs#L58)、[Source 契约](../../../crates/enouia-memory-contract/src/source.rs#L94)。

### 必须先冻结的具体问题

- **边的分类与方向。** 证据、生成依赖、替代、冲突、共享属性和模型相似性不合并为一个无类型 link。第一片推荐只映射已有结构；语义相似/共同事件/人工关系需要自己的来源和状态设计。
- **边端点的身份。** 端点指逻辑 ID，还是精确修订？显示当前节点时如何解释指向旧修订的替代/证据？生成布局 ID 不得成为 Canonical ID；换算法或换布局后，详情仍落回原始记录。
- **关联是否是待审事实。** “同标签”可确定性生成；“共同经历”“因果”“承诺关系”属于需要来源或审核的陈述。主人否定一条模型关联时，是本地隐藏、否定假说，还是修正正式记忆？三者不能共用“删除”动作。
- **弱关系的空态与负证据。** 两节点没有线不表示不存在关系；隐藏端点不能通过悬空边、簇名、度数或空轨泄露存在性。聚类应基于本轮可见范围重算，还是隐藏整个受限派生簇，须有明确规则。

**尚缺**：通用实体/事件/故事身份、typed edge 的持久化/批准/否定语义、受权邻域与聚合返回契约。当前 [`RecordKind`](../../../crates/enouia-memory-contract/src/record.rs#L12) 是闭合集，没有 event-graph/feedback/trigger 这类新记录。不能自行加一个 JSON 文件或塞进自由 extensions 后宣称已接通治理。

## 3. 派生投影须有安全依赖，不能仅做“缓存失效提示”

### 已可复用

[`context_source_refs`](../../../crates/enouia-memory-contract/src/memory.rs#L203) 已区分支持与上下文；[`delta` 的依赖收集](../../../crates/enouia-memory-contract/src/delta.rs#L220) 将其纳入闭包；[`derived_sensitivity_ok`](../../../crates/enouia-memory-contract/src/policy.rs#L37) 约束派生敏感度；[`check_fresh`](../../../crates/enouia-memory-vault/src/store.rs#L982) 对引用检查新 tombstone 并报告 policy 变化；[`add_context_dependents`](../../../crates/enouia-memory-govern/src/delete.rs#L298) 已枚举 capsule/inspection/dispatch/session/checkpoint 的删除传播。新图/摘要不自动进入这个枚举。

### 必须先冻结的具体问题

1. **依赖清单覆盖什么。** 至少需区分支持证据、模型实际看过的上下文、输入的 Memory 精确修订及其必要来源、Session/Event/Checkpoint、算法/归一化版本。只保存最终引用的两句话，无法清理模型看过但未引用的敏感正文。
2. **内存投影与持久化派生物分别如何处理。** 前端布局可在可见有界结果上临时计算，锁定/换库/撤回后清空。持久化标题、摘要、聚类、向量、缩略图或通知预览必须有可查依赖和删除路径；“可重建”不代表可以无限保留旧字节。
3. **什么失效，什么删除。** 修订/算法变化可使缓存过期；policy/deletion 变化必须阻断旧结果交付。逻辑删除、purge、归档、修订和单主迁移的传播不同，不能都叫 invalidate。purge 后重建不得从旧缓存或备份旁路复活。
4. **授权按输出区分。** 读某条正式记忆不自动授权读取其原始来源片段；依赖存在检查也不是来源正文许可。MemoryRead 可允许的既有 evidence 元数据，与 SourceRead 才可返回的原文要分开。新摘要若融合多条输入，需确定输出的主体/项目、敏感度、目的与授权闭包，不能拿其中一个 public 节点代表整簇。
5. **现有安全函数如何接入新记录。** `check_fresh` 仅能检查交给它的引用；新增派生物若未提供完整依赖，它无法替你发现未知关系。新持久化对象需明确接入 set/delta 验证、权限、purge/export/restore，以及 Host publication 的哪个闭包；不能只复用一个字段名。

**最小设计交付**：选择一种短小派生物，用“支持来源公开，但分析上下文私有且被删除”的合成例子走完生成、读、撤回、purge、重建。先证明依赖闭合，再扩展到图和向量；本轮不实现这些路径。

## 4. 同一快照的分页、计数与详情必须能对上

现有 [`CommitPin`](../../../crates/enouia-memory-contract/src/ports.rs#L52) 包含 commit/sequence/policy/deletion epoch。索引 [`request_hash`](../../../crates/enouia-memory-index/src/search.rs#L133) 与 cursor 绑定 principal、query、过滤、版本、快照和屏障；Workspace 通用 [`page`](../../../crates/enouia-memory-workspace/src/lib.rs#L348) 则只是当前 commit + offset。这两个游标不是通用可互换格式。

必须先回答：

- 新时间窗/主题/邻域分页的 cursor 绑定哪些请求条件、主体、算法和时区？切过滤或时区后能否继续旧 cursor？推荐在设计里明确拒绝，不在前端沿用。
- 内容固定到旧 snapshot 时，是否每页仍应用当前 policy/deletion？沿用现有屏障方向，不能为了可重复动画恢复已删内容。策略变更后整页拒绝还是返回重开标记，须可预期。
- count 是精确当前可见总量、当前页量、已加载量，还是有界估算？过滤权限要先于聚合；不返回“隐藏了 N 条”。区间事件和同源重复引用的分母也必须明确。
- 命中候选上限、缺来源、尚未索引、局部选定和真正空结果如何区分？这些不能统一显示 0；当前 search 已有 partial，可复用其诚实表达原则。
- 当前 Workspace [`memory_read`](../../../crates/enouia-memory-workspace/src/lib.rs#L989) 只按 memoryId 读当前提交最新修订；[`MemoryArgs`](../../../crates/enouia-memory-contract/src/workspace.rs#L88) 无 revision 参数。点旧节点后返回了新修订，UI 应明确刷新/失效，不把旧版本的边与新正文拼起来。若要精确旧修订读取，需要独立契约，而非在页面给未定义参数。

**推荐首片**：有界窗口、固定稳定排序、快照/屏障标识、完整性说明和详情一致性。不要首片承诺无限滚动全历史、全库精确 facets 与任意时间穿梭。

## 5. MCP scope 与新投影不能靠工具名字继承

[`Scope`](../../../crates/enouia-memory-contract/src/policy.rs#L48) 明确各 scope 独立；[`policy_matches`](../../../crates/enouia-memory-contract/src/policy.rs#L255) 按服务器解析的资源 kind/project/sensitivity 检查。Source/Session 没有 project 字段；客户端传的 project hint 不能为它们补授权限。当前 [`HostPort`](../../../crates/enouia-memory-host/src/mcp/read.rs#L64) 只接现有五类 scope、六工具，不包含全库 graph/timeline/feedback/scheduler 服务。

待冻结的最小授权矩阵应列出“调用主体 × 输出内容 × 现有 scope 是否足够 × 当前策略资源集合”，至少区分：正式记忆、Source 片段、候选/Review、生成依赖元数据、完整 Inspection、图簇/计数、反馈写入和通知投递。

建议第一轮新投影先留在 owner 的受信 Runtime/本地领域路径，不扩大 MCP 工具目录。将来若向 Agent 开放，逐输出做授权；不把新能力暗塞进 memory_read/context_get 返回。ContextRead 的 grant 不开放 memory/source 读工具；当前 compiler 的 index 路径仍会执行其既有 MemoryRead 策略，二者也不能被合并成“有 context 权限就全能读”。候选/Review 更不能借 MemoryRead 暴露 owner 审核。

当前 [`mcp::context`](../../../crates/enouia-memory-host/src/mcp/context.rs#L101) 只允许 current commit、无 session/branch、LocalMock、有限 purpose，且保存 capsule/inspection。`asOf` 业务时间允许，不等于 `knownAt` 历史提交或会话选择已开放。新增投影和通知不应绕过 native grant、撤回、最终新鲜度检查及内容无关的 audit。

## 6. 真正 dry-run 的副作用边界必须单独冻结

当前 Workspace [`context_preview`](../../../crates/enouia-memory-workspace/src/lib.rs#L1875) 会保存 Capsule/Inspection；MCP context_get 同样使用持久化 compiler，并在 Host 包装层追加 audit。当前 [`Index::open`](../../../crates/enouia-memory-index/src/store.rs#L136) 可能创建索引，rebuild/update 也会写本地投影。只说“不修改 Canonical 正文”不足以称无副作用。

先选一种承诺，而不是先命名一个新 endpoint：

| 候选承诺 | 必须明确的范围 |
| --- | --- |
| 全合成纯内存演示 | 不连接真实 Vault，不创建 index/receipt/session/capsule/feedback，不外发；UI 明确是说明性模型 |
| 获准真实域只读实验 | 选择的 pin/index 如何只读取得；是否允许内容无关 audit 或本地指标；这些写入不能隐藏 |
| 保存可复现比较 | 明确是写操作，设计稳定键、确切参数/输入绑定、生命周期和删除依赖，不叫 dry-run |

若选择严格无写实验，冻结：输入上限、算法/参数版本、固定 Clock、预算、权限/删除屏障、只读索引就绪条件、取消结果和保密的排除解释；不得触发外部 embedding/Provider。算法权限门槛不可在 slider 中降低。若依赖的现有 compiler 会写，就提取纯计算层的设计，不直接换按钮文案。

**关键反例**：同一试验重复运行、拖动 slider、取消、失败、权限中途变化后，除了明确允许的 audit 外，head/receipt/capsule/inspection/session/反馈/访问强化/索引不得悄然新增。本文仅要求未来定义这些性质，不表示测试已实现或通过。

## 7. “有帮助”反馈是使用反馈，不是事实证据

当前有 Review、候选 reason、版本和稳定回执，但没有通用 feedback 记录或“帮助反馈影响排序”的契约。现有审阅的批准/拒绝能治理事实，不能直接借来表示“这次搜索没用”。

需要先选定最小动作，例如“本次有帮助 / 不相关 / 不再主动提及这一类”；中文只是产品讨论用语，尚非 wire 枚举。随后明确：

- 反馈指向哪次 query/结果和哪条精确修订？是否允许用户选择只影响这次还是长期策略？不收集就无法解释的全文不要默认保存。
- 行为来源如何区分？系统命中、点开、Agent 评价、主人点击和主人事实纠正都不同。点开卡片不构成独立 user evidence；“不相关”不把原事实变假。
- 谁能写、如何撤销/更正、重复点击怎样去重、旧版本反馈是否仍适用、删除反馈目标时怎样清理？真实身份由 native 取得，不信客户端 actor。
- 是否参与排序？若参与，只允许有上限、可回退的版本化实验；不改 Canonical、证据等级、ACL、敏感度、review 或替代生效日。

推荐第一片先用合成实验的瞬时反馈说明，不持久化。未来真实反馈若保存，需要自己的最小持久化/权限/删除语义；不要藏在 tags、confidence、priority 或 extensions，造成看似无需契约的隐性学习。

## 8. 触发提案、调度与通知投递是三层授权

现有 valid_until/review_after/open_loops/episode 时间可成为**候选线索**，并不授权计划任务或通知。`session_checkpoint` 产物保持 provisional，不能把模型写的 open loop 自动变成主人待办或提醒。

待冻结的问题必须落到一个具体合成规则：“主人选择在某时区、某个日期范围内、最多一次回看一条选定经历”，至少回答：

1. **主体与目的**：谁提出规则、谁精确启用、针对哪位 owner/哪个 client？读记忆/提候选 scope 不包含通知投递许可。
2. **条件与时间**：确切时间、时区、夏令时、Clock 回退/前进、quiet hours、有效截止、单次/重复、冷却和重复事件键是什么？未知发生日不得由 captured_at 补齐周年。
3. **触发与交付**：生成一张私有建议卡、排队、Runtime 通知、移动端推送分别何时发生、哪个组件负责？建议首片只做主人主动打开的回顾或合成规则，不创建真实后台 worker。
4. **取消/撤回竞态**：取消先于队列读取、晚于计算、正好发生在投递前、重启与重放各怎样处理？发送前重新验证当前权限/删除/规则状态，不凭旧 capsule 或待投递正文继续。
5. **内容与安静性**：锁屏是否只显示不含正文的通用提示？离线积压过期后丢弃、汇总还是等主人选择？不能重新上线就补发一串私人经历。
6. **规则身份与单主**：规则和投递回执如何去重、恢复、同步与删除？当前本地 Host 不等于云端持续服务；电脑关闭仍能运行属于 MV-9 激活后的真实出口。不得把 Runtime、MCP 和云 worker 各自变成写者。

不读取 Runtime Activity/Usage，不从情绪推断、空闲监测或记忆正文中的“提醒我”句子自动取得授权。记忆唤起只提供允许的内容和理由，也不授予执行记忆里行动的工具权限。

## 9. 哪些维度可以先用好现有 Schema

“无需新增 Canonical Schema”不等于“现有 Workspace 页面已经能编辑全部字段”。当前 [`MEMORY_FIELDS/type_fields`](../../../crates/enouia-memory-govern/src/propose.rs#L29) 允许多种结构；但 Workspace [`DecisionArg`](../../../crates/enouia-memory-contract/src/workspace.rs#L136) 只接受 editedContent 等既有参数，`review_plan` 的 EditAccept 将 [`details` 设为 None](../../../crates/enouia-memory-workspace/src/lib.rs#L1102)。新增字段编辑入口仍需获准接口切片，不能前端硬塞 details。

| 可先改善的维度 | 复用方式 | 仍须承认的限制 |
| --- | --- | --- |
| 条件、否定、计划/实际、未知 | 在 content/摘要保留限定，给支持片段和完整选定上下文 | 自由文本不是结构化条件求值引擎 |
| 来源角色、时间精度、分支/缺父节点 | 展示真实 Source/extraction citation 元数据 | 不从旁证填身份或缺失发生日 |
| 已批准类型/项目/标签/分类 | 使用现有字段做当前有界可见集合的筛选/分组 | 没有完整后台 facets，不显示全库精确统计 |
| episode 与项目阶段 | 复用已有 EpisodeFields、ProjectStateFields；通过未来获准的候选生成版本提供 | 通用 extractor 当前只支持 fact/preference，不能声称已自动提取这些类型 |
| 当前性与历史变化 | 使用现有 valid/review/volatility/supersedes 与 temporal 函数 | UI 当前 summary flags 不是完整 currency，读取层需对齐 |
| 意义与当时感受 | 本人明确叙述可先作为带时点和证据的普通陈述或 episode 内容 | 不增加未经确认的情感坐标、心理状态、关系强度或重要性分 |
| 来源/依赖/替代图 | 在获准已加载集合上映射现成关系，提供列表替代 | 不把布局相邻、共享标签升级为人生关系 |
| 初始化补全质量 | 复用现有 quality/主题导航工具，以重要单元独立分母评漏提 | 合成流程通过不等于真实质量，真实样本/外发另授权 |

下一轮可先冻结一个最小读投影或一个限定抽取扩展，不需要同时引入实体本体、图数据库、向量库、情绪模型、学习反馈和调度系统。

## 10. 给后端实施者的可复制启动提示词

以下提示词只承接主人明确领取的那个切片。未有领取指令时只能继续设计和核查，不开新 MV、不碰真实库。

```text
你正在 Enouia Memory 工作。先读取本仓库当前 AGENTS.md、实施计划、相关 ADR 和精确 HEAD/工作区状态，保护并行改动，列出本次 owned-file allowlist。研究材料位于 docs/design/memory-evolution-2026-10-24/，先读01-current-core.md和05-contract-questions.md，再按主人的明确领取范围选读02～04及前端CONTRACT_READINESS.md。路径先核实，不从旧交接推断当前实现。

本包是设计提案，不授权你启动新 MV、实现全部A～H、接真实MCP客户端、读取私人Vault/导出、调用真实Provider/embedding、迁主或部署。若主人尚未明确领取一个切片，只提交该切片的最小契约设计与具体待定点；不要把本提示词当作补发的授权。

对已获准的切片，先写出实际复用字段/函数、现有接口限制、最小新语义、合成正反例及必要验证范围。不要从前端mock倒推Schema，也不要把安全相关依赖藏入extensions。域对象、read projection、派生缓存、支持证据与生成上下文分开；当前policy/deletion屏障约束旧快照、标题、边、簇名和计数。

优先修正表达和映射，不先加依赖。明确memory_list的active/inactive与search currency不同；Archived不会因includeHistorical进入搜索；未来生效supersede不提前抹掉旧业务状态。Workspace memory_read只读当前最新revision，详情须与所选版本对账。现有review_plan只可编辑正文，不能硬传结构化details。

若是投影：先限定对象层、时间口径、可见范围、稳定排序/分页、snapshot、最新撤回检查、partial/未知与详情一致性。若是typed edges：先映射已有证据/依赖/替代/冲突，新增推断关系另定状态。新持久派生物必须接入安全依赖、敏感度、purge和恢复闭包。

若是dry-run：现有context_preview、MCP context_get和compiler会保存产物，Index open/update也可能写；先确定允许什么副作用，不换按钮名假装只读。若是反馈/trigger：点击不是事实证据，读权限不是学习或通知权限；不自动强化、批准、归档或删除，后台/通知另有明确启用与撤回规则。

Core保持独立，不依赖Runtime checkout、Activity或第三方仓库路径。产品UI由Runtime承载；真正改变集成面时，在同一实现提交更新runtime-surface和RUNTIME.md。验证与改动风险相称，文档只查文本/链接，局部行为做focused tests；明确列已跑和未跑检查。交付代码、合成证据、真实兼容、个人接受与生产运行分别陈述。保留原有主人状态快照，不刷新。
```

本页完成时只做源码/文档核查与本文件格式/链接检查，没有把任一提案视为已冻结契约或已开始的实现工作。
