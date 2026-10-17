# MV-7 · 完整分析输入的谱系格式一致性

日期：2026-10-16；2026-10-17 收尾。基线 `ecf5a94`（PR #31）；分支 codex/history-analysis-lineage-shape。范围见 [ADR-MEM-64](../adr/064-analysis-input-lineage-shape.md)。

## 复现与修正

既有矩阵检查会拒绝非法 selected_lineage，但普通 evaluator 只验证完整输入的 hash、基础形状和选定作用域，没有调用谱系行检查。自洽地更新输入文本 hash 后，null、空数组、对象、布尔值或非法行仍能得到 included_hash_and_scope_matched。该状态原本仅声明 hash/作用域一致，并非父链认证；本片统一两个入口的格式门槛。

把既有矩阵 lineage_shape 移入 evaluator 并共用，普通输入存在该字段时调用相同检查。要求行数组完整、字段精确、来源索引覆盖唯一、父索引范围与唯一性、未选父数量为非负整数或 null，拒绝布尔替代。缺失字段的旧/无谱系输入仍可用，null 的未知父数量不变，不推断边或补读原文。矩阵继续使用同一 helper，没有新增原生执行/身份/谱系事实认证。

## 合成验证

两项新增、14 个失败子例在原版明确复现；修正后完整 Python 为 65 项且通过。覆盖非法顶层谱系、行索引/父索引/未知计数/额外字段/重复行，以及合法未知和缺失字段不改写。evaluate.py、review.py、compare_reviews.py 遇到第二例非法谱系时均 exit 1、stdout 为空、无 traceback，不能输出前例的部分材料或指标。

默认/两组扩展校准、八份原生合成材料及既有 Unicode/部分评审材料，共 13 套、29 个案例的完整报告 JSON 值与基线逐一按规范 digest 比较相同，CLI UTF-8 输出与 API 一致；八份原生矩阵检查仍通过。

固定离线工具链的完整必需检查全部 exit 0：fmt、默认并行 Rust 388 项、严格 Clippy、Python 65 项、独立 JSON Schema 36 项及 Runtime surface。公开内容检查覆盖八个文件、92 个本地链接、路径/凭据形状与 diff 空白，两份状态快照 hash 不变。10 月 17 日收尾沿用已保存的同版完成日志和明确退出码，未无目标重跑；日志、兼容结果、单独退出码和汇总存于忽略的 mv7-analysis-lineage-* 文件。

## 边界

只收紧离线格式，生产 Rust、依赖、契约和 Runtime surface 不变，无新宿主采用动作。native_validation 仍为 not_rerun、overall_model_quality 为 null；形状合格不证明边、授权、真实模型质量或主人验收。未读取真实资料/凭据、调用 Provider、激活 Runtime/MV-8、部署或刷新状态快照。
