# 历史质量评估（离线人工判定）

这个工具统计已经填写的审核判断，不自动理解或评分中文陈述。先用合成校准集运行：

```powershell
python -m unittest discover -s tools/history-quality -p 'test_*.py'
python tools/history-quality/evaluate.py
```

默认只读仓库内的 [合成套件](../../tests/fixtures/history/quality-cases.json) 和它明确选定的来源文件。不会访问网络、凭据或查找私人历史。报告写到标准输出；退出码 0 只表示生成了报告，不能当作质量通过。

## 原生合成组合检查

现有 Rust 假 Provider 用例可显式导出 OpenAI/Anthropic × 无/有所选谱系 × 旧/完整输入 builder 的八份合成套件及清单，放在忽略的本地目录。没有开启该测试环境变量时不生成这些文件，生产 builder 仍只返回内存材料。

```powershell
$env:CARGO_NET_OFFLINE = 'true'
$env:ENOU_TEST_QUALITY_MATRIX = '1'
cargo test -p enouia-memory-provider --test providers --locked quality_bundle_optionally_keeps_exact_saved_analysis_input_without_writes
Remove-Item Env:ENOU_TEST_QUALITY_MATRIX
python tools/history-quality/check_native_matrix.py --root .local/mv7-quality-native-matrix
```

检查器只读取明确给出的合成目录和固定文件名；验证八种组合齐全、Rust 声明的套件哈希与 Python 一致、旧/完整材料只相差输入原文、实际 Provider 绑定/谱系模式及待审/未知状态保留。配对采用规范 JSON 哈希比较，不把 1 与 true 等不同 JSON 类型混同。开始生成时先置空清单，失败运行不能留下上一轮完整清单。检查清单是测试材料，哈希声明不认证原生来源；输出仍为 native_validation=not_rerun、review_required、整体模型质量 null。它不读取 Vault、不调用模型、不判断实际语义质量，详见 [验证报告](../../docs/validation/MV-7-native-quality-matrix.md)。

## 判定步骤

合成组合检查对 `selected_lineage` 检查行数组、所选来源索引的完整唯一覆盖、父索引的范围/唯一性，以及非负整数或 null 的未选父数量；不接受布尔值替代整数、空标记或额外行字段。未知父数量仍保留 null。这只确认原生格式的结构，不验证边的事实、不补读父来源或认证父链。

输入读取统一限制 JSON 容器嵌套深度为 128；字符串中的括号、引号及转义不参与深度计数。NaN、Infinity、-Infinity 与浮点转换溢出一律拒绝，重复键规则保留。合法 UTF-8/16/32 字节输入继续兼容。旧式来源文件须是会话对象数组，选定 mapping/message/content/parts 结构须可读取；合法混合文本和附件元数据原样保留，不下载或解释附件。非法结构只返回普通错误，不输出部分指标或 traceback。

1. 固定选定来源消息、原生任务及保存的模型响应。来源选择限 16 条，套件限 32 个 case；来源 JSON 限 2 MiB，套件限 512 KiB。`source_file` 必须在套件目录内，不能跨目录或借符号链接越界。
2. 用 `digest` 算法（UTF-8、非 ASCII 不转义、键排序、无多余空白的 JSON）登记 `scope_value_sha256` 和 `response_value_sha256`。作用域包含每条选定原始节点、会话和消息 ID；这两个值不是原生 task/wire hash。改写来源或回答会使旧判定拒绝。
3. 逐项审核 changes 的 statement、explanation，以及 summary 的 statement。填写 supported、contradicted、unsupported 或 uncertain，并给出理由及精确来源引用。引文存在不等于陈述被支持，否定、条件、时间、分支、角色和未采纳建议都需要人工判断。遗漏的审核项保持 unreviewed。
4. 预先列关键语义单元，逐项标 observed、missing 或 unknown。observed 必须关联已经判 supported 的响应路径；missing/unknown 不得伪称存在路径。尚未判定的单元保持 unknown，不当成“不重要”。
5. 查看每个 case 的状态和计数。存在反证、不受支持陈述或漏提时为 issues_found；仍有 uncertain/unreviewed/unknown 时为 review_required；全部已支持且关键单元均出现才为 supported_in_review。

`supported_fraction_of_adjudicated_items` 的分母为 supported + contradicted + unsupported；没有判定时为 null。uncertain/unreviewed 另列，不能藏在百分比里。覆盖分母是事先定义的全部关键单元，missing/unknown 均保留；`all_required_units_reviewed` 区分是否判完。报告的整体模型质量固定为 null。

## 证据与激活

输入集合必须使用 JSON 数组：cases、changes/summary、item_judgments/unit_judgments、评审 evidence 和每个单元的 paths。case、回答项、判断和证据项为对象，case_id 为非空白字符串，paths 中每项为字符串。空对象、空字符串、布尔值或对象键不能代替数组/身份；工具不自动转换。任一 case 格式错误时整套退出 1，不输出前面合法 case 的部分指标。这些结构门槛不改变合法套件的评分或未知状态。

可选的 history_quality::build_with_analysis_input 还会返回 native_task_input_text，保留模型实际看到的完整分析输入及原始 UTF-8 hash，包括版本化指令、原建议和谱系元数据。旧 build 输出不变。额外输入仍受 64 KiB 任务预算和 256 KiB 套件预算限制，保存/导出由宿主另行处理，只有私有套件包含原文。

离线工具对可选原文检查精确 hash、JSON 形状、重复键及所选来源/片段/文本/角色/上下文的一致性。报告的 analysis_input_inspection 为 included_hash_and_scope_matched，旧套件为 not_included；报告不复写输入原文。这些是结构检查，不认证提交者、不查 Vault、不证明父链真实性，native_validation 仍为 not_rerun，reviewer、人工判断和整体模型质量规则不变。

原生宿主可调用 history_quality::build，从明确选择的已完成语义任务返回一份内存中的套件。它使用 native_selected_spans_v1 内联作用域，保留选定原文、Source 修订/片段 hash、角色、时间/分支上下文和未引用覆盖；不伪造原始会话/消息 ID，不自行写文件。宿主对任何保存/导出单独处理。单 case 上限 256 KiB，关键单元由主人预先提供，最多 32 个。

生成的 reviewer 固定为 pending，item_judgments/unit_judgments 都为空；空模型回答也保留全部选定来源及未审的重要单元。pending 不允许附带判断；填写后须将 reviewer 改为 owner 或 synthetic_fixture。scope_value_sha256/response_value_sha256 用紧凑 UTF-8 JSON 值 hash，原生任务与响应原文 hash 另列，不混作同一种绑定。CLI 检查内联片段的范围与 hash，但不会访问 Vault 或认证 native_builder_validation 声明；native_validation 仍为 not_rerun。

有人工判定的合成 case 的 reviewer 必须是 synthetic_fixture；待审为 pending。未来 owner_selected 样本须登记 `native_task_input_sha256`，待审保留 pending，实际填写判断后标 owner；这些是提交者声明的元数据，本工具不会认证主人、查验 Vault 或重跑原生响应验证。`native_validation: not_rerun` 保持明确。格式允许这种后续样本，不表示真实资料、模型或预算已获授权。

真实样本先由 Morii 选择并另行启用，原件、响应、评审套件和报告放在私有位置，不提交到仓库。最终主人验收、真实模型的忠实性/漏提、运行费用和恢复门槛仍独立记录；不能拿合成校准结果代替。工具不批准或写入记忆。
