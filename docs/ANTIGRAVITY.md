# Antigravity 渠道

Antigravity 是独立的供应渠道，上游为 Google Cloud Code Assist 的 Antigravity 桌面客户端协议。它通过标准 AI API 网关提供 Responses、Chat Completions、Messages 和 Images 调用，不提供 `/v1/antigravity` OAuth 客户端服务根，也没有套餐、订阅或客户端状态协议。官方常量位于 `crates/codex2api-common/src/providers/antigravity/version.rs`，当前对齐 Antigravity Desktop `2.24.0`（原生构建 `1000335682`）。请求格式依据该版本 `language_server` 的 `UserAgent`、`fetchAvailableModels`、`fetchRetrieveUserQuotaSummary` 与 `GetImageGeneration` 实现，额度字段与展示顺序对照其 renderer 和本机已登录客户端的实际返回。sub2api 仅作为官方交互与协议转换的参考，不是依赖，也不引入其模型映射、额度抓取或积分功能。

## 目录与账户隔离

各 crate 的 `src/providers/antigravity/` 保存 Antigravity 身份、认证、请求、响应、错误与管理实现；前端渠道表单与官方数据视图位于 `frontend/src/components/providers/antigravity/` 与 `frontend/src/lib/providers/antigravity/`。入口模块仅负责渠道分发，`scripts/check-module-boundaries.py` 检查各渠道之间的跨渠道导入。

供应凭据使用独立的 `AntigravityCredentials`（Google access token、refresh token、到期时间、Cloud Code Assist 项目、档位和用户资料），`auth_mode` 为 `antigravity`，不会被解释为 Codex `auth.json` 或 Grok 凭据。每个供应账户在 PostgreSQL 内保存独立的安装 UUID、冻结的操作系统／架构／系统版本、可选时区、凭据版本和代理；HTTP 连接与刷新锁按账户隔离。供应身份由 `google:<邮箱>` 标识，不跨提供商合并。

## 供应授权和资料

供应授权提供代码／回调与 RT 两种方式，没有设备码。代码授权生成带 PKCE S256、`access_type=offline` 和 `prompt=consent` 的 Google 授权链接，回调地址为官方客户端的 `http://localhost:8085/callback`；管理员可以粘贴浏览器返回的纯授权代码或完整回调链接，链接必须匹配本次 state。RT 通过 Google OAuth 令牌端点换取访问令牌，用 `userinfo` 核实邮箱后保存；一行一条，批量新账户独立生成指纹和安装 UUID，重新授权保留原身份。

保存凭据时调用 `v1internal:loadCodeAssist` 读取当前档位与 Cloud Code Assist 项目；没有项目时按默认档位调用 `v1internal:onboardUser` 并轮询完成。档位 `free-tier`、`g1-pro-tier`、`g1-ultra-tier` 分别显示为 Free、Pro、Ultra；付费档位优先使用 daily 服务根，其余使用生产服务根，连接失败或 404 时切换到下一地址。loadCodeAssist 的原始结果保存在官方资料快照中，“刷新官方资料”重新执行上述流程。令牌到期前 300 秒自动刷新；刷新返回 `invalid_grant`、`invalid_client` 或 `unauthorized_client` 时记录授权失效，其他失败保留状态并等待下一次尝试。

## 伪装与上游请求

每个请求都像官方桌面客户端：`User-Agent` 为 `antigravity/hub/2.24.0 (aidev_client; os_type={os}; arch={arch}; cl=1000335682)`，操作系统取 `windows`、`darwin`、`linux`，架构取 `amd64`、`arm64`，由账户冻结输入与公共版本常量在运行时生成，不保存派生 UA，也不使用代理主机信息。生成请求发送到 `v1internal:streamGenerateContent?alt=sse`，信封包含 `project`、`agent-` 前缀的 `requestId`、`userAgent: antigravity`、`requestType: agent`、`model` 与 Gemini `request`。服务根为 `https://cloudcode-pa.googleapis.com` 与 `https://daily-cloudcode-pa.googleapis.com`。

## 协议转换

Antigravity 的线格式是 Cloud Code Assist 的 Gemini 请求；转换矩阵 `codex2api-upstream/src/convert/gemini/` 直接从 Responses、Chat 或 Messages 的原始请求构造它，不经过中间格式。`request.rs` 分别为三种入站格式把消息映射为 `contents` 的 user／model 角色，包含文本、带 `thoughtSignature` 的思考、`inlineData`、`functionCall` 与 `functionResponse`；system／developer 文本进入 `systemInstruction`，排在客户端身份提示与边界提示之后；`generationConfig` 携带 `maxOutputTokens`（缺省 64000）、temperature、topP、按推理强度映射的 `thinkingConfig`，以及结构化输出的 `responseMimeType`／`responseSchema`；函数工具转换为清理后的 `functionDeclarations`（`schema.rs` 内联 `$ref`、合并 allOf／anyOf／oneOf、常量转枚举、约束写入描述，只保留 Gemini 接受的字段），`tool_choice` 映射为 `toolConfig.functionCallingConfig`；没有函数工具时才声明 `googleSearch`。`sessionId` 优先取会话头或 `prompt_cache_key`，否则从首条用户消息派生；Responses 的字符串输入、字符串 content 和文本块输入保持同一会话标识，后续轮次不会重建。渠道传输 `providers/antigravity/` 再加上账户绑定的 v1internal 信封并解析所选账户对应推理强度的上游标识。

`events.rs` 把 Gemini SSE 分块直接转换为调用方协议：Responses 调用方得到 reasoning、message、function_call 输出项与 completed／incomplete／failed 终态；Chat 调用方得到带 `reasoning_content`、`tool_calls` 与 finish_reason 的分块，思考签名随 `reasoning` 扩展回放，工具签名使用 Google 的 `tool_calls[].extra_content.google.thought_signature`；Messages 调用方得到按顺序写出的 thinking、text 与 tool_use 块。`finishReason` 的 `STOP` 为完成，`MAX_TOKENS` 为 `max_output_tokens`，安全类原因为 `content_filter`，`MALFORMED_FUNCTION_CALL` 为失败；用量取 `promptTokenCount`（含缓存）、`cachedContentTokenCount`、`candidatesTokenCount` 加 `thoughtsTokenCount`，推理 Token 为 `thoughtsTokenCount`。内容增量随收到随发送，终态事件在流结束时发出，缺少终态按未完成失败处理。续接 `previous_response_id`、Codex 搜索、压缩、计数和原生 Responses WebSocket 不属于该渠道，明确拒绝。

签名必须随其原始思考块或工具调用回传，不能合并到相邻的思考文本。Responses 和 Messages 用独立的空 reasoning／thinking 块携带后续工具的签名；Chat 把签名绑定在具体工具上。客户端续接时须保留相应签名字段和工具调用 ID，网关无法恢复被客户端丢弃的签名。规则依据 [Google thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)。

缓存读取计数来自上游 `usageMetadata.cachedContentTokenCount`，总输入包含缓存，思考计入输出一次。分散在多个分块中的计数合并后交付，下游三种格式和本地账本保留官方报告值；缺失字段不证明未命中缓存，缓存写入没有官方测量时保持未知。会话稳定性有助于复用，但不保证上游命中。

## 图像接口

`POST /v1/images/generations` 与 `POST /v1/images/edits` 通过 `convert/gemini/images.rs` 直接构造 Gemini 请求，发送到非流式 `v1internal:generateContent`。信封使用 `requestType: image_gen` 与请求级 `image_gen/<UUID>` 标识，`contents` 包含提示词和参考图的 `inlineData`；`generationConfig` 使用 `candidateCount: 1` 与 `imageConfig.aspectRatio`，缺省比例 `1:1`。不加入聊天系统提示或工具。图像模型取供应账户同步结果，例如官方 `imageGenerationModelIds` 返回的 `gemini-3.1-flash-image`；不会把聊天选择器的模型列表当作完整目录，也不硬编码模型白名单。

每次请求一张图，`n` 只能为 1；输出为 `b64_json`。编辑接受最多 14 张 PNG、JPEG 或 WebP 的 base64 data URL，或现有 multipart 上传。可指定 `aspect_ratio`；`size` 仅接受 `auto`，其他尺寸、quality、mask、远程图片 URL、file_id、流式和异步任务均明确拒绝。返回只包含完成的图片及实际尺寸，排除 `thought` 中间图；无图、截断或非法图像返回失败，不构造成功结果。真实 Token 用量写入账本，按请求开始时管理员配置的图像价格及实际返回张数结算一次，缺失价格保持未知。使用前在模型管理中将对应模型设为图像、配置价格并授权给 API 组。

示例：

```json
{"model":"gemini-3.1-flash-image","prompt":"画一棵树","n":1,"response_format":"b64_json","aspect_ratio":"1:1"}
```

## 模型、额度与计价

供应账户“可用模型”的“同步上游模型”调用官方 `v1internal:fetchAvailableModels` 并替换该账户的上游模型。模型按官方返回的显示名称分组：去掉末尾的推理强度标签 `(Low)`、`(Medium)`、`(High)` 后显示名称相同的上游标识属于同一个模型，包括显示名称完全相同的别名标识。上游标识本身不表示推理强度，例如显示名称 `Gemini 3.5 Flash (High)`、`(Medium)`、`(Low)` 分别对应 `gemini-3-flash-agent`、`gemini-3.5-flash-low`、`gemini-3.5-flash-extra-low`，而 `gemini-2.5-flash`、`gemini-2.5-flash-thinking`、`gemini-3.5-flash-lite` 都显示为 `Gemini 3.5 Flash Lite`。客户端使用的模型名由显示名称转换：小写，空格与连字符合并为一个连字符，其他标点去掉，例如 `gemini-3.5-flash`、`gemini-3.5-flash-lite`、`gpt-oss-120b`、`claude-opus-5.5`；统一内置模型表的 Gemini 与 Claude 模型名使用同一形式。其他标签（如 `(Thinking)`）属于模型名称，没有显示名称的标识以标识本身为名。模型描述保存每个推理强度对应的上游标识、不带强度标签的上游标识（多个别名时优先与模型名相同的标识，否则取字典序第一个）与所属额度池。同步只改变供应账户的可用模型，从不写入计费配置：统一内置模型表在启动时登记的模型之外，需要计费的模型由管理员在计费配置中添加。

请求使用客户端模型名，推理强度通过协议自身的参数传入：Responses 的 `reasoning.effort`、Chat Completions 的 `reasoning_effort`、Messages 的 `output_config.effort`（缺省 `medium`，关闭思考为 `none`）。有推理强度变体的模型只接受所选供应账户实际返回的强度标签：`low`、`medium`、`high` 分别对应 `(Low)`、`(Medium)`、`(High)` 的上游标识；未指定强度时使用不带强度标签的上游标识。没有对应标识时直接返回 400 并列出该模型支持的推理强度，不降级、不替换为其他强度。推理强度在选中供应账户之后校验，同一号池的账户应返回相同的变体；选中账户缺少所请求的强度时请求失败，不切换账户。没有推理强度变体的模型发送其上游标识，推理强度映射为 `thinkingConfig` 思考预算。用量记录保存客户端模型名与请求的推理强度。

统一内置模型表的 Gemini 与 Claude 价格来自 Google 与 Anthropic 的官方公开价格并按长上下文档位分段；GPT-OSS 等没有官方价格的模型保持未定价，不会被当作免费。价格快照在请求开始时冻结，输入、缓存读取、缓存写入、输出各结算一次。

官方额度展示使用 `v1internal:retrieveUserQuotaSummary` 的 `groups`，保留官方顺序、`displayName`、各 `buckets` 的 `bucketId`、`window`、`remainingFraction`、`resetTime` 与 `disabled`。界面按官方剩余比例显示：Gemini Models 和 Claude and GPT models 各自显示实际返回的周窗口及 5 小时窗口；Gemini 4 Argon 是否单独成组由官方账户结果决定，不按模型名称或套餐推断。没有返回的窗口不占位，缺少比例保持未知；该接口的比例为 protobuf oneof，缺失不能解释为零。列表、卡片和详情读取同一份 PostgreSQL 快照，本地倒计时与页面读取不请求上游；详情“从官方刷新”同时读取模型目录及额度汇总，全部成功后更新快照，失败保留此前数据。

调度仍使用 `fetchAvailableModels` 对每个上游标识报告的有效 `quotaInfo`，不把展示窗口反推为模型用量。显示名称与推理强度变体归并后的模型按其有效比例和重置时间冷却或恢复；Claude 与 GPT-OSS 的已声明额度池一起处理。目录的 `quotaInfo` 不是 oneof，官方省略零值比例且提供重置时间时表示已耗尽；完全没有额度信息保持未知。读取汇总失败时，不应用这次目录中的恢复结果。

## 错误处理与调度

裁决位于 `codex2api-upstream/src/providers/antigravity/verdict.rs`，依据 sub2api 的 Antigravity 重试与切换策略：

| 上游情况 | 处理 |
| --- | --- |
| HTTP 401 | 永久回避（授权失效）并换号；成功的重新授权清除 |
| 403 | 回避 10 分钟并换号 |
| 404 | 回避请求的模型 1 小时并换号 |
| 429 或流内 `RESOURCE_EXHAUSTED` 明确说明额度耗尽（`QUOTA_EXHAUSTED`／`QUOTA_EXCEEDED` 或 “exhausted your capacity”） | 回避本次请求的模型及其额度池全部模型到 Google 的 RetryInfo 时间（缺省 60 秒）并换号，不采用 Google 报告的上游或厂商模型名 |
| 其他 429 | 按 RetryInfo 延迟（1 到 8 秒）同号重试最多 3 次，然后限流回避到该时间并换号 |
| 400 “invalid project resource name” | 同号重试 2 次，然后回避 10 分钟并换号 |
| 可由调用方修正的 400 类请求错误 | 原样返回调用者，不改账户状态 |
| 503 `MODEL_CAPACITY_EXHAUSTED` | 回避该模型到 RetryInfo 时间（缺省 60 秒）并换号 |
| 500 `INTERNAL` | 同号重试 2 次，然后回避 30 分钟并换号 |
| 529 | 过载回避 10 分钟并换号 |
| 其他 5xx | 同号重试 2 次，然后仅本次请求回避并换号 |
| 连接失败 | 回避 10 分钟并换号 |
| 流在首个输出前结束 | 同号重试 1 次，然后回避 5 分钟并换号 |

模型回避保存在 `supplier_blocks`（`scope='model'`），以账户、模型和凭据版本为键；模型描述声明同一 `quota_pool` 的模型一起回避、一起恢复。号池索引取账户回避与请求模型回避中较晚者，因此同一账户的其他模型继续参与调度；回避到期自动结束，官方读取报告仍有余量、重新授权或管理端“重置状态”立即清除。候选耗尽时调用者收到 503 `no_available_supplier`，上游 429 从不转发。供应详情“账户资料”显示当前有效的模型回避。

## 导入导出

供应导入导出的 Sub2api 格式支持平台 `antigravity`、类型 `oauth`，凭据字段为 `access_token`、`refresh_token`、`expires_at`、`email`、`project_id`、`plan_type`、`token_type`。导入账户独立生成本地指纹，邮箱作为账户主标识；原生导入校验 `antigravity` 凭据格式与账户身份一致。
