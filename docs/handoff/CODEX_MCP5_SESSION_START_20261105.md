# MCP-5 session_start 接续（2026-11-05）

c346838 已推送 initialized/v2 读取控制字段修正，草稿 #85 已更新。其后已复现
并收束 catalog/qualified 的同两字段。owned allowlist：这两个 MCP mod.rs、
segmented_agent.rs / qualified_mcp.rs 对应定向测试、agent-catalog/qualified-read
合成 manifest、本交接/read-control 报告、integration manifest/RUNTIME.md。
不改领域全局 enum，不调用 NativeMemoryOwner，不改变有效工具/权限/布局/指纹。
2026-11-06 接续：这两个入口各复现 8 个反例后复用 string_enum。直接入口 2 项、
实际 stdio 2 项、surface 4 项通过；独立 Python 76 schemas、两个 manifest 各
22 件通过。定向 Clippy/fmt/surface/公开检查通过，无全量。此前 Clippy/surface
因审批检查额度耗尽未执行，现已同一步重试通过，未把失败审批写为验证通过。
当前 aggregate 为 2c856f8deb96d9dd8bafc68ee4a8f97bd6e66f2e28808aa11cb56af768a38520。
四个读取 adapter 的 operation/intent 对象标签缺口已收束，未宣称领域所有 enum
均完成合同一致性验证；详见 read-control 报告的 2026-11-06 小节。

本片从 2fde3f7 接续：反例确认读取 operation / intent 的字符串合同被 serde
对象标签绕过，context/initialized 共 16 条路径已复现并最小修正。owned
allowlist：mcp/json.rs、initialized/mod.rs、v2/mod.rs、opening_mcp.rs、必要的
对应读取测试/合成语料、ADR126/合同、本交接/验证报告及 integration manifest /
RUNTIME.md。不改 AGENTS、手动状态或其他 worktree，不新增方法/权限/持久化。
实现为 json::string_enum，在 initialized/v2 的两个控制字段处要求字符串，再
传原类型向内。Host 12 项、实际管道 3 项及 surface 4 项通过；独立 Python
76 schemas（两语料分别 18/22 件）通过；定向 Clippy/fmt/公开检查通过，无全量。
本片 aggregate 0a969a802b2d919f209cb976662b2e9f3112d689fbd1ad5aaad9d5417f2a0c76，
schema/hello 指纹未改。报告见 [read-control](../validation/MCP-5-read-control-strings.md)。
提交后接着核实 catalog/qualified 相同字段，不做领域全局 enum 重写或新 MV。

## 当前基线与授权

Morii 要求沿既定路线自主继续，不逐项索要指令；已明确启动 MCP-5 合成开发，
其他硬边界继续保留。当前分支 codex/mcp5-opening-resources，最新实现提交
c346838 已推送；其后的 catalog/qualified 修正以本文件开头及 Git 为准。
草稿 PR #85 仍以未合并的 MCP-4 分支 / PR #84（4aa1605）为
base；不合并。origin/main 为 PR #83 的 2a6310e，包含 f42f3b1，已核对远端。
其他 main / captured-source-lifecycle worktree 保留；无 reset/clean。

本次从自己的 399a217 接续，保留 AGENTS 与手动状态快照。主仓库 MCP-4 本机说明
存在，私人数据和真实工具配置未打开。Runtime 并行工作仍以它自己的交接为准。
本文件收束重复记录，本次文档整理只拥有本文件，不改实现或状态快照。

## 本轮已交付

| 提交 | 内容 |
| --- | --- |
| bf4a747 | 先冻结 ADR127 / AGENT_SESSION_START_MCP_V1，再实现可被 Agent 调用的只读 session_start |
| 9bd2b5d | 标准 resource_link；独立完整成功响应合同，纳入 exact pipe-10 指纹 |
| e6e95d2 | prompt / tool 双入口形成后旧链接失效、同连接显式新读的实际管道验收 |
| 310d5ba | 实际可执行文件 stdio 透传、链接取回及 EOF 撤权验收 |
| 0d2f25f | 旧 pipe-5 / pipe-6 拒绝新工具并保持原读取 / note 保存能力 |
| c9eb8f4 | 复现并修复 kind 对象形状被 serde 单变体枚举错误接受的问题 |

session_start 复用同一 QualifiedAgentReader 与 memory_start 的有界召回：
min(4, profile.max_items) / min(16384, profile.max_bytes)，返回原 request_id 和
同一 actual-context payload。它不创建持久 Session、不捕获、不 checkpoint、
不形成记忆、不调用模型/Owner，不创建收据、作业或必须由主人处理的队列。
新 capability 标记合同，不产生授权；独立 MemoryRead 仍必要，SourceRead 可选。
只有 pipe-10 context profile 为六个读工具；旧 pipe-5/6 和报告 profile 保持原义。

成功含原结构化响应及其文本副本，再附一个静态名 actual_recall 的资源链接。
按链接读取仍受同一 reader 的授权、当前 scope/snapshot 和缓存检查；不重算、
不自动获取、不授予权限。新 memory / prompt / tool 召回共享实际缓存规则。
旧链接失效后，只有仍有授权时的显式新读可取当前结果；这不适用于未知发布。

kind 缺陷先有失败反例，再最小改为 String + 既有常量检查。对象 kind 现在在
native recall 前拒绝；schema / capability / hello 指纹未因这次纠错改变。
对应源码 mcp/opening/mod.rs、native.rs；合同及两个 session schema 在 contracts/next。

## 实际证据

完整报告：[MCP-5-session-start](../validation/MCP-5-session-start.md)，旧 MCP-5
开场/响应合同报告继续有效。报告逐片记载日志和运行范围，不能简单相加冒充全量。

- session_start 首片当前三个 Host 目标共 21 passed；合同/表面 7 passed。
- 资源链接片相关 Host 13 passed；合同/表面 8 passed；额外指纹断言单项通过。
- 后续形成后双入口、实际 stdio 出口各一个定向测试通过；旧 profile 两项通过。
  只运行所列旧用例，没有执行调用 NativeMemoryOwner 的旧测试。
- 最新 kind 修正后：session 测试 3 passed、实际管道成功路径 1 passed、
  合同/表面 8 passed，0 failed。修正前反例失败日志保留，未伪装为全程通过。
- 独立 Python jsonschema 4.26.0：76 schemas、19 session cases、16 link cases、
  实际 session 输入/输出、完整响应及既有 opening 报文通过。原 ID、文本/结构化
  内容、链接/实际资源相等另有断言；JSON Schema 本身不保证这些跨字段关系。
- 各片定向 Clippy、fmt、适用的 surface 与推送前公开检查通过。未跑全工作区，
  不把历史 913 / 948 当作当前 HEAD 证据。纯测试/文档片未重复 schema/surface。

当前 source aggregate：
e4b5c470c69dbe36a9f85d0d9588d036a1222a720f8aab94e816ac56cb94c4c3。
runtime-surface.json 与 RUNTIME.md 已在同一实现提交登记；没有修改 Runtime 仓库。

## 继续边界

继续仅在已授权 MCP-5 的合成开场/资源范围处理有依据的实现或合同缺口；当前没有
未收束代码片。新片先列 owned-file allowlist，先冻结最小合同，再同片实现、
按风险验证、公开检查、带 Codex trailer 提交推送并更新草稿。保留其他贡献者署名，
不刷新手动项目状态，不将历史未完成提案当批准，也不自动启动新 MV 或真实接入。

只用新建合成根和自有进程，不打开私人 Vault、导出、备份、恢复材料或凭据。
Runtime-only capture；不开放 conversation_capture / 外部 Agent 宿主 hook。
Agent 提示/报告仍是不可信解释，内容/证据必须追到真正 Runtime UserStatement；
Agent 文字不能替代原话或 Owner 同意。不接真实模型/Provider/embedding，模型、
预算与外发仍需独立决定。forget-hide 尚未选择，不实现立即隐藏。不改 Runtime
pin、真实配置或共享外壳，不读 Activity/Usage，不部署、迁主、双写或创建自动化。

OutcomeUnknown / RecoveryRequired 保留完整首个原 Value/原键，不回原生工具结果，
实际断开并撤权，显式恢复；不自动重发、重连或换键。含 AgentNote / Correction /
Supersede / Forget delta 的根不能由旧二进制打开。合成结果不代表真实模型质量、
私人库验证、Runtime 产品采用或主人验收；合并仍须 Morii 单独指示。
