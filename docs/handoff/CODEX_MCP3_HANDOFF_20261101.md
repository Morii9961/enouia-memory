# MCP-3 交接：捕获已能入队和形成，下一步接传输并托管 worker

日期：2026-11-01，Asia/Shanghai。本文件只交接，不启动实现。启动范围以 Morii 发送的
[提示词](CODEX_MCP3_START_PROMPT_20261101.md)为准。本机路径在被忽略的
`.local/handoff/MCP3_LOCAL_20261101.md`，不写入公共文档。

## 1. 基线

| 项目 | 核实结果 |
| --- | --- |
| Memory `main` | `c9bf1be`，含 Core-1～7 与受限 MCP；比接续分支少 31 个提交 |
| 接续分支 | `claude/mcp-captured-formation-queue`，头部含本交接提交，实现端点 `795c08a` |
| 分支链 | `87fd505`（PR #77 顶端）→ `68ca1cc`（Owner 捕获）→ `8a1c13d`（入队）→ `795c08a`（worker） |
| PR | #56～#77 共 22 个草稿开启；`68ca1cc` 之后三个提交尚无 PR |
| 不在链上 | `codex/owner-vault-bootstrap`（#76），Owner 空库创建，基于 `3b6e974` |
| Runtime | 主线 `d0208eb`，pin `bf6bfd1`，58 条命令；未采用上述任何草稿 |
| 集成面 | aggregate `c3b9a6d1745cf7a733bc2dc28d4294249728918e984dea32271c66a1064f24e7` |

开工前重新 fetch 并比对，本表只是 2026-11-01 的快照。

## 2. 已确认的主人决定

- **只做 Runtime 采集。** 2026-10-26 Morii 先答“两者都实现并去重”，随后改为“只做 Runtime
  就行”，以后者为准，见 [ADR-MEM-102](../adr/102-runtime-only-capture-boundary.md)。不开发外部 Agent 宿主
  hook，也不开放 `conversation_capture`。需求 03 里的 Agent 采集已被取代。
- `memory_note` 仍是可选的 Agent 提示，不是采集入口。
- 缺失时区默认 `Asia/Shanghai`，并记录默认来源。
- 日常记忆全自动，不做逐条人工审核。不确定内容由系统暂存或重评，不堆成待处理队列。

## 3. 现状

### 核心

主线有准入、持久作业、自动闭环、持续策略、形成、召回和授权读模型，都只有合成证据。链上另有
UI v2 记录字段（#56、#59、#61、#63、#68）、初始化（#67）。

| 能力 | 状态 |
| --- | --- |
| 捕获写入 Owner 根 | 已完成，原生接口，`RuntimeCaptureHost::enroll_owner` |
| 入队 | 已完成，ADR-111，`enqueue_captured`、`queued_captures`、`RuntimeCaptureHost::enqueue` |
| 形成 worker | 已完成，ADR-112，`CapturedFormation::step`，单步驱动 |
| 初始化共用 | 已完成，种子、引文、prepare 共用，`step_with_captures` 让 live 优先 |
| 真实分析器 | **没有**。`CaptureAnalyzer` 只有脚本实现 |
| worker 托管 | **没有**。无线程、前台让路、取消或 Host 接线，没有东西调用 `step` |
| Owner 根的持续模型策略 | **没有**。`ContinuousPolicyHost` 只绑定 `AutomationHost`，未绑定分段 Vault |
| 实体与修订 | **没有**。日常分析不能发布实体，也不能修订旧记忆 |
| 对话上下文 | **没有**。分析每次只看一条来源 |

### MCP

| 分片 | 状态 |
| --- | --- |
| MCP-Resume-1 原生准入与本地传输 | 草稿完成（#57） |
| MCP-2 新格式只读 | 草稿完成，初始化库五工具走 pipe-5（#58、#60、#62、#64、#70～#75、#77） |
| MCP-3 Runtime 捕获 | 旧布局 pipe-1 草稿完成（#65、#66）；Owner 根只有原生接口，**缺 wire** |
| MCP-3 引文定位 | 草稿完成（#69），`memory_note` 待用 |
| MCP-3 `memory_note` | 未开始 |
| MCP-4 纠正与请求 | 未开始 |
| MCP-5 开场与资源 | 未开始 |

## 4. 建议顺序

**片 A：Owner Runtime wire。** 新的版本化协议，例如 `memory-runtime-capture-pipe-2`，只接受
`automation-segmented-owner-v1` 和 `memory-runtime-owner-capture-native-1`。

- pipe-1 保持旧布局，现有 `serve_connection` 对 Owner 能力的拒绝不变。
- 复用现有受保护管道、对端核验、帧限制和单次绑定。
- 捕获和入队是两个事务。崩溃后用同键重放即可恢复，wire 的回执要分别表达，不能把入队说成形成。
- 增加该 producer 自身作业的状态读取，给 Runtime 的 Formation 用，不返回他人作业或正文。
- 结果未知保留原包原键，不自动重发或重连。
- 集成面变化在同一提交重新生成并登记 `RUNTIME.md`。不改 Runtime 仓库。

**片 B：worker 托管。** 让形成在同一个已租约 Vault 上自动运行。

- 独立后台线程，前台优先，有退避。`step` 返回 false 或 Busy 时不空转。
- 锁定、撤回、退出时停止，不跨授权继续。`OutcomeUnknown` 后停机，等恢复后重开。
- 把 `CaptureAnalyzer` 改成有界、可取消，真实模型不能阻塞在同步调用里。可参考 Core-2 的
  `Generator`、`PendingGeneration`、`StopToken`，但它们在 Host crate，Vault 不能反向依赖。
- worker 崩溃不能带倒宿主，不占满 Runtime 通用 worker。
- 只用假分析器验证。

这两片做完，Runtime 就有了可采用的完整本地通路。下面这些不属于本次范围：

| 项目 | 为什么不在本次 |
| --- | --- |
| 真实分析器与 Core-4 绑定 | 需要 Morii 决定模型、预算、外发范围 |
| `memory_note` | Agent 写入面，等片 A、B 后由 Morii 单独领取 |
| 草稿链合并、Runtime 采用 | 需要 Morii 决定顺序，且另开切片 |
| 个人库、真实客户端、云端 | 需要 Morii 明确授权 |

## 5. 已知缺口

交接时不要把这些当成已解决：

- `analysis_unknown` 是终态，不重发，会放行同会话后续轮次。该来源暂不形成记忆，补分析需要单独设计。
- 分析后被删除来源的任务永远停在 `forming`，没有副作用，但状态不会收尾。
- 含入队记录的 Owner 库，旧版本应在解码时拒绝。这是按闭合枚举推断的，未用旧构建实测。
- 并发过期路径只在步与步之间确定性验证过，没有多线程竞态测试。片 B 引入线程后需要补。
- 日志接近 16 MiB 上限的情况未验证，长期运行时需要关注终态任务累积。
- `formation_worker.rs` 修复后未重跑工作区全量，只重跑了 `enouia-memory-init` 全部测试、fmt、
  Clippy 和 surface。修复前全量为 840 通过、0 失败、4 忽略。片 A 的全量会补上这项证据。

## 6. 容易踩的坑

- ADR 87、88、89 有 Core 与 MCP 两组同号文件，必须用完整文件名。新 ADR 从 113 起，110 已被
  #76 占用，106～109 目前空缺但不要使用，避免和未发布分支冲突。
- 评估器的拒绝和快照过期都以 `AutomationFault::Contract` 返回，要用 `rules()` 里的稳定 ID 区分，
  见 `formation_worker.rs` 的 `retryable`。
- `read` 返回的记忆按 memory ID 排序，不是形成顺序。断言顺序要用回执序号。
- 部分权限受真实时间限制，上限 300 秒。笔记本用电池时测试慢 3 倍左右，曾因此过期失败。
- 工作区测试默认遇错即停，排查时加 `--no-fail-fast`。

## 7. 验证

依据：[MV 报告索引](../README.md)、[入队报告](../validation/MCP-3-captured-formation-queue.md)、
[worker 报告](../validation/MCP-3-captured-formation-worker.md)、
[队列合同](../contracts/CAPTURED_FORMATION_QUEUE_V1.md)、[ADR-112](../adr/112-captured-formation-worker.md)。

推荐环境：`CARGO_NET_OFFLINE=true`、`CARGO_INCREMENTAL=0`、`--test-threads=1`、`--no-fail-fast`。
Schema 或 validator 改动才跑 `tools/schema-check/check_schemas.py`，集成面改动才跑
`tools/integration/runtime_surface.py`。

以上数字都是合成证据，不代表真实模型质量、个人数据或主人验收。
