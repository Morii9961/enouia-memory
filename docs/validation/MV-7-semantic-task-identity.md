# MV-7 · 相同输入的语义任务身份隔离

日期：2026-10-16。基线 `bc6baa6`（PR #30）；范围为既有 AnalysisGuard、VaultAdapter 和质量 builder 的合成集成验证，生产行为不变。

## 验证目标

为同一选定来源和主题创建两个不同 Session/输入事件的语义任务，即使输入 hash 相同也不能互换任务身份。重新打开合成 Vault 后，交叉任务/adapter、伪造 Session 和伪造输入 hash 的调用应在假凭据读取、调用接纳与传输前被拒绝。拒绝不改当前提交和调用台账，且不生成质量材料。

拒绝后用正确配对发送一个受控假响应，确认合法路径仍可完成、保存响应并构建 pending/unknown 的只读材料。仅使用已有两家 Fake Provider 和临时合成 Vault；不证明真实出口或 Runtime 产品验收。

## 结果

新增单个用例 semantic_tasks_with_equal_input_hashes_keep_identity_after_reopen，内部覆盖 OpenAI/Anthropic × 旧/谱系四种组合。定向检查 exit 0；四组都实际得到相同 input hash、不同 Session 和输入事件，并在同一进程中重新打开 Vault 后执行后续流程。这不是独立进程重启或断电恢复证明。

每组四种拒绝：另一合法任务的 guard 配左侧调用、左侧 adapter 配右侧任务、伪造 Session、伪造输入 hash。预期错误分别为 PermissionDenied 或 InvalidRequest；假凭据读取与传输计数均为 0，两侧调用台账为空，当前提交不变。伪造 Session/hash 也不能构建完整分析输入的质量材料，提交继续不变。

每组随后以左侧正确配对完成一次假调用：只读一次假凭据、传输一次、左侧调用台账仅一项，右侧仍空；保存响应能重新取得原任务身份。质量 builder 返回 pending、空 item/unit 判断，且不推进当前提交。没有发布或批准记忆，没有生产 guard、请求策略或重试逻辑变更。

固定离线工具链的完整必需检查全部 exit 0：fmt、默认并行 Rust 388 项（Provider 79 项）、严格 Clippy、Python 63 项、独立 JSON Schema 36 项和 Runtime surface。公开内容检查覆盖本片三个文件、14 个本地链接、路径/凭据形状及 diff 空白，两份状态快照 hash 不变。定向日志、各项单独退出码及汇总保存在忽略的 mv7-semantic-identity-* 文件。

## 接入与限制

仅改 Provider 测试和验证/交接文档，无生产 Rust、依赖或契约变化。Runtime surface 检查为 47 commands / aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`，无新宿主采用动作。拒绝和合法对照均为本地 Fake Provider，不证明真实模型质量、真实网络出口或 Runtime 产品采用；未启用真实资料/凭据、MV-8、部署或状态快照刷新。
