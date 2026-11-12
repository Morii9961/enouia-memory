# 连接器长期只读授权：合成验证（2026-11-12）

对应 [ADR141](../adr/141-connector-owner-read-grants.md)。独立分支
`codex/connector-read-policy`，基线 main `45de3c0`；交付前远程 main 仍为该提交。
Morii 已明确同意独立只读授权与历史修订边界，并授权本片登记 ADR141、清单和采用说明。
并行初始化分支及 PR #102、ADR140 未被修改或合入本片。

## 先刻画既有行为

`connector_policy_semantics` 的四个测试先在未引入新读取模型时执行，通过后在实现上回归：

- 形成策略延期会生成新修订/哈希；旧许可失效，新许可可看来源，但旧记忆/历史不可读。
- System 读取同样受精确当前引用限制；重开不改变结果。
- 新策略 ID 即使覆盖相同来源、关闭形成权限，也不能接管旧记忆。
- 停用再启用不会自动迁移记忆的旧 PolicyRef。

因此没有把七天形成策略延期当成长授权，也没有重写历史引用。

## 实际执行的检查

Windows、Rust 1.98.1、离线且 `--locked`，仅使用虚构临时 Owner 根、本片独立 target。
下表为各目标最终通过的测试数，跨次执行不重复累计：

| 检查 | 结果 |
| --- | --- |
| host `connector_policy_semantics` / `owner_read_grant` / `remote_read_admin_cli` | 4 / 6 / 1 通过 |
| host `remote_service` / `remote_oauth` | 3 / 6 通过 |
| host `qualified_agent` / `qualified_agent_tools` | 5 / 6 通过 |
| host `initialized_agent_native` / `initialized_agent` / `initialized_carrier` / `initialized_mcp` | 2 / 2 / 2 / 8 通过 |
| host `opening_mcp` / `opening_session_start` | 10 / 3 通过 |
| vault `segmented` / `segmented_agent` / `segmented_backup` | 4 / 4 / 4 通过 |
| contract `catalog` / `owner` | 2 / 1 通过 |
| 聚焦 clippy：host lib、remote binary、上述五个新增/直接修改测试目标；vault/contract lib，`-D warnings` | 通过 |
| `cargo fmt --all -- --check`；`git diff --check` | 通过 |
| `tools/schema-check/check_schemas.py` | 通过；既有 jsonschema 4.26.0，79 个 schema、46 个 catalog 文档及其余既有 corpus |
| `tools/integration/runtime_surface.py` | 通过；清单与 RUNTIME.md 同提交更新 |

初次受限环境的原生测试遇到 `StorageFailed`，在授权的原生 Windows 权限环境重跑通过；
不把环境失败算作产品通过。测试中曾修正虚构来源数量预期；静态检查的折叠 if、map 键迭代
提示也已修正并重跑。没有运行完整 workspace suite/build 或本片 Linux/WSL 测试。

## 新授权证据

六个 `owner_read_grant` 测试覆盖：

- 只有原生 Owner 可计划/确认，伪造历史引用被拒绝；计划不写库，相同已提交计划不再写。
  形成策略停用、修订、到期后，显式授权仍能读取原精确记忆和历史；旧入口仍拒绝。
  不因此恢复创建/修订权限；重开保持授权且拒绝旧实例许可。
- 续期和撤回使旧 permit、真实召回 context 缓存、已有 opening 会话失效；重新授权和重开
  都不能复活已删除记忆。
- Agent、主体/项目、来源、敏感度、原文开关、授权有效期和计划 snapshot 门槛仍生效。
  无 source_read 的来源文本入口拒绝；授权到期拒绝新旧读取。
- 同一策略 ID 的多个明确历史修订可选；派生来源闭包和 Private 来源不能借此越过范围。
- 现有备份导出/恢复重放授权；恢复根在主人激活前继续隔离，激活后能读获准旧记忆。
- `AfterCurrent` 注入未知发布结果后，状态查询拒绝；原生 forward recovery 恢复原授权
  ID/修订，再确认同一计划不产生新提交。

`remote_read_admin_cli` 运行实际二进制的 status、plan、明确确认、续期、撤回与 OAuth status；
缺确认标志拒绝，预览不写，状态没有原文。续期只延长期限，撤回后不能用续期复活。

`remote_service` 在已有回环 HTTP、DCR/PKCE、六读工具与 session_start 实际召回流程上
增加显式授权分支：形成停用/到期后仍按授权读取；约四分钟换代后旧会话 404，新会话可读；
并行原生 OAuth 查看/撤回后，旧会话 access 返回 401、refresh 兑换失败。
OAuth 状态文件不含批准码、code 或 token 原值，读取未改变 Vault snapshot。

## 证据边界

上述全部为合成根、合成范围与本机回环；不是实际 ChatGPT、Provider、生产 Vault、Linux VPS、
TLS 或部署验收。原有 Linux/OAuth 报告仅作为既有实现证据，不冒充本片 HEAD 的 Linux 验证。
未读取原始聊天、操作真实 Vault、删除旧库、修改 Runtime 仓库或刷新手动项目状态快照。
真实来源、敏感度、原文权限和期限仍由 Morii 决定；合并、Runtime 采用、独立加密备份、
真实恢复、迁移和主人日用验收仍未完成。本片不启动新 MV 阶段。
