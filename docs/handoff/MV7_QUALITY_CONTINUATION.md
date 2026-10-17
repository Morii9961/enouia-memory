# MV-7 质量链路持续工作接续

日期：2026-10-14。实现基线 `68d3d13545e941f54f65e42f512c347ca7b21a0a`，分支 codex/history-quality-json-hardening。主人要求持续推进当前 MV-7；真实资料、Provider 调用、Runtime 激活和 MV-8 不在本轮授权内。每片先完成验证并提交、推送，状态快照保持手动维护。

## 当前完成与发布

- PR #17：H2i，head a8c09b9，等待既有合并选择。
- PR #18：H2j，head 121b1df，草稿 base 为 PR #17 分支。
- PR #19：H2k，head 68d3d13，草稿 base 为 PR #18 分支。
- H2k：严格有限数字、128 层容器预算、旧来源结构拒绝；23 项 Python 合成检查、跨语言材料、fmt/严格 Clippy、36-schema 独立检查、Runtime checker 与公开内容检查通过。最终串行 Rust 382 项通过，退出码 0 已保存。日志/结果在忽略的本地验证区；没有真实模型质量或主人验收结论。

## 必须继续处理的并行夹具问题

默认并行确认运行退出码 101，Workspace 60 项通过、4 项失败。证据为本地 mv7-h2k-workspace-tests-confirmed.log 与 mv7-h2k-rust-exit.json：

1. concurrent_context_session_input/fork/checkpoint_replays_the_published_id：低层 Session 调用直接 unwrap 可重试的 Vault Busy/WriterBusy；VaultOptions 默认等待写锁 2 秒，负载下会返回该契约允许的错误。
2. full_session_ask_replays_an_input_published_while_the_call_is_paused：有序释放夹具要求首调用在 5 秒内完成，本次没有满足；此前还曾有双调用就绪期限失败。

位置是 crates/enouia-memory-workspace/src/tests.rs 的 check_concurrent_context_session_write、with_concurrent_allocations_at 和 call_with_contention_retry。后一 helper 已针对页面接口的 index_not_ready 与 retryable fault.writer_busy 使用有界重试，低层 Context 夹具尚没有同样处理。

下一步先用真实合成 writer lock 强制 Busy，再验证原始请求键下的恢复/replay；仅对契约明确可重试的 WriterBusy 重试，不能重发 Provider 或吞掉其他错误。独立调查受控调度是否可隔离其他场景的资源竞争，保留同场景两调用并发和旧 pin/删除屏障的断言。不放宽 5 秒/10 秒超时，不把全局串行化或一次通过称为稳定性修复。方案尚未实施。

本轮在共享五小时窗口使用约 93%、周窗口约 76% 时保存接续，未使用额度重置信用或购买额度。启动下一片前先确认能完成 required checks 的余量。

## 接续约束

重新核对 HEAD/远端/PR、工作树和适用 AGENTS.md；保留用户 AGENTS.md 修改及未跟踪 docs/status/。只提交本片文件，Codex trailer 保留。若 Workspace source/测试改动进入 Runtime watched surface，同一提交重新生成 manifest 并记录测试夹具采用边界。按 pinned toolchain 执行全部要求并保存退出码，明确默认并行与串行结果。不要刷新项目状态快照或读取私人材料。
