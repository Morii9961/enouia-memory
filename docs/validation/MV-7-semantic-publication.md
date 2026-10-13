# MV-7-H2g · 绑定选择的提案发布与恢复

日期：2026-10-13；实现基线 main `82ca5eb`。范围为 [ADR-MEM-56](../adr/056-bound-semantic-proposal-publication.md)，不启动 MV-8。

新增可选原生 publish 入口，重读保存响应和当前来源，验证主人明确选择，再用独立命名空间的绑定回执发布一条 pending。请求 hash 覆盖任务、响应、序号、内容、动作、目标修订和 BusinessTime，排除 head 与随机 ID/提交时间。同一个已提交键换内容、动作或任务会冲突；当前 head 不符合时不新写。正式确认仍走既有 plan/confirm。

重启、主人编辑/审核以及原目标已被替代后，原选择可恢复同一候选 ID，不覆盖当前候选、不重新排入 pending、不外发请求。DuplicateOf 沿用现有去重：不写入、不生成回执、不把新解释追加到既有候选。宿主仍须串行化发布与锁定/关闭，并正确区分去重与已提交结果。

新增两项原生测试与一项治理并发测试，并扩展既有替代/来源清除用例。双 Provider 的受控响应覆盖改键内容/动作/任务、旧 head、锁定、去重、主人编辑后确认、重启恢复、选定来源修订后的拒绝；无关来源修改不阻止恢复。BeforeCurrent/AfterCurrent 注入 I/O 失败后，分别重做未发布写入或找回已发布回执，最终只增加一次提交、一个候选。确定性暂停 ID 分配制造并发竞争，相同请求返回赢家 ID，不同请求返回 IdempotencyConflict，均不写正式记忆。

来源修订测试初版误改了任务之外的项目消息，恢复成功；核对规划与 fixture 后，明确断言无关修改允许恢复，选定偏好来源修改拒绝恢复。修正后的用例通过，未改产品规则。故障证据是受控 I/O/重启 fixture，不代表断电、真实账户或模型质量验收。

从根目录用 Rust 1.98.1、离线环境及 RUST_TEST_THREADS=1 执行完整 cargo test --workspace --locked。后台完整日志记录 377 项通过、0 失败，51 组含最后的文档测试均结束；会话被打断后原 shell 退出码未能恢复，因此不将该退出码记作已捕获。日志已逐组核对，源码未再改变。用例内部的并发线程保留；上一片默认并行的两项 Workspace 写锁/调度失败仍未解决，本片未改 Workspace 或测试超时。

fmt 与严格 Clippy 退出码 0；独立 python-jsonschema 4.26.0 校验 OK（36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction）。Runtime checker 通过，aggregate 为 `90f9815b071025562bf55f5f6bd5ecff713cf8d53e605e3e9325266de020b554`。12 个本片公开文件、83 个本地链接、公开路径/credential 形状及 git diff --check 通过，两份状态快照 hash 未变。未安装依赖。原生相关 12 项定向测试、额外修正后的来源绑定用例及治理并发用例均已返回退出码 0。

Runtime surface 与采用记录同片更新；47 条命令与存储 schema 不变。真实模型质量、个人历史、Runtime 产品采用、规模/费用、独立恢复和主人验收仍 pending。状态快照未刷新。
