# Claude 接续：Enouia Memory · MV-7

更新：2026-10-10。范围：Memory 仓库内的 MV-7 功能实现与合成验证。

本文是提前准备的接续记录，不自行启动 Claude、其他任务、真实调用或发布。主人让 Claude 接手时，应先停止在同一工作树继续写入，避免两个任务同时修改同一功能。接手时以当前 Git 状态和适用的 `AGENTS.md` 为准；不要把本文的基线当成永远最新。

## 主人的授权与工作方式

- 主人已明确开始 MV-7，要求先完成功能，暂缓真实 smoke。当前只考虑 OpenAI 与 Anthropic；订阅所有权不证明 API 账户能力。
- 每完成一个独立功能就提交一次。当前授权是本地提交，没有本任务的 push、PR、合并或发布授权。
- 多个任务共用 Codex 限额。Codex 在每个功能提交前后查看共享窗口；任一窗口剩余约 25% 时停止开启新功能，保留收尾和交接余量。不要自动使用重置额度或购买额度。Claude 使用自身会话的限制与归属规则。
- 不读取真实 API 凭据、私人历史、运行 Vault 或其他任务数据；不进行真实 Provider 请求、Runtime 激活或账户测试。
- 继续 MV-7 不代表开始 MV-8。下一阶段必须由主人明确启动。

## 接手基线

| 项目 | 记录 |
|---|---|
| 仓库 | Enouia Memory，独立 Cargo workspace |
| 分支 | `codex/mv7-native-providers` |
| 最新已验证功能提交 | `fe48ebbf4051e33870fcd48b5ca4d8a775719731` |
| 本次 MV-7 起始基线 | `8197890` |
| 工具链 | Rust `1.98.1`，`x86_64-pc-windows-gnu`；Python 3.12 / python-jsonschema `4.26.0` |
| 最近完整验证 | 2026-10-10，326 项 Rust 测试通过，含 51 项 Provider 检查和 60 项 workspace Core 检查 |
| Runtime 接入面 aggregate | `f1a52c732445fe9ea0c5d5b33eaec3fd5fe7b9a71376ca3215e594f1792cf167` |

制作此交接前，功能改动均已提交，工作树干净。本文及其入口链接是后续文档改动；其提交不改变上述功能基线。接手首先重新检查目录、分支、HEAD、工作树和其他任务正在使用的文件；保留他人的修改，不重置、不覆盖、不清理其他工作树。

最近四个功能分别提交：

| 提交 | 结果 |
|---|---|
| `cead203` | 原生 HTTPS 使用自己的 Ring TLS 配置及固定平台证书验证器，不安装或继承宿主的全局加密配置；只显式引用已有锁定的验证器版本。 |
| `d66f00e` | Anthropic 累计 input/output/cache 计数不能倒退；矛盾更新永久失败，并保留此前有效用量用于失败记录和后续额度计算。 |
| `8d8281a` | `CredentialStore::delete` 增加可信原生设置的凭据移除入口；仅允许两个固定条目，成功或确认不存在可重复执行，其他系统错误明确返回。只做假调用验证，未操作实际凭据。 |
| `fe48ebb` | 原生响应头按 ASCII 大小写无关规则匹配 JSON/SSE 类型；不接受其他格式、通配符、混合类型列表或 Unicode 仿形。 |

## 已有能力与当前阶段

MV-7.2 的 Memory 侧原生调用链已实现：外部目的地编译、精确请求体 inspection、站立策略及私密内容单次批准、凭据读取端口、持久化 admission、流式文本、原子终态、用量和限额、取消、关闭与恢复规则。OpenAI Responses 与 Anthropic Messages 均有合成接受路径。

同一输入已 admission 后不能重发，包括失败、取消、崩溃或结果未知。已知用量不能退款式覆盖原始 reservation；失败流保留最后有效计数，缺失值保持 unknown。原生 HTTP 不自动 retry、fallback、redirect 或推断 proxy。401/403 保留结构化错误，错误体不进入文本/存储。SSE 支持碎片化 UTF-8、LF/CRLF/CR、开头 BOM、严格终态与永久失败锁存。

MV-7.4 的可选抽取已实现来源片段绑定、版本化提示、完整响应校验、原输入绑定、候选审核队列、暂停/恢复、拒绝项抑制、积压及预算限制。候选不能自动成为正式记忆；字面引用包含性不等于语义真实性。时间、分支不确定性、否定/限定上下文和真实用户偏好证据保留。

MV-7.1 的实际账户/模型能力、精确 tokenizer 和真实价格仍未确认。MV-7.3 的双 Provider 真实受控验收按主人要求暂缓。Runtime 产品宿主接入、凭据设置入口、工作线程取消/join 和原生界面验收仍待做；workspace/reference UI 当前仍调用 Mock。因此不能称真实模型已接通、Runtime 已采用或 MV-7 阶段验收已完成。

## 必读入口与代码

1. 根目录 `AGENTS.md`、[阶段计划](../design/IMPLEMENTATION_PLAN.md)、[MV-7 报告](../validation/MV-7.md)。
2. [ADR-MEM-47](../adr/047-explicit-provider-dispatch.md)、[原生接入指南](../integration/PROVIDERS.md)、[Runtime 接入记录](../integration/RUNTIME.md)。
3. `crates/enouia-memory-provider/src/{client,codec,stream,transport,windows,egress,policy,vault,extraction}.rs`。
4. `crates/enouia-memory-provider/tests/providers.rs`：真实本地合成 Vault、假 secrets、假字节 transport；原生 builder/凭据移除的隔离 unit case 位于对应模块。
5. `crates/enouia-memory-context/src/session.rs` 中的原子 invocation output，及 contract 的 invocation/extraction 数据结构和 Schema。

历史 `CLAUDE_START_PROMPT.md`、`docs/design/CLAUDE_HANDOFF.md` 的旧 MV-0 主体仅是历史记录，不能覆盖本轮阶段和仓库边界。

## 下一项可独立接续的 Memory 侧工作

审查原生响应的重复 `Content-Type` 字段。当前 `transport.rs` 通过 `headers().get(CONTENT_TYPE)` 读取一个值；对单个字符串中的混合类型列表已有拒绝，但多个独立同名字段尚无针对性接受检查。[RFC 9110 的 Content-Type 规则](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.3) 将其定义为单值字段，并说明不同解析方式会造成歧义。

这是候选工作项，尚未新增复现或修复，不要把它标成已确认的事故。先使用已锁定 HTTP 库的内存 header 对象/纯 helper 与假 transport 验证实际行为；这项复现无需网络服务器，不调用真实 Provider。若证实应拒绝重复字段，保持 401/403 分类、拒绝体零回调、最后有效用量、admission/no-resend 和现有合法单值大小写行为；复核是否需扩展纯 head 验证接口并记录宿主采用责任。每个独立完成的功能验证后单独提交。

若没有证实新的 MV-7 缺口，不为消耗额度扩充无依据功能，也不转入 MV-8。Runtime 产品接入需主人给该仓库的明确工作范围；本仓库不添加到 Runtime checkout、crates、fixtures 或 target 的依赖。Activity & Usage 属于 Runtime，不在 Memory 中读取或实现。

## 验证和证据

最近 `fe48ebb` 的根目录检查全部通过：

```powershell
$env:CARGO_NET_OFFLINE = 'true'
$env:CARGO_INCREMENTAL = '0'
$env:RUST_TEST_THREADS = '1'
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python tools/schema-check/check_schemas.py
python tools/integration/runtime_surface.py
```

Cargo 使用仓库 pin；Windows GNU linker/toolchain 须在 PATH 中。Python 使用已配置的 3.12 虚拟环境及 `tools/schema-check/requirements.txt` 的固定版本，不临时更换验证器。

独立 Python 结果：36 schemas、53 valid records、117 record mutations、4,913 set records、51 IPC、63 workspace、46 store、8 invocation、19 extraction-job cases。Runtime manifest 与日志中的 aggregate 一致。合成证据不证明真实账户、账单、TLS handshake、Windows credential store 或实际宿主行为。

本地 `.local/mv7-workspace-tests.log`、`.local/mv7-clippy.log` 是忽略的运行输出，可能被下一次检查覆盖，不提交也不当成唯一证据。稳定入口为 MV-7 报告和对应提交的代码/测试。完整串行根目录检查约需 6～10 分钟，开始前为测试、修复、提交和交接预留额度。

旧完整运行曾遇到现有 Core 并发测试的有界 writer/index 等待或有序释放超时。隔离复验和完整重跑通过；未因此改变产品代码或等待规则。单 harness worker 不关闭测试内明确的线程/进程竞争。遇到类似失败先保留输出并聚焦复验，不以局部通过代替新功能的完整验证，也不要未经诊断扩大修复范围。

若触及 Runtime 集成面，在同一功能提交中重新生成 `docs/integration/runtime-surface.json` 并更新 `docs/integration/RUNTIME.md`。文档交接本身不改变 watched surface，可沿用刚验证的实现证据，检查链接/差异及 manifest 即可，无需重复长测试。

## 收尾与归属

提交只包含本任务文件，不带真实记忆、聊天、数据库、附件、凭据或 `.local/` 输出。Codex 实质参与的提交须按 `AGENTS.md` 保留 Codex co-author trailer；Claude 的新增贡献按其当前会话给出的 trailer 记录，不猜造身份，也不移除其他贡献者。

停止时记录实际 HEAD、未提交范围、最后完整验证及未验证项。若新功能尚未完成完整检查，不称其已完成，不与已验证功能混淆；保留可恢复的修改和准确接续指令。没有新的明确授权，不 push、不创建 PR、不部署、不操作真实账户。

可供主人启动 Claude 时复制：

> 在 Enouia Memory 仓库接续 MV-7。先读 `AGENTS.md` 和 `docs/handoff/CLAUDE_MV7_HANDOFF.md`，核对当前分支、HEAD 和工作树，保留其他任务的改动。继续已授权的 Memory 侧功能，先审查交接中的重复 Content-Type 候选；每完成一个功能验证并单独提交。真实 smoke、实际凭据操作、Runtime 产品接入、发布和 MV-8 仍未由本文授权。报告合成与真实证据的区别，并使用你当前会话的提交归属规则。
