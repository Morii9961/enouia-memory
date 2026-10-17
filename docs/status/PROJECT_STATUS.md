# Enouia Memory 项目状态快照

快照时间：2026-10-17T16:34:42+08:00。Protocol v1.0；项目 ID：`enouia.memory`。阶段：development；整体进度未知，保留 null。

实现依据：`codex/history-context-dependencies`，提交 `a8ba24c4d75842b3d057d4667622d606c2bafd7b`；这是生成快照前的实现提交，不是未来的文档提交。配套 [标准导入 JSON](project-status.json) 供 Morii 手动导入 Dashboard。

项目仍在 MV-7。本地存储、检索、审核和会话接续已有合成证据；历史初始化的批次处理、语义候选、人工质量材料、任务身份与谱系格式也已实现部分链路。云端单主是 ADR-MEM-49 已确认的目标，不再列为方向未定；云端服务、MCP 与 MV-8 仍未启动，不能把离线代码当作电脑关机后可用的服务。

本次交付补上模型读过但未引用来源的依赖：上下文来源单独记录，支持证据保持实际引文，敏感度、审核、编译和候选清除据此处理。此前五项契约回归、一项审核传播回归、双接口原生合成场景，以及独立 Schema、历史质量 Python 和接入面检查通过。Morii 最后要求停止测试、快速交付，因此最终全量 Rust 已中断，严格 Clippy 未完成，不能称全部必需检查通过。本快照仅引用保存记录，tests_rerun 为 false。

日常使用仍受真实模型质量、完整历史整理、长期独立备份恢复和 Morii 验收限制。旧资料没有自动补录依赖；其他正式派生记忆可能保留原文，本例清除后编译返回 IntentionallyPurged，尚不能称正常使用闭环恢复。另有旧 workspace 有序并发故障未解释，当前通过片段也不证明其已修复。

Runtime 本地只读 pin 记录仍为 8186aed，四十七条命令；这不证明运行状态。本次接入面已登记新字段的兼容要求，Runtime 需另行升级 Core 与契约并在宿主验证，尚未采用本次修复。真实资料范围、服务信任、自动接受规则和激活安排仍待 Morii 决定。

下一步至多三项：P0，由 Morii 指定 Claude 接手范围及是否恢复最终检查；P1，单独安排 Runtime 采用和宿主验证；P2，决定真实样本、受控调用与后续阶段。当前交付后停止，不安排后台工作或自动同步。

证据依据：[本次验证报告](../validation/MV-7-semantic-context-dependencies.md)、[前序完整验证](../validation/MV-7-analysis-lineage-shape.md)、[路线](../design/CLOUD_PRIMARY_AND_HISTORY.md)、[Runtime 兼容日志](../integration/RUNTIME.md)及[Claude 交接](../handoff/CLAUDE_MV7_HANDOFF_20261017.md)。这些证据区分代码、合成检查、实际集成和主人验收；没有真实调用、生产迁移或发布结论。
