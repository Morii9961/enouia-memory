# Core-7 授权读模型与模拟端口验证

状态：完成独立 native/合成实现与验证；不等于 Runtime 已采用或真实模型质量验收。

日期：2026-10-24～25。基线 Core-6 `be6115348a737bcf1e429bcb0ee0d7ec2044e5ed`。
分支 `codex/core7-authorized-read-model`。Morii 的继续授权覆盖本片。
10 月 24 日按额度停止要求保存 WIP `49c24956b92e15e7d6a20ee97d2d53eb7a8d34a2`；
次日按 Morii 的继续指令复核、补强并重新验证。本片提交不含 Runtime 修改；
Core-8 已在接续过程中由 Morii 另行明确授权，并在独立 Runtime 工作树推进。

## owned-file allowlist

- `crates/enouia-memory-contract/src/next/{mod,attention,read_model}.rs`
- `crates/enouia-memory-contract/tests/read_model.rs`
- `crates/enouia-memory-vault/src/automation/{mod,read_model}.rs`
- `crates/enouia-memory-host/src/automation/{mod,application,read_model}.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-host/tests/automatic_loop/read_model_cases.rs`
- `crates/enouia-memory-host/tests/automatic_loop/continuation_cases.rs`（必要状态观察测试修复）
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
页游标绑定 root snapshot、filter、profile 和 native sensitivity ceiling。
变化后拒绝续页，WIP 版本游标需重新从首页开始。source text 默认不返回，需明确
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

3 项契约测试通过，read-model fixture 共 17 个 schema/native 正反例，覆盖
版本/capability、System 身份、回执、假全局能力、重复行、来源 hash、字节数、
错误有效性标签、未来 learned time 及 synthetic effect 假冒 native receipt。
新增负例暴露 page 有效性检查遗漏后已修正，并重跑通过。
接续增加 6 个纯验证负例：缺失进度游标、重复历史、同修订内容矛盾、重复来源、
召回总数矛盾及重复选择。补强严格页顺序、游标与末行绑定、历史修订顺序与
上界、当前历史值一致性、来源唯一性和 selected/excluded 的完整唯一计数。
其中缺失进度游标已先复现验证器接受错误结果，再修复通过。

5 项 native read-model 测试通过：

- 两页稳定读取，snapshot/filter/profile/sensitivity 变化拒绝 cursor；Public 对 Private
  的 items/count/unknown-date count 皆为空，隐藏详情同样不存在；
- 原生纠正形成两条授权历史、按需原文、原 UserStatement；commit 时间与
  captured 时间相差 5/6 秒，证明不同字段；读取不改 CURRENT，超小预算拒绝；
- 局部派生边只在授权端点之间出现，来源删除后详情/列表不复活，锁定拒读；
- 两个模拟 effect 明确，选中结果可比，均无记录、preview、实验或索引写入。
- 事件窗上界排除，未知日期单独统计；expired/adopted/open 三轴保持独立；
  受保护 episode 仍受保护；source text 与历史按字节预算分别省略并标记部分覆盖，
  必需当前项不变；native 时钟回退拒绝读取。

10 月 24 日额度达到 98% 时保存 WIP，没有在当时宣称完整验证通过。
次日发现旧日志中全套最终均返回 0；因本次补强有代码变更，重新执行全套，
最终结果见下方收尾记录。初次沙箱内 native 测试在合成作业根初始化时返回
StorageUnknown；在正常 Windows 文件权限下同组测试通过，未放宽产品持久化规则。
默认并行全套曾有两项旧 Workspace 竞态返回 `fault.writer_busy`：已有 2 秒 writer
等待上限，而失败 trace 中赢家在约 3.2 秒后完成；单独运行 10 项并发测试均通过。
全套限定 `RUST_TEST_THREADS=4`，仍保留每项测试内部真实并发，未改产品等待或幂等语义。
该次检查另暴露旧 Core-4 测试对非阻塞 `status()` 的单次 unwrap；它允许返回 Busy。
测试现改用既有有界状态观察 helper，未重试生成，原生成次数/总预算/修订账本断言保留；
focused 回归通过后重跑全套。
独立 Python 3.12 / jsonschema 4.26.0 已通过：50 个 schema、read-model 17 个正反例。
runtime-surface 已生成并在 RUNTIME.md 登记，55 条 Workspace 命令不变。

2026-10-25 最终收尾：Rust 1.98.1，`CARGO_NET_OFFLINE=true`；
`cargo fmt --all -- --check` 通过；在 `RUST_TEST_THREADS=4` 下
`cargo test --workspace --locked` 共 629 项通过、0 失败；
`cargo clippy --workspace --all-targets --locked -- -D warnings` 通过。
独立 Python 3.12 / jsonschema 4.26.0 校验通过，50 个 schema / 17 个读模型 fixture；
Runtime surface 为 `4369ee7d336402945117f0e571c08c6a70c7d43e96efbff43a815a1cd3abdfb5`，
已在同提交登记。公共内容、owned-file 边界、文档本地链接与 diff whitespace 检查通过。

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
