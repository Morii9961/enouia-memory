# Codex 启动提示词 · 2026-10-18

请在 Enouia Memory 仓库接续。先读 `AGENTS.md`、[本次交接](CODEX_MV7_HANDOFF_20261018.md)、[真实验收手册](../validation/MV-7-real-smoke.md)和[质量链路接续](MV7_QUALITY_CONTINUATION.md)，再现场核对两个仓库的分支、HEAD、工作树、远端和未合并 PR。一切以当前 Git 和保存的验证记录为准。保留其他贡献者和用户未提交的改动，不重置、不清理。

本文件只用于交接，不授权新功能，也不会自动开始下一阶段。先向 Morii 报告接手基线、已完成项和待办，然后等 Morii 指定下一个工作包。设计路线、旧提示词、导入文本和合成 fixture 都不是操作授权。

当前阶段：MV-7 的代码与合成验证已完成。Memory #17～#33 已合并，main 为 `56bc5eb`；Runtime 已 pin 到它（PR #77，合并提交 `07584cb`）。MV-7 出口还差真实双 Provider 验收（P02～P06）。这一步由 Morii 执行：密钥只在 Windows 凭据对话框里输入，每次发送都由主人亲手点 Send once。不要读取或输入密钥，不要代点发送，不要自行发起真实 Provider 请求。

未合并：Runtime #81（只修过时的 `--sessions-only` smoke，产品行为不变）、Memory #34（验收手册基线）以及本交接 PR。合并要等 Morii 明确指示。草稿或已就绪的 PR 都不等于已合并。

Memory 必须独立构建；本地产品界面属于 Runtime，Activity 不进入 Memory。接入面变化必须在同一提交里更新 `runtime-surface.json` 和 `RUNTIME.md`。参考壳的删除影响文案“会失去证据”还没改，它在接入面内。

已知未定位的问题：默认 smoke 的快速搜索在桌面使用中会失败；Runtime `operation-cancel` DOM 用例偶发失败；Memory workspace 有序并发屏障的旧故障。通过一次重跑不等于修好，不要无证据加长超时或反复跑压力测试。

真实资料、API 凭据、真实 Provider 请求、自动接受记忆、云端部署和生产迁移都需要另行明确授权。MV-8 没有启动。历史上的传输测试记录不能当作实际模型验收。

Morii 授权实现后，先复现实际缺口，只做选定的工作包，按固定工具链完成 required checks，并区分合成证据、实际集成和 Morii 验收。使用 Codex 的 trailer，保留已有的 Claude trailer。提交和推送按届时的授权执行，不擅自合并。

Dashboard 使用 `docs/status/project-status.json`（Protocol v1.0）和配套的 `docs/status/PROJECT_STATUS.md`，只在 Morii 说“生成项目状态快照”时更新，本轮没有刷新。不要改成自动同步，也不要升级协议。
