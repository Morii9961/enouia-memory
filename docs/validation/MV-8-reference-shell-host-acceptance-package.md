# MV-8.1j 合成现场验收包准备

日期：2026-10-23（Asia/Shanghai）。接续[收口报告](MV-8-reference-shell-host-closeout.md)，限定于现有参考壳 Host 交付。**验收运行产物和两个合成根已准备；实际窗口、托盘和图形壳正常退出仍待验收。**

交付方向已按 Morii 的纠正收敛：Runtime 早已承载 Memory 的产品界面和 embedded Core；本包是旧参考壳的工程产物，不是新的产品 UI，也不是 Runtime 的替代入口。后续不用 Computer Use；未完成的参考壳图形项保留待验收，不作为继续 Runtime 产品工作的前置。

## 固定版本与交付物

- 分支：`codex/mv8-reference-host-closeout`。小修提交 `be044b4` 固定了 API → action catch 后的原 packet/key 保留及原五项 focused 检查记录。
- 验收程序和 fixture preparer 的源码提交：`eaa002f84a9452257e18e8512d7fe99f73089bd1`，包含上述小修。交付报告之后的文档提交不改变这组程序的源码 pin。
- 新增 [prepare_reference_host example](../../crates/enouia-memory-vault/examples/prepare_reference_host.rs)：只在操作系统临时位置创建随机全新目录、生成合成 genesis、限制 owner ACL、bootstrap Host v2 并验证租约重新取得。它没有输入根、导入、启动进程或删除功能，不能用于真实根迁主。
- 本地验收包位于忽略的 `.local/mv8-reference-acceptance-20261023/`。包括 Host、启用现有 `custom-protocol` 的参考壳、对应 x64 WebView2 loader、preparer、固定合成 A/B 根的本地元数据、手动 launcher、构建/进程观察记录和 README。没有安装、注册自启动或启动图形壳。

从仓库根目录，人工启动 A 的命令如下；B 和 embedded 入口详见包内 README：

```powershell
& .\.local\mv8-reference-acceptance-20261023\launch.ps1 -Fixture A
```

launcher 只读取此次准备的合成元数据和包内程序，返回新 shell PID。窗口需要人工操作，故该入口显式以可见交互方式启动；本轮未执行它。根不在仓库或同步目录内，不接受私人 Vault。元数据中的临时绝对路径和本地 IDs 不进入公共报告或 Git。

| 包内文件 | SHA-256 |
| --- | --- |
| `enouia-memory-host.exe` | `ff13b220043d13943edaa1ea5c5d56c4133bb4e9935f5088a6a7c0e525515bf3` |
| `enouia-memory-workspace.exe` | `59e500c3bfc430f8f9d7b1e3fa1c54778275c4dc6d457b5dee5d4f5c5e587ea2` |
| `WebView2Loader.dll` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` |
| `prepare_reference_host.exe` | `62c129d71847856217e9beb14251bd2b76241a6be1d3028185819cbb7271ae44` |

这些摘要用于固定本次手工验收产物，不能证明窗口、托盘或 Runtime 产品行为通过。构建为 pinned Rust 1.98.1 GNU、offline/locked、dev debug info 关闭、incremental 关闭、四 jobs；这是验收用 dev 程序，不是安装包或发布签收。

## 本轮实际检查与新增观察

| 检查 | 实际结果 | 证明边界 |
| --- | --- | --- |
| preparer 编译、格式及实际执行 | 创建两套新合成 v2 根；每套 bootstrap 后释放并重新取得 coordinator；成功 | 仅合成根准备，不是迁移既有根 |
| preparer 传入已有合成 A 路径 | 参数在创建目录前拒绝；A 的 CURRENT 摘要不变 | 不接受输入根；不证明任意根恢复 |
| `npm run build` | TypeScript 与生产 Vite build 通过；20 modules | 包含本轮小修，不是原生渲染观察 |
| 同版本 Host＋参考壳 build | 最终通过；实际嵌入新前端、现有 custom-protocol feature | 仅该验收配置；不等于 release 或安装通过 |
| 包内 Host＋合成 A，第一轮自有隐藏 Broker | ready PID 对应刚创建的 Host；Broker 正常退出 0 后 Host 自退 0 | 本轮没有连接/发送页面 packet，没有图形壳 |
| 同根第二轮独立 Broker/Host | 再次 ready；Broker 和 Host 均正常退出 0；两轮前后 CURRENT 相同 | 实际重新取得同一根的 lease；未用 kill；不核销 Tauri tray/exit 接线 |
| 文档、launcher 语法、source pin、DLL 来源及保护文件核对 | 交付前完成针对性核对 | 不扩展为全 workspace 验证 |

两轮进程结果保存于 `.local/mv8-reference-acceptance-20261023/owned-host-observation.json` 和对应 stdout/stderr。观察脚本只登记自己的 Broker PID，保留实际 Host 进程句柄，显式允许 Broker 正常退出，再等待该 Host 自退。没有扫描其他进程、调用应用命令、真实 Provider 或私人数据。

准备过程中保留两类失败：受限进程应用 owner ACL 后写新 Vault 被拒绝，在正常权限下运行同一 preparer 成功，没有放宽 ACL 或产品检查；原生构建的两次 `windres` 失败在普通/正常权限下相同。后者通过独立资源编译定位：Rust bin 在前时 GCC `cc1` 返回入口点缺失码 `0xC0000139`；只把 MSYS2 bin 提前，`cc1` 和同一 resource.rc 的 windres 立即通过，随后原生构建通过。只调整该次进程 PATH，未修改系统 PATH、crate、依赖或编译器版本。初始格式检查差异已由 pinned rustfmt 修正。

本轮没有重跑上一轮已通过且源码未变的五项 Node 检查，没有运行 full workspace tests/Clippy、schema cross-check、历史质量、真实 Provider、Runtime 或 Activity 检查。此次 build 用于产生明确获授权的现场验收程序，不把构建当作验收替代。

## 停止界面操作后的源码核查

后续只读核查了 `Backend::from_args`、`NativeHost::start/reconnect/shutdown`、Broker、HostService、pipe Client/server 及 overlay capability。页面请求仅使用 Call frame；页面 open/create/unlock 被拒绝；ticket 在握手、锁定和断线后撤销；未知调用先断开且不自动重发；重连仍使用保留的 Child 和原 endpoint；正常 shutdown 不 kill Host，启动失败清理限定于刚创建的 child。overlay 没有 exit、picker 或 startup capability。以上为源码核查及已有测试证据的对应关系，本轮没有启动程序或重复运行测试，也未发现需要新增修复的具体失败。

当前本片可用 CLI 完成的工程收口已完成；不把剩余旧参考壳图形签收作为 Runtime 既有集成或历史结果采用的阻塞。MV-8.2 的新 MCP 开发仍须明确解除本片原有的范围排除；本报告本身不构成授权。

## 剩余出口与 Runtime 声明

在随后一次已停止的观察中，仅确认了无参数参考壳主窗口显示 `core healthy / embedded`、`Vault：none`，并显示 `Ctrl+Alt+M` 被占用的提示。页面退出点击被用户的 Esc 停止，结果未知；没有开始合成 A 的图形 Host 模式。对应文字保存在忽略的 `embedded-window.txt`；截图含无关应用遮挡，不纳入交付材料。不查询或操作无关应用，也不将这次观察记为完整 H01 签收。

原清单 H01–H10 的完整现场签收仍为待验收：默认 embedded、显式 Host/错误启动、主窗口隐藏与恢复、overlay scope、托盘 lock/reconnect/unlock、异常 Host 退出反馈、托盘与页面正常退出、对照根生命周期隔离及不支持入口的实际反馈。“没有参考壳程序/持久合成 fixture”的准备缺口已消除，但不再继续操作旧壳，也不以更多服务模拟证据替代这些图形项目。

确定的提交后丢回执仍复用既有真实服务 fault 和小修回归；本轮没有增加故障注入命令或未知结果恢复按钮。此前“实际窗口/托盘无法由当前工具操作”是对工具能力的误判；当前停止原因是 Morii 已明确要求不用 Computer Use。以后只用 CLI、源码和已有记录核销本片；图形项目保留待验收，不默认要求 Morii 立即操作旧参考壳。

Runtime 的历史结果最小采用仍为 `9d68d3d8707749ad3b0e2a36cfc97d71a27e712f` / workspace IPC v1 / **55 commands** / aggregate `c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`。验收程序采用含小修的 surface aggregate `ad55bcf532c7cb44d9249537b68f7d986a1f4d50a3e752a3cef55f2e2aa44b34`；preparer 位于 examples，不改变 integration surface。没有改命令、存储、版本或传输协议；不要求 Runtime 追 main 或等待本片。

“Runtime 采用待完成”仅适用于这组 native Host 模式及相应产品验收，不能泛指 Memory 尚未接入 Runtime。本片没有重新读取 Runtime checkout，不声称核实其最新 UI 或 pin；既有产品归属按 [Runtime 集成说明](../integration/RUNTIME.md) 保持不变。

本片没有推送、PR、合并、部署、真实资料迁主、picker/Provider adapters、多客户端、MCP 或下一 MV 阶段。AGENTS、手工状态文件和旧实验原件继续保留；本轮追加的报告不是项目状态快照。
