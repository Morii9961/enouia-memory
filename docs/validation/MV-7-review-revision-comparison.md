# MV-7 · 人工评审修订的只读核对

日期：2026-10-16。基线 `483f765`（PR #28）；分支 codex/history-quality-review-delta。按 [ADR-MEM-63](../adr/063-offline-review-revision-comparison.md) 继续当前离线质量路线。

## 行为与边界

compare_reviews.py 显式接收 --before 和 --after，各套件沿用 512 KiB 输入上限与 evaluator 的完整验证；比较快照须与已验证 suite hash 一致。以唯一 case_id 匹配案例，只允许 reviewer、item_judgments 和 unit_judgments 改动，其余案例字段及套件元数据按规范 JSON digest 比较。来源、回答、关键单元或原生声明改动，即使各自 hash 已更新且两份独立合法，也拒绝比较。1 与 true 等 JSON 类型不混同。

以回答路径及固定关键单元位置报告新增、修改、撤回。案例或判断重排不产生伪修改；before/after 和单元位置均从 0 开始。输出只有 hash、位置、路径、声明和前后计数，不复写案例名、单元名称、来源、回答、引文、理由或分析输入。元数据仍不保证匿名或可公开。完整比较后才输出 UTF-8 JSON，报告上限 128 KiB，任何错误时 stdout 为空。不会保存、合并、回写、导入 Markdown、代判或批准；撤回判断不自动解释为质量提升。

## 合成验证

七项新增检查使 Python 总数为 61：部分评审及未知/原文不复写；判断编辑与撤回；更新 hash 后仍拒绝来源/回答/关键单元/原生声明及元数据改动；案例和两种判断重排；读取间快照变更；受限编码 CLI 及后例非法时整份拒绝；最大 32 个案例、每例 40 个回答路径和 32 个单元的报告仍在预算内。所有材料均为合成声明，supported 预设不证明语义理解。

实际用既有 Rust 假 Anthropic/谱系/完整输入的 pending 套件，与上一片固定 synthetic_fixture 练习副本比较，GBK 标准输出环境中 CLI exit 0，报告为 2071 字节：新增 /changes/0/statement 和单元 0；两个未审、一个未知保持，状态仍为 review_required。两份输入逐字节不变，native_validation=not_rerun、overall_model_quality=null。结果存于忽略的 mv7-review-delta-native* 文件；未重新生成或修改原生材料。

固定离线工具链的完整必需检查全部 exit 0：fmt、默认并行 Rust 387 项、严格 Clippy、Python 61 项、独立 JSON Schema 36 项和 Runtime surface。公开内容检查覆盖本片七个文件、83 个本地链接、路径/凭据形状及 diff 空白，两份状态快照 hash 不变。日志、单独退出码与汇总为忽略的 mv7-review-delta-* 文件。

## 集成与未完成门槛

生产 Rust、依赖、契约及 Runtime surface 未改，47 commands / aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 保持，无新增宿主采用动作。快照核对不是文件系统事务，也不认证评审者、原生来源或谱系。真实质量、漏提、全量尺度与主人验收仍 pending。未接触真实资料/Provider、Runtime 激活、MV-8 或手动状态快照。
