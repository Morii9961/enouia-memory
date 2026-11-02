# 前后端契约可用性与缺口

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

本页是设计接线清单，不定义新 wire 字段、接口名、Schema 或版本号。现有能力必须按 Runtime **实际 pin** 的源文件核实；Memory 新分支上存在某能力，不等于产品已采用。未来字段、状态机、兼容策略与测试必须先在 Memory 的授权设计切片中确定，不能由前端 mock 倒推成正式契约。

**最新产品方向已改为全自动，见 [06](../../design/memory-evolution-2026-10-24/06-automation-and-ombre-migration.md)。** 下文描述的 owner plan/confirm 是现有接口事实，不是未来必须保留的日常交互。需要真正的系统自动准入/巩固契约；Morii 不应被要求清空待审队列，前端也不能自行替系统伪造 owner 确认。

当前真实Runtime前端接续点为`redesign/explore` / `98cd21261e510013c845b722f40ec50d861e0f72`，已只读确认其Memory pin仍是`56bc5eb8b08fb80cd6df892bf53d200ff23afc4c` / 47-command Workspace。正式后端路线转入 [07](../../design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md)，本次仅Core-1新契约与纯规则，未开启真实V2写库。MCP新接线暂停、已有实现保留；旧Workspace仍可用，Host切换不是前端重构前提。

## 0. 三种采用状态必须分开

| 状态 | 页面行为 | 可宣称的事实 |
|---|---|---|
| 旧 pin 保留 | 真实原生界面继续当前已采用IPC与权限/确认/恢复语义 | 旧能力仍可用；没有完成全自动核心迁移 |
| 新能力合成预览 | 独立标记的fixture呈现新形成/巩固/召回/淡出体验，无真实库写入 | 设计可以讨论；不是正在后台运行的生产功能 |
| 单独受控采用 | 后端契约与出口通过后，由指定Runtime集成负责人串行改pin/lock/adapter/消费面 | 只声明实际采用且验证的能力；不并行改pin |

Memory后端交付领域契约、兼容与失效语义、Memory区域挂载所需的数据/动作边界。Runtime如何挂载React组件、共用token、设置路由、窗口和native lifecycle由Claude现有前端/集成权威决定，Memory不提供替代整个Runtime的壳。详见 [隔离与集成说明](RUNTIME_BOUNDARY_AND_INTEGRATION.md)。

## 1. 核对来源

| 内容 | Memory 内的权威指针 |
|---|---|
| 页面请求、参数、key要求 | [workspace-v1.schema.json](../../../contracts/ipc/workspace-v1.schema.json) |
| 返回字段及编排行为 | [workspace lib.rs](../../../crates/enouia-memory-workspace/src/lib.rs)、[views.rs](../../../crates/enouia-memory-workspace/src/views.rs)、[history.rs](../../../crates/enouia-memory-workspace/src/history.rs)、[materialization.rs](../../../crates/enouia-memory-workspace/src/materialization.rs) |
| 正式记忆和时间 | [memory-v1.schema.json](../../../contracts/memory/memory-v1.schema.json)、[DATA_MODEL.md](../../design/DATA_MODEL.md) |
| 召回解释 | [inspection-v1.schema.json](../../../contracts/context/inspection-v1.schema.json)、[capsule-v1.schema.json](../../../contracts/context/capsule-v1.schema.json)、[context.rs](../../../crates/enouia-memory-contract/src/context.rs) |
| Runtime 采用方式 | [RUNTIME.md](../../integration/RUNTIME.md)、[runtime-surface.json](../../integration/runtime-surface.json)；Runtime 自身的 pin、adapter 和消费测试 |

IPC v1 的请求 Schema 不冻结所有 `result` 内部字段。有些返回是 camelCase wrapper，有些嵌入 snake_case 存储记录；不要统一大小写后丢字段，也不要对“旧版本没有字段”显示虚构默认值。未知安全语义应关闭相关动作并解释能力不足。

## 2. 现有接口能完成的工作

以下为已阅读 Memory 源码的行为；Claude 须在真实 Runtime pin 复核后才接线。

| UI任务 | 已有能力 | 限制/正确表现 |
|---|---|---|
| 列表/卡片 | `memory_list` | 返回页、总数、snapshot、cursor；默认按 `updated_at` 排列，不是事件时间；默认仅status active，不提供currency，不能据此叫“全部当前有效记忆” |
| 字面搜索 | `memory_search` | 可含历史、cursor、partial、ranking版本；没有逐维评分或可调权重；includeHistorical仍排除archived |
| 详情 | `memory_read` | 仅以memoryId读取当前pin的最新rev，无指定revision读取参数；wrapper含summary/record/evidence/supersededBy；record字段按其精确存储Schema解释 |
| 来源原文 | `source_excerpt` | 明确source修订、字节分页、来源角色、发生/收到时间、附件引用、untrusted；不是全库来源检索 |
| 候选队列 | `candidate_list` | pending页与目标版本/冲突；不等于所有终态候选历史可查询 |
| 审阅 | `review_plan` / `review_discard` / `review_confirm` | 先展示精确diff，再确认；过期/冲突重规划；页面当前只可编辑editedContent，不能传details/分类/时间；真实写入继续原授权边界 |
| 纠正/遗忘 | 现有提案/plan流程 | 不改正式记录直写；遗忘与永久清除不由UI同义替换 |
| 导入 | 当前pin已有的pick/import/operation组合 | 原生选择只给token/displayName；不得让页面直接传文件路径 |
| 会话时间与来源 | `session_list` / `session_detail` | 只用已返回的事件/分支/持久化状态，不臆造丢失分支或全文 |
| 召回解释 | `context_inspect` | 看纳入/排除、reason/rank/token_cost等真实inspection；政策/删除变化可能拒绝读取 |
| 新召回预览 | `context_preview` | **本地写入**Capsule/Inspection，返回saved_preview；不是只读计算 |
| 历史任务/批次 | `history_*` 家族，按pin逐项核对 | 53/55命令能力不能按名称猜；准备、本体审查、发送和结果应用分开 |

### 一个特别容易做错的接口

`context_preview` 的现有 arguments 只有 query、可空 sessionId 和 branchId；请求必须带 idempotencyKey。它调用编译服务并保存产物。现有界面不能传“情绪、唤醒度、时间权重、重要性权重、遗忘速度、topN”去改变该服务，也不能拖动 slider 时对真实 Vault 连续保存。

因此合成实验室不调用此命令；查看已有解释只使用已有标识与受当前策略约束的读取。以后若明确获批保存新预览，界面必须告诉 Morii 会产生本地记录；发送外部 Provider 又是另一个需要审查确切请求的动作。

### 记录状态、修订与权限的额外门槛

- `memory_list` 的 active/inactive 是存储管理状态，不是业务当前性；active仍可能过期或冲突，未来生效的替代链下superseded也不等于当前业务失效。summary没有完整currency，禁止推导current_supported。搜索返回的currency按其实际语义呈现。
- 点击列表某revision后，`memory_read(memoryId)`可能已读到新revision。必须比对所选版本和响应版本，旧摘要、旧时间、旧关系与新正文不能混拼；提供刷新提示与重新选择。浏览旧修订正文需要新契约，不能靠当前接口伪装。
- archived可用`memory_list(includeInactive=true)`与`memory_read`管理查看；`memory_search(includeHistorical=true)`仍排除archived。管理可读不等于自动召回可用。
- MemoryRead、SourceRead和ContextRead是不同权限。图、轴、密度聚合需要明确授权的独立投影；不能为现有六工具MCP暗加聚合能力或暴露隐藏对象的数量/关系。
- 数据模型有subject/project/tags/category/valid/volatility/priority/episode/project_state字段，不代表Workspace有相应编辑接口。`review_plan`的DecisionArg只有editedContent，Core的EditAccept使用details:None。正文之外的编辑等待明确IPC扩展或既有后台候选生成路径，不在前端硬传details。

## 3. 可以先做，但必须准确标明范围

| 视图 | 当前可交付的真实版本 | 不应声称 |
|---|---|---|
| 时间线 | 记录更新时间线；已加载对象中有明确事件字段者可独立展示 | 全库人生时间线、完整月份覆盖 |
| 星图 | 显式选定有界集合的来源/替代关系；每条边可追踪 | 全库关系图、真实关系强弱、语义簇已经存在 |
| 日历 | 当前加载集合按明确时间口径分组，并写范围 | 全部收到材料、全库独立证据密度 |
| 筛选 | 后端已提供的条件；或明确“当前已加载范围”的本地筛选 | 全库标签/情绪/主题facets与精确计数 |
| 召回管线 | 真实inspection已有原因、预算、完整性、排名 | 图中五阶段每一步都有测量、可重放时长或逐项分数 |
| 生命周期 | 状态/时效/替代/删除等既有规则解释 | 自主巩固、自然遗忘、主动唤醒已运行 |

不要为获得全量图悄悄分页拉取全部正文，也不要把所有标签、敏感字段和完整来源文本复制进一个前端全局store。显示范围应与实际已授权响应一致，锁定/换库/撤回时清除所有派生结构。

## 4. 后端先行的设计请求

以下是待讨论的**产品数据需要**，不是已经冻结的字段名或接口；包的编号不等于 MV 阶段。

### C-A：时间与覆盖的授权投影

需要：事件发生区间/精度/时区与未知标记；系统收到/确认/修订时间；唯一对象身份与来源修订；按当前权限投影的时间窗分页、可解释统计、同一snapshot与覆盖局限；有界返回和详情跳转。

授权投影须独立定义其作用域；Memory读取授权不自动允许批量来源、Context解释、图边或统计读取。若投影对象版本与详情返回无法一致，先返回明确失效状态，不拼接跨版本内容。

必须决定：记忆、来源、候选、审阅分别怎么计数；跨日区间怎么聚合；重复导入和同源派生怎么去重；事实时间与已知时间如何选择；无权限对象不泄漏存在性。没有这些决定，不画“真实证据密度”或在浏览器自行重建历史权限。

### C-B：故事/实体/关联投影

需要：人/项目/地点/主题等稳定身份，别名与消歧状态；事件/故事段可调整归属；边的关系类型、出处、可信程度/待确认状态、版本、删除传播；有界邻域和聚合簇的授权语义。

必须决定：来源证据边、生成依赖边、同属性边、模型联想边分别表达什么；推断能否被 Morii 编辑/否定；如何保留旧版本而不泄漏已删除内容。主题相同不等于因果，也不等于真实人际关系。

### C-C：无副作用的召回实验

需要：只在合成或明确授权沙箱中运行，参数快照、算法版本、候选资格、排序解释、预算与排除原因、对照运行、覆盖局限；可重复的时间推进与取消。

必须决定：是否完全不持久化、是否产生审计、与真实记忆隔离方式、读取权限、结果失效条件。资格/权限/删除门槛不可调低；不能把暴露隐藏条目的排除原因作为实验便利返回。若未来允许真实只读实验，需独立契约明确其副作用，不能把现有保存服务换一个按钮名。

### C-D：形成、巩固、唤醒与退出日常使用

需要：多维抽取候选、跨来源支持与反证、情境化偏好、短期到长期的可解释提议、巩固/复核记录、可用强度、主动唤醒理由/冷却/静默、可逆的淡出与归档、删除的独立确认。

必须决定：什么变化只影响排序、什么产生待审提议、什么需要 Morii 确认；算法读到/模型重复生成是否能增加强度；自然时间如何推进；哪些记忆可以受保护；“遗忘”是减少主动出现、归档，还是永久清除。Enouia 不应因为模型反复引用同一条记忆就制造独立印证，也不应把遗忘动画直接绑定删除。

数据从哪里来应先讲清：Morii 明确叙述、明确纠正、再次遇到相同情境、真实使用反馈、模型推断各有不同证据权重；未记录不填默认值。情绪与关系意义可以先成为待审标注，不能凭正文语气推出诊断或人物意图。

## 5. 跨仓接线规则

1. Runtime 改布局、交互、适配器与视觉；Memory 改领域、机器契约和安全语义。新产品UI落 Runtime，`apps/workspace`保留参考壳/验收角色。
2. 一旦 Memory 集成面实际改变，在同一实现提交生成 runtime-surface 并登记 RUNTIME.md。文档提案本身不触发重新生成，也不伪造聚合值。
3. Runtime 升 pin 必须是独立受控采用切片，核对 manifest/lock/pin与adapter；在当前design探索权限下保持旧pin。不得让后端Session和前端Session并行修改pin，也不能用path依赖、分支依赖、patch、本地fork绕开。
4. Host/MCP 接入和 UI 改版可以解耦；不要为做记忆格顺便迁移私人Vault、切云主或登记真实Agent。Activity/Usage不进入Memory模型、统计或IPC。
5. 浏览器合成演示与原生真实界面清楚分离。原生失败显示真实失败；不能自动回退到同外观fixture并继续显示“已保存”。

6. Memory会影响Runtime内Memory、Context、Sessions、Provider相关消费面，以及Home/Settings中的Memory摘要/管理插槽和快速搜索的Memory结果；这些不等于授权重写Home、全局Settings或整个shell。Activity/Usage、独立producer、Runtime Inspector的冻结演示、非Memory的窗口/启动功能保持隔离。源码位置和共享风险见隔离文档。

## 6. 验收如何分层

前端合成验收验证：候选/正式/历史/来源/依赖语义、视图互通、未知与部分覆盖、布局、键盘与异步状态。后端合成验收验证：时间/关系聚合、策略过滤、删除传播、同源去重、沙箱副作用和可重放性。真实 Provider、真实库质量、实际 Windows 窗口/屏幕阅读和 Morii日用接受分别记录，不能由设计稿、CI或模拟结果相互代替。
