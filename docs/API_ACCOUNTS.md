# API 渠道

API 渠道把任意以 Key 认证的 OpenAI Responses、Chat Completions 或 Anthropic Messages 端点接入供应池。它只服务标准 AI API 网关，不提供 OAuth 客户端服务根、套餐、订阅或客户端状态协议。常量位于 `crates/codex2api-common/src/providers/api/`：接受的线格式 `openai_responses`、`chat_completions`、`messages`；每种线格式接受的认证头方式——OpenAI Responses 与 Chat Completions 只有 `bearer`（`Authorization: Bearer`），Anthropic Messages 可选 `x_api_key`（`x-api-key`）或 `bearer`，Messages 请求始终携带 `anthropic-version: 2023-06-01`；同号重试次数上限；以及固定的 `User-Agent: codex2api/{版本}`。

## 目录与账户隔离

各 crate 的 `src/providers/api/` 保存凭据、隔离 HTTP 客户端、传输、裁决与管理实现；前端表单位于 `frontend/src/components/providers/api/` 与 `frontend/src/lib/providers/api/`。入口模块仅按渠道分发，`scripts/check-module-boundaries.py` 检查四个渠道之间不互相导入。

每个账户保存一个名称、一个 Key、一个 Base URL、一种线格式、一种认证头方式和同号重试配置。名称是 API 账户唯一的账户名（登录渠道使用邮箱），保存在 `supplier_accounts.display_name`，1 到 128 个字符。凭据以 `auth_mode='api'` 写入 `supplier_tokens`（`access_token` 为 Key，`raw_auth_json` 为 `{api_key, base_url, wire, auth_scheme, retry_statuses, retry_count}`）。账户身份是 Base URL 与 Key 的 SHA-256 指纹：同一 Base URL 下再次创建同一 Key 时更新原账户。API 账户没有设备指纹，`supplier_accounts` 的 `os_type`、`os_version`、`arch` 为 NULL（其他渠道非空，由数据库约束保证），安装 UUID 仍独立生成。每个账户拥有自己的 reqwest 客户端与出站代理，连接池不跨账户共享。

## 创建与管理

“添加供应账户”选择提供商 API 后分三步：

1. 接入配置：名称、线格式、认证头（选项随线格式变化）、Base URL（选择线格式时自动填入官方地址，可改为任何兼容端点）、同号重试状态码与次数、出站代理，以及该账户的 API Key。一次创建一个账户。
2. 模型列表：“获取上游模型”用当前配置与 Key 读取 Base URL 下的 `models` 列表（`POST /admin/api/suppliers/api/accounts/models`，`{fingerprint: 配置, api_key}`，返回 `{items: [模型名]}`，不保存任何内容）并加入列表；也可从计费目录选择或直接输入模型名。至少一个模型。
3. 确认：展示全部配置、隐藏的 Key 与模型列表，确认后 `POST /admin/api/suppliers/api/accounts/start`（`{fingerprint: 配置, api_key, models}`）在同一事务中保存账户、凭据与模型列表。

配置对象为 `{name, base_url, wire, auth_scheme, retry_statuses, retry_count, proxy_id}`，`GET …/setup` 返回表单默认值。账户详情“账户资料”提供 API Key 编辑与保存（`POST /admin/api/suppliers/{id}/relogin`，`{api_key}`），名称、配置、账户 ID、标签、调度与可用模型保持。创建、确认和详情中的 Key 默认按密码隐藏，眼睛按钮切换显隐；详情通过管理员专用的 `GET /admin/api/suppliers/api/accounts/{id}/key` 读取当前 Key，响应禁止缓存，不写入浏览器持久存储，上游错误信息中的 Key 仍被隐去。“接入配置”页签编辑名称与接入配置（`PUT /admin/api/suppliers/{id}/fingerprint`，配置对象）；同号重试状态码输入一个 400–599 的数字后按回车添加为标签，支持移除并防止重复，确认页同样以中性标签展示。“可用模型”的“同步上游模型”读取 `models` 列表并替换该账户的上游模型；自定义模型照常添加。该渠道没有官方额度或资料页，账户资料不展示回避原因、账户回避至、模型回避、上游订阅及套餐到期字段。

## 请求与转换

执行器选中 API 账户后，按其线格式把下游请求直接转换为该端点的官方请求：`convert/responses/`（OpenAI Responses 方言）、`convert/chat_completions/` 与 `convert/messages/` 分别包含从 Responses、Chat、Messages 三种下游格式出发的请求构造与返回事件转换。请求始终流式（Chat Completions 带 `stream_options.include_usage`），同步调用由网关聚合。推理文本、工具调用、工具结果、多模态内容、`max_tokens`/`max_completion_tokens`、推理强度与思考预算、停止序列和结构化输出按各协议的对应字段映射；带签名的 thinking 只在目标支持时回放。请求发往 `{Base URL}/responses`、`{Base URL}/chat/completions` 或 `{Base URL}/messages`，只携带固定 UA、内容类型与认证头，不携带调用者的 Cookie、Key 或身份头。

Chat Completions 的结束原因后仍继续读取最终用量，Messages 读取至 `message_stop`。转换器产生完整终止事件、账本完成结算后才向调用者释放成功结果；缺少必要结束信号的流不得视为成功。

## 错误处理与调度

裁决位于 `codex2api-upstream/src/providers/api/verdict.rs`，依据 sub2api 对 OpenAI 与 Anthropic 接口的切换策略：

| 上游情况 | 处理 |
| --- | --- |
| 401 | 永久回避（授权失效），Key 保留，等待管理员替换 |
| 402 | 永久回避（账单受限） |
| 403 | 回避 10 分钟并换号 |
| 404、400 中说明模型不存在 | 回避该模型 1 小时并换号 |
| 404／405 端点不存在 | 回避 10 分钟并换号（Base URL 配置错误） |
| 429 `insufficient_quota` 等额度耗尽、消息说明额度用尽或余额不足 | 回避到 `Retry-After` 或 1 小时并换号 |
| 其他 429 | 回避到 `Retry-After` 或 60 秒并换号 |
| 400 “credit balance is too low”（Anthropic） | 账单回避 1 小时并换号 |
| 529、503 `overloaded_error` | 过载回避 10 分钟／60 秒并换号 |
| 其他 400 类请求错误 | 原样返回调用者，不改账户状态 |
| 5xx、连接失败 | 仅本次请求回避并换号；连接失败回避 10 分钟 |
| 账户配置的同号重试状态码 | 先在本账户按配置次数重试（1 秒起指数退避，最长 8 秒），再按上述规则处理 |

同号重试只属于 API 账户的配置；一次请求最多换号几次由设置中的“请求重试次数”决定，所有渠道共用。候选耗尽或次数用尽时调用者收到 503 `no_available_supplier`；上游 429 从不转发。回避状态保存在 `supplier_blocks`，到期、重新录入 Key 或管理端“重置状态”结束。

## 导入导出

Sub2api 格式支持类型 `apikey`、平台 `openai`（`api_protocol` 为 `chat_completions` 时导入为 Chat Completions，否则为 OpenAI Responses）与 `anthropic`（Messages），凭据字段为 `api_key`、`base_url`、`api_protocol`，账户名称取 sub2api 的 `name`，同号重试使用默认次数且不配置状态码。导出时按线格式写出相同字段；其他平台的 `apikey` 行跳过。原生导入校验 `api` 凭据格式与账户名称，并按 Base URL 与 Key 指纹重建账户身份。
