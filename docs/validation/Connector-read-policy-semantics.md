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
提示也已修正并重跑。这批未运行完整 workspace suite/build 或 Linux/WSL 测试；
合并后的 Linux 增补见下节。

## PR #103 合并后的 Linux 增补

2026-11-12，PR #103 合并为 main `d529eebfe2b273cba2a1eeb1dd2f0f8c11181853`。
本片独立 worktree 从该 main 创建 `codex/connector-read-policy-linux`；并行初始化主工作树
和仍独立的 PR #102 未被修改。合并后的 Runtime surface checker 通过，aggregate 保持
`0e2c57a3ce85e42d37707ff53364d506b42b1aa7723cd51cd828055d9936b16a`。

使用既有 WSL2 Ubuntu-24.04、Linux 6.6.114.1、Rust 1.98.1，离线、`--locked`。
构建输出为新建的独立临时目录；Owner 根和恢复目标均由合成 fixture 新建，未复制真实库。

```text
cargo +1.98.1 test -p enouia-memory-host --test connector_policy_semantics --test owner_read_grant --test remote_read_admin_cli --test remote_service --test remote_oauth --locked
cargo +1.98.1 clippy -p enouia-memory-host --lib --bin enouia-memory-remote --test connector_policy_semantics --test owner_read_grant --test remote_read_admin_cli --test remote_service --test remote_oauth --locked -- -D warnings
```

测试分别为 4 / 6 / 1 / 3 / 6，全 20 项通过；聚焦 Linux clippy 通过。
包括新授权的删除/范围/期限门槛、未知发布恢复、备份隔离和激活后的读取、实际二进制主人管理，
以及真实本机回环的 DCR/PKCE、六读工具、session_start 召回、许可换代和 OAuth 主动撤回。
首次启动检查的自动审批超时未执行，按工具提示重试后执行成功；没有因此跳过检查。
未安装工具链或依赖，未停止并行会话进程或关闭 WSL。增补只改文档，无实现/Schema/集成面变更，
未重跑已有 Windows 检查或完整 workspace suite。

## 迁移前 read-check 增补

2026-11-12，在 main `f932f5f` 上以独立分支 `codex/connector-read-preflight` 延续 ADR141。
此前设置授权和服务配置后，只能用实际启动服务来检验许可；现补 `read-check`，与服务共用
配置检查和短许可构造，不新增授权语义或持久数据类型。

Windows 和 WSL2 Ubuntu-24.04 均使用既有 Rust 1.98.1、离线、`--locked`，执行：

```text
cargo test -p enouia-memory-host --test remote_service --test remote_read_admin_cli --test owner_read_grant --locked
cargo clippy -p enouia-memory-host --lib --bin enouia-memory-remote --test remote_service --test remote_read_admin_cli --locked -- -D warnings
```

每个平台分别为 4 / 1 / 6，全 11 项通过，聚焦 clippy 均通过。Windows 格式检查、
diff 检查及 Runtime surface checker 通过；Schema/validator 未改，未重复 schema cross-check。
本片没有运行完整 workspace suite。

新增的 service 测试及实际 CLI 断言确认：

- 自检在配置端口已被占用时仍成功，不启动监听，不创建 OAuth 状态目录，snapshot 不变。
  输出只有模式、scope、截止时间、snapshot 和临时 scope hash，没有记忆原文。
- 旧配置明确显示当前形成策略模式；形成停用后失败，显式选择历史读取授权后成功。
  剩余一分钟时，检查用 permit 同样截止于授权到期。
- 原文越权、错误 Agent、额外来源、未知 grant、HTTP issuer、公网绑定地址、Vault 内的
  OAuth 状态路径、无效 opening profile 均被拒绝；授权到期或撤回后自检失败。
- 既有真实回环 DCR/PKCE、六读、session_start、换代和 OAuth 撤回回归继续通过。

自检结果不能被后续服务当作授权；临时 permit 返回前已撤回，服务仍按当前状态重新核验。
它不证明目录可写、端口可用、TLS、备份新鲜度、VPS 或真实 ChatGPT 已验收。

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
TLS 或部署验收。Linux 新证据仅限上节列明的 20 项测试和静态检查，其他既有报告不冒充新回归。
未读取原始聊天、操作真实 Vault、删除旧库、修改 Runtime 仓库或刷新手动项目状态快照。
真实来源、敏感度、原文权限和期限仍由 Morii 决定。授权实现已通过 PR #103 合并；Runtime 采用、
独立加密备份、真实恢复、迁移和主人日用验收仍未完成。本片不启动新 MV 阶段。
