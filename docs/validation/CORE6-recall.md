# Core-6 情境召回与周期淡出验证

日期：2026-10-24。基线 Core-5 `e59617012c626d93c6932c2ff5c9355c988e4564`。
分支 `codex/core6-contextual-recall`，依 Morii 的继续授权实施。原 checkout、
并行 Provider/Claude Runtime 树、借阅文档、AGENTS、手动状态和私人数据均保留。

## owned-file allowlist

- `crates/enouia-memory-contract/src/next/{mod,attention}.rs`
- `crates/enouia-memory-contract/tests/attention.rs`
- `crates/enouia-memory-vault/src/automation/{mod,attention}.rs`
- `crates/enouia-memory-host/src/automation/{mod,application,attention}.rs`
- `crates/enouia-memory-host/tests/automatic_loop.rs`
- `crates/enouia-memory-host/tests/automatic_loop/{formation_cases,attention_cases}.rs`
- `contracts/next/attention-v2.schema.json`
- `tests/fixtures/attention/{profile,projection,manifest}.json`
- `tools/schema-check/check_schemas.py`
- `docs/adr/090-contextual-recall-maintenance.md`、`docs/adr/README.md`
- `docs/contracts/CORE6_RECALL.md`、`docs/contracts/CONSTRAINT_MAP.md`
- `docs/integration/{RUNTIME.md,runtime-surface.json}`、本报告

## 实现

[ADR-MEM-90](../adr/090-contextual-recall-maintenance.md)与
[正式契约](../contracts/CORE6_RECALL.md)定义原生选择和非权威 attention 投影。
current System/policy/source/deletion/lineage/敏感门槛先于候选、计数和解释。
再按词面、显式别名、条件线索、事件时间和可解释固定算法选择，精确限制
最终限定语文本的 UTF-8 bytes。unknown event time 不填入消息日期。

attention profile 是检索参数，不授予访问权限。active/fading/archived/protected
与知识、采用及计划状态分离。原始来源 captured 时间决定 age；反复生成摘要
或搜索不刷新它。历史相关查询可以找回合法 archived 项；受保护旧经历不因
年龄被压掉。记录、truth、owner 历史和 open loop 状态均不改写。

一个显式 Memory 原生线程周期重建 `indexes/core6-attention.json`，最多一例、
可取消/join，锁定暂停，Weak Host 避免永久占住根。投影只有元数据、snapshot/
profile/算法/时间，损坏或过期都不是 authority。读取始终重新过门禁而不相信缓存。
没有前端定时器、共享 Runtime 池、Activity/Usage、通知或正文行动执行器。

审查时发现历史排序时钟可能影响“当前有效期”判断，已拆成 ranking time 与
fresh native authorization time；增加回归证明旧排序时间不能复活已过期当前状态。

## 实际检查

6 项纯 attention 测试通过，覆盖 schema/native 13 个正反例、固定时钟重放、
词面/别名、条件/时间/staging 硬门槛、淡出/历史救回、实际字节预算及重复观察拒绝。

7 项 native attention 测试通过：

- ceramics -> pottery/cup 别名弥补固定词面空缺，多次搜索不写 CURRENT/记录/index；
- 缺少条件线索时弃答，正文限定语计入预算，Public 不显示 Private 的候选/排除数量；
- 原始源老化 200 天，open loop archived 仍 open，旧 episode protected，历史可救回；
- 损坏缓存不提供权限，删除后历史读取也不复活，锁定拒绝读取和维护；
- 真实单例周期线程生成/更新投影、锁定暂停、取消/join，不改 CURRENT；
- 事件窗口只匹配已知事件日期，不把未知日期放到 captured 当天；
- 旧排序时间不能绕过 fresh native 当前有效期，显式历史目的仍可限定读取。

离线 `cargo fmt --all -- --check`、`cargo test --workspace --locked`
和 `cargo clippy --workspace --all-targets --locked -- -D warnings` 全部通过，
全 workspace 621 项测试通过。独立 Python 3.12 /
jsonschema 4.26.0 通过：49 个 schema，新增 attention 13 个正反例。
runtime-surface 已生成并登记，55 条 Workspace 命令未改。

## 比较与局限

固定案例比较：旧词面 ceramics 无结果，新别名选择 1 个实际已授权 pottery
记录；缺条件时新选择弃答而不是把有条件偏好当通用；旧经历仍可找到，过期
当前状态仍受原有门槛保护。时间窗口、撤回、字节截断与原记录不变分别检查。
这些是确定性合成案例，不是跨数据集 Recall@K 或真实语义召回质量成绩。

100–60000 ms 是有界合成维护间隔，不是生产调参或延迟承诺。没有向量/图扩展、
学习排序、搜索强化、实体/情绪推断、主动通知、真实 Provider 或个人验收。
投影替代写入可重试，不代表 record/Provider 事务可盲重试。进程外独立恢复、
物理 purge、整盘回滚及生产压力仍未证明。

## Runtime 与下一片

没有 Runtime pin/工作树、MCP、IPC、默认 Host/Workspace 路径或旧读者语义变化。
旧 Core-5 HostV2 仍按旧读法忽略投影；不能宣称已经支持 attention 选择能力。
未来 Runtime 原生采用须保留条件/时间/knowledge/adoption/attention、授权先于
counts 和 fresh authorization time，并接原生生命周期，不让 UI 定时器驱动真实维护。
通知投递、安静时段、撤回和离线积压仍是不可用能力，不由时间字段推导权限。

后续 Core-7 在独立切片冻结实际有界读模型和模拟端口；不自动进入 Runtime
采用、真实迁移、merge/deploy，也不刷新手动项目状态。
