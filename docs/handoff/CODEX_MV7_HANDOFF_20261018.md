# Enouia Memory · Codex 交接（2026-10-18）

Claude 按[10 月 17 日交接](CLAUDE_MV7_HANDOFF_20261017.md)接手后，依次完成了 Morii 指定的几项工作，现按主人要求停下，交给 Codex。本文与[启动提示词](CODEX_MV7_START_PROMPT_20261018.md)只供人工交接，不启动会话，也不授权新工作。接手时以现场 Git、PR 状态和下面引用的验证记录为准。

## 已完成

**1. 补完检查后合并 Memory #17～#33。** 10 月 17 日交接时全量 Rust 被中断、严格 Clippy 没跑完。合并前在 `1f170d7` 上补跑了全部 required checks：Rust 395 通过、0 失败，严格 Clippy、fmt、python-jsonschema 交叉校验（36 schemas）和接入面检查（aggregate `2c1c0640…`）都 exit 0，日志在忽略目录 `.local/mv7-merge-*.log`。随后按顺序逐个改 base 为 `main`，再用合并提交合入。现在 Memory main 是 `56bc5eb`，树与 `1f170d7` 一致。远端分支没有删除。

**2. Runtime 采用新 Core。** Runtime PR #77 已合并为 `07584cb`：pin 从 `8186aed` 改为 `56bc5eb`，surface `2c1c0640…`，47 条命令不变，`Cargo.lock` 只改 Memory 来源行。依 ADR-MEM-65，删除影响里的“其他记忆”现在也包括只把被删来源当作上下文读过的记忆，所以 Runtime 文案从 “would lose evidence” 改成 “would lose a recorded source (cited evidence or read context)”。检查：Rust 66 项和 Clippy，Node 98 项，DOM 53 项，桌面构建，pin 检查与 Memory checkout 一致，`--forget-only` 16/16，默认 smoke 101/101。第一次默认运行在 autostart 步骤遇到 node 驱动退出 `0xC0000409`，没有失败项，重跑未复现。合并时 Runtime #75/#76 已先入 main；合并后的树重跑前端检查，Node 98 项、DOM 55 项通过。详见 Runtime `docs/validation/Memory-pin-56bc5eb-2026-10-17.md`。

**3. 真实双 Provider 验收的准备。** 用 Runtime `07584cb` 构建了当前用户安装包（SHA-256 `7d17c3fb…`），连同空的合成 Vault 文件夹、不含密钥的设置草稿和说明放在主人本机的验收包目录 `EnouiaMV7Smoke`（不入库，旧文件移到其中的 `old-2026-10-11`）。[验收手册](../validation/MV-7-real-smoke.md)的基线更新在 Memory PR #34，未合并。这个构建的执行前检查：`--forget-only` 16/16、`--session-writes-only` 21/21、`--context-only` 19/19；默认 smoke 停在快速搜索，换成先前通过的构建也一样，判断为桌面焦点争用，具体原因没查。

**4. 查明 `--sessions-only` 卡住。** 不是产品缺陷。该测试模式写于 10 月 6 日，会在读取会话 A 期间点会话 B；Runtime `ef5d2cd` 之后，读取期间会话行一律禁用，`session-id-filter` 和 Session selection focus 两条后续用例也依赖这一点，所以点击不会触发。Runtime PR #81 只改 smoke 这一段并更新说明，产品行为不变，修改后连续两次 21/21，`npm test` 98 项 + DOM 58 项通过，未合并。PR #34 的手册已同步更正。

## 待办

1. **待 Morii 决定：** 合并 Runtime #81、Memory #34 和本交接 PR。没有指示就不要合并。
2. **MV-7 出口：真实双 Provider 验收（P02～P06）。** 由主人执行：核实两家 API 的模型名，填写设置文件，在 Set key 弹出的凭据对话框输入密钥，并亲手点 Send once。Codex 可以协助准备、核对和记录，但不能输入或读取密钥，不能代点发送，也不能自行发起真实请求。Runtime main 现在是 `b8254d8`（又合入了 #78～#80）；验收包是 `07584cb` 的构建，需要的话先重建并更新手册基线。结果写进手册的执行记录和 [MV-7 报告](../validation/MV-7.md)；P02～P06 全部满足才算 MV-7 验收通过。
3. **参考壳文案：** `apps/workspace/src/App.tsx` 的删除影响仍写“会失去证据”，应与 Runtime 一致。它在接入面里，修改时需要同一提交重新生成 `runtime-surface.json` 并在 `RUNTIME.md` 记一行。尚未开始。
4. **H2 真实历史质量（H01～H06）：** 等 Morii 决定使用哪些真实资料和模型预算。
5. **已知问题（不阻塞，未定位）：** 默认 smoke 的快速搜索在桌面使用中会失败；Runtime `operation-cancel` DOM 用例单独运行时偶发失败（3 次 1 次）；Memory workspace 有序并发屏障的旧故障。不要无证据加长超时，或反复跑压力测试。

仍待 Morii 决定：自动接受范围、真实资料与模型预算、云服务信任、首个客户端、备份位置与恢复秘密、迁移激活。MV-8 没有启动，需要主人明确说“开始 MV-8”。

## 本机工作树

- **Memory：** 主 checkout 停在已合并的 `codex/history-context-dependencies`，`AGENTS.md` 有用户自己未提交的修改，不要提交、覆盖或还原。`.local/worktrees/` 下有 PR #34（`mv7-real-smoke`）和本交接（`codex-handoff`）的工作树。仓库外另有一个分离 HEAD 的旧工作树，不是本轮创建的，不要动。
- **Runtime：** 主 checkout 有他人未提交的 Activity 相关改动，不要动。`.claude/worktrees/` 下：`memory-pin-56bc5eb`（已合并，可清理）、`runtime-main-07584cb`（安装包构建所在）、`runtime-pre77-372d6d1`（对照构建）、`sessions-detail-stall`（PR #81）。后两个的 `apps/desktop/node_modules` 是指向 `runtime-main-07584cb` 的目录联接；清理这两个工作树前先单独删除联接，否则可能连带删掉目标目录的内容。同目录下的 `*-logs` 是本轮日志。

## 验证与边界

照根 `AGENTS.md` 使用固定工具链和全部 required checks；区分合成证据、实际集成和 Morii 验收。原生 smoke 使用在 Git 工作树外用 `enouia-memory init <已存在目录> --confirm-new-vault` 新建的合成 Vault。真实资料、API 凭据、真实 Provider 请求、自动接受记忆、云端部署和生产迁移都需要另行明确授权。提交保留已有 trailer，Codex 参与时加 Codex trailer；推送前检查公开内容。
