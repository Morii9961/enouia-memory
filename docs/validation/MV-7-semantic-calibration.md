# MV-7 · 语义质量题集扩展

日期：2026-10-15；基线 `8ed2297`（PR #24），沿用 [ADR-MEM-54](../adr/054-history-quality-assessment-harness.md)。本片补充离线评审校准材料，不改变 evaluator 的判定规则或生产代码。

## 十个新增合成题

[题集](../../tests/fixtures/history/semantic-calibration-cases.json) 只引用既有 initialization-synthetic.json 中明确选定的消息。回答和判定都是预设合成材料，reviewer 为 synthetic_fixture，原生任务 hash 为 null；工具不生成语义判定、不调用模型、不把材料当成主人验收。

| 范围 | 预设回答与检查 |
| --- | --- |
| 偏好过去与现在，两例 | 正例保留过去偏好长篇、现在希望先给简短结论及生效日期未说明；反例反转偏好。消息时间不补作事实生效日期。 |
| 编辑分支与主分支，两例 | 正例分别保留地图与取消地图，以及未采纳导航；反例把主分支取消扩展到所有分支。精确引文不消除分支范围错误。 |
| 助手建议与用户决定，两例 | 正例保留用户地图计划、助手导航建议及尚未接受；反例把助手建议升级为已作出的用户决定。 |
| 未知日期，两例 | 正例保留日期未知；补造具体日期的反例计 unsupported，不能因没有日期便伪称存在相反日期或计 contradicted。 |
| 忠实但遗漏，一例 | 回答仅保留未采纳导航，陈述 supported，但预列的取消地图单元 missing。支持比例 1、覆盖 1/2、all_required_units_reviewed=true，状态仍为 issues_found。 |
| 项目现状未定，一例 | 两个历史分支的存在可支持，断言项目当前状态仍 uncertain；一项 supported、一项 uncertain，当前状态单元 unknown。已判定项支持比例 1，状态仍为 review_required，不能把 uncertain 藏入比例或未知当作漏提。 |

这组题的四个 supported_in_review、五个 issues_found 和一个 review_required 是预设判定经工具计算后的状态分布，不是模型通过率。原有默认三例、来源文件和 evaluator 代码与基线比较保持不变。来源与回答的 JSON 值 hash 分别绑定，所有审核引文须出现在选定原文中；另行检查了回答中的 20 处引文也确实位于选定来源。报告 native_validation 保持 not_rerun，整体模型质量为 null。

## 验证

三项新增 Python 回归覆盖十个确切 case 的状态、支持/反证/无支持/未定计数和关键单元覆盖，以及满支持比例下的遗漏和未知。全部 42 项 Python 检查通过；显式 CLI 生成十例报告成功，suite_value_sha256 为 `1f5c53d4bc4c5338d1621aaaafbc15af031e2def43f462fe36955b7bbd668fbc`。无新依赖或网络访问，报告留在忽略的本地 mv7-semantic-calibration-report.json。

Rust 1.98.1 / GNU Windows / offline：fmt、默认并行完整 Rust 387 项（Provider 78、history 13、Workspace 66）、严格 Clippy 全部 exit 0；Python 3.12.10 / python-jsonschema 4.26.0 独立校验 36 schemas 与 Runtime checker 同样 exit 0。Clippy 保留既有增量缓存硬链接回退复制的环境提示，检查正常完成。五个公开文件、13 个本地文档链接、路径/credential 形状、git diff --check 和两份状态快照 SHA-256 检查通过。

完整日志和退出码分别为忽略的本地 mv7-semantic-calibration-rust.log / rust-exit.json、python.log / python-exit.json、clippy.log / clippy-exit.json、python-checks.json；汇总为 mv7-semantic-calibration-results.json，文件均使用同一 mv7-semantic-calibration- 前缀。不提交临时材料或报告原件。

## 集成与边界

Runtime surface 仍为 47 commands，aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 已登记；没有新 Runtime 入口、采用动作或生产限额变更。新增题集不证明真实模型语义忠实性、漏提率、完整历史规模/费用或主人接受；预设标签可以供后续显式审核使用，不能被当成实际模型测量。旧屏障故障仍未定位；真实资料/Provider、Runtime 激活、MV-8、部署和状态快照均未动。
