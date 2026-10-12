# 历史质量评估（离线人工判定）

这个工具统计已经填写的审核判断，不自动理解或评分中文陈述。先用合成校准集运行：

```powershell
python -m unittest discover -s tools/history-quality -p test_evaluate.py
python tools/history-quality/evaluate.py
```

默认只读仓库内的 [合成套件](../../tests/fixtures/history/quality-cases.json) 和它明确选定的来源文件。不会访问网络、凭据或查找私人历史。报告写到标准输出；退出码 0 只表示生成了报告，不能当作质量通过。

## 判定步骤

1. 固定选定来源消息、原生任务及保存的模型响应。来源选择限 16 条，套件限 32 个 case；来源 JSON 限 2 MiB，套件限 512 KiB。`source_file` 必须在套件目录内，不能跨目录或借符号链接越界。
2. 用 `digest` 算法（UTF-8、非 ASCII 不转义、键排序、无多余空白的 JSON）登记 `scope_value_sha256` 和 `response_value_sha256`。作用域包含每条选定原始节点、会话和消息 ID；这两个值不是原生 task/wire hash。改写来源或回答会使旧判定拒绝。
3. 逐项审核 changes 的 statement、explanation，以及 summary 的 statement。填写 supported、contradicted、unsupported 或 uncertain，并给出理由及精确来源引用。引文存在不等于陈述被支持，否定、条件、时间、分支、角色和未采纳建议都需要人工判断。遗漏的审核项保持 unreviewed。
4. 预先列关键语义单元，逐项标 observed、missing 或 unknown。observed 必须关联已经判 supported 的响应路径；missing/unknown 不得伪称存在路径。尚未判定的单元保持 unknown，不当成“不重要”。
5. 查看每个 case 的状态和计数。存在反证、不受支持陈述或漏提时为 issues_found；仍有 uncertain/unreviewed/unknown 时为 review_required；全部已支持且关键单元均出现才为 supported_in_review。

`supported_fraction_of_adjudicated_items` 的分母为 supported + contradicted + unsupported；没有判定时为 null。uncertain/unreviewed 另列，不能藏在百分比里。覆盖分母是事先定义的全部关键单元，missing/unknown 均保留；`all_required_units_reviewed` 区分是否判完。报告的整体模型质量固定为 null。

## 证据与激活

合成 case 的 reviewer 必须是 synthetic_fixture。未来 owner_selected 样本须显式登记 owner 及 `native_task_input_sha256`；这些是提交者声明的元数据，本工具不会认证主人、查验 Vault 或重跑原生响应验证。`native_validation: not_rerun` 保持明确。格式允许这种后续样本，不表示真实资料、模型或预算已获授权。

真实样本先由 Morii 选择并另行启用，原件、响应、评审套件和报告放在私有位置，不提交到仓库。最终主人验收、真实模型的忠实性/漏提、运行费用和恢复门槛仍独立记录；不能拿合成校准结果代替。工具不批准或写入记忆。
