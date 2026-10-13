# MV-7 历史初始化续做交接

记录日期：2026-10-12。实现基线为 main `939f82620821fe91662e4953a1896630e1361a66`（PR #12 合并）。本次只保存交接，没有新增实现；继续工作仍限于 MV-7，不启动 MV-8、真实 Provider 或个人历史导入。

## 已合并能力与证据

H1 规划和 H2a 来源绑定/批次恢复、H2b 确切主题对比/替代草案、H2c 来源导航、H2d 语义审核任务均已实现。H2e 的离线人工质量工具绑定来源与回答 JSON hash，分别统计支持、反证、漏提和未知。相关入口：

- [当前路线](../design/CLOUD_PRIMARY_AND_HISTORY.md)、[实施计划](../design/IMPLEMENTATION_PLAN.md)。
- [H2d 合成验证](../validation/MV-7-semantic-review.md)：语义任务复用普通 source-linked Session 和 Provider 账本；没有 ExtractionJob，也不发布候选。
- [H2e 验证](../validation/MV-7-quality-assessment.md)：9 项 Python 测试、逐例运行的 371 项 Rust 测试、fmt、严格 Clippy、独立 schema 交叉检查和 Runtime checker 通过。默认并行全量曾因既有暂停调用测试的调度前提失败；单项及逐例复测通过，默认并行稳定性仍未解决。
- [Runtime 接入记录](../integration/RUNTIME.md)：47 条命令；aggregate `0d48e30dc5645d1cd7b9e5a190788d932dff9d86c9603a251274de184c76c329`。H2 原生可选 helper 的产品接入和验收仍 pending。

上述功能证据来自既有实现报告。2026-10-13 收尾核对：交接期间逐例全量 Rust 日志记录 371 项通过、0 失败；fmt、严格 Clippy、独立 schema 交叉检查和 Runtime checker 再次通过，5 个本地链接及公开内容检查通过，状态快照 hash 未变。合成响应与评分校准不证明实际模型语义质量、个人资料可用性或阶段验收完成。

## 接下来继续的工作

2026-10-13 接续：下列原生草案要求已在 H2f 实现，见 [报告](../validation/MV-7-semantic-proposal-drafts.md)。草案经既有 head-bound 路径发布 pending，模型归属与引文保留在 reason 中，正式确认仍独立。以下内容保留为原始领取要求；产品宿主采用、实际质量和主人验收仍未完成。

优先处理语义审核结果到待审提案的显式交接。当前只有已保存响应的只读 `history_semantic::preview`；不要把以下待实现要求写成已完成功能。

1. 由主人明确选择 task、已完成响应和单条解释，重新读取并验证来源、响应 hash、权限和当前 head；先形成可检查的草案，再走既有待审提案及 plan/confirm。草案不批准正式记忆，不发送新请求。
2. 保留 task/input/dispatch/response 绑定、模型解释和原文引用。语义任务没有 ExtractionJob，不能借用前序抽取 run ID 标成自己的 ModelExtraction。`OwnerManual` 表示主人提交提案，不能据此把模型解释或 assistant 原文提升为用户事实；引用的 EvidenceClass 必须保持。具体持久化归属尚未选定，需要先核对 Candidate/Source/Origin 契约，避免丢失模型贡献及删除依赖。
3. 新信息、纠正、取消、冲突和不确定分别处理。目标记忆及其当前修订由主人选择；BusinessTime 显式提供，不从消息时间猜测。冲突、未知顺序或跨分支差异不自动变为替代关系。

实现时至少验证：伪造/过期选择、来源修订或删除、无权限/锁定、重复键重放、模型证据归属、未知时间、目标修订冲突，以及提案仅进入 pending。重用原生现有发送与治理边界，不增加自动批准路径。新产品 UI 由 Runtime 实现；本仓库保持 transport-neutral。若修改 Provider/Core 接入面，同一提交须重新生成 surface 并记录 Runtime 采用要求。

实际模型忠实性、重要信息漏提、规模/费用、双 Provider 实测、Runtime 产品采用、独立恢复和主人验收仍是后续门槛。真实凭据、资料、HTTP 和部署需要各自明确授权。

## 工作区与提交约束

开始时核对实际 HEAD、remote 和工作区，不把这份基线当作永久最新。保留其他会话的 `AGENTS.md` 修改与未跟踪 `docs/status/`；本交接不刷新状态快照。只提交自己的文件，推送前检查个人路径、私密材料、credential 形状及本地链接。实现恢复后执行仓库要求的完整验证，记录默认并行失败或隔离条件，不能隐去失败。提交使用仓库要求的 Codex co-author trailer。

共享限额临近耗尽时，先收尾、提交并推送，再检查仍在运行的相邻会话。关机不得中断尚未完成的会话；其完成状态应在执行关机前重新读取。
