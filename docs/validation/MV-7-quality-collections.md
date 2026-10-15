# MV-7 · 离线质量集合与身份的结构边界

日期：2026-10-15，基线 `ce93561`（PR #20）。范围见 [ADR-MEM-61](../adr/061-quality-collection-shapes.md)，仍为已授权 MV-7 的离线合成工作。

已复现：把待审响应 changes/summary 的数组换成空对象或空字符串并重新计算响应 hash，旧工具仍生成评估报告；把已支持路径换成对象，observed 单元仍被接受并保留 supported_in_review。空对象/字符串的评审集合也被当作无判断；空白或非字符串 case_id 没有明确身份门槛。原因为使用 len、迭代和真假值前未确认容器类型。CLI 的响应空对象复现退出 0，输出指标；旧实现隔离复核及最初失败日志保留于本地 mv7-collection-shapes-before.log、mv7-collection-shapes-baseline-corrected.log。

现在明确要求 suite 对象、case 对象数组、非空白字符串身份；回答/评审集合均为数组，回答项/判断/证据项为对象，证据集合为数组，所有单元状态的 paths 均为字符串数组。非法输入不能靠空值、对象键迭代或 Python 的布尔/整数兼容通过。没有自动转换数据或改变合法输入的评分含义。

最终 28 项 Python 合成回归通过，exit 0，覆盖合法校准、非法容器、身份、原生待审输入、精确引文和旧来源附件元数据。CLI 用一个合法 case 加一个格式错误 case 验证整套退出 1、不输出任何指标或 traceback。默认合成校准报告以及 Rust H2j 合成私有套件的报告均与旧版逐字段一致：原生输入 hash/选定作用域匹配、reviewer=pending、重要单元未知、native_validation=not_rerun、整体模型质量 null。日志为本地 mv7-collection-shapes-python-final.log，退出码 mv7-collection-shapes-python-exit.json；前后报告为 mv7-collection-shapes-valid-before.json、mv7-collection-shapes-valid-after.json 和 mv7-collection-shapes-cross-language-before.json、mv7-collection-shapes-cross-language.json。新增测试首次复用可变夹具导致错误，已按子例深拷贝修正，旧输出保留于 mv7-collection-shapes-test-fixture-error.log。

仓库根 Rust 1.98.1、CARGO_NET_OFFLINE=true，未设置 RUST_TEST_THREADS，cargo test --workspace --locked 共 384 项通过、0 失败，Workspace 66 项、10.01 秒；退出码 0 已保存于 mv7-collection-shapes-rust-exit.json，日志 mv7-collection-shapes-rust.log。fmt --check、严格 Clippy（mv7-collection-shapes-clippy.log、mv7-collection-shapes-clippy-exit.json）、独立 python-jsonschema 4.26.0 的 36 schemas 及 Runtime checker 均 exit 0。公开内容/链接和原状态快照 hash 检查通过，结果汇总于本地 mv7-collection-shapes-check-results.json。所有证据为合成/离线，不构成真实质量或主人验收。

没有 Rust/契约/依赖或 Runtime 接入面变更，aggregate 保持 `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`，47 commands，无新产品采用动作。native_validation 仍为 not_rerun，整体模型质量为 null，格式检查不认证原生元数据或主人判断。旧有序屏障超时仍未定位，本片不以 Python 修正说明其稳定性；状态快照未刷新，真实资料/Provider 和 MV-8 未启用。
