# MCP-5 session_start 接续（2026-11-05）

7f98d2b 已推送部分输出/flush 验收。其后复现 catalog 筛选七个枚举字段的
对象标签违反既有可空字符串合同，共 28 条路径；已在 MCP 筛选边界最小修正。owned
allowlist：mcp/json.rs 与 initialized/catalog/qualified SearchArgs、opening_mcp.rs /
opening_pipe.rs 及两相邻读取定向测试、三个 catalog 型合成 manifest、ADR126/
合同、本交接/read-control 报告及 integration manifest/RUNTIME.md。
不改领域 CatalogFilter 本身，不扩展 v2 独有筛选字段，不动游标/模型/真实配置。
三个 catalog 型适配器共用 json::catalog_filter，先约束七字段表示再构造原类型。
开场 10 项、相邻直接入口 2 项、实际管道/stdio 3 项、surface 4 项通过；合法
字符串/null/缺省保持可用，拒绝不增加 native search 计数，快照/业务字节不变。
独立 Python 77 schemas、三个 manifest 各 29 件通过；定向 Clippy/fmt/公开检查
通过，无全量。当前 aggregate ecc8332a5025d9b87b776d2bdb2e85400efdd49e5fe8bb3032509c9a2452a455。
schema/hello 指纹未改；仍不声称领域所有嵌套 enum/游标均已完整 schema 等价。

ba97e17 已推送同一记忆修订资源生命周期验收。其后已补 stdio 输出部分写入/
flush 失败的合成故障注入，证明完整首包保留、停止后续输入、实际断开撤权。
owned allowlist 仅 opening_pipe.rs、本交接与 read-tool-links 验证报告；不改
生产代码/schema/surface，不补跑无关检查，保持 -j 1。已经写出的字节无法撤回，
不得把传输不确定误称为“未写出任何字节”或 Memory 发布成功/失败。
新单项覆盖 memory_read / memory_recall 的 partial / flush 四场景，通过；旧的
写入前失败用例也单项通过。完整原 Value/原键保留，不消费后续输入，实际断开
并撤权；主动 Closed 探测未重发或覆盖首包。partial 是半条 JSON，flush 场景
完整含链接 JSON 已在缓冲区，不能撤回，不冒称无输出。定向 Clippy/fmt 通过，
不重跑未变的 schema/surface；详见 read-tool-links 报告对应小节。

b9bc1c9 已推送共享元数据形状修正，草稿 #85 已更新。其后已补现有资源承诺
的修订生命周期证据：同一 memory_id 的 System 更新后，记忆链接读当前修订，
旧 recall 链接 stale；显式新召回后读当前结果。新证据必须来自已保存的 Runtime
UserStatement 原话，不使用 fixture.revise（该辅助函数另造来源），不调用
NativeMemoryOwner。owned allowlist 仅 opening_tool_links.rs、本交接与 read-tool-links
验证报告；无实现/schema/surface 改动，不重复其独立检查，保持 -j 1。
单项修订生命周期测试通过：使用实际 Runtime 捕获原话更新同一记忆到 revision 2，
原 memory 链接读取当前修订；旧 recall 仍 stale，只有显式新召回替换旧缓存。
scope_hash 保持，SourceRead=false，读取不改更新后快照、不增 capture/Owner 动作。
定向 Clippy/fmt 通过，无全量；见 read-tool-links 报告的修订生命周期小节。

f514016 已推送读取工具资源链接，草稿 #85 已更新。其后复现共享会话层的
非对象 _meta 在 20 条路径被接纳/推进初始化，已在 wire-session 边界最小修正。已有
ADR126/合同先补充。owned allowlist：read_session.rs、新 opening_metadata.rs、
opening_pipe.rs 必要实际管道用例、ADR126/合同、本交接/验证报告、integration
manifest/RUNTIME.md。只处理元数据容器形状，不解释其权限，不改 legacy MCP
独立入口，不新增协议特性、模型、Owner 动作或队列。
ReadSession 对元数据容器只检查一次；合法对象不改写、不授予权限。元数据/开场
13 项、实际管道 10 项、原 note 保存回放 1 项及 surface 4 项通过；独立 Python
既有 77 schemas/语料通过，不冒称其覆盖新元数据状态机。定向 Clippy/fmt/公开
检查通过，无全量。未知状态优先于元数据拒绝：_meta:null 原包仍完整保留、断开。
当前 aggregate 4cb740a0f0daab6363df2c07734a796b2c7e0e1a26b85979c45a0fb14ec8d11a，
schema/hello 指纹未改。报告见 [元数据边界](../validation/MCP-5-metadata-boundary.md)。
曾因空间不足链接失败；只删除本工作树 target/debug/deps 中四个有新替代、未占用、
路径/标记核实的旧测试 exe（约 1.3 GB），未删目录/数据/.local/其他 worktree。
重跑 -j 1 已通过；后续保持单编译任务，勿把磁盘错误或首次缺登记检查写为通过。

2026-11-06：dab0160 已推送 catalog/qualified 控制字段修正。其后沿 ADR126
资源路线，已为 context profile 的成功 memory_read / memory_recall 添加标准
resource_link；先冻结上述 ADR/合同补充，再实现。memory 链接为当前独立授权
详情，不承诺复现原工具分页/来源全文；recall 链接仍是同 reader 实际缓存。
owned allowlist：opening/mod.rs/native.rs、新 tool-links schema/corpus、
opening_tool_links.rs / opening_pipe.rs / 合同测试、Python schema checker、
ADR126/现有合同、本交接/新验证报告及 runtime-surface.json/RUNTIME.md。
旧 profile、手动状态、AGENTS 和其他 worktree 不改，不新增读取或写入授权。
当前四个 Host 目标分两组共 25 passed、合同/表面 9 passed；独立 Python
77 schemas / 23 新样例 / 5 实际工具响应通过源合同及方法专用 bundle。定向
Clippy、fmt、surface、公开检查通过，无全量。空的有界召回仍有实际缓存链接；
missing/error 无链接，原生结构化数据/文本保持，旧 profile 仍仅原文本块。
当前 aggregate 为 20fcbe005023e0070acd996c5e72436809d806858ec815287d02e1f1793687f6。
详见 [读取工具链接报告](../validation/MCP-5-read-tool-links.md)。新指纹绑定两份
完整响应 schema；必须匹配端点，不自动回退/重连。不是 Runtime 或真实模型采用。

c346838 已推送 initialized/v2 读取控制字段修正，草稿 #85 已更新。其后已复现
并收束 catalog/qualified 的同两字段。owned allowlist：这两个 MCP mod.rs、
segmented_agent.rs / qualified_mcp.rs 对应定向测试、agent-catalog/qualified-read
合成 manifest、本交接/read-control 报告、integration manifest/RUNTIME.md。
不改领域全局 enum，不调用 NativeMemoryOwner，不改变有效工具/权限/布局/指纹。
2026-11-06 接续：这两个入口各复现 8 个反例后复用 string_enum。直接入口 2 项、
实际 stdio 2 项、surface 4 项通过；独立 Python 76 schemas、两个 manifest 各
22 件通过。定向 Clippy/fmt/surface/公开检查通过，无全量。此前 Clippy/surface
因审批检查额度耗尽未执行，现已同一步重试通过，未把失败审批写为验证通过。
当前 aggregate 为 2c856f8deb96d9dd8bafc68ee4a8f97bd6e66f2e28808aa11cb56af768a38520。
四个读取 adapter 的 operation/intent 对象标签缺口已收束，未宣称领域所有 enum
均完成合同一致性验证；详见 read-control 报告的 2026-11-06 小节。

本片从 2fde3f7 接续：反例确认读取 operation / intent 的字符串合同被 serde
对象标签绕过，context/initialized 共 16 条路径已复现并最小修正。owned
allowlist：mcp/json.rs、initialized/mod.rs、v2/mod.rs、opening_mcp.rs、必要的
对应读取测试/合成语料、ADR126/合同、本交接/验证报告及 integration manifest /
RUNTIME.md。不改 AGENTS、手动状态或其他 worktree，不新增方法/权限/持久化。
实现为 json::string_enum，在 initialized/v2 的两个控制字段处要求字符串，再
传原类型向内。Host 12 项、实际管道 3 项及 surface 4 项通过；独立 Python
76 schemas（两语料分别 18/22 件）通过；定向 Clippy/fmt/公开检查通过，无全量。
本片 aggregate 0a969a802b2d919f209cb976662b2e9f3112d689fbd1ad5aaad9d5417f2a0c76，
schema/hello 指纹未改。报告见 [read-control](../validation/MCP-5-read-control-strings.md)。
提交后接着核实 catalog/qualified 相同字段，不做领域全局 enum 重写或新 MV。

## 当前基线与授权

Morii 要求沿既定路线自主继续，不逐项索要指令；已明确启动 MCP-5 合成开发，
其他硬边界继续保留。当前分支 codex/mcp5-opening-resources，最新实现提交
c346838 已推送；其后的 catalog/qualified 修正以本文件开头及 Git 为准。
草稿 PR #85 仍以未合并的 MCP-4 分支 / PR #84（4aa1605）为
base；不合并。origin/main 为 PR #83 的 2a6310e，包含 f42f3b1，已核对远端。
其他 main / captured-source-lifecycle worktree 保留；无 reset/clean。

本次从自己的 399a217 接续，保留 AGENTS 与手动状态快照。主仓库 MCP-4 本机说明
存在，私人数据和真实工具配置未打开。Runtime 并行工作仍以它自己的交接为准。
本文件收束重复记录，本次文档整理只拥有本文件，不改实现或状态快照。

## 本轮已交付

| 提交 | 内容 |
| --- | --- |
| bf4a747 | 先冻结 ADR127 / AGENT_SESSION_START_MCP_V1，再实现可被 Agent 调用的只读 session_start |
| 9bd2b5d | 标准 resource_link；独立完整成功响应合同，纳入 exact pipe-10 指纹 |
| e6e95d2 | prompt / tool 双入口形成后旧链接失效、同连接显式新读的实际管道验收 |
| 310d5ba | 实际可执行文件 stdio 透传、链接取回及 EOF 撤权验收 |
| 0d2f25f | 旧 pipe-5 / pipe-6 拒绝新工具并保持原读取 / note 保存能力 |
| c9eb8f4 | 复现并修复 kind 对象形状被 serde 单变体枚举错误接受的问题 |

session_start 复用同一 QualifiedAgentReader 与 memory_start 的有界召回：
min(4, profile.max_items) / min(16384, profile.max_bytes)，返回原 request_id 和
同一 actual-context payload。它不创建持久 Session、不捕获、不 checkpoint、
不形成记忆、不调用模型/Owner，不创建收据、作业或必须由主人处理的队列。
新 capability 标记合同，不产生授权；独立 MemoryRead 仍必要，SourceRead 可选。
只有 pipe-10 context profile 为六个读工具；旧 pipe-5/6 和报告 profile 保持原义。

成功含原结构化响应及其文本副本，再附一个静态名 actual_recall 的资源链接。
按链接读取仍受同一 reader 的授权、当前 scope/snapshot 和缓存检查；不重算、
不自动获取、不授予权限。新 memory / prompt / tool 召回共享实际缓存规则。
旧链接失效后，只有仍有授权时的显式新读可取当前结果；这不适用于未知发布。

kind 缺陷先有失败反例，再最小改为 String + 既有常量检查。对象 kind 现在在
native recall 前拒绝；schema / capability / hello 指纹未因这次纠错改变。
对应源码 mcp/opening/mod.rs、native.rs；合同及两个 session schema 在 contracts/next。

## 实际证据

完整报告：[MCP-5-session-start](../validation/MCP-5-session-start.md)，旧 MCP-5
开场/响应合同报告继续有效。报告逐片记载日志和运行范围，不能简单相加冒充全量。

- session_start 首片当前三个 Host 目标共 21 passed；合同/表面 7 passed。
- 资源链接片相关 Host 13 passed；合同/表面 8 passed；额外指纹断言单项通过。
- 后续形成后双入口、实际 stdio 出口各一个定向测试通过；旧 profile 两项通过。
  只运行所列旧用例，没有执行调用 NativeMemoryOwner 的旧测试。
- 最新 kind 修正后：session 测试 3 passed、实际管道成功路径 1 passed、
  合同/表面 8 passed，0 failed。修正前反例失败日志保留，未伪装为全程通过。
- 独立 Python jsonschema 4.26.0：76 schemas、19 session cases、16 link cases、
  实际 session 输入/输出、完整响应及既有 opening 报文通过。原 ID、文本/结构化
  内容、链接/实际资源相等另有断言；JSON Schema 本身不保证这些跨字段关系。
- 各片定向 Clippy、fmt、适用的 surface 与推送前公开检查通过。未跑全工作区，
  不把历史 913 / 948 当作当前 HEAD 证据。纯测试/文档片未重复 schema/surface。

当前 source aggregate：
e4b5c470c69dbe36a9f85d0d9588d036a1222a720f8aab94e816ac56cb94c4c3。
runtime-surface.json 与 RUNTIME.md 已在同一实现提交登记；没有修改 Runtime 仓库。

## 继续边界

继续仅在已授权 MCP-5 的合成开场/资源范围处理有依据的实现或合同缺口；当前没有
未收束代码片。新片先列 owned-file allowlist，先冻结最小合同，再同片实现、
按风险验证、公开检查、带 Codex trailer 提交推送并更新草稿。保留其他贡献者署名，
不刷新手动项目状态，不将历史未完成提案当批准，也不自动启动新 MV 或真实接入。

只用新建合成根和自有进程，不打开私人 Vault、导出、备份、恢复材料或凭据。
Runtime-only capture；不开放 conversation_capture / 外部 Agent 宿主 hook。
Agent 提示/报告仍是不可信解释，内容/证据必须追到真正 Runtime UserStatement；
Agent 文字不能替代原话或 Owner 同意。不接真实模型/Provider/embedding，模型、
预算与外发仍需独立决定。forget-hide 尚未选择，不实现立即隐藏。不改 Runtime
pin、真实配置或共享外壳，不读 Activity/Usage，不部署、迁主、双写或创建自动化。

OutcomeUnknown / RecoveryRequired 保留完整首个原 Value/原键，不回原生工具结果，
实际断开并撤权，显式恢复；不自动重发、重连或换键。含 AgentNote / Correction /
Supersede / Forget delta 的根不能由旧二进制打开。合成结果不代表真实模型质量、
私人库验证、Runtime 产品采用或主人验收；合并仍须 Morii 单独指示。
