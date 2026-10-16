# OAuth 客户端接口

OAuth 模块 `codex2api-oauth` 专门服务配置了本服务地址的客户端，包括 ccodex、cgrok 和通过 C# 启动器接入的 Codex Desktop。它负责客户端登录、刷新、撤销、推理、账户、订阅额度和客户端状态协议。请求绑定用户明确选择的订阅，按订阅权益和额度执行。

标准 AI API 网关由 `codex2api-api` 提供，使用 API Key、API 组及用户钱包，接口见 [AI API 网关](API.md)。两个模块只共享存储、供应传输和调度基础设施，不相互调用协议处理器。

| 项目 | OAuth 客户端 | AI API 网关 |
| --- | --- | --- |
| 默认监听地址 | `127.0.0.1:8080` | `127.0.0.1:8083` |
| 监听配置 | `CODEX2API_OAUTH_BIND` | `CODEX2API_API_BIND` |
| 对外 origin | `CODEX2API_PUBLIC_OAUTH_URL` | `CODEX2API_PUBLIC_API_URL` |
| 管理设置字段 | `oauth_url` | `api_url` |
| 服务根 | `/v1/chatgpt`、`/v1/grok` | `/v1` |
| 凭据 | 平台订阅 OAuth access token | 用户 API Key |

管理端“设置 → 访问地址”保存完整 origin（协议、域名和可选端口），不填写路径。保存的值优先于环境变量并立即生效。用户 API Key 页只读取 `api_url`；OAuth 发现、客户端事件地址和授权链接只读取 `oauth_url`。

“设置 → 转发与客户端”中的号池调度供 OAuth 和 API 共用；“OAuth 客户端规则”中的 User-Agent 黑白名单只检查 OAuth 推理请求。标准 API 的调用限制由 API 组、API Key 和用户钱包策略负责。

## 平台路由

ChatGPT 服务根为 `{oauth_url}/v1/chatgpt`。ccodex 的 `BASE_OAUTH_URL` 和 Desktop 启动器服务器地址都指向此根。启动器输入纯 origin 时自动补上 `/v1/chatgpt`。客户端继续使用原协议的相对路径：

- `/oauth/authorize`、`/oauth/token`、`/oauth/revoke`。
- `/api/accounts/deviceauth/usercode`、`/api/accounts/deviceauth/token`、`/codex/device`。
- `/codex/desktop-auth`，包括嵌套授权地址中的 PKCE、state 和 callback。
- `/backend-api/codex/responses`、`/backend-api/codex/models`、`/backend-api/wham/usage` 等客户端路径。
- `/v1/responses` 等客户端原有平台 API 路径；完整路径为 `/v1/chatgpt/v1/responses`。

Grok 服务根为 `{oauth_url}/v1/grok`，cgrok 应配置这个服务根。发现地址为 `/v1/grok/.well-known/openid-configuration`，issuer 固定为当前 OAuth origin 加 `/v1/grok`。登录与刷新使用该根下的 `/oauth2/*`，推理使用 `/v1/grok/v1/responses` 等原协议路径。完整契约见 [Grok 渠道](GROK.md)。

浏览器身份验证和确认在用户网站的 `/user/api/oauth/*`，属于用户网站 API。供应账户授权由管理端和 `codex2api-suppliers` 负责。

同域反向代理可把 `/v1/chatgpt`、`/v1/grok` 及其子路径原样转发到 OAuth 监听器，其余标准 `/v1/*` 转发到 API 监听器；`/admin`、`/user` 分别转发到管理端和用户端。也可使用独立域名。OAuth 和 API 都需要支持 WebSocket 与不缓冲的 SSE。`/healthz` 分别在两个监听器上提供健康检查。

