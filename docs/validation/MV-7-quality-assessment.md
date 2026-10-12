# MV-7-H2e · 离线人工质量评估

日期：2026-10-12；main 基线 `00afd8a`。范围：[ADR-MEM-54](../adr/054-history-quality-assessment-harness.md)。继续 MV-7，不启动 MV-8。

新增标准库 Python 工具，从显式人工判定统计陈述、解释和摘要的支持/反证/无支持/不确定，以及关键单元的出现/漏提/未知。来源选择和响应值分别用 canonical JSON SHA-256 绑定；变更会拒绝旧判定。正判路径才能支持“单元已出现”，未判项保留 unknown/unreviewed。元数据中的原生 task hash 另列，工具不认证提交者、不重跑 native validator，不产出总体模型质量百分比或批准记忆。

合成校准有三种结果：保留“取消地图、未采纳导航”的正确回答为 supported_in_review；引用仍精确但把“不采纳”反转成“采纳”的回答得到一项 contradicted、两项 missing；空回答未评审时保留两个 unknown，已判支持比例为 null。它们校准计算流程，不证明实际模型通过。退出码 0 只表示报告已生成。

九项标准库测试通过：三种校准状态、变更回答/来源拒绝旧 hash、反证路径不能算正覆盖、坏/重复路径规则、坏审核引用、未判单元保留 unknown、来源路径越界/无绑定 owner 样本拒绝、JSON 重复键拒绝。生成的合成报告保存于忽略的本地验证区域，没有公开私人样本。使用步骤见 [README](../../tools/history-quality/README.md)。

Python 3.12.10 标准库九项测试退出码 0，合成报告生成成功。Rust 1.98.1、GNU Windows、离线格式检查与严格 Clippy 退出码 0。完整 `cargo test --workspace --locked` 在本次进程设置 `RUST_TEST_THREADS=1` 后为 371 项通过、0 失败；这隔离不同测试用例的负载，用例内部自己的并发线程仍保留。

默认并行的一次全量运行未通过：既有 `session_ask_replay_refuses_evidence_deleted_while_the_call_is_paused` 的调度前提未在五秒内满足（第一条调用须先完成，第二条才恢复）。未改 Rust 实现或放宽超时；该项单独复测 0.65 秒通过，逐例全量复测亦通过。失败日志保留在忽略的本地验证区；默认并行稳定性不因这次隔离通过而视为已解决。重复键 Python 测试最初受沙盒临时目录写入限制，最终用内存字节输入测试解析规则，不写外部临时文件。

独立 python-jsonschema 4.26.0 校验 OK：36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases。Runtime checker 通过，`git diff --check` 通过；10 个本片公开文件和 74 个本地文档链接核对通过，所查个人绝对路径/credential 形状未命中，两份状态快照 SHA-256 未变。未安装依赖。

本片不改 Rust 实现或 Runtime surface；集成 aggregate 仍为 `0d48e30dc5645d1cd7b9e5a190788d932dff9d86c9603a251274de184c76c329`，47 条命令不变，无 Runtime 采用动作。没有真实资料、模型外发、凭据、后台跟踪、部署或状态快照更新。实际质量、真实双 Provider、Runtime 采用、重要信息漏提、规模/费用、独立恢复和主人验收仍 pending。
