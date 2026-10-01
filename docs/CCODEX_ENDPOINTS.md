# ccodex 0.159.3 接口核查

客户端基线：`01fc69f4026735edfdf6789820549727a4867b11`。入口来自 `gucooing/codex` 的 `ccodex` 分支。默认服务根为 `https://oauth-ai.alsl.xyz/api/oauth/chatgpt`；下表路径均相对这个根。

客户端在共享 HTTP request draft 和 WebSocket connector 将第一方 destination 映射到此根，保留 method/path/query/body，OAuth issuer 和 bootstrap URL 也从相同服务解析。供应端仍按官方固定地址、账户身份、指纹和代理访问官方。客户端地址覆盖不代表以下尚未实现的业务已完成。

| 请求组 | 入口与本次结论 | 数据/管理来源 |
| --- | --- | --- |
| 浏览器授权 | GET `/oauth/authorize` → 静态授权页 → bootstrap/submit → 原本地 callback；现有PKCE/state/cookie/CSRF链路保留 | 虚拟用户名/密码、授权码、登录设备 |
| token/refresh/revoke | POST `/oauth/token`、`/oauth/revoke`，JSON及form编码；原有实现复用 | `virtual_devices`与代理签发token，登录设备页可查询/撤销 |
| device code | POST `/api/accounts/deviceauth/usercode` → 浏览器`/codex/device` → POST `/api/accounts/deviceauth/token` → 原`/oauth/token` | 新迁移0048保存15分钟pending授权；授权后120秒PKCE单次code；最终进入同一设备表/管理页 |
| device reader | 官方 `login/src/device_code_auth.rs` 要求device_auth_id、user_code字符串和**字符串interval**；pending为403/404，成功含authorization_code/code_challenge/code_verifier | 实现返回interval="5"、pending403、过期410；保存精确`{service}/deviceauth/callback`供token兑换，此URI不是客户端实际GET调用 |
| Responses/compact/Guardian | `/backend-api/codex/responses`及子路径，HTTP/SSE/WS；原执行/计价/额度链路复用 | 本地实际账本、后台用量和请求记录；供应仅执行 |
| WS interrupt | `response.interrupt`沿原socket透传，不建新账；create及完成usage仍由原逻辑管理 | 已增interrupt不增加generation账本的回归；云端执行结果待记录 |
| models/usage/workspace/profile | 原backend-api路由保留；模型描述符同步官方0.159.3，新增gpt-6.1-sol描述、不自动发放价格/权益 | 管理模型目录/价格/订阅、本地虚拟额度；不继承供应额度或私人资料 |
| 图像/搜索/input_tokens/Realtime | 复用已有显式路由；真实外部结果/用量按既有规则记录 | 原管理模型计价、请求/通话记录；真实上游未在线验证 |
| 配置/会话/通知/插件/MCP/任务 | 复用现有明确实现与权限；详细方法、归属和既有缺口见CODEX_REQUEST_AUDIT | 虚拟state/resources/events及相应管理记录，不把空数组当新能力实现 |
| 文件create/PUT/finalize/download | **按用户最新要求，本次不新增** | 原有未实现状态保留；本次新增的文件接口、迁移、管理页及引用校验均已撤回 |
| 官方购买/云插件执行/远程控制完整产品 | **不因地址路由而自动完成**，原审计中的未实现分支仍明确返回不支持 | 不伪造交易、外部授权或执行成功 |

实际安装Desktop为Windows包`26.928.3736.0`。只读检查其`app.asar`中的`app-shared-7552fc8d5d82.js`等构造：`account/login/start`的ChatGPT分支仍读取login_id/auth_url，默认useHostedLoginSuccessPage=false，并保留`account/chatgptAuthTokens/refresh`与账号/工作区ready分支。检索未发现renderer直接构造上述deviceauth HTTP；设备码wire证据来自0.159.3原生CLI。未执行或修改Desktop，不能将这项源码核查表述为Desktop登录实测成功。只读提取证据在忽略目录`target/codex-audit-20261001/desktop/`。

CLI确实包含MCP文件传递实现：官方固定commit的[core/src/mcp_tool_call.rs](https://github.com/openai/codex/blob/01fc69f4026735edfdf6789820549727a4867b11/codex-rs/core/src/mcp_tool_call.rs)在MCP参数准备阶段调用重写流程，[core/src/mcp_openai_file.rs](https://github.com/openai/codex/blob/01fc69f4026735edfdf6789820549727a4867b11/codex-rs/core/src/mcp_openai_file.rs#L247)调用`upload_openai_file`，具体协议在`codex-api/src/files.rs`。这不是通用CLI上传命令，也不证明所有第三方服务已支持该能力。本轮只修改客户端的服务地址、安装名称/默认目录及发布渠道，保留官方原有能力代码；不新增代理文件业务。

更精确的启用条件是`mcp_tool_call.rs::openai_file_input_optional_fields_for_server`要求server等于`CODEX_APPS_MCP_SERVER_NAME`，且参数表非空；参数表来自工具的`_meta["openai/fileParams"]`。因此这是OpenAI Apps专用约定：先取得托管文件，再把file_id/download_url交给Apps工具。普通第三方MCP不会进入这段自动上传逻辑，可按自己的工具协议传文件；MCP协议不要求经模型提供商存储。
