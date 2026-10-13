# MV-7-H2f · 主人选定的语义提案草案

日期：2026-10-13；实现基线 main `d0dd537`。范围为 [ADR-MEM-55](../adr/055-owner-selected-semantic-proposal-drafts.md)，仍在 MV-7。

新增可选原生 `history_semantic_proposal::draft`。主人明确选择保存响应的 dispatch/hash、变化序号、提案内容及创建/替代动作； helper 重读整个已完成响应并复核当前来源、权限和 head，只返回草案。提案保留模型原陈述、解释、task/input/response 绑定和精确引文；OwnerManual 表示主人提交提案，原始模型证据仍是 model_claim，没有伪造抽取 run 或用户确认来源。事实默认为 uncertain，冲突为 disputed，偏好为 tentative；仅模型证据不能创建偏好。

替代只接受有已验证用户祖先链的纠正/取消；主人指定同主题的活跃、完整目标当前修订及 BusinessTime。消息日期不补业务生效日。既有 propose_at_head 只发布 pending，之后仍须现有可信主人 plan/confirm。宿主须为确切选择保留稳定 key，并在发布时检查 owner/lock；普通草案结构和 reason 文本本身不证明授权。

沿用现有提案去重语义：DuplicateOf 返回既有候选，不把新草案的模型解释追加到它。宿主应明确显示这个结果，不能宣称新响应来源已经持久化到既有候选；修改选择须使用绑定新选择的 key。

新增三项 Rust 测试覆盖：双 Provider 保存响应上的主人修改/模型归属、原始证据类别、伪造响应绑定/序号、空白及 secret 形状、锁定/权限、head 冲突与同键重放、仅发布 pending；冲突不能替代、当前目标修订、已知/明确 unknown 的业务时间及独立确认后的替代边；仅有模型证据的不确定解释也不能成为偏好。原有来源 purge 用例扩展到候选历史：修复前来源已清除而派生候选仍可读取；修复后编辑掉原始证据的候选新旧修订均 IntentionallyPurged，无关待审提案保留，旧语义任务及草案无法重建。失败/修复日志保存在忽略的本地验证区。

治理修复按所有候选修订的 Source 依赖计算 WithDependents 清理范围。当前候选改写为无关内容不能隐藏旧引文；主人在既有删除预览中看到扩大的候选范围，再确认清除。其他正式记忆沿用既有 broken-provenance 处理。

默认并行全量有两项既有 Workspace 测试失败：concurrent_context_session_checkpoint_replays_the_published_id 遇到 WriterBusy；full_session_ask_replays_an_input_published_while_the_call_is_paused 未满足五秒内第一条调用完成的前提。单独复测分别在 0.44 秒和 0.56 秒通过。未修改 Workspace 实现或放宽超时；默认并行稳定性仍未解决。随后从根目录设置 RUST_TEST_THREADS=1 执行完整 cargo test --workspace --locked，374 项通过、0 失败、退出码 0；这隔离不同用例，用例内部的并发线程保留。完整日志保存于忽略的本地验证区。

Rust 1.98.1 离线 fmt、严格 Clippy 已通过；独立 python-jsonschema 4.26.0 交叉检查 OK（36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction）。Runtime checker 通过，aggregate 为 `d97a45909df371aa94c02e6a2d58a8548c8d15227078575fb28b5b0607156ae1`。14 个本片公开文件、90 个本地链接、公开路径/credential 形状及 git diff --check 通过，两份状态快照 hash 未变。未安装依赖。

初版测试缺少类型导入、误用 BusinessTime 构造方法，编译后已修正；一次重编译因先前仍在运行的 Windows 测试程序占用同名 exe 失败，等待结束后复测通过。受控响应只验证来源、治理与拒绝行为，不证明模型理解或实际质量。

本片没有新增 wire 命令或存储 schema。Runtime 可选原生采用需要更新 pin 并按接入日志接入；产品界面、真实模型和个人历史、规模/费用、独立恢复及主人验收仍 pending。状态快照没有刷新。
