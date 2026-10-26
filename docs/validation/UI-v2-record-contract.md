# UI v2 后端缺口修复：第一片记录契约

日期：2026-10-26。交付范围：01 §3 第一片；字段等待 Morii 确认。

## 基线与隔离

- 已 fetch 核实 origin/main 为 `c9bf1be7819dba19915adf6031b1900c032b73d1`，
  不是直接采用交接包中的旧观察。
- 自有分支：`codex/ui-v2-record-contract`；自有独立工作树位于原工作区 ignored
  `.local/worktrees/ui-v2-backend` 下。原 main 工作树与主 checkout 的未提交内容未纳入本片。
- 只读 Git 树确认并行 MCP 使用 ADR-MEM-92；本片使用 **ADR-MEM-93**。
  没有修改 MCP 工作树、Runtime 仓库、Memory pin 或手动项目状态。
- 只用合成 fixtures、新建临时 NTFS 根、自有测试进程。没有读取私人 Vault、
  导出、凭据，没有调用真实模型/embedding，也没有部署或真实初始化。

## 格式与字段对应

新根布局 **`automation-host-v3`**，document schema **2**，准入规则
**`admission-rules-3`**。旧根不原地升级；旧布局拒收新字段，旧消费者拒绝新布局。
完整字段表、限制与解释见[记录契约](../contracts/UI_V2_RECORDS.md)；决定见
[ADR-MEM-93](../adr/093-ui-v2-record-layout.md)。

| 字段组 | 实际位置/形状 | 01 / 02 对应 |
| --- | --- | --- |
| 标题与语言 | AdmissionInput.record_fields 的 title/content_lang；来源也有 content_lang | G02/G17；02 §3、§4.1 |
| 事件与有效时间 | 必需 RecordTime：from/until/precision/bound/tz/basis；与 Core-5 区间一致，未知不补日期 | G10；02 §4.2 |
| 实体与参与者 | entities[] 的稳定 ent ID/role/证据；同提交内有带修订的实体目录、来源别名、敏感度、访问策略和虚构/真实区分 | G09；02 §4.3 |
| 证据 | 既有 EvidenceRef + 精确 quote/speaker/Unicode scalar [start,end)，解析至具体来源修订及其消息定位 | G14；02 §4.4 |
| 形成来由与附注 | prompt_version/import_batch_id/mode；附注绑定记忆修订、模型和提示词版本，情绪/重要度不是事实 | G19 的记录部分；02 §4.5、§6 |
| 对话与轮次 | 同提交内追加修订的 conversation、participants、branches、turn/source refs、起止时间、捕获轮次数与覆盖说明；来源保留 assistant_model | G14；02 §3 |
| 主人动作 | 独立 plan/confirmation/record；v3 根现在就有空账本与撤回水位；System 事务不能改账本，owner_action 执行仍拒绝 | G08 的格式；02 §5 |
| 投影共同规则 | algorithm/version/snapshot/scope hash/withdrawal watermark、覆盖及精确依赖；任何屏障变化使缓存失效 | G18；02 §7 |

实体/对话目录随来源在同一见证提交中追加；不存在单独的权威 sidecar。
标题、证据与附注进入请求/决定/响应/提交哈希。默认来源捕获仍只有 runtime_event；
导出/Agent 采集和主人 manual_assertion 的可信入口分别由后续会话/G08 实现。
空主人账本预先冻结，避免等 G08 才向已初始化数据格式增加字段。

## 实际验证

固定 Rust 1.98.1 / x86_64-pc-windows-gnu，Cargo 全程 CARGO_NET_OFFLINE=true。

| 检查 | 最终结果 |
| --- | --- |
| cargo fmt --all -- --check | 通过 |
| cargo test --workspace --locked | 670 通过，0 失败，0 忽略；75 个 test/doctest 组 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过 |
| Python 3.12.10 / jsonschema 4.26.0 独立 cross-check | 51 个 Schema；UI 语料 19 正例 + 31 反例；无分歧 |
| runtime_surface.py --write 及检查 | 已生成且登记，aggregate 见下 |
| git diff --cached --check / 公开内容检查 | 无空白错误；自有文件中未发现个人绝对路径或凭据形状字符串 |

集成面 aggregate：`8e9fc69d931c86f14bbd7501f7cb806f5146d7e6103c13c26cbc45d7fdbb108d`。
原生定向测试及独立 Schema cross-check 也实际运行过；日志保留在工作树 ignored `.local/`。


新增语料为 **19 个正例、31 个反例**，由 Rust 子集 Schema validator、原生类型与
独立 python-jsonschema 共同核对。新增 7 个契约测试与 2 个实际文件/事务测试覆盖：

- 中文/emoji 的字符定位、错误 quote/offset/speaker、来源修订/对象锚点；
- 未知时间、区间 precision/bound、语言子集、形成批次与附注版本绑定；
- 三消息分支/轮次定位、缺失来源、同名实体不合并、计数与敏感度闭包；
- 所有主人动作形状；System/Agent/Provider/Client 冒充主人、计划内容/快照变更、
  过期确认均拒绝；有效 Owner 标签的记录也不能夹带进入 System capture；
- 附注中的本机生成 result ID 不改变同一输入的逻辑任务键，实际请求/提交哈希仍保留完整绑定；
- 新根原子保存字段/目录、拒绝缺字段与不精确引文、失败不推进 head、原键重放、
  关闭重开保持原见证回执；删除来源后回执复用与召回都拒绝；
- 旧布局拒收新字段、旧布局枚举及 v1 descriptor parser 拒绝新布局/主版本，旧 fixtures 的哈希保持有效；
- 投影快照/策略/删除/撤回/范围/算法/版本任一变化使 envelope 不可复用。

首次文件测试在受限沙箱下因 Windows owner-only ACL 返回 StorageFailed；
获准环境中同一临时合成根测试通过，没有放宽生产 ACL 校验。
此前一次完整 workspace 检查通过后，补全了空主人账本门禁及字段反例；
因此在最终代码上重新执行完整 workspace 检查。最终结果以上表为准。

## 与需求的差异及未交付能力

1. quote 使用原文精确匹配，不做需求中建议的空白/全半角归一化回退。
   找不到精确原文就拒绝，不编造可定位的证据。
2. content_lang 接受有界 BCP 47 子集 language[-Script][-REGION][-variant...]，
   未支持 extlang、扩展、private-use 和 grandfathered 标签；未知用 und。
   时区校验 IANA 风格名称语法，不含时区注册表或历史偏移换算。未替 Morii
   选择默认时区；初始化/投影会话需明确记录主人选择及 basis。
3. tone/importance 用千分整数，均可空，仅作建议；title 与 content 的语义一致、
   语言检测质量、一条一陈述不能靠结构校验证明，后者仍归 G21/S2。
4. 本片保留 Core-3 的规模上限；对话读取接口、七种投影、主题标注重叠继承、
   主人入口执行及操作查询尚未实现。空账本/投影 envelope 不是已执行的主人动作
   或已建好的投影。现有 Core-7 端口仍只接受 HostV2，不能冒称已读 v3 数据。
5. 没有迁移旧主人审核历史、运行真实模型、接 MCP、升 Runtime pin 或进行 UI 验收。
   本仓库既有 ADR 的 System/Owner、来源、删除与权限边界始终优先。

## 停止点与后续

本片验证后 commit/push 自有分支，不合并 main。**等待 Morii 确认字段**，
确认后才继续 G01 规模 → G06 检索/筛选 → G23 统计，再按原顺序推进时间/主题/图、
过程读取及最后 G08。S2/S3 应采用本片的确切提交与字段，不能把本片作为
真实导入、模型外发、MCP 采集激活或 Runtime Core-8 切换的授权。
