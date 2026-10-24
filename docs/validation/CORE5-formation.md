# Core-5 多维形成、变化与巩固验证

日期：2026-10-24。基线 Core-4 `d6e1c55a1c0c5fcf17b858efd9198688e24f782d`。
分支 `codex/core5-dimensional-formation`。Morii 已明确授权继续 Core-4/5
及后续路线；本片只提交 Core-5 的正式契约、原生合成实现与证据。

复用隔离工作树，保留原 checkout、并行 Provider 树、Claude Runtime 树、
借阅文档/研究包、AGENTS 和手动状态材料。本片没有真实账号、私人源或外部脚本。

## owned-file allowlist

- `crates/enouia-memory-contract/src/next/{mod,admission,rules,storage,storage_rules,formation}.rs`
- `crates/enouia-memory-contract/tests/{next_admission,formation}.rs`
- `crates/enouia-memory-vault/src/automation/mod.rs`
- `crates/enouia-memory-host/src/automation/application.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-host/tests/automatic_loop/formation_cases.rs`
- `contracts/next/{automation-v2,storage-v2,formation-v2}.schema.json`
- `tests/fixtures/formation/{policy,dimensions,manifest,key-units,scripted-output}.json`
- `tools/schema-check/check_schemas.py`
- `docs/adr/089-dimensional-formation.md`、`docs/adr/README.md`
- `docs/contracts/{CORE5_FORMATION,CORE3_AUTOMATIC_LOOP,CONSTRAINT_MAP}.md`
- `docs/integration/{RUNTIME.md,runtime-surface.json}`、本报告

## 具体变更

[ADR-MEM-89](../adr/089-dimensional-formation.md)和
[契约](../contracts/CORE5_FORMATION.md)先冻结新 layout/rule 能力。只有显式
空合成根的 `automation-host-v2` + rule 3 可写正式维度。HostV1 保持原语义，
拒绝 rule-3/维度写入；旧 Core-3/4 严格 descriptor 在解码业务记录前拒绝新
layout。缺省新字段不序列化，旧 input/policy bytes/hash 保持，v1 常量不变。

形成维度覆盖 fact/preference、episode/project_state、plan/open_loop、
relationship_experience、subjective_understanding/theme。保留条件、发生断言、
未知/区间事件时间、独立有效期、变化理由、反证 context、精确派生父修订及
历史回退目标。主体/项目仍用已有正式字段；获知时间来自真实 native commit，
不是生成器填写。报告发生仍是“本人说过发生”，不等于独立证实。

纯规则区分暂停/恢复/取消/完成，禁止普通 update 越过项目里程碑或重开终态；
明确纠正需要新支持来源修订。计划不会通过改类型直接变成经历。年龄和有效期
不会把 open loop 自动完成。回退用新 revision 精确复现旧陈述、条件和来源，
不倒退 CURRENT，不复活已删除或过期修订的来源。

巩固选择精确当前父记录，展开并去重原始 source refs，仍经既有持久作业、
准入和合法事务。父记录不合并删除，重复反思不生成自己的新 Source 或独立
支持。派生结果不能冒充用户自述，理解/主题保持 hypothesis/staged；未来可
自动重评或弃答，没有必由 Morii 清理的审核队列。父修订/删除改变后，后代
使用及回执恢复重新检查精确 lineage。当前 context 带上类型、条件和时间限定。

## 实际验证

9 项 formation 契约测试通过，覆盖：类型/schema 负例、计划状态机、rule/
策略和新证据要求、未知及未来时间、系统理解/反证、精确父来源、历史回退、
旧 layout 拒写，以及独立关键单元的漏提比较。

7 项 native formation 测试通过，使用真实合成文件、OS lease 和脚本模型：

- 计划 -> 暂停 -> 恢复 -> 取消，与另一条已发生经历分开，重启后可查；
- 项目 planned/decided/implemented/tested/released 逐步留历史；
- 有条件偏好纠正后生成第 3 个精确回退修订，原件/旧修订未改；
- 两条原始来源巩固、再次反思，仍只有两条原始来源且支持不增加；父纠正后拒用旧后代；
- 反证依赖使结果 disputed/staged，新证据再纠正采用；
- Core-4 持续 fake 策略自动完成 System 理解提交，但普通自述 context 仍弃答；
- 有效期届满退出当前召回，open loop 仍 open 且历史保留。

Core-1/3/4 契约和 Core-2 的 18 项作业回归也通过；native automatic_loop
当前共 35 项（包含两个子进程入口）。focused clippy 通过。
离线 `cargo fmt --all -- --check`、`cargo test --workspace --locked`
及 `cargo clippy --workspace --all-targets --locked -- -D warnings` 全部通过；
全 workspace 608 项测试通过。独立 Python 3.12 / jsonschema 4.26.0
cross-check 通过：48 个 schema，新增 formation 10 个正反例。
runtime-surface 已生成并登记，55 条 Workspace 命令不变。

## 比较证据和局限

关键单元分母独立保存于 `key-units.json`：九种形式，含条件、取消、未知时间、
暂停、未竟事项、关系经历和系统理解。Core-4 没有这套 typed dimensions，
此维度表示比较为 0/9；预置脚本输出为 9/9。故意漏掉一条时识别 1 条漏提，
把 cancelled 改为 completed 时识别 1 条错误。原文仍可能在旧 content 中，
所以 0/9 不代表旧模型完全漏掉内容，9/9 也不代表新模型会自行正确抽取。

以上是表示能力、规则和合成管线证据，不是任意自然语言理解、情绪/人物关系
识别、全量覆盖或真实提炼质量。脚本输出和 native form 选择是预先设定的。
尚未实测真实模型的忠实/漏提/误提、独立评审或私人历史；不能宣布语义质量验收。

固定研究包 folio `bf6887575fa8e6fa519279be9dc7e5886a29d2ea` 和 Ombre
`115d831d979bf67efc7150f976e2cb34c333c935` 仅用于形成/反思边界参考；
没有执行外部仓库、迁入其数据库、复制源码或连接 embedding/Provider。

## Runtime 与后续

Runtime 工作树/pin 不变，旧消费者不能只检查 schema major 后打开新 layout。
未来采用需保留 occurrence/event/valid/learned time、condition、state/change、
machine hypothesis 与 self-report 的区别及来源/派生/回退引用；不能把模型
理解当 Morii 的感受、把 cancelled 显示为 completed 或把 plan 放进已发生经历。

没有真实格式迁移、生产旁库/双写、MCP 新工具、通知、图谱/向量、周期认知、
物理 purge 或独立恢复，也没有 merge/deploy/项目状态刷新。Core-6 下一片处理
有解释的情境选择、周期维护和自然淡出，继续先合成验证，不依赖 Activity/Usage。
