# 主人管理连接器只读授权

[ADR141](../adr/141-connector-owner-read-grants.md) 的原生入口。以下路径和范围均为占位符；
本说明不选择真实来源、主体/项目、敏感度、原文权限或期限，也不授权真实库操作。

## 四种期限

形成策略决定哪些材料可被采集/形成；记忆永久记录当时精确的策略 ID、修订与哈希。
独立读取授权另行明确批准其中哪些历史修订和来源可供某个 Agent 读取。形成策略到期、
修订或停用不再自动撤回这份单独授权；删除/记忆撤回与读取授权撤回仍然优先。
这不延长形成/写入权限，也不重写旧记忆。

Vault permit 最长五分钟，远程服务约四分钟换代并结束旧 MCP 会话。临近授权到期时，
permit 缩短到剩余时间；到期停止读取/服务，不会靠 OAuth 延命。OAuth access 为一小时；
refresh 每次使用轮换，并从该次重新计算三十天。它们只证明客户端连接身份和 `memory.read`，
不决定具体哪些记忆可读。撤回不可能收回客户端此前已收到的内容。

## 查看、设置、续期和撤回 Vault 授权

先停止远程服务、Host、initializer 和 worker，以拥有该根的 OS 账户运行。命令普通打开
明确配置的 Owner Vault，因此未激活恢复根、未知发布、旧布局和活动租约仍会拒绝。
不直接编辑 Vault 文件，不用恢复/启动标志绕过隔离。

```text
enouia-memory-remote read-status <config.json>
enouia-memory-remote read-check <config.json>
enouia-memory-remote read-plan <config.json> <explicit-grant.json>
enouia-memory-remote read-set <config.json> <reviewed-plan.json> --confirm-owner-read
enouia-memory-remote read-renew-plan <config.json> <grant-id> <UTC-expiry>
enouia-memory-remote read-revoke-plan <config.json> <grant-id> --revoke
```

`read-status` 返回 snapshot、当前时间、历史形成策略/精确 `policy_refs` 和当前授权；
没有原文、令牌或令牌摘要。由主人核对 active 与起止时间，选择实际范围。
同一策略 ID 的多个历史修订须逐一明确选入，不自动批准未来修订。

迁移准备或每次修改配置后，使用 `read-check` 核对这份配置当前能否取得读取许可。
它复用 `serve` 的配置和许可门槛，校验原生 opening profile；成功返回 `read_ready=true`、
检查时间、snapshot、`authority_mode`、所选 grant ID、Agent 和范围/短许可截止时间。
未选择 grant 时明确显示 `current_formation_policy`，不会自动改用其他授权。
失败以非零退出，不启动监听、不执行召回、不返回原文、不创建 OAuth 目录或状态，也不提交授权。
这个命令同样要求停止占用 Vault 租约的进程。

输出的 `scope_hash` 对应检查用的临时许可，返回前已撤回，不能提交给服务或客户端复用；
结果只反映检查时点，不是上线验收。它不检查端口占用、OAuth 目录实际可写性、DNS/TLS、
反向代理、备份新鲜度或 ChatGPT 互通；随后 `serve` 仍从当前状态重新取得许可。

`explicit-grant.json` 的闭合字段形状见
[合成示例](../../tests/fixtures/catalog/owner-read-grant.json)和
[Schema](../../contracts/next/owner-read-grant-v1.schema.json)。示例不是可直接应用到真实库的选择。
grant_id 使用独立的 policy ID，不能与形成策略 ID 重叠；初始 revision 为 1。
principal 必须是所选 Agent，vault_id/subject_id/project_id、完整 policies、source_ids、
max_sensitivity、source_read、active 和起止时间都须明确提供。sources 必须已存在且未删除。

把 plan 命令的 JSON 输出保存在仓库外、主人保护的位置，检查其中**完整 grant**，再将该文件
交给 `read-set`。plan 不写 Vault；确认写入要求相同 snapshot，且在五分钟内。
变更中途发生其他提交就重新预览；未知结果须先停止并通过原生 forward recovery 恢复原发布，
之后用 `read-status` 查原 grant ID/修订/内容，不换 ID 或重新计划来重发。
同一份已经提交的 plan 可查得相同结果，不再产生提交。

续期只改变期限并递增修订，不改变来源、原文或敏感度；已撤回授权不能用续期命令复活，
需完整 `read-plan` 重新审阅。撤回 plan 保留原范围并设 active=false；这两个命令也只是预览，
均须将输出交给 `read-set ... --confirm-owner-read` 才会写入。范围增减同样通过完整 plan。
每次新修订使旧 permit、旧 MCP 会话及其实际召回缓存失效。重开不能复活旧许可。

服务配置增加可选 `read_grant_id`。主体、项目、policy_ids、source_ids、max_sensitivity、
source_read 仍必须是授权内的明确范围；配置可以进一步缩小，不能扩大。
未提供 read_grant_id 的配置保留原来的当前形成策略门槛；没有自动回落。
确认授权后再启动服务；有 OAuth token 的客户端也必须重新初始化 MCP 会话。
source_read=false 禁止来源文本入口和带原文详情，但获准记忆自身仍可能包含引文内容。
当前来源读取也不提供原始归档包或附件文件字节下载。

## 查看与撤回 OAuth 客户端

使用同一配置和拥有其 OAuth 状态目录的 OS 账户；这些操作复用已有状态锁，能与服务并行。

```text
enouia-memory-remote oauth-status <config.json>
enouia-memory-remote oauth-revoke <config.json> <client-id> --confirm-revoke-client
enouia-memory-remote approve <config.json>
```

status 只列客户端 ID 与尚有效的 access/refresh 最晚到期时间。revoke 删除该客户端登记、
待兑换 code 和所有 token，并清除当前未使用批准码；下一请求，包括旧 MCP 会话，认证失败。
其他客户端不被撤回。要再连接，须重新登记并通过新的主人批准码；approve 本身不续期 Vault 授权。

撤回 Vault grant 结束所选 Agent 的具体读取；撤回 OAuth 客户端结束该客户端的连接资格。
两者独立。当前服务只把所有获批客户端映射到同一个配置的 Agent/读取范围，不提供每客户端
多范围路由；若要完全停读，撤回所选 grant 并停止服务，不仅仅撤回某个客户端。

## 备份与采用

新授权及历史策略由不可变提交链保存，现有 Owner export/restore 会完整重放，恢复隔离仍生效。
首次写入前保留采用前备份；旧二进制不认识新 delta，可能拒绝开库，不能用改 JSON 来降级。
恢复仍要求主人确认备份包含最新删除/撤回；不能恢复较旧备份来重新获得权限。
OAuth 状态仍在 Vault 外，单独管理，不从备份凭空恢复 token。
真实 Linux、ChatGPT、部署、独立加密备份和主人日用验收仍需各自完成。
