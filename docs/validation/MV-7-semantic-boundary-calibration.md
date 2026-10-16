# MV-7 · 条件、引用与主体归属校准

日期：2026-10-16。基线 `cd5b49e`（PR #29），沿用 [ADR-MEM-54](../adr/054-history-quality-assessment-harness.md)。范围是独立合成评审材料，不更改评估规则或模型调用。

## 题集范围

为条件偏好、引用中的指令、访客与本人偏好三种边界分别提供保留限定与错误扩展的预设回答。每对使用同一选定来源和同一精确引文，却具有相反的预设人工判定；引文存在不等于陈述受支持。

使用单独的合成来源文件和显式选择套件，不改既有 initialization-synthetic.json、默认三例或原十例扩展校准。所有 reviewer 都是 synthetic_fixture，任务 hash 为 null；不调用模型，不证明真实语义质量、指令防护或主人验收。

## 验证记录

两项新增检查使完整 Python 总数为 63：逐例核对六个预设状态、支持/反证和关键单元覆盖；逐对核对来源相同、精确引文相同而回答 hash 不同，并检查回答与审核引文确实在选定原文中。三例 supported_in_review、三例 issues_found 只表示固定标签经工具计算的分布，不是模型测量。所有消息时间保持 null，不补造事实生效时间。

显式执行 evaluate.py 与 review.py CLI 均 exit 0。六例评审稿的每个 assessment、选定来源及回答 hash 与计数报告一致，源文件和套件逐字节未改。既有默认三例、原十例扩展、原初始化来源及三个工具实现与基线没有差异。suite_value_sha256 为 `67752d339e8a5aa1440037d5ed8a913d938635b87c2bcfd0acb8be6fe52a2f41`；报告与合成评审稿保存在忽略的 mv7-semantic-boundary-* 文件。

固定离线工具链的完整必需检查均 exit 0：fmt、默认并行 Rust 387 项、严格 Clippy、Python 63 项、独立 JSON Schema 36 项及 Runtime surface。公开内容检查覆盖六个文件、27 个本地链接、路径/凭据形状及 diff 空白，两份状态快照 hash 不变。完整日志、单独退出码、CLI 核对和汇总保存为忽略的 mv7-semantic-boundary-* 文件。

## 集成与限制

生产 Rust、依赖、契约和 Runtime surface 未改；47 commands / aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 保持，无新增宿主采用动作。native_validation=not_rerun、overall_model_quality=null。题集不能证明模型能识别条件、引用和主体，也不证明真实指令防护；真实质量、漏提、全量尺度和主人验收仍待完成。无真实资料/Provider、Runtime 激活、MV-8、部署或状态快照刷新。
