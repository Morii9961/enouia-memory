# MV-7-H2h · 原生来源绑定的待审质量材料

日期：2026-10-13；main 基线 `6f6b0c4`。范围见 [ADR-MEM-57](../adr/057-native-quality-review-bundles.md)，仍在 MV-7。

新增只读 history_quality::build，复核主人选定的已完成语义任务后，在内存中返回离线工具可读的单 case 套件，保留 Source 修订、片段、原文、角色、时间/分支与未引用覆盖。紧凑 JSON 值 hash 与原生 task/响应原文 hash 分开。关键单元由主人预先指定，reviewer 为 pending，判断数组为空；不写文件、不调用模型、不发布或批准记忆。

Python 工具支持 native_selected_spans_v1 内联片段，拒绝同时指定文件、坏范围/hash 和重复片段。pending 不能携带判断，判断只能引用所选原文。离线工具不访问 Vault，也不认证提交者/原生声明，native_validation 保持 not_rerun，整体模型质量为 null。

合成 Rust 材料通过 Python 读取并核对两种 JSON 值 hash：3 个陈述/解释/摘要项目保持 unreviewed，两个重要单元保持 unknown，支持比例 null，review_required；不构成质量通过。另有空回答保留来源覆盖与 pending-owner 声明的合成检查。测试输出只写忽略的本地验证区，生产 helper 不导出。

完整实现检查待记录；前片默认并行 Workspace 写锁/调度稳定性仍未解决，本片不改 Workspace 或放宽超时。真实模型、个人历史、实际忠实性/漏提、规模/费用、Runtime 产品采用和主人验收仍 pending。状态快照未刷新。
