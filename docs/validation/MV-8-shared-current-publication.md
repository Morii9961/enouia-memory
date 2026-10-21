# MV-8.1c · 数据与主权共同发布

> Historical 2026-10-18 evidence from the retained isolated worktree. Current mainline integration and final check results are recorded separately in [the 2026-10-21 report](MV-8-authority-integration.md). Original failure and scope statements below are preserved.

2026-10-18。接续主人明确授权的 MV-8 与持续工作指示。实现位于
`codex/mv8-host-contracts`，起点 `56bc5eb`；所有本轮改动仍未提交、推送或合并。

## 实现与证据边界

[ADR-MEM-68](../adr/068-shared-current-authority-publication.md)定义显式 Host
模式：CURRENT 可选引用不可变 authority，普通数据与主权只通过同一次 CURRENT
替换发布。旧根省略字段，编码保持兼容。原生 HostCoordinator 持 host.lock，
每次操作持 writer lock 重读状态、校验实际文件、检查原生身份与 quiescence，
之后才允许重放或写入。promotion 后的数据提交记录新的 primary identity。

bootstrap 先持久化绑定 Vault 的 mode marker。中断、CURRENT/authority 缺失或
损坏不自动修复、不降级为旧写入。普通 commit、恢复采用、purge、sweep、旧格式
export/reconcile 被持久栅栏拒绝；早期 authority-only 试点不自动升级或混用。
部分启用失败目前只能拒绝，显式恢复工具尚未实现。

数据摘要重新校验 descriptor、完整提交链及其 segment、保留 revision、未 purge
的对象实际字节与哈希，并绑定 commit/policy/deletion 水位。它是完整 pinned 数据
闭包校验，不覆盖可变审计、凭据、平台元数据，也不是目标机器完整恢复验收。
本片模拟同一合成根上的身份转换，不证明跨主机切换、Linux/VPS 或断电恢复。
没有 Host 可执行进程、认证 IPC、MCP、Runtime 接入或真实迁移入口。

## 检查记录

首轮新增 8 项定向测试全部通过：完整 lifecycle、持久旧入口与导出栅栏、暂停/晋升
和原生身份、缓存存在时仍发现实际对象损坏、丢响应重放、部分启用和损坏拒绝、
真实跨进程租约，以及发布前/后进程 abort 后只恢复相匹配的旧/新数据与主权。
随后补充新主实际数据写入身份断言；其验证由下述最终全套覆盖，首轮证据不冒充
最后版本证据。

第一次在原 E 盘 target 编译失败：真实磁盘空间不足（os error 112），不是注入错误。
没有清理缓存或用户文件。构建转到 C 盘独立临时 target；固定 Rust 1.98.1、MinGW、
offline/locked，关闭 incremental 和 dev/test debug 信息以控制空间。未改变测试
并发度、超时或产品逻辑。首轮定向输出在忽略目录 `.local/mv8-host-transaction-targeted.log`
及对应 exit JSON。磁盘失败的编译输出保留在本次会话工具记录中。

最终 required checks 正在执行；以 `.local/mv8-shared-final-checks.json` 和
`mv8-shared-final-{fmt,rust,clippy,schemas,surface}.log` 的完成退出码为准。不得把运行中
日志当通过。此前两片全套的旧 ordered barrier 失败与一次隔离通过继续保留，未修复。

Runtime 接入面 aggregate：`5f0062b30d437f6a9edc81c332f293a4f48c083f563c8544ae2f7848e13dfdda`，
已同片记录于 RUNTIME.md。Dashboard 快照未更新。Memory 主目录用户 AGENTS.md
哈希仍未改变；Runtime 并发工作及前一片 MV-7 工作树完整保留。

## 接续

先读取本报告最终结果和保存日志，核对当前 Git/工作树，不能只用旧交接的 main。
后续仍在已授权 MV-8 内：明确部分启用与损坏 CURRENT 的 Host-aware 恢复，
再接用户级唯一进程、原生认证/会话撤销/版本握手、参考壳断连重连，之后才实现
受限 MCP。当前 API 的原生身份与 owner approval 依赖调用方，不构成已认证远程入口。
不要将旧备份手工恢复或旧二进制访问声称为已受分布式栅栏保护。

MV-7 P02～P06 仍由 Morii 在凭据对话框输入密钥并亲点 Send once。实际资料、
Provider 请求、生产迁移/部署和 PR 合并边界不变；本片未取得这些授权。
