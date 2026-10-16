# MV-7 质量链路持续工作接续

## 2026-10-16 普通来源路径解析补充

仍在 codex/history-matrix-manifest-boundary，基于已推送的 `8c87b83` 继续更新草稿 PR #26。普通 evaluate.py 的来源文件/套件目录解析循环现转为读取前的普通输入错误；新增一项、两个位置的回归，完整 Python 46 项通过。默认/扩展校准及八份原生合成材料共十份报告、21 个案例，与旧版完整 JSON 值一致，见 [报告](../validation/MV-7-matrix-manifest-boundary.md)。上次补充全量没有最终退出记录，不作为通过；本次独立补跑默认并行 Rust 387 项、fmt、严格 Clippy 均 exit 0，其余同版必需检查也已通过，结果已保存。仍保留 Windows 实际链接未验证的限制，不更改生产 Rust、Runtime 接入面或状态快照。

## 2026-10-15 合成清单目录边界

当前分支 codex/history-matrix-manifest-boundary，基于 PR #25 的 `c301ba8`。补上清单 manifest.json 的解析目标范围检查，目录外目标读取前拒绝，目录内目标读取解析后的文件；解析循环转为普通输入错误，16 KiB 预算保留，见 [报告](../validation/MV-7-matrix-manifest-boundary.md)。旧版清单两项回归分别失败/报错，循环解析两个子例报 RuntimeError；修正后完整 Python 45 项、默认并行 Rust 387 项及全部 required checks exit 0，八份既有 Rust 合成材料兼容检查通过。当前 Windows 符号链接探针返回 1314，链接证据来自可控 Path.resolve 模拟，实际链接验收未完成。生产 Rust/契约/接入面与状态快照未改；继续沿用验证、提交、推送和草稿 PR 流程。

## 2026-10-15 语义校准题集扩展

当前分支 codex/history-semantic-calibration，基于 PR #24 的 `8ed2297`。新增十个预设合成回答/判定，覆盖偏好时态、分支、助手建议、未知日期、忠实但遗漏及现状未定，见 [报告](../validation/MV-7-semantic-calibration.md)。42 项 Python、默认并行完整 Rust 387 项及全部 required checks exit 0。支持比例为 1 的遗漏例仍为 issues_found，含 uncertain/unknown 的例子仍为 review_required；四个支持、五个问题、一个待审仅为预设计算结果，不是模型通过率。默认三例和 evaluator 生产逻辑未改，Runtime surface 与快照保持原样，真实模型/资料、主人验收及下一阶段未启动。继续沿用验证、提交、推送和草稿 PR 流程。

## 2026-10-15 合成批次边界与恢复

当前分支 codex/history-batch-boundaries，基于 PR #23 的 `422399d`。为既有 H2a 增加三项原生合成验证：32 批在第 16 批后取消并重新打开恢复，33 批拒绝；15 条新消息加前文保持完整；32 个分支共 512 个片段引用超出协调元数据预算时不发布 Session。默认并行完整 Rust 387 项（history 13）及 required checks 全部 exit 0，见 [报告](../validation/MV-7-batch-boundaries.md)。数量和字节预算必须同时满足，不能据此承诺 32 个满批或真实全量规模。生产实现、契约、依赖和 Runtime surface 均未改；真实语义质量、Runtime 采用、主人验收及旧屏障故障仍未完成，状态快照未刷新。继续沿用提交、推送和草稿 PR 流程，不自动合并或进入下一阶段。

## 2026-10-15 合成谱系结构修正

当前分支 codex/history-quality-lineage-shape，基于 PR #22 的 `c251125`。修正矩阵检查仅凭 selected_lineage 字段存在即接受的缺口；检查原生行字段、完整唯一的来源索引、父索引范围/唯一性和可为空的未选父数量，见 [报告](../validation/MV-7-matrix-lineage-shape.md)。39 项 Python、默认并行 Rust 384 项、八份原生合成产物兼容及 required checks 均通过；已保留修正前 13 个失败子例。生产 Rust/契约/依赖/接入面未改，快照未刷新。继续采用提交、推送、草稿 PR 流程；结构合格仍不认证父链或真实质量，真实资料/Provider、Runtime 激活、MV-8 和主人验收仍待独立工作。

## 2026-10-15 原生质量合成组合验证

当前分支 codex/history-quality-native-matrix，基于 PR #21 的 `9b25145`。新增 opt-in Rust 测试产物和标准库 Python 检查器，覆盖两家 Fake Provider × 无/有谱系 × 旧/完整 builder 的八种组合，见 [报告](../validation/MV-7-native-quality-matrix.md)。36 项 Python、默认并行 Rust 384 项及 required checks exit 0；八份新生成 Rust 合成产物的哈希、Provider/谱系标记和新旧材料配对均通过。所有产物在忽略目录；生产 builder、契约/依赖和 Runtime 接入面未改。

继续沿用验证、提交、推送、草稿 PR 工作流。矩阵清单是合成测试材料，不认证原生来源或提交者，不给出模型质量，native_validation 仍为 not_rerun；主人验收和旧屏障故障仍未完成。真实资料/Provider 调用、Runtime 激活、MV-8 和状态快照不在本片动作内。

## 2026-10-15 离线集合边界修正

当前分支 codex/history-quality-collection-shapes，基于 PR #20 的 `ce93561`。修复离线评估将空对象/字符串当作数组、将对象键当作 observed 路径及非字符串身份的结构缺口；已保留失败复现，见 [报告](../validation/MV-7-quality-collections.md) 和 [ADR-MEM-61](../adr/061-quality-collection-shapes.md)。28 项 Python、默认并行 Rust 384 项及 required checks exit 0；合法校准和 Rust 合成私有套件的完整报告与旧版一致。无 Rust/接入面/依赖变更，原 manifest aggregate 不变。

主人再次要求继续并推送新更新；本片沿用验证、提交、推送及草稿 PR 的工作流。不要把合法格式、supported_in_review 或 CLI exit 0 当作原生认证、全局质量/主人验收。旧有序屏障故障仍待出现时依据 trace 定位，已有成功材料不需无目标反复运行。真实资料/Provider、Runtime 激活、MV-8 和状态快照均未动。

## 2026-10-15 接续结果

分支 codex/workspace-contention-fixtures 基于下述 `e37c186`。已完成三个低层 Context 夹具的精确 WriterBusy 有界重试，真实合成写锁证明忙时不发布、原键恢复只写一次并可重放；其他错误和三次上限有回归。默认并行完整 Rust 384 项、Workspace 66 项及 required checks exit 0，见 [本片报告](../validation/MV-7-workspace-contention.md)。Runtime watched tests 的 manifest 已更新，产品适配和生产重试策略不变。

补充诊断后完整默认并行 384 项再次通过；另一次 Workspace 64 测试线程的定向诊断 66 项、7.13 秒通过，退出码均单独保存。旧有序超时仍未复现；不要无目标重复跑相同材料，将来出现故障时先读取阶段 trace。

下面的历史“方案尚未实施”仅描述 10 月 14 日状态；低层 WriterBusy 重试现已落实。有序首调用完成/双就绪的旧时限失败仍待定位，本次通过不代表其长期稳定性。已补充失败时的逐线程/屏障毫秒时序；下一步若故障出现，先按阶段定位，保持原屏障、两调用并发和旧 pin/删除断言；不要直接改生产逻辑、扩大期限或全局串行化。PR #17～20 尚未合并；合并选择仍需主人明确指示。真实资料/调用、Runtime 激活、MV-8 和状态快照均未动。

## 2026-10-14 历史接续

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
