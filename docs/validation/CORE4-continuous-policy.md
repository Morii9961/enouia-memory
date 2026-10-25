# Core-4 持续生成策略验证

日期：2026-10-24。基线 Core-3 `564fc7d5f493404e1a3746e7e2ff744a4bd2ffb2`。
Morii 明确授权完成 Core-3 后继续 Core-4/5 及后续；本片分支
`codex/core4-continuous-policy`。复用隔离工作树，不修改原 checkout、并行
Provider/Runtime 树、借阅设计/研究文档、AGENTS 或手动项目状态。

## owned-file allowlist

- `crates/enouia-memory-contract/src/next/{mod,continuation}.rs`
- `crates/enouia-memory-contract/tests/continuation.rs`
- `crates/enouia-memory-host/src/automation/{mod,worker,application,continuation}.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-host/tests/automatic_loop/continuation_cases.rs`
- `contracts/next/continuation-v2.schema.json`
- `tests/fixtures/continuation/{policy,journal,unknown,manifest}.json`
- `tools/schema-check/check_schemas.py`
- `docs/adr/088-continuous-generation-policy.md`、`docs/adr/README.md`
- `docs/contracts/CORE4_CONTINUOUS_POLICY.md`、`docs/contracts/CONSTRAINT_MAP.md`
- `docs/integration/{RUNTIME.md,runtime-surface.json}`、本报告

## 实现及结果

[ADR-MEM-88](../adr/088-continuous-generation-policy.md)与
[正式契约](../contracts/CORE4_CONTINUOUS_POLICY.md)冻结独立持续外发策略。
Owner 只做有范围的 native 策略设置，之后 System 分析不需要逐请求点击。
目的固定为 AdmissionInput 记忆分析；目的地 API/model/adapter、Vault、
准入策略 revision/hash、主体/项目、来源、producer、敏感上限、时限、
token/cost 单次与总额均须匹配。高度敏感拒绝，无凭据字段。

显式作业根使用 managed 原子 journal 和独立 lease，保存完整策略历史与
content-free 尝试。发送前持久预留；未知/失败不退款、不盲目重发，已知超额
按更大值计入，策略续订或重启不清零。每次至多一个在途，重开默认锁定。
Core-2 ready 门禁默认兼容；新策略不可用时作业保留原输入并自动暂停发送。
仅证明尚未交接的 Paused 可以撤销 Core-2 的本地未发送预留；交接后 Paused
转为 Unknown。持久化未知使控制器拒绝继续发送。

撤回/锁定先取消关联 jobs，再关闭 native 策略 epoch。Core-3 应用持有同一
策略读屏障至事务提交，撤回使用写屏障；已生成响应不能在后来撤回后补写。
新 fake 适配器明确拒绝真实 API 执行，既有 V1 private 外发确认逻辑未改。

实际 focused 检查：3 项契约测试通过，17 个 schema/native 正反例；
12 项 native continuation 测试通过，其中含一个专用子进程入口。覆盖：
缺策略零发送、授予后自动形成、范围/模型不符暂停、在途及生成后撤回、
共享总额度、续订不清零、到期/锁定、unknown/overrun 跨重启保留、
交接前后 Paused 区分、lease/版本/身份拒绝、预留前后持久化故障，及真实
进程终止后 unknown 保留且不重发。Core-2 的 18 项回归与 Core-3 原有
16 项闭环也通过。focused clippy 修正一处可折叠 if 后通过。

离线 `cargo fmt --all -- --check`、`cargo test --workspace --locked`
及 `cargo clippy --workspace --all-targets --locked -- -D warnings` 通过；
全 workspace 592 项测试通过。独立 Python 3.12 / jsonschema 4.26.0
已通过：47 个 schema，新增 continuation 17 项正反例。runtime-surface
已生成并登记；55 条 Workspace 命令未改。

## 采用、局限与后续

仅在合成临时根使用 fake transport，未读私人 Vault/导出、账号或凭据，
未建立生产旁库。没有新依赖、V1 schema/审批/Provider 行为变化、新 MCP 工具、
Runtime pin/工作树修改、迁主、merge 或部署。

生产采用还必须把已认证 native controller 绑定至对应 AutomationHost/jobs，
提供真实目的地/凭据、受限传输、价格与 tokenizer、生命周期及 foreground。
本片不激活真实策略，也不证明远端计费、取消、实际 token 限额或模型质量。
整盘回滚、独立恢复、策略导入/迁移未实现；不能删 journal 来重置额度。
Core-5 将在另一片扩展多维形成、时间/变化与有来源的巩固，不改变这些外发边界。
