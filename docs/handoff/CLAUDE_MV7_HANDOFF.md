> Historical MV-7 handoff. On 2026-10-24 its local hardening branch was integrated with the current 58-command mainline; use docs/validation/Provider-hardening-mainline-integration-2026-10-24.md for the current merge boundary. The old branch/head and startup text below are historical, not new activation authority.

# Claude 接续：Enouia Memory · MV-7

更新：2026-10-11。范围：Memory 仓库内的 MV-7 功能实现与合成验证。

本文是提前准备的接续记录，不自行启动 Claude、其他任务、真实调用或发布。主人让 Claude 接手时，应先停止在同一工作树继续写入，避免两个任务同时修改同一功能。接手时以当前 Git 状态和适用的 `AGENTS.md` 为准；不要把本文的基线当成永远最新。

## 主人的授权与工作方式

- 主人已明确开始 MV-7，要求先完成功能，暂缓真实 smoke。当前只考虑 OpenAI 与 Anthropic；订阅所有权不证明 API 账户能力。
- 每完成一个独立功能就提交一次。当前授权是本地提交，没有本任务的 push、PR、公共分支合并或发布授权；本地整合已合并主线以接续实现，不向远端写入。
- 多个任务共用 Codex 限额。主人在本次 2026-10-10 接续中要求继续工作直至限额耗尽，覆盖此前约 25% 余量就停止新功能的约定；主人随后要求接近限额时停下工作，检查隔壁 Runtime 会话是否完成后关机，须为收尾保留额度。每一段仍须完成验证、保存和准确记录；不要自动使用重置额度或购买额度。Claude 使用自身会话的限制与归属规则。
- 2026-10-11 主人要求第二天继续 MV-7 工作。昨日停工和关机已执行，不将那次关机指令延续为本轮自动关机安排；保持原有真实 smoke / Runtime / 发布 / MV-8 边界。
- 不读取真实 API 凭据、私人历史、运行 Vault 或其他任务数据；不进行真实 Provider 请求、Runtime 激活或账户测试。
- 继续 MV-7 不代表开始 MV-8。下一阶段必须由主人明确启动。

## 接手基线

| 项目 | 记录 |
|---|---|
| 仓库 | Enouia Memory，独立 Cargo workspace |
| 分支 | `codex/mv7-provider-boundary-hardening`，本次接续的本地分支 |
| 最新已验证功能提交 | 本轮十项独立修复截至 `18e739c`；本提交本地整合已合并主线 `a62ca4d`（PR #7 的 Core 原生/抽取托管及待执行 smoke 手册），并完成整合后的全部规定检查。 |
| 本次 MV-7 起始基线 | `8197890` |
| 本次 Codex 接续基线 | `7c3ef41`；干净的本地 main 从旧 MV-6 版本快进到远端已合并主线后开始 |
| 工具链 | Rust `1.98.1`，`x86_64-pc-windows-gnu`；Python 3.12 / python-jsonschema `4.26.0` |
| 最近完整验证 | 2026-10-11，362 项 Rust 测试通过，含 83 项 Provider 检查和 64 项 workspace Core 检查；fmt、严格 Clippy、36-schema 独立交叉校验及 Runtime manifest 检查通过。 |
| Runtime 接入面 aggregate | `cda04231c3f1cb1314dd094abd6b669c7077d6b7f5525224e11a2a7f34258fc1` |

十项修复已经独立提交；本次主线整合、fixture 兼容与交接更新共同保存为本地 merge commit，保留两侧贡献历史。接手首先重新检查目录、分支、HEAD、工作树和其他任务正在使用的文件；保留他人的修改，不重置、不覆盖、不清理其他工作树。

已合并主线中的近期功能：

| 提交 | 结果 |
|---|---|
| `cead203` | 原生 HTTPS 使用自己的 Ring TLS 配置及固定平台证书验证器，不安装或继承宿主的全局加密配置；只显式引用已有锁定的验证器版本。 |
| `d66f00e` | Anthropic 累计 input/output/cache 计数不能倒退；矛盾更新永久失败，并保留此前有效用量用于失败记录和后续额度计算。 |
| `8d8281a` | `CredentialStore::delete` 增加可信原生设置的凭据移除入口；仅允许两个固定条目，成功或确认不存在可重复执行，其他系统错误明确返回。只做假调用验证，未操作实际凭据。 |
| `fe48ebb` | 原生响应头按 ASCII 大小写无关规则匹配 JSON/SSE 类型；不接受其他格式、通配符、混合类型列表或 Unicode 仿形。 |
| `1e1f685` | （Claude）重复 `Content-Type` 字段（含完全相同的重复）视为歧义并拒绝；401/403 分类、拒绝体零回调、保留 reservation 和不重发均不变。新增纯 `HttpsTransport::single_content_type` 供宿主离线 harness 使用，已记入 RUNTIME.md。 |
| `f61cba4` | （Claude）非流式原生请求原先发送两条 `Content-Type: application/json`、没有 `Accept`（第二次 builder 调用误用 Content-Type，且 reqwest 追加同名字段）。现在每个请求只带一条 Content-Type 和一条与模式匹配的 Accept。只构造请求不发送，未做真实服务器验收。 |
| `695be47` | （Claude，经主人确认）原生凭据须为可见 ASCII 且不含空格（0x21-0x7E），否则在 admission 前以不可重试的 `Unauthenticated` 本地拒绝。复现时未修复状态经真实 transport 向 OpenAI 固定端点发出过一次带合成凭据和合成请求体的 HTTPS 请求，详见 MV-7 报告；修复后不再联网。 |
| `dfaa7c2` | （Claude，MV-7.1 官方文档核对）当前 Anthropic 模型默认开启 thinking（`display` 默认 `omitted`），纯文本请求也会先收到 `thinking` 块，可能还有 `redacted_thinking`。旧解码器会让每次这类回复失败。现在两种模式都校验并跳过推理块，不暴露、不保存；`model_context_window_exceeded` 视为 `Length`，`refusal` 仍为失败。依据是官方文档中的合成形状，不是真实模型输出。 |
| `3f1671b` | （Claude，主人授权"需要 Runtime 协作就直接用"后）ADR-MEM-48：workspace Core 通过 `Workspace::set_native_provider` 接收宿主注入的 transport、secret store 和经主人核实的目的地；新增 7 条 workspace IPC 命令（授权/撤销计划、精确请求体准备、单次发送 operation、本地中断、状态），共 43 条。未注入时一律 `provider.not_configured`，参考外壳仍只用 Mock。关闭/锁定先关原生开关再 cancel/join。修正新 Session 取"第一条 Policy"的问题（授权也是 Policy 记录）。Provider crate 进入 Runtime 接入面。证据为假 transport/secret 的页面通道测试。 |
| `8186aed` | （Claude）workspace Core 托管可选抽取，授权增加 extraction 参数，新增准备、预览、逐条应用和暂停入口，共 47 条命令；仍用假 transport 验证。 |
| `a62ca4d` | （Claude）新增双 Provider 真实受控验收手册，状态为待执行；本文不启动该手册。 |

## 已有能力与当前阶段

本次 Codex 接续的本地工作：

| 提交 | 结果 |
|---|---|
| `5e5c0cb` | Windows 凭据保存与传输共享格式校验；含空格的密钥在写驱动前被拒绝，避免保存后才发现无法调用。两项假驱动回归，完整 332 项检查通过。 |
| `21881ea` | 凭据回归使用原生纯校验加无法联网的假 exchange；校验退化时也只能在假调用或 admission 断言上失败，不再意外向真实 API 发出请求。专项检查和严格 Clippy 通过，产品代码未改变。 |
| `f8d85b9` | Anthropic `message_start.content` 必须为空数组；原先带有文本的起始内容会被丢弃后仍返回 Completed。错误形状现在永久失败；合法空文本终态仍保留用量，持久化失败不允许重发。官方协议、独立探针和完整 333 项合成回归支持本次修复。 |
| `4d33d25` | 完整历史校验严格匹配 Completed/AssistantCompleted、Length 或 Failed/TurnFailed、Cancelled/TurnCancelled；4 个有效 Vault 历史和 16 项组合回归通过。完整 334 项合成检查及规定交叉验证通过。 |
| `207450a` | Anthropic 非流式拒绝回复在失败 ledger 保留经过验证的 reported usage，包括缓存归一化；11 种真假消息/计数/transport 控制和后续更高用量限额验证通过。完整 335 项合成检查及规定交叉验证通过。 |
| `67dc75c` | Completed/incomplete 终态文本不一致仍永久失败，但保留已验证的 reported usage。6 种纯解码情况和两种持久化/后续限额情况通过，错误 event/status 仍不接受计数。完整 336 项检查通过。 |
| `2c3ab0e` | 抽取的创建、读取、暂停/恢复、预览、准备和应用入口拒绝过期来源修订；校验期间来源修订或策略 epoch 变化返回冲突，无关会话提交仍允许。6 种状态/敏感度情况和 4 种并发情况已通过，完整 338 项检查及规定交叉验证通过。旧任务不自动升级为新修订。 |
| `72192a0` | 预先准备的抽取请求在发送授权后及 admission 使用的 head 上复核当前 Source 修订。6 种发送前情况、4 种流式变化和 3 种并发授权情况已通过，完整 341 项检查及规定交叉验证通过。不会撤回已交给 transport 的字节；局部输出事务的最后一道检查另见下一项。 |
| `693dc76` | Context 在 guarded 输出的同一 commit pin 上检查 input 对应抽取 job 的 Source 修订。12 种 pipeline、2 种直接 Core chunk 及 2 种历史回执情况通过；新过期输出拒绝，已发布精确回执、无内容失败记录和不可重发规则保留。完整 344 项检查及规定交叉验证通过。 |
| `d3acc0b` | 多个 job 引用同一 input 时，活动 job 校验及 admission 以 InvalidRequest 拒绝歧义预算/来源路由，保留原档案。8 种歧义及 2 种独立 input 预算控制通过；完整 346 项检查及规定交叉验证通过。 |
| `5cc47db` | 在 reservation 的同一 pin 上复核当前 pending 队列，prepare 后填满队列不再进入 exchange；审核后释放容量只允许尚未 admission 的 input 继续。6 种双 API 情况通过，完整 347 项及规定检查通过。 |
| `cdafc6f` | OpenAI failed 及三个拒绝的 incomplete 原因永久失败时保留合法 reported usage，兼容 nullable error；23 种 JSON、15 种 SSE 和十种持久化模式通过。完整 350 项及规定交叉验证通过。 |
| `f54ae07` | OpenAI created/in_progress 中经过校验的 reported usage 在中断后保留，未知分量不擦除已有值；倒退或跨 Response 覆盖永久失败并保留原计数。48 种纯解码和 7 种持久化情况通过，完整 352 项及规定检查通过。 |
| `37d403f` | OpenAI Completed/Length 共享 failed/progress 的 identity/nullable error 校验；Malformed reply 不保存成功或新计数，成功 fixture 补齐合成官方身份字段。40 种纯负例、合法控制和 12 种持久化模式通过；完整 354 项及规定检查通过。 |
| `18e739c` | 双 API 的 JSON body / SSE event 拒绝嵌套及转义后的重复成员，不选择首/末字段。两个 parser unit、20 种纯负例、合法 metadata/先前计数控制及八种持久化模式通过，完整 358 项及规定检查通过；根依赖未变。 |
| 本提交 | 整合主线 `a62ca4d` 的 Core 原生/抽取宿主；OpenAI 假回复补齐已要求的合成 identity 字段，不放宽产品校验。四项新增宿主检查及完整 362 项根目录检查、严格 Clippy、Python 交叉验证和新 Runtime manifest 均通过。 |

MV-7.2 的 Memory 侧原生调用链已实现：外部目的地编译、精确请求体 inspection、站立策略及私密内容单次批准、凭据读取端口、持久化 admission、流式文本、原子终态、用量和限额、取消、关闭与恢复规则。OpenAI Responses 与 Anthropic Messages 均有合成接受路径。

同一输入已 admission 后不能重发，包括失败、取消、崩溃或结果未知。已知用量不能退款式覆盖原始 reservation；失败流保留最后有效计数，缺失值保持 unknown。原生 HTTP 不自动 retry、fallback、redirect 或推断 proxy。401/403 保留结构化错误，错误体不进入文本/存储。SSE 支持碎片化 UTF-8、LF/CRLF/CR、开头 BOM、严格终态与永久失败锁存。

MV-7.4 的可选抽取已实现来源片段绑定、版本化提示、完整响应校验、原输入绑定、候选审核队列、暂停/恢复、拒绝项抑制、积压及预算限制。候选不能自动成为正式记忆；字面引用包含性不等于语义真实性。时间、分支不确定性、否定/限定上下文和真实用户偏好证据保留。

MV-7.1 的实际账户/模型能力、精确 tokenizer 和真实价格仍未确认。MV-7.3 的双 Provider 真实受控验收按主人要求暂缓。workspace Core 已能托管原生发送及可选抽取（ADR-MEM-48，工作线程取消/join 由 Core 负责）；参考外壳仍只用 Mock。新主线手册记录 Runtime main `36c6542` 已采用旧 Memory pin `8186aed`，包含原生配置、凭据入口及产品界面。本会话未核验 Runtime 当前 HEAD、未写入或运行其产品流程；今天整合后的 Memory 修订及新 aggregate 尚需 Runtime 采用验收。不能称真实模型已接通或 MV-7 阶段验收已完成。

## 必读入口与代码

1. 根目录 `AGENTS.md`、[阶段计划](../design/IMPLEMENTATION_PLAN.md)、[MV-7 报告](../validation/MV-7.md)。
2. [ADR-MEM-47](../adr/047-explicit-provider-dispatch.md)、[原生接入指南](../integration/PROVIDERS.md)、[Runtime 接入记录](../integration/RUNTIME.md)。
3. `crates/enouia-memory-provider/src/{client,codec,stream,transport,windows,egress,policy,vault,extraction}.rs`。
4. `crates/enouia-memory-provider/tests/providers.rs`：真实本地合成 Vault、假 secrets、假字节 transport；原生 builder/凭据移除的隔离 unit case 位于对应模块。
5. `crates/enouia-memory-context/src/session.rs` 中的原子 invocation output，及 contract 的 invocation/extraction 数据结构和 Schema。

历史 `CLAUDE_START_PROMPT.md`、`docs/design/CLAUDE_HANDOFF.md` 的旧 MV-0 主体仅是历史记录，不能覆盖本轮阶段和仓库边界。

## 下一项可独立接续的 Memory 侧工作

交接中的重复 `Content-Type` 候选已证实并在 `1e1f685` 修复：锁定的 hyper 1.11.1 逐条 `append` 收到的字段，旧代码的 `headers().get` 只读第一条，因此 `application/json` 后接 `text/html` 会被当作 JSON 接受。证据只来自合成 header map 和假 transport，没有真实服务器响应头或 HTTPS 验收。

Claude 随后审查原生传输与 SSE 路径，修复了非流式请求头重复 Content-Type 的问题（见提交表）。SSE 分帧、事件状态机、用量累计和日志持久化的审查未发现新的可复现缺口。

此前待定的凭据形状收紧已经主人确认并实现（见提交表）。`malformed_native_credentials_do_not_consume_private_approval_or_admission` 已改为调用真实 `HttpsTransport` 的纯 preflight/凭据校验，并通过无法联网的假 exchange 验证 Client 的审批/admission 顺序；不再把合成凭据交给真实 exchange。因此即使形状或调用顺序校验退化，测试也只能记录假调用并失败。随后按路线图做了 MV-7.1 中不需要账户的官方文档核对（结果见 MV-7 报告），修复了 Anthropic thinking 块问题。账户能力、模型上限、tokenizer 和价格仍待真实调用核实。

本轮复现的非流式 JSON refusal 用量缺口已经修复并完整验证，见本地提交表。对流式计数累计/倒退/矛盾、终态失败锁存、凭据 setup/read/send 边界和 reservation 下限的复查没有证实其他必须修改的缺口。原有并发 checkpoint 在上一轮全仓检查曾触发有界 WriterBusy，精确隔离及完整重跑通过；最新 335 项完整检查没有失败。2026-10-11 已复现并修复 OpenAI 终态文本冲突丢失合法计数的问题，完整 336 项验证没有失败。抽取入口的旧来源修订缺口也已复现和修复，完整 338 项检查没有失败。

预先准备请求的发送检查缺口已在当前工作中修复，完整 341 项检查及规定交叉验证通过。独立合成探针没有真实 HTTP；保持其他用途的历史快照语义和无关提交控制。

输出提交时机缺口已在当前工作中修复，完整 344 项检查及规定交叉验证通过。旧行为是在最后一次 SendGuard 之后、InvocationJournal 委托给真实 VaultAdapter 的 finish 之前合成改变 Source 修订，输出事务仅检查 policy/deletion epoch，保存 Completed 后才向 Client 返回冲突。独立 Journal 包装器仅注入来源变化，不伪造 Provider 输出或存储结果；直接 Core probe 另复现了新 chunk 的过期来源写入。修复后同一 commit pin 保护新输出，精确历史回执和无内容失败记录保留。终态提交被拒绝时保留 OutcomeUnknown；这不证明远端取消，不能重发 input。

共享 input 的预算缺口已修复，完整 346 项检查及规定交叉验证通过。存储校验允许多个不同 run 引用同一 input，旧 Native claim 按第一项选择预算。独立合成探针在验证干净的 Vault 中选择 max_reserved_tokens=1 的第二项，却采用第一项 100000 的限额，并记录一次假 exchange。当前 SDK 的 create 不生成这种图；现在活动 job 校验和 admission 拒绝歧义 input 路由，保留原存储/历史结构，不任意选择第一项或最新项。不同 input 的 job 仍按各自预算处理。没有真实模型调用、实际凭据或账户使用。

OpenAI response.failed 及三个已知拒绝的 incomplete 终态用量缺口已经修复。nullable error 不被误当作无效 Response；23 种 JSON、15 种 SSE 和十种持久化模式及完整 350 项根目录检查通过。仍拒绝输出，只保留经过响应/计数校验的用量，不采信任意错误体。官方定义用于形状核对，不是真实模型证据。

OpenAI 进度用量缺口已修复，48 种纯解码情况、7 种持久化模式及完整 352 项根目录检查通过。fmt、严格 Clippy、Python 交叉验证及 Runtime manifest 通过。计数只来自经过校验的 Response，可能是局部报告；官方 created/in_progress 示例 usage=null，共享类型允许 usage 不证明实际模型会在此时报告用量。

两个后续抽取候选未证实需要修改：已 admission 后增加同 input 的 job，四种双 API/数组顺序的假调用仍按原 dispatch 完成，所有 job 的 Source 修订必须与同一 input 完全一致；删除现有 job 的合成迁移被 extraction.history 拒绝，不能借此绕过原限额。没有更改 Core 或历史约束。

成功 OpenAI Response 身份校验已修复，40 种纯负例、合法 JSON 控制、12 种持久化模式和完整 354 项根目录测试通过，规定检查全部通过。仅将既有 identity/error 校验移入共享输出解析，不新增输出类型；成功 fixture 的补齐不改变发送请求。Malformed Completed/Length 仍保留已接受的局部流文本、原 reservation 和不可重发规则，不采信非法响应计数。

重复 JSON 成员缺口已修复，两个 parser unit、20 种纯负例、合法 metadata 和两种先前计数保留控制、八种持久化模式及完整 358 项根目录测试通过，规定检查全部通过。拒绝解码后相同的嵌套/转义字段，不选择 first/last，不将字符串中的 JSON 再解析。此前计数/局部文本与 reservation 保留；歧义事件不能完成或授权重发。RFC 8259 的 SHOULD unique 与适配器选择拒绝的策略已分别记录。根 Cargo 依赖及锁文件未变。

十项独立修复及主线 a62ca4d 的本地整合已完成，完整 362 项及全部规定检查通过。下一轮先核对 Git、适用 AGENTS 和主人授权，再审查有证据的 MV-7 缺口；不要将本表的合成接受证据当成账户/账单、真实 HTTP/TLS 或 Runtime 产品宿主接受证明。真实 smoke、实际凭据操作、发布、Runtime 产品接入和 MV-8 仍未授权。

2026-10-11 通过紧凑会话快照核对“继续 Enouia Runtime 工作”：会话为 idle，最新执行仍是 interrupted，最后评论为已验证/推送并开始收尾。此前已读取的保存/push 证据对应 64659d93e70eb8b6e1b7ff7ad8c7c833d53779b2；PR #29 当时仍为 draft，不能称该项目或真实验收全部完成。本仓库没有向该会话发送消息或写入 Runtime。

主线中的历史 Runtime 采用记录：`7963150` 更新原生宿主 pin，`9ae1039` 记录 ADR-032 的主人配置和原生凭据入口，`7ba612b` 增加 Sessions 的 Provider 面板；`bfeb6aa` 随后采用 Memory `8186aed` 的 47 条命令及来源抽取入口。新主线真实验收手册的记录基线为 Runtime main `36c6542`、Memory pin `8186aed`，因此旧的“分支未合并”说明不再作为当前状态。这里转述已合并文档，未在本会话独立运行 Runtime；其中默认 smoke 与假 transport 的接受结果不证明真实凭据、HTTPS 或双 Provider 验收。

下一次 Runtime 采用应在其授权范围内更新到本次合并后的 Memory pin，逐条采用 RUNTIME.md 的修复及 ADR-MEM-48 记录，再验证产品流程。凭据保持原生录入并以 UTF-8 交给 CredentialStore，不经过页面；模型和能力由主人核实，不能推断。真实验收手册仍待主人明确启动。

若没有证实新的 MV-7 缺口，不为消耗额度扩充无依据功能，也不转入 MV-8。Runtime 产品接入需主人给该仓库的明确工作范围；本仓库不添加到 Runtime checkout、crates、fixtures 或 target 的依赖。Activity & Usage 属于 Runtime，不在 Memory 中读取或实现。

## 验证和证据

最近一次（十项修复与最新主线的本地整合）的根目录检查全部通过：

```powershell
$env:CARGO_NET_OFFLINE = 'true'
$env:CARGO_INCREMENTAL = '0'
$env:RUST_TEST_THREADS = '1'
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python tools/schema-check/check_schemas.py
python tools/integration/runtime_surface.py
```

Cargo 使用仓库 pin；Windows GNU linker/toolchain 须在 PATH 中。Python 使用已配置的 3.12 虚拟环境及 `tools/schema-check/requirements.txt` 的固定版本，不临时更换验证器。

最新完整运行：362 Rust tests（83 Provider cases、64 Core cases），格式与严格 Clippy 通过。

独立 Python 结果：36 schemas、53 valid records、117 record mutations、4,913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction-job cases。Runtime manifest 与日志中的 aggregate 一致。合成证据不证明真实账户、账单、TLS handshake、Windows credential store 或实际宿主行为。

本地 `.local/mv7-main-integration-workspace-tests.log`、`.local/mv7-main-integration-clippy.log` 是忽略的运行输出，可能被下一次检查覆盖，不提交也不当成唯一证据。稳定入口为 MV-7 报告和对应提交的代码/测试。完整串行根目录检查约需 6～20 分钟，开始前为测试、修复、提交和交接预留额度。

旧完整运行曾遇到现有 Core 并发测试的有界 writer/index 等待或有序释放超时。隔离复验和完整重跑通过；未因此改变产品代码或等待规则。单 harness worker 不关闭测试内明确的线程/进程竞争。遇到类似失败先保留输出并聚焦复验，不以局部通过代替新功能的完整验证，也不要未经诊断扩大修复范围。

若触及 Runtime 集成面，在同一功能提交中重新生成 `docs/integration/runtime-surface.json` 并更新 `docs/integration/RUNTIME.md`。文档交接本身不改变 watched surface，可沿用刚验证的实现证据，检查链接/差异及 manifest 即可，无需重复长测试。

## 收尾与归属

提交只包含本任务文件，不带真实记忆、聊天、数据库、附件、凭据或 `.local/` 输出。Codex 实质参与的提交须按 `AGENTS.md` 保留 Codex co-author trailer；Claude 的新增贡献按其当前会话给出的 trailer 记录，不猜造身份，也不移除其他贡献者。

2026-10-10 本轮按主人接近限额时停止并关机的要求进入收尾：所有 5 项修复独立本地提交，当前代码完成完整验证，未发送真实 HTTP，未操作实际凭据，也未 push。停止时记录实际 HEAD、未提交范围、最后完整验证及未验证项。若新功能尚未完成完整检查，不称其已完成，不与已验证功能混淆；保留可恢复的修改和准确接续指令。没有新的明确授权，不 push、不创建 PR、不部署、不操作真实账户。

2026-10-11 本轮十项 Memory 侧修复均独立验证并本地提交，随后整合最新主线 a62ca4d；最新完整 362 项及规定检查通过。收尾时共享五小时额度已使用约 89%，停止新增功能，隔壁 Runtime 会话仍 idle / interrupted。没有 push、PR、真实 Provider 调用或 Runtime 写入；未开始 MV-8。关机授权的处理见本页“主人的授权与工作方式”：昨日那次已经执行，本轮不重复安排自动关机。

可供主人启动 Claude 时复制：

> 在 Enouia Memory 仓库接续 MV-7。先读 `AGENTS.md` 和 `docs/handoff/CLAUDE_MV7_HANDOFF.md`，核对当前分支、HEAD 和工作树，保留其他任务的改动。继续已授权的 Memory 侧功能，只处理有证据的缺口；每完成一个功能就验证并单独提交。真实 smoke、实际凭据操作、Runtime 产品接入、发布和 MV-8 仍未由本文授权。报告合成与真实证据的区别，并使用你当前会话的提交归属规则。
