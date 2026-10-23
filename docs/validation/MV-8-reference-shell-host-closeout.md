# MV-8.1j 参考壳 Host 交付收口

核对日期：2026-10-23（Asia/Shanghai）。本片只收口 ADR-MEM-79 的既有参考壳行为，不开启新阶段。**实际窗口、托盘和图形壳正常退出仍待验收。** 本轮修复一处已复现的原请求丢失；没有新增原生界面观察。

## 基线与保护范围

- 本地 `main` / HEAD 与 [GitHub main](https://github.com/Morii9961/enouia-memory/tree/b0b891d7daa2aed49029b50b36cbef58335669e7) 均为 `b0b891d7daa2aed49029b50b36cbef58335669e7`，即已合并 #48。普通 Git 网络读取失败后，通过 GitHub connector 的只读 branches/main 接口核实；没有 fetch、切分支或覆盖工作区。
- 读取跨项目交接、仓库 AGENTS、[ADR-MEM-79](../adr/079-reference-shell-native-host.md)、[原 MV-8.1j 报告](MV-8-reference-shell-host.md)及下表证据。启动时已有的 AGENTS 和两个手工状态文件修改不属于本片。
- 本片文件允许列表：`apps/workspace/src/App.tsx`、`apps/workspace/e2e/host-api.test.mjs`、`docs/integration/runtime-surface.json`、`docs/integration/RUNTIME.md`、原 MV-8.1j 报告和本报告。没有暂存、提交、推送、合并、部署或清理其他内容。
- 本轮修复是上述 main 基线上的未提交补丁；`b0b891d7` 本身不包含它。后续验收程序必须明确包含这份补丁，不能拿旧 exe 宣称已验证修复。
- 只读 worktree 注册表确认旧 `mv8-host-contracts`、`existing-extraction-preparation` 等实验仍保留。当前 Host 路线以 ADR-MEM-73 至 79 和已合并实现为准；保存历史结果以 #40 / ADR-MEM-72 为准。旧 `extraction_prepare_saved` 草稿不作为现行接口，不整块合并旧草稿，也不修改原件。

## 出口表

“已有”表示复用已保存的合成证据；不是本轮重跑，更不是人工签收。

| 出口 | 已有证据及本轮补充 | 结论与尚缺证据 |
| --- | --- | --- |
| 默认 embedded；Host 仅显式选定程序、existing v2 根、Host/Vault ID | `Backend::from_args`、`Launch::parse`；既有参数拒绝测试和实际 Host 子进程 drill | 实现/合成证据已有；默认图形启动、完整/错误参数的原生对话框待验收 |
| Host 启动失败不构造 embedded，不 bootstrap/升级根 | 原参考壳报告、启动分支和自有启动失败子进程清理逻辑 | 代码边界已有；错误程序/错误 Vault ID 下的实际窗口和根不变观察待验收 |
| 真实 Library 管理/审核、同 key 回放、锁定时保留 lease | [Host Workspace service](MV-8-host-workspace-service.md)、既有 reference-backend drill | 已有；不再加一批服务模拟测试替代图形验收 |
| 原生窗口 scope、队列前 ticket、页面不能选择控制 tag | `workspace_call`、`window_scope`、Broker；[native reconnect](MV-8-native-client-reconnect.md)和[Windows transport](MV-8-windows-host-transport.md) | 原生适配器/真实 pipe 证据已有；实际 main/overlay 与 IPC 接线待验收 |
| 不确定回执保留原 packet/key，不自动重发 | 真实服务丢回执 fault；生产 API 四项既有测试；本轮新增一项调用者错误状态回归 | **补齐 `useAction` 内存错误状态保留**，五项 focused Node 检查通过；原生窗口错误展示待验收；没有可点击的恢复或跨 renderer 持久化能力 |
| 托盘 lock/reconnect/unlock 使用同一自有子进程、原 endpoint；旧 ticket 失效 | 既有真实三进程 drill、reference-backend drill；源码持有 Child，不发现或替换其他 PID | 后端行为已有；菜单点击、对话框反馈、窗口响应和实际子进程身份连续性待验收 |
| 主窗口关闭只隐藏，快捷键显示 overlay，托盘重新显示主窗口 | Tauri 配置及窗口事件接线 | 实现已有；实际窗口/托盘/快捷键行为待验收 |
| 托盘/页面 exit 先尝试 lock/disconnect；Host 随所属 Broker 进程退出释放 lease | 既有独立 Broker 正常退出后 Host 自退与 lease 再获取；shutdown/exit 接线 | **实际图形壳正常退出未证明**。reference-backend 测试最终 kill 自有 child 不能核销此项 |
| Host 已退出时只报重启要求；不换 PID、不重启 Host、不回落 embedded | `NativeHost::reconnect` 的 retained Child 检查 | 代码边界已有；实际托盘反馈及无新增 Host 的观察待验收 |
| Runtime 产品采用、个人日用验收 | 本片不读取 Runtime、不访问私人 Vault 或真实 Provider | 均未证明；参考壳通过也不能代替它们 |

## 必要小修与实际检查

未知回执到达 `api.call` 时，`CallError.originalRequest` 已保存原 `requestId`、`idempotencyKey` 和克隆参数。然而 `useAction` catch 原先仅保存文字和可选 retry 回调，离开 catch 后就不再持有该 packet。新增 focused 回归在原代码上出现 **4 passed / 1 failed**：错误状态中的原请求为 `undefined`。

小修只让 `useAction` 错误状态保存该原请求；仍按既有 `retryable` 控制按钮，未知结果没有 retry 回调。没有新增按钮、自动重连、自动重发、请求键、存储或传输行为。保留限于该 hook 的当前错误状态：显式 clear、新 action、组件卸载或 renderer 退出会清除它；不承诺持久恢复，也不将原 packet 渲染到页面或写入日志。`useLatestRead` 不承担此写操作错误状态，未改动。

| 本轮实际执行 | 结果 |
| --- | --- |
| 新回归针对原代码运行 `node --test e2e/host-api.test.mjs` | 预期复现失败：4 通过、1 失败；丢失原请求 |
| 小修后同一 Node 命令 | 5 通过、0 失败；检查 API → action catch → 再读取错误状态的 packet、原参数、无 retry 回调及仅一次 invoke |
| `npm run typecheck`（`apps/workspace`） | 通过 |
| `python tools/integration/runtime_surface.py --write`，随后默认检查 | 接入面已再生成并核对；仅 App.tsx 文件摘要及 aggregate 改变，55 命令不变 |
| 允许列表 diff、文档链接/格式和 manifest 比较 | 交付前核对；原有三项他人修改未纳入本片 |

Node 新回归只替换最小 hook 状态端口和 invoke，运行生产 hook/API；它不是 React 渲染、WebView、原生窗口或托盘证据。本轮不运行 Rust tests、Clippy、前后端 build、schema cross-check、Provider、历史质量、Runtime 或 Activity 检查。未改 Rust、schema、存储、依赖或版本，没有扩大验证的具体原因。

复用的保存证据：`.local/mv8-reference-native-acceptance.log` / `.exit` 中的 15 项参考壳测试及启用的实际 Host drill；`.local/mv8-broker-host-final.log` / `.exit` 中的 Host 测试；`.local/mv8-reference-api-final.log` 中的原四项 API 检查；`.local/mv8-reference-workspace.log` 中最终通过的 pipe/Broker/fault 场景。原 `mv8-broker-focused.log` 保留了错误 CandidateId fixture 的早期失败，不能代替 final 成功记录。原报告的 490 项全 workspace 和 Clippy 仍是历史证据，本轮没有重跑或将其改称新增证据。

## 人工验收前置与操作

当前工具没有原生 Windows 窗口/托盘控制能力。本地 `target/debug` 有 Host exe、CLI exe，`target/debug/deps` 有参考壳**测试程序**；未发现可直接验收的 `target/debug/enouia-memory-workspace.exe`。前端 `dist` 是单独的 Web 构建，不能单独证明 Tauri shell。没有据此启动不可观察、不可正常关闭的窗口，也没有为清单反复构建。

开始人工验收前，准备同一固定工程版本的 Host 和 reference shell（连同本轮小修），并记录构建来源。兼容摘要或文件时间不能证明程序相同。使用独立本机固定磁盘临时目录的**已初始化合成 Host v2 根 A**，另备独立根 B；从对应合成 fixture 记录取得 `dev_...` / `vlt_...`，禁止猜 ID、复制私人 Vault 或读取私人日志。已有 reference-backend 测试会自动清理根，不能把它运行结束后的临时路径当可用 fixture。若没有合成 v2 fixture，先停在此前置，不让页面或普通 CLI init 冒充 Host bootstrap；可信准备步骤可参照 `native_host.rs` 的 `explicit_reference_backend_runs_its_own_synthetic_host_child`：`Vault::create` → owner ACL → `HostCoordinator::bootstrap` → 记录 IDs → 释放准备者句柄。此准备不能用于真实根迁主。

设置四个变量为上述已确认的绝对路径/合成 ID，再从交互式 PowerShell 显式启动（值不写入公共报告）：

```powershell
& $shellExe --host-executable $hostExe --vault $syntheticRootA --host-id $syntheticHostIdA --host-vault-id $syntheticVaultIdA
```

每项记录执行时间、版本、自己拥有的 shell/Host PID、动作及可见结果；截图只包含合成内容。未执行项保持“待验收”。

| 编号 | 精确操作 | 预期结果与取证范围 | 当前 |
| --- | --- | --- | --- |
| H01 | 单独无参数启动 shell；不选择任何已有资料。观察托盘后通过“退出”结束 | embedded 默认；没有 Host 子进程，没有 Host reconnect/unlock 菜单；没有自动打开私人根 | 待验收 |
| H02 | 按上方命令启动 A；记录本次父子 PID；在“候选审核”填入合成原话和主题键，点击“保存为候选”；只检查 pending，不批准 Canonical | 一个明确自有 Host；可见 pending 候选；无 picker/Provider 前置，不发送模型请求 | 待验收 |
| H03 | 点主窗口 X；确认 shell/Host 仍存活；托盘选“显示 Enouia Memory”；按 Ctrl+Alt+M（或明确选定的未冲突字母）；关闭 overlay 后再显示主窗口 | X 只隐藏，主窗口可恢复，overlay 为独立快速搜索窗口；快捷键冲突须单独记录，不视为 scope 通过 | 待验收 |
| H04 | overlay 搜索已明确批准的合成记忆；若可用开发工具，只从该 overlay 的现有模块 invoke `workspace_call`，传 `remember` 且在 JSON 中声称 `window: main`；也尝试页面 `vault_unlock` | 搜索成功；写入/伪造 scope 被拒绝、无新候选；页面不能变成原生 unlock。无可用开发工具则仅可签收可见搜索，伪造 IPC 项继续待验收 | 待验收 |
| H05 | overlay 显示时，托盘点“锁定 Vault”；再打开主窗口。点“原生 Host：重新连接”，随后点“原生 Host：解锁”，每步确认对话框 | lock 隐藏 overlay；同一 Host PID/lease；锁定时单纯 reconnect 的握手可被拒绝，应给出反馈，不自动解锁或重发写入；显式 unlock 建立新会话，候选数保持既有结果 | 待验收 |
| H06 | 在 A 仍锁定时，用相同显式参数另启动一个 shell 争用 A；确认错误框后关闭该失败 shell。若同时启动 B，仅使用独立 B 的参数并记录其所属 PID | 争用失败可见，不构造 embedded；A 原 Host 不受影响。B 可作为本轮自有对照，不构成多客户端 enrollment；退出 A 时 B 应保持存活 | 待验收 |
| H07 | 先正常退出 A 的原 shell/Host，避免 lease 争用掩盖握手失败。另起失败启动案例：缺少 `--host-vault-id`；再用 A 的正确配置但故意给错的合成 Vault ID。每次确认错误框并结束该次启动 | 失败可见，未出现可工作的 embedded 页面；已存在 A 的 CURRENT 不变；失败清理只涉及该次刚创建的 child | 待验收 |
| H08 | 正常启动 A 后，仅终止已核实的本次自有 Host PID；在壳中触发一次读取，随后分别点托盘 reconnect/unlock | 显示断线/Host 已退出、需重新启动的原生错误；不产生新 Host，不扫描其他 PID，不改用 embedded，不自动重发原写入。故障终止不能作为正常 exit 证据 | 待验收 |
| H09 | 先退出 H08 仍存活的旧 shell。正常重启 A，记录 PID；托盘点“退出”。确认 shell 退出后 Host 自退；再显式启动 A 并读取候选。另一次启动通过“运行状态”页的“退出 Enouia Memory”结束 | 两条退出路径均先尝试 lock/disconnect；Host 自退并释放 lease；后续可打开 A。观察 15 秒仍未退出则记录 PID/失败，不用 kill 补成通过（15 秒是人工观察限额，非协议 SLA）；若 B 作为对照运行，其 PID/根不受影响 | 待验收 |
| H10 | Host 运行时尝试现有 picker、页面切根/解锁及自启动设置；不选私人路径、不启用登录启动 | 明确拒绝/不可用；固定 A 不变；页面无法伪造原生控制 | 待验收 |

H04 的搜索需要先以现有计划/明确确认流程批准一条合成候选，不能把 pending 未被搜索到当搜索失败。H05 的“锁定期间独占 lease”和旧 ticket 拒绝仍依赖已有后端证据；菜单看到成功文字不能独立证明它们。

“写入已提交、回执丢失”的实际图形展示仍未覆盖。现有壳没有安全、确定地在提交后截断单次回执的人工开关；H08 杀 Host 不能指定该时间点。复用既有真实服务 fault 和本轮 hook 回归，不新增产品故障命令/注入框架，也不把浏览器伪造错误当真实 native IPC 观察。若后续现场遇到未知结果，保持当前错误状态以保留 `originalRequest`，不要清除/导航/刷新或当新操作提交；记录其原 packet/key，并由已有明确恢复流程核实。当前参考壳没有恢复按钮；这项展示仍标待验收。

## 给 Runtime 的稳定契约声明

Runtime 历史结果采用的接口前置已由 [#40](https://github.com/Morii9961/enouia-memory/pull/40) 提供，最小固定提交仍为 **`9d68d3d8707749ad3b0e2a36cfc97d71a27e712f`**，workspace IPC v1、**55 commands**，aggregate **`c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`**。该提交是已核对 main 的祖先；两处 manifest 的命令列表一致。

`b0b891d7` 的 aggregate 为 `d855683e0879c7887aae819b1345e31587a9a956be5fff5bf31de42ed08478d5`。本轮仅参考 UI 错误状态小修使接入面 aggregate 变为 **`ad55bcf532c7cb44d9249537b68f7d986a1f4d50a3e752a3cef55f2e2aa44b34`**；它不是新协议或要求 Runtime 追 main 的指令。命令、schema、存储、版本和传输协议均未改变。Runtime 无须等待本片人工签收，也不需要为历史结果采用 Host/MCP/云端。

若 Runtime 以后另行采用 native Host，匹配程序、原生 scope/ticket、连接生命周期及原 packet/key 的责任仍适用；此处只补明调用者也必须保留 API 给出的原 packet，不能 catch 后只留下文字。实际产品采用、picker/Provider adapters、个人验收仍为独立出口，本片不实施它们。
