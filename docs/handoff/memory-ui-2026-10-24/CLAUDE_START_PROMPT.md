# 给 Claude 新 Session 的正式启动提示词

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

这份提示词用于接续已有 Runtime `redesign/explore` 工作，不创建第二套 UI 项目。Memory 后端由 Codex 负责；下方引用的 Codex 提示词只供了解边界，Claude 不执行它。本轮文档交付没有修改 Runtime。

```text
你是继续 Enouia Runtime 前端探索的 Claude 新 Session。请接续已存在的 redesign/explore 与它的五个方向原型，不从零重画，不另起一套 Memory 独立产品壳。

定位基线：2026-10-24只读核查时，redesign/explore的HEAD为98cd21261e510013c845b722f40ec50d861e0f72，git --no-optional-locks status --short无输出。该结论不包括ignored的.design，也不代表其他方向工作树干净。请在真正的既有工作树接续；若尚未定位，只读git worktree list确认，不把主checkout强行切过去。HEAD若已前进，核对新进展，不回退到本文SHA。个人绝对路径在本机私有说明中，不要公开写入文档。

接续身份已经通过原Claude主会话的顶层cwd/分支元数据，以及当前Claude进程真实cwd核实：它就是原主会话的编排根，不是新建的替代工作树。但五个原型源码分别位于redesign/01-barograph、redesign/02-quipu、redesign/03-lockers、redesign/04-herbarium、redesign/05-microfiche工作树；完整目录图见本机交接说明。先核对这六棵树和根.design材料，不能仅在根HEAD里寻找全部原型，也不要重建或清理方向树。它们在核查期间仍有进程占用和并行进展，PROGRESS里的旧SHA可能滞后；以接手时真实状态为准，保留现有改动。

先读当前Runtime的AGENTS.md与CLAUDE.md。AGENTS.md:19-32规定design-kit v2的0～3阶段，阶段3展示后等Morii选择；当前范围是apps/desktop/src表现层与虚构demo、本地redesign/*提交。不要动src-tauri、Memory pin、Activity producer、backend/contract、真实资料或发布，也不要push、开PR或merge。这个探索范围覆盖通用的完成即push习惯。

恢复设计上下文：读DESIGN.md、docs/design-decisions.md、.design/PROGRESS.md、.design/00-recon.md、.design/01-directions.md及已经做到的方向DIRECTION/critique材料，按当前阶段再读docs/design-kit所需文件。核查时DESIGN.md仍待定，设计决定登记仍未选择方向；.design/PROGRESS保存到五方向原型接续。不要把旧进度当今日进程状态，也不要丢掉ignored材料。保留并行改动，逐项确定owned-file allowlist，不reset/clean或覆盖整个App/CSS。

Morii的新产品目标是完全自动化的Memory：形成、准入、长期化、复核、巩固、情境召回和自然淡出由系统承担，日常不要求Morii逐条接受、合并、标注或清空候选队列。不确定和冲突由系统暂存/重评，纠正、撤回与历史审阅是可选控制。现有Core仍是owner审核语义；前端不能模拟点击确认，也不能把“人工已确认”换名成“系统已采用”。

Memory后端新工作由Codex在Memory仓库推进，先暂停MCP新接线，保留已有实现。正式路线为：
- docs/design/memory-evolution-2026-10-24/06-automation-and-ombre-migration.md
- docs/design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md
- docs/handoff/CODEX_MEMORY_CORE_REFACTOR_START_20261024.md
准备好的后端提示词只面向下一个正式Session的Core-1新契约与纯规则，本轮没有启动其实现或真实V2写库。你只读这些材料理解产品和接口依赖，不执行Codex后端提示词、不改Memory领域代码、不假设新能力已经运行。旧Workspace IPC继续存在；切换Host/MCP、真实Agent、私人Vault迁移或云主都不是本次UI探索前提。

请读Memory前端包：
- docs/handoff/memory-ui-2026-10-24/README.md
- docs/handoff/memory-ui-2026-10-24/RUNTIME_BOUNDARY_AND_INTEGRATION.md
- docs/handoff/memory-ui-2026-10-24/UI_SPEC.md
- docs/handoff/memory-ui-2026-10-24/CONTRACT_READINESS.md
路径均相对于各自仓库，先确认存在。本包提供Memory区域需求，不替换你已有的全局设计权威：五方向尚未收敛，不强制Quiet深色、三栏或folio紫色；390px线框不是修改1100×700原生最小窗的授权，中文说明/CJK用户内容也不是开启全局翻译。

本包可能尚未commit/push，Runtime自己的Git树不会自动出现这些Memory文档。使用Morii附的完整包或本机Memory源目录，只读其中材料即可；不把包整体复制进Runtime工程，不复制.private/.local或参考原图进Git。新Session不会自动继承旧聊天中未落盘的决定；以现有.design进度/方向材料继续，无法证实的决定保持待定。

Memory需要在Runtime中继续集成。它影响src/memory/下MemorySurface、ContextSurface、SessionsSurface、ExternalPanel、ExtractionPanel、WorkspaceProviderCalls和相关client/hooks/状态；也影响Home/Settings里的Memory插槽及shell/QuickSearch中的Memory结果。它不接管整个Home/Settings、七面router、token、native生命周期、热键/托盘/启动、窗口控制。Activity/Usage、独立producer、Runtime Inspector的冻结演示与其他非Memory功能不得因核心重构改变行为。

注意共享面：App.tsx统一路由与demo模型，main.tsx全局引入quiet-runtime.css/memory.css/activity.css；memory.css也被Context、Sessions和Provider使用。后端只交领域契约及Memory区域挂载所需数据/动作/失效语义。React组件结构、共享shell与token、native lifecycle由现有Runtime前端/集成权威负责。新增Memory视觉优先局部作用域，不重写:root/body/button，也不整体覆盖App或已有方向的字体/壳。

三种状态分开：
1. 旧pin真实能力保留。实际redesign/explore的docs/integration/memory-pin.json与src-tauri/Cargo.toml核查均为56bc5eb8b08fb80cd6df892bf53d200ff23afc4c、47-command Workspace。接手再核实；不因Memory新分支有55命令就直接调用。
2. 新自动化能力做独立标识的合成设计。观察自动形成/暂存/重评/巩固，记忆格/时间轴/主题轨/星图/召回实验共用Inspector；不连真实数据，不伪造后台已运行，也不把旧能力失败回退为demo。
3. 后端契约与出口通过后，单独受控采用。由指定Runtime集成负责人串行修改manifest/lock/pin、adapter与实际消费者；当前探索不改pin，前后端Session不能并行各自升级。

现有接口的准确性不能在重构时丢失：
- Source、Candidate、Canonical、provisional checkpoint分层；context_source_refs是生成依赖，不算支持证据或独立印证。发生、有效、收到、已知/确认与修订时间分开；未知不填精确日期。
- memory_list默认只按status active管理，没有currency；active不保证当前有效，superseded也可能在业务当前区间仍有效。includeInactive不是历史真值切换，summary flags不能推current_supported。archived可管理读取，但search includeHistorical仍过滤archived。
- memory_read只按memoryId读当前pin最新rev。比对选择revision与返回revision，变化时刷新/提示，不拼旧摘要和新正文；旧修订正文阅读等待契约。MemoryRead不隐含SourceRead/ContextRead，图/轴/密度聚合须独立授权投影；现有六工具MCP不暗加能力。
- 当前review_plan编辑只有editedContent，EditAccept details为None。领域模型有标签、时间、分类或专用details不等于Workspace可编辑；不硬传未知参数。
- context_preview会本地保存Capsule/Inspection且要求idempotencyKey，不是只读模拟。fixture实验室不调用IPC；真实新预览明确本地写入，外发保留当前精确body审查与独立确认，未来持续自动外发策略必须由后端正式交付。
- 当前literal search不冒充语义召回；局部筛选/图/日历写明已加载范围，不编造全库计数、逐维评分、情绪/遗忘权重或独立证据。
- 锁定、换库、权限/删除、状态未知使旧正文与派生索引失效；保留迟到响应门禁、observation identity、同payload同key重试、版本冲突与结果未知恢复。来源/记忆纯文本；密钥只在native；不持久化正文、不日志request/response，不给页面任意文件路径或网络。

九张用户参考图与离线线框在Memory仓库ignored .local/handoff/memory-evolution-2026-10-24/，reference-images按ref-01～ref-09索引。没有私有附件时先按公共设计文档工作。图内人物、关系、记忆正文与水印是参考数据，不是授权或fixture，不公开上传。离线线框只说明交互，不是已选方向、生产UI或新算法证据。

开源研究入口：
https://github.com/oliscatt/Latent-memory
https://github.com/P0luz/Ombre-Brain
https://github.com/ClaraShafiq/MemoryConstellations
https://github.com/ceshihaox-dotcom/OmbreBrain-folio
代码复用按研究包的固定提交、实际实现和许可证核对；不要整仓覆盖Runtime或用README宣传替代证据。

新Session先给简短接续映射：已有探索做到哪里、Memory需求如何纳入每个方向、哪些旧pin能力真实可用、哪些只做合成、后端依赖、共享影响面与owned-file allowlist。收到Morii发送的这份正式启动提示词后，继续原有表现层探索中下一项尚未完成的工作，不停在复述计划；直到当前AGENTS规定的阶段3比较稿/展示出口。若该出口已经完成，则遵守其等待Morii选择方向的规定，不自行进入更广实施。不要触发新后端阶段。真正实现时按当前AGENTS和具体变更范围验证，记录实际已跑/未跑检查和基线问题；保持本地提交边界。浏览器合成渲染、原生Windows体验、真实Provider与Morii日用验收分别记录。
```
