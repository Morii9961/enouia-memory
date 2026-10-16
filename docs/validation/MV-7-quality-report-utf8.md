# MV-7 · 离线质量报告的 UTF-8 输出

日期：2026-10-16。基线 `7e51341`（PR #27）；分支 codex/history-quality-utf8-report。

## 问题与修正

同一份原生内联合成套件，将案例名设为含中文及表情的字符串：直接 evaluate 调用可用，CLI 在 PYTHONIOENCODING=ascii:strict 或 cp936:strict 下却 exit 1。原因是 JSON 虽以 ensure_ascii=False 序列化，最后的 print 仍经过标准输出的环境编码；UnicodeEncodeError 被输入错误处理捕获。旧版本回归出现两个编码子例失败，stdout 为空，分别报告 ASCII 和 GBK 编码失败；这不是输入绑定错误。

现在先完成整个套件的验证、JSON 序列化和 UTF-8 编码，再通过 stdout.buffer 输出完整 bytes，保留末尾换行。与既有评审稿一致，报告接收方应以 UTF-8 解码；不改变系统代码页、终端设置、任何输入文件或 stdout 全局配置。输出编码是稳定的数据格式，不保证当前终端字体及解码显示。

## 合成验证

新增两项回归：受限 ASCII/GBK 输出环境下，合法中文及表情案例名均 exit 0、stderr 为空，完整 UTF-8 JSON 值等于 evaluate API 结果；前例合法而第二例绑定错误时，仍 exit 1、stdout 为空、无 traceback。完整 Python 54 项通过。首次沙箱运行受到 Windows 临时目录 ACL 拒绝，未作为产品故障或通过证据；在允许的本地环境复跑后，先得到原版两个失败子例，再得到修正后完整通过。

默认/扩展校准、八份既有原生合成材料及 Unicode 案例共 11 份套件、22 个案例的完整报告 JSON 值，与基线逐一按规范 digest 比较完全一致；新 CLI 的 UTF-8 输出也与 API 结果一致。输出换行固定为 LF，不沿用 Windows 文本流的 CRLF 转换。

固定离线工具链的全部必需检查 exit 0：fmt、默认并行完整 Rust 387 项、严格 Clippy、Python 54 项、python-jsonschema 4.26.0 的 36 schemas，以及 Runtime surface 检查。公开内容检查覆盖本片五个文件、19 个本地链接、路径/凭据形状和 diff 空白；两份状态快照 hash 不变。日志和单独退出码、兼容结果及汇总保存为忽略的 mv7-utf8-* 文件。

## 范围

只修改离线 CLI 的输出编码，计数、评审状态、未知项、JSON hash 与输入验证规则不变。生产 Rust、依赖、契约和 Runtime 接入面无变化，47 commands / aggregate `42acddadc5686b962d6ae3d8a755b009f9c158fef1f95c65a95ff6f25e43de44` 保持；无新增宿主采用动作。未引入真实资料、Provider 调用、Runtime 激活、MV-8 或状态快照更新。受控编码子进程证明数据输出，不能替代实际终端显示或主人验收。
