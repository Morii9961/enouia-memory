# Enouia Memory · MV-0 独立审核与仓库纠正

审核日期：2026-09-28。审核者：Enouia / Codex。对象：Claude 在 Runtime 仓库（本地 checkout） 中尚未提交的 MV-0 变更，基线 HEAD `cd14bcf`。

**结论：保留现有工作，但 MV-0 暂不通过冻结验收。先完成独立仓库迁移与本报告中的契约修正，再复验；不要直接开始 MV-1。**

## 1. 目录问题的责任与新的权威边界

此前我给 Claude 的启动提示词明确把 Runtime 仓库（本地 checkout） 指定为实现仓库，还要求不要在 Memory 仓库（本地 checkout） 建工程。Claude 按这份错误交接执行。目录安排错误首先来自我的设计交接，不能归咎于 Claude 擅自选错目录。

Morii 本轮已明确：**Enouia Memory 应在自己的文件夹和独立 GitHub 仓库中开发**。据此，今后的边界为：

| 项目 | 本地位置 | 责任 |
|---|---|---|
| Enouia Memory | Memory 仓库（本地 checkout） | Memory 的设计、契约、领域实现、测试、版本与未来服务 |
| Enouia Runtime | Runtime 仓库（本地 checkout） | Windows Runtime/客户端、Activity 及面向 Memory 的集成适配 |
| GitHub Memory 仓库 | `https://github.com/Morii9961/enouia-memory` | Memory 工程的独立版本历史 |
| 真实 Vault | 源码库以外的独立私有数据根 | 真实历史、附件、记忆、密钥与运行状态，不入 Git |

用户已选择公开仓库。我已创建并通过 GitHub API 核实 `Morii9961/enouia-memory` 为 **PUBLIC、isEmpty=true**。本轮尚未将代码迁入、提交或推送，也没有清理 Runtime 工作树。创建仓库不代表代码已验收。

建议新 Memory 默认运行根为 `%LOCALAPPDATA%\EnouiaMemory`，支持显式配置。当前无真实 Memory 数据迁移证据，因此这里只给新的布局建议，不创建或移动运行数据。Runtime 通过配置和接口使用 Memory，不拥有其数据库或文件目录。

## 2. 本轮实测与未实测

- 已检查当前 Git 状态、六个共享文件的 diff、新 crate 的核心契约/验证器、Schema 测试器、合成样本、ADR 和阶段报告。
- Runtime 的实际 HEAD 为 `cd14bcf`，Memory 工作仍未提交；六个共享修改与报告一致。未发现本轮 Memory diff 修改 Activity 实现文件。
- 使用固定 Rust GNU 工具链，在系统临时目录指定独立 target，执行 `cargo test --locked --offline -p enouia-memory-contract`：**35 项通过，0 失败**。
- 独立构造了六个额外探针，调用当前 crate 的公开解析/验证函数，结果见各问题与末尾证据。
- `git diff --check` 通过；所检查的 `docs/memory` 本地文件链接无缺失。原始 v0.1 稿 SHA-256 仍为 `BFC7D55D219A1EB667C666739F943CAB034DB1A2508ED4F56E9EA81F5ABD1EA4`。
- 本轮没有重跑整个 Runtime 的 166 项测试、fmt、clippy；Claude 报告里的全量结果仍是其提供的证据，不冒充我本轮复验。
- 没有真实 Vault、真实模型调用或部署。本报告发现的是待实施系统的契约/纯逻辑缺陷，不能描述成已有生产数据泄漏。

## 3. 必须修改的发现

### F1 · P1：外发确认没有绑定到这次请求和内容

位置：`set.rs:1314`（Runtime 草稿行号，代码已迁入本仓库）、`context.rs:476`（Runtime 草稿行号，代码已迁入本仓库）、`candidate.rs:233`（Runtime 草稿行号，代码已迁入本仓库）。

当 private memory 发给外部 Provider 时，校验器只判断 `confirmation_review_id` 是否能找到任意 ReviewRecord。它不检查该 review 是否批准外发，也不绑定 request_id、实际内容哈希、Provider/model、范围或有效期。现有 ReviewAction 也没有独立的外发确认语义。

**复现**：基于合法的 MoriMeta confirmed set，将 destination 改为合成 external provider，补 policy ID，并把当初保存这条记忆的普通 `accept` review 用作 confirmation；记录解析及 `validate_set` 均通过，返回 `violations=[]`。

**影响**：用户同意长期保存某条信息，会被错误当成同意把它发送给外部模型；也无法阻止批准被换内容、换 Provider 或跨请求复用。

**最小修改**：设计单独的 EgressApproval/Grant 契约，绑定 owner、request、完整 payload hash、资源集合/revisions、完整目的地、policy epoch、有效期和一次性 nonce。不要把记忆审核与外发确认混用。持久化/发送的实际实现可后置，批准对象的语义与纯校验必须先补齐。

**回归验收**：普通 accept、错请求、错 Provider、同长度不同正文、过期批准、重放批准均拒绝；Identity、checkpoint、最近会话、附件/工具输入等载荷也须纳入覆盖，不能只检查 `memory_refs`。

### F2 · P1：删除和敏感度降级也只检查“有一份审核记录”

位置：`set.rs:324`（Runtime 草稿行号，代码已迁入本仓库）、`set.rs:923`（Runtime 草稿行号，代码已迁入本仓库）。

`declassification_review_id` 只要引用存在，便允许派生记录低于来源敏感度；tombstone 的 review_id 也只检查存在，不要求 `confirm_delete` 或与删除目标/模式一致。

**复现 A**：将生命周期 set 中 tombstone 的 review_id 换成普通 accept review，`validate_set` 仍返回空。

**复现 B**：将 private 来源支持的记忆改为 normal，并把其原 accept review 当降级批准；同步 capsule 的等级后，`validate_set` 仍返回空。

**最小修改**：分别绑定 action、操作者、目标 IDs/revisions、前后敏感级别、删除范围/mode、最终 diff hash 与提交关系。敏感度降级采用明确的 declassification action 或独立批准类型；tombstone 必须追溯到同一次删除的确认结果，不能接受任意 review。补上正确批准与错目标、错动作、错 revision 的反例。

Claude 增加 owner-only delete / confirm_delete 入口的方向合理；问题在于跨记录验证尚未真正建立这些绑定。

### F3 · P1：发送一致性函数会接受完全不同的同长度文本

位置：`provider.rs:115`（Runtime 草稿行号，代码已迁入本仓库）。

`ProviderRequest::matches_dispatch` 只比较 dispatch/capsule ID、消息条数、角色、字节长度和工具数量。它没有核对消息 content_hash、完整 request_hash、工具名称或定义。

**复现**：读合法 Dispatch fixture，把实际消息替换为相同字节长度的全 `X` 字符串，`matches_dispatch` 返回 true。

**影响**：Inspector/批准对应的内容与真实发出的内容可以不同，却被标为一致。这是纯函数逻辑问题，不必等文件存储实现后才能发现。

**最小修改**：定义并实现确切的 payload 编码与哈希契约，核对所有 messages/tools 及被批准的请求配置；普通形状比较若保留必须改成不会误导调用者的名称，不能用作最终外发门禁。明确 Dispatch 中哈希如何定位到私有实际请求对象，并把对象保存、完整性验证和删除传播安排到后续对应阶段。

**回归验收**：同长替换、工具同数量换定义、message 顺序变化和输出配置变更，都不能沿用先前实际请求或批准的哈希。

### F4 · P1：PolicyGate 的冻结输入不足以表达项目与目的地授权

位置：`ports.rs:179`（Runtime 草稿行号，代码已迁入本仓库）、`record.rs:16`（Runtime 草稿行号，代码已迁入本仓库）。

AccessRequest 只有 principal、operation、scope、purpose、DestinationKind、sensitivity、request_id；没有目标 memory/source/project/revision、使用的 policy ID、实际 Provider binding，也没有经过批准的 payload 绑定。两个同敏感级别、不同项目的读取，以及两家 external provider，在这份端口输入中无法可靠地区分。

设计还要求 versioned policy files，但当前 18 种 RecordKind 和 schemas 中没有持久 PolicyRecord。只有 PolicyId 和 epochs 不足以恢复授权本身。

**最小修改**：先补完整 policy/grant/revocation 记录、资源选择器和目标绑定，以及 PolicyGate 的上下文输入。可以通过明确不可伪造的服务端解析结果传入；不要依赖 request_id 去某个未定义的隐藏全局表补资料。默认 deny，允许的结果应关联确切政策版本与范围。

**回归验收**：同 principal/同 sensitivity 的跨项目请求得到不同结果；相同资源切换 Provider 要重新授权；权限和撤回可从文件契约重建，而非仅留在数据库/内存中。

### F5 · P2：未知替代时间被批准时间变成确定时间

位置：`temporal.rs:27`（Runtime 草稿行号，代码已迁入本仓库）。

原设计明确：不知道新事实何时生效时不能擅自截断旧事实有效区间。当前实现却在 effective_from 和 valid_from 都未知时使用 approved_at。

**复现**：将新记录的两个生效时间设为未知；它于 2026-09-21 获批，查询 2026-09-28 时，旧事实返回 Superseded，新事实返回 CurrentSupported。批准只证明“用户在这时确认了记录”，不证明事实最迟从此刻生效；未来计划也可能提前被批准。

**最小修改**：保留 unknown，不把系统记录时间转换成业务生效时间。相关新旧事实标需要复核或语义冲突；只有用户明确给出生效日期或“立即生效”，才确定起点。即使考虑用 valid_from 作为替代起点，也应校验该时间确实适用于该 supersedes 关系的命题范围。

### F6 · P2：极大预算值会让 Rust 校验器 panic

位置：`context.rs:268`（Runtime 草稿行号，代码已迁入本仓库）。

ContextCapsule 的纯校验直接相加 `estimated_tokens + safety_margin_tokens`。这些字段是 u64；Rust 解析入口不自动运行 JSON Schema 中的数值上限约束。

**复现**：将合法 capsule 的 max_tokens/estimated_tokens 都设为 u64::MAX，safety_margin_tokens 设为 1；在本轮 debug 构建中，`parse_value<ContextCapsule>` 触发 panic，而不是返回结构化校验错误。

**最小修改**：对所有 wire/storage 数值实行与 Schema 一致的范围校验，使用 checked arithmetic。补极大值、边界值和跨字段相加溢出测试。发布配置下的溢出行为仍需单独测试，本轮没有假称已测试 release。

## 4. 其他验收与报告修正

1. **独立 Schema 校验仍需补齐。** 当前自写子集 validator/regex engine 与被测 Schema、fixtures 同由本次实现提供，可能共同漏掉同一规则。离线实现它可以理解，但不能替代独立 Draft 2020-12 validator 的交叉验证。修正轮应固定一款成熟验证器，仅用于测试，验证 `$ref`、条件分支、枚举、数值边界、未知字段及正反例。本轮发现本地两套 Python 均未预装 jsonschema，常用 Node 环境也未找到 Ajv 2020，故没有冒充已完成独立交叉核对。
2. **保留“静态通过”的限定。** D03 的仓库无关扫描是生产源码静态约束；测试工具仍直接加载整个 Runtime 的 contracts 目录，包括 Activity。它不能证明迁到另一个仓库就能运行。
3. **原“回退六个共享文件”方案过于笼统。** 工作树已有其他贡献者推进。清理时只能移除本次 Memory 的确切 hunks、member 和 lock 条目，不能把六个文件整体恢复到旧 HEAD，更不能 reset --hard / clean 处理工作树。
4. **目录错误不等于现有代码都应丢弃。** Schema、严格解析、合成 fixtures、规则编号和边界意识均可保留，修好后迁入独立工程。35 项测试通过是有效证据，但不抵消本轮新增反例。
5. **本轮无须重开架构大讨论。** Local Primary、可读文件、可重建索引、人工审核、独立 Activity 和未来受限 VPS 路线继续保留；优先修复仓库归属与实际契约缺口。

## 5. 推荐的独立工程形态

```text
enouia-memory/
  Cargo.toml / Cargo.lock / rust-toolchain.toml
  AGENTS.md / README.md / .gitignore
  crates/
    enouia-memory-contract/
    enouia-memory-foundation/   仅确有共享必要时创建
  contracts/{memory,context,provider,ipc}/
  tests/fixtures/memory/
  docs/{design,adr,validation,reviews}/
```

这是迁移后的目标，不是本轮已创建的工程。

当前 `enouia-memory-contract` 通过 `../enouia-common` 依赖 Runtime 内部 crate。不要改成 Runtime 仓库（本地 checkout） 或 `../../Enouia Runtime` 的跨仓库 path 依赖，否则仍要求相邻源码库存在。

推荐将 Memory 真正需要的 Clock/Cancellation/Lock/AtomicFile 等最小端口及其自有健康/错误 DTO 放到 Memory 自己的契约/基础模块；Runtime 在集成适配层实现或映射它们。不要直接复制整个带 Activity 含义的 common 包。未来 Runtime 消费 Memory 的明确版本发布物或固定 Git revision，版本化 IPC/schema 作为跨进程边界。独立仓库不要求现在就提前实现 MV-8 的常驻 Host。

测试只加载实际依赖的 Memory/context/provider/IPC schemas；将必须共用的 IPC 定义本地明确版本化，解除对 Activity 目录的存在性依赖。Memory 从独立 checkout 能构建和测试，不靠旁边 Runtime 的 fixtures、文档或 target。

## 6. 下一轮顺序：MV-0R，尚不进入 MV-1

### R0：先保护待迁移成果

记录 Runtime 当前 HEAD、完整 status、六个共享文件 diff、全部 Memory 新文件清单及 SHA-256。先保存可恢复的补丁与未跟踪文件快照，放入被 Git 忽略的本地备份位置；只复制，不先删除。再次检查其他贡献者有无新改动。

### R1：建立独立 Memory workspace

在 Memory 仓库（本地 checkout） 初始化 Git 并连接已创建 remote，建立忽略规则后再暂存。真实 vault/raw archive/session/attachments、运行数据库、密钥、恢复材料、本地迁移快照和 target 不得纳入提交。现有设计稿先保留，以迁移映射决定最终路径，不覆盖原始 v0.1。

将新契约、schemas、fixtures、规范、验证说明有序复制进来，核对迁移前后哈希；路径修改后另列差异。解除 Runtime 内部 common 与 Activity schema 依赖，补独立 AGENTS.md 和构建说明。不要夹带任何 Activity 实现。

### R2：先修契约，再宣称冻结

修 F1～F6；新增本轮六个反例及项目/Provider 权限区分测试。补独立 Schema validator。同步 ADR、字段说明与约束覆盖，不能只修测试让错误规则继续通过。原 35 项与新增反例共同验证。

### R3：复验后清理 Runtime 中误放的 Memory 改动

仅在目标文件已核对、独立构建/测试通过且来源快照可恢复后，按 R0 清单移除 Runtime 内本次 Memory 新增文件和精确共享 hunks。清理前再读最新 diff，避免覆盖并发工作。任何与 R0 不一致的内容先保留并报告。

Runtime 可保留一份简短的“Memory 独立仓库/未来适配”说明，但不保留第二套权威 Memory Schema/代码副本。对 Runtime 的清理后检查使用其自身测试约定，保证 Activity 工作与其他贡献者改动不丢。

### R4：修正文档入口和交接

修正 README、总架构、数据根说明、实施计划、决策文档与旧提示词中“Runtime 是实现仓库”的假设。旧设计/验收报告保留为历史并标版本；新报告明确之前的 MV-0 需复验，不能覆盖旧测试输出来伪造曾经通过。

### R5：提交与发布

新仓库仅提交经过审查的设计、代码、fixtures 和测试。先做公开内容检查，遵守该实施任务的既有提交/推送授权及贡献署名要求；不要把修正前不合格成果标成已冻结版。是否开始 MV-1 是下一次明确的阶段决定，不由本轮修正自动触发。

## 7. 独立反例的实测输出

本轮临时探针源文件：`<本地临时目录>/enouia-memory-review-repro.rs`。它读取合成 fixtures，链接本轮临时 target 中构建的当前 Memory contract rlib；未修改 Runtime 源码/fixtures。

```text
unknown_supersession:
  approved_at=2026-09-21T10:01:00.000Z
  query=2026-09-28T08:00:00.000Z
  old_effect=Superseded
  new_currency=Some(CurrentSupported)
same_length_unrelated_payload_matches_dispatch=true
ordinary_accept_reused_as_external_consent: violations=[]
ordinary_accept_reused_as_delete_consent: violations=[]
ordinary_accept_reused_as_declassification: violations=[]
extreme_budget_panics=true
```

这是错误输入被当前校验接受/崩溃的证据，不是六条修复后的测试通过。实现者应把上述情形转成预期拒绝或保留不确定性的正式回归测试。
