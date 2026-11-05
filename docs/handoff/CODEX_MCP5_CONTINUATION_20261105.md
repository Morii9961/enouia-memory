# MCP-5 接续（2026-11-05）

f731161 已提交推送完整响应合同；14 项 Host 定向结果、2 项合同、74 schemas / 31
wire cases / 7 实际响应 / 4 内层数据、scoped Clippy/fmt/surface/公开内容检查通过。
随后纯测试/文档加强的身份矩阵六项测试通过：15 种拒绝配置，以及两种 source-disabled
reader 的成功登记配置。pipe-9、错误 Vault/Agent、缺少 MemoryRead 均拒绝；更宽的
SourceRead 进程登记不升级 native reader。所有拒绝均撤权，读取不改快照。

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
