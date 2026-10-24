# Core-7 授权读模型与模拟端口验证

状态：WIP；因额度达到 98% 按用户要求停止并关机，Core-7 未宣布完成。

日期：2026-10-24。基线 Core-6 `be6115348a737bcf1e429bcb0ee0d7ec2044e5ed`。
分支 `codex/core7-authorized-read-model`。Morii 的继续授权覆盖本片；接近限额后
停止新增功能，完成本片检查点。没有开始 Runtime 的 Core-8 采用。

## owned-file allowlist

- `crates/enouia-memory-contract/src/next/{mod,attention,read_model}.rs`
- `crates/enouia-memory-contract/tests/read_model.rs`
- `crates/enouia-memory-vault/src/automation/{mod,read_model}.rs`
- `crates/enouia-memory-host/src/automation/{mod,application,read_model}.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-host/tests/automatic_loop/read_model_cases.rs`
- `contracts/next/read-model-v2.schema.json`
- `tests/fixtures/read-model/{page,detail,simulation,manifest}.json`
- `tools/schema-check/check_schemas.py`
- `docs/adr/091-authorized-memory-read-model.md`、`docs/adr/README.md`
- `docs/contracts/CORE7_READ_MODEL.md`、`docs/contracts/CONSTRAINT_MAP.md`
- `docs/integration/{RUNTIME.md,runtime-surface.json}`、本报告

原 checkout、并行 Provider 与 Claude Runtime 树、AGENTS、手动项目状态、
借阅设计/交接/研究包、私人资料和 `.local` 均不纳入提交。

## 交付与契约

[ADR-MEM-91](../adr/091-authorized-memory-read-model.md)与
[读模型及 mount 契约](../contracts/CORE7_READ_MODEL.md)冻结
`memory-native-read-1`，提供显式 HostV2 合成根的 native page/detail、
选定当前授权修订历史/来源、局部精确派生/回退关系与两种只读模拟。
没有新 RecordKind、v1 IPC/MCP 工具、默认路由、依赖或 Runtime pin。

所有计数、正文、来源和关系边先通过当前身份/策略/删除/来源/lineage/敏感门槛。
页游标绑定 root snapshot、filter 和 profile。source text 默认不返回，需明确
选中详情请求；JSON 有界，详情优先省略可选正文/旧返回历史，标明 scope 内
部分结果，必需当前数据放不下则拒绝。不可访问详情与不存在一样返回空。

知识、采用、有效性、attention、计划/发生、条件、支持/上下文、策略/规则、
实际提交时间和原始来源 captured 时间分别保留。learned_at 从当前已验证
不可变链的实际包含 commit 取得；歧义或 native 时钟回退时拒绝，不用来源日
冒充获知日。列表/详情只返回已存 witnessed receipt，不创建回执或 owner 确认。

`simulate_synthetic_recall` 是无根、无 IO 的 fixture 计算；
`simulate_native_recall` 是实际当前根只读计算。明确 effect 标签区分两者；
均不调用旧 `context_preview`、不保存产物、不开 Provider。保存实验不可用。
全局时间轴/图谱、旧 owner 历史投影、向量也在 capabilities 中明确为 false。

## 实际检查

2 项契约测试通过，read-model fixture 共 17 个 schema/native 正反例，覆盖
版本/capability、System 身份、回执、假全局能力、重复行、来源 hash、字节数、
错误有效性标签、未来 learned time 及 synthetic effect 假冒 native receipt。
新增负例暴露 page 有效性检查遗漏后已修正，并重跑通过。

4 项 native read-model 测试通过：

- 两页稳定读取，snapshot/filter/profile 变化拒绝 cursor；Public 对 Private
  的 items/count/unknown-date count 皆为空，隐藏详情同样不存在；
- 原生纠正形成两条授权历史、按需原文、原 UserStatement；commit 时间与
  captured 时间相差 5/6 秒，证明不同字段；读取不改 CURRENT，超小预算拒绝；
- 局部派生边只在授权端点之间出现，来源删除后详情/列表不复活，锁定拒读；
- 两个模拟 effect 明确，选中结果可比，均无记录、preview、实验或索引写入。

focused clippy 通过。用户要求额度将尽时停工关机；额度达到 98% 时停止，
全 workspace 检查尚未完成，不宣称本片完整验证通过。本提交是 WIP 检查点，
后续必须重新完成 fmt、workspace test、workspace clippy 后才能宣布 Core-7 完成。
独立 Python 3.12 / jsonschema 4.26.0 已通过：50 个 schema、read-model 17 个正反例。
runtime-surface 已生成并在 RUNTIME.md 登记，55 条 Workspace 命令不变。

## 采用影响与边界

这是 Memory-native 的正式有界数据接口和模拟端口，不是 Runtime 原生产品
已经采用，也不证明真实模型质量/私人历史覆盖/真实 Provider 或大规模性能。
当前状态仍是匹配版本下的合成证据；旧 Runtime pin、旧 embedded Core、
MCP/Host 交付和旧 owner 历史保持原语义。

Core-8 前置：Runtime 单独采用匹配 pin/lock/manifest、schema/layout/rule/read
capability/effect、native adapter 和生命周期；逐一验证 Memory、Context、
Sessions、Provider 消费及无关功能。Memory 区域挂载由 Runtime 设计权威整合，
不覆盖共享 App/main/router/token。真实数据切换还需明确源范围、固定快照、
独立恢复、来源/历史/删除一致性和旧主栅栏；真实模型外发另需账号/预算资源授权。

没有 merge/deploy、真实迁移/迁主、生产旁库/双写、通知或项目状态刷新。
