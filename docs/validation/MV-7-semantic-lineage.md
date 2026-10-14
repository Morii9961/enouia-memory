# MV-7-H2i · 可选来源谱系输入

日期：2026-10-13；main 基线 `9f0d7e5`。范围见 [ADR-MEM-58](../adr/058-selected-source-lineage-input.md)，仍在 MV-7。

语义任务原先要求模型区分后代纠正与分支冲突，却没有提供父来源关系。新增 create_with_lineage 从已选来源修订读取直接父关系，仅输出所选片段之间的索引与未选父来源数量，不补读未选正文或原始 ID；未知父关系保留 null，缺口不等于没有祖先。元数据与额外指令进入既有任务 hash 和 64 KiB 总预算。

旧 create 的任务内容、键和 Request ID 推导保持原样。新 helper 的 Session/input 和 Request ID 都使用独立键空间；不可复用旧请求的批准。首次针对检查发现仅分开 Session 键仍产生 Request ID 冲突，已补齐独立推导并重测。恢复复核仍重算完整计划，模型输出仍经过原生引用、用户证据与实际所选父链检查。

验证记录：双 Provider 新旧任务共存、直接父链和分支、重复打开、缺失父来源、权限/锁及无候选写入的两项针对检查 exit 0。fmt、Clippy -D warnings、Python 12 项、独立 python-jsonschema、Runtime surface 和公开内容检查均 exit 0。全仓库逐例 Rust 检查尚未完成：按主人先前的近额度停工/关机要求，在五小时使用量约 91% 时中止；不能计为全量通过。来源修订后发送前拒绝的两种 helper 扩展检查仍须随全量复跑确认。默认并行 Workspace 稳定性沿既有报告保持 pending；不放宽超时。

不发送真实请求、不导入个人历史、不开启新 MV 阶段。Runtime 产品采用、真实语义忠实性/漏提和主人验收仍 pending；状态快照未刷新。

2026-10-13 停工位置：PR #17 为草稿，功能提交 `d9f21ba` 已推送；忽略的本地 mv7-h2i-check-results.json 与 mv7-h2i-workspace-tests.log 保留中止证据。H2h 的 379 项全量证据不作为本片完整检查。

## 2026-10-14 接续与请求 ID 修复

本轮从 `50497a4` 接续。复核发现仅将文字 namespace 加入新 Request ID 的 hash 输入仍有确定性冲突：旧 create 接受任意不超过 256 字节的主人键，恰好使用新任务的 namespace/hash 字符串即可得到同一个 Request ID。新增合成回归在修复前明确失败，两条独立输入的 Request ID 相同。

新 helper 的 hash 输入现在先置入超过旧键最大长度的域，再包含 namespace 与键 hash；旧 create 的任务字节、键和 Request ID 推导保持不变。两家 Provider 下，旧任务、新任务与构造键任务分别发送、重启恢复及直接父链/分支检查已通过。未合并的早期 H2i helper 只有合成证据；其旧推导创建的合成谱系任务须使用新主人键重建，不静默重绑、复用批准或重发。

本轮首次全量在沙箱内的 CLI owner-only ACL 检查失败。独立临时空目录复现了 protect 的 AccessDenied；获准沙箱外验证后 CLI 五项通过，未放宽代码或测试。修复发现后停止仍在运行的旧代码检查，对最终代码重新完整验证；这两次未完成运行均不计为全量通过。原始日志分别保存在忽略的本地 mv7-h2i-workspace-tests-resumed.log、mv7-h2i-workspace-tests-unsandboxed.log 与 mv7-h2i-request-alias-before.log。

最终验证：仓库根目录，Rust 1.98.1，CARGO_NET_OFFLINE=true、RUST_TEST_THREADS=1 的 cargo test --workspace --locked 共 381 项通过、0 失败、退出码 0，含 77 项 Provider、64 项 Workspace 检查。两种 helper 的来源修订后发送前拒绝检查通过；fmt --check、Clippy -D warnings、Python 3.12 / 12 项质量工具测试、独立 python-jsonschema 4.26.0（36 schemas）和 Runtime checker 均 exit 0。公开内容及 93 个本地链接检查通过，状态快照 hash 未变。最终全量日志为 mv7-h2i-workspace-tests-final.log，结果登记为 mv7-h2i-resume-check-results.json，均在忽略的本地验证区。

Runtime aggregate 为 `87901de47e760bdb3cc38a0753c40039b82d87d4719e6fac218387bfa5cc8363`，已同步 manifest 与采用记录，仍为 47 条命令。默认并行稳定性没有重跑或修复；本轮全量证据限于串行合成范围。PR #17 可进入审阅；合并、Runtime 产品采用、真实模型语义质量与主人验收仍独立，不开启 MV-8。
