# MCP 工作接续：核心重构后的首个本地接入切片

日期：2026-10-25，Asia/Shanghai。本包只准备下一 Session 的启动材料，未启动 MCP 功能开发、客户端配置、真实资料访问或部署。实际执行以 Morii 在新 Session 发送的任务和当时的 AGENTS 为准。

## 1. 接续方向与基线

Morii 已澄清下一段回到 MV-8／MCP。此前“暂停 MCP”是核心重构期间的执行顺序，不能继续解释成永久暂停；也不能据此认为合成验证已经完成真实客户端和私人记忆库的采用。

| 项目 | 本次核实结果 |
| --- | --- |
| Memory `main` | `c9bf1be7819dba19915adf6031b1900c032b73d1`，Core 整合 [PR #55](https://github.com/Morii9961/enouia-memory/pull/55) 已合并 |
| 被验证的整合源提交 | `bf6bfd10bf3c870ae2ce7ceb5c6bd635b08bf327`，与上述 merge commit 树内容相同 |
| Runtime surface | `01cfc998a6707897b1956ef31248dbd44102578440ff558095ab5566917f5b82`；Workspace IPC v1 保留 58 条命令 |
| 既有 MCP／Host | 经 [PR #53](https://github.com/Morii9961/enouia-memory/pull/53) 合入；旧实现端点 `9286cd8` 仅作历史证据，不作新开发基线 |
| Runtime 采用 | [PR #99](https://github.com/Morii9961/enouia-runtime/pull/99) 已合并；核查时远端 main 为 `fe312594711ff8b849bf8db8be44d4c0a689163d`，Memory pin 仍为 `bf6bfd10bf3c870ae2ce7ceb5c6bd635b08bf327`／58 命令 |

这些是日期对应的快照。开始前重新 fetch、核实真实 checkout、最新 main、未提交资料和并行任务；不要用本表覆盖更新实现。Runtime 是只读契约消费者参考，不是本片可修改仓库。

[Core 整合报告](../validation/CORE-main-integration.md)记录 661 项 Rust、fmt、clippy、独立 Schema 和集成面检查。Runtime 的 `docs/validation/Core8-main-integration-2026-10-25.md` 记录 79+8 Rust、161 Node、76 个 DOM 场景／1030 项断言和 18/18 原生合成验证。这些是既有检查，不是本次准备材料重新执行的结果；也不证明真实模型质量、私人历史覆盖、生产 System 身份或数据切换。

## 2. 实际断点

现有 MCP 在 `enouia-memory-host` 内实现，没有独立 `enouia-memory-mcp` crate。应优先复用现有 Host、传输、Broker、权限和回执路径。

| 能力 | 已有证据与缺口 |
| --- | --- |
| 协议 | 固定支持 MCP `2025-11-25` 的受限 stdio／JSON-RPC；这不是“最新版本”声明 |
| 工具 | 仅 `memory_search`、`memory_read`、`memory_source`、`context_get`、`memory_propose`、`session_checkpoint`；`memory_update` 只是未广告的旧别名 |
| Host 内领域绑定 | 六工具已有进程内合成绑定；受当前记录策略、独立 scopes、来源／删除约束和原生生命周期控制 |
| 写入语义 | 旧提议和 provisional checkpoint 保留原语义、主体隔离回执与未知结果处理，不借用 Owner Workspace，也不变成机器批准 |
| 原生准入 | `Grant::from_authenticated_agent` 是“已完成外部认证后的可信嵌入边界”，本身不做 OS 认证或客户端登记 |
| 可运行接入 | `examples/mcp_synthetic_stdio.rs` 仅协议示例，不打开 Vault，工具返回 backend unavailable；不是可用的外部 Agent→Host 服务器 |
| 未交付 | 外部 Agent 原生认证／登记、真正可运行的本地接入传输、真实 MCP 应用安装／配置与个人验收 |
| 仍受限 | 更广 Context 的历史／会话／分支／外发选择未开放；Runtime 仍为 embedded 采用，不能称作已迁到 native Host 客户端 |

直接证据：[MCP 边界 ADR](../adr/080-restricted-mcp-stdio-boundary.md)、[六工具出口表](../validation/MV-8-host-agent-checkpoints.md)、[MCP 主线整合报告](../validation/MCP-mainline-integration-2026-10-24.md)。

## 3. 下一 Session 领取的首片

工作包暂称 **MCP-Resume-1：外部 Agent 原生准入与可运行本地传输**。这是工作包名称，不是预定 wire schema 或既有 MV 编号。

先用最小 ADR／契约冻结原生登记者、Agent principal、scope ceiling、当前记录策略、OS 对端绑定、连接版本／代次、期限、撤回和锁定／退出语义，再在同一片实现，不能停在宏观计划。具体类型和传输形状按现有项目模式确定。

交付边界：

1. 原生可信登记／launcher 路径核验本地对端身份，建立受限 Agent 会话；允许使用必要的 OS 进程／账户／登录会话元数据，不读取 API 凭据。身份和授权不能取自 `clientInfo`、工具参数、自报 PID 或聊天中的“同意”。用户可控制客户端授权，不要求逐条批准每条机器记忆。
2. 用独立合成客户端进程走真正本地 stdio／原生传输，接到同一个已绑定 Host。MCP 桥不能另开 Vault writer 或把请求转给 Owner Workspace。优先复用现有 Windows 受保护管道、对端核验、deadline 和回收机制。
3. 保持现有六工具和旧工具含义，验证当前策略、独立 `source:read`、敏感度和项目／来源约束。不得开放 Owner review、任意 Workspace 转发、原生生命周期控制或 System 身份。
4. 锁定、撤回、退出、重启和连接代次变化使旧准入失效；同一请求与原键绑定不变。已知拒绝与结果未知分开，未知结果保留原包／原键，不自动重发、重连或生成替代键；真实成功回执只能来自合法提交。
5. 明确支持与拒绝的格式／能力矩阵，给出可重复的合成启动方法和结果报告。真实 Codex／Claude 等应用配置、个人数据、真实 Provider 和远程开放不在首片中。

## 4. 自动记忆新格式不能被旧 MCP 偷渡

必须分别检查源码 pin、Record schema、存储 layout、Host publication、Workspace IPC、Memory IPC／MCP 与新原生能力，不能用一个“V2”代替全部。

- 旧记录 schema 1、闭合 `RecordKind`／`AnyRecord`、原 Owner 批准字节与历史保持。历史 Host `host-current-v2` 仍可承载 schema-1 记录，不等于自动记忆 schema 2。
- Core 自动化根 `automation-host-v2`、规则 3 和 `memory-native-read-1` 是显式独立能力。现有原生读模型经过 System gate，不能直接当作对外 Agent 读取端口。
- 旧 Memory IPC 六工具不会因为 pin 升级就自动成为机器格式 reader。格式／能力不支持时，必须在业务解码、查询、计数和正文返回之前拒绝；unknown-major 原字节诊断不是业务支持。
- 首片如仅接通旧格式的真实本地传输，应明确记录“自动化 V2 根的 Agent reader 未交付”，列作后续独立适配，不能宣称新版 MCP 全部采用完成。若本片确需补新版读取，先冻结版本化、策略过滤的 Agent 契约并限于必要范围；不能转发 System native read 来省略权限。
- Agent 话语不是 Morii 自述；提议／checkpoint 不等于 Owner 确认或客观独立证实。旧候选不能批量晋升，旧工具也不能暗中改变为自动 Canonical 写入。新机器准入仍走合法 System 管线，含糊／冲突走自动暂存与重评，不新建必须由 Morii 清理的队列。

参阅 [Core-1 格式／消费者矩阵](../contracts/CORE1_AUTOMATIC_ADMISSION.md)、[当前自动提交契约](../contracts/CORE3_AUTOMATIC_LOOP.md)、[Core-7 读模型](../contracts/CORE7_READ_MODEL.md)。

## 5. 首先读取的正式材料与源码

按实际路径核实后读取：

- `AGENTS.md`、`docs/design/IMPLEMENTATION_PLAN.md`、`docs/adr/README.md`、`docs/design/INTERFACES.md`；旧阶段“暂停／禁止合并”是各自日期的边界，当前用户任务优先。
- 本文件、[ADR 映射](MCP_MAINLINE_ADR_MAP_2026-10-24.md)、[Core 整合报告](../validation/CORE-main-integration.md)。
- `080-restricted-mcp-stdio-boundary.md`、`087-host-scoped-agent-reads.md`、`088-host-agent-proposals.md`、`089-host-agent-local-context.md`、`084-host-agent-checkpoints.md`，均位于 `docs/adr/`。
- `docs/adr/075-native-host-session-gate.md`、`077-windows-host-transport.md`、`078-native-client-reconnect-boundary.md`，以及相关验证报告。
- 上节三个 Core 契约、`docs/adr/091-authorized-memory-read-model.md`、`docs/integration/RUNTIME.md`。
- `crates/enouia-memory-host/src/mcp/`、`service.rs`、`windows.rs`、`transport.rs`、`framing.rs`、`compatibility.rs`、`broker.rs`、`main.rs`；`examples/mcp_synthetic_stdio.rs`；现有 IPC／Host schemas 和相关 tests。路径均相对 Host crate 的相应目录，先 `rg --files` 核实。

当前 ADR 87／88／89 存在 Core 与 MCP 两组同编号文件：例如 `087-automatic-memory-transactions.md` 与 `087-host-scoped-agent-reads.md`。必须按完整文件名消歧，不能只按编号覆盖任一组；旧映射文档是 10 月 24 日快照，不能当作后来没有重号的证明。新 ADR 选实际空闲编号；确需修复本片交叉引用时做最小修正，不以全局重编号取代功能实现。

## 6. 工作区与数据保护

新开发从最新 main 建 `codex/` 分支，优先使用当前可用的合适工作树，确有冲突再隔离。原始 checkout 仍有主人 AGENTS／设计／状态改动和进程引用；具体本机位置在 ignored `.local/handoff/MCP_RESUMPTION_LOCAL_20261025.md`，没有时先从启动目录或登记工作树受控查找，不猜路径。

旧 Core／MCP 远端源分支已清理；`codex/history-planning-host` 是被正式 Workspace 历史接口取代的旧草案，也已归档删除，不要重建。旧 Runtime Core-8 整合工作树亦已不存在。不能从旧 handoff 提到的目录或分支推断它现在仍可写。

先列本片 owned-file allowlist。保留所有他人未提交材料、AGENTS 和 `docs/status/PROJECT_STATUS.md`／`project-status.json`，不 reset／clean／覆盖，不把整包 `.local`、参考图、私人附件或恢复材料纳入 Git。若新隔离树缺少尚未提交的公共交接文档，只从启动 checkout 复制明确需要的公共文件；不能因缺文件忽略授权边界。

本片不修改 Runtime／Claude 工作树、共享 App／main／router／token 或无关模块。Activity／Usage 从不进入 Memory，也不是后台认知输入。只使用明确新建的临时合成根和自有测试进程，不打开私人 Vault、聊天导出、API 凭据，不安装或修改真实 MCP 客户端配置，不调用真实 Provider／embedding，不部署、远程开放、迁主或生产双写。

## 7. 验证、交付与停止

覆盖：伪造身份／scope／root、错误 OS 对端或失效连接、跨会话／过期／撤回／锁定、旧格式／新格式版本拒绝、当前策略与来源删除、敏感数据及计数不泄露、原键冲突与响应丢失、真实本地进程断连／退出回收；不要只用 fake port 代替本片声明交付的真实本地传输。

遵守当时 AGENTS 和直接用户要求，验证强度与修改面匹配。适用时使用 pinned Rust、offline／locked 测试、fmt 和严格 clippy；Schema／validator 修改才运行 pinned Python 独立 cross-check，集成面修改必须同提交生成 `runtime-surface.json` 并登记 `RUNTIME.md`。跨 workspace 契约变化执行必要全套；纯文档或重复评估不跑无关 build／smoke。既有通过数字只作基线，最终报告列本片实际执行的检查。

完成后检查公开内容，仅提交并推送 owned files，附空行分隔的 `Co-authored-by: Codex <267193182+codex@users.noreply.github.com>`。报告 commit／branch、版本矩阵、实际证据、未交付项与下一片，然后停止。不自动 merge／部署，不自动扩大工具／Context／真实资源范围，不创建跟踪自动化或关机任务。

首个 Session 的出口是可运行、经过原生准入和当前策略约束的本地合成 MCP 接入片，不是所有 MCP 产品采用、自动化 V2 对外读取或私人日用目标同时完成。
