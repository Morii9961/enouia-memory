# MV-7 · 合成历史批次上限与恢复

日期：2026-10-15；基线 `422399d`（PR #23）。本片为既有 [H2a](../adr/050-archived-history-extraction-batches.md) 增加三项原生合成边界验证，不改变生产实现、限额或持久化格式。

## 覆盖范围

| 合成材料 | 检查结果 |
| --- | --- |
| 33 条线性消息，每批 1 条，显式逆序选择 32 批 | 33 批选择以 InvalidRequest 拒绝，head 不变；32 批在第 16 个子任务保存后取消，重新打开后续建为 32 个唯一任务和 33 个 Session。已保存任务及其状态保留，选择顺序与每批前文正确；再次原键重放，完整结果和 head 不变。 |
| 31 条线性消息，每批 15 条 | 三批分别保留消息 0～14、14～29、29～30；第二批含 15 条新消息和 1 条前文。所有所选正文和 user/assistant 角色逐条核对，31 个协调依赖去重保留。 |
| 29 条共享前文和 32 条分支末消息，共 61 条来源 | 选取 32 个含前文的分支批次，共 512 个片段引用。整体协调元数据超限时返回 BudgetExceeded，head 不变且没有 Session；缩小为一批后，同一未占用键可创建含 16 个片段的任务。 |

32 批只是数量上限，不能保证 32 个满批同时满足 128 KiB 协调元数据上限。测试保留显式拒绝，不缩短正文、不丢前文、不自动拆分或改选。未选择的第 33 条消息不会混入第一组协调依赖；批次 Ready、应用游标 0 和无 Provider 调用记录不表示已经抽取或理解内容。合成 Vault 最终一致性检查通过，第一组没有 Candidate 或正式 Memory。

## 验证记录

初步三项定向验证通过；32 批场景含生成、导入、中断、重新打开、补建、再次重放和一致性检查共 5876 ms，分支元数据场景 1962 ms。这是同一次本地合成运行的观察值，不是吞吐基准、持续负载、真实三年资料的容量或工期估算。随后补充正文集合长度和角色断言，最终版本由下述完整检查覆盖。

最终版本在仓库根使用 Rust 1.98.1 / GNU Windows / offline：fmt 检查、默认并行完整 Rust 387 项（原有 384 + 新增 3；history 13、Provider 78、Workspace 66）、严格 Clippy 均 exit 0。Python 3.12.10 / python-jsonschema 4.26.0 独立交叉检查 36 schemas 与 Runtime surface 检查均 exit 0。Clippy 有增量缓存硬链接失败后回退复制的环境提示，检查正常完成，没有修改缓存策略或产品代码。

四个公开文件、七个本地文档链接、路径/credential 形状、两份状态快照 SHA-256 及 git diff --check 均通过。退出码单独保存：mv7-batch-boundary-rust-exit.json、mv7-batch-boundary-clippy-exit.json 和 mv7-batch-boundary-python-checks.json；对应完整日志与定向日志为忽略的本地 mv7-batch-boundary-* 文件，临时合成原件与 Vault 不提交。

## 集成与仍待验证

测试入口为 crates/enouia-memory-provider/tests/history.rs，独立场景位于 cases/history_scale.rs；只使用本仓库合成生成器、现有导入/规划、Vault 和 Session 内嵌 ExtractionJob，没有网络、凭据或 Provider 发送端口，没有新增记录或修改契约。

Runtime 接入面仍为 47 commands，aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 已登记，无新产品适配要求。实际模型忠实性/漏提、全量成本与规模、真实双 Provider、Runtime 产品采用及主人验收仍未完成；旧并发屏障故障也不由本片证明修复。状态快照保持原样，真实资料/Provider、MV-8 和部署未启动。
