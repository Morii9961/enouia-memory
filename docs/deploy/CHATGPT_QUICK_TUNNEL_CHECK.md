# 用 Cloudflare 临时隧道在本机验证 ChatGPT 连接器

[ADR-MEM-176](../adr/176-remote-tunnel-readiness.md)。正式部署需要域名（见
[REMOTE_MCP_CLOUDFLARE_TUNNEL.md](REMOTE_MCP_CLOUDFLARE_TUNNEL.md)）。在那之前，可以用
Cloudflare 的临时隧道（trycloudflare，不需要域名和账号），在本机把 ChatGPT 接到 Memory 上
验证一次。本页的路径和 ID 都是占位符；要不要做、接哪个库，由 Morii 决定。

## 先知道这几点

- 每次启动 cloudflared，地址都是随机的 `https://<随机名>.trycloudflare.com`。地址变了，
  配置里的 `resource`、`issuer` 要跟着改，ChatGPT 里的连接器也要删掉重加，旧 token 作废。
  所以它只适合验证，不适合日常使用。
- TLS 在 Cloudflare 终止，Cloudflare 能看到明文，正式隧道也是这样。临时隧道没有 SLA，
  只用于测试。
- 地址是公开的，谁都能访问元数据、客户端注册和授权页，但授权必须输入 Morii 在本机运行
  `approve` 打印的一次性审批码，10 分钟内有效。没有 token，任何工具都调用不了。
- 客户端注册最多 16 个。从没通过审批的注册 1 小时后自动作废，名额会空出来
  （[ADR179](../adr/179-unapproved-client-lapse.md)）；急用时也可以用 `oauth-revoke` 手动清除。
- **同一时间只能有一个进程打开库。** 从导出开始，到 serve 停止为止，Runtime 必须保持关闭。

## 事前准备（只做一次）

1. 安装 cloudflared（由 Morii 本人安装）：`winget install --id Cloudflare.cloudflared`。
2. 从已合并的 main 构建两个程序，放到仓库和数据根之外的目录：

   ```powershell
   cargo build --release --locked -p enouia-memory-host --bin enouia-memory-remote -p enouia-memory-cli --bin enouia-memory-owner-backup
   ```

3. 在数据根下建一个连接器目录（不能放在库目录里），例如 `<数据根>\connector\`。按
   [linux/remote.example.json](linux/remote.example.json) 的形状写 `remote.json`：
   - `vault_root`、`host_id`：库目录及其 `vault.json` 里的 `host_id`；
   - `state_dir`：`<数据根>\connector\oauth`；
   - `bind`：`127.0.0.1:8787`；
   - `agent_id`、`connector.producer_id`：给 ChatGPT 新分配的两个 `prn_` ID，
     不能和 Runtime 已登记的 Agent 重复；
   - `subject_id`：`vault.json` 里的 `owner_subject_id`；
   - `policy_ids`、`source_ids`：从 `read-status` 里挑一个该主体的形成策略，以及它的一个
     来源作为种子。授权会自动跟随这个主体的其余窗口和以后的新窗口，所以不用列全；
   - `max_sensitivity`：按 Morii 的决定填写；
   - `source_read`：是否允许 ChatGPT 读取来源原文（`memory_source`）；
   - `profile`：一份 AttentionProfile，形状见 `tests/fixtures/attention/profile.json`，
     `aliases` 可以留空；
   - `resource`、`issuer`：先随便填一个 https 地址，第 4 步再改。

## 当天的步骤

1. **关闭 Runtime**，从托盘里退出，确认没有其他进程打开库。
2. **导出并校验**，导出目录必须是空的：

   ```text
   enouia-memory-owner-backup export <库目录> <数据根>\backups\export-<日期>-seq<N>
   enouia-memory-owner-backup verify <同一个导出目录>
   ```

   两个命令都要退出 0，`valid=true`，并且 seq 等于预期的当前值。对不上就停下，不要继续。
3. **建立读取授权**：

   ```text
   enouia-memory-remote read-draft <remote.json> <到期时间，UTC，例如 2026-12-31T00:00:00Z> plan.json
   ```

   它只写 plan.json，不写库；同名文件已存在时拒绝覆盖，输出里有 `grant_id`。打开 plan.json
   检查 `grant`：
   - `principal` 是 ChatGPT 的 `agent_id`；
   - `purpose` 是 `chatgpt_connector`，`follow_new_sources` 是 true；
   - `max_sensitivity`、`source_read` 和到期时间都对。

   确认无误后，由 **Morii 本人** 执行：

   ```text
   enouia-memory-remote read-set <remote.json> plan.json --confirm-owner-read
   ```

   把 `grant_id` 写进 `remote.json` 的 `read_grant_id`，然后运行
   `enouia-memory-remote read-check <remote.json>`，应返回 `read_ready: true`。
4. **启动隧道**：`cloudflared tunnel --url http://127.0.0.1:8787`。记下它打印的
   `https://<随机名>.trycloudflare.com`，运行
   `enouia-memory-remote set-origin <remote.json> https://<随机名>.trycloudflare.com`
   （[ADR204](../adr/204-remote-set-origin.md)）。它把 `resource` 改成 `…/mcp`、`issuer` 改成这个地址，其他字段不动；
   也可以照旧手改这两处。
5. **启动服务**：另开一个窗口运行 `enouia-memory-remote serve <remote.json>`，保持窗口开着。
6. **在 ChatGPT 里添加连接器**：在开发者模式里新建自定义 MCP 连接器，地址填第 4 步的
   `/mcp` 地址，认证选 OAuth。授权页会要求输入审批码，这时在本机第三个窗口运行
   `enouia-memory-remote approve <remote.json>`，由 Morii 在 10 分钟内把打印出的码输入页面。
7. **验证**：在对话里打开这个连接器。
   1. 先让 ChatGPT 调用 `memory_search`，确认能读。
   2. 再让它对一段简短的对话调用 `memory_submit_conversation`，先 `preflight: true`，
      再正式提交，规则见 [CHATGPT_CONNECTOR_USAGE.md](CHATGPT_CONNECTOR_USAGE.md)。
   3. 提交之后马上再搜一次，应该能看到新记忆。
8. **结束**：在 serve 和 cloudflared 两个窗口分别按 Ctrl+C。打开 Runtime，确认能看到这条
   新记忆，设置页里有连接器授权（`chatgpt_connector`）。
9. **保留还是撤销**：
   - 隧道地址下次会变，可以用 `oauth-status` / `oauth-revoke ... --confirm-revoke-client`
     撤销这次的 ChatGPT 客户端。
   - 读取授权可以保留到到期日，下次验证直接复用；也可以用
     `read-revoke-plan <remote.json> <grant-id> --revoke`，再执行
     `read-set ... --confirm-owner-read` 撤销。

## 出问题时

- serve 因结果未知退出：不要直接重启，也不要换新的 `request_id` 重试。先运行
  `enouia-memory-remote recover <config> --confirm-recover`（[ADR205](../adr/205-remote-recover-command.md)），
  它会把写到一半的那次提交补完，或者报告库本来就是完整的；再启动 serve，让 ChatGPT 用同一个
  `request_id` 重发。`recover` 本身失败时，用最近一次校验过的导出恢复。
- ChatGPT 报授权失败：检查 `resource` 和 `issuer` 是不是当前隧道的地址，审批码有没有过期。
- 读不到新记忆：运行 `read-check`，确认授权有效、没有过期，`follow_new_sources` 是 true。

### 对照 serve 窗口里的日志

serve 窗口每个请求打印一行：时间、方法、路径、状态码、耗时，MCP 调用再加工具名，被拒的加
`error`（[ADR189](../adr/189-remote-access-log.md)）。日志里没有内容和令牌，可以放心截图给开发会话看。

| 看到的现象 | 日志里对应的行 | 怎么办 |
|---|---|---|
| ChatGPT 说连不上 | 完全没有新行 | 隧道没通：看 cloudflared 窗口有没有报错，地址是不是这次的随机名 |
| 添加连接器时失败 | `POST /oauth/register 400` | 回调地址不在 `redirect_prefixes` 里，或者 16 个客户端名额满了：用 `oauth-status` 查看，撤销旧客户端 |
| 授权页报错 | `GET /oauth/authorize 302` 后 ChatGPT 显示错误 | `invalid_target`：`resource` 和填入的地址不一致（末尾斜杠没关系，[ADR197](../adr/197-oauth-chatgpt-tolerance.md)）；`invalid_scope`：请求里一个配置的 scope 都没有 |
| 输入审批码后失败 | `POST /oauth/authorize 400` | 审批码过期或输错：重新运行 `approve` |
| 能授权但调用失败 | `POST /mcp 401` | 令牌过期或被撤销：在 ChatGPT 里重新连接 |
| 用着用着要求重新授权，或反复失败 | 一串 `POST /oauth/token 400` | 刷新令牌被判为重放（同一个旧令牌两分钟后又被用，或者来自别的客户端，[ADR207](../adr/207-refresh-reuse-grace.md)），整组令牌已撤销：在 ChatGPT 里删掉连接器重新添加，再用 `approve` 拿新审批码 |
| 工具调用报错 | `tool=… error` | 让 ChatGPT 把返回里的 `problems` 原样贴出来，按提示改；常见的已在 [CHATGPT_CONNECTOR_USAGE.md](CHATGPT_CONNECTOR_USAGE.md) 的“出错时”列出 |
| 偶尔一次 404 | `POST /mcp 404` | 会话过期后客户端会重新初始化；连续出现再看是否换过令牌（[ADR200](../adr/200-remote-session-continuity.md)） |
| serve 启动就退出 | 窗口里 `vault: Busy; another process holds the Vault` | Runtime 或别的命令开着主库：先关掉再启动（[ADR194](../adr/194-remote-busy-hint-and-vps-renewal.md)） |
