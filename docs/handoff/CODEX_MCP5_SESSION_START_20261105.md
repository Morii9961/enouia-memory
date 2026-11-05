# MCP-5 session_start 接续（2026-11-05）

Morii 再次要求沿开发路线自主继续，保留全部硬边界，不逐项索要指令。
9bd2b5d 已推送标准资源链接与完整成功合同。其后开场工具在新记忆形成后的实际
管道生命周期验收已通过：旧链接拒绝、同连接显式新读、来源仍为 Runtime
UserStatement；不改变实现或权限。当前片 allowlist 仅 opening_pipe.rs、验证
报告和本交接；未因测试改动重跑 schema/surface，全工作区也未运行。
bf4a747 已推送 session_start 实现，草稿 #85 已更新。其后补标准 resource_link
及完整工具成功响应合同，先补充 ADR127/合同再编码；不新增方法/权限/持久化。
本片 owned allowlist：现有 session schema/corpus、opening/mod.rs/native.rs、
opening_session_start/opening_pipe 和 mcp_opening 测试、Python schema checker、
ADR127/本合同、本交接、验证报告及 integration manifest/RUNTIME.md。
起点 codex/mcp5-opening-resources / 399a217，依赖草稿 PR #85（base #84）；
main 仍为 PR #83。旧交接列 session_start 未交付，现有 memory_start 是 prompt，
故本片先冻结 ADR127 / AGENT_SESSION_START_MCP_V1，再实现可被 Agent 作为工具
调用的只读开场 recipe。没有开始新 MV、真实模型或 Runtime 采用。

session_start 不创建 Session、不捕获、不 checkpoint、不形成记忆。复用同一 reader
和 memory_start 的有界召回；输出同一个 actual-recall context，进入原缓存语义。
新 capability 只标记工具合同，不产生权限。只有 pipe-10 context profile 增至六个
读工具；pipe-5、pipe-6、报告 profiles 保持原义。Guide 新增 opening_tool；exact
fingerprint 绑定新 descriptor/schemas，旧端点不回退。实现及本片定向验证已完成。

Owned files：ADR127/register、新合同与 session schema/corpus；mcp/opening 服务和
native 指纹；opening_mcp/opening_pipe 与新增 session 工具测试；Python schema
登记、既有 guide 合成样例；runtime-surface.json/RUNTIME.md；本交接和验证报告。
AGENTS、手动状态快照、其他 worktrees 和并行修改均保留。提交需 Codex trailer，
公开检查后推送既有接续分支并更新草稿 #85；不合并。

本片证据见 [session_start 验证](../validation/MCP-5-session-start.md)：当前 Host
三个目标共 21 passed，合同/表面测试 7 passed；独立 Python 75 schemas、18 个新增
语料和实际输入/输出 2 件通过；定向 Clippy、fmt、surface 通过。未跑全工作区，不
沿用 913/948 为当前证据。surface aggregate 为
05a92cd3e3cc0889f84e68fcec016617f75ced5f50b227b8982d895a0cde2fde。
实际 session 工具在无 SourceRead 的 MemoryRead 租约上返回资源逐值一致；原生未知
发布则不回结果、保留完整原包并断开。未改持久化格式，也未创建 Session。

资源链接片：工具成功文本后增加标准 resource_link，地址等于实际 context.uri，
读取仍走当前 reader 的原生缓存与快照检查。独立完整 wire schema 纳入 exact
fingerprint；旧五工具结构不变。相关 Host 13 passed、合同/表面 8 passed，独立
Python 76 schemas / 16 新增 wire cases / 实际完整响应及 payload 通过；定向
Clippy、fmt、surface 通过。当前 aggregate：
ef4679f92dc027f24d64378fa43b80d3756aec354f81ef3fa1f1e8df8d512320。
原 RPC ID、文本/结构化内容相等、链接/实际资源相等均独立断言；schema 本身不保证
这些跨字段关系。完整证据及未做事项仍见上述验证报告，不把新结果写回手动状态快照。

继续同一草稿分支的 MCP-5 开场/资源合成路线；每片先冻结最小合同、实现验证、公开
检查、提交推送并更新草稿；不要为继续工作逐项询问，也不能把此授权扩为新 MV 或
真实接入。既有主交接仍有效，本交接补充 ADR127，不替代 Runtime 自己的交接。

最终行为/检查以本片验证报告与实际日志为准。只有新建合成根和自有进程；不打开
私人 Vault/导出/备份/恢复材料/凭据，不读 Runtime Activity/Usage，不改其仓库、
pin、真实配置或共享壳。无外部 Agent capture hook、真实模型/Provider/embedding、
forget-hide、部署、迁主、双写或自动化。未知结果保留完整原 Value/原键，停止连接 /
授权，显式恢复，不自动重发/重连/换键。不要把 schema 通过当作真实模型质量或采用。
