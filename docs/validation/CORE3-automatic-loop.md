# Core-3 自动记忆闭环验证

日期：2026-10-24，Asia/Shanghai。基线为 Core-2
`defb2530092db8149e614a2290cc991fd423ece5`，本片分支
`codex/core3-automatic-loop`。Morii 的“继续继续”授权本片；后续又明确
授权完成后继续 Core-4/5。后续切片单独提交，不混入本片结果。

## 工作区和提交范围

复用任务的隔离工作树，核实 branch/HEAD/status/worktrees。原 checkout
仍在实现基线 `9286cd8f89669d9ec3b92f943fd74072457536e7`；本树从已推送的
Core-2 向前实现。并行 Provider 树未修改。借阅的设计、交接、研究包仍保持
未跟踪；未提交 AGENTS、手动项目状态、私人原件或 `.local` 材料。

本片 owned-file allowlist：

- `crates/enouia-memory-contract/src/next/{mod,admission,rules,storage,storage_rules}.rs`
- `crates/enouia-memory-contract/tests/{next_admission,automation_storage}.rs`
- `crates/enouia-memory-host/src/automation/{mod,model,worker,application}.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-vault/src/lib.rs`
- `crates/enouia-memory-vault/src/automation/{mod,publication}.rs`
- `contracts/next/{automation-v2,storage-v2}.schema.json`
- `tests/fixtures/next/manifest.json`
- `tests/fixtures/automation-vault/{descriptor,genesis,capture,admitted,current,publication,manifest}.json`
- `tools/schema-check/check_schemas.py`、`tools/integration/runtime_surface.py`
- `docs/adr/087-automatic-memory-transactions.md`、`docs/adr/README.md`
- `docs/contracts/{CORE1_AUTOMATIC_ADMISSION,CORE2_DURABLE_JOBS,CORE3_AUTOMATIC_LOOP,CONSTRAINT_MAP}.md`
- `docs/integration/{RUNTIME.md,runtime-surface.json}`、本报告

## 实现

[ADR-MEM-87](../adr/087-automatic-memory-transactions.md)与
[正式契约](../contracts/CORE3_AUTOMATIC_LOOP.md)定义显式 V2 合成根。
复用既有根验证、owner-only ACL、ManagedRoot、Host/Writer OS 锁及
AuthorityState；没有新 crate、依赖或生产旁库。现有默认入口和 v1 语义不变。

来源保留原文、hash、修订和 native 身份；用户自述仍是 UserStatement。
System 决定保持知识状态与采用状态分离，批次事务验证 set/delta、依赖闭合、
敏感继承、目标修订、同源重复、结果/提交关联。保存原始生成请求和响应，
事务成功后才返回真实结果回执。模型无法提供 owner 权限或原生来源观察。

为自动纠正新增 rule-2 的窄策略委托：可修订同一 policy ID 下由机器创建的
目标，仍受当前策略、主体/项目、权限及精确修订约束。flag 默认 false 且不
序列化，原 rule-1 policy bytes/hash 不变。旧 next reader 拒绝新字段/rule；
不修改全局 SchemaVersion、RecordKind/AnyRecord 或旧 OWNER_ONLY。

显式 AutomationHost 将 Core-2 私有 worker 的持久生成完成事件接到合法事务。
生成不持有 Vault writer；native 控制与应用串行化，变更根前取消旧发送许可。
新工作/重评由 native 合成事件提交，worker 自动应用，无逐条 owner 审核。
最小词面召回先验证权限/当前修订，再筛选并限制数量；输出“本人说过 X”的
限定语。新的 staged 修订屏蔽旧 adopted 陈述，历史不改写。

CURRENT 选择完整 immutable 数据和 authority。发布 intent -> CURRENT ->
witness 后才能回执；不确定错误使实例不可继续写。普通 open/read 不修复。
显式同一 native primary 只能完成已验证的精确 intent，不新造操作或倒退水位。
旧 ADR-MEM-73 的恢复确认规则未改变。

## 验证与证据边界

focused 检查已执行：3 项 storage contract、16 项 admission、16 项 native
automatic_loop、18 项 Core-2 回归全部通过；focused clippy 通过。
native 的 16 项包括一个无专用环境时直接退出的子进程入口；父测试真实终止
三个发布边界的子进程，验证回收 lease、精确前进恢复和同一结果/回执。

覆盖：无 apply/审核步骤的自动发布；形成、纠正、取消后跨重启召回；
不确定/冲突暂存及新证据重新采用；批次第二项失败时全部不写；Owner 冒充、
遗漏 context、伪造锚点/目标、源事件别名拒绝；锁定/删除/策略/修订变化拦截；
等待生成时 writer 可用；旧 v1 owner/review 原字节不变及旧读者拒绝新根；
五个 fault 边界与冲突 witness；原生 replay 对改变原始响应拒绝。

离线 `cargo fmt --all -- --check`、`cargo test --workspace --locked`
及 `cargo clippy --workspace --all-targets --locked -- -D warnings` 均通过；
全 workspace 共 577 项测试通过。首次全量测试因新模块的集成清单尚未
生成而被 runtime_surface 门禁拒绝；已生成并登记后重跑通过，不修改门禁。

独立 Python 3.12 / python-jsonschema 4.26.0 cross-check 已通过：46 个 schema，
其中 18 个 next、18 个 automation、17 个 automation-store 文档正反例。
集成 surface 已生成并在 RUNTIME.md 登记，55 条 Workspace 命令不变。

这些是合成、真实本地文件/进程证据，不是模型语义质量、网络账号或产品验收。
进程终止不证明硬件掉电耐久、整盘回滚检测或独立恢复；链/状态有明确上限，
没有压缩/清理。结构规则不判断任意自然语言是否忠实、完整或已发现所有冲突。

## 采用和后续

Runtime pin/工作树未变；旧 embedded Core、MCP、Host/Workspace 路径保持旧语义。
新根必须在 descriptor/session capability gate、解码/查询之前拒绝交给旧读者。
未来采用需匹配 schema/layout/rule、native System/来源认证、生命周期、foreground
事件及 witnessed read model，不能仅在页面隐藏字段或把 adopted 标成 owner 确认。

未实现真实 Provider/embedding、持续外发策略、私人迁移/迁主、后台周期重评、
复杂巩固/淡出、完整时间轴/图谱、通知、MCP 新工具或 Runtime 原生产品采用。
Core-4 下一片先用 fake transport 验证范围、目的地、预算、撤回、重启和未知费用，
不因本片通过激活真实账号。没有 merge、部署或刷新手动项目状态。
