# MV-7 真实双 Provider 受控验收手册

状态：**待执行**。本页给出 MV-7 出口门槛（[ACCEPTANCE](../design/ACCEPTANCE.md) P02～P06）的真实验收步骤。现有功能验收使用合成数据和假 transport；历史凭据形状探测曾向真实 HTTPS 端点发送合成凭据及合成请求，不构成真实账户或模型接受证据（见 [MV-7 报告](MV-7.md)）。本手册尚未执行。

代码基线：Memory `a9d61c5`（main，含 ADR-MEM-48），Runtime `36c6542`（main，pin `8186aed`，Runtime ADR-032）。

## 原则

- 只用**合成 Vault 和合成记忆**。真实请求会把记忆文本发给 Provider，不要打开个人 Vault。
- 每一步发送都由主人在 Runtime 界面里亲自点“Send once”。Claude 不代按发送，不读取、不输入、不复述密钥。
- 每家 Provider 只发少量请求，每个 Session 用 `maxReservedTokensPerSession` 封顶。费用由主人的账户承担。
- 订阅不代表 API 能力。模型名、上下文窗口和输出上限都要在 Provider 控制台或官方文档上，按主人的账户核实后再填写。

## 准备（主人）

1. 用 Runtime main 构建的版本（安装包或 `enouia-desktop.exe`）。
2. 在 `%APPDATA%\com.enouia.runtime\memory-providers.json` 写入设置，格式见 Runtime 的 `docs/integration/memory-providers.example.json`。两家都要写，`model` 填核实过的精确模型名，`verifiedAt` 填核实时间（`YYYY-MM-DDTHH:MM:SS.000Z`）。文件里不能出现密钥字段，有未知字段时整个文件会被拒绝。
3. 新建一个合成 Vault 文件夹。它要位于本地 NTFS 固定磁盘上，不在 Git 工作树、同步文件夹或系统目录里。在 Runtime 里用“create new vault”创建。
4. 在 Memory 页面用“Remember”记下一两条合成事实（例如“合成项目代号是 Quiet Harbor”），然后在审核里接受。

## 步骤与证据

| # | 对应 | 操作 | 期望结果 | 记录 |
|---|---|---|---|---|
| 1 | P03 | 打开 Sessions，新建会话，在右侧“External provider”面板对 OpenAI 点 “Set key…”，在 Windows 凭据对话框的密码栏粘贴 key | 提示 “Key saved”；页面、日志和设置文件里都没有 key | 截图（不含 key） |
| 2 | P03 | 勾选或不勾选 extraction，点 “Review a grant…” 并确认 | 面板显示授权已生效 | 授权 policyId |
| 3 | P02 | 输入一个会用到合成记忆的问题，点 “Prepare the question for openai…” | 显示确切请求体，其中含批准过的合成记忆，不含 key | 请求体截图、wireHash |
| 4 | P02 | 点 “Send once” | operation 成功；Session 中出现回复；调用记录为 completed，并有上报的 token 数 | dispatchId、状态、token 数 |
| 5 | P02 | 对 Anthropic 重复 1～4 | 同上；当前模型默认会思考，回复中不出现推理内容 | 同上 |
| 6 | P04 | 准备一次请求后，在 operation 运行中点取消，或在设置里把输出上限设得很小再发 | 结果为 cancelled、failed 或 length，不会显示成完整回复；不会自动重试或换模型 | 调用状态 |
| 7 | P06 | 发送进行中锁定 Vault，再解锁 | 调用记录显示 “outcome unknown”；“Close locally…” 可以在本地关闭；旧输入不会重发 | 状态前后对比 |
| 8 | P06 | 撤销授权后再准备一次 | 准备被拒绝，没有任何请求发出 | 错误规则 |
| 9 | P05（可选） | 授权时勾选 extraction，在 Memory 页面打开某条记忆的来源，点 “Suggest memories from this part…”，查看请求、发送、预览、加入审核 | 建议附带原文引用；只进入审核队列，不会自动变成记忆 | 候选数、预览截图 |
| 10 | P03 | 在面板点 “Remove key” | 提示已删除；之后再发送会在本地被拒绝（未认证），没有网络请求 | 错误规则 |

## 收尾

- 删除合成 Vault，或保留作证据但不提交。删除 `memory-providers.json` 后，Runtime 会回到 Mock-only。
- 结果写进本页的“执行记录”，同时更新 [MV-7 报告](MV-7.md) 的阶段结论。只有 P02～P06 都满足，才能称 MV-7 验收通过。
- 截图里不能出现 key、个人数据或本机私有路径。

## 执行记录

尚未执行。
