# Enouia Memory · 架构设计包 v1.2

> **v1.1 修订（2026-09-28，MV-0R）：** Enouia Memory 在本仓库 [Morii9961/enouia-memory](https://github.com/Morii9961/enouia-memory) 独立开发（ADR-MEM-19）。原设计中“把实现放进 Runtime 工作区”的安排已失效，MV-0 草稿已迁入本仓库并按 [独立审核](../reviews/MV0_REVIEW_AND_REPO_CORRECTION.md) 修正 F1～F6，结果见 [MV-0R 报告](../validation/MV-0R.md) 与 [ADR 登记簿](../adr/README.md)。本版只修改工程归属、数据根与阶段入口；v1.0 原文可在 Git 历史（提交 da0d1e3）中查看。下文“本轮核对范围”记录最初设计包的范围。

> **v1.2 修订（2026-10-04，ADR-MEM-45）：** Enouia Runtime 的 Windows 客户端成为 Memory 本地部分的产品客户端：它以固定 Git 修订嵌入 `enouia-memory-workspace`，只经自身适配器和 workspace IPC v1 调用。本仓库的 `apps/workspace` 保留为参考外壳与验收工具，不是产品客户端；本地部分的新产品界面先在 Runtime 落地。改动 Runtime 依赖的集成面时，须在同一提交中按 [Runtime 集成说明](../integration/RUNTIME.md) 登记 Runtime 须采纳的内容。领域 crate、契约、数据规则、CLI 与全部云端阶段（MV-7～MV-11）仍在本仓库。本版补充宿主归属、集成面登记规则和两种 IPC 形状的区分，其余设计不变。

设计日期：2026-09-28。设计负责人：Enouia（本轮由 Codex 完成）；后续实现交接对象：用户指定的 Claude Opus 5.5。

**本设计把 Memory Vault 放到 Runtime 的首要基础设施位置：先保存可恢复的历史，再建立经审核的记忆，最后接入 Windows、模型、MCP 与 VPS。** 本目录只包含设计文档；没有实现、真实历史导入、部署或数据上传。

这里的 v1.0 是文档版本，不代表软件已经发布，也不改写 Runtime Architecture v0.3、Memory schema v1 或 ActivityData v1 的版本含义。

## 阅读路径

| 文档 | 回答的问题 | 主要读者 |
|---|---|---|
| [PRODUCT_VISION](PRODUCT_VISION.md) | Enouia 要保住什么，什么才算完成 | Morii、实现者 |
| [MEMORY_ARCHITECTURE](MEMORY_ARCHITECTURE.md) | 模块、真相源、进程、目录与事务如何组成整体 | 架构与存储实现者 |
| [DATA_MODEL](DATA_MODEL.md) | 记忆、来源、时间、版本、候选和会话的精确语义 | 契约实现者 |
| [IMPORT_REVIEW](IMPORT_REVIEW.md) | 如何无损接收历史、提取候选并安全确认 | 导入与审核实现者 |
| [CONTEXT_MODEL](CONTEXT_MODEL.md) | 如何找对记忆、控制外发、生成可解释上下文 | 检索与模型实现者 |
| [INTERFACES](INTERFACES.md) | Windows、Provider、MCP、VPS 如何联动 | 客户端与服务实现者 |
| [PRIVACY_RECOVERY](PRIVACY_RECOVERY.md) | 权限、加密、备份、删除、故障与恢复 | 安全与运维实现者 |
| [IMPLEMENTATION_PLAN](IMPLEMENTATION_PLAN.md) | 按什么顺序实现，每一步交付什么 | Claude Opus 5.5、Morii |
| [ACCEPTANCE](ACCEPTANCE.md) | 如何证明完成，而非仅仅可以演示 | 测试与验收者 |
| [DECISIONS_AND_SOURCES](DECISIONS_AND_SOURCES.md) | 哪些选择已作出，与旧文档哪里不同，依据是什么 | 所有维护者 |
| [CLAUDE_HANDOFF](CLAUDE_HANDOFF.md) | 下一轮交给 Claude 的完整执行入口 | Claude Opus 5.5 |

Morii 可以先读愿景、总架构和实施计划。Claude 在实现前须读完整设计包及目标仓库当时的 AGENTS.md；从 MV-0 开始，不直接进入后续阶段。

## 已作出的关键选择

1. 本地 Vault 是 Memory 域权威源；Activity 保持独立数据根、调度和公开契约。
2. 原始字节、已审核记录、操作历史、会话均保存为可恢复文件；SQLite 和向量仅作可重建索引。
3. Canonical 使用版本化 JSON，Identity 使用 Markdown；人工可读不等于所有文件都可绕过审核直接编辑。
4. 单写者、不可变修订、事务清单及一个 CURRENT 指针共同提交，避免“几次单文件替换”冒充跨文件事务。
5. 模型和外部 Agent 只能提出候选；正式提交必须有可验证的用户确认。
6. 优先级、敏感度、事实时效性独立；旧事实可以正确描述过去，却不足以证明今天的状态。
7. 先离线 Mock 闭环，再 Windows 最小记忆界面，再真实 Provider / MCP；完整桌面 Companion 随后扩展。v1.2：Windows 界面由 Runtime 客户端承载（嵌入固定修订的 `enouia-memory-workspace`），本仓库 `apps/workspace` 为参考外壳。
8. VPS 先做可替换的网关和持久队列；加密备份、加密设备副本、可供服务器计算的有限副本是三种不同能力。

## 本轮核对范围

已完整阅读用户提供的原始 v0.1 整理稿（含个人实例，本地保存于 `docs/history/private/`，不入公开仓库；哈希见 DECISIONS_AND_SOURCES）和启动提示词，并只读核对 Runtime 仓库（本地 checkout） 的 v0.3 架构、M0 契约、实施计划、ADR、Cargo workspace 与工作树状态。原始整理稿保留不动。现有 Runtime 仓库未修改。

设计包内的 MUST / 必须为实现约束，目标值为未来验收要求，所有“默认”均是本设计选择；没有把它们描述成已经实现或用户已经执行过的设置。启动提示词中的实施命令、模型示例和项目故事是需求材料，不构成本轮编码、访问私人历史或上线授权。

文档级检查见 [ACCEPTANCE 的本轮检查记录](ACCEPTANCE.md#本轮文档检查记录)。后续技术实现只有通过对应阶段验收后，才可被称为已完成。
