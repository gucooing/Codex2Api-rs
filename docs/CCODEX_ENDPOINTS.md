# ccodex 服务接口

官方协议基线为 Codex 0.161.0，commit `979011409de0a60b52f179721948e65531d26144`。
客户端维护于 gucooing/codex 的 ccodex 分支。下列路径相对配置的 BASE_OAUTH_URL，
默认服务根为 `https://oauth-ai.alsl.xyz/api/oauth/chatgpt`。

客户端共享 HTTP/WS 出口只映射第一方服务地址，保留方法、路径、查询、PKCE、state 和业务正文。
供应请求继续使用官方地址、供应身份及逐账户指纹。

客户端断开 HTTP 或 WebSocket 连接后，已发往上游的生成继续到终态，并按最终实际用量结算。断连本身不计为平台失败；上游错误、空闲超时与凭据撤销仍按原有规则处理。

服务不采集客户端遥测、Statsig 事件、SDK 异常或指标。原采集地址返回 404；功能配置刷新与公开资源缓存保持独立。

| 请求组 | 合同 | 数据与管理 |
| --- | --- | --- |
| 浏览器授权 | GET /oauth/authorize，授权页 bootstrap/submit，本地 callback | 虚拟用户名/密码、授权码、登录设备 |
| 交换/刷新/撤销 | POST /oauth/token、/oauth/revoke，JSON/form | virtual_devices 和本服务按官方协议签发的客户端 token；与网页登录 JWT 分开 |
| 设备码 | POST /api/accounts/deviceauth/usercode、/token；浏览器 /codex/device | 15 分钟 pending、授权后 120 秒单次 code、原设备管理 |
| 设备码响应 | device_auth_id/user_code/interval 为字符串；pending 403、过期 410；成功返回 PKCE code 参数 | 保存精确的 service/deviceauth/callback 兑换 URI |
| Responses/compact/Guardian | backend-api/codex/responses 及相应子路径，HTTP/SSE/WS | 同一模型授权、实际账本及请求记录 |
| WS interrupt | 原 socket 转发 response.interrupt，保持动态响应 ID | 不创建新生成记录、不自动重连 |
| 模型/额度/工作区/资料 | backend-api 现有明确路由，模型目录只提供虚拟账户允许的已启用模型 | SQLite 模型、预设、套餐、虚拟额度和自有资料 |
| 图像/搜索/input_tokens/Realtime | 现有显式 HTTP/WS 路由 | 实际返回数据、计价和通话归属 |
| 配置/会话/通知/任务 | 按账号归属查询、写入或执行 | virtual state/resources/events 及管理记录 |

Responses 在输出开始前遇到供应授权失效、配额耗尽或 ChatGPT HTTP 402 账单受限时，在标签号池内换号重试；号池耗尽返回 `supplier_pool_exhausted`。402 保留供应凭据并持续退出号池，不作为临时限流或带到期时间的额度冷却。已输出内容、工具事件或网络中断不能盲目重放，保留结构化错误、Retry-After、SSE 未完成原因及 WS close。供应请求限流保留结构化错误代码、状态和重试信息，供应诊断文本不返回用户交给客户端退避，不记录账户状态、不换号。虚拟账户 RPM 拒绝为 429 `virtual_rpm_exceeded`，独立于消费金额额度。供应重新授权成功自动解除旧失败状态，保留管理员启停设置；供应凭据与虚拟凭据刷新分别管理。

文件 create/PUT/finalize/download 及外部插件/MCP/云自动化/购买等尚未实现的能力，
不能通过地址转发、空集合或模拟 success 宣称支持。具体边界见
[架构说明](ARCHITECTURE.md#client-routes-and-capability-boundaries)。

官方 CLI 的 OpenAI Apps 文件传递以工具 _meta.openai/fileParams 为条件，依赖托管文件 API。
普通第三方 MCP 可以使用自己的传输协议；MCP 协议本身不要求把文件上传到模型供应商。

浏览器授权入口在 AI API 端，身份验证和明确确认在独立用户端 `/user/api/oauth/authorize/{bootstrap,identify,approve}`；设备码的浏览器步骤使用 `/user/api/oauth/device/{bootstrap,identify,approve}`。令牌兑换、刷新、撤销和设备码轮询仍在 AI API 端。浏览器已有用户会话时直接展示身份，否则密码只验证当前授权链接。授权表单不提供身份类型选项，用户名和密码经同一身份存储验证。虚拟用户只能授权 OAuth，不能成为用户中心会话；管理和统计仍与普通用户隔离。三端分别监听，AI 凭据不能访问用户网页 API 或管理 API。
