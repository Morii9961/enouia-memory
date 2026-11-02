# 全自动记忆目标与 OmbreBrain 迁移判断

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

日期：2026-10-24，Asia/Shanghai。Enouia 实现基线仍为 `9286cd8f89669d9ec3b92f943fd74072457536e7`。本次复核 folio 的 `main` 仍为 `bf6887575fa8e6fa519279be9dc7e5886a29d2ea`，上游 Ombre-Brain 的 `main` 仍为 `115d831d979bf67efc7150f976e2cb34c333c935`。仅研究与方向修正，没有运行外部源码、真实模型或迁移私人资料。

后续执行已按 Morii 新指令整理为 [Core-0～Core-8 路线](07-core-refactor-and-runtime-route.md)：先做全自动核心，暂缓 MCP 新接线，Memory 后端与 Runtime 当前前端探索分别由 Codex/Claude 接续。本文保留架构比较，具体工作顺序以 07 为准。

## 1. 已经明确的产品目标

Morii 最新明确要求：**完全自动化的记忆系统，日常不需要 Morii 逐条审核、整理、合并、标注或催促记忆。** 这覆盖形成、长期化、复核、巩固、情境召回与自然遗忘。前一版把人工审核当作目标系统长期必经步骤，与这个要求不一致，现予纠正。

“主人必须审核”目前是代码事实，今后要改成系统准入。它不是不能修改的产品原则。旧报告中对来源、时效、恢复、权限的源码结论仍有效；02～05、研究建议和前端交接里将日常人工审核作为默认路线的部分，以本页的新方向为准。

自动化不应改成“简单内容自动通过，其余全部堆给 Morii”。不确定、冲突与证据不足先由系统保留为假说/暂存状态，等待新证据、批次重评或下一次相关情境；可以明确弃答，不能把人工清空待审队列当成正常使用的前提。Morii 的纠正、查看、撤回和控制入口是可选控制权，不是日常工作量。

## 2. folio 的架构与自动化到底做到了哪一层

```text
会话宿主 / 导入 / 网页
        ↓ 提交内容或调用工具
Python MCP + HTTP 服务（server.py）
        ├─ hold：单条记忆 → 模型打标 → 合并或新建
        ├─ grow：长段/日记 → 模型拆分 → 多个记忆桶
        ├─ breath：检索或自动浮现 → 排序/压缩 → 放入上下文
        ├─ dream：组织近期记忆和反思提示 → 宿主 AI 决定后续写入
        └─ trace / feel / pinned：状态更新、主观沉淀、长期保护
        ↓
记忆桶：YAML 元数据 + Markdown 正文
        ├─ dynamic / permanent / feel / archive / trash
        ├─ 主题、标签、重要度、效价/唤醒度、时间与激活次数
        └─ 独立 SQLite embedding 索引
        ↓
后台衰减循环 → 按活跃度自动归档
网页：记忆格 / 时间线 / 星图 / 导入 / 模拟台 / 配置
```

### 记忆形成与组织

`hold` 明确要求调用方 AI 主动保存值得跨会话保留的内容，执行自动打标和合并/创建；它没有 Enouia 式逐条 owner review。`grow` 把长输入拆成多个条目，各自写入，失败时还可能退回整段保存。`_merge_or_create` 会检索近似桶，达到配置阈值后让模型合并正文；也可以关闭自动合并。[hold/grow][hold]、[合并入口][merge]

每个桶是带 YAML frontmatter 的 Markdown 文件，分目录保存；模型负责抽取主题和情绪等属性，系统负责文件和索引。embedding 实际存于 SQLite，并记录所用模型，换 embedding 模型后不直接混算旧向量。[桶管理][buckets]、[向量存储][embedding]

### 长期化、情绪与“做梦”

永久/保护标记使某些桶跳过普通衰减；`feel` 是 AI 第一人称的主观沉淀，能把来源桶标为 internalized。`dream` 会整理近期记忆、相似关联和 feel 聚合线索，返回给宿主 AI，由它继续调用工具来反思、放下或写入长期内容。这比“只保存事实字段”丰富，但**不是一个独立完成全库提炼并保证续跑的无人值守巩固作业系统**。[dream 实现][dream]

### 召回与遗忘

`breath` 综合词面/主题、可选向量、情绪、时间与重要性等通路；不同入口还有保护、去重、多样性、预算或随机浮现规则。自然遗忘有真正后台循环，缺省约每 24 小时检查一次，低活跃桶自动归档，永久/feel/保护桶例外。这是可访问性变化，通常不是自动物理擦除。[衰减引擎][decay]、[配置][config]

### 必须区分的两种“全自动”

1. **无需人审批，AI 自己调用记忆工具。** folio 已经很接近这种体验；其写入和合并可直接发生。
2. **宿主保证捕获输入，服务保证定时提炼、失败恢复、巩固、使用与复核。** 不能只连接一个 MCP 就假定这一整套已经具备。

folio 有 SessionStart hook 调用 breath-hook/dream-hook，将返回文字注入 Claude 上下文。源码中 dream-hook 是返回近期材料，hook 本身不执行“读完后必须生成并提交巩固结果”的完整任务循环。因此直接部署后仍需合适的宿主、触发与执行机制，才能达到 Morii 所说的不需要介入。[会话 hook][hook]、[hook 服务端][hook-server]

## 3. 与 Enouia 当前架构的区别

| 方面 | OmbreBrain-folio | 当前 Enouia | 全自动 Enouia 的目标 |
|---|---|---|---|
| 设计中心 | AI 主动记忆、情绪与活跃度 | 来源保真、人工准入、版本与恢复 | 自动认知流程 + 可追溯持久化 |
| 写入权 | AI 工具直接新建/合并/修改桶 | 模型提议，owner review 才正式化 | 受限系统 actor 自动准入、纠正与巩固 |
| 原始单位 | 记忆桶 Markdown + 元数据 | Source / Candidate / Canonical / Session 分层 | 保留原件；自动生成经历、命题、状态与主观理解 |
| 长期化 | permanent/保护、feel 与 dream 引导 | 主要是人工批准后进入 Canonical | 系统按证据/时效/来源归属自动维护稳定视图 |
| 召回 | 多信号评分与可选语义检索，支持自动浮现 | FTS 和确定性排序，调用驱动 | 情境自动触发，融合召回并保留解释 |
| 遗忘 | 定时衰减、低分自动归档 | 有时效/归档/删除，无认知衰减循环 | 自动淡出、压缩与归档；不把过时误作已完成 |
| 纠错 | 桶更新、模型合并与管理操作 | 精确修订、替代、冲突与回执 | 自动新修订、证据更新、旧结论失效与可回退 |
| 持久化 | Python / Markdown / SQLite 向量索引 | Rust Vault / immutable revisions / CURRENT / 可重建索引 | 保留单一权威写入与恢复，接入自动认知 worker |
| 产品界面 | 自带整套网页 | Runtime 是本地产品 UI | 观察、探索、纠正；审阅不再是必经入口 |

这意味着 Enouia 缺的首先是**自动认知与自动准入层**，不是少一个 MCP 或少一页星图。现有持久化与证据工作可以继续利用；日常人工批准语义需要真正修改。

## 4. 全自动模式需要怎样改变当前核心

目前并非一个配置开关：

- `CanonicalMemory` 的批准字段和校验要求 Owner；`ReviewRecord` 的可信入口、nonce 与校验也要求 Owner。[memory](../../../crates/enouia-memory-contract/src/memory.rs#L189)、[review record](../../../crates/enouia-memory-contract/src/candidate.rs#L260)
- 跨记录校验把 Canonical 修订、review 和候选终态关联；Vault 的 owner-only 记录种类及 actor 检查阻止普通 System 写入这些记录。[set validator](../../../crates/enouia-memory-contract/src/set.rs#L666)、[Vault](../../../crates/enouia-memory-vault/src/store.rs#L170)
- 治理 plan/confirm 检查 owner。现有 System actor 可以作为新机制起点，但它目前没有自动准入能力。[治理入口](../../../crates/enouia-memory-govern/src/review.rs#L1058)、[actor](../../../crates/enouia-memory-contract/src/common.rs#L82)
- 云 Provider 的私人内容外发仍要求逐请求确认；若长期后台提炼采用云模型，这一域也要改成有范围、目的地、预算和期限的持续策略，不能只修改 Canonical admission。[实际外发规则](../../../crates/enouia-memory-contract/src/policy.rs#L28)

建议新增真实的**机器准入决定**，记录系统 actor、证据/反证、输入与目标修订、算法/模型/策略版本、决定理由和提交回执。不要让系统伪造“Morii 手动批准过”，不要把旧人工 ReviewRecord 的历史改成机器生成。

目标自动循环为：

```text
宿主可靠捕获 → 来源归档 → 经历/命题提炼
  → 主体/时间/条件检查 → 重复与冲突比较
  → 系统自动准入 / 自动暂存假说 / 自动等待新证据
  → 同一 Host 提交新修订 → 索引/时间轴/关系视图更新
  → 会话与事件触发召回 → 有界反馈 → 批次巩固/复核/淡出
```

这不要求每条都运行一个昂贵的“AI 审核员”。精确结构、来源和版本先由代码判断，语义任务按需要调用模型；清楚的本人陈述可自动记录为“本人说过 X”，不把它擅自升级为外部独立证实。推断、含糊或冲突自动保持相应知识状态，后续重评，不以“找 Morii 点击批准”作为唯一出口。

事实状态、系统采用状态与注意力强度分别表达：一个旧事件可以确定发生过却很少主动出现；一个高情绪条目仍可能是假说；一条计划不因长时间未访问自动变成已完成。对自动合并尤其要保留新旧版本与支持范围，错误决定可自动回退，原件不被模型摘要覆盖。

## 5. 能不能直接迁移

**可以，值得认真采用其认知机制；但软件迁入、已有数据转换和产品接线是三件事。** folio 根目录是 MIT，软件副本/实质代码复用需保留相应版权和许可；依赖与第三方资产另查。[LICENSE][license]

它自带的 MIGRATION.md 处理的是上游 Ombre 的 buckets 与字段兼容，不是 Enouia 的 Source/Candidate/Review/Canonical 格式。不能把现有 Vault 目录直接交给它当 buckets，也不能推测原始来源、历史修订、访问策略与回执会自动保留下来。[迁移指南][migration]

| 方式 | 得到什么 | 实际代价 | 判断 |
|---|---|---|---|
| 整套 folio 替换 Memory 后端 | 最快接近原版体验，直接使用其 Python/网页/桶机制 | Runtime adapter、数据转换、来源/历史/权限/备份与单主体系都要重接；仍需宿主自动捕获/巩固 | 可选，不能叫复制仓库就迁完 |
| 迁入认知流程，保留 Enouia 存储底座 | hold/grow/breath/dream/decay 的自动体验，继续利用现有来源与恢复 | 需要存储/Provider 适配和机器准入契约；Rust 移植或 Python worker 需比较 | 推荐的正式整合方向 |
| 先隔离运行原版作行为基线 | 直接测原系统形成、合并、召回和衰减，而不是只看图 | 先用合成材料；它不成为第二个生产写者，不自动同步私人库 | 推荐的首个验证切片 |

这里的推荐不来自“我们已经写了很多所以不能换”，而是：Morii 的目标可以通过迁入真正的认知机制实现，同时无需主动舍弃已经有效的来源、修订和恢复能力。如果对照结果证明整套后端更合适，仍可以选择替换，不提前锁死技术栈。

folio 也需要带着证据选择版本：它的查询路径仍有命中 touch，而当前 Ombre 上游已把查询和强化拆开；folio 的衰减循环又比当前上游更克制，移除了老化自动 resolved。因此不能假设“只跟最新上游”或“原样 folio”就包含每项更合适的行为。[folio touch][touch]、[上游显式强化][reinforce]、[folio 衰减][decay]

## 6. 修正后的实施优先级与 UI 交接

1. **以原版行为为基线。** 同一组合成对话检验自动形成、事件时间、变更、合并、feel、召回与归档；确认哪些机制直接采用、哪些替换。需要完整的写入/巩固触发，不只是 SessionStart 召回。
2. **先打通自动准入和持续提炼。** 冻结系统 actor/机器决定/兼容迁移，以及后台模型的持续调用策略；实现一个“来源 → 自动记忆 → 下次自动使用”的闭环，不再先扩人工审核 UI。
3. **再扩自动巩固与自然遗忘。** 按版本维护经历、当前状态、主题与主观沉淀，自动处理不确定与旧结论，保留稳定来源和回滚路径。
4. **Runtime 变成观察与控制工作台。** 默认展示自动形成的记忆、形成理由、时间/星图、自动巩固与淡出过程。人工修改/撤回是可选入口；不出现必须 Morii 消化完才能继续的“待审 backlog”。候选/暂存/已采用/存在争议的标识按新契约确定，不能把现有“人工已确认”按钮改名伪装自动化。

产品目标已经由 Morii 确认是全自动；**整套替换还是保留底座的具体工程方案尚未选定，也没有在本次对话中启动真实迁移、模型调用或部署。** 下一份实现设计应解决上述自动闭环，不再询问 Morii 是否愿意逐条审核。

[hold]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L705-L926
[merge]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L257-L334
[buckets]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/bucket_manager.py#L1-L75
[embedding]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/embedding_engine.py#L83-L104
[dream]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L1163-L1280
[decay]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/decay_engine.py#L225-L346
[config]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/config.example.yaml
[hook]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/.claude/hooks/session_breath.py
[hook-server]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L116-L250
[license]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/LICENSE
[migration]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/MIGRATION.md
[touch]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L648-L668
[reinforce]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/tools/trace/_reinforce.py
