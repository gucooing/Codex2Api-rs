# ccodex 服务接口

官方协议基线为 Codex 0.160.0，commit `a956835d020762cb2b570053af06f643a11c0ecc`。
客户端维护于 gucooing/codex 的 ccodex 分支。下列路径相对配置的 BASE_OAUTH_URL，
默认服务根为 `https://oauth-ai.alsl.xyz/api/oauth/chatgpt`。

客户端共享 HTTP/WS 出口只映射第一方服务地址，保留方法、路径、查询、PKCE、state 和业务正文。
供应请求继续使用官方地址、供应身份及逐账户指纹。

| 请求组 | 合同 | 数据与管理 |
| --- | --- | --- |
| 浏览器授权 | GET /oauth/authorize，授权页 bootstrap/submit，本地 callback | 虚拟用户名/密码、授权码、登录设备 |
| 交换/刷新/撤销 | POST /oauth/token、/oauth/revoke，JSON/form | virtual_devices 和本服务签发的 token |
| 设备码 | POST /api/accounts/deviceauth/usercode、/token；浏览器 /codex/device | 15 分钟 pending、授权后 120 秒单次 code、原设备管理 |
| 设备码响应 | device_auth_id/user_code/interval 为字符串；pending 403、过期 410；成功返回 PKCE code 参数 | 保存精确的 service/deviceauth/callback 兑换 URI |
| Responses/compact/Guardian | backend-api/codex/responses 及相应子路径，HTTP/SSE/WS | 同一模型授权、实际账本及请求记录 |
| WS interrupt | 原 socket 转发 response.interrupt，保持动态响应 ID | 不创建新生成记录、不自动重连 |
| 模型/额度/工作区/资料 | backend-api 现有明确路由，模型目录只提供虚拟账户允许的已启用模型 | SQLite 模型、预设、套餐、虚拟额度和自有资料 |
| 图像/搜索/input_tokens/Realtime | 现有显式 HTTP/WS 路由 | 实际返回数据、计价和通话归属 |
| 配置/会话/通知/任务 | 按账号归属查询、写入或执行 | virtual state/resources/events 及管理记录 |

重试、断线恢复和排队消息恢复由 CLI 决定。代理保留结构化错误、Retry-After、SSE 未完成原因及
WS close，不叠加生成重试。供应 token 的一次 401 恢复与客户端虚拟凭据刷新分别管理。

文件 create/PUT/finalize/download 及外部插件/MCP/云自动化/购买等尚未实现的能力，
不能通过地址转发、空集合或模拟 success 宣称支持。具体边界见
[架构说明](ARCHITECTURE.md#client-routes-and-capability-boundaries)。

官方 CLI 的 OpenAI Apps 文件传递以工具 _meta.openai/fileParams 为条件，依赖托管文件 API。
普通第三方 MCP 可以使用自己的传输协议；MCP 协议本身不要求把文件上传到模型供应商。
