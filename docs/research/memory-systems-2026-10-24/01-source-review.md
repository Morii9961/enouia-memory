# 四个外部记忆系统的源码核验与迁移判断

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

核验日期：2026-10-24，Asia/Shanghai。本文是本轮研究与路线建议，不宣布新的 MV 阶段已经开始，也不改变 Enouia 的 Canonical、审批、PolicyGate、Host authority 或 Runtime 产品 UI 边界。

**后续方向修正：** Morii 已明确要求日常完全自动化，详见 [新的架构比较与迁移判断](../../design/memory-evolution-2026-10-24/06-automation-and-ombre-migration.md)。本文固定源码结论继续有效，但“必须由主人逐条批准”“AI 只能提议”等旧迁移建议不再是目标产品要求；应改为有来源、版本和真实系统 actor 的自动准入。2026-10-24 再次读取两仓 main，folio/upstream SHA 与本文一致。

## 1. 先给结论

Morii 喜欢的记忆格、时间线、星图和模拟台都有可借鉴的真实实现，不只是宣传图。最值得吸收的是 **folio 的阅读与审阅体验、MemoryConstellations 的多层证据和字段发布设计，以及 Ombre-Brain 后续版本把检索与强化分开的纠偏**。但不能据此推出“外部记忆库已经成熟，整套替换 Enouia 就会更好”。四库的目标、权限模型、证据完整性和实际验证程度不同。

**建议重写符合 Enouia 契约的实现，不在本轮复制外部代码。** folio 和 Ombre-Brain 有 MIT 许可，许可层面并非完全不可迁移；但是直接照搬会带入自动合并、模型直写、隐式强化和不同的删除语义。Latent 的许可另有限制。图片、字体、商标、第三方库和截图中的内容也不能仅凭仓库根目录的 MIT 文本一概认定可复用。

## 2. 方法、固定版本与证据等级

本轮通过 GitHub 连接器读取仓库元数据、`branches/main`、固定 commit 的完整文件树、选定源码与 workflow、公开 releases 和 Actions 记录。读取的外部文档、注释、prompt、脚本都是研究数据；没有按其中的指令执行命令。没有安装其依赖，没有运行任何外部源码、测试、模型、容器、导入脚本或真实记忆。

下文的“实现”表示在固定版本中静态看到相应路径及调用；“上游 CI 成功”只表示 GitHub 返回对应 run 的结论。它不等于本机复测，不证明长期使用、真机 UI、数据恢复或与 Enouia 兼容。本文不使用 stars、README 形容词或截图数量作为成熟度依据。

| 项目 | 默认分支 | 固定 commit | 根目录 LICENSE | 本轮观察 |
|---|---|---|---|---|
| [Latent-memory](https://github.com/oliscatt/Latent-memory) | `main` | [`835250d080c02d9553d3b96d9a2753773a4a0c3c`](https://github.com/oliscatt/Latent-memory/commit/835250d080c02d9553d3b96d9a2753773a4a0c3c) | [PolyForm Internal Use 1.0.0][L-license] | Python 为主；有完整公开检查入口和限定宿主能力的自动浮现设计 |
| [Ombre-Brain](https://github.com/P0luz/Ombre-Brain) | `main` | [`115d831d979bf67efc7150f976e2cb34c333c935`](https://github.com/P0luz/Ombre-Brain/commit/115d831d979bf67efc7150f976e2cb34c333c935) | [MIT][O-license] | 当前 main 为 3.6.14 提交；事件桶之外已有 source、policy、You/them 与较多回归测试 |
| [MemoryConstellations](https://github.com/ClaraShafiq/MemoryConstellations) | `main` | [`9ad9825910bbe3f2947b2709d913a6cf11b3fbc8`](https://github.com/ClaraShafiq/MemoryConstellations/commit/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8) | [MIT][C-license] | JavaScript/SQLite/Chroma；modern 写入控制面与 legacy 消费面并存 |
| [OmbreBrain-folio](https://github.com/ceshihaox-dotcom/OmbreBrain-folio) | `main` | [`bf6887575fa8e6fa519279be9dc7e5886a29d2ea`](https://github.com/ceshihaox-dotcom/OmbreBrain-folio/commit/bf6887575fa8e6fa519279be9dc7e5886a29d2ea) | [MIT，注明原作者与 fork 修改作者][F-license] | 当前 main 是 2026-07-11 release snapshot，只有一个根提交，前端内容很丰富 |

### 许可与 lineage 需要怎样理解

- folio 的 [README][F-readme] 明确自述来自 P0luz/Ombre-Brain，并说明修改重点是 UI、便利功能与检索精度；[CHANGES.md][F-changes] 和迁移脚本提供演进线索。GitHub 元数据却是 `fork=false`，固定 commit 无 parents。因此可以说“作者自述的衍生版本”，不能声称已经通过 Git ancestry 证明它与当前上游的全部关系，更不能把 README 的“核心相同”扩展成当前两版源码逐字相同。
- 根目录 MIT 文本要求随软件副本或实质部分保留其版权与许可声明。未来若复制代码，应记录源仓库、完整 commit、路径、修改说明和所需 notices；先盘点引用文件及第三方资产的许可。本轮只做概念与产品参考，不复制实现。
- Latent 的 [LICENSE][L-license] 是限制性许可，不能按 MIT 处理。其 Copyright License 节包含 `you may not distribute the software`；Changes and New Works 等条款还需要结合用途逐条复核。这里仅记录原文限制，不替 Morii 作完整法律结论；迁移或分发前必须重新审阅完整许可。可独立研究其问题定义，按 Enouia 自己的契约实现同类行为。

### 成熟度证据与不能下的结论

| 项目 | 已读证据 | 可支持的判断 | 本轮没有证明 |
|---|---|---|---|
| Latent | [release workflow][L-ci] 为 Python 3.10/3.11/3.12 运行公开自检；固定 SHA 的[上游 run](https://github.com/oliscatt/Latent-memory/actions/runs/39892821527) 为 success；有 [v2026.10.20 release](https://github.com/oliscatt/Latent-memory/releases/tag/v2026.10.20) | 存在多 Python 版本的维护与自动检查，不是只写了一份 README | 没有复跑自检、校准集、用户真机/宿主兼容或长期质量 |
| Ombre | [workflow][O-ci] 包括静态、安全、覆盖率和隔离 Docker 集成入口；固定 SHA 的 [Tests run](https://github.com/P0luz/Ombre-Brain/actions/runs/34560303372) 为 success | 主分支该固定版本有上游 CI 证据，并且回归面比 folio 广 | 未读完整日志/制品、未重跑；不保证真实账号、长期存储、Windows 或视觉验收。其他分支较新的失败 run 不能套给这个 SHA |
| Constellations | [package scripts][C-package]、[testing 文档][C-testing]、modern/lane/config 契约测试文件 | 有明确的受控写入测试入口和消费者切换边界 | 公开 Actions API 返回 0 runs、releases 为空；不能说公开 CI 已绿，更不能说 modern 已完成真实生产召回 |
| folio | [workflow][F-ci] 默认只跑 scoring 与 feel flow，真实 LLM 测试为有 key 才运行；根目录另有 protected-surfacing 测试 | 存在聚焦回归代码，UI 和 API 有真实连接 | 固定 SHA 按 push 查询未返回 run；近期 success 是每日备份 workflow，不能充当 Tests 通过。未运行源码、API 或 UI，未验证备份可恢复 |

Actions 查询是核验时快照，不是维护健康承诺。release 标签与 main 的 SHA 不应混用；本文所有算法链接都固定到上表的完整 SHA。

## 3. Latent：值得学的是分层和“何时该查”，不是直接引入它的所有产品判断

### 真实实现

Latent 把长期交互拆成四层：常驻的人格/原则与里程碑索引、按需展开的事件正文、显式维护的未解决事项、会话续接线索。其 MCP 负责让模型能查，persona/宿主协议负责让模型知道什么时候查；两者不是同一个能力。[README][L-readme]

在 [`src/memory_retrieval.py`][L-retrieval] 中可以看到：

1. BM25 使用字符分词；默认无真实 embedding 时另有字符余弦代理，不能把这一档称为经过语义模型验证的向量记忆。
2. 词面、可选向量、图扩展按 RRF 融合；`rrf_fuse` 只看排名，图路线可用不同的 k。`MemoryIndex` 明确区分已校准、未校准的 embedding 阈值：未校准向量不能单独打开候选准入。
3. 主动 retrieve 与只读查询入口共用排序核心，但只有主动入口增加命中权重。主检索不会让无上限 weight 直接乘进 RRF；这比“查得越多就无条件排在最前”克制。
4. 换窗 `recall_recent` 根据近因和有上限的权重挑选，并明确不因为系统自动召回就加权；其近因偏好是一次选择策略，不是把档案随时间删除。
5. retraction 与 supersession 分账：前者认为原条目不应再作为证据召回，后者记录事实正常改变，历史意图下仍能沿链查看旧版本。正文、索引和来源定位相互关联。

[`draft_extraction.py`][L-extraction] 的初始化候选有明确的 source、精确 evidence 和锚号/范围校验；候选默认为未确认。它要求“事件、如何理解、当前状态”等不同信息。对 Enouia 最有用的是：**不要只抽一句总结，要保留说话人、逐字依据、事件时间和现在是否仍成立**。不能要求模型自己准确数字符偏移，再把偏移不准当作 evidence 不存在。

[`passive_recall.py`][L-passive] 存在真正的逐轮自动浮现服务：输入准入、候选筛选、冷却、来源校验、delivery 与 inspect 等逻辑。它不是把普通 MCP search 改名；需要宿主能在用户输入之后、请求模型之前注入，并控制这些内容是否写入对话历史。**有 MCP 连接不等于宿主已经有自动唤醒能力。**

### 适配 Enouia 时的边界

- 可以采纳“会话续接、未解决事项、稳定事实、事件正文分层”的思想，并保持它们不同的失效规则。
- 可以采纳“系统自动带回不等于 Morii 重新确认重要”的原则；自动召回不应自己制造强化信号。
- 不直接照搬“永不时间衰减”的产品结论。Morii 明确想要自然遗忘；Enouia 可让可见性/活跃性衰减，同时保留证据与所有权。稳定偏好、承诺、事实真值和临时状态不应共用一条衰减曲线。
- Latent README 中“记忆由 AI 自己决定如何修改、人的页面只能留言”的产品价值与 Enouia 的 owner review 不是同一种权力模型。Enouia 的人工修正、Canonical 晋升与审批不能让给模型判断。
- Latent 的 README 自己记录了 absent-query 判断与跨多窗口串联的弱项。不能因为返回 top-k 非空就宣称找到了答案；应允许“没有足够依据”。没有在本轮复测这些检索质量结论。
- `hut/` 有真实 HTML/JS 与资产，但文件名中有多份 `*-mock.html`/prototype。它适合探索情感化入口的隐喻，本轮没有验证所有页面的读写和宿主联动，不能把山屋静态画面当成端到端产品验收。

## 4. Ombre-Brain：比 folio 更新的机制，也仍需要逐项筛选

### 真实机制

它以带 YAML frontmatter 的 Markdown 事件桶为基础，保存标题、主题、tags、重要度、效价/唤醒度、创建/激活时间和可见性状态。`hold`/`grow` 创建或组织记忆，`breath` 根据调用意图召回，`dream` 提供近期材料给模型反思；这些名称是产品隐喻，不是记忆形成已经得到认知科学验证的证明。

[`src/decay_engine.py`][O-decay] 使用重要度、激活次数、时间、情绪和 resolved 等修正计算活跃度，低于阈值归档；特定类型或标记跳过普通衰减。归档是读取池变化，不是证据物理销毁。

[`SurfacePolicyVM`][O-policy] 把 spontaneous、search、automatic、importance、dream 区分开。相同条目的“别主动拿给我”与“我现在明确查它”不必产生相同结果；删除/墓碑/归档另有规则。这种 **按调用意图做准入、再评分** 的思路可迁入 Enouia，但还必须补上 Enouia 的目的、数据等级、目的地、有效授权与 Host 状态。

`retrieval/scoring.py` 还提供候选特征、gate、原因码和可序列化结果的类型。[评分类型][O-scoring] 中有语义、词面、时间、情感、未解决事项、承诺和图邻居；这是可观察评分的实现参考，不能仅凭这个模块存在就宣称所有生产入口已经统一使用它。实际入口仍要从 `bucket_manager.search`、`tools/breath/search.py` 与 `surface.py` 追调用。当前桶检索子分也有 [独立纯函数][O-bucket-score]，自发浮现则主要按衰减得分及配额选择。

### 非常值得迁移的纠偏：读取与强化分开

当前 main 的 [`tools/breath/search.py`][O-search] 明确不再在搜索结果上 touch；[`trace/_reinforce.py`][O-reinforce] 将强化收敛到显式选定的一条记忆。其[回归测试][O-reinforce-test] 覆盖反复检索不累权、精确 ID 查看不强化、自动浮现不强化和仅显式 reinforce 才 touch。这里的测试只是静态读到，不声称本轮运行。

Enouia 应更进一步把事件类型分开：

- 看过：访问/观测统计，可清除，不能证明事实成立。
- 用上了：最终上下文实际包含并被当前任务引用的信号，仍不等于 owner 确认。
- 明确重要：用户标注或经批准的强化事件，可以影响活跃性。
- 再次得到独立支持：新的原始来源支持同一 claim，才是巩固证据。

模型自己的回忆或总结不能再作为独立外部证据，否则它会把自己重复讲的内容“养成事实”。

### 来源层、关系和长期认识

[source layer ADR][O-source-adr] 与 [`source_store.py`][O-source] 有内容寻址、范围引用、哈希校验和显式 source-read；原文不参与日常浮现打分。借鉴点是 **正文可简洁，证据可回溯，读证据不顺便强化**。Enouia 继续使用自己的 Source/provenance/egress 语义；精确 ID 或标题只能表明选中意图，不能替代授权。

[`relation_store.py`][O-relations] 有真实的关系元数据规范、自动关联提示和人工移除/改型逻辑；自动关系仍是 hint，不等于已经证明实体之间存在事实关系。

You/them 是可选的长期认识层。服务里有支持桶、跨日期确认与晋升条件，来源被移除后会降级或失效；这比一次对话就生成永久人格标签更完整。[You service][O-you]、[them service][O-them]。但“模型确认过数天”仍不是 Enouia 的 owner approval，关于第三人的认识也必须继承其原始数据政策。不能用重复自我见证替代独立证据。

### 需要拒绝的当前上游行为

当前固定版本的 `run_decay_cycle` 对某些普通桶执行 **importance≤4、超过 30 天未激活便自动 resolved**；坏时间在这一分支按 999 天处理。它特意排除了 plan、letter 等类型，但普通事件仍可能被自动宣布结案。[源码 L323–L335][O-auto-resolve]

Enouia 不应复制这条语义：**旧了、未访问、没查到，不代表已经完成或不再真实。** 无法解析时间应进入 unknown/需检查，不应加速“结案”。衰减只能改变当前唤起策略，完成、取消、事实替代必须有来源或 owner 决定。

## 5. OmbreBrain-folio：保留它的 UI 优点，避开其机制债务

### 它不只是截图

固定树中存在 `v2/cells/`、时间线 `v2/`、`v2/network/`、`v2/console/`、`v2/mobile/`，并有真实 API bridge。比如 [cells bridge][F-cells]、[network bridge][F-network-bridge] 会读取 `/api/buckets`，详情再按 ID 拉取，并能调用创建、更新等 API。[server.py][F-server] 有静态托管、详情/编辑、导入、合并预览/提交、衰减配置、回收站和 debug 路由。因此可以把它作为“完整交互的源码参考”，不是只看一张概念稿。

本轮没有在浏览器启动它、连接其服务器或用真实数据验证。大量导出的 HTML、hash 命名 JS 和字体资产也不符合 Enouia 现有可维护组件结构；不建议把整包网页嵌进 Runtime。

### 形成、巩固与 feel 的实际语义

[`hold`][F-hold] 普通路径会做模型打标、相近桶合并等操作；feel 路径创建单独类型，写入模型的第一人称感受，并可把 `source_bucket` 标为 internalized，使原桶退出部分默认浮现。源码里的“内化”主要是 **派生记忆加可见性状态变化**，不是已经实现一套有独立证据和审批的长期巩固状态机。

[`dream`][F-dream] 取近期候选、提示模型反思，可借 embedding 提示相似 pair；若多个 feel 主题相近，提示模型可能将其 pinned。它本身主要在给材料和提示，后续靠模型调用工具落笔。相似度、重复 feel 与“确信”没有天然等价关系；Enouia 应将它变成可审阅的巩固提案。

模型感受可以是 Enouia 的一种主观注释，但需要清楚标注作者、生成模型/版本、依据和审阅状态，不能混为 Morii 的感受、原始事件或事实。原始记忆是否默认安静也应由独立 policy 决定，不能因为 AI 写了一个 feel 就自动证明已经“消化完”。

### 衰减：是真实规则，但不能当作通用记忆科学

[`calculate_score`][F-decay] 的主要结构为：

```text
活跃度 ≈ importance × activation_count^0.3 × exp(-lambda × 未激活天数)
         × 时间/唤醒度组合 × resolved 修正 × urgency 修正 × highlight 修正
```

默认 `lambda=0.05`；3 天内时间占比更大、超过 3 天情绪占比更大。protected/permanent 返回固定高分，feel 可返回固定分。resolved 和 internalized 的组合进一步压低分数；低于 archive threshold 才归档。这个模型易于解释，也容易暴露问题：

1. 同一分数混合“最近、情绪、重要、是否完成、是否需要再出现”，让不同产品动作互相牵连。
2. 三天处权重比例切换是分段规则，可能产生跳变；具体参数没有在本轮证实适合 Morii。
3. 激活次数和 `last_active` 同时影响保留与排序，错误强化会双重延长可见性。
4. protected、高亮、feel、重要度是不同维度，应分别表达“别自然沉下去”“更易看到”“主观注释”“价值判断”，不要只把它们都渲染成更亮的一颗星。

folio 的 `run_decay_cycle` 特意移除了按年龄自动 resolved 的行为，这一点比当前上游更适合 Enouia。归档路径是搬入 archive 并保留文件；软删除则进 trash，purge 才物理删除。[bucket 管理源码][F-delete]。因此不能把它的自然衰减误述为自动物理删除。不过整套 purge、全清回收站和用户操作授权语义仍需重做为 Enouia 的 impact preview、确认和持久审计。

### 强化反馈循环：这正是不能整搬的地方

folio 无 query 的自动浮现明确不 touch，值得保留；但有 query 的 `breath` 会对真正放入响应的桶后台 `touch`。[server L648–L668][F-touch-call]。`BucketManager.touch` 会刷新 `last_active`、增加 `activation_count`，再给时间相邻桶轻微加权。[touch/ripple][F-touch]

这会形成“为了核对频繁检索→更活跃→更容易再次浮现”的反馈循环。时间相邻也不能证明语义相关，debug 或批量查看不应替附近事件作重要性判断。当前 Ombre 已将搜索与显式强化分开；迁移 folio UI 时应采用这一纠偏，而不是复活旧行为。

### Breath 五阶段模拟台不是生产召回的精确回放

[`console-breath.js`][F-breath-ui] 确实有“输入→候选→评分→阈值→排序”的界面，默认 threshold=50、topN=4。它读取 [`/api/breath-debug`][F-breath-debug] 的分数，再在浏览器里 filter/slice。五阶段动画按定时器推进，与请求并行；它不是后端逐阶段事件流。

debug 路径遍历非归档桶、计算 topic/emotion/time/importance 与 warmth，返回分数最高的最多 50 条。生产 `breath` 另有可见性池、关键词与向量补充、冷启动、多样性、token 预算等分支；有 query 的少结果路径还有概率性的随机旧记忆浮现。因此：

- topN=4 是该模拟页面的客户端默认截断，不是 Enouia 应照搬的召回条数。
- 50 条返回中的局部结果不能冒充全库过滤证据。
- 用户调滑块看到结果变化，不代表生产配置或真实发送 payload 已相应改变。
- 随机惊喜适合单独的“偶然想起”入口；不能混进事实查询并充当找到答案，也必须受 policy、敏感度和预算约束。

Enouia 可以保留五阶段叙事，但应展示 **同一份真实 trace 的取证、准入、排序、去重/多样性、预算编译和最终选择**；dry-run 应使用冻结快照，不调用模型、不改变强化、不写 Canonical，也不签发可重用的实际发送授权。

### 星图的连线到底是什么

folio 的 [`buildLinks`][F-graph] 以共享主题 tag 连边，同日、双方高重要度和双方 feel 给边加分，最多保留 600 条。worker 有同样的导出算法。它从桶列表在客户端重算，是 **共现可视化**；不是有独立 ID、证据、有效期和审阅状态的事实关系存储。

这个区别应进入 Enouia 的图例和详情：

- “共享主题/语义接近”用提示边，标出算法来源；不能表示 A 导致 B 或两人有某关系。
- 经证据与审阅成立的关系边单独存储，带边类型、方向、来源与状态。
- 来源已失效、条目被删除或权限不允许时，图布局不能保留泄露关系的悬空节点/边。
- 600 条是渲染取舍，不是“没有显示就没有关系”；必须显示覆盖范围、过滤状态和替代列表。

## 6. MemoryConstellations：最值得迁的是证据管线，不能把旧模拟逻辑当成新架构已上线

### modern 管线的真实分层

[`memoryScribe.js`][C-scribe] 用复合游标读取消息，只提原子 observation，校验精确引文、说话人、时间和 domain，生成带来源指纹的 claim。失败不前进游标，有缩批与冷却；这解决的是批次可靠性，不是把失败内容丢弃。

[`memoryProtocol.js`][C-protocol] 负责 source fingerprint、claim/review、版本和任务；[`memoryEvidenceResolution.js`][C-resolution] 组织审查、职责、分类与阻断原因。来源变化、旧 revision 或 input hash 不符时，不应继续发布旧结论。身份解析与正式星座准入分开：普通小细节可以挂在已有实体下面，不能每个名词都自动变成永久星座。[identity][C-identity]、[admission][C-admission]

Episode 候选必须来自合格 claim；缺少稳定实体或计划/进行中事件可进入 transient，而不是伪装成已完成往事。[episode candidates][C-episodes]。Saga 是对已经发布事件的叙事组织，不是第二份可自证的事实库。[saga projection][C-saga]

[`memoryProfileFields.js`][C-profile] 将 Facts、Status、Judgment 分别发布、版本化并保留历史；非空 Judgment 限定 persona writer 与 persona hash，客观字段的生产者不同。对 Enouia 的启示是 **事实、当前状态和 AI 的主观看法不应由一段混合总结来回覆盖**。但它的宿主审查者类型仍不等于 Enouia owner approval；迁移时提案与批准必须映射回 Enouia 自己的权限。

### 检索和部署边界同样需要如实看待

[`memorySearchProjection.js`][C-search] 的现代检索会读取最近 500 个 projection、按查询词出现次数评分并截断；它不是全量语义索引或已经验证的高级记忆路由。中文、长历史、非字面查询和全库覆盖需要另外评测。不能因目录里有 Chroma 就推断所有现代读取已经走完善的向量检索。

[架构][C-architecture] 与 [porting status][C-porting] 明确区分维护链 running 和聊天 read-mode verified：前者不能证明真实人格模型已经消费新管线；文档说明参考部署仍是 running/legacy，宏观认知/直觉与完整消费者切换尚未完成。这是作者的声明，本轮没有连接其参考部署复验。

`memory.html`、`js/memory/{data,layout,render,panels}.js` 确实有 universe→galaxy→constellation→star 的多级探索，布局使用固定 seed 避免刷新抖动。[UI data][C-ui-data]、[layout][C-ui-layout]、[render][C-ui-render]。其桥既可来自关系资料，也可能由共享碎片推导；几何线条不能自动当成严格因果。现代后端存在不等于当前星图已全部消费现代发布字段，本轮不作这一保证。

### legacy 生命周期不应复制

[`lifecycle.js`][C-lifecycle] 的旧路径包含 active→cooling→frozen→tombstone：冻结时删向量，墓碑时把片段正文写成 `[expired]`。[`tasks/cron.js`][C-cron] 仅在非 modern 时调度这套生命周期；因此不能把它描述成 modern 管线统一行为。

Enouia 可以从中学习“临时碎片不永久占据活跃池”，但不采用自动清空正文来模拟遗忘。证据保留、索引移除、检索可见性和物理擦除应各有独立状态及授权。也不复制其旧宏观认知/人格融合机制；上游自己已经在 modern 模式封锁相关旧旁路。

## 7. 迁移决策矩阵

“采纳”表示纳入本轮路线建议，不表示已实现；“改造”表示保留问题解法但重写到 Enouia 契约；“实验”表示先在合成数据、冻结快照和影子评估验证；“拒绝”表示不进入产品默认路径。

| 能力 | 来源 | 决策 | Enouia 中的落点与前提 |
|---|---|---|---|
| 记忆格/列表、筛选、详情 drawer、来源跳转 | folio | 采纳交互 | Runtime 产品 UI；使用已发布的后端投影，不让前端重新断言真值 |
| 导入三栏：待审队列/原文与提案/近似与冲突 | folio | 改造 | Source→Candidate→review plan/confirm；不能复制自动直写/静默合并 |
| 时间线、日历密度、并行事件轨道 | folio 与用户图 | 改造 | 先补 event/source/recorded/valid 时间和区间不确定性；热度是记录密度，不是生活价值 |
| universe/galaxy/constellation/star 逐级探索 | Constellations | 采纳交互 | 稳定实体投影+可缩放图；键盘与列表替代；布局位置不作语义事实 |
| 共现/语义提示边 | folio | 实验 | 派生缓存，注明算法；不越过 PolicyGate，不自动成为 Canonical 关系 |
| 有来源的实体、别名、关系准入 | Constellations | 改造 | Candidate/事实关系提案，带来源、版本、owner review 和撤销链 |
| 四层续接、未解决事项、当前状态 | Latent | 改造概念 | 不复制受限源码；分别管理时效、来源和作用域；不把昨日状态当今日事实 |
| Claim→Episode→Saga/Profile 分层 | Constellations | 改造 | 可以共享 Source，但不能让派生叙事互相冒充独立证据 |
| Facts/Status/Judgment 分开发布 | Constellations | 采纳原则 | 事实真值、时效、主观作者分离；AI 判断保留为带标记的提案/注释 |
| source_read 的精确选中与只读证据回溯 | Ombre | 改造 | 继承 Enouia 目的/敏感度/目的地授权；ID+标题不是认证 |
| 检索与强化分开、自动召回不强化 | 当前 Ombre、Latent 部分路径 | 采纳原则 | 每类访问事件独立记录；强化有明确来源，不能靠 UI 打开次数升级事实 |
| 有目的的自动唤醒 | Latent、Ombre | 改造+影子实验 | Runtime 宿主触发、Memory 出选择结果；先有关闭、冷却、去重、敏感度和原因码，再讨论主动呈现 |
| 时间/情感衰减 | Ombre/folio | 实验 | 只作用于活跃性与召回机会；不改变 truth/approval；类型化保护规则，先评估漏召和骚扰 |
| AI feel/反思 | Ombre/folio | 改造 | 明确作者与主观性；不冒充 Morii 感受，不自动确认原事已结束 |
| Breath 五阶段解释台 | folio | 改造 | 冻结快照+与生产同核的 trace；模拟结果不能换取发送授权或写入事实 |
| 重复模型自我见证自动升永久身份 | Ombre/folio | 拒绝为默认 | 独立证据和 Morii 决定优先；可保留主观假设，不能自动晋升 Canonical |
| 搜索命中即 touch、时间涟漪自动强化 | folio | 拒绝 | 阅读、使用、重要、独立支持分账；不制造自我强化正反馈 |
| 老化自动 resolved；坏时间按极老处理 | 当前 Ombre | 拒绝 | unknown 时间保持未知；完成/取消需要证据 |
| 自然遗忘时清空片段正文 | Constellations legacy | 拒绝 | 先降低默认可见性；物理擦除必须走 Enouia 明确流程 |
| 概率随机旧事混进检索答案 | folio | 拒绝为检索默认 | 可做单独“偶然想起”实验，标明非查询证据且受用户开关控制 |
| 批量导入、模型打标直接发布正式事实 | 多库旧路径 | 拒绝 | 导入完成不等于初始化被人工接受；必须保留 candidate/approval 状态 |
| 由配置页面直接保存 Provider key/任意 endpoint | folio 等 | 不迁移实现 | 使用 Runtime 现有受控 Provider 配置；Memory 不接管账户/Activity/Usage |

## 8. 按 Enouia 的路线迁入，最先补的不是更多视觉花样

### P0：冻结维度与权力，再做多层展示

先定义一条记忆是 statement、event、preference、commitment、current-state、relationship、subjective reflection 还是 hypothesis；这些类型可以重叠，但不得把派生推断当作原始证据。每项至少有来源、说话人/主体、事件时间及精度、记录/提取时间、有效区间、证据支持与冲突、敏感度、审阅/批准状态和 supersession/withdrawal 信息。

将三个问题分开：**它是否有依据、它现在是否仍成立、它现在应不应该出现。** 候选数量增长、图更漂亮、某项分数更高，都不能替代这三项判断。

### P1：先让初始化可复核，再增加唤醒质量

用既有 Source/Candidate 链补做“维度覆盖与缺口清单”：哪些来源已处理、哪些时间段/附件/角色未覆盖、哪些只是猜测、哪些不能归属。补提取应形成新候选和 diff，不改写已审定的事实，也不声称基础对话抽取已经完成关系与时间理解。

第一批产品 UI 优先做记忆格/列表、详情与来源、时间线、导入审阅。它们能直接暴露当前核心逻辑的缺项；星图和模拟台随后复用同一投影，避免四个页面各算一套“重要度”。

### P2：用可回放的影子实验选择唤醒与遗忘策略

候选规则先跑合成时间推进和冻结数据回放；对比现有策略与候选策略的漏召、误召、冲突、过期状态、零答案查询、敏感度阻断和上下文预算。不要先把 lambda、top-k、情绪坐标做成一堆滑块再让 Morii猜最佳值。

必须涵盖的反例包括：长期未提的重要承诺、频繁 debug 的无关事件、虚构角色与真实人物同名、计划取消、同一事实随时间变化、来源撤回、一个 feel 派生多个 summaries、被系统自己反复召回、没有答案、模糊时间、时区边界、大库局部截断和失效授权。

这些只是设计验收用例，不是已经运行的结果。新增实现阶段仍由 Morii明确选择；外部源码不是阶段授权。本轮结束于研究、路线与 Claude 前端交接材料，未迁移代码、未处理真实私有记忆、未连接真实 Provider，也未修改集成协议。

## 9. 固定源码索引

以下链接均指向本轮固定 commit，供 Claude 与后端开发者核对；不要只追 `main` 后在不同日期混用证据。

[L-license]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/LICENSE
[L-readme]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/README.md
[L-ci]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/.github/workflows/release-checks.yml
[L-retrieval]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/src/memory_retrieval.py#L263-L287
[L-extraction]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/src/draft_extraction.py#L484-L554
[L-passive]: https://github.com/oliscatt/Latent-memory/blob/835250d080c02d9553d3b96d9a2753773a4a0c3c/src/passive_recall.py#L965-L1040
[O-license]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/LICENSE
[O-ci]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/.github/workflows/tests.yml
[O-decay]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/decay_engine.py#L180-L272
[O-auto-resolve]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/decay_engine.py#L323-L335
[O-policy]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/policy/surfacing.py
[O-scoring]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/retrieval/scoring.py#L11-L212
[O-bucket-score]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/retrieval/bucket_scoring.py
[O-search]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/tools/breath/search.py#L421-L433
[O-reinforce]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/tools/trace/_reinforce.py
[O-reinforce-test]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/tests/test_retrieval_reinforcement_decoupling.py#L96-L186
[O-source-adr]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/docs/adr/ADR-0001-source-evidence-layer.md
[O-source]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/storage/source_store.py
[O-relations]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/storage/relation_store.py
[O-you]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/you/service.py#L511-L540
[O-them]: https://github.com/P0luz/Ombre-Brain/blob/115d831d979bf67efc7150f976e2cb34c333c935/src/ombrebrain/them/service.py#L848-L877
[F-license]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/LICENSE
[F-readme]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/README.md
[F-changes]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/CHANGES.md
[F-ci]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/.github/workflows/tests.yml
[F-server]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py
[F-cells]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/v2/cells/ombre-bridge.js
[F-network-bridge]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/v2/network/ombre-bridge.js
[F-hold]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L705-L785
[F-dream]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L1163-L1280
[F-decay]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/decay_engine.py#L134-L217
[F-delete]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/bucket_manager.py#L1055-L1172
[F-touch-call]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L648-L668
[F-touch]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/bucket_manager.py#L1198-L1270
[F-breath-ui]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/v2/console/assets/console-breath.js#L326-L475
[F-breath-debug]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/server.py#L3462-L3548
[F-graph]: https://github.com/ceshihaox-dotcom/OmbreBrain-folio/blob/bf6887575fa8e6fa519279be9dc7e5886a29d2ea/v2/network/assets/e964578f-ced4-4468-bb7d-984e769e4f55.js#L37-L88
[C-license]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/LICENSE
[C-package]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/package.json
[C-testing]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/docs/TESTING.md
[C-architecture]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/docs/MEMORY_ARCHITECTURE.md
[C-porting]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/docs/PORTING_STATUS.md
[C-scribe]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryScribe.js#L30-L170
[C-protocol]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryProtocol.js#L160-L261
[C-resolution]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryEvidenceResolution.js
[C-identity]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryConstellationIdentity.js#L6-L100
[C-admission]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryConstellation.js
[C-episodes]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryEpisodeCandidates.js
[C-saga]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memorySagaProjection.js
[C-profile]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memoryProfileFields.js#L48-L98
[C-search]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/memorySearchProjection.js#L64-L71
[C-ui-data]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/js/memory/data.js#L80-L190
[C-ui-layout]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/js/memory/layout.js
[C-ui-render]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/js/memory/render.js
[C-lifecycle]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/services/lifecycle.js#L14-L95
[C-cron]: https://github.com/ClaraShafiq/MemoryConstellations/blob/9ad9825910bbe3f2947b2709d913a6cf11b3fbc8/tasks/cron.js#L69-L76
