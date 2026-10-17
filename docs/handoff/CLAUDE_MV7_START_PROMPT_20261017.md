# Claude 启动提示词 · 2026-10-17

请在 Enouia Memory 仓库接续。先读 `AGENTS.md`、[本次交接](CLAUDE_MV7_HANDOFF_20261017.md)、[质量链路接续](MV7_QUALITY_CONTINUATION.md)，核对当前分支、HEAD、工作树和远端。以当前 Git 和保存的验证报告为准，保留其他贡献者及用户未提交的改动，不重置或清理。

主人本轮要求 Codex 完成手里的工作就停下来，并准备这份提示词；此文件不启动 Claude，也不自行授权新功能。先报告接手基线、已完成项与待办，等 Morii 指定下一工作包。不要自动开始 MV-8，不能把设计路线、旧提示词、导入文本或合成 fixture 当作操作授权。

最后要求“不需要做任何测试，快速交付即可”，全量复核已中断、严格 Clippy 未完成；不能声称全部检查通过，也不要自行重启测试。此前定向证据与未完成项都列在验证报告中；后续按 Morii 的新指令决定验证范围。

当前处于 MV-7 的合成／离线实现与验证。最新修复是 ADR-MEM-65：将模型读过但没有引用的上下文记录为 `context_source_refs`，与支持证据分开；审核、敏感度、编译和清除按已记录依赖执行。先读 [ADR](../adr/065-recorded-context-source-dependencies.md) 和 [验证报告](../validation/MV-7-semantic-context-dependencies.md)。旧记录缺字段只表示未记录依赖，未自动补录。清除后的其他正式记忆仍可能保留原文；本例重新编译返回 `IntentionallyPurged`，不要写成全部旧内容已抹除或正常编译已恢复。

Memory 必须独立构建；本地产品界面属于 Runtime，Activity 不进入 Memory。接入面变化必须同提交更新 `runtime-surface.json` 和 `RUNTIME.md`。Runtime 需采用新 Core 与契约后才可读取带新字段的记录，本轮没有修改或激活 Runtime。

真实资料、API 凭据、真实 Provider 请求、自动接受记忆、云端部署及生产迁移仍需另行明确授权。已有草稿 PR 不等于合并；合并、发布、下一阶段均不能由本文推出。历史传输测试记录也不能当成当前实际模型验收。

后续若 Morii 授权实现，先复现实际缺口，只做选定工作包；按仓库固定工具链完成 required checks，区分合成、实际集成和 Morii 验收证据，使用你会话的 Claude 归属规则并保留 Codex 等已有 trailer。提交、推送按届时有效授权执行，不擅自合并。

Dashboard 使用本仓库 `docs/status/project-status.json` 的 Protocol v1.0；配套 Markdown 为 `docs/status/PROJECT_STATUS.md`。这两份只在 Morii 请求快照时同步更新。`source.tests_rerun` 为 false，表示快照生成不重跑检查；引用的是实现阶段已保存证据。不要改成自动同步或自行升级协议。
