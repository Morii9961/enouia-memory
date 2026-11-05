# MCP-5 接续（2026-11-05）

2cb7604 已推送授权失效验收，本轮已核对远端与本地一致、main 未变，并补齐 #85
说明。后续单项跨入口实际管道验收通过：不先调用 prompt，memory_recall 工具的
实际结果即可从 recall URI 逐值读取；工具与 prompt 的成功召回互相替换缓存，失败
召回不覆盖前次成功结果。跨入口重复 RPC ID 仍拒绝，读取不改快照，EOF 撤权。
开场 prompt 是可选入口，不成为自动读取的人工前置任务。实现/schema/指纹未改。

d51e341 已推送跨入口验收。随后加强 source-disabled 的两种原生登记配置：
memory_source 返回已知 permission_denied、无 data/原文，但不关闭仍有效的
MemoryRead 连接；后续召回成功，其实际资源逐值一致，快照不变。单项测试通过，
无主人提权任务或权限推导。只改测试/报告，复用未变实现的相关证据。

f731161 已提交推送完整响应合同；14 项 Host 定向结果、2 项合同、74 schemas / 31
wire cases / 7 实际响应 / 4 内层数据、scoped Clippy/fmt/surface/公开内容检查通过。
随后纯测试/文档加强的身份矩阵六项测试通过：15 种拒绝配置，以及两种 source-disabled
reader 的成功登记配置。pipe-9、错误 Vault/Agent、缺少 MemoryRead 均拒绝；更宽的
SourceRead 进程登记不升级 native reader。所有拒绝均撤权，读取不改快照。

1d01558 已推送身份矩阵。后续单项实际管道测试通过：保持同一 reader/连接，保存
可选 AgentNote，再从已授权的合成 Runtime UserStatement 经普通 System evaluator
形成第二条记忆。旧 recall URI 已过期且不重算；明确的新 read ID 可在原连接取得
当前两条记忆，新 URI 逐值一致，旧 URI 不冒充新结果。无模型/新捕获授权/自动重连，
Agent 文字不入内容，读取不改快照，无采集作业或主人操作；EOF 后 reader 撤权。

f7c92d5 已推送同连接过期流程。随后补齐内层“按方法选输出定义”的校验：总合同也
接收合法请求，不能单独证明一个输出 slot 正确。36 个外层样例含五个内层类型反例；
请求包、错误类别资源、报告回执或坏 JSON 即使外层合法，也不被当作开场数据。
七个实际响应与四份实际内层数据按方法专用源定义和导出 bundle 均通过 Python
检查（74 schemas），两项合同测试通过。只改测试工具/样例/说明，无生产权限或
schema/指纹变更；可选合成 transcript 需用当前测试重新生成，不添加旧格式兼容。

af99484 已推送方法专用内层校验。后续单项实际管道测试通过五种常见授权失效：
主动撤权、锁定后解锁、策略换版、到期、进程登记撤权。均实际断开、无新结果、
完整首包保留，下一次发送为 Closed 且不覆盖原包；旧 reader 仍失效。只在锁定 /
解锁这一例，由原生测试操作者明确新建 reader、进程和登记后接收原 read Value；
不自动重连/重发/换键，不涉及 Vault 发布恢复或私人恢复材料。实现/指纹未改。

Morii 要求继续既定路线，并已明确授权 MCP-5 合成开发；其他硬边界保留。
当前 codex/mcp5-opening-resources 以 f425c17 接续，工作区开工时干净。
origin/main 仍为 PR #83 的 2a6310e，包含 f42f3b1；PR #84/#85 未合并。
同机 main 和旧 captured-source-lifecycle worktree 保留，无 reset/clean。
本次读取 #85 后没有新评论或评审线程，不代表 CI 或主人验收。

本片先补充 ADR126 / MCP_OPENING_RESOURCES_V1：保留既有内层记忆 payload schema，
新增 mcp-opening-wire-v1，覆盖五种新方法的实际 JSON-RPC 成功响应。方法专用
response_schema(method) 与五份 schema 纳入 pipe-10 的 exact hello 指纹。
没有新方法、写授权、捕获、模型或内容形成行为；旧 profile 和错误响应不变。

实际管道现取证七个响应（五种方法，三种资源 URI），逐项验证外层结构，并独立
解析/验证四份内层 payload。另从实际目录/指南/prompt 重建握手指纹，证明新合同
确实被绑定；缺少五份响应 schema 的原指纹不同。没有运行旧二进制或重连回退。
已有八项开场服务测试通过，六项管道测试和两项合同测试通过。独立 Python 检查
已通过：74 schemas / 31 wire cases / 7 实际响应（含方法专用 bundle）/ 4 内层
payload。响应原 ID、资源原 URI 和内层 URI 另有定向对应检查通过。最终提交与完整
证据说明见 [本片验证](../validation/MCP-5-wire-contracts.md)。

Owned files：新 wire schema/corpus，mcp/opening 的 schema 导出和指纹，opening_pipe
和合同测试，Python cross-check 登记；ADR126/现有合同补充、integration manifest /
RUNTIME.md、本交接和验证报告。其他并行文件不改。AGENTS 保留，手动状态快照不更新。

后续仍在 MCP-5 开场/资源的合成合同与验收范围，不自动跨新 MV。不接真实模型 /
Provider / embedding，不读私人 Vault、聊天导出、备份或凭据，不改 Runtime 仓库 /
pin / Activity / 实际配置。外部 Agent capture hook、forget-hide、部署、迁主、双写、
自动化和 main 合并未获授权。合成结果不代表真实模型质量、Runtime 采用或主人验收。
OutcomeUnknown 保留完整原 Value/原键，断开/撤权，显式恢复；不自动重发或换键。
开工先核对 branch/HEAD/status/origin/worktrees，再读当前合同和对应实际源码。
