# MV-8.1b · 主权控制文件与原生租约

> Historical 2026-10-18 evidence from the retained isolated worktree. Current mainline integration and final check results are recorded separately in [the 2026-10-21 report](MV-8-authority-integration.md). Original failure and scope statements below are preserved.

日期：2026-10-18。在主人已授权的 MV-8 和随后“继续”指示下，接续 [8.1a 契约](MV-8-host-authority-contracts.md)。同一分支 `codex/mv8-host-contracts`，起点 `56bc5eb`，尚无提交/推送/合并。

## 边界与实现

[ADR-MEM-67](../adr/067-authority-control-file-persistence.md)先明确：原子写一个主权文件不能冒充 authority 与 CURRENT 的共同事务。本片只增加可显式调用的 Windows `host_authority::ControlStore`，不实现完整 AuthorityStore，不暴露普通数据进度入口，不接入当前 Workspace/CLI/Runtime。

控制文件为 `config/host-authority.json`。打开时重新验证根、NTFS 和受保护 owner ACL，再持有已有 host.lock 与 Vault writer lock，直到 drop/进程退出。根权限不合格时拒绝，不自动修 ACL。受控路径、重解析点检查、有界读取和刷新后同目录原子替换复用既有 Vault 原语。bootstrap 只接受相同 Vault、独立 checkpoint 和新 epoch-1 状态；任何已存在文件都拒绝覆盖。open 不初始化缺失/损坏文件。

每次 inspect/apply 重新读盘、严格验证并调用原生 checkpoint 端口。只发布相同 checkpoint 的 quiesce/promote/abort。写入错误保守返回 outcome_unknown，调用方重新检查；准确 promotion 重放不再写盘。checkpoint reader 和目标恢复证明仍是原生端口，测试 reader 是固定合成材料，不能当完整快照验证。

## 验证与失败记录

新增 8 项 Windows 定向测试全部通过，exit 0：重开与 promotion 收据、真实 Vault head 不变、缺失/损坏/不可达无缓存降级、外来 Vault 与错误 checkpoint、权限不合格拒绝且不修改 ACL、发布前错误与发布后丢响应、跨进程双锁排他，以及两个受控子进程中断边界。中断测试确认子进程到达 apply；重开只看到完整旧状态或完整 quiesced 状态，进程退出已释放租约。

首次定向 1 通过、6 失败，错误均在新的 owner ACL 前置检查：旧 synthetic lifecycle 不设置受保护根 ACL。只在新测试的临时根显式设置保护，并新增拒绝反例；没有放宽产品校验或触碰用户目录权限。失败和修正日志分别保存在忽略目录 `.local/mv8-authority-control-targeted.log`、`-targeted-fixed.log` 及对应 exit JSON。

接续实现后按 Rust 1.98.1、MinGW UCRT64、CARGO_NET_OFFLINE=true，在可执行 Windows ACL 操作的非沙箱环境运行一次默认并行全套。没有设置全局串行化或改动源码/测试期限。

| 检查 | 最终结果 |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 101；已执行汇总 399 通过、1 失败，不能称全套通过 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| 独立 python-jsonschema 4.26.0 / Python 3.12.10 | 沿用本轮同版输入证据：38 schemas、19 Host 文档用例及全部既有材料通过，exit 0；此后未改 Schema/fixture/独立 checker |
| `python tools/integration/runtime_surface.py` | 最终 aggregate 已记录，exit 0 |
| diff、公开内容形状与状态文件范围检查 | 通过；未发现新私有路径/凭据形状，未改 docs/status |

失败仍为旧 `full_session_ask_replays_an_input_published_while_the_call_is_paused`，Workspace 65 通过、1 失败。trace：首调用 21 ms 恢复，5031 ms 等待超时释放第二调用，两者分别 6216/6225 ms 返回。没有修改该 Workspace 代码、5/10 秒期限、两调用并发、旧 pin 或删除断言。8.1a 首次完整检查的同类失败也保留，不以重跑覆盖。

随后只做一次单包/单用例隔离诊断，1/1、0.81 秒、exit 0；它并非前次全套的同一构建可执行文件，不构成对根因的受控证明。读取调用路径确认 TurnId 分配处的暂停早于 writer lock 与索引编译，尚未定位首调用内部慢在哪一段，不推断磁盘、锁或调度为根因。没有延长时限、压力重跑或宣称修复。全量门槛保持未通过。

最终命令输出与退出码在忽略目录 `.local/mv8-authority-control-final-{fmt,rust,clippy}.log`、`-final-checks.json`，隔离诊断在 `.local/mv8-ordered-barrier-isolated.log` 和 `-isolated-exit.json`。Schema 证据在 `.local/mv8-host-contracts-schemas-final.log`，接入面以最终检查输出及本报告 aggregate 为准。日志不入库。

## 未完成与采用

这是合成 Vault 上实际文件与子进程验证，不是 OS 崩溃/断电、Linux/VPS、完整快照恢复或实际主机迁移验收。租约存活时覆盖现有 host/writer 锁；租约结束后旧 Core/CLI 仍不读取控制文件，因此持续和跨主机栅栏尚未激活。真正普通数据写入需要后续共同事务与恢复协调器，不能把两次独立写入拼接上线。

当前没有 Host 可执行进程、认证 IPC、版本握手、Runtime 重连或 MCP 客户端支持声明。MV-7 P02～P06、真实资料、凭据、自动接受、云部署及生产切换边界保留；Dashboard 两份 Protocol v1 快照未刷新。当前 Runtime 无需改 pin 或适配器；新增接入面与继续采用要求同片记录于 [RUNTIME.md](../integration/RUNTIME.md)。

最终接入面 aggregate 为 `4c290ac0efdb00e335fac832d18052421ffa2310c45b7c07a1e804a03ad88bb4`。交付前只读复核时，Runtime 的并发工作已将主 checkout 改为 `codex/runtime-v1-inspector`、HEAD `a4a3876`，新增未合并 #82，并有新的未提交界面/设置草稿改动；这些不是本片改动或验证证据。本片未切换 Runtime 分支或覆盖文件。Memory #34/#35 和 Runtime #81 仍 open，本片没有合并授权。Memory 主目录仍为 `main=9f0d7e5`，用户 AGENTS.md 的 hash 不变；本片代码只在独立 MV-8 工作树，先前 MV-7 文案工作树也保留。
