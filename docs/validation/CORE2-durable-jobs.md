# Core-2 持久作业与资源隔离验证

日期：2026-10-24，Asia/Shanghai。基线：Core-1
`678c2d75d0eb88f3c98c31a95d08a490d24627be`。Morii 在 Core-1 交付后指示
“继续下一步”，本片只领取 Core-2，分支 `codex/core2-durable-jobs`。
没有进入 Core-3，不创建 PR、不 merge/deploy、不升级 Runtime。

## 工作区与 owned files

复用当前任务已附的隔离工作树，核实 branch/HEAD/status/worktrees。
开始时没有 tracked 改动，公共交接借阅副本仍未提交；沿用已复制的最新
优先级/路线，不从旧 HEAD 重新制定路线。原 checkout、AGENTS、手动状态
快照、借阅设计/研究/交接文档及 Claude 的 Runtime 树均未修改或纳入提交。

本片 allowlist（19 个文件）：

- `crates/enouia-memory-host/src/lib.rs`
- `crates/enouia-memory-host/src/automation/{mod,model,worker}.rs`
- `crates/enouia-memory-host/tests/automation.rs`
- `crates/enouia-memory-vault/src/fs.rs`：仅公开既有 bounded-read 方法
- `contracts/next/job-journal-v2.schema.json`
- `tests/fixtures/automation/{queued,generated,manifest}.json`
- `tools/integration/runtime_surface.py`、`tools/schema-check/check_schemas.py`
- `docs/adr/086-durable-automation-jobs.md`、`docs/adr/README.md`
- `docs/contracts/CORE2_DURABLE_JOBS.md`、`docs/contracts/CONSTRAINT_MAP.md`
- `docs/integration/RUNTIME.md`、`docs/integration/runtime-surface.json`
- 本报告

## 具体实现

[ADR-MEM-86](../adr/086-durable-automation-jobs.md)与
[作业契约](../contracts/CORE2_DURABLE_JOBS.md)先冻结本片边界。
新增显式 SyntheticJobs，采用已有 ManagedRoot、bounded read、原子替换
与独立终身 OS 文件锁。拒绝包含 Vault 的根；只在合成临时根测试，没有
建立生产旁库或改变真实 CURRENT。无新 crate、依赖或锁文件变化。

批次最多 8 个输入，journal 最多 16 个作业、16 MiB，一个专属 Memory
线程且最多一个在途调用。输入必须通过当前 Core-1 上下文验证，并精确
绑定 Vault、base commit、primary epoch、policy/deletion 水位、真实
System、策略 revision/hash、来源/目标修订。内容去重不依赖新生成的
operation/result ID；重复请求返回原作业和原身份，变更 payload 重用
operation 或跨批次重复逻辑输入拒绝。

分发前先持久记录 running 意图与 token/cost 预留，结果与 cursor 同一
次原子替换推进。费用是保守合成预算，不是实际计费。资源限额跨重启
保留，不因未知结果退回额度。未知调用不自动重发；有完整结果的游标
不重复生成。取消/锁定/权限变化/过期/退出阻断新分发，晚到结果不会
恢复作业或推进游标。重开默认锁定，需新 native permit；只有没有未知
调用的 invalidated 作业才允许精确重评后显式 resume。

worker 不使用 Runtime 共享池或 Vault writer lock，前台需求阻止下一次
后台分发。生成等待在控制门外，status/cancel/shutdown 不 join 生成器。
start/poll panic 被捕获并标为 unknown。测试还故意阻塞 poll，证明控制
仍可用；不宣称能在 Rust 线程内强杀不合作的引擎。真实适配器必须遵守
非阻塞 start/poll 合约，不可信引擎需要另行进程隔离。

## 验证与证据边界

固定 Rust 1.98.1 / GNU Windows / CARGO_NET_OFFLINE=true。当前树 AGENTS
要求完成前运行全 workspace 检查，本片照此执行；没有安装依赖。

| 检查 | 实际结果 |
| --- | --- |
| `cargo check -p enouia-memory-host --locked` | 通过 |
| `cargo test -p enouia-memory-host --test automation --locked -- --test-threads=4` | 通过：最终 18 项（17 个主场景与 1 个子进程入口），包含可变响应、哈希校验与未来结果存储预留 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --workspace --locked` | 最终版本通过：557 项，零失败；0 doctests，exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 最终版本通过，exit 0 |
| 独立 Python schema cross-check | 通过：python-jsonschema 4.26.0，45 schemas，queued/generated 两个有效 journal 加 16 个负例（共 18 个），原有 fixtures 同步检查 |
| Runtime surface 生成与检查 | 通过：aggregate 已登记，55 个 Workspace 命令未变 |
| owned diff、公开内容、本地链接检查 | 通过：19 个 owned files，119 个本地链接，无私人绝对路径或凭据形状命中 |

合成场景覆盖：精确 Core-1 准入；去重与 operation 冲突；前台优先；
跨重启原结果保留；取消/锁定/退出；全部绑定轴变化、策略过期与已保存
时间水位回退；在途主权变化与晚到结果；独立 writer lock 可同时取得；
token/cost/capacity/concurrency/output 限额；超时、unknown、start/poll
panic；实际落盘失败不发送或保留未知结果；未知 major 和 OS 排他。

实际子进程在两种持久边界被父进程终止：第一项结果已保存且下一项尚未
分发时，重启只生成剩余项；调用已分发但结果尚未保存时，重启保留
unknown、预留与原身份且不重发。没有把假端口结果等同于真实模型质量、
独立备份恢复、断电持久性、OS 认证或 Runtime 产品验收。

首次沙箱内原子替换统一返回 StorageUnknown，同一用例在允许环境重跑
成功；未削弱底层文件/权限约束。后续实际落盘、并发和进程检查在允许
环境运行。日志保存于 ignored `.local/core2-*.log` 与 exit JSON。
首轮 workspace 555 项和严格 Clippy 已通过；复核后将生成结果由预定决定
改为精确输入绑定的原始 JSON，并补充结果存储预留，最终完整重跑为上述
557 项。最终日志/退出码另存 `.local/core2-final-*.log` 和
`.local/core2-final-rust-exits.json`，不是以早期结果冒充最终版本。

## Runtime 影响、未实现项与停止点

Runtime surface 新增 Host automation 源、job schema 和已有 bounded-read
实现；兼容记录同提交交付，aggregate：
`304a1537d09729c88d527198a5f57cbd3138f6155e90ed7104326aeed94ddb91`。
现有 HostService、MCP、Workspace IPC、默认 owner writer 与 Runtime pin
没有接入本模块。后来 native 采用必须提供真实生命周期/foreground 事件
及新鲜 permit，不能从页面或 MCP 请求伪造。

生成响应允许与输入中的初始分析不同；以精确 input hash、response hash
和原始 JSON 保存，不把响应中的 owner/权限/知识声明当作授权或已准入。
Generated 仅表示不可信生成响应保存，不是记忆已应用。没有 Canonical 写入、
ReviewRecord、owner confirmation、applied 标记或成功提交回执。
没有真实 Provider/embedding、持续外发策略、私人 Vault/聊天导出/凭据、
Activity/Usage、通知、产品 UI 或生产迁移。

Core-3 的前置是本片持久作业与 Core-1 契约；另行领取后，仍须实现新格式
合成库的合法单 Host 事务、精确依赖/权限重评、幂等、崩溃恢复与真实提交
回执，再验证“来源→自动记忆→下次使用”的合成闭环。本片提交推送后
停止，不自动开始 Core-3 或 Runtime 采用。
