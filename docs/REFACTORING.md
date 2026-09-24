# 虚拟账户架构重构

用户于 2026-09-22 授权整体工程重构：删除 API Key 客户端模式，只保留虚拟账户；整理迭代遗留代码；明确权限隔离；为未来其他提供方的虚拟账户建立可扩展边界。用户进一步确认：供应账户与消费账户完全分离，每个虚拟账户只使用一个提供商；消费账户的提供商不可切换，执行路由只能选择该提供商的供应账户。当前只实现 ChatGPT，不实现或伪造 Grok 登录、协议和执行能力。Codex 固定基线不变。

## 架构约束

- OAuth 仅授予当前消费账户的使用权限；实际 scope 随授权保存，刷新只能保持或缩小范围。账户管理员、套餐、价格、路由和供应凭据不属于客户端权限。
- 客户端主体只能是通过登录获得令牌的虚拟账户。管理员会话与客户端令牌分开；供应账户不是客户端身份。
- 提供方、虚拟账户、供应绑定、套餐、模型是独立概念。跨提供方绑定与令牌使用必须拒绝。未来提供方必须注册自己的协议适配器。
- HTTP、WebSocket、后台任务和其他执行入口使用同一个执行授权服务。订阅、模型和额度校验不能依赖日志采集是否成功解析。
- 模型权限使用显式状态，区分全部已启用模型、指定模型和无模型。到期免费访问使用独立权益，不能把缺少付费权益解释成全部模型。
- 列表和执行读取同一份有效权益与模型目录。协议转换由 ChatGPT 适配层负责；供应模型元数据不能成为授权来源。
- 账户资源始终按虚拟账户限定查询、修改和删除。客户端数据不从供应账户回退，未实现能力明确返回错误。
- 用量明确区分虚拟账户归属与供应执行账户；只有 5 小时和 7 天费用窗口。历史价格快照和历史费用不重算。
- 数据库采用追加迁移；旧迁移不改写。删除旧密钥凭据和运行代码，保留历史用量并明确标注来源。
- 管理网页同步反映账户、提供方、有效权益和历史记录。客户端自己的偏好、会话与审批保持只读管理。

## 实施及验收清单

- [x] 保留工作区基线并运行重构前测试。
- [x] 建立提供方与有效权限的领域模型和统一应用服务。
- [x] 新增数据迁移，移除 API Key 凭据，规范账户与用量归属。
- [x] 移除旧认证、管理界面、静态资源和无效路由；保留必要的客户端协议路径。
- [x] 接通统一执行授权，覆盖正常和异常 HTTP/WS 帧、到期和动态权限变更。
- [x] 统一模型目录与管理端，清理供应账户回退和重复分支。
- [x] 整理模块、命名、文档、格式和持续集成检查。
- [x] 验证迁移保留历史，验证多账户和跨提供方隔离，执行工作区测试及实际 Desktop 协议检查。

验收分别记录服务端测试、管理端检查、启动器及实际 Desktop 运行证据；不得把模拟上游或解析测试描述成实际推理成功。

## 核心实现

`codex2api-core` 定义 provider、模型范围和策略错误；`codex2api-service::ExecutionService` 统一检查有效订阅、启用模型、同提供商授权及两段费用额度。`codex2api-api` 只组合适配器，ChatGPT 的路由、OAuth、身份转换和客户端协议位于 `providers/chatgpt`。目前注册表只包含 ChatGPT；管理入口拒绝创建未安装适配器的提供商，测试中的第二提供商仅用于数据库隔离验证。

供应账户、供应凭据与消费账户分别存储。消费账户只拥有一个固定提供商，套餐、模型、设备授权和执行路由必须属于该提供商。`0029` 删除客户端密钥表并保留带历史来源的用量；`0030` 按提供商区分模型和价格；`0031` 拆分供应表与执行路由；`0032` 保存实际 OAuth scopes；`0033` 让清空路由和删除供应账户保留并递增路由版本，防止旧表单覆盖后来配置。迁移不重算费用、不重新生成身份、不清除消费账户历史。

客户端令牌仅授权本消费账户。管理员 cookie 与 OAuth 分开；OIDC 身份令牌仅在请求 `openid` 时签发，邮箱、姓名和刷新令牌分别受 `email`、`profile`、`offline_access` 限制。刷新只能保持或缩小 scopes。用户账号头、路径、查询及持久资源校验都不能依赖供应身份。消费登录改为 Next 页面加 JSON bootstrap/submit，PKCE、state、回调来源、CSRF 和一次性授权码仍由服务端验证。

Responses WebSocket 的账户检查与用量账本为必需参数。畸形 metadata、未授权预热帧、Realtime 会话/响应模型及已知 transcription 模型都在转发前检查。Realtime 的 path/query call_id 必须命中本账户及当前供应路由；冲突和重复参数拒绝。已有请求的上游完成用量先落库，再决定是否向已撤销凭据交付；拒绝返回结构化错误和 close frame。

## 模型目录与执行依据

全局目录的提供商、名称、类型、启停状态与套餐范围共同决定可见和可执行模型。账户旧模型配置只保留历史显示元数据，管理端只读。ChatGPT 列表为缺少显示元数据的已授权文本模型生成 slug/title/description；带旧版本组时补齐安全的基础选择，防止 Desktop 优先旧版本组而遗漏新模型。分类、分组、滑块和默认选择一并剔除无权或非文本模型。

Codex 模型协议描述来自固定参考提交的 `codex-rs/models-manager/models.json`，副本为 `crates/codex2api-upstream/src/chatgpt-models.json`，不导入官方 crate、不升级基线、不使用供应私人目录。管理端显示已验证元数据状态和来源。没有已验证描述的模型不伪造推理强度、工具或模态能力，也不会出现在 Codex 的完整描述列表；ChatGPT 最小列表仍可显示已授权文本模型。

| 执行入口 | 实际构造依据与处理 |
| --- | --- |
| Responses、Guardian、Guardian classifier | 固定源码 `codex-api/src/common.rs::ResponsesApiRequest` 的必填 model；普通/异常 HTTP 与 WS 都走统一授权。 |
| Search、Memory summarize | 固定源码 `codex-api/src/search.rs::SearchRequest` 与 `common.rs::MemorySummarizeInput` 都带必填 model，不推测上游默认。 |
| 图片生成/编辑 | 固定 `ext/image-generation/src/tool.rs` 实际构造独立 `/images` 请求并指定模型；按该入口检查和结算，不为未观察到的 Responses 工具字段添加推测规则。 |
| Realtime | 安装包 `app-initial` 的 `cml` 明确发送 session.model；固定 realtime protocol 包含 session.audio.input.transcription.model。前置检查并保存会话模型，未知默认模型明确拒绝。 |
| ChatGPT 原生会话 | 安装包调用 prepare 后发送生成；prepare 和生成使用统一模型授权。resume 恢复既有 SSE，核对账号/供应归属而不重复收费。 |
| 云任务 | 安装包新建和 follow-up 可省略 metadata.model_slug；follow-up 只继承本账户、同一供应执行的已记录模型并显式传给上游。无法确认模型时返回 task_model_unavailable，保存实际失败操作记录，不创建成功任务。 |

## 已执行的验证

重构前基线保存于 `target/refactor-baseline-20260922-153651.zip`，原测试日志为 `target/virtual-only-baseline-tests.log`。本轮存储迁移与隔离测试覆盖旧费用/身份/历史保留、固定提供商、跨提供商套餐更新与供应凭据冲突拒绝、路由清空后旧版本拒绝、客户端写权限及 scope 收缩。

服务端 loopback WebSocket 测试覆盖错误 metadata（对象 size、数值 reasoning.effort）、未授权 generate:false、Realtime session/response 字段歧义、附属 transcription 模型、额度错误和实际 close frame；被拒帧转发计数为零。另一个测试在收到 response.created 后撤销凭据，确认已到达的 response.completed 用量和费用落库，再向客户端返回 401 并关闭。

实际 Desktop 证据使用动态发现的安装包 `OpenAI.Codex 26.915.4065.0`，其原生 app-server 报告 `0.155.0-alpha.9.2`。这只是客户端兼容性证据，代理上游固定版本仍为 `0.154.0`、参考提交仍为 `a8964cb1bad67bc26a826fb07d1bef99c6a3f008`。

- `scripts/windows/Test-DesktopModelCatalog.cjs` 从真实 app.asar 读取 schema 与 `Icn` 选择器，并按 app-shared 的实际 import 定位 runtime。最小目录、旧 A 版本组加全局新 B、空目录均执行真实 reader；基础项没有伪造推理能力。集成结果见 `target/core-desktop-catalog-tests.log`。
- `Test-DesktopQuotaContract.cjs` 与 `Test-DesktopRateLimits.py` 对同一实际服务响应验证身份、整数 used_percent、300/10080 分钟窗口和 reset_at；原生 `account/rateLimits/read` 接受整数，旧浮点响应确实被 reader 拒绝。5 小时和 7 天分别耗尽均通过，见 `target/core-native-quota-test.log`。
- 原安装 codex.exe 在 Windows 官方包身份宿主 `Invoke-CommandInDesktopPackage` 内运行，完成 JSON OAuth 登录、configRequirements、remoteControl 注册与认证 WebSocket、退出重启及刷新。见 `target/core-native-desktop-package-test.log`。测试使用临时 SQLite、凭据和配置，未复制运行时、修改安装包、ACL 或生产启动器，也未覆盖用户当前账号。

原安装 app-server 的 `model/list` 已单独验证：临时账户只授予 luna/astra，实际返回恰好两条固定来源描述，推理档位和分页都可读取；Rust fixture 还断言 `/backend-api/codex/models` 确实命中本代理，排除只读取本地内置目录。见 `target/core-native-models-granted.log`。同一 fixture 改为空授权目录后，实际 app-server 返回了九条内置模型，见 `target/core-native-models-empty.log`；这是已观察到的客户端 fallback，不能把界面出现的模型当成服务权限。没有可用描述的目录会产生同样的空服务列表。服务端始终按当前套餐和目录拒绝无权执行，未添加伪模型或修改客户端来绕过该行为。

以上原生检查证明实际客户端协议读取、登录及主机注册流程，未执行供应端真实推理，不宣称任务生成或模型推理成功。

## 最终验收结果（2026-09-23）

- `cargo fmt --all --check`、`cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings` 全部通过。
- `cargo test --workspace --all-targets --no-fail-fast`：37 个目标，共 179 通过、0 失败、2 常规忽略。忽略项分别是需要真实外网代理的检查和原安装 Desktop app-server 检查；后者已按上文使用真实原生运行时单独执行成功。外网代理检查本次未执行。结果见 `target/refactor-final-tests.log`。
- 前端最终产物经负责实现的智能体及主审核完成 format/lint/typecheck、12 项单测、Next 静态导出与真实浏览器链路检查，包括弹窗底部保存栏边界、OAuth 授权兑换及消费令牌访问管理 API 的 401。Rust 构建校验前端源码与导出清单一致。
- `cargo build -p codex2api --release` 通过，产物 `target/release/codex2api.exe`，26,567,168 字节，SHA-256 `1c56fed1a6ba3b9a21178c4ef41c98993fedc56a4ffaee21bfd1dd0f2baf3e1d`。没有发布、替换或重启生产服务。
- Release 二进制在独立临时工作目录启动，无 frontend 目录或 Node 服务。`/admin/`、授权页、一个 hashed JS 和 CSS 均与最终静态导出字节一致；匿名 session 仅返回 `authenticated:false`，受保护管理 API 和 `/v1/models` 返回 401。测试进程 PID 12644 已停止，临时目录已清理；见 `target/refactor-release-smoke.json`。
- 同次 smoke 使用现有数据库的只读快照，从迁移 28 升到 33，保留 2 个供应账户的 installation_id/冻结指纹、2 个消费账户、1 个设备、1 条路由和 1,087 条用量的所有价格快照及费用。仅临时副本中的 2 条 in_progress 记录按既有启动恢复规则改为 interrupted/missing_usage，其余历史列逐一相等。原 `data/codex2api.sqlite` 未迁移、未改写。

## 保持明确的能力边界

没有接入的虚拟插件/连接器目录授权及私有执行器返回具名未实现错误；不能把供应账户目录/资源替换身份后交给消费账户。原有本账户安装记录及客户端偏好照常读取。云自动化等未接入执行器仍保持明确的未实现状态。

Realtime、云任务及其他不能可靠结算的操作对有限额账户仍拒绝；不限额执行保留未计价状态。云任务缺少显式或已记录模型时拒绝，管理员可在任务执行/操作记录中查看来源模型或失败原因。未配置价格、未报告 usage、缺少已验证 Codex 模型描述都不解释成免费或成功。官方供应 OAuth 交换取得的内部 openai-api-key 属于固定官方供应适配协议，继续保留，与已删除的客户端 API Key 模式无关。
