# ChatGPT 连接器使用说明（每日记忆、实体与主题）

连接器部署好之后（见 [REMOTE_MCP_CLOUDFLARE_TUNNEL.md](REMOTE_MCP_CLOUDFLARE_TUNNEL.md)），
ChatGPT 可以读你的记忆，也可以提交新记忆。本页说明怎样让它每天工作。示例里的名字、内容
都是虚构的。

## 它能用哪些工具

- 读：`memory_search`、`memory_read`、`memory_source`、`memory_related`、`memory_recall`。
  能读到什么，由你在服务器上给它的读取授权决定（可设为“自动跟随”新窗口，见
  [CONNECTOR_READ_GRANTS.md](CONNECTOR_READ_GRANTS.md)）。
- 写：
  - `memory_submit_conversation`：提交一段对话的原文，以及从你原话里提炼出的记忆。
  - `memory_submit_theme`：从已有的每日记忆中，归纳出一个反复出现的主题。

两个写工具都支持 `preflight: true`，只检查、不写入。同一个 `request_id` 重复提交只会
回放，不会写两次。提交对话的回执里有 `memories`，列出这次形成的记忆的 `memory_id` 和标题
（[ADR183](../adr/183-connector-receipt-memories.md)），归纳主题时可以直接引用。

## 调用时只需要填最少的字段

[ADR178](../adr/178-chatgpt-shaped-tool-inputs.md) 之后，服务端会补齐其余字段：

- 读：`memory_search` 只需要 `query`（可以加 `form`、`attention`、`limit`，以及 `days`：只看最近
  N 天记下的记忆，例如每周归纳主题时用 7，见 [ADR182](../adr/182-chatgpt-recent-search.md)）；`memory_recall`
  只需要 `text`；`memory_read` / `memory_related` 只需要 `memory_id`；`memory_source` 只需要
  `source_id`；`session_start` 可以不带参数。
- 写：`request_id` 可以是任意稳定的字符串（最多 128 字），同一个字符串重复提交只会回放。
  每个 turn 只需要 `message_id`、`speaker`（user 或 assistant）和 `text`；分支、序号、父消息、
  语言会自动补上。每条记忆的 `event_time` / `valid_time` 不填时按“未知”处理。

完整格式仍然接受。

## 纠正和遗忘

你在对话里说“那条记错了”或“那条忘掉吧”时，ChatGPT 可以替你提交请求
（[ADR184](../adr/184-chatgpt-correction-forget-requests.md)）：

1. 先用 `memory_submit_conversation` 提交包含这句话的对话。回执的 `turns` 里有每条消息的
   `source`。
2. 再调用 `memory_request_correction`（附 `correction`）或 `memory_request_forget`（附 `reason`），
   带上那条记忆的 `memory_id`、那条消息的 `source_id`，以及你原话里的一句（逐字）。

请求会进入 Runtime 的待确认队列，你确认之前，记忆不会有任何改动。

## 每日提炼的规则

1. 每条记忆必须**逐字引用**你在同一次提交里说过的一句话（`quote`），只能引用你自己的话，
   不能引用 ChatGPT 的回答。
2. `title` 写成一句简短的标题；`kind` 只能用服务配置里允许的类型。
3. 记忆里提到的人、地点、项目，可以写进 `entities`（每条最多 8 个），格式为名字、类型、
   是否真实存在、在这句话里的角色。每个名字都会建一张独立的卡片：
   - 同名同类型的卡片会自动显示为“同名，未确认”的关联；
   - 是不是同一个人，最后由你在 Runtime 里确认。
4. 一句话里没有值得记住的内容时，就不要提交记忆，只提交对话原文即可。

## 主题

- 当它发现几条每日记忆（2 到 32 条）讲的是同一件反复出现的事时，可以提交一个主题：
  标题、用它自己的话写的概括，以及这些记忆的 `memory_id`（从 `memory_search` 的结果里拿）。
- 主题会被标为 **ChatGPT 的推断**，不会被当成你说过的话；它链接到所引用的那些记忆。
- 主题只能涵盖连接器自己提交的每日记忆，初始化导入的历史记忆不在其中。
- 你可以在 Runtime 里重命名、置顶或解散主题。

## 可以放进 ChatGPT 自定义指令的一段话

> 每天对话结束时，调用 `memory_submit_conversation`：附上今天对话的原文（request_id 用
> “日期-主题”这样的字符串），以及从我的原话里提炼出的记忆。每条记忆都要逐字引用我说的一句话，并在 `entities` 里写出提到的人和地点。
> 先用 `preflight: true` 检查，没问题再正式提交。
> 每周用 `memory_search`（`days: 7`）看一次最近的记忆，如果有反复出现的主题，就用 `memory_submit_theme` 提交一个，概括要
> 用你自己的话写，并说明这是你的推断。

## 出错时

- 返回 `invalid_submission`：同时会附带 `problems`，逐条说明哪里不对，例如引文没有逐字出现在
  那条 user 消息里、`kind` 不被允许、字段超长（[ADR181](../adr/181-connector-refusal-reasons.md)）。
  按提示修改后，用同一个 `request_id` 重新提交。
- 返回结果不确定（例如网络中断）：服务端会停止，不会自动重试；先在服务器上恢复，再用同一个
  `request_id` 查询或回放。
