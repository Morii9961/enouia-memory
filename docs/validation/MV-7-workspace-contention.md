# MV-7 · Context 并行夹具的写锁竞争

日期：2026-10-15；起点 `e37c186`，基于 PR #19。仍在已授权 MV-7 的合成验证范围，只修改 Workspace 测试及其接入面记录。

此前默认并行全量 exit 101，三个 Context input/fork/checkpoint 场景直接 unwrap 可重试 WriterBusy，另一个 full_session_ask 的首调用完成屏障超时。旧失败日志保留，见 [H2k 报告](MV-7-quality-json.md)。本片没有把两类失败合并为一个根因。

先以合成临时 Vault 的真实文件锁强制复现：Context save_input 等待既有 2 秒写锁期限后返回 Busy/WriterBusy/retryable，head 不变；释放锁后沿用原请求键发布一次，再次调用返回原 ID 且 head 不变。修复前直接调用的复现用例通过（exit 0），确认这是契约允许的暂态错误。

测试专用 helper 只重试 code=Busy、fault=WriterBusy、retryable=true 的低层 Context 操作，最多三次、间隔 5 毫秒，最后错误原样返回。三个并行夹具保留原参数和键。回归验证真实锁释放后两次尝试成功、仅一条输入；持续 Busy 三次结束，非重试 Busy、其他 fault 和 IdempotencyConflict 一次返回。生产逻辑、Provider 发送策略、接口/契约/依赖均未改变。

同场景仍有两个并发调用；receipt、原 ID、旧 pin、分支和删除屏障断言保留。没有加大全局/场景串行锁，没有放宽 5/10 秒期限。尚未解释或修复旧的有序完成屏障偶发超时；单次通过不能证明负载下长期稳定。

默认并行 Workspace 66 项通过，exit 0（本地 mv7-contention-workspace.log、mv7-contention-workspace-exit.json）。仓库根 Rust 1.98.1、CARGO_NET_OFFLINE=true，未设置 RUST_TEST_THREADS，完整 cargo test --workspace --locked 384 项通过、0 失败，退出码 0 独立保存（mv7-contention-full.log、mv7-contention-full-exit.json）；其中 Workspace 66 项、7.22 秒。fmt --check、严格 Clippy（mv7-contention-clippy.log、mv7-contention-clippy-exit.json）、独立 python-jsonschema 4.26.0 的 36 schemas 与 Runtime checker 均 exit 0。公开文件/本地链接、路径/凭据形状及原状态快照 hash 检查通过。本轮默认并行成功是一次完整验证，不能撤销旧时限失败或证明它已修复。

继续核对了有序夹具的暂停点：整条 session_ask 在 save_input 的 TurnId 分配处暂停，位于 commit 及写锁获取之前；完成通知在整个 page call 返回后发出。后续 compiled 在索引锁内执行，旧日志没有记录阶段耗时，不能据此归因。现在记录调用线程的开始、分配暂停/恢复、返回及主线程屏障阶段的毫秒时序，仅失败时输出；两个工作线程均先 join，再传播 panic。没有请求/记忆内容，也没有调度隔离或生产修复。后续故障可分辨未就绪、未恢复、未返回或工作线程 panic。

补充一次定向较高并发诊断：cargo test -p enouia-memory-workspace --lib --locked -- --test-threads=64，66 项通过、0 失败、7.13 秒，exit 0 保存于 mv7-contention-pressure-64-exit.json，日志 mv7-contention-pressure-64.log。仍未复现旧有序超时；该结果只覆盖这一合成运行，不是任意负载稳定性或故障根因结论。

初版接入面 aggregate 为 `ac15437a5c3a74b62d71f63c4519aaede5d630c9e9b7ab342b96a58ec0702111`；补充失败诊断后为 `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`，仍为 47 commands。补充诊断后重新执行默认并行全量 Rust：384 项通过、0 失败，Workspace 66 项、6.29 秒，exit 0 已独立保存于 mv7-contention-trace-full-exit.json（日志 mv7-contention-trace-full.log）；fmt --check、严格 Clippy（mv7-contention-trace-clippy.log、mv7-contention-trace-clippy-exit.json）、Runtime checker 和公开检查通过。36-schema 检查沿用本轮此前证据，schema 输入没有变化。结果为本地 mv7-contention-trace-check-results.json。有序超时未再次触发，失败输出路径没有本轮实跑故障证据，原因仍未定位。变更仅在 watched tests；[Runtime 记录](../integration/RUNTIME.md)说明没有产品适配动作，采用这些合成夹具可更新 pin。真实历史/模型质量、Runtime 产品采用和主人验收仍待独立工作；状态快照未刷新，MV-8 未开始。
