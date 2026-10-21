# Host authority v1 · MV-8.1a

2026-10-18；[ADR-MEM-66](../adr/066-host-authority-and-platform-contracts.md)。
这是新增的主权协议与平台端口，不是当前 Vault 的格式升级，也不是部署说明。

## 状态与水位

`AuthorityState` 用 Vault ID、文档 revision、`primary {host_id,
primary_epoch}` 和 checkpoint 标识权威状态。host_id 使用已有 DeviceId，
由原生宿主确定，不能从普通客户端参数获取。epoch/revision/commit sequence
为正的 JSON 安全整数；policy/deletion epoch 为非负安全整数。

checkpoint 记录 commit ID、sequence、完整快照摘要和 policy/deletion epoch。
摘要覆盖获选快照的来源对象、正式记录、候选、审核、会话、删除台账及策略；
不能拿索引摘要或对象数量代替。实际完整性必须由存储适配器验证，结构验证
不能认证调用方填入的 digest。原始文档上限 256 KiB，重复解码字段名、未知字段
及未知主版本拒绝。所有字段按 schema 严格解析，不保存路径或秘密。

初始主为 epoch 1。每次状态变化递增文档 revision；迁主才递增 primary_epoch。
初始化只用于主人明确批准的新 authority 引导；状态丢失、损坏或暂时不可达时，
禁止自动调用 initial 重建 epoch 1。
普通写入还必须使 commit sequence 恰好递增一次，不得倒退 policy/deletion 水位。
旧版本、溢出、同提交不同摘要或错误 Vault 均拒绝，不自行修复或回填旧库。

## 迁移顺序

1. 原生控制面基于最新 active 状态构造精确 TransferPlan：Vault、操作 ID、
   原主、目标、基准 revision 和 checkpoint。plan_hash 是固定 canonical JSON
   的 SHA-256，主人确认这份精确计划。模型/导入文本不能提供确认。
2. 在独占事务中复核原主和完整计划，发布 quiesced。此后所有数据写入拒绝。
   适配器必须先停止接单、取消或完成有界工作并确定最后持久提交，不能只改标记。
3. 复制并在目标独立验证完整快照及独立恢复材料。只有目标读回的 checkpoint
   与计划完全相同，主人再次确认后，才能在同一权威存储中发布目标 active、
   新 epoch 与 last_promotion 收据。准备计划、比较摘要均不授予客户端身份。
4. 原主使用旧绑定访问最新权威时返回 fenced。提升的准确重放返回原状态；
   相同操作 ID 改正文、错误证明、错误目标或旧迁移不能再次提升。
5. quiesced 时可由主人中止并恢复原主，不降低水位或 epoch。提升后旧计划的
   abort 必须拒绝。回迁是从最新主开始的新迁移，不能恢复旧 authority 文件。

第一版最多保留 128 次 promotion 的全部精确计划收据；达到上限拒绝新增迁移，
不能自动清理收据或重新使用已有操作 ID。跨越后续迁移的精确 promotion 重放
只返回当前状态，不重新发布或提升 epoch。成功 abort 的旧计划不可再次使用；
状态已不匹配时返回冲突，不能盲目重试。没有自动重发、自动回滚或按时间抢占旧主。

## 平台端口与事务边界

新增 `AuthorityStore::transact` / `AuthorityTransaction` 定义单个 Vault 的独占
原子事务；事务必须读取最新 authority 和已验证数据 checkpoint，校验相同的
expected revision，再持久化 authority。普通数据写入也必须在该事务内完成。
成功只能在完整提交的持久化条件满足后返回；提交后响应丢失则重新读取状态，
按精确收据恢复。未提交故障不得暴露部分发布。错误统一为有界 HostFault，
不得带正文、路径或 token。事务实现不能把返回错误等同于“肯定未提交”。

这两个完整事务端口尚无生产适配器。后续 MV-8.1b 的
[ADR-MEM-67](../adr/067-authority-control-file-persistence.md)提供显式调用的 Windows
主权控制文件试点，持有 host/writer 双锁，只允许同 checkpoint 的控制转换；
不实现普通数据写事务，也不自动保护租约结束后的旧 Core/CLI。
基础的 WriterLock/AtomicFile 端口继续有效；
Windows/云平台分别证明真实进程锁、同卷原子替换、目录持久化、受限数据根、
只允许 owner 的 ACL/等效权限，以及 authority 与数据的一致发布。
Provider 密钥继续通过既有 SecretStore/SecretBytes 原生端口处理，不进入
authority、计划、Vault、MCP 或日志；本片不访问系统凭据。

主权查询失败时返回 unavailable，不信任缓存快照。本地副本即使具有相同 Vault
ID 与旧 epoch，也不能据此写入。纯 transition 的 `check_writer` 只比较所给状态，
不是认证或持久写入许可；必须在真实存储事务里调用。复制旧状态后自行检查
仍可能通过，合成模型不会把这个限制隐藏成跨主机栅栏已生效。

## 本片出口与后续

本片交付严格 schema、纯状态转换、平台端口和合成失败路径；现有 Vault 格式、
47-command workspace IPC、嵌入 Core 和 Runtime pin 不变。MV-7 真实 P02～P06
继续 pending，Dashboard 快照不刷新。

后续 MV-8.1 必须实现持久事务与故障恢复、用户级 Host 进程、认证 IPC、会话
撤回/锁定、版本握手和参考壳重连，再由 Runtime 显式采用。MCP 六工具及真实
客户端验收在此基础上实现。真实数据迁移、密钥、云服务信任、恢复秘密和生产
切换仍在各自激活点由 Morii 决定。

## MV-8.1c 接续

[ADR-MEM-68](../adr/068-shared-current-authority-publication.md)将普通数据与主权通过同一 CURRENT 原子发布；HostCoordinator 每次持 writer lock 重验实际文件并检查原生身份。旧根编码不变；显式启用后的 marker 和主权目录阻止旧写入、清理、恢复采用与有损导出。digest 只覆盖完整 pinned 数据闭包，不覆盖审计、凭据或平台元数据；不是完整迁移恢复证明。当前没有生产入口，早期 authority-only 试点不自动升级。前文各片出口描述保留历史范围。
