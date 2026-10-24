# Core-1 自动准入契约与纯规则验证

日期：2026-10-24，Asia/Shanghai。实施基线与原 checkout 均为
`9286cd8f89669d9ec3b92f943fd74072457536e7`；本片分支
`codex/core1-auto-admission`。只完成授权的 Core-1，不启动 Core-2。

## Core-0 与文件保护

已核实原树 branch/HEAD/status/worktrees 和任务附属工作树；原树存在
AGENTS、路线、Runtime 说明、手动状态及交接材料的未提交修改，没有复用
其他工作树。创建独立 managed worktree，从相同 HEAD 建立上述分支。
公共设计/交接/研究 Markdown 从原树受控复制用于阅读，最新计划和 Runtime
说明另存 ignored 参考副本；没有复制原图、私人附件或整包 .local。
这些借阅文档不属于本片提交。当前原树实现与材料基线没有提交差异，
未用旧交接覆盖新实现。Runtime 的探索/pin 信息来自交接，不作为本轮
Runtime 现场复核；没有访问或修改 Claude 的 Runtime 树。

本片 owned-file allowlist（仅这些文件进入实现提交）：

- `crates/enouia-memory-contract/src/lib.rs`
- `crates/enouia-memory-contract/src/next/{mod,admission,rules}.rs`
- `crates/enouia-memory-contract/tests/next_admission.rs`
- `crates/enouia-memory-contract/tests/support/schema.rs`
- `contracts/next/automation-v2.schema.json`
- `tests/fixtures/next/{policy,input,decision,commit,sources,manifest}.json`
- `docs/adr/085-machine-admission-drafts.md`、`docs/adr/README.md`
- `docs/contracts/CORE1_AUTOMATIC_ADMISSION.md`、`docs/contracts/CONSTRAINT_MAP.md`
- 本验证报告
- `docs/integration/RUNTIME.md`、`docs/integration/runtime-surface.json`
- `tools/integration/runtime_surface.py`、`tools/schema-check/check_schemas.py`

原树 AGENTS、计划、docs/README、两份手动状态、其他人的未提交材料均不
纳入提交；无 reset/clean，没有改源码/依赖以适配其他仓库。

## 已实现

[ADR-MEM-85](../adr/085-machine-admission-drafts.md)先冻结最小契约，再实现
现有 contract crate 的独立 next 模块。四种 closed 文档分别表示版本化
策略、精确准入输入、机器决定草案和未来提交关联。原全局 schema 1、
RecordKind/AnyRecord、OWNER_ONLY、CURRENT、默认写入口及 v1 IPC 未修改。

纯规则绑定真实上下文提供的 System 身份、完整策略 revision/hash、Vault/
base commit/primary epoch/policy/deletion 屏障；区分精确支持证据与全部
分析依赖；继承最严格敏感度并保守要求相同访问策略。目标必须是可写、
未删除的机器记录精确修订/hash，结果必须是合法下一修订；旧 owner
记录不作为机器改写目标。重复 source、相同内容或同一上游消息不增加支持。

清楚自述自动采用为 attributed_statement，含义是“本人说过 X”，没有
独立客观证实或伪造 owner 审核。推断/含糊/冲突分别保持 hypothesis/
uncertain/disputed 与 staged，带新证据/策略变化触发自动重评的状态。
缺少/过期/越权依赖拒绝准入，不产生必须由 Morii 清空的审核队列。
规则只消费分析分类，不宣称模型已经理解否定、身份、语境或全部语义。

结果/提交验证重新运行纯准入规则并绑定 exact snapshot、operation、
decision digest 与 result，仅返回 unit 成功；没有存储提交或实际回执。
真实单 Host 合法事务、幂等与 witnessed 成功回执仍是 Core-3 的职责。

## 版本、迁移与 Runtime

[契约及矩阵](../contracts/CORE1_AUTOMATIC_ADMISSION.md)区分源码 pin、record
schema、存储布局 2、host-current-v2、Workspace/Memory IPC 和自动化文档。
Core-1 的 V2 parser 只读草案，不是 V2 Vault 业务 reader/writer。
旧 Runtime/旧 MCP 只接支持的 V1 根；未来新根必须在 descriptor/CURRENT
schema major 和会话能力上、记录解码与查询之前拒绝旧消费者，不能把新
kind 塞进 V1 库后让旧程序跳过。unknown-major 仅能保留原字节诊断。

迁移设计要求源精确快照、独立恢复、原 owner 字节/批准历史保留、完整依赖
和删除闭合、单主栅栏与明确采用。新写发生后不能以降级二进制或旧备份
冒充回滚，不能降低策略/删除/主权水位。均未执行真实迁移。

本片只增加 library contract，55 命令与 native wire hashes 不变。
Runtime surface 已登记新增源与 schema，采用说明已更新；无需本片升级
Runtime pin 或迁 Host。未来最小读模型须保留形成方式、知识/采用、
证据/上下文、完整性及草案/真实保存的差异，不能显示成“主人已确认”。
当前 surface aggregate：
`c20dfd4275499f57e43b72d9d773071d8e485fe063cc780d8efbad786efff49b`。

## 实际检查

使用固定 Rust 1.98.1 / GNU Windows，CARGO_NET_OFFLINE=true，未添加依赖。

| 检查 | 本轮结果 |
| --- | --- |
| Core-1 聚焦规则测试与旧格式 header 拒绝回归 | 通过：新增 15 组测试；包含 4 个有效文档、14 个结构/语义负例及额外纯规则反例 |
| `cargo test -p enouia-memory-contract --locked`（包括 v1 record/set、owner 审核与 Runtime surface guards） | 通过：共 118 项，零失败，含 4 项 Runtime surface guards；0 doctests |
| `cargo fmt --all -- --check` | 已通过 |
| `cargo clippy -p enouia-memory-contract --all-targets --locked -- -D warnings` | 通过 |
| 独立 Python schema cross-check | 通过：python-jsonschema 4.26.0，44 schemas、18 next 文档正反例；同时复查原有 fixtures |
| Runtime surface 生成/检查 | 通过，aggregate 已登记，同一实现提交交付 |
| owned diff / 文档链接 / 公开内容检查 | 通过：22 个 owned files，117 个本地文档链接，无私人绝对路径或凭据形状命中；结果保存于 ignored 本地检查记录 |

最初新 fixture harness 未加载 next 目录、随后两个身份负例期待 shape
而实际返回精确语义 rule；已按现有 harness/ref 模式修正，新 14 组规则
测试随后全部通过，再增加旧 descriptor/CURRENT major 拒绝回归。
没有为通过测试削弱规则。

验证日志保存在本树 ignored `.local/core1-*.log`。没有运行全 workspace
test/clippy/build、smoke、外部研究脚本或真实模型质量评估；本片未改当前
跨 workspace 业务契约/默认路径，受影响 contract crate 的回归范围足够。
没有私人 Vault、聊天导出、凭据、真实 Provider/embedding、MCP 新接线、
生产旁库/双写、Runtime/Activity 访问、部署、merge 或状态快照更新。

## 未实现与下一片前置

未实现 V2 业务存储 reader/writer、自动作业、后台提炼、时间轴/星图、向量
召回、持续外发策略、通知和真实格式迁移；结构合成证据不构成日常自动
记忆闭环、模型语义质量或主人验收。

Core-2 可基于本契约继续设计并实现持久作业绑定，但仍须另获领取：精确
输入/策略/屏障绑定，生成与应用分离，重启/游标/逻辑去重，取消、锁定/
换库/退出失效，未知结果不盲发，以及独立资源预算和前台优先。它不能把
纯 binding 成功当作提交成功，也不能借旧 MCP/owner confirm 完成发布。
本片 commit/push 后停止；不自动开始 Core-2 或 Runtime 采用。
