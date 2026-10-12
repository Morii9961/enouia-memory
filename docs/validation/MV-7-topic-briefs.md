# MV-7-H2c · 带来源覆盖的主题导航回顾

日期：2026-10-12。范围：[ADR-MEM-52](../adr/052-source-linked-topic-briefs.md)。H2b 已通过 PR #9 合并到 main（`88e1e18`）。本片接续 MV-7，未启动 MV-8。

## 实现

历史审核保留每个选定任务的响应 hash、应用状态/游标、来源修订/片段/hash、完整选定原文和引用该片段的建议索引。来源重新按当前 SourceRead 权限解析，source/snippet hash 必须等于 job 保存值。零候选响应及没有被建议引用的片段仍列出；重复片段的出现次数与唯一片段数分开。这只说明已选完成响应的覆盖，不表示收到的全部历史或重要信息都已处理。

`history_review::topic_brief` 生成中文 Markdown 来源导航，允许确切 TopicKey 筛选；没有匹配主题仍保留覆盖范围和缺失信息提示。每个主题列原始建议/引用、当前候选修改/状态及 H2b 比较线索。所有导入、模型、主人文字序列化在 JSON 代码块里，不能成为报告标题、HTML 或新指令。默认 128 KiB，硬上限 2 MiB；有界 writer 超限拒绝整个结果，不截断。返回 base commit、文本 hash 和内存文本，不写文件、不调用模型、不新增正式记忆。

## 发现与修正

多行原文案例最初在准备请求阶段被拒绝。定位到抽取 query 本来已是 JSON，同一输入又作为 recent Turn 文本渲染进上下文 JSON；二次转义产生双反斜杠，被既有保守 UNC 扫描误判。仅对 Extraction 的 rendered turn 使用固定占位文字，完整原文保留在 user query；存储 capsule 的 Turn 文本、event IDs 和来源依赖完整保留，仍通过 native exact-input 校验。其他 Purpose 渲染不变。曾尝试改变存储 Turn 文本，被现有输入绑定拒绝；最终实现只调整渲染。

来源选择、secret/path 检查和准备/发送的 exact-body guard 均未放宽。真实路径/credential 形状即使位于多行来源中仍在创建抽取任务前拒绝，不写 job 或请求，也未访问凭据。请求准备后采用新 body/hash 和正常批准；旧已保存 capsule 重建不匹配的旧请求仍应拒绝，不自动迁移批准。

## 合成验证

四组新增用例和六组 H2b 回归共十组定向测试通过：

- 两家假 Provider：换行、反引号、标题、HTML 与“自动批准”作为合成原文保留在 JSON 数据中；解析出的报告代码块数量固定，不插入新节。零候选任务明确有一个未引用来源、未知日期和独立游标；审核不写候选或 Canonical。
- 空主题筛选保留两个来源片段和“没有匹配不证明不存在”的提示。输出 cap 少一字节拒绝，精确 cap 返回同一文本/hash；非主人和锁定拒绝，head 不变。
- 两个批次重复引用祖先，统计四次片段出现、两个唯一片段及两个未引用片段。原陈述、主人修改和 ExactRepeat/SamePath 线索并存，不成为两个正式事实。
- 合成多行中的真实路径形状和 credential 形状仍拒绝。所有假请求检查存储 Turn 仍等于原 query，实际 rendered user query 完整，system context 只保留抽取占位文字。

Rust 1.98.1、GNU Windows、离线全仓库验证退出码均为 0：`cargo fmt --all -- --check`、`cargo test --workspace --locked`（364 项通过，0 失败）、`cargo clippy --workspace --all-targets --locked -- -D warnings`。完整测试和 Clippy 输出留在忽略的本地日志中。

Python 3.12.10 / python-jsonschema 4.26.0 独立校验 OK：36 schemas、53 valid records、117 record cases、4913 set records、51 IPC、83 workspace、46 store、8 invocation、19 extraction cases。Runtime surface checker 通过且 aggregate 已登记；`git diff --check` 通过。11 个本片公开文件及 75 个本地文档链接核对通过，所查个人绝对路径/credential 形状未命中，两份状态快照 SHA-256 未变。未安装新依赖。

## Runtime 与后续门槛

集成面为 `5ca6efdcfb3b452dc63f4c68707cd0c7637eacc26109b6ebf397ac7d841fa34a`，采用要求在同片 [Runtime 日志](../integration/RUNTIME.md) 登记。47 条 workspace 命令、Schema 和持久化格式不变，Runtime 尚未采用此原生 helper；产品入口归 Runtime。

回顾是确定性的来源导航，不是模型派生摘要。语义新增/纠正/取消判断、主题摘要质量、重要信息漏提、真实双 Provider/个人材料、全量规模/成本和独立恢复仍待后续。没有私人原件、真实凭据/外发、云端部署或状态快照更新。
