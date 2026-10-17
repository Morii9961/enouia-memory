# MV-7 · 合成组合的谱系结构检查

日期：2026-10-15；基线 `c251125`（PR #22），仅修正离线合成检查器，不改变原生任务/父链语义。

已复现：有谱系的组合只检查 selected_lineage 字段存在。将其换成 null、空字符串/对象/数组，或填入布尔索引、越界/重复父索引、负数未选父数量，重新计算输入与清单哈希并保持旧/完整套件配对后，原检查仍通过。修正前 13 个合成子例失败输出保留于本地 mv7-lineage-shape-before.log。

现在要求与原生 SelectedLineage 一致的行字段和数组：覆盖全部所选来源且索引唯一，父索引为范围内的唯一整数，未选父数量为非负整数或 null。不自动转换布尔/整数或解释父边事实；未知/未选父数量保留，来源顺序不推断时间或祖先关系。

39 项 Python（原有 36 + 新增 3）合成回归通过，含 rehash 后的非法结构和 null/0/正数的合法未选父声明。此前同版本 Rust 生成的八份合成产物继续通过新检查，输出仍为 review_required、native_validation=not_rerun、整体模型质量 null。仓库根 pinned/offline/default parallel Rust 全量 384 项通过，exit 0 已保存；fmt、严格 Clippy、独立 python-jsonschema 4.26.0 的 36 schemas、Runtime checker 及公开内容/链接/状态快照 hash 检查通过。日志和退出码为本地 mv7-lineage-shape-python.log、mv7-lineage-shape-python-exit.json、mv7-lineage-shape-rust.log、mv7-lineage-shape-rust-exit.json、mv7-lineage-shape-clippy.log、mv7-lineage-shape-clippy-exit.json；跨语言结果为 mv7-lineage-shape-cross.json。全部为合成证据。

改动限检查器、合成回归和文档，生产 Rust、契约/依赖和接入面未变；aggregate 为 `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`，47 commands。格式合格不认证父链、模型质量或主人判断；原有屏障故障仍未定位。状态快照未刷新，真实资料/Provider、Runtime 激活和 MV-8 未启动。
