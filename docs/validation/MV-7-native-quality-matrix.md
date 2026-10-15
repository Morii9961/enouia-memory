# MV-7 · 原生质量材料的八种合成组合

日期：2026-10-15；基线 `9b25145`（PR #21）。沿用 ADR-MEM-57/59 的私有评审材料边界，仅补充合成验证覆盖。

检查确认原生 scope/response 的数值字段均为整数，未发现浮点哈希兼容问题。既有 H2j 导出钩子只保留 OpenAI + 谱系 + 完整输入的一份套件，离线检查未逐一覆盖两家 Provider 的四种任务/builder 组合。新增 opt-in 测试钩子导出八份：OpenAI/Anthropic × 无/有谱系 × 旧/完整 builder，另写带整套哈希的测试清单。原钩子保持兼容；生产 builder、协议与评分不变。

只有显式 ENOU_TEST_QUALITY_MATRIX 才写入忽略的本地 mv7-quality-native-matrix 目录。开始时置空 manifest，四个任务全程通过后才发布完整八项清单。使用现有 Fake Provider/合成临时 Vault，不读取个人历史、调用真实模型或写正式记忆。

标准库 Python 检查器读取明确目录和固定文件名，验证组合完整/唯一、Rust 套件哈希与 Python JSON 值哈希相同、读取前后套件一致、删除完整输入后与旧材料的规范 JSON 哈希相同、任务 Provider 绑定与清单名称相符、谱系模式与任务 hash 分离，并要求待审/未知状态。不会认证清单提交者或重新查 Vault；native_validation=not_rerun、整体模型质量 null。

原生定向用例与最终全量重新生成的八份 Rust 产物全部通过 Python 检查；36 项 Python（原有 28 + 新增 8）通过。新增回归拒绝缺项/重复、改 hash、逐份可评估但旧/新配对不一致、重算 hash 后的谱系隐藏或 Provider 名称错配、路径/布尔类型替代，以及 Python 普通相等把 1 与 true 混同的边界。类型/Provider 回归的修正前失败输出保留于本地 mv7-native-matrix-type-before.log、mv7-native-matrix-provider-before.log。

仓库根 Rust 1.98.1、CARGO_NET_OFFLINE=true、默认并行 cargo test --workspace --locked 384 项通过、0 失败，exit 0 独立保存于 mv7-native-matrix-rust-exit.json（日志 mv7-native-matrix-rust.log）。fmt --check、严格 Clippy（mv7-native-matrix-clippy.log、mv7-native-matrix-clippy-exit.json）、独立 python-jsonschema 4.26.0（36 schemas）及 Runtime checker 均 exit 0。最终 Python 日志/退出码为 mv7-native-matrix-python-final.log、mv7-native-matrix-python-exit.json，真实 Rust 合成产物跨语言结果为 mv7-native-matrix-cross-final.json、mv7-native-matrix-cross-exit.json。公开代码/链接、产物忽略及原状态快照 hash 检查通过，汇总为本地 mv7-native-matrix-check-results.json。

改动限 Rust 测试夹具、Python 检查器及文档，无生产 Core/Provider、wire/storage 契约、依赖或 Runtime 接入面变更；aggregate 仍为 `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`，47 commands。原有屏障超时仍未定位，本片的组合检查不作稳定性或实际模型质量结论。状态快照未刷新，真实资料/Provider、Runtime 激活、MV-8 与主人验收保持原有边界。
