# 把主库从本机搬到 Linux VPS

[ADR-MEM-177](../adr/177-vps-move-rehearsal.md)。长期路线是主库放在 VPS 上，由 ChatGPT
通过远程服务读写（[REMOTE_MCP_LINUX.md](REMOTE_MCP_LINUX.md)）。本页说明怎样用现有的
导出、恢复和激活工具搬过去。路径、主机名和 ID 都是占位符。什么时候搬、搬到哪台机器，
由 Morii 决定。

## 先知道这几点

- **只能有一个主库。** 从导出开始，本机的库就不能再写入。搬完以后，本机那份只是旧副本：
  Runtime 要么不再打开它，要么只做只读查看，并且清楚它不会再更新。本机和 VPS 两边同时
  写入，会让两份库分叉，而且无法自动合并。Runtime 以后怎样读 VPS 上的库，属于路线里
  “本地副本”那一阶段，需要另外设计。
- 导出包含全部明文，传输和存放都要放在只有 Morii 能访问的地方。
- 导出不包含 OAuth 状态和令牌。搬到 VPS 后，ChatGPT 的连接器要重新添加、重新审批。
  读取授权在库里，会跟着搬过去。
- 已删除和已撤回的记忆在导出里仍然保持删除状态。

## 步骤

1. **本机停用**：退出 Runtime，停止本机的 `serve` 和 cloudflared。
2. **本机导出并校验**，目录必须是空的：

   ```text
   enouia-memory-owner-backup export <本机库目录> <空导出目录>
   enouia-memory-owner-backup verify <导出目录>
   ```

   记下输出里的 seq 和清单哈希。
3. **传到 VPS**：把整个导出目录打包，例如用 tar，通过 scp 传到 VPS 上服务账户的私有目录，
   例如 `/var/lib/enouia-memory/import/`，权限设为 0700。在 VPS 上以服务账户再运行一次
   `verify`，确认 seq 和清单哈希与本机一致。
4. **准备 VPS 上的数据区**：按 [REMOTE_MCP_LINUX.md](REMOTE_MCP_LINUX.md) 建好服务账户
   和数据区，包括 `DO_NOT_DELETE.USER_DATA` 和 `.enouia-owner-data.json`，再建一个空的
   恢复目录，例如 `/var/lib/enouia-memory/home/vaults/<vault_id>`。
5. **恢复并激活**，以服务账户运行：

   ```text
   enouia-memory-owner-backup restore <导出目录> <空恢复目录> --confirm-restore
   enouia-memory-owner-backup activate <恢复目录> <导出目录> --confirm-current-backup
   ```

   恢复完成后、激活之前，库不能打开。激活由 Morii 确认：这份导出就是最新的，本机在导出
   之后没有再写入。
6. **配置服务**：从 [linux/remote.example.json](linux/remote.example.json) 开始改：
   - `vault_root` 改成恢复目录；
   - `host_id` 保持库描述符里原来的值；
   - `state_dir` 改成 `/var/lib/enouia-memory/oauth`；
   - `resource`、`issuer` 改成正式域名；
   - `read_grant_id` 沿用本机已经确认的授权；
   - 种子策略和来源不变。

   然后运行 `enouia-memory-remote read-check <config>`，应返回 `read_ready: true`。
   授权到期或想换范围时，按 [CONNECTOR_READ_GRANTS.md](CONNECTOR_READ_GRANTS.md) 处理。
7. **上线**：安装 systemd 单元（[linux/enouia-memory-remote.service](linux/enouia-memory-remote.service)），
   配好 Caddy 或命名隧道，然后启动服务。在 ChatGPT 里重新添加连接器，在 VPS 上运行
   `approve` 拿审批码，输入后先搜索一次，再提交一段对话验证。
8. **本机副本**：保留导出包作为搬迁前的备份。本机的旧库不再写入；要删要留由 Morii 决定。

## 出问题时

- `verify` 失败：重新传输，不要改导出里的任何文件。
- `restore` 中途失败：这个目标目录作废，保留它以便排查，换一个空目录重新恢复。没有清理命令。
- `activate` 拒绝：说明导出与恢复内容不一致，或者不是同一份导出。重新导出、重新恢复，
  不要强行合并。
