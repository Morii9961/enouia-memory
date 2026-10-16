# MV-7 · 只读人工质量评审稿

日期：2026-10-16；基线 `c4df13e`（PR #26）。范围：[ADR-MEM-62](../adr/062-offline-quality-review-worksheet.md)，继续当前 MV-7 离线人工评审准备。

## 行为

新增 review.py，必须显式提供 --suite，只向标准输出返回有界 UTF-8 Markdown。选定原文、回答、逐项路径、关键单元和已有判断按案例对照；固定中文状态/计数来自现有 evaluator，不生成新判断。pending、空判断、unknown、不确定及遗漏全部保留，overall_model_quality 仍为 null、native_validation 仍为 not_rerun。评审者和原生绑定都作为套件声明展示，不认证身份或来源。

可选完整分析输入展示解析后的 JSON 结构，并另列原始 UTF-8 hash；不声称展示的排版是原始字节。所有原文、身份、陈述和理由位于 JSON 代码块中，只有静态标题、固定状态文字和计算出的数字进入文档结构。评审稿包含原文，属于私有材料；工具不保存文件、不改源套件、不查私人目录、不读取 Vault、不外发或批准记忆。没有 Runtime 产品 UI 或 wire/storage 变化。

完整套件先由 evaluator 验证，展示前复核读取的套件值和每例来源值与已验证 hash 一致；不一致或超限拒绝整份结果。默认 512 KiB，最大显式上限 2 MiB，以 UTF-8 字节计算；错误时 stdout 为空，避免半份评审材料被误用。该一致性检查不是文件系统事务或来源认证。

## 合成验证

六项新增检查覆盖：默认三例的原文/路径/已有判断/未知状态保留；原生完整输入结构与 hash；含换行、代码围栏、HTML 和表格样式的非可信文字不脱离 JSON 数据区；精确 UTF-8 字节边界及接近完整文档时 CLI 仍零输出；读取期间套件变更；展示期间来源变更。全部 52 项 Python 检查通过。

纯合成 CLI 预览为忽略目录中的 mv7-review-worksheet-synthetic.md，三例共 9647 字节；实际文件 bytes 与 render 返回值的 UTF-8 编码完全相等。另用既有 Rust 假 Anthropic/有谱系/完整输入材料验证实际原生字段，11105 字节评审稿保留原响应、完整来源和输入 JSON 值及 hash，pending 与两个 unknown 单元不变，见本地 mv7-review-worksheet-native.json。此记录不构成真实材料或主人验收。

完整必需检查均 exit 0：cargo fmt、默认并行 Rust 387 项、严格 Clippy、Python 52 项、独立 JSON Schema 36 项及 Runtime surface 检查。公开内容检查覆盖本片七个文件、75 个本地文档链接、路径/凭据形状和 diff 空白；两份状态快照 hash 未变。日志、退出码和汇总为忽略的本地 mv7-review-worksheet-* 文件；原始套件、JSON 计数报告和生产 builder 行为不变。

## 原生合成材料的部分评审衔接

检查既有 Rust 合成路径后确认，semantic_options 已经过归档导入、批次 materialize、Fake Provider 抽取和语义响应，再由质量 builder 生成待审套件；不需新增另一条流程。取上述假 Anthropic/谱系/完整输入套件的独立练习副本，只填写预设 synthetic_fixture 的一个 supported 陈述及一个 observed 单元，分别调用 evaluate.py 和 review.py CLI。

两条命令 exit 0，评审稿中的 assessment 与计数报告完全一致：支持 1、未审 2，关键单元出现 1、未知 1；支持比例为 1 而状态仍为 review_required，整体模型质量为 null、native_validation 为 not_rerun。11756 字节评审稿保留未填判断为 null，选定来源、回答、原生绑定、完整输入及各自 hash 均未改；原始 pending 文件逐字节不变。保存结果见忽略的 mv7-review-worksheet-roundtrip.json。README 给出同一合成练习步骤；没有自动判定或 Markdown 回写。此文档补充沿用同版实现的完整必需检查证据，未重复 Rust 全量。

## 集成与未完成门槛

Runtime 接入面仍为 47 commands，aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 已登记，无新宿主采用动作。真实人工评审、模型忠实性/漏提、全量成本/规模、Runtime 产品采用、独立恢复和主人验收仍待完成。Windows 实际链接验证及旧并发屏障问题也没有由本稿证明解决。没有真实资料/Provider、Runtime 激活、MV-8、部署或状态快照刷新。
