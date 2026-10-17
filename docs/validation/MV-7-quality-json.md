# MV-7-H2k · 离线质量输入的严格 JSON 与结构边界

日期：2026-10-14；起点 `121b1df`（PR #18 的 H2j 功能分支）。范围见 [ADR-MEM-60](../adr/060-bounded-quality-json-reader.md)，仍在 MV-7，只修改离线工具与文档。

已复现：默认 JSON reader 接受 NaN 和浮点溢出到 Infinity；在输入 hash/作用域一致时，含非标准数字的任务仍被标为 included_hash_and_scope_matched。4096 层嵌入任务触发 RecursionError，CLI 输出 traceback。旧来源数组中的标量会话、数组 mapping 触发 AttributeError，同样未进入普通错误路径。

现在预先扫描带字符串/转义语义的文本，容器深度最多 128；拒绝非有限数字、保留重复键拒绝与既有字节预算。有限数字、普通文本、null 和 UTF-8/16/32 输入保持兼容。选定旧来源须满足会话对象数组、mapping 字典、消息/content 对象及 parts 数组的读取结构；合法附件元数据保留在来源 hash 内，不解释或下载。

23 项 Python 合成检查通过，含修复前失败的数字/深度/来源结构回归。两种 CLI 故障复现现在均退出 1、标准输出为空、无 traceback；合法 Rust H2j 套件经严格 reader 后仍为 review_required，输入 hash/作用域一致、重要单元未知、native_validation=not_rerun、整体模型质量 null。一个沙箱临时目录权限失败在固定获准环境中复核通过，没有改变测试或系统权限。

fmt --check、严格 Clippy、独立 python-jsonschema 4.26.0（36 schemas）和 Runtime surface checker 已通过。首次全量日志 51 个结果摘要、382 项均通过，但后台会话结束状态已无法读取；未把该次 shell 退出码补猜为 0。保存退出码的默认并行确认运行 exit 101：三个 Context input/fork/checkpoint 夹具对可重试 WriterBusy 直接 unwrap，full_session_ask 的首调用完成时限也失败（Workspace 60 通过、4 失败）。本片未改 Rust，不能把该间歇性故障称已修复，失败日志保留。

最终串行复核：仓库根，Rust 1.98.1、CARGO_NET_OFFLINE=true、RUST_TEST_THREADS=1，完整 cargo test --workspace --locked 共 382 项通过、0 失败；已保存退出码 0。23 项 Python、合法 Rust 套件跨语言复核及 6 个公开文件/54 个本地链接检查通过。状态快照 hash 不变。结果为忽略的本地 mv7-h2k-check-results.json，串行全量日志为 mv7-h2k-workspace-tests-serial.log，退出码为 mv7-h2k-rust-serial-exit.json；串行证据不替代默认并行稳定性。

Rust/契约/依赖及接入面无变更，aggregate 仍为 `deb63c9f1796f776a9fb7657243719317f99b87dbe01b3f01a2654fc9410a371`，无须新 Runtime 原生采用。

修复前后及全量证据在忽略的本地 mv7-quality-json-before.log、mv7-quality-deep-cli-before.log、mv7-quality-source-shape-before.log、mv7-h2k-python-final.log 与 mv7-h2k-workspace-tests-confirmed.log；退出码另存 mv7-h2k-rust-exit.json。真实模型质量、实际历史与主人验收仍待单独授权和验证；状态快照未刷新，MV-8 未开始。
