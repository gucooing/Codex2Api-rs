# Antigravity 渠道

Antigravity 是独立的供应渠道，上游为 Google Cloud Code Assist 的 Antigravity 桌面客户端协议。它只通过标准 AI API 网关提供 Responses、Chat Completions 和 Messages 三种调用，不提供 `/v1/antigravity` OAuth 客户端服务根，也没有套餐、订阅或客户端状态协议。官方常量位于 `crates/codex2api-common/src/providers/antigravity/version.rs`，当前伪装的客户端版本为 `2.9.1`。sub2api 仅作为官方交互与协议转换的参考，不是依赖，也不引入其模型映射、额度抓取或积分功能。

## 目录与账户隔离

各 crate 的 `src/providers/antigravity/` 保存 Antigravity 身份、认证、请求、响应、错误与管理实现；前端渠道表单与官方数据视图位于 `frontend/src/components/providers/antigravity/` 与 `frontend/src/lib/providers/antigravity/`。入口模块仅负责渠道分发，`scripts/check-module-boundaries.py` 检查三个渠道之间的跨渠道导入。

供应凭据使用独立的 `AntigravityCredentials`（Google access token、refresh token、到期时间、Cloud Code Assist 项目、档位和用户资料），`auth_mode` 为 `antigravity`，不会被解释为 Codex `auth.json` 或 Grok 凭据。每个供应账户在 PostgreSQL 内保存独立的安装 UUID、冻结的操作系统／架构／系统版本、可选时区、凭据版本和代理；HTTP 连接与刷新锁按账户隔离。供应身份由 `google:<邮箱>` 标识，不跨提供商合并。

## 供应授权和资料

供应授权提供代码／回调与 RT 两种方式，没有设备码。代码授权生成带 PKCE S256、`access_type=offline` 和 `prompt=consent` 的 Google 授权链接，回调地址为官方客户端的 `http://localhost:8085/callback`；管理员可以粘贴浏览器返回的纯授权代码或完整回调链接，链接必须匹配本次 state。RT 通过 Google OAuth 令牌端点换取访问令牌，用 `userinfo` 核实邮箱后保存；一行一条，批量新账户独立生成指纹和安装 UUID，重新授权保留原身份。

保存凭据时调用 `v1internal:loadCodeAssist` 读取当前档位与 Cloud Code Assist 项目；没有项目时按默认档位调用 `v1internal:onboardUser` 并轮询完成。档位 `free-tier`、`g1-pro-tier`、`g1-ultra-tier` 分别显示为 Free、Pro、Ultra；付费档位优先使用 daily 服务根，其余使用生产服务根，连接失败或 404 时切换到下一地址。loadCodeAssist 的原始结果保存在官方资料快照中，“刷新官方资料”重新执行上述流程。令牌到期前 300 秒自动刷新；刷新返回 `invalid_grant`、`invalid_client` 或 `unauthorized_client` 时记录授权失效，其他失败保留状态并等待下一次尝试。

## 伪装与上游请求

每个请求都像官方桌面客户端：`User-Agent` 为 `antigravity/{版本} {os}/{arch}`，操作系统取 `windows`、`darwin`、`linux`，架构取 `amd64`、`arm64`，由账户冻结输入与公共版本常量在运行时生成，不保存派生 UA，也不使用代理主机信息。生成请求发送到 `v1internal:streamGenerateContent?alt=sse`，信封包含 `project`、`agent-` 前缀的 `requestId`、`userAgent: antigravity`、`requestType: agent`、`model` 与 Gemini `request`。服务根为 `https://cloudcode-pa.googleapis.com` 与 `https://daily-cloudcode-pa.googleapis.com`。

## 协议转换

公共层只处理规范 Responses 请求与事件。`providers/antigravity/request.rs` 把规范 Responses 请求转换为 Gemini 请求：消息映射为 `contents` 的 user／model 角色，包含文本、带 `thoughtSignature` 的思考、`inlineData`、`functionCall` 与 `functionResponse`；system／developer 文本进入 `systemInstruction`，排在客户端身份提示与边界提示之后；`generationConfig` 携带 `maxOutputTokens`（缺省 64000）、temperature、topP、按推理强度映射的 `thinkingConfig`，以及结构化输出的 `responseMimeType`／`responseSchema`；函数工具转换为清理后的 `functionDeclarations`（内联 `$ref`、合并 allOf／anyOf／oneOf、常量转枚举、约束写入描述，只保留 Gemini 接受的字段），`tool_choice` 映射为 `toolConfig.functionCallingConfig`；没有函数工具时才声明 `googleSearch`。`sessionId` 由会话亲和种子派生。

`providers/antigravity/events.rs` 把 Gemini SSE 转换回 Responses 事件：候选项的思考、文本和函数调用依次成为 reasoning、message、function_call 输出项，思考签名随项保留；`finishReason` 的 `STOP` 为 completed，`MAX_TOKENS` 为 incomplete（`max_output_tokens`），安全类原因为 incomplete（`content_filter`），`MALFORMED_FUNCTION_CALL` 为 failed；用量取 `promptTokenCount`（含缓存）、`cachedContentTokenCount`、`candidatesTokenCount` 加 `thoughtsTokenCount`，推理 Token 为 `thoughtsTokenCount`。终态事件在流结束时发出，缺少终态按未完成失败处理。续接 `previous_response_id`、Codex 搜索、压缩、计数、图像和原生 Responses WebSocket 不属于该渠道，明确拒绝。

## 模型、额度与计价

供应账户“可用模型”的“同步上游模型”调用官方 `v1internal:fetchAvailableModels`，按返回的模型标识、显示名称和能力替换该账户的上游模型；同步时把能识别厂商（Gemini、Claude、GPT、Grok）的新模型按厂商标签登记到计费目录，归一化后能匹配统一内置模型表中的模型名时默认启用并带价格，否则保持未定价；识别不出厂商的模型只保留在该账户的可用模型中，需要时在计费配置里手动添加。该表的 Gemini 与 Claude 价格来自 Google 与 Anthropic 的官方公开价格并按长上下文档位分段；GPT-OSS 等没有官方价格的模型保持未定价，不会被当作免费。价格快照在请求开始时冻结，输入、缓存读取、缓存写入、输出各结算一次。

官方额度按模型返回 `quotaInfo.remainingFraction` 与 `resetTime`。供应账户详情的“官方额度”逐模型显示已用比例与重置倒计时，读取既有 PostgreSQL 快照，“从官方刷新”才请求官方。刷新结果同时写入模型冷却：剩余为 0 且重置时间在未来的模型进入冷却，剩余大于 0 的模型立即恢复。列表与定时任务不请求官方。

## 错误处理与调度

| 上游情况 | 处理 |
| --- | --- |
| HTTP 401 | 记录授权失效并切号；成功的重新授权清除失效状态 |
| 429 `RESOURCE_EXHAUSTED`，`RetryInfo.retryDelay` 不少于 7 秒，或消息含 exhausted your capacity／quota exceeded | 记录该模型的冷却直到重置时间并切号；错误未指明模型时按本次请求的模型记录 |
| 其他 429 与短暂等待 | 返回结构化错误与 `Retry-After`，由客户端退避，不改供应状态 |
| 403、400 类请求错误 | 保留错误，不刷新身份，不切号 |
| 5xx、503 容量不足、连接失败或超时 | 保留状态与重试指示，由客户端决定重试，不改供应状态 |

模型冷却保存在 `supplier_model_cooldowns`，以账户、模型和凭据版本为键。号池索引取账户冷却与模型冷却中较晚者，因此同一账户的其他模型继续参与调度；冷却到期自动结束，官方读取报告仍有余量或重新授权立即清除，管理端“重置状态”同样清除。普通限流不写入冷却，也不切换供应账户。供应详情“账户资料”显示当前有效的模型冷却。

## 导入导出

供应导入导出的 Sub2api 格式支持平台 `antigravity`、类型 `oauth`，凭据字段为 `access_token`、`refresh_token`、`expires_at`、`email`、`project_id`、`plan_type`、`token_type`。导入账户独立生成本地指纹，邮箱作为账户主标识；原生导入校验 `antigravity` 凭据格式与账户身份一致。
