# MV-7-H2j · 私有待审材料中的完整分析输入

日期：2026-10-14；实现起点 `a8c09b9`（H2i 功能分支）。范围见 [ADR-MEM-59](../adr/059-quality-bundle-analysis-input.md)，仍在 MV-7。

H2i 的模型输入包含谱系信息，原质量材料只有任务 hash，审核时还需另取完整输入。本片新增可选只读 history_quality::build_with_analysis_input，在已完成响应及来源复核后，从同一 pin 返回保存的精确 UTF-8 任务文本及 hash。旧 build 套件字节不变；既有 64 KiB 任务和 256 KiB 套件预算继续约束完整结果。生产 helper 只返回内存材料；宿主另行处理明确的保存/导出。

离线工具拒绝输入 hash 改变、重复 JSON 键、形状错误、超 UTF-8 字节预算及输入与选定来源/片段/文本/角色/上下文不一致。原文只出现在私有套件，指标报告仅增加 analysis_input_inspection：新材料为 included_hash_and_scope_matched，旧材料为 not_included。检查不认证原生声明或主人身份、不查 Vault、不重跑父链/响应验证。所有判断仍为空，未审陈述与未知单元保留，overall_model_quality 为 null。

两家 Provider 的新旧任务合成检查已通过，覆盖精确输入、谱系保留、旧 builder 字节一致、重复读取无写入、伪造 task hash 和锁定拒绝。Rust 生成的合成谱系套件已由 Python 读取：输入 hash/作用域一致，review_required、native_validation=not_rerun，两个重要单元仍 unknown，不构成质量通过。

最终检查：仓库根，Rust 1.98.1、CARGO_NET_OFFLINE=true、RUST_TEST_THREADS 未设置，cargo test --workspace --locked 默认并行共 382 项通过、0 失败、退出码 0，含 78 项 Provider、64 项 Workspace。fmt --check、Clippy -D warnings、Python 3.12 / 16 项测试、独立 python-jsonschema 4.26.0（36 schemas）与 Runtime checker 均 exit 0。12 个公开文件及 99 个本地链接检查通过，状态快照 hash 未变。结果为忽略的本地 mv7-h2j-check-results.json，全量日志为 mv7-h2j-workspace-tests-default-parallel.log。该证据是本轮合成运行，不证明长期并行稳定性。

额外 H2i 证据：本片改动前构建的 `a8c09b9`，在获准环境取消 RUST_TEST_THREADS 后，默认并行全仓库 381 项通过、退出码 0；单独默认并行 Workspace 64 项也通过。首次沙箱运行因临时根 canonicalize 的 AccessDenied 失败，未放宽测试。旧间歇性并发失败本轮未复现；一次通过不能证明长期稳定性，也不宣称已修复。日志为忽略的本地 mv7-h2i-workspace-tests-default-parallel.log / mv7-parallel-workspace-before-unsandboxed.log，结果为 mv7-h2i-default-parallel-results.json；这不是 H2j 的最终检查。

Runtime 接入面已重新生成并记录采用要求，仍为 47 条命令，无新增 wire/storage schema 或依赖。真实资料、实际模型忠实性/漏提、Runtime 产品采用和主人验收仍 pending；状态快照未刷新，MV-8 未开启。
