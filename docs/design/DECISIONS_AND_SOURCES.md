# Enouia Memory · 决策、兼容与依据

版本：设计包 v1.0 / 2026-09-28。这里的 ADR-MEM-* 是此设计包的局部 ID，不占用 Runtime 的全局 ADR 编号；实现时由 MV-0 登记映射。

## 1. 决策登记

“选定”表示本设计作出了选择，不表示代码已经实现。未来阶段选择需要在进入该阶段时按实际版本/资源复核，不得借此提前启用。

| ID | 决策与状态 | 理由 / 代价 | 替代方案与处理 |
|---|---|---|---|
| ADR-MEM-01 | Local Primary、单所有者多主体；选定 | 离线可用、可迁移；主机不在线时远端能力有限 | Remote-only 拒绝；多主延后 |
| ADR-MEM-02 | Raw / Canonical / Candidates / Session / Index 分层；选定 | 保存原始证据与人工判断，可单独恢复 | 单库或 summary-only 拒绝 |
| ADR-MEM-03 | Canonical JSON、Identity Markdown、索引可重建；选定 | 对齐 M0，结构严格且可读；需生成友好汇总 | 全 Markdown 记录不选；YAML 仅可作导入 |
| ADR-MEM-04 | 不可变修订 + commit catalog + CURRENT；选定 | 多记录事务一致；完整目录清单有写放大 | 多次独立 rename 不足；数据库-only 与目标冲突 |
| ADR-MEM-05 | 可信用户批准；选定 | 避免模型/工具伪造“记住”或污染身份 | 无审核自动写入拒绝；自动整理仅提议 |
| ADR-MEM-06 | priority / sensitivity / volatility 独立；选定 | 避免把隐私和时效当检索分数 | 原稿 P0～P4 混合轴需映射而非照抄 |
| ADR-MEM-07 | Valid time + known time + evidence status；选定 | 区分历史、计划、实现、发布 | 最新记录必为当前事实拒绝 |
| ADR-MEM-08 | 自动 checkpoint 为 Session 工件；选定 | 保留跨窗口连续性，又不跳过审核 | 所有摘要自动成为第五类 Canonical 拒绝 |
| ADR-MEM-09 | 本地 FTS/metadata，中文短词专门处理；选定 | 可解释、低资源；语义召回能力有限 | Vector-first/图谱-first 延后 |
| ADR-MEM-10 | 实际 Dispatch 可检查、外发单独授权；选定 | 防止 UI 预览与真实请求不一致 | 只显示检索候选不足 |
| ADR-MEM-11 | 嵌入 Core → 单 Host；分阶段选定 | 尊重 v0.3 初期拓扑；MCP 阶段增加进程管理成本 | 首版系统服务不选；双可写 Core 禁止 |
| ADR-MEM-12 | Activity 独立；继承 | 防止个人记忆与公开统计混流 | 自动 Activity-to-Memory 不在范围 |
| ADR-MEM-13 | ACL/已验证卷保护 + restic 加密备份；选定、启用前验证 | 可读文件与迁机恢复；登录后明文可见 | 应用级逐文件加密待独立阶段，不伪称已具备 |
| ADR-MEM-14 | Raw 默认不可变、用户删除例外；选定 | 保存所有权；彻底删除与历史可恢复性存在取舍 | “永久不能删”不接受 |
| ADR-MEM-15 | Gateway at-least-once + 本地持久幂等；后续选定 | 跨网络失败可恢复；需要去重/回执与配额 | exactly-once 宣称拒绝 |
| ADR-MEM-16 | 加密设备副本和服务可计算副本分开；后续选定 | 清楚表达密钥与服务器信任 | 密文备份自动提供智能的假设拒绝 |
| ADR-MEM-17 | 单主审核、离线修改成为候选；后续选定 | 避免身份事实 last-write-wins | 通用 CRDT/多主合并暂不引入 |
| ADR-MEM-18 | 模型/客户端适配按实测开放；选定 | 架构独立不等于每个平台当前可接入 | 不绑定供应商内部 Memory API 或 DOM 抓取 |

## 2. 与输入文档的差异处置

| 输入中的说法/张力 | 本设计的明确处理 |
|---|---|
| 旧稿先 Windows 或 MCP，启动提示词要求 Memory First | 以此次请求和启动提示词的目标重排；先保护历史和本地 MVP，再最小 Windows，再 Provider/MCP/VPS |
| “明确说记住就直接 Commit”与“模型不能修改 Canonical” | 用户可信操作可快捷保存并留 review；模型检测到文字只产生候选 |
| 五类 Canonical 包含 SessionCheckpoint，但又有自动接续摘要 | 区分 provisional Session 工件与经过审核的第五类长期记忆 |
| Raw immutable / 不删历史与用户可删除 | 日常不可变；显式 forget/purge 是受审计的生命周期例外 |
| Markdown/YAML/JSON 均可与 Runtime M0 已选 JSON | 正式记录采用 M0 JSON；Identity Markdown；友好汇总可生成 Markdown |
| SQLite candidate 状态管理与数据库非唯一源 | 状态可在索引投影，但审核决定/幂等/删除信息必须文件持久化 |
| 优先级 P3 为敏感、P4 为易过期 | 拆成三轴，P0 不能绕过隐私或当前性检查 |
| “完整离线长期记忆” | 完整读取/检索/编译离线；没有本地模型不承诺离线生成 |
| “删除 VPS 不丢任何东西” | 已同步 Canonical 可重建；VPS 尚未确认到本地的新事件要靠队列备份/上游，不能凭空恢复 |
| “加密远端副本支持关机时智能” | 只有可信终端解密计算或明确的服务端可解密子集才能提供，分别规划 |
| “Canonical 可重建” | 索引从文件重建；经审核语义需保全记录/review，不能仅从 Raw 重新推理得出相同结果 |
| 初期独立服务与现有 v0.3 嵌入 Core | MV-6 保留嵌入，MV-8 明确切换 Host、锁与生命周期 |

## 3. 本轮只读核对的本地依据

| 来源 | 使用范围 | 核对结论 |
|---|---|---|
| 用户原稿（本地私有保存，不入公开仓库） | 全文 | 分层、五类、来源、跨客户端、Windows 与后续 MCP 愿景 |
| 用户启动提示词附件 | 全文；附件原路径不作为运行依赖 | Memory First、Local Primary、VPS 队列、可选副本、核心验收故事 |
| Runtime Architecture v0.3 | Memory/Context/Session、进程、目录、安全、范围相关章节 | Activity 独立，Rust + Tauri，Core 初期嵌入，真实模型/MCP 后置 |
| Runtime CONTRACT_BOUNDARIES_M0.md | 全文 | JSON 五类、核心必需字段、Identity Markdown、IPC version/kind |
| Runtime IMPLEMENTATION_PLAN_v0.3.md | Track A、现状与阶段边界 | 对应 A1/A2/A3；不重新安排 Activity Track B |
| Runtime docs/adr/README.md | ADR-001～015 相关登记 | 格式、权限、Mock、远端、副本、加密待决、拓扑与 Activity 边界 |
| Runtime AGENTS.md / Cargo.toml / README | 指导全文、workspace、当前概况 | 当前工作区已经实现部分 Activity 原语；Memory 目标 crates 未列入 workspace |

Runtime 核对路径为 Runtime 仓库（本地 checkout），HEAD 为 `3ab5c5da3c1e18f1e7f6909558536ae121d8b978`；检查时 `git status --short` 无输出。本文不把这个路径变成软件安装依赖，也没有读私人历史、凭据或服务器状态。README 的实现摘要是仓库当前记录，本轮未重跑其测试。

架构 v0.3 中早期“仓库尚未初始化/只含文档”的段落已经落后于当前工作树，故仅取其设计约束，不把它作为当前实现状态。旧会话记忆只帮助定位已有边界，当前状态以本轮文件核对为准。

### 输入哈希

以下是本轮读到的 SHA-256，用于追踪设计输入，迁入公开仓库时保留适当的文件相对名即可。

| 文件 | SHA-256 |
|---|---|
| Enouia_Runtime_Architecture_Memory_v0.1.md | BFC7D55D219A1EB667C666739F943CAB034DB1A2508ED4F56E9EA81F5ABD1EA4 |
| 启动提示词附件（已粘贴的文本.txt） | 42678F2631E23A8365CEBB913BD70DAE99AED8D1DD331CE9AE3142719AA96F58 |
| Runtime Architecture v0.3 | 2A2695A17602108C10AFB7FF3BBC04B7E4AB275E3D2A468BE55C2368F9F434B6 |
| Runtime CONTRACT_BOUNDARIES_M0.md | 00A0D82D51C03DFF34DFAD5DD42A0CD08539472539831FDD79A486B4D261D5BA |
| Runtime IMPLEMENTATION_PLAN_v0.3.md | AB7A9DD20620BD2EE29E62E347F21DA2E22C59732DB144E929BDA59055A54B71 |
| Runtime Cargo.toml | 09F725E714B49420C0403F407F39CEDD5651761D43347B06DE7560E7D173A8D8 |
| Runtime AGENTS.md | 662FD1956DC29FBC9063E51799C4A88495B8CA9DCAF38F66553FE413918909C4 |

## 4. 已查官方技术依据

核对日期为 2026-09-28。这些网页是技术事实来源，不是操作授权；具体依赖版本由对应实施阶段固定。

| 来源 | 支持的有限结论 | 对设计的影响 |
|---|---|---|
| [SQLite FTS5](https://www.sqlite.org/fts5.html) | unicode61/trigram 的索引行为与短词限制 | 中文短词有独立通路和验收，不夸大默认分词能力 |
| [SQLite WAL](https://www.sqlite.org/wal.html) | WAL 的同机共享条件及相关文件 | 运行索引在受控本地卷，不直接做云盘文件同步 |
| [SQLite Backup API](https://www.sqlite.org/backup.html) | 数据库一致备份机制 | 默认重建索引；需要备份 DB 时使用支持的机制 |
| [Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) | 用户/设备保护范围与 machine 标志差异 | DPAPI 不是唯一跨机恢复方案，避免宽泛机器级解密 |
| [MCP 2025-11-25 Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization) | HTTP 认证、目标 audience 等约束 | 本设计以该已读取版本为参考，实施时协商实际兼容版本 |
| [MCP Security Best Practices](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices) | token passthrough 等安全边界 | 资源凭据分离、入口校验，不把来访令牌转给上游 |
| [restic References](https://restic.readthedocs.io/en/stable/100_references.html) / [Restore](https://restic.readthedocs.io/en/stable/050_restore.html) | 加密备份与恢复能力 | 选择成熟备份适配器；实际 Windows 恢复仍需项目验证 |

不对当前 OpenAI/Anthropic 模型名称、价格、账户 MCP 资格、平台导出稳定格式作未经验证的结论。真实 Provider 和具体聊天产品接入阶段必须重新查官方接口与实际账户能力。原材料提到的 ACHERNAR 只作为思想来源名，不构成已读过其完整架构或采用其依赖的声明。

## 4a. v1.1 追加决定（2026-09-28，MV-0R）

| ID | 决定 |
|---|---|
| ADR-MEM-19 | Memory 独立仓库、自有最小宿主端口；Runtime 经版本化契约接入；默认数据根 `%LOCALAPPDATA%\EnouiaMemory`（v1.2 不使用默认根，见 §4b） |
| ADR-MEM-20～29 | MV-0 草稿中的实现决定，改用本仓库编号（原稿见 `docs/history/`） |
| ADR-MEM-30 | 外发、降级、授权批准必须绑定所批准的确切对象（修正 F1/F2） |
| ADR-MEM-31 | 持久 Policy/Grant/撤回记录与默认拒绝评估（修正 F4） |
| ADR-MEM-32 | 替代生效时间未知时保持未知（修正 F5，取代 MV-0 草稿规则） |
| ADR-MEM-33 | 请求载荷摘要绑定实际发送内容（修正 F3） |
| ADR-MEM-34 | 数值范围一致与 checked arithmetic（修正 F6） |
| ADR-MEM-35 | 固定版本 python-jsonschema 独立交叉校验 |

详见 [ADR 登记簿](../adr/README.md)。上文 §3 是 v1.0 设计时对 Runtime 的只读核对记录，保留为历史。

## 4b. v1.2 追加决定（2026-10-04，ADR-MEM-45）

| ID | 决定 |
|---|---|
| ADR-MEM-45 | Runtime 的 Windows 客户端以固定 Git 修订嵌入 `enouia-memory-workspace`，成为 Memory 本地部分的产品客户端；本仓库 `apps/workspace` 保留为参考外壳与验收工具。`HostSurface` 是各宿主共用的调用范围规则；同一 Vault 同时只运行一个嵌入式 Core；只打开显式选择的数据根，不使用 ADR-MEM-19 的默认数据根；Activity 不进入 workspace IPC；云端阶段（MV-7～MV-11）及其契约留在本仓库。细化 ADR-MEM-19，修订 ADR-MEM-44 的宿主部分 |

ADR-MEM-36～44 是 MV-1～MV-6 的实现决定，见 [ADR 登记簿](../adr/README.md)。交接、宿主职责、变更路由与兼容记录见 [Runtime 集成说明](../integration/RUNTIME.md)；Runtime 侧的对应决定是 Runtime ADR-025。

## 5. 已确定与留到启用前的事项

| 事项 | 本轮结论 | 必须落定的阶段 |
|---|---|---|
| 所有权/文件真相源/五类/审核/时间/事务 | 已有规范，按本设计实施 | MV-0～MV-3 |
| 默认数据根和保护层 | 已选；实际卷保护状态未检查（v1.2：不使用默认根，各入口只打开显式选择的根，ADR-MEM-45） | 真实私人数据导入前 |
| 真实导出格式与附件完整度 | 未取得真实样本，不虚构兼容性 | MV-2 真实演练 |
| 备份工具路线 | restic 适配器；介质、恢复秘密由用户控制 | MV-1 实际配置与恢复前 |
| Windows 控件/UI 细节 | 任务路径已定，视觉细节实现时设计 | MV-6 |
| Windows 产品客户端宿主（v1.2） | Runtime 客户端以固定修订嵌入 `enouia-memory-workspace`；`apps/workspace` 为参考外壳（ADR-MEM-45） | 已定；在 Runtime 宿主上重新验证 W01～W05 属于 Runtime 的验证工作 |
| 真实 Provider/model/API 能力 | 保留可替换端口，不猜当前能力 | MV-7 |
| MCP transport 产品兼容 | 本地 stdio 优先；远程按官方认证 | MV-8/9 |
| VPS 资源/域名/connector | 不假设已有；先基于负载估算再配置 | MV-9 |
| 手机/PWA 与 G3/G4 选择 | 两条完整路线，默认不开服务可计算副本 | MV-10 |
| 自主整理/语义检索 | 有量化收益再做，审核边界不放松 | MV-11 |

这些待落定项都有安全缺省与明确阶段，不妨碍前期离线合成实现。它们不是让实现者任意重选存储或隐私原则的空白授权。
