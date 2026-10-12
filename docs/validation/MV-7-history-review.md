# MV-7-H2b · 跨批对比与显式替代草案

日期：2026-10-12。范围：[ADR-MEM-51](../adr/051-history-review-and-explicit-supersession.md)。H1/H2a 已经 PR #8 合并到 main（`ef4ff4b`）；本片接续 MV-7，不启动 MV-8。

## 实现与边界

`provider::history_review::preview` 读取主人选定、已有完成响应的抽取任务。只按 Fact/Preference 类型、确切主体集合、明确项目及 claim_key/scope 分组；原始模型建议、完整选定原文、引用、消息时间、当前候选修订/状态各自保留。比较结果为完全重复、有日期差异或无序差异，另列同路径、分支差异、不同上下文或未知。共享祖先不被当成分支冲突；没有自动选择最新陈述或判定取消/纠正。

未知日期、非用户证据、主人内容/详情修改、已审核/撤回及抑制状态有显式标记。无日期仍未知，capture time 不代替业务时间；来源中的“尚未接受”“不采纳”保留在完整上下文。最多 32 个任务、256 条建议、512 个上下文来源和 4096 对比较，超限失败，不返回截断结果。没有额外模型请求、后台调度、数据库或协议字段。

`supersession_draft` 要求主人明确选定当前待审 Create 候选、当前活跃且来源完整的正式记忆以及 BusinessTime（可以明确 Unknown）。确切主题键必须一致，内容必须不同；未解析的新项目不能充当稳定项目身份。草案使用当前主人修订、原证据及原抽取归属，保留 base commit。公开的 `govern::propose_at_head` 复用现有发布实现，在所见 head 上发布待审提案；已有幂等回执可重放而不新写。正式记忆仅在普通可信 plan/confirm 后改变，替代链及旧记忆仍可查。

## 合成证据

六组定向测试通过，运行于两家假 Provider 和隔离 Vault；受控 claims 来自 [review-golden](../../tests/fixtures/history/review-golden.json)，原文来自 [初始化样例](../../tests/fixtures/history/initialization-synthetic.json)。这是手工固定假响应的来源/治理行为验收，不是模型理解质量。

- 过去/现在偏好按日期导航；相同祖先的重复与编辑分支/主分支差异分别呈现。取消及未采纳建议保留原文，未知日期/非用户证据不升级为确认；只读审核不改变 head，不新增 Canonical。
- 主人修改和撤回不覆盖原模型响应。非主人、关闭 native flag、重复选定任务、未完成响应和比较超限被拒绝。
- 主人指定已知或 Unknown 生效时间后，草案→待审提案→可信确认形成替代链。修改后的候选内容、证据、confidence 和原模型 actor 保留；草案本身不写正式记忆。
- 陈旧候选/目标、不同类型/主题/主体/项目、未解析新项目及草案之后的撤回被拒绝；旧 head 无法新发布，历史保持可验证。

发现与处理：最初将草案 Origin 的 actor 改成请求主人，却保留 ModelExtraction 类型，触发既有 `candidate.origin_actor` 约束。定向复现后保留完整原抽取 Origin；主人授权仍在正常审核确认层。latest 检查使用只服务最新修订的 `read_record`，不能仅依赖也服务历史修订的 `read_parsed`。测试传输改为一次完整 chunk，避免多任务治理案例写出大量无关流式账本增量；分块解码仍由既有专项测试验证。

Rust 1.98.1、GNU Windows、离线验证全部退出码 0：`cargo fmt --all -- --check`；`cargo test --workspace --locked`（360 项通过，0 失败）；`cargo clippy --workspace --all-targets --locked -- -D warnings`。完整测试与 Clippy 输出保存在忽略的本地日志中，不提交临时 Vault 或原件。

Python 3.12.10 / python-jsonschema 4.26.0 独立校验 OK：36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases。Runtime checker 的 aggregate 已登记。`git diff --check` 通过；15 个本片公开文件及 73 个本地文档链接核对通过，所查绝对路径和 credential 形状未命中，两份状态快照 SHA-256 未变。未安装新依赖。

## Runtime 与剩余工作

Runtime surface 为 `5e9752957a73fbe2f6f87d108bfe9024ad9c5f72adbc9802f3e987251e11ebac`，已在同片登记。47 条 workspace 命令、Schema 与持久化格式不变；没有新增产品界面。Runtime 若以后原生采用 helper，须更新 pin、保持主人/锁定检查和明确任务选择，显示未知时间/分支/候选状态，草案按 base head 发布并继续可信确认。Runtime 真实接入/主人验收尚未完成。

语义新增/纠正/取消判断、带覆盖范围的主题回顾、重要内容漏提评估、三年全量尺度/费用、跨导出复用来源重映射、真实双 Provider、个人材料和独立恢复仍待后续处理。本片不触碰真实资料、凭据、外发、云端部署或两份手动项目状态快照。
