# Claude 下一轮：MV-0R 独立仓库迁移与契约修正

Morii 已纠正工程归属：**Enouia Memory 必须在 Memory 仓库（本地 checkout） 独立开发，使用公开 GitHub 仓库 `https://github.com/Morii9961/enouia-memory`。** 此前将 Runtime 指定为实现仓库的提示词已失效；这是 Enouia 原交接的问题，不要求你为按旧提示词执行而重做全部成果。

本轮只完成 **MV-0R：安全迁移现有 MV-0 成果、修复契约问题、独立复验、移除 Runtime 中误放的本次 Memory 改动**。完成后停止，不进入 MV-1，不实现 Vault 存储/模型/UI/MCP/VPS。

## 先读

完整阅读 `docs/reviews/MV0_REVIEW_AND_REPO_CORRECTION.md`，尤其 F1～F6、迁移步骤 R0～R5、反例输出；再读现有设计包和两边适用的 AGENTS.md。重新检查 Runtime 当前 HEAD/status/diff；不要假设它仍停留在 cd14bcf。

GitHub 仓库已由 Enouia 创建并确认公开，审核时为空。不要再次创建或覆盖远端；接手时先核实是否已有新内容。本提示词不是声称迁移已完成。

## 执行顺序

1. **保护来源**：先记录本次 Memory 路径清单、哈希及共享文件精确 diff，保存未跟踪文件与补丁的本地恢复快照。不要先删 Runtime 中的任何成果。禁止 reset --hard、git clean 或将六个共享文件整体恢复到旧提交。
2. **独立 workspace**：在 Memory 仓库（本地 checkout） 建 Git/Cargo 工作区，连接已有 remote。先设置忽略规则，不把真实记忆、Raw 导出、附件、会话、数据库、密钥、备份材料、迁移快照和 target 入 Git。保留已有设计原稿，明确哪些是历史、哪些为新规范。
3. **复制并解耦**：复制本次 Memory 契约、schemas、fixtures、文档与测试，核对哈希后再改路径。解除 `../enouia-common` 以及测试加载 Runtime Activity schemas 的依赖。Memory 拥有自己必要的最小端口/DTO，Runtime 将来通过适配器接入；不要引入跨仓库绝对或相邻目录 path 依赖，不复制 Activity 实现。
4. **修复全部审核发现**：F1 外发批准必须绑定精确请求/内容/目的地；F2 删除/降级批准必须绑定操作和目标；F3 同长度不同 payload 不能匹配 Dispatch；F4 补持久 Policy/Grant 与目标感知的 PolicyGate；F5 未知生效时间保持未知；F6 数值范围与 checked arithmetic 防止 panic。
5. **补真实回归验证**：原 Memory 测试与新增反例一起通过；引入固定版本独立 JSON Schema 验证器交叉检查，不能仅依赖本轮自写的 validator。契约修正同步更新 schemas、类型、ADR、fixtures 与约束表，不只修改测试期望。
6. **独立复验**：从 Memory 自身根目录执行 fmt/test/clippy 及 Schema 校验。用隔离 checkout/工作目录证明不需要 Runtime 仓库（本地 checkout）；不通过删除或重命名别人正在使用的 Runtime 目录来测试隔离。
7. **精确清理来源**：目标可恢复且复验通过后，再检查 Runtime 最新状态，只清理与快照匹配的本次 Memory 文件和 hunks。保留其他贡献者 Activity 变化；发现混合改动先保留并报告，不做整文件回退。对 Runtime 清理后的受影响范围完成验证。
8. **更新文档与交付**：修正工程路径、运行数据根、依赖边界、MV 阶段映射与启动入口。保留旧报告作为历史，写 MV-0R 实测报告。新仓库的提交/推送按本次实际授权及贡献约定执行，不因旧文档声称“未授权”而忽略当前已有明确授权；未经过公开内容检查不得上传。

## 必须复现并修好的反例

- 普通 accept review 不能当成 private 内容的外发确认。
- 普通 accept review 不能当成 tombstone 的删除确认。
- 普通 accept review 不能当成 private → normal 的降级批准。
- 实际消息改成相同字节长度的其他内容时，Dispatch 校验必须拒绝。
- effective_from 与 valid_from 都未知时，不得用 approved_at 让旧事实确定失效、新事实变成 current_supported。
- 极大 u64 token 预算应返回结构化错误，不 panic；Schema 与 Rust 范围一致。
- 同用户、同敏感度但不同项目/Provider 的请求必须可以依据授权得到不同结果。

这些问题已在合成资料上验证，不涉及真实泄漏。审核报告中的临时 Rust 探针可作复现参考，但不要把临时二进制/target 当正式工程文件迁入。

## 交付报告

说明迁移前后清单与哈希、独立仓库/remote 状态、F1～F6 修复位置和测试、Schema 交叉验证、独立构建证据、Runtime 精确清理结果、保留下来的其他工作、提交状态与尚未激活能力。

本轮不再把“35 项测试通过”单独作为“MV-0 已冻结”的依据。完成 MV-0R 后停止，让下一轮从已经复验的独立 Memory 工程进入 MV-1。
