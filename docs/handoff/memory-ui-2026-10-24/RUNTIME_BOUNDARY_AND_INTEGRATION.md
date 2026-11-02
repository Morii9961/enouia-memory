# Runtime 接续、隔离与采用边界

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

日期：2026-10-24。本文基于真实 Runtime 探索工作树的只读核查，不修改 Runtime、不启动后端实现。个人绝对工作树位置仅保存在本机私有交接说明；公开接续标识为分支 `redesign/explore` / HEAD `98cd21261e510013c845b722f40ec50d861e0f72`。

## 1. 接续现有探索，先保住正在做的工作

只读 `git --no-optional-locks status --short` 无输出，说明核查时该工作树没有 tracked/untracked 修改；这不包含 ignored `.design/`，也不覆盖五个方向工作树。Git HEAD 只包含设计工具包安装，不代表全部探索成果都在该提交。

补充身份核验：原 Claude 主会话顶层 cwd/分支元数据与当前 Claude 进程真实 cwd 均指向这个编排根，确认它就是此前工作的主探索树。五个实现方向分别在 `redesign/01-barograph`、`redesign/02-quipu`、`redesign/03-lockers`、`redesign/04-herbarium`、`redesign/05-microfiche` 工作树；主树和五方向须作为一组接续。它们仍有进程与并行修改，旧 PROGRESS 不能替代各树当前 Git 状态。此次旧树清理完整排除这六棵树及其 ignored 资料；本机完整目录图随私有交接说明提供。

| Runtime 相对文件 | 已核实事实 | 新 Session 的处理 |
|---|---|---|
| `AGENTS.md:19-32` | design-kit v2，探索0～3阶段；每方向独立工作树；本轮只允许表现层与虚构demo，保留契约 | 继续原探索，不新建重复方案树、不覆盖旧成果 |
| `AGENTS.md:32` | 当前探索只本地提交；禁止src-tauri、Memory pin、Activity producer、backend/contract、push/PR/merge | 新Session不能套用通用“完成就push”习惯 |
| `CLAUDE.md:5-9` | AGENTS是共同规则，相关阶段再读design-kit；方向材料由各代理自己阅读 | 读已到阶段所需文件，保留其截图/批评口径 |
| `DESIGN.md:3-6` | 选定设计仍待定 | 不擅自把Quiet、folio、本包三栏或某方向填为最终设计 |
| `docs/design-decisions.md:19` | 尚无选定方向的记录 | 没有记录不构成选择，先还原实际进展 |
| `.design/PROGRESS.md:22-31` | 五方向原型接续，有各自WIP/提交与端口；最后保存进度为10月19日 | 这是保存的进度材料，不自动等于今日进程或完成状态；核对各方向当前状态后续做 |
| `.design/00-recon.md:7-13,38-66` | 英文产品、1100×700最小窗、7面导航、560×420快速搜索、契约与demo诚实标识 | 中文参考内容/CJK排版不等于开启界面翻译；390px概念稿不改原生窗口规格 |
| `.design/spec-issues.md:3-11` | 原生底色、测试类名、基线时序问题、demo Activity语义、共享颜色等已知问题 | 先作为已有背景；本包不顺带修它们或扩大授权 |

本次未发现 `.design/review/index.html`，不能声称阶段3比较稿已经交付。该结论不推断子方向工作树内没有更多成果。Claude 新Session应继续同一探索上下文、已有方向/批评材料与本地成果；不把“新Session”理解为重新开始产品UI。

## 2. 真正影响到哪些 Runtime 面

Memory 领域不只是一张 Memory 页面。后端契约变化必须识别所有已存在消费者，但不能因此接管整个 Runtime。

| 范围 | Runtime 相对源码与线索 | 隔离规则 |
|---|---|---|
| Memory/Vault/来源/候选/导入/纠正 | `apps/desktop/src/memory/MemorySurface.tsx`、`CandidateSource.tsx`、`CandidateConflict.tsx`、`SuggestionPreview.tsx`、`SavedExtractionJobs.tsx` | 主要Memory区域；新观察视图优先在此类局部边界设计 |
| Context/召回解释 | `apps/desktop/src/memory/ContextSurface.tsx`、`savedMockContext.ts`；`App.tsx:563-565` | 后端数据消费者；不等于可以独立改顶级路由或Context安全语义 |
| Sessions/checkpoint | `apps/desktop/src/memory/SessionsSurface.tsx`、`SessionEventBody.tsx`；`App.tsx:566-568` | 保留会话分支、持久化、provisional与恢复状态；新核心需兼容评估 |
| Provider发送/抽取 | `apps/desktop/src/memory/ExternalPanel.tsx:24`、`ExtractionPanel.tsx:18`、`WorkspaceProviderCalls.tsx:9`、`providerPreparation.ts`、`providerConfirmation.ts`、`providerSend.ts` | 是Memory相关调用面，不是Runtime一般工具系统；界面不能承担自动准入或外发策略 |
| Memory状态与挂载插槽 | `apps/desktop/src/memory/status.tsx:13,49,182,260` 的provider/badge/home/settings；`App.tsx:558,576,582` | Home/Settings容器仍由Runtime负责；只按已有插槽传递Memory片段 |
| 快速搜索结果 | `apps/desktop/src/shell/QuickSearch.tsx:5-7,72`调用Memory搜索 | 是共享shell中的Memory消费者；后端只交结果契约，不改窗口/热键/失焦行为 |
| 页面传输与native adapter | `apps/desktop/src/memory/client.ts:1-12,24-58`；`apps/desktop/src-tauri/src/memory.rs:1-17,360-387` | Workspace IPC seam仍在；native适配是受控集成任务，不由探索并行改 |
| native Provider配置 | `apps/desktop/src-tauri/src/provider.rs`；`main.rs:54-56,71-72` | 由Runtime持有原生设置/凭据边界；Memory领域仅提供能力与策略契约，不把密钥移到页面 |

浏览器另有 `apps/desktop/src/screens/Memory.tsx`、`Context.tsx`、`Sessions.tsx` 的虚构演示。它们与原生 `src/memory/*` 是两条渲染路径，不可只改demo便报告产品已采用。`App.tsx:560-568` 明确区分两者，`App.tsx:32-36`说明native与独立Activity来源。

## 3. 真正非 Memory 的功能及共享风险

| 对象 | 证据指针 | 默认处理 |
|---|---|---|
| Activity/Usage与独立producer | `apps/desktop/src/activity/client.ts:1-5` 仅activity_call/activity_setup；`ActivitySurface.tsx`；`src-tauri/src/activity.rs` | 不通过Memory IPC、不借记忆统计、不更改producer/调度/上传/账号 |
| Activity页面与浏览器演示 | `App.tsx:569-571`、`screens/Activity.tsx` | 不因Memory时间线/主题泳道而并入同一数据容器；旧demo问题单独记录 |
| Runtime Inspector | `App.tsx:572-574`、`screens/Runtime.tsx` | 仍明确是冻结Runtime-local虚构设计；不借Core重构复活旧backend |
| 窗口、托盘、启动与快捷键 | `src-tauri/src/shell.rs`、`hotkey.rs`、`startup.rs`、`companion.rs`、`main.rs:47-63,76-107` | 保留当前Runtime实现与权限；Memory重构不更改关闭/隐藏/退出语义 |
| 共享导航与视图模型 | `App.tsx:20-43,556-582` | 七面路由、快捷键、焦点、demo状态由Claude现有探索权威管理；Memory包只提出区域挂载需求 |
| 全局样式/token | `main.tsx:6-8`全局引入；`quiet-runtime.css:1-13`含:root/body/focus与reduced-motion | 不在Memory样式重定义全局token/body/button；新局部样式限定Memory容器，选定方向负责全局皮肤 |
| 原生与浏览器启动入口 | `main.tsx:24-27`统一root/error边界与overlay分流 | 不增加第二个运行壳、独立路由根或绕过现有boundary |
| Memory相关native生命周期/根租约 | `src-tauri/src/memory.rs`、`root_lease.rs`、`provider.rs`与shell协作 | 属共享集成责任；不是纯“非Memory无关”，也不因领域所有权归Memory就授权Codex修改Runtime |

`src/memory/memory.css`本身也给Sessions、Context、Provider组件使用；它不是只影响Memory Vault一页。旧`.qr*`类与颜色还被demo、原生Home、测试选择器使用。新Session应先声明允许改的局部文件与共享文件的精确插入点，不整体覆盖App、全局CSS或共享状态provider。五方向里若已改变token/字体/壳，Memory设计应适配各自方向，而非强行复位到本包线框。

## 4. 后端只交领域契约与区域挂载边界

Codex在Memory仓库输出以下内容；这是一份交付清单，尚不是新字段名或wire Schema冻结：

1. 自动形成/采用/暂存/重评/巩固/召回/淡出的语义，真实来源、版本、时间、关系、能力与完整性；机器采用与旧owner审核分开。
2. 兼容与能力矩阵：现有Workspace消费者能继续读什么，新投影要什么授权，未知能力如何降级，锁定/删除/换库如何失效。
3. Memory区域挂载所需的对象身份与修订绑定、列表/详情/时间/关系/解释的有界读取需要、动作/结果/恢复边界；不规定Runtime整页路由、React组件签名、布局token或native生命周期实现。
4. 合成案例和错误/空/冲突/缺失来源/不确定时间/部分覆盖语义；不包含真实记忆、私有导出、凭据或活动记录。
5. 每个后端切片实际出口、未实现项、Runtime须采用的差异与版本。只有集成面实际改变时才同步runtime-surface与RUNTIME兼容记录；规划文件本身不生成伪聚合。

当前正式后端入口为 [Core重构路线](../../design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md) 和 [Codex启动提示词](../CODEX_MEMORY_CORE_REFACTOR_START_20261024.md)。准备的正式后端Session只面向Core-1新契约/纯规则；本轮未启动实现或真实V2写库。MCP新接线暂停，已有实现保留；前端继续旧Workspace，不以Host切换或真实Agent接入为前置任务。

## 5. 采用顺序与责任

| 状态 | 责任方 | 允许发生的变化 |
|---|---|---|
| 保持旧pin真实能力 | Runtime现有adapter/前端 | 核查基线为56bc5eb/47命令，继续已有原生能力；不变更已保存资料 |
| 新能力合成设计 | Claude现有redesign探索 | 在独立标识的demo中讨论自动形成与格/轴/图/实验；旧能力失败不回退到demo |
| 后端契约/纯规则交付 | Codex的Memory Session | Core-1范围内交付并验证；不编辑Runtime，不把设计变成假生产进度 |
| 单独受控采用 | 后续指定Runtime集成负责人 | 契约/出口达标后串行更改manifest/lock/pin、native adapter与实际消费面；不与设计探索并行改pin |

现有探索的 `AGENTS.md:32` 明确排除pin和src-tauri。本包没有扩张这一范围，也不要求Claude为了“接上自动化”越权执行后端启动词。后续采用切片应说明自身明确授权与允许文件；不会因为用户确定了总产品目标就静默切换私人Vault、真实V2格式、云主、模型外发或生产。

## 6. 新 Claude Session 的最小接续成果

先还原实际探索状态、并行方向与已有成果；提交一份简短映射，列清“可以纳入现有方向的Memory观察设计 / 旧pin真实消费者 / 待后端契约 / 共享文件影响面 / 本次允许文件”。其后只按当前任务授权继续设计或表现层工作，本轮规划阶段不实施。

保护非Memory功能的验收不是宣称从此无需回归。后续真正改动共享文件时，按实际范围检查七面导航、窗口控制、Activity独立性、Settings插槽、快速搜索与Memory消费者；文档规划本轮不跑这些检查。探索规定的check/test/build由相应实现Session在需要交付代码时执行，并如实保留基线已知问题，不把本次只读核查说成已经通过测试。
