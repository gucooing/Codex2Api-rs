# AI API 网关

AI API 网关在同一 Rust 进程的独立端口提供 ChatGPT、Grok 和 Antigravity 模型调用。默认地址为
`http://127.0.0.1:8083/v1`。`CODEX2API_API_BIND` 控制监听地址，
`CODEX2API_PUBLIC_API_URL` 提供公开 origin 的启动默认值；管理员在
**设置 / 访问地址 / AI API 网关** 保存的 origin 优先。用户 API Key 页面读取这一
地址，不从浏览器地址或转发头推测端口。反向代理可使用独立域名指向 8083，也可将标准 `/v1/*` 路由到此入口，
将 `/v1/chatgpt`、`/v1/grok` 及其子路径路由到 OAuth 入口 8080。Antigravity 没有 OAuth 客户端服务根，只通过本网关调用。

## 调用与平台

Messages 可使用 `x-api-key`；同时携带 Bearer 时必须一致，显式 Bearer 无效时不回退。
所有模型调用与模型发现均支持 `Authorization: Bearer <API Key>`。Key 绑定一个
API 组；组没有平台，执行时从组的标签号池中选择已配置该模型的供应账户，由该账户
的平台决定上游协议。调用者不能用请求头切换供应账户或平台。
浏览器会话、OAuth access/refresh token 和 API Key 各自独立。

API 组手动授权模型：组的模型列表与管理员目录中已启用、未删除模型的交集即可用范围，每个模型可单独设置倍率、每用户并发和 RPM。管理端候选项、用户模型广场、`GET /v1/models`、HTTP 与 WebSocket 准入使用相同范围。标签可多选且不限平台，仅用于选择执行号池；标签和供应模型变化不改变组的模型权限。Responses 压缩、ChatGPT 搜索、输入计数和 Responses WebSocket 只在 ChatGPT 供应账户上执行；图片接口可使用 ChatGPT 或 Grok。调度在选择供应账户和复用会话绑定前检查协议支持范围。用量记录以请求模型的平台标签标识请求，并保留实际执行的供应账户。号池暂时没有支持该模型的可用供应账户时，返回供应不可用错误，不越出所配置号池。

| 路径 | 行为 |
| --- | --- |
| `GET /healthz` | 无凭据的进程健康检查 |
| `GET /v1/models` | 当前 API 组可用的模型列表 |
| `POST /v1/responses` | Responses JSON / SSE，保留完整输出项、工具结果和实际用量 |
| `GET /v1/responses` | ChatGPT 原生 Responses WebSocket，使用同一个 Bearer API Key |
| `POST /v1/messages` | Messages JSON / SSE，按实际供应账户协议转换，保留工具调用、图片及实际用量 |
| `POST /v1/chat/completions` | Chat JSON / SSE，包含 function/custom 工具、结构化输出、推理、拒绝、引用和音频字段 |
| `POST /v1/responses/compact` | ChatGPT 上下文压缩，JSON 请求和响应，按真实 Token 结算 |
| `POST /v1/responses/input_tokens` | ChatGPT 输入计数，返回实际计数结果，不作为生成 Token 扣费 |
| `POST /v1/alpha/search` | ChatGPT 官方独立搜索协议，按管理员配置的每次价格结算 |
| `POST /v1/images/generations` | 对应渠道原生图像生成 |
| `POST /v1/images/edits` | JSON 图像引用 / file_id，或 multipart 图片上传 |

## 请求转换

API 模块负责 HTTP 入站、API Key 鉴权、准入、调用执行器、下游交付和钱包结算。执行器位于
`codex2api-upstream/src/executor/`，转换矩阵位于 `codex2api-upstream/src/convert/`，各渠道的传输与裁决位于
upstream 的 `providers/<channel>/`。先按 API 组号池与模型权限选中供应账户，再把入站请求直接转换为该账户线格式的官方请求；每次尝试都重新转换，所有尝试保持同一个请求标识。

每个供应账户只说一种线格式，按渠道固定，不按模型描述或模型名推断：ChatGPT 为固定 Codex 的 Responses 方言；Grok 为 Grok Build 的 xAI Responses 方言（固定参考提交的内置官方模型表 `xai-grok-models/default_models.json` 为官方模型声明 `api_backend: responses`）；Antigravity 为 Cloud Code Assist 的 `v1internal:streamGenerateContent` Gemini 线协议；API 渠道账户为所配置的 OpenAI Responses、Chat Completions 或 Anthropic Messages。

### 转换矩阵

没有中间标准格式，也没有直通：每个（入站格式，线格式）组合是一对函数——把下游原始请求转为该线格式的官方请求，把该线格式的事件转为下游协议事件。`convert::supports` 声明矩阵，选号时直接排除没有对应转换的供应账户。

| 入站 | 线格式 | 模块 |
| --- | --- | --- |
| Responses、Chat、Messages | `codex`、`xai`、`openai_responses` | `convert/responses/`：请求按三种方言各自的字段策略重建，事件按入站协议转换 |
| Responses、Chat、Messages | `gemini` | `convert/gemini/`：请求构造为 `contents`、`systemInstruction`、`generationConfig` 与函数声明，Gemini SSE 分别转为 Responses 事件、Chat 分块或 Messages 事件 |
| Responses、Chat、Messages | `chat_completions` | `convert/chat_completions/`：请求构造为 messages、tools、`max_completion_tokens`、`reasoning_effort`、`response_format`，分块分别转为三种下游事件 |
| Responses、Chat、Messages | `messages` | `convert/messages/`：请求构造为交替的 user／assistant 内容块、`input_schema` 工具、`max_tokens` 与 thinking 预算，Messages 事件分别转为三种下游事件 |
| `codex_client` | `codex` | 官方客户端请求只去往其线格式，渠道重绑供应身份 |
| `grok_client` | `xai` | 同上 |
| 图像生成、图像编辑 | `gemini` | `convert/gemini/images.rs`：非流式 `generateContent`，参考图映射为 `inlineData`，结果转为 `b64_json` |

同名协议也要转换：Responses 调用方打到 ChatGPT 账户时按固定 Codex 的请求形态重建，打到 OpenAI Key 时按 OpenAI API 的字段范围重建。文本生成与官方客户端一致，始终以流式方式请求上游（Antigravity 使用 `alt=sse`，Chat Completions 带 `stream_options.include_usage`）。客户端选择同步时，网关把转换后的同协议事件聚合为该协议的完整 JSON，因此同步与流式输出一致。用量账本按上游线格式的真实事件计量（`convert::observe`），不以转换结果计费。转换后的终态事件在结算完成后才交付客户端；没有终态事件就结束的上游流按未完成失败处理。

流式请求收到正文、思考或工具参数增量就向客户端发送，即使同一上游分块还包含结束标记，也只延后完成事件与最终用量。Messages 的并行工具块按创建顺序输出，前一块参数未结束时，后一块等待；不会为凑齐整个回答而缓存已可交付的内容。

| 入站 → 线格式 | 主要映射 |
| --- | --- |
| Responses → Responses 方言 | 字符串 input 与内容别名、Chat 风格嵌套工具与 tool_choice、`instructions` 转为首个系统级消息（Codex 为 developer 角色，xAI 为 system 角色）、reasoning 项去除仅输出字段、结构化工具输出内容与发现的工具 |
| Chat → Responses 方言 | system/developer/user/assistant、tool 输出、function/custom 调用、`reasoning`/`reasoning_content`、response_format、verbosity、web_search_options、logprobs |
| Messages → Responses 方言 | system（去除 Claude Code 的 `x-anthropic-billing-header`）、文本、图片、文档、tool_use/tool_result；带签名的 thinking 以 `encrypted_content` 回放，无签名的 thinking 不回放 |
| Responses／Chat → Chat Completions | input 项或 messages 转为 messages，function_call 并入 assistant 的 `tool_calls`，function_call_output 成为 tool 消息，reasoning 文本成为 `reasoning_content`，`max_output_tokens` → `max_completion_tokens`，`reasoning.effort` → `reasoning_effort`，`text.format` → `response_format` |
| Messages → Chat Completions | system、文本／图片／文档块、tool_use → `tool_calls`、tool_result → tool 消息、thinking → `reasoning_content`、`max_tokens`、`stop_sequences` → `stop`、思考预算按大小映射推理强度 |
| Responses／Chat → Messages | 系统文本合并为 `system`，内容转为交替的 user／assistant 块，function_call → `tool_use`，输出 → `tool_result`，带签名的 reasoning → `thinking`，推理强度 → `thinking.budget_tokens`（`max_tokens` 缺省 8192 并保证大于预算），`parallel_tool_calls=false` → `disable_parallel_tool_use` |
| Messages → Messages | 去除本地字段与客户端归属块，其余按原样发送 |

Messages 工具选择映射为 `auto`、`required`、`none` 或具名函数；
`disable_parallel_tool_use` 不改变 Responses 的并行调用。Messages 推理关闭时为
effort `none`，否则取 `output_config.effort`（缺省 medium）并请求自动摘要。
空工具结果以 `(empty)` 发送。`stop_sequences`、`top_k` 与 `metadata` 不发往
Responses 方言。

Messages SSE 同一时刻只开放一个内容块；交错的上游项目在前一块完成后依次发送。
`message_start` 中的计数按流式惯例为 0，实际输入、缓存读写和输出用量在
`message_delta` 中给出。thinking 块的签名在 `content_block_stop` 之前发送。
终态快照只补发增量未送达的后缀，不重复发送文本或参数。

函数参数、结构化工具输出、多模态内容和工具调用 ID 保留。
reasoning 回放保留 encrypted_content、summary/content，并处理 store=false 下
无法检索的 reasoning ID；已有 include 数组也补齐加密推理内容。

客户端提供的 session/thread、request ID 和 `prompt_cache_key` 保留其语义，不按
供应账户或 API Key 重新派生。`client_metadata` 仅接受合法动态协议上下文；供应
凭据、安装 ID、冻结指纹和版本始终由对应渠道的 transport 提供。调用者的 Cookie、
API Key、UA、组织/账户选择头和 access_programs 不用于选择供应身份。
普通 `metadata` 留在本次请求内，并在 Responses/Chat 响应中回显；Messages 不回显该字段。它不写入用量表或请求正文存储。
`x-codex-turn-state` 可回传及再次携带，转发前核对调用者和选中供应账户的归属。

ChatGPT 生成请求按固定 Codex 的请求形态发送：`store=false`、`stream=true`，
补齐 `reasoning.encrypted_content`，不发送 `instructions`。订阅上游不接受的
`max_output_tokens`、`max_completion_tokens`、`max_tokens`、`temperature`、
`top_p`、penalty、`chat_template_kwargs`、`user`、`metadata`、
`prompt_cache_retention`、`safety_identifier`、`stream_options`、`truncation`、
`stop`、`stop_sequences`、`seed`、`logit_bias` 不上送，也不会被伪装成已生效；
实际用量仍按供应报告结算。具名 `tool_choice` 只指向已声明的工具，未声明时由
模型自行选择（`auto`）。ChatGPT 的 HTTP Responses 不接受 `previous_response_id`，
需要续接时使用原生 Responses WebSocket。

Grok 按 Grok Build 的 Responses 构造器发送：`store=false`、`stream=true`，补齐
加密推理。Grok 保留其原生采样、输出限制和搜索工具字段，不套用 ChatGPT 的参数
处理规则；没有工具或具名工具未声明时不发送工具选择。

Antigravity 按官方桌面客户端的 v1internal 信封发送：`project`、`agent-` 前缀的 `requestId`、
`userAgent: antigravity`、`requestType: agent`、`model` 与 Gemini `request`；请求与事件转换细节见 [Antigravity 渠道](ANTIGRAVITY.md)。API 渠道账户的端点、认证头与转换见 [API 渠道](API_ACCOUNTS.md)。

### 错误与换号

上游错误由选中账户的渠道裁决：请求本身的错误按下游格式原样返回；临时错误同号重试；授权、账单、额度、权限、过载、5xx 与传输错误回避该账户（仅本次请求、限时或永久，账户级或某个模型）并换号。上游 429 从不转发。一次请求最多换号的次数由设置中的“请求重试次数”决定（默认 3 次，所有渠道共用），候选耗尽或次数用尽返回 503 `no_available_supplier`，号池内没有任何账户配置该模型返回 404 `model_not_found`；调用者只在本服务自身限制（RPM、并发、排队、预算）时收到 429。供应凭据、账单、额度与权限细节只进管理端账本。

后台持久化作业、生成 `n>1`、查询/删除已存储响应不是 Codex 订阅协议提供的能力。
固定 Images 协议不提供 mask、流式输出或托管图片 URL；对应请求明确返回不支持，
不会伪造异步任务、下载地址或成功结果。Grok、Antigravity 与 API 渠道没有 Codex 搜索、
压缩、计数和原生 Responses WebSocket 传输；Antigravity、Chat Completions 与 Messages 账户不提供 `previous_response_id` 续接，API 渠道不提供图像生成，这些路径明确拒绝。

Antigravity 图像接口采用官方 Desktop 的 `requestType: image_gen` 与非流式 `generateContent`。每次一张图（`n: 1`），输出 `b64_json`；编辑支持 base64 data URL 和 multipart，最多 14 张参考图。可设置 `aspect_ratio`，`size` 仅接受 `auto`。不支持的参数直接拒绝；图片数量和尺寸按实际响应结算，思考中间图不计入交付。完整约束与示例见 [Antigravity 图像接口](ANTIGRAVITY.md#图像接口)。

## 搜索与计数

独立搜索直接发送官方 `SearchRequest`，字段为 `id`、`model`、`reasoning`、`input`、
`commands`、`settings`、`max_output_tokens`。id 缺省时生成本次请求标识；model 必须
属于 API 组的可用模型。commands 保留搜索、图片搜索、open、click、find、截图、
金融、天气、体育和时间等原生操作。不会把搜索替换成一次模型文本回答。

```json
{
  "model": "管理员启用的模型",
  "commands": {
    "search_query": [{"q": "Rust async runtime"}],
    "response_length": "short"
  }
}
```

搜索 JSON 返回实际 `output`，以及可选的 `encrypted_output`、`results`。搜索按次
价格由管理员在每个 ChatGPT API 组的 `search_price_usd` 中独立配置，单位为 USD / 次。
该金额直接作为最终单价，不再乘组或模型倍率，也不回退到订阅搜索价格。Grok 与 Antigravity 不提供
该设置。请求开始时冻结单价，后续改价不影响历史。缺价明确拒绝；明确零价允许零余额。
用户模型广场的搜索卡片展示所选 API 组的搜索单价。输入计数继续检查凭据、模型范围和调用限制，但不要求钱包余额、不触发偿债，
返回的预估 Token 数不写成已消费 Token。

## 响应与连接

HTTP JSON、SSE 和原生 WebSocket 共用请求准入、供应调度和 usage_records。
共用响应识别器按实际帧识别 SSE，支持缺失或错误 Content-Type，以及 SSE event
字段提供事件类型的响应。正常完成、未完成、流中错误和传输中断分别处理；HTTP 200
只是连接状态。没有已生成输出或用量时才允许因确定的授权失效、额度耗尽或 402
切换供应；普通限流交给客户端退避，并保留结构化错误和 Retry-After。

同步和流式 Chat 均保留真实工具调用、reasoning、refusal、annotations、logprobs、
音频以及 usage。Chat 工具使用 `tools`、`tool_choice` 和 `tool_calls`。仅当调用者要求
`stream_options.include_usage=true` 时发送最终 Chat 用量 chunk。响应清理只处理
供应协议信封；正文、JSON schema、工具结果和搜索 results 中同名业务字段不会被删除。

WebSocket 使用真实上游连接，支持固定协议的 `response.create`、`generate=false`
预热及 `response.interrupt`；其余事件明确拒绝。每次生成重新检查 Key、组策略、模型、
余额、RPM 和并发。同连接可复用多轮；空闲连接不占生成并发。终态发送给客户端之前
提交用量和钱包结算；客户端离开后收尾已开始的生成，错误响应包含 CloseFrame。
预热不虚构消费记录或费用。

## 钱包与记录

API Key 由普通用户在用户网站创建并归属于该用户；订阅身份不单独登录用户网站。
Key 使用随机秘密，数据库只保存哈希与可展示前缀；完整 Key 仅在
创建或轮换成功时返回一次。Key 只有启用和停用两种状态，可改名、启停和改绑其他可用
API 组；轮换在原 Key 身份与用量历史下签发新秘密，旧秘密立即失效；删除不可恢复，
Key 退出列表并停止鉴权，已产生的用量记录保留归属与名称。停用、删除、用户停用、
组停用或删除均阻止后续准入。准入时发现 Key 已改绑其他组或组规则已变，返回组已更改；
请求在等待供应商或重试时重新核对 Key 与组版本，此时发现任一变化与凭据失效同样处理。

请求者 IP 在入口解析一次并写入同一用量记录，规则见[架构说明](ARCHITECTURE.md)：只有直接对端
属于可信代理网段时才读取转发头，客户端直连时伪造的转发头不生效。管理员和本人可在
用量记录与详情中查看。

所有 API 和 OAuth 请求统一写入 `usage_records`，使用相同的用量字段、错误记录、
写入与结束处理、查询和统计。API 钱包扣费不消耗 OAuth 订阅的 5 小时、7 天、
30 天额度。使用量只保存账单元数据，不持久化请求提示、工具输入或图片内容。
模型基价与倍率在开始时快照；完成时以实际普通输入、缓存读取、缓存写入与
输出结算，推理 Token 已包含在输出中，不重复收费。图像根据成功返回数量
和实际分辨率结算，复用模型计费中匹配档位或最近已配置档位的规则；整个模型
未定价或缺失数量保持未知；按分辨率定价还要求实际尺寸。统一图像价格使用单独的 `*` 价格规则，对全部分辨率生效，不能与尺寸规则混用。

费用以整数 nano USD 计算，用户钱包仍为整数美分。小于一美分的费用保存于
`usage_wallet_state`，累计达到一美分才原子扣减余额并写不可变 `usage`
钱包流水，不对每个微小请求单独向上取整。已执行的并发请求可能超过剩余
余额，超出部分保留待扣，后续收费调用受余额检查约束，不丢失真实已消费费用。
管理员增加余额或下一次 API 准入时，先以原账单来源清偿已知的整美分待扣款。
订单支付和管理员减少余额需保留 API 已发生的待扣金额。进程重启将未完成
请求恢复为中断，缺失的用量保持未知，不能按零费用补出成功记录。
结算通过请求 ID 幂等，修改价格或倍率不会重算历史。API 扣费、偿债、订单支付及管理员余额调整不改变用户身份版本；管理员增减金额按事务内实时余额执行，正常消费不会使待提交调整报版本冲突。

用量列表返回 `billing_calculation`，只包含本次结算使用的用量、单价、倍率和金额；
原始价格目录快照不向浏览器发送。管理端另有 `cost_calculation`，按同一请求的
模型计费快照与实际用量计算，不含 API 组或模型倍率；按次搜索分别快照基础操作价和
API 组售价。缺少历史价格或用量时不补造公式。用户 API 不返回成本字段。

请求失败数字状态读取上游错误对象或事件中的 `status_code` / `status`，接受整数与
整数字符串；普通上游拒绝保留其实际响应状态。成功连接的 200/101 不作为失败码，
不从 `error.code`、错误类型或原因文字推断数字。错误原因仍单独保存和展示。

**API 密钥** 查看 Key 和当前并发，所有请求统一在管理员 **用量记录**、用户
**使用信息** 查看真实用量与费用。失败分两种视图：管理员看到真实错误、供应渠道以及
网关已恢复的每次失败尝试（刷新授权后重试或切换供应账户）；用户与调用方只看到错误处理后的
结果，已恢复的失败对用户是正常请求。可由调用方修正的错误（如推理强度、模型不存在、参数无效）
保留错误码和脱敏原因，供应授权、计费、额度、权限和号池容量统一为 `service_unavailable`。用户查询始终绑定本人所有权。
钱包流水通过 `usage_record_id` 关联同一条用量记录；没有独立 API 用量接口。
两端用量表将“模型”和“接口”分列；“类型”显示 http/流式、http/同步或 websocket/流式。
管理端接口分“入站”和“上游”两行。`upstream_endpoint` 从发起上游请求的实际 URL 提取路径，
例如 Antigravity 的 `/v1internal:streamGenerateContent`、ChatGPT 的
`/backend-api/codex/responses`，以及 API 供应账户 Base URL 下的协议路径；不保存域名和查询参数。
重试与切号各自保留失败尝试的上游路径，主记录保存最终供应商实际请求的路径；未选择供应商或
最终供应商尚未发起请求时保持空值，显示“—”，不沿用上一家供应商的路径。用户端只展示入站接口，DTO 显式排除上游路径、供应身份、
路由标签、秘密、指纹与供应商额度。
**模型广场** 默认展示管理员模型计费目录，横向切换 API 组后展示模型有效倍率、
倍率后的价格、不重叠的标准输入范围及覆盖全部标准区间的服务档位倍率。图片支持统一价和分辨率价格。模型限额显示在组内模型卡片及详情，组限额单独显示；其他计费展示基础或当前组的搜索单价，缺价保持未知。
