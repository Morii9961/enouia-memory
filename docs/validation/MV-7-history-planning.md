# MV-7-H1 · 历史初始化规划与云端路线对齐

验证日期：2026-10-11；提交收尾：2026-10-12。起点：`main@a62ca4d2c6f53a7116e4bc5b33ac77a42e07e9f4`。本报告记录 H1 实现及验证，提交标识以 Git 历史为准。开始时已有 AGENTS.md 修改及 docs/status/ 基线，本次保留它们，未更新手动快照；它们不属于本次 H1 提交。

## 范围与结果

[ADR-MEM-49](../adr/049-cloud-primary-and-history-initialization.md) 和 [设计包 v1.3](../design/CLOUD_PRIMARY_AND_HISTORY.md) 把目标对齐为云端完整记忆单主、本地初始化与恢复、MCP 回忆和候选回传。更新计划、设计入口和验收出口；当前仍是本地实现。H1 是 MV-7 的离线准备工作，没有启动 MV-8、部署、真实资料导入或模型外发。

新增 `crates/enouia-memory-import/src/history.rs`：复用既有 ParsedInput 和来源 locator，生成时间导航、未知日期列表、按根到叶路径分批的计划与缺口报告。保留助手/用户证据分类，分支选择不等于批准；坏图不给提炼批次，覆盖数据仍保留。共享祖先重复引用且受总预算限制。模块无文件、Vault、秘密、模型、网络或批准端口；没有新 Schema、持久化记录、IPC 命令、依赖或 UI。

合成 fixture 为 `tests/fixtures/history/initialization-synthetic.json`，包含 2023～2026 的偏好纠正、计划取消、未采纳助手建议、编辑分支、未知时间、缺附件、无法解析节点及指令注入文字。样例固定读取编译时嵌入的 fixture，没有真实文件输入参数。

## 验证记录

- 固定 Rust 1.98.1、离线、GNU Windows；MinGW UCRT64 在 PATH 前。Python 3.12.10 的仓库虚拟环境使用已安装的 python-jsonschema 4.26.0，无依赖安装。
- `cargo test -p enouia-memory-import --test history --locked`：9/9 通过。覆盖时间对账、分支/祖先/批次、来源 hash、角色保留、未知选择、坏图和断开的环、时钟倒退、总引用限制、空及非树输入。
- `cargo fmt --all -- --check`：通过。
- `cargo test --workspace --locked`：343 项通过，0 失败（含本次 9 项）；退出码 0。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：通过，退出码 0。
- `python tools/schema-check/check_schemas.py`：python-jsonschema 4.26.0，36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases；OK。
- `python tools/integration/runtime_surface.py`：既有 aggregate 已登记，通过；`--write` 重生成得到同一 aggregate，manifest 无差异。
- 内置 `plan_history` 样例运行成功：6 条已知时间、1 条未知时间、1 个无法解析节点、1 条部分内容、1 个不可用附件；6 个小批次、9 次来源引用，两个项目分支分别保留。
- 文档检查：17 个修改/新增文档的 187 个本地链接可解析，代码块成对，未发现本机绝对路径或所查 credential 形状；`git diff --check` 通过。两份原有状态快照 SHA-256 前后相同。

环境例外：首次沙箱内 rustfmt 返回 Windows 拒绝访问；完整测试首次在 Tauri build.rs 中因同类 OS error 5 失败。保持实现不变，在获准的沙箱外用相同固定工具链重跑后，格式化、完整测试和 Clippy 均通过；不把最初失败当成产品逻辑缺陷。

新增测试使用 parser 和真实 locator 解析核对原始合成字节；模型建议文字不构成语义质量通过证据。H01～H03 为 H1 合成范围，H04～H06 和 CP01～CP04 均 pending。

## Runtime 与限制

Runtime 集成面仍为 `5eb04c2bdb66950dcd6d59a84cf89a7e3976243cf3de8acd902e0db64346416e`，47 条命令，当前宿主行为未改变。H1 未暴露为工作区操作，不要求 Runtime 新增产品入口；将来 Host/cloud 改传输必须独立记录版本与采用要求。

本次不实现跨时间模型提炼、自动合并、主题回顾、可恢复任务账本、实际 MCP/云端服务或真实三年规模性能。内存 MessageRef 只在配对解析结果中有效；消息数不是 token 预算。收到的观测时间范围不证明平台历史完整。Windows 测试不证明 Linux 云端持久化；加密备份和真实双模型验收仍 pending。

## 下一工作包与回退

H2 的边界是归档来源版本绑定、可恢复批量提炼、纠正/取消/分支冲突建议和合成质量问题集，继续人工审核。真实 Provider、个人导出、独立恢复资源在对应激活点由 Morii 选择。之后 MV-8 需单独明确启动。

回退只涉及本次 planner、example、fixture、测试与路线文档；没有生产数据迁移或格式修改。保留已存在的 AGENTS.md 和手动状态快照，不以撤回 H1 删除其他工作。
