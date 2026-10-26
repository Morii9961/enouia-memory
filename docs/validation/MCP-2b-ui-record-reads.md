# MCP-2b：冻结 UI 字段的合成新根读取

日期：2026-10-26。分支 `codex/mcp-ui-record-reads`。
以前一片 `645deec2f8a0a7494c9f153f5e1d0b25191e460c` 为基线，引入字段依赖
`24a03e02bdf18094d60bc4d3268d380bd1b669a1`（PR #56），保留其提交父链及原文件。
main 经 fetch 核对仍为 `c9bf1be7819dba19915adf6031b1900c032b73d1`。
本片未修改并行会话或原始 checkout；AGENTS、状态快照、Runtime 和 Claude 文件不动。

## 结果

[ADR 097](../adr/097-agent-ui-record-reads.md)／[读取契约](../contracts/MCP_AGENT_RECORD_READS_V1.md)
实现独立的 `agent_record_reader`、`memory-agent-record-read-1` 与 HostV3 明确握手。
五工具 memory_search/read/source/related/recall 经过真实管道与独立进程，另有实际 stdio
可执行文件接入。同一个 Host／Vault，桥不打开第二个 writer。

返回修订绑定的标题、语言、知识／采用、精确时间及其未知／precision/bound/tz/basis、
实体及已授权别名、证据原句／speaker／scalar 范围、实际 producer/model/prompt/import/live
形成信息与可空附注。System 采用不等于 Owner 确认；记录原句不被提升为客观事实。
SourceRecordFields 独立受 SourceRead 控制。Source ID 与会话链接不是整段会话 Context。

实体名称／别名的全部来源也必须属于该条策略和 Agent scope，并通过当前来源修订、
删除与敏感度检查；否则整条记忆在计数前隐藏。重评／不确定仍留在 System 自动管线，
没有新建逐条人工审核队列。所有字段计入输出／召回预算，Context 返回同一实际计算结果。

## 版本与拒绝

| 入口 | schema-1 / host-current-v2 | schema-2 / automation-host-v2 | schema-2 / automation-host-v3 | 分段新根 |
| --- | --- | --- | --- | --- |
| 旧六工具 | 保留旧义 | 拒绝 | 拒绝 | 拒绝 |
| MCP-2a 五工具 | 拒绝 | 支持原 Core-7 字段 | 拒绝 | 拒绝 |
| 本片记录读取 | 拒绝 | 拒绝 | 完整冻结字段，只读 | 尚未适配，拒绝 |

HostV3 仍有原 128 条／64 次提交等边界；本片不宣称 20,000 条规模或分段布局支持。
相同 schema major、升级 pin 或 Agent 参数不能跨过 layout/capability/hash 及原生身份门禁。
Core-7 System 端口没有直接转交 Agent，普通 Agent 也不能创建 Owner 动作。

## 实际证据

五组新增合成回归覆盖：

- 新根真实合法事务形成回执后，page/detail/source/recall 的原字段、EntityRecord、来源权限、
  未知时间和准确 trace；读取前后所有业务文件逐字节一致（OS 独占锁文件除外）。
- 别名依赖另一来源时，窄 scope 整条隐藏且计数为零；更完整 scope 可读；删除别名来源后
  再次隐藏。SourceRead 缺失时来源及 source_record_fields 均为空，原文请求拒绝。
- 真实独立 Agent 的五工具及完整字段筛选；正常召回与预算排除均提供真实 trace，原生
  Context 逐值相等。错误的 Owner-confirmed 排除原因必须被 Schema 拒绝。
- 两种旧客户端在握手阶段拒绝 HostV3，业务文件不变。
- 实际 `enouia-memory-mcp --protocol automation-records-v1` 经 stdio／管道接入既有 Host，
  EOF 后自有进程退出，未开第二个 writer，未写业务数据。

新增 Schema 正反例 12 项，无伪造成功回执 fixture。独立校验器检验实际广告的
input/outputSchema、6 次真实字段工具交换和 1 个错误输出；旧 HostV2 106 次交换另验。
本地 Rust 子集补了已有 trace Schema 所需的 prefixItems 校验，按
[JSON Schema 2020-12](https://json-schema.org/draft/2020-12/json-schema-core#section-10.3.1.1)
逐位置检查前缀并让 items 检查剩余元素；独立 Python 校验对同一实际 trace 作交叉检查。
这仍是测试用子集，不是完整 Schema 实现或生产身份校验器。

## 检查

最终源码使用 pinned Rust 1.98.1，CARGO_NET_OFFLINE=true：

| 实际检查 | 结果 |
| --- | --- |
| cargo fmt --all -- --check | 通过 |
| cargo test --workspace --locked -- --test-threads=1 | 700 项通过，零失败；automatic_loop 合计 67 项 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过；CARGO_INCREMENTAL=0 避免本机缓存硬链接警告 |
| pinned Python 3.12 / python-jsonschema 4.26.0 | 53 schemas、50 UI 字段用例、12 新请求正反例通过；6 次新根交换＋1 错误输出与 106 次 HostV2 交换分别通过 |
| python tools/integration/runtime_surface.py | 通过，58 命令，aggregate 11575fe13c2694d0a82e2761b73d82da6c4b726ef732189352fab94dd5bde9c4 已登记 |
| git diff --check、公开内容及修改面审查 | 通过，61 文件在 owned／精确依赖范围内，无私人路径／凭据形态／冲突标记 |

传输／ACL 场景用正常 Windows 用户令牌运行，没有放宽产品 ACL。日志、两份实际合成
传输记录和 commit-message 在 ignored target，不提交。测试成功不等于真实模型质量、
私人根初始化、Runtime 渲染或生产采用。

## 初始化与后续

私人新库尚未初始化 **不是开发阻塞**。本片从空目录创建临时合成 HostV3 根、合成策略、
来源和记忆，已验证读取链路；没有读取聊天导出、私人 Vault、凭据或调用真实模型／embedding。
真实初始化应采用最终确认的存储／字段／读取版本，不能把这份合成证据当作私人库已切换。

尚未交付：分段布局 Agent 适配、全局主题／事实／建议边、实际 Runtime UI 采用，以及
MCP-3～5 的采集／纠正／开场。记录中的 suggested_relations 只保留为有来源的模型附注，
不伪装为已实现的关系图。下一片先适配已提交的分段读取接口，再接 Runtime-only 采集。
缺失时区使用 Asia/Shanghai 并保留默认来源，未知事件日期不补造。外部 Agent 宿主 hook
已明确取消；本仓库不改 Runtime 或真实 MCP 客户端配置。

## Owned-file allowlist

本片独立修改／新增下列文件；另外只引入明确的字段依赖提交，不重写上游字段类型。
依赖 44 文件中 38 文件已逐字节（换行归一）确认与上游相同；六个集成连接文件保留两侧。
无 reset/clean、私人初始化、部署、迁主、生产双写或自动化任务。

```text
contracts/next/agent-record-read-v2.schema.json
crates/enouia-memory-contract/tests/support/schema.rs
crates/enouia-memory-host/src/automation/agent_read.rs
crates/enouia-memory-host/src/automation/agent_record_fields.rs
crates/enouia-memory-host/src/automation/agent_tools.rs
crates/enouia-memory-host/src/bin/enouia-memory-mcp.rs
crates/enouia-memory-host/src/mcp/v2/mod.rs
crates/enouia-memory-host/src/mcp/v2/native.rs
crates/enouia-memory-host/src/mcp/v2/schemas.rs
crates/enouia-memory-host/tests/automatic_loop/agent_mcp_cases.rs
crates/enouia-memory-vault/src/automation/agent_read.rs
crates/enouia-memory-vault/src/automation/read_model.rs
docs/adr/097-agent-ui-record-reads.md
docs/adr/README.md
docs/contracts/MCP_AGENT_RECORD_READS_V1.md
docs/design/INTERFACES.md
docs/integration/RUNTIME.md
docs/integration/runtime-surface.json
docs/validation/MCP-2b-ui-record-reads.md
tests/fixtures/agent-record-read/manifest.json
tools/schema-check/check_schemas.py
```
