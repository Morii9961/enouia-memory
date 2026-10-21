# MV-8.1a · Host 主权与平台事务契约

> Historical 2026-10-18 evidence from the retained isolated worktree. Current mainline integration and final check results are recorded separately in [the 2026-10-21 report](MV-8-authority-integration.md). Original failure and scope statements below are preserved.

日期：2026-10-18。基线 `56bc5eb8b08fb80cd6df892bf53d200ff23afc4c`，分支 `codex/mv8-host-contracts`。Morii 已明确批准开始 MV-8；MV-7 真实 P02～P06 仍 pending，不因本片开发而完成。根 checkout、上一片 MV-7 文案工作树和其他贡献者的未提交改动均保留。

## 实际缺口与实现

现场确认已有 `indexes/host.lock` 只排斥同一 Vault 的第二个嵌入 Core。当前 Vault/IPC 没有 `primary_epoch`，也没有跨主机的停写、快照比对和提升协议。[ADR-MEM-66](../adr/066-host-authority-and-platform-contracts.md) 和 [Host authority v1](../design/HOST_AUTHORITY_V1.md)先冻结本片契约，未倒推修改已有存储格式。

新增纯 `host` 模块和两个闭合 Schema。authority 记录单一 Vault/主机/epoch、checkpoint、阶段与全部 promotion 收据。迁移必须先 quiesce，复核目标的完整 checkpoint，再提升一个 epoch；abort 只恢复仍在 quiesced 的原主。普通进度拒绝旧提交、跳号、重复提交 ID/快照摘要和撤回/删除水位倒退。精确 promotion 可跨后续迁移重放而不再次发布；保留全部收据，128 次上限后拒绝新增迁移。

原始文档上限 256 KiB，直接 typed parse 拒绝重复解码字段名、未知字段、未知主版本、不安全整数与非对象输入。`AuthorityStore` / `AuthorityTransaction` 冻结原生平台的最新 authority、数据 checkpoint、独占事务和持久发布义务；密钥仍沿既有 SecretStore 原生端口，本片未增加秘密的序列化或读取。

## 验证

新增 10 项 Rust 合成测试、4 个合法文件和 15 个命名负例，分别通过 Rust 子集 Schema 与独立 python-jsonschema 交叉校验。负例明确区分 schema 可表达的 shape/数值/预算与 Rust 的 hash、主权链、阶段绑定语义。

首次定向 7 通过、2 失败：新 Schema 使用绝对 `$ref`，仓库子集测试器按相对路径解析。保留失败日志，改用现有相对引用模式，不扩大验证器。补齐边界后定向 10/10、exit 0。

本片首次全套 exit 101：新增 10 项全部通过，其余测试直到 Workspace 的旧有序屏障失败（65 通过、1 失败）。失败为 `full_session_ask_replays_an_input_published_while_the_call_is_paused`：首调用在 127 ms 恢复，5133 ms 等待超时，5800 ms 才返回；第二调用在超时释放后 5808 ms 返回。该 trace 尚不足以确定首调用内部耗时位置，不据此归因到锁、磁盘或调度。现有 Workspace 源码/测试未因本片修改；不放宽 5 秒期限、不做压力重跑，不宣称修复或全套通过。

fmt、单独补完的严格 workspace Clippy、38-schema 独立交叉检查（含 19 个 Host 文档用例）和接入面检查均 exit 0。Clippy 结果在同前缀 `-clippy-exit.json`；全套中的其余成功输出不能抹去本次失败。

本机原始/修正定向日志为忽略目录 `.local/mv8-host-contracts-targeted.log` 和 `-targeted-fixed.log`；全套检查使用同前缀 `-fmt.log`、`-rust.log`、`-clippy.log`、`-checks.json`，独立检查为 `-schemas-final.log`、`-surface.log`、`-python-checks.json`。日志不入库。

## 证据边界与采用

这是主权协议与可执行纯模型，不是运行中的 Host。FakeStore 模拟原子状态、数据 checkpoint、发布前故障和发布后丢响应，不证明真实文件系统、进程锁或断电恢复。测试还明确展示：复制旧状态后的本地纯比较可能通过；只有读取权威且同事务发布才能成为实际栅栏。权威不可达时不能运行事务或降级到旧副本。

当前 Workspace/CLI 尚未调用这些规则；旧 Vault 未被迁移，尚未获得跨主机栅栏。没有 Host 可执行进程、认证管道、会话撤回/重连、MCP 工具或真实客户端兼容性结论。无真实 Provider、凭据、私人资料、云端部署或生产迁移操作。

Runtime 接入面 aggregate 为 `b6ab9767c32af5dd3625fa555ba2507bc155d067109cdb90ffc6713d6ce56edb`，同片 [RUNTIME.md](../integration/RUNTIME.md)记录了新增契约和未来采用要求。47 条命令不变，本片无需 Runtime pin 或适配器切换。后续 MV-8.1 应实现平台持久事务与故障恢复，再做用户级 Host、认证 IPC、版本握手和参考壳重连；Runtime 显式采用后才退役嵌入写者。MCP 在这些基础上接续。

本片尚未提交、推送或合并任何 PR；既有 Memory #34/#35、Runtime #81 的合并继续等 Morii 明确指示。Dashboard Protocol v1 两份状态文件未刷新。快速搜索焦点、operation-cancel DOM 和旧有序屏障问题仍未定位，不因本片测试通过宣称修复。

Morii 随后要求继续，已在同一工作分支接续 [MV-8.1b 主权控制记录](MV-8-authority-control-file.md)。该包新增实际 Windows 文件/进程证据，仍未实现完整 authority/data 事务；本报告保留 8.1a 原始失败和证据边界。
