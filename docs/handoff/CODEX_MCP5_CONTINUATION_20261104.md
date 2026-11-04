# MCP-5 合成接续（2026-11-04）

Morii 已明确启动 MCP-5 合成开发，其他硬边界继续保留。当前 checkout 的
codex/mcp5-opening-resources 从已推送 MCP-4 收束提交 4aa1605 新建；依赖草稿
[PR #84](https://github.com/Morii9961/enouia-memory/pull/84)，未合并 main。
原 main 仍为 PR #83 的 2a6310e（包含 f42f3b1）；旧 worktree 和 AGENTS 保留。

实现 c34a428 已提交推送，草稿
[PR #85](https://github.com/Morii9961/enouia-memory/pull/85) 以 PR #84 分支为 base，
没有合并。61 个 Host 定向/回归、最终 11 个本片测试、1 个合同测试、73 schemas、
scoped Clippy/fmt/surface/公开内容检查通过。后续纯验收加强：13 个本片测试通过，
证明同一 Agent 不同 reader 的缓存隔离、MemoryRead-only 管道、SourceRead 不推导、
ContextRead 不充当授权别名，以及四项上限/更窄 profile/一字节预算不产生主人队列。

109b15b 已推送连接/来源/预算验收。后续单项跨 profile 测试通过：真实合成 Runtime
UserStatement 经普通 System evaluator 形成记忆；同时保存带恶意主人授权声明的
AgentNote/Correction/Supersede/Forget 解释。开场和 memory 资源只读出原记忆与
真实捕获原话，Agent 文字不进入内容/证据，不阻塞读取、不隐藏目标、不产生主人操作
或采集队列。没有启动消费者分析器、模型或新的宿主 hook。详见同一验证报告。
同项验收还将四种已被原生端保存的合法写包（原工具名/伪装 memory_read）提交给
开场连接，八次全部拒绝且快照不变；同 Agent 持有的独立写 permit 不升级 reader。

本片 ADR126 / MCP_OPENING_RESOURCES_V1 先冻结，再实现只读开场提示与资源。
新 pipe-10/profile 使用同一个 initialized QualifiedAgentReader，沿用五个读取工具，
新增 memory_start prompt、静态指南、当前 memory detail 和同连接实际 recall 资源。
没有写 permit、Runtime 捕获、真实模型调用、Owner 操作或必须逐项处理的队列。
prompt 的 MCP user 角色不成为 Runtime UserStatement 或主人授权。

Owned files：mcp/opening、共享 ReadSession 能力声明、initialized schema 内部可见性、
显式 launcher 分支；qualified_agent 的专用 context 连接检查、Vault agent 连接轮询；
对应合同/schema/corpus、两组 Host 测试、合同测试、Python cross-check 登记、ADR
register、integration manifest/RUNTIME.md、本交接和验证报告。其他文件不要改。

发现并收束的实际问题：只读 metadata 轮询没有观察本实例的 unknown publication
恢复要求。现在轮询检查 native.available；新 MCP profile 将 OutcomeUnknown 和由
该未知发布锁定的 RecoveryRequired 都视为无结果、首个完整 Value 锁存和停止。
新 context 入口还即时检查策略版本；旧 pipe-5 保留原有拒绝方式和工具能力。
策略检查误放共享入口的版本曾导致现有 pipe-5 定向测试失败，已隔离到新入口。

验证及最终提交以 [MCP-5 报告](../validation/MCP-5-opening-resources.md) 为准；
未完成日志不能当通过。启动时先核对 Git status、当前 HEAD、origin、worktrees，
保留并行修改。只使用新的合成根与自有进程，不打开私人材料、真实配置或 Runtime。
不调用真实模型/Provider/embedding，不改变 Runtime pin，不部署或更新手动状态快照。
forget-hide、真实模型预算/外发、产品采用和合并仍需 Morii 独立决定。

后续仅在已授权 MCP-5 的开场与资源范围内推进合成合同和验收；不自动跨新 MV。
不得把合成结果写成真实模型质量、Runtime 已采用或主人验收。OutcomeUnknown
保留原始包原键，断开/撤权，显式恢复；不自动重发、重连或换键。含新 delta 的旧根
不可交给旧二进制。本片没有新增链 delta，也没有处理任何私人或真实根。
