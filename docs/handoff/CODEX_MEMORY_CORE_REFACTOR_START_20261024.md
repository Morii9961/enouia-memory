# Codex 正式后端 Session 启动提示词

> 历史材料（2026-10-24，2026-11-02 补入版本管理）。实现状态、暂停与授权只描述原日期；当前事实以主线实现、分阶段计划及最新验证报告为准。本文不启动任何工作。

用途：Morii 在 **Enouia Memory 的实际实现 checkout** 中开启新的 Codex Session 后，直接发送下方完整代码块。它授权那个新 Session 完成 Core-1 的独立合成实现；本文件作为被读取的文档本身不启动实现。不要把它交给 Claude 当 Runtime 的工作指令。

```text
你正在 Enouia Memory 仓库承担正式后端开发。Morii 已确定目标：完全自动化的记忆形成、准入、纠正、巩固、情境召回与自然淡出，日常无需Morii逐条审核。人工纠正是可选控制，不得把不确定结果堆成必须由Morii处理的队列。

本Session授权范围：完成 docs/design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md 的 Core-1“自动准入契约、版本矩阵与纯规则”独立合成实现切片。Core-0只作短前置核实，随后实际完成Core-1；不要停在再写一套宏观计划。阶段内可自主处理必要兼容和测试修复。完成后提交、推送本片owned files并报告，然后停止；不得自动进入Core-2及后续切片，不自动merge/deploy或升级Runtime。

先读当前AGENTS.md、docs/design/IMPLEMENTATION_PLAN.md的最新优先级、相关ADRs，以及：
- docs/design/memory-evolution-2026-10-24/README.md
- docs/design/memory-evolution-2026-10-24/06-automation-and-ombre-migration.md
- docs/design/memory-evolution-2026-10-24/07-core-refactor-and-runtime-route.md
- docs/design/memory-evolution-2026-10-24/01-current-core.md
- docs/handoff/memory-ui-2026-10-24/RUNTIME_BOUNDARY_AND_INTEGRATION.md
- docs/integration/RUNTIME.md
路径先核实。材料的实现基线是Memory 9286cd8f89669d9ec3b92f943fd74072457536e7，真实checkout可能已变化，必须先比对，不以旧交接覆盖新实现。

这些交接文档当前可能仍是原checkout未提交文件，新建Git worktree不会自动带入。若隔离工作树缺少它们，先从Morii提供的完整包或原checkout受控复制公共文档到新树，保留原树不动；不能只取HEAD后因为找不到文档就重写路线或忽略授权边界。本机源位置在ignored交接说明中。不要把参考图、私人附件或.local整包加入Git。

工作区保护：核实branch/HEAD/status/worktrees与并行工作，列本次owned-file allowlist。已存在的AGENTS.md、docs/status/PROJECT_STATUS.md、docs/status/project-status.json及他人的未提交材料不能顺手纳入提交，不reset/clean/覆盖。若共享checkout有冲突，依应用规则复用适合的已附工作树或建立隔离工作树；创建分支使用codex/前缀。不要改Claude的Runtime工作树、共享App/main/router/token或任何非Memory功能。

优先级：暂停MCP新工具、客户端认证/接线、远程开放和更广Context对外暴露。保留现有MCP/Host交付与旧语义，不删除。既有MCP不是新机器写入的捷径。产品UI仍在Runtime，apps/workspace仅参考壳。Runtime当前redesign/explore处于设计探索，仍pin旧embedded Core；不要为本片升级它、迁Host或让它帮助补领域逻辑。Activity/Usage绝不进入Memory，也不作为后台认知的数据源。

本片具体交付：
1. 先冻结最小ADR/契约：真实System主体、机器准入决定、版本化策略、精确输入/目标修订、支持证据与context依赖、结果引用与提交关联、知识状态和采用状态。普通自述自动采用为“本人说过X”，不等于客观独立证实；推断/含糊/冲突可暂存并自动重评，不以真人审核为必经出口。概念名称不是预定wire schema，请依现有项目模式确定最小正式类型。
2. 明确V1/V2 reader/writer和消费者矩阵，保留旧owner记录的原字节与批准历史；不伪造owner/ReviewRecord/confirmation。当前SchemaVersion仅支持1、RecordKind/AnyRecord闭合，unknown-major只提供原字节诊断，不代表业务读取新格式。不得机械改全局版本常量，也不得把新kind塞入旧库后声称旧客户端会忽略。
3. 同一切片实现隔离的新类型、验证器、纯准入转换及合成正反例。优先现有crate内清楚隔离的next-version模块；只有实际需要才新增crate/依赖。不得改变当前默认写入口、真实CURRENT、OWNER_ONLY保护或现有v1 IPC含义，不发布实际System写库，不执行旧候选批量晋升。
   结果/提交关联在本片只是决策草案的纯绑定验证，不产生实际成功回执；真实提交回执由后续合法事务生成。
4. 说明未来作业和Runtime最小读模型需要的能力/状态映射，但不在本片实现后台worker、完整时间轴/星图、向量检索、持续Provider策略、通知或新的MCP工具。
5. 本片保存版本/迁移/回滚设计与验证报告。对旧Runtime pin与旧MCP读取新格式如何在解码/查询前拒绝给明确规则；不要等页面最后隐藏字段。真实新库迁移及实际运行绑定留给后续切片。

验证重点：旧v1记录与owner审阅语义保持；System不会冒充Owner；版本不支持时拒写；缺证据/上下文依赖、越范围策略、过期目标修订、同源重复支持等负例明确。只实现机器决策的结构/纯规则，不宣称模型已能自动理解所有语义。Schema/validator实际修改才跑schema cross-check；集成面实际修改才在同一实现提交生成runtime-surface并登记RUNTIME.md。跨workspace契约若确实受影响，按AGENTS运行必要全套；不要为重复宏观评估或文档编辑跑无关build/smoke。

外部参考：复用研究包固定SHA，不重新无边界研究四仓或执行外部脚本。Ombre原版基线可后续在隔离合成实验中采用；本片不连接真实Provider/embedding、不读私人Vault/聊天导出/凭据、不部署、不迁主，也不建立生产旁库或双写。没有这些资源不是Core-1的阻塞，使用合成fixture与纯规则完成。

完成后检查公开内容，提交时在空行后加：
Co-authored-by: Codex <267193182+codex@users.noreply.github.com>
仅提交并推送本片owned files；不包含主人/他人的改动，不自动合并。报告commit/branch、具体变更、实际检查、未实现项、Runtime采用影响及Core-2前置，然后停止。不要刷新手动项目状态快照，不创建跟踪自动化或关机任务。
```

继续后续阶段时，Morii 只需明确领取对应 Core-N，并保留范围、文件保护与验证规则；不要每次重新询问是否接受全自动产品目标。
