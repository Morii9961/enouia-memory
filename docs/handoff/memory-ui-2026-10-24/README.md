# Memory 产品界面交接 · 2026-10-24

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

这份材料供 Claude 新 Session **接续已存在的 Runtime `redesign/explore` 探索**。Memory 的后端核心重构由 Codex 在 Memory 仓库承担；Claude 继续负责 Runtime 产品 UI 与现有设计探索。Memory 的格、轴、图和解释面应融入现有前端，Runtime 的独立功能与共享外壳不能因 Memory 核心重构受牵连。本轮只整理规划与正式启动提示词，没有修改 Runtime、升级 pin 或启动实现。

**最新产品目标：完全自动化，不需要 Morii 日常逐条审核。** 先读 [全自动与 Ombre 迁移说明](../../design/memory-evolution-2026-10-24/06-automation-and-ombre-migration.md)。主界面改为观察自动形成、巩固、召回与淡出的过程；人工纠正、撤回和历史审阅作为可选控制，不是前置步骤。下文当前 IPC/版本限制仍有效，旧人工审核优先路线已被替代。

正式后端接续以 [Core 重构与 Runtime 路线](../../design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md) 和 [Codex 启动提示词](../CODEX_MEMORY_CORE_REFACTOR_START_20261024.md) 为准。准备好的提示词只授权下一个正式 Session 完成 Core-1 新契约与纯规则；本轮没有启动其实现或真实 V2 写库。MCP 新接线暂停，已有实现保留；Workspace 旧 IPC 继续有效，切换 Host 不是前端重构前提。Claude 默认不执行 Codex 后端提示词。

## 先读的结论

1. Memory 已有来源、候选、可信审核、Canonical、修订/替代/冲突、Context Capsule、会话及删除屏障；这些能力不能被一个漂亮的卡片库取代。界面应把它们从工程术语变成可理解的关系。
2. MCP 是接入边界，不能证明“记忆的生长机制已经完成”。当前受限 MCP/Host 的本地合成证据，与 Runtime 是否采用、真实客户端能否使用、Morii 是否认可记忆质量，是不同问题。
3. 日常体验围绕“系统记住什么、如何形成、为何想起、怎样重评与淡出”。现有 pin 的人工审核流程作为兼容事实保留；全自动体验先清楚标为合成设计，后端具备真实契约后单独采用，不能由页面代点确认。
4. 不建议直接把 OmbreBrain-folio 整套覆盖 Runtime。应借鉴它的任务组织与观察窗口，保留 Enouia 的来源、权限、审核和恢复语义；代码复用还需逐仓核对许可证与实际依赖。研究结论由主交付包提供。

## 交接文件

| 文件 | 用途 |
|---|---|
| [UI_SPEC.md](UI_SPEC.md) | 信息架构、视图、视觉方向、交互、状态、可访问性和分期 |
| [CONTRACT_READINESS.md](CONTRACT_READINESS.md) | 哪些能力现有接口可做，哪些必须先补 Memory 契约；含“只读模拟”的副作用提醒 |
| [CLAUDE_START_PROMPT.md](CLAUDE_START_PROMPT.md) | 可单独复制给 Claude 的启动提示词 |
| [RUNTIME_BOUNDARY_AND_INTEGRATION.md](RUNTIME_BOUNDARY_AND_INTEGRATION.md) | 真实探索基线、Memory 消费面、非 Memory 隔离、共享外壳与分步采用规则 |

本机另附 [.local 内的可点击线框](../../../.local/handoff/memory-evolution-2026-10-24/design-study.html)：记忆格、时间轴/主题泳道、星图/等价列表和召回实验室共用一个 Inspector。它使用全部虚构的内存 fixture，没有网络、IPC 或持久化；属于设计提案，不是 Runtime 实现或真实记忆质量证据。

## 当前证据与接手时的核查

Memory 本次阅读基线为 `codex/mv8-mcp-boundary` / `9286cd8f89669d9ec3b92f943fd74072457536e7`。领域边界见 [Runtime 集成说明](../../integration/RUNTIME.md)、[设计数据模型](../../design/DATA_MODEL.md)、[Context 模型](../../design/CONTEXT_MODEL.md)；请求以 [workspace IPC v1](../../../contracts/ipc/workspace-v1.schema.json) 为准，返回值还需核对相应 revision 的 Core 构造与 Runtime 消费测试。集成说明顶部存在早期命令数量叙述，接手时应按精确 revision 的 Schema 和兼容记录核对，不能直接抄旧数字。

2026-10-24 已通过只读 Git 查询定位实际 Runtime 探索工作树：分支 `redesign/explore`，HEAD `98cd21261e510013c845b722f40ec50d861e0f72`；`git --no-optional-locks status --short` 无输出。此结论不包含 ignored 的 `.design/`，也不证明其他方向工作树没有改动。个人绝对位置只记于本机私有说明，不写入本公开交接。

随后通过原 Claude 主会话顶层元数据和当前进程真实 cwd，进一步确认它就是此前工作的主探索根。五个方向源码在各自 `redesign/01-barograph` 至 `redesign/05-microfiche` 工作树；主树、五方向、ignored `.design/` 须一起保留和接续。核查时它们仍在运行并有并行进展，不能仅复制根 HEAD 来重建完整工作上下文。本机入口说明已补全六树目录图。

该工作树 `docs/integration/memory-pin.json:5-8` 与 `apps/desktop/src-tauri/Cargo.toml:22-27` 一致，仍为 `56bc5eb8b08fb80cd6df892bf53d200ff23afc4c` / 47-command Workspace。`AGENTS.md:19-32` 定义现有探索范围；`DESIGN.md:6` 仍待定，`docs/design-decisions.md:19` 尚无选定方向。`.design/PROGRESS.md:22-31` 记录五方向原型接续，不能把本包的 Quiet 线框或三栏布局当作已选全局设计。接手必须重读当前分支状态和本机探索进度，保留它们，具体边界见新增隔离文档。

公开文档只记录仓库与契约元数据。未读取私有 Vault、初始化候选内容、聊天导出、个人会话或凭据。因此“初始化只提了基础信息”在这里是 Morii 的体验反馈，并非对真实库逐条质量审核后的统计结论。

## 九张参考图

图像由 Morii 提供，仅作为设计参考。原图含示例记忆与第三方水印；不得把其中正文抽成 Enouia fixture、真实记忆、产品文案或人员关系。原图随本机私有附件包保留，不进公开 Git，不自动转发第三方服务。

私有附件目录：仓库根下 `.local/handoff/memory-evolution-2026-10-24/reference-images/`。主包以 `ref-01` 至 `ref-09` 建立附件索引；若跨设备交接，需由 Morii 选择传递这一私有目录。公共文档保持可在没有原图时独立阅读。

| 图 ID | 参考结构 | Enouia 应吸收的任务 | 必须避免的误读 |
|---|---|---|---|
| ref-01 | 记忆格：列表/卡片，详情浮层 | 快速浏览、筛选、进入同一条记忆的证据与修订 | 收藏、亮点、打动等主观标记不等于事实已确认 |
| ref-02 | 月/日折叠时间线，日期导航 | 在某一天回到完整情境，显示精度与资料覆盖 | 导入日期不等于事件发生日期；没记录不等于没发生 |
| ref-03 | 三栏导入与逐条审阅 | 左待审队列、中候选编辑、右证据/冲突，局部保存草稿 | 文件解析完成不等于记忆已形成；相似不等于应合并 |
| ref-04 | Breath 五阶段模拟 | 看见查询、资格过滤、排序、预算与结果的因果链 | 现有 Core 不接受图中的情绪或权重滑条；不能伪造分数 |
| ref-05 | 配置页与提示词/权重 | 普通偏好与高级实验分层，明确生效范围与恢复入口 | Provider 密钥不进页面；实验参数不自动改生产策略 |
| ref-06 | 日历密度、审阅地图 | 分清收到材料、生成候选、已经确认三种覆盖 | 密度不是记忆质量或生活丰富度；不能用 Activity 代填 |
| ref-07 | 手机浏览、当日记忆、逐条审核 | 紧凑窗口保留浏览/证据/审核完整任务链 | 响应式设计不等于已经授权独立移动客户端或云同步 |
| ref-08 | 主题泳道、证据密度、事件点 | 观察生活主题如何交错，并能展开到来源 | 主题归类和连线是可修正的解释，不是关系或因果证据 |
| ref-09 | 月份并行事件轨道 | 多条持续事件并置，显露重叠、未知区间与中断 | 横条跨度不证明期间每天都发生；模糊日期不能精确画线 |

## 开源参考入口

- [Latent-memory](https://github.com/oliscatt/Latent-memory)：记忆形成/巩固机制的研究入口。
- [Ombre-Brain](https://github.com/P0luz/Ombre-Brain)：记忆核心与召回机制的研究入口。
- [MemoryConstellations](https://github.com/ClaraShafiq/MemoryConstellations)：时间、主题与记忆关系可视化的研究入口。
- [OmbreBrain-folio](https://github.com/ceshihaox-dotcom/OmbreBrain-folio)：浏览、审阅、时间线和模拟界面的研究入口。

这些是用户指定的项目入口，不代表本文件已完成四仓代码、安全或成熟度审计。具体实现/许可证结论以主交付包的固定提交与文件证据为准，不以截图或 README 宣传代替。

## 本次验证范围

仅阅读源文件、Schema、既有设计和参考图，并检查本目录文档的链接与术语一致性。没有运行测试、构建、Schema cross-check、Runtime surface checker，也没有刷新手工项目状态快照。本目录不改变现有契约或实施计划的阶段授权。
