# MV-7 · 离线质量输入的路径边界

日期：2026-10-15；基线 `c301ba8`（PR #25）。合成矩阵原来对八份套件解析路径并检查根目录范围，却直接读取 root/manifest.json，遗漏清单本身的同等检查。

现在先解析 manifest.json，确认目标在已选根目录中，再读取解析后的路径，原有 16 KiB 清单预算不变。目录外的解析目标在任何 JSON 读取前被拒绝；目录内目标可正常使用。此处只检查解析时的位置，不认证清单声明或原生来源。

两项可控路径解析回归在旧版产生一项失败、一项错误，记录为本地 mv7-manifest-boundary-before.log：目录外目标未被拒绝；目录内目标未被用作实际读取文件。另核对固定 Python 3.12 实现，循环链接由 Path.resolve 抛出 RuntimeError；清单/套件的两个模拟子例原来都以该异常退出，见 mv7-manifest-boundary-loop-before.log。现在在路径解析处将其转换为普通 ValueError，沿用 CLI 输入错误处理，不暴露 traceback。

修正后完整 45 项 Python 检查通过。read_json 调用记录证明外部目标拒绝前没有读取、内部目标使用解析后的文件，解析循环也在对应文件读取前失败。既有同版本 Rust 生成的八份合成材料通过兼容检查，native_validation 仍为 not_rerun，整体模型质量 null。

实机符号链接探针仅用了隔离合成临时文件，但当前 Windows 返回 Win32 1314，未能创建链接；没有修改系统权限或设置。因此本片的链接目标证据来自 Path.resolve 模拟，不能称已通过实际 Windows 符号链接验收。探针脚本和结果留在忽略的本地区域，无私人资料读取。

## 2026-10-16 普通评估入口补充

在 `8c87b83` 上继续核对发现，普通 evaluate.py 的旧式 source_file 和套件目录解析仍会直接抛出循环链接 RuntimeError。现在在这两处解析完成且范围通过前不读取来源；循环解析转为普通 ValueError，由原 CLI 输入错误处理返回。没有增加自动重试、改选来源或更改评分规则。

新增一项含两个解析位置的可控回归，旧版两子例均报 RuntimeError，见本地 mv7-source-loop-before.log；修正后完整 Python 46 项及八份既有 Rust 合成材料兼容检查通过。对默认三例、扩展十例和八份原生合成套件，与 `8c87b83` 的 evaluator 逐份比较，十份合法报告、21 个案例的完整 JSON 值均不变，见 mv7-source-loop-compatibility.json。实机链接验证限制与上段相同。

首次补充全量日志停在 Provider 测试中途，进程已结束且没有最终退出码，因此不作为通过证据；保留 mv7-source-loop-rust.log。2026-10-16 独立补跑默认并行完整 Rust 387 项通过，fmt、严格 Clippy 均 exit 0，使用 mv7-source-loop-resumed-* 日志/退出码。完整 Python 46 项、独立 schema 36 项及 surface 的已保存同版结果均为 exit 0；七个 PR 公开文件、14 个本地链接、路径/credential 形状、git diff --check 和快照 SHA-256 检查通过。汇总为 mv7-source-loop-results.json，以上文件均在忽略的本地区域。

## 2026-10-15 清单修正的验证记录

Rust 1.98.1 / GNU Windows / offline：fmt、默认并行完整 Rust 387 项和严格 Clippy 均 exit 0。Python 3.12.10 / python-jsonschema 4.26.0 独立交叉检查 36 schemas 与 Runtime checker 均 exit 0；五个公开文件、13 个本地文档链接、路径/credential 形状、两份状态快照 SHA-256 及 git diff --check 通过。日志、单独退出码和汇总均为忽略的本地 mv7-manifest-boundary-* 文件；结果汇总为 results.json，实链探针记录为 symlink-probe.json，均使用同一前缀。

生产 Rust、契约、依赖与 Runtime surface 未变：47 commands，已登记 aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44`。真实模型质量/规模、主人验收和旧屏障故障仍待验证；状态快照未刷新，真实资料/Provider、Runtime 激活、MV-8 和部署未启动。
