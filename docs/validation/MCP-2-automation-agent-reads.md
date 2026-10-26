# MCP-2a：自动化 V2 Agent 原生读取与真实本地传输

日期：2026-10-26。分支 `codex/mcp-v2-readonly`，依赖已经推送的
MCP-Resume-1 `16592dd0019370c3697756811447aa73dfcaa719`。
本片开始和交付前 fetch 的 main 仍为 `c9bf1be7819dba19915adf6031b1900c032b73d1`。
没有合并 main，也没有修改原始 checkout、Runtime、Claude 工作树、AGENTS 或手动状态快照。

## 本片交付与权限

[ADR 095](../adr/095-native-agent-automation-reads.md) 冻结原生 Agent 委托，
[ADR 096](../adr/096-versioned-automation-agent-mcp.md) 冻结独立版本的五工具与传输。
[契约](../contracts/MCP_AGENT_READ_V2.md) 和
[Schema](../../contracts/next/agent-read-v2.schema.json) 是本仓库的实现边界；
Runtime UI v2 的 03-MCP_REQUIREMENTS.md 是需求输入。

原生 Owner 注册精确 subject/project/source 集合、敏感度上限、独立 SourceRead 和短期有效期；
密封 permit 绑定当前策略及 Vault 实例。OS 登记绑定实际活进程与管道对端，客户端自报
principal/scope/PID/clientInfo 不授予权限。读端既不转发 System 读口，也不借用 Owner
Workspace。管道断开、撤回、到期或锁定后旧 reader 永不复活。

五工具 memory_search/read/source/related/recall 通过真实保护管道工作；标准 stdio bridge
可显式附着到已经登记的同一个 Host，不打开另一个 Vault writer。每工具声明闭合的
inputSchema/outputSchema 和只读注解；结果同时提供一致的结构化对象与 JSON 文本。
授权先于计数、排名、历史、来源与关系。读取只保留临时调用计数，不强化注意力。

Context 原生接口返回同一个 AgentReader 最后一次计算出的真实召回结果：trace、文本和
条目逐值相等，不再执行一次模拟排名。缓存不持久化，不证明客户端已经收到或使用结果；
快照变化或授权失效后不能读旧 trace。Runtime 实际接线与渲染验收没有发生。

## 版本、能力与拒绝矩阵

| 路径 | schema-1 / host-current-v2 | schema-2 / automation-host-v2 | 新 UI 根 |
| --- | --- | --- | --- |
| 原有六工具 / pipe-1 | 原有读取、待定提议、临时 checkpoint | 业务解码/查询/计数前拒绝 | 拒绝 |
| 新五工具 / pipe-2 / memory-agent-read-1 | 不兼容，无自动回退 | 明确支持范围内只读 | automation-host-v3、分段布局尚未适配，拒绝 |
| Core-7 System native | 自动化读端不支持 | 原有 System 权限，不是 Agent 凭证 | 按对应实现，不能转交 Agent |
| 新 Agent 形成/纠正/主人提交 | 不重开旧接口 | 本片没有 writer | MCP-3/4/5 未交付；主人提交不经过 MCP |

新握手同时校验 schema、布局、capability、MCP 版本和九份 Schema 的兼容哈希。
仅升级 source pin 或相同 schema major 不会授予新版读取。Memory 当前限定的知识、采用、
有效性和时间轴原样保留；「主体说过」不被提升为事实，System 采用不等于 Owner 确认。

## 实际合成证据

所有根、来源、策略、形成作业和 Agent 进程都由测试新建。形成成功回执来自现有真实
合法合成事务，不编造成功结构。没有私人 Vault、真实账号、凭据、Provider/embedding、
真实 MCP 应用配置、Activity/Usage 输入或远程服务。

- 原生读取七组回归：当前策略/来源/敏感度和先授权计数；独立来源权限；绑定 grant、
  snapshot、profile 的游标；锁定/解锁/撤回/到期/时钟回退；Owner/System 冒充和跨 Vault
  permit；隐藏祖先/删除与历史边；历史、预算、旧布局、四种时间轴及当前授权优先于旧排名。
- 真实独立测试 Agent：五工具均经过 Windows 管道；隐藏条目的 ID 不出现在结果，计数只含
  委托范围。UTF-8 来源片段保留截断说明；related 显式声明没有事实边/建议边能力。
- 同一条 100 次 search 的 attention 完全相同；前后全部业务文件逐字节相同。
  OS 独占的 `indexes/host.lock` 和 `vault/LOCK` 不做内容快照，不能声称比较了锁字节。
- 原生 Context 返回值与真实 MCP recall 的 data 逐值相等。旧客户端握手、伪造 major/
  principal/System capability、未支持实体过滤和 Workspace 参数均拒绝。
- 已失效连接保留原包，下一次调用仍为 closed，不自动重试/重连。没有 SourceRead 的已知
  拒绝不会关闭仍合法的连接，也不会制造未知结果或成功收据。
- 标准 `enouia-memory-mcp` 可执行文件：真实 stdio initialize/list/recall，经已登记管道
  进入既有 Host；结果与同一个原生 Context 相等；EOF 后自有进程退出；业务文件不变。
- 独立 python-jsonschema 对五工具实际广告的两种 Schema 及 106 次真实工具交换做校验；
  Rust 子集校验器另检真实输出和十个正反例。没有仅以自制成功 fixture 代替事务或传输。

## 检查记录

使用 pinned Rust 1.98.1；Rust 检查设置 CARGO_NET_OFFLINE=true。
以下最终检查实际通过：

| 检查 | 结果 |
| --- | --- |
| cargo fmt --all -- --check | 通过 |
| cargo test --workspace --locked -- --test-threads=1 | 686 项通过，零失败；包含 60 项 automatic_loop 与旧 Host/MCP 回归 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过；CARGO_INCREMENTAL=0，避免本机缓存硬链接警告，不改变产品逻辑 |
| Python 3.12 / pinned python-jsonschema 4.26.0 独立 cross-check | 51 schemas、10 Agent 正反例和 106 次实际交换全部通过；原有全量 schema/fixture 组同时检查 |
| python tools/integration/runtime_surface.py | 通过，58 命令；aggregate f54a0a612504d9ae8b84fbc164235e1b6f8fad40622697c644541adb85e2c578 已登记 RUNTIME.md |
| git diff --check；公开内容及 owned-file 核对 | 通过；28 文件，无私人绝对路径/凭据形态/冲突标记 |

传输/文件权限测试使用正常 Windows 用户令牌；受限令牌的 AccessDenied 没有通过放宽
产品 ACL 来规避。独立检查实际使用 `check_schemas.py --mcp2-transcript`，会执行全部原有
校验并额外检查运行中广告的 input/output Schema 和真实交换。日志、106 次合成传输记录
和 commit-message 留在 ignored target，不提交。

此前中间完整检查及最终源码的严格 Clippy 已完成；最终 workspace 串行运行用于避免把
已知历史作业测试的并发 Busy 波动当成本片业务变更。没有修改该无关作业逻辑。

## 与 03 要求的差距及下一片

本片交付的是 **MCP-2a / 当前 Core-7 的 automation-host-v2 读取**，不能称作完整 UI v2
MCP-2。交付前发现并行会话已推送
[字段 PR #56](https://github.com/Morii9961/enouia-memory/pull/56)，
源提交 `24a03e02bdf18094d60bc4d3268d380bd1b669a1`，定义 automation-host-v3；
该会话同时在实现分段规模/筛选/统计。本片没有修改其文件，也没有把新根伪装为已支持。
原计划使用的未提交 ADR 093/094 因并行占号改为 095/096。

以下字段/能力明确 unavailable，相关未支持过滤拒绝：title、content_lang、entities、
time precision/bound/tz-basis、扩展 formation note/prompt/import/live 元数据、主题、事实边和
建议边。当前版本的形成/知识/采用/事件/有效/learned/source-capture 字段可用。
待依赖字段和读取切片稳定后，应先补 MCP-2b：显式适配新根、完整字段和当前授权投影，
再交付 MCP-3 的 capture/note/live 管线，不能直接宣称新格式全部采用。

03 §6 中读取不强化、旧格式拒绝和只读本地传输已验证；采集形成字段与初始化覆盖一致、
引用归一定位、preflight、Formation/Inspector 实际 UI 和日常宿主采集均属于未交付验收。
字段契约目前要求逐字引文，比 03 建议的全半角/空白兼容更严格；后续必须按契约存原句，
任何归一定位必须显式设计，不能改写证据。未知时间不补成发生事实。

下一片开始前需要 Morii 决定可靠采集由哪个宿主 hook 负责，以及缺失时区使用何种默认值。
已有字段定义没有授权改真实客户端配置。forget 是否立即可逆隐藏在 MCP-4 前另行确认。
没有新增宏观路线、自动化、部署、生产双写或私人激活。

## Owned-file allowlist

只提交以下本片文件；其他文件不暂存、不覆盖：

```text
contracts/next/agent-read-v2.schema.json
crates/enouia-memory-host/src/automation/agent_read.rs
crates/enouia-memory-host/src/automation/agent_tools.rs
crates/enouia-memory-host/src/automation/application.rs
crates/enouia-memory-host/src/automation/mod.rs
crates/enouia-memory-host/src/automation/read_model.rs
crates/enouia-memory-host/src/bin/enouia-memory-mcp.rs
crates/enouia-memory-host/src/mcp/mod.rs
crates/enouia-memory-host/src/mcp/native.rs
crates/enouia-memory-host/src/mcp/v2/mod.rs
crates/enouia-memory-host/src/mcp/v2/native.rs
crates/enouia-memory-host/src/mcp/v2/schemas.rs
crates/enouia-memory-host/tests/automatic_loop.rs
crates/enouia-memory-host/tests/automatic_loop/agent_mcp_cases.rs
crates/enouia-memory-host/tests/automatic_loop/agent_read_cases.rs
crates/enouia-memory-vault/src/automation/agent_read.rs
crates/enouia-memory-vault/src/automation/mod.rs
crates/enouia-memory-vault/src/automation/read_model.rs
docs/adr/095-native-agent-automation-reads.md
docs/adr/096-versioned-automation-agent-mcp.md
docs/adr/README.md
docs/contracts/MCP_AGENT_READ_V2.md
docs/design/INTERFACES.md
docs/integration/RUNTIME.md
docs/integration/runtime-surface.json
docs/validation/MCP-2-automation-agent-reads.md
tests/fixtures/agent-read/manifest.json
tools/schema-check/check_schemas.py
```
