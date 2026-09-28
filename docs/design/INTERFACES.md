# Enouia Memory · Windows、Provider、MCP 与 VPS 接口

版本：设计包 v1.0 / 2026-09-28。工具名称是本项目拟定接口，不代表任意平台目前已经支持。所有外部连接都在对应阶段验证。

## 1. 统一应用边界

Windows、最小 CLI、MCP、未来 PWA 复用同一套领域操作。传输可不同，权限、版本、幂等、审核和错误语义不能不同。接口只接受逻辑 ID，不接受任意本地路径、SQL、shell 命令或调用方指定的“管理员身份”。

本地 IPC 响应沿用 Runtime M0 的 `schemaVersion: 1` 与 kind discriminant；存储字段继续 snake_case，由后端映射，不改变 Activity 现有 DTO。公共响应头包含 requestId、kind、vaultCommitId、policyEpoch、operationId（写操作时）、result/error。

通用错误：unauthenticated、permission_denied、not_found、vault_locked、busy、revision_conflict、idempotency_conflict、invalid_source、broken_provenance、unsupported_schema、index_not_ready、budget_exceeded、storage_full、storage_failed、audit_unavailable、provider_unavailable、offline、cancelled。附 retryable 与脱敏详情；不得返回原始异常堆栈、绝对路径或令牌。

长任务返回 operationId，用户按 ID 查询状态/取消；进度不是完成证据。写操作只有 Vault commit 已达持久化条件才返回 committed。读操作返回所用快照和 stale/partial 状态。

## 2. 核心接口目录

| 领域操作 | 最小输入 | 主要输出 | 权限 |
|---|---|---|---|
| memory_search | query、过滤器、cursor、limit、time mode | 有权看到的 IDs/revisions、snippet、时间/证据标记 | memory:read + 范围限制 |
| memory_read | memory_id、可选 revision | 记录、来源摘要、版本/替代链 | memory:read |
| memory_source | source_id、限定 locator/range | 有界原始片段、来源元数据、附件可用性 | source:read，不能从 memory:read 推导 |
| memory_propose | proposal、来源、idempotency key | candidate_id、revision、pending/duplicate 状态 | memory:propose |
| candidate_list | 状态/项目/页码 | 可审核条目与差异 | owner:review |
| candidate_review | action、expected revision、精确 diff approval | commit、正式记忆 IDs 或拒绝结果 | owner:review；不向普通 Agent 暴露 |
| identity_review | 精确 Identity diff、approval | 新身份修订和 commit | owner:identity |
| context_get | 编译输入与目的地 | capsule、允许的说明、版本与期限 | context:read + 对应记忆范围 |
| session_checkpoint | session/branch、covered events、摘要、来源 | provisional checkpoint_id | session:propose |
| import_preview / import_start | 系统选取的文件句柄/受控 import token | 报告/operationId | owner:import |
| backup / restore_preview | 已配置目标、snapshot_id | 固定清单、恢复计划 | owner:backup / owner:restore |
| delete_preview / delete_commit | IDs、模式、范围、expected versions、approval | 影响范围、tombstone/purge receipt | owner:delete |

`memory_update` 为旧稿兼容名称，若实现，只是 memory_propose 的 revise/supersede 别名，必须返回 pending；不得保留一个能直接写 Canonical 的后门。早期 MCP 最小集为 context_get、memory_search、memory_read、memory_source、memory_propose、session_checkpoint。

memory_search 默认 limit 20、最大 100；单次来源片段默认不超过 8 KiB；超限分页/明确拒绝，避免“单条 source”返回整个导出。attachment_get 是后续额外授权接口，不因工具能读取文字便自动开放图片原件。

幂等写键绑定 principal + operation + key，同时保存 request_payload_hash；同键不同正文报冲突。MV-1～MV-8 不自动删除写收据；后续清理须保留去重摘要或明确协议重试期，不让旧写请求重新生效。

## 3. Windows 联动

Windows 采用已有 Tauri + React 方向，UI 只接 typed DTO，文件选择后由后端处理。第一版页面按实际任务设计：

| 页面 | 必须能完成的操作 | 必须可见的状态 |
|---|---|---|
| Memory Explorer | 按类型/项目/时间浏览，打开来源和替代链 | approved、过期、冲突、历史、来源缺失 |
| Import Center | 预览、启动、暂停/取消、查看覆盖报告 | 原件已归档、解析中、附件缺失、部分失败 |
| Candidate Review | Accept/Edit/Reject/Merge/Supersede | 精确 diff、证据、旧事实、敏感与外发范围 |
| Context Inspector | 看本轮 included/excluded、目的地、token 和来源 | 预览、实际发送、中断、未发送之间的区别 |
| Sessions | 选择项目/分支、checkpoint、接续范围 | 最后已保存事件、摘要覆盖、未完成事项 |
| Vault & Recovery | 备份、恢复预览、删除预览、锁定与权限 | 最后成功备份、最后验证恢复、磁盘空间、索引水位 |
| Runtime Status | 各组件健康与本地/远端连接状态 | Core、Vault、Index、Provider、Sync、Activity 独立状态 |

先做上述最小记忆操作路径，之后补托盘、全局快捷键、overlay、图片拖放和 Provider mode。快捷键与剪贴板均显式操作；不后台持续读剪贴板。拖放会说明是仅本轮附件、归档来源还是提议长期记忆，不默认三者全做。

前端不能凭“本机页面”自称 owner；可信窗口只加载本地应用资产，严格配置 IPC capabilities、CSP、导航和附件渲染。来源 Markdown/HTML 当不可信内容，不启用脚本和远程图片自动加载。

“关闭窗口”“退出 Runtime”“锁定 Vault”“暂停同步”是四个不同操作；界面分别说明。MV-8 Host 上线前关闭 UI 可停止 Core，MV-8 后退出 UI 不一定退出 Host。Activity 调度行为始终按原域规则处理。

健康状态沿用 Runtime 的 `Healthy / Degraded / Unavailable / Recovering`，另带 operational mode、lastAttemptAt、lastSuccessAt、error code、数据水位与 age。索引重建可使 MemoryIndex=Recovering，而 Vault 仍 Healthy；Provider 不可用不把已保存记忆标丢失。健康探测不推进来源更新时间，也不自动产生 VPS Runtime heartbeat。客户端协议/API 版本不支持时显示可恢复错误，不用空列表伪装空 Vault。

## 4. Provider 边界

保留既有 ProviderRequest / ProviderResponse / ToolRequest / ToolResult / ProviderCapabilities 抽象。先 Mock，再分别接用户选择的 OpenAI、Anthropic 或本地模型适配器。本设计不固定真实模型 ID、价格、账户权限或未验证能力；“Claude Opus 5.5”是用户指定的实现交接对象，不是本轮调用或兼容性结论。

ProviderCapabilities 至少包括模型绑定、文本/图片支持、上下文限制、输出预留、token counting、streaming、tool calling、取消能力与验证时间。不可用字段为 unknown，不从名字推测。各适配器固定所支持的 API/SDK 版本与实际 smoke test 后启用。

Provider 永远由 Core 后端调用，密钥通过 OS 保护的秘密存储读取，不进入前端、Vault、日志、MCP 参数或备份。外部模型故障后默认保留请求/响应状态，允许用户重试；不自动切换到另一个未经同意的目的地。

本地 Mock 只读取已记录 capsule 并返回确定性引用，用于证明流程，不模拟“模型真的理解”。真实模型验收检查共同证据和时间限定，不比较文学风格一致性。

## 5. 本地 MCP 与独立 Memory Host

MV-8 首先实现用户级单一 Host，通过受限 Windows named pipe 或等效认证 IPC 供 UI、CLI、stdio MCP adapter 使用。选择并验证管道 ACL、调用进程身份、消息大小、超时与 schema；本机可连接不等于有全部权限。stdio adapter 只是授权客户端，不另开一个直接写 Vault 的实例。

Host 拥有 Vault 写锁；UI 连接失败时显示离线/Host 启动错误，不偷偷降级成第二写者。启动、升级、退出顺序为停止接新请求、取消/完成有界操作、提交水位、释放句柄。运行进程与安装版本不匹配时只读或拒绝，不跨版本并写。

接入特定聊天产品前需验证该账户与版本是否提供所需 MCP transport、认证、工具权限、网络可达性和工具结果限制。某产品不可接入时保留本地 CLI/手动 capsule 导出方案；这不是 Memory MVP 失败。工具调用不提供未经 API 暴露的全量平台会话历史。

HTTP MCP 在远程阶段使用兼容且锁定版本的标准认证流程。服务端校验目标 audience、issuer、过期、scope 与撤回状态；不同资源使用不同令牌，禁止把上游来访令牌透传给模型或其他服务。[MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization) 与 [Security Best Practices](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices)

## 6. VPS 的分层路线

VPS 不复用 Moriium author database、Activity receiver、公开静态目录或其密钥。即使同机部署，也分用户、目录、凭据、进程、监听入口和备份策略。初期单实例 Gateway + 持久队列 + 对象暂存足够；不因未来扩展预先要求 PostgreSQL、Redis 或 Kafka。

| 能力级别 | VPS 保存/处理什么 | 本机关闭后能力 | 信任含义 |
|---|---|---|---|
| G1 网关/事件队列（MV-9） | 受限事件 envelope、持久待同步对象、回执 | 接收并排队；需 Vault 的读请求返回 local_offline | 队列是新事件的暂时唯一副本，需要独立备份 |
| G2 加密备份（MV-9 可选） | 客户端加密的快照对象与必要元数据 | 不能搜索或解密记忆 | VPS 无解密密钥，不提供在线智能 |
| G3 加密设备副本（MV-10a） | 供已授权设备解密的记录包与清单 | 手机解密后本地检索/编译，再按许可调用模型 | VPS 只转存；终端是可信端点 |
| G4 可计算的有限副本（MV-10b） | Morii 显式选择的低敏资料，服务能解密使用 | 服务端提供受限上下文/问答 | 服务端属于信任边界，不能宣称对它端到端保密 |

默认只走 G1，G2 作为备份选项；G3/G4 均独立开启。只有 G3 配套终端计算或明确接受 G4 信任扩展，才可能做到“电脑关机还能用有记忆的 Enouia”。保存密文快照本身不能实现这一目标。

## 7. Gateway 队列契约

外部事件 envelope 包含 event_id、connector_id、producer_event_id（若有）、received_at、claimed_occurred_at、payload_hash、payload_size、schema_version、sensitivity、encryption_mode、dedupe_key、signature verification result。事件文字是数据，不会自动执行工具。

服务须先完成身份/签名、重放窗口、大小和速率验证，再持久化 payload 与队列元数据，之后才能返回 durable_received。跨 SQLite 与对象文件时先持久对象后提交引用，接受崩溃后孤儿对象清理，但不接受确认后对象丢失。这里的队列 DB 是网关运行权威状态，不适用本地 Memory 索引“随时删除”规则。

状态：received → leased → local_persisted → acknowledged → retention_pending → removed；失败可回到 received 或进入 dead_letter。提供 at-least-once 投递，靠 durable 去重实现不重复应用，不承诺端到端 exactly-once。

本地处理顺序：拉取带租约批次 → 验证 envelope/hash → 事务保存 inbox 与来源、去重收据 → 返回本地 commit_id 回执。仅在本地持久接收成功后 ACK。抽取模型失败不影响 ACK，因为来源已经安全到本地；后续候选可独立重试。

ACK 丢失则服务重投，本地回同一 receipt。批次游标只有前面的条目均已持久接收或正式拒收/dead_letter 并记录原因时才前移，不能因后面的事件成功而越过缺口。租约超时重投，指数退避加抖动，重复失败进入有可见原因的 dead letter，不静默丢弃。

建议缺省：单事件 256 KiB、租约 60 秒、拉取批次 50、已确认 payload 保留 7 天后清除、待确认超 30 天提醒但不自动删除、总配额初始 1 GiB。参数在 MV-9 压测后冻结。配额满时拒绝新收件并返回 retryable，不能先 2xx 再丢弃；没有重试能力的上游可能损失事件，必须在 connector 能力报告中披露。身份/回放去重摘要保留期至少覆盖声明的重试窗口，超窗事件需重新协调。

电脑关机时不排队敏感“读取全部记忆”请求等以后悄悄发送；读操作快速返回 offline。只有明确可异步的事件/候选提议进入队列。每次重新上线先处理删除/策略同步，再接普通资料。

外部 webhook 若向 VPS 提交明文，VPS 在入口就能看到明文；后续再加密不使入口变成 E2EE。由自有客户端加密的 payload 可保持转发服务不可读，但其大小、时间、设备和连接元数据仍可见。

## 8. 多设备、副本冲突与主设备迁移

MV-10 保持单一 Primary writer。其他设备可以读授权副本、产生会话事件和候选，但不能离线直接改 Canonical；回到 Primary 后审核。双方均修改同一条的情况生成 conflict，不采用 last-write-wins 覆盖身份或项目事实。

副本 manifest 带 vault_id、primary_epoch、snapshot/commit_id、policy/deletion epoch、允许集合、截止水位、签名/认证信息、过期时间。客户端检测回退和撤回；G4 默认有效期 24 小时，可由用户缩短，到期拒绝当前事实问答，不借用无限期旧缓存。

已离线设备无法被保证立即远程擦除；撤回阻止后续服务与同步，并要求重连先应用 tombstone。新副本不携带曾删除资料。备份恢复后需先对账最新删除/设备撤回信息，再恢复同步。

迁移 Primary：冻结旧主写入 → 备份并验证 → 新机导入 → 校验同一 vault/提交点 → 轮换 primary_epoch 和设备凭据 → 明确撤销旧主同步资格 → 新主恢复写入。旧主遗失时通过恢复密钥和操作记录办理接管，旧机以后出现只能只读并提交冲突候选，不能自动恢复主身份。

G4 在源电脑关机期间运行时沿用相同编译规则与发布版本，只面对批准子集；规则不由 VPS 管理员任意重写。缺少来源、过期或 policy 版本未知时拒绝输出。服务器可计算子集始终可丢弃重建，不是唯一完整记忆库。
