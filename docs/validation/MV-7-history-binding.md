# MV-7-H2a · 归档来源绑定与抽取批次恢复

日期：2026-10-12。起点：`codex/history-initialization@5035c2e714e63fbfc285b346c7cd78fe03f1c576`。本片由 Morii 的继续开发指令领取，提交及推送标识以 Git 历史/远端分支为准。既有 AGENTS.md 和手动快照不在本次提交范围，快照内容未刷新。

## 已实现

`enouia-memory-provider::history::materialize` 从指定的已完成导入读取归档字节，核对 input hash、大小、adapter/schema，重新得到 H1 计划。主人明确指定批次索引；绑定当前来源修订、locator、content hash、说话人/证据、原始时间和完整度，附上同一路径前一条消息。全部片段先通过现有访问、删除、新鲜度、敏感/秘密与字节限额检查。

协调计划作为现有 Session input 保存，含原件/adapter/计划版本、选定顺序、来源/片段 hash、模型、subject 和每个子任务的限额；source_refs 列出依赖。它是可恢复的内部元数据，没有新 Session 字段、存储记录类型或公开 JSON/IPC 契约，不从正文获取批准或权限。

每批复用已有 ExtractionJob 与独立 Session。原 key/options 绑定整个计划，改选定顺序、模型或限额会报幂等冲突。取消、发布失败及重启后重放已保存子任务；现有游标、暂停、completed/failed/unknown 调用记录仍保留。`next_batch` 只是导航，不能授权重发。每批仍须正常请求审核、授权、宿主额度与一次发送；不增加全局账户预算控制。

单次最多 32 个显式选定批次，H1 每批 1～15 条消息并附一条前文；片段至多 32 KiB、单抽取至多 64 KiB、协调元数据至多 128 KiB，归档读取沿用 64 MiB 上限。超限或不安全选择直接拒绝，不默默省略。部分子任务发布后取消会留下可恢复进度，不声称整个多批事务原子完成。

## 验证

- 针对性来源绑定测试 10 项通过：归档 hash/依赖、重启且 head 不增殖、同键变更、非法/非主人选择、取消恢复、CURRENT 发布前/后的合成 I/O 失败恢复、暂停游标、超长文本拒绝、无关已清理来源隔离、原来源修订后旧计划拒绝。
- 两家假 Provider 的显式调用闭环通过：保留计划取消的原文、时间和未采纳建议上下文；候选仍待主人审核。恢复区分抽取 completed 和失败调用；没有再发请求。主人选择 `WithDependents` 清理来源时，协调计划内容被覆盖，原任务不能重新创建；Vault 验证仍一致。
- Rust 1.98.1、GNU Windows、离线：`cargo fmt --all -- --check`、`cargo test --workspace --locked`（354 项通过，0 失败）和 `cargo clippy --workspace --all-targets --locked -- -D warnings` 均通过，退出码 0。
- Python 3.12.10 / 已安装的 python-jsonschema 4.26.0：独立交叉校验 36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases；OK。未安装新依赖。
- Runtime 集成面检查通过，aggregate 已登记；47 条命令不变。`git diff --check` 通过；16 个本片公开文件及 152 个本地文档链接核对通过，所查绝对路径/credential 形状未命中，两份既有快照 SHA-256 未变。
- 完整测试和 Clippy 输出保存在忽略的本地验证日志中；上面是公开的合成范围记录，不提交任何测试 Vault、归档临时文件、秘密或真实会话。

发现与处理：新绑定层最初枚举 retained catalog 时读到无关已清理来源，导致其他任务误失败。用 `WithDependents` 合成回归复现后，仅跳过明确 purge 标记的条目；被选中的缺失/已清理来源仍拒绝，未吞掉其他存储错误。删除传播检查开始时误选 `AllRevisions`，原设计不因此清理来源；改为真正的来源清理范围后验证通过，没有扩大产品删除范围。两次测试类型引用错误已按本仓库 `commit::DeleteMode/DeleteScope` 修正。

## Runtime 与仍待验收

集成面重新生成并登记为 `33d01e1c682ed564fcb8ca7854f59e2f3ca4fc7f70a1e50cddfc07b7ff704912`。47 条 workspace 命令及既有宿主行为不变；Runtime 当前无需新增入口。在以后原生接入采用 history helper 时升级 pin，并保持显式批次、原 key/options、逐请求审核与失败/未知调用不重发。

本片无真实资料、API、凭据、部署或生产数据迁移。合成失败注入/重新打开不是断电或独立加密恢复证据；假响应引用正确也不是模型理解质量证明。只绑定这次导入写出的当前来源，不自动把其他导出复用的来源重映射到这份原件。未实现跨批新增/重复/纠正/取消/冲突判断、主题回顾、全量三年尺度/成本、原件分段读取或 Runtime 产品入口。

下一片继续 H2 的跨时间候选和质量样例；用独立问题集检查过去/现在、取消和未采纳建议、分支冲突、未知时间和缺证据，再决定全量处理成本。真实 Provider、个人样本和备份另行激活；MV-8 仍须明确启动。

回退为撤回本片 helper/测试/ADR/登记；已有单片抽取与 H1 仍可用。没有 Vault 格式迁移；已写入协调输入和子任务使用既有格式，可保留以供审核或明确删除，不删除其他会话的改动。
