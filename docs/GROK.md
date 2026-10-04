# Grok 渠道

Grok 与 ChatGPT 是独立渠道。客户端参考 [gucooing/grok-build](https://github.com/gucooing/grok-build)，参考提交为 `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`，`SOURCE_REV` 为 `559751fdcec02d413e4c57c8832ab275e4f44980`，协议版本为 `1.0.45`。客户端源码仅供参考，不作为 Rust 依赖。

## 目录与账户隔离

各 crate 的 `src/providers/grok/` 保存 Grok 身份、认证、请求、响应和管理实现；`src/providers/chatgpt/` 保存 ChatGPT 实现。入口模块仅负责渠道分发。数据库、供应池、请求账本、模型计价、用户钱包和订阅事务是公共基础能力。`scripts/check-provider-boundaries.py` 检查跨渠道导入。

Grok 供应凭据使用独立的 `GrokCredentials`，不会通过 Codex `auth.json` 解析。每个供应账户在 SQLite 内保存独立身份、安装 UUID、冻结的设备资料、凭据版本和代理；HTTP 连接与刷新锁按账户隔离。供应表中的 `chatgpt_account_id` / `chatgpt_user_id` 是兼容既有数据库的列名，Grok 分别保存带个人／团队前缀的主体及官方用户编号，不跨提供商合并。

## 供应授权和资料

供应授权提供代码／回调、设备码和 RT 三种方式。代码授权同时接受官方页面的纯授权代码和完整回调 URL。URL 必须符合本次请求的地址和 state；两种输入都使用本次 PKCE verifier，并验证 ID token 的签名、issuer、audience、有效期与 nonce。设备码遵循轮询间隔和 `slow_down`。RT 通过 xAI `/oauth2/token` 交换，使用官方 `/v1/user` 验证主体后才保存。

RT 创建一行一个，每批最多 50 条；页面依次提交并显示每行的真实结果。批量新账户独立生成设备指纹和安装 UUID，沿用选定代理和时区。同一提供商、用户和主体的重新授权复用原账户与指纹。失败项可重试，已完成项不重复提交；页面结果只含行号和账户链接，不显示凭据。

官方供应地址是 `https://cli-chat-proxy.grok.com/v1`，issuer 是 `https://auth.x.ai`。请求使用 Grok Build UA、`X-XAI-Token-Auth: xai-grok-cli`、`x-grok-client-version` 和供应身份，不携带 Codex 头或客户端账户凭据。刷新按凭据版本比较更新，迟到结果不能覆盖新授权。

供应列表和详情以邮箱作为主标识。订阅名称优先采用 `/settings` 的 `subscription_tier_display`；缺失时才将 `/user?include=subscription` 的内部档位转换为官方名称，不会把缺失值猜成付费订阅。资料保存在账户和官方快照中，旧账户可以通过“刷新官方资料”更新。额度从 `/billing?format=credits` 的实际周期与百分比生成紧凑摘要，继续使用既有 SQLite 缓存。

## 模型和计价

管理端模型列表是模型可用性的唯一配置来源。模型启用且在套餐授权范围内即可出现在客户端列表并发送请求，无内置模型白名单，不区分“已验证模型”和“自定义模型”。官方目录只用于查询、导入和补充描述；客户端内置的两个离线模型不代表实际可用模型集合。

“模型配置”的价格预设来自 [xAI 官方计价](https://docs.x.ai/developers/pricing)。已核实的文本预设覆盖 Grok 4.7、4.6、4.5、4.3、grok-build-0.1 以及列出的 4.20 模型；200,000 输入 Token 起使用官方长上下文价格。预设不推断未公布档位、不改已有人工价格，管理员可以添加任意有效模型名并设置价格。输入、缓存读取、缓存写入、输出按请求开始时的价格快照结算，推理不重复计费。未知费用保持未知，有额度限制时不能把缺价当免费。

Grok 套餐独立选择 Grok 订阅档位、模型范围、售价、供应池及 7 天／30 天外层和可选 5 小时内层额度。新旧用户各有独立 Grok Free 身份；管理员授予、钱包预览／确认／支付、到期回落、用量和设备撤销共用同一持久化事实，不能跨平台绑定供应或模型。

订阅选项对应 Grok Build 的完整已知档位：Free、X Basic、X Premium、X Premium+、SuperGrok Lite、SuperGrok、SuperGrok Plus、SuperGrok Heavy。客户端 `/settings` 使用官方展示名，不使用管理员自定义的套餐名称；`/user.subscriptionTier` 和访问令牌的数字 `tier` 分别遵循客户端枚举。例如 `GrokPro` 对应 SuperGrok，`SuperGrokPro` 对应 SuperGrok Heavy。旧套餐和订单历史保留；旧 `premium`、`premium_plus`、`supergrok_pro` 值按对应档位读取，原有 `team` 不冒充任何官方档位，也不再作为新增选项。

## cgrok 接入

默认服务根与 ccodex 同域，为 `https://oauth-ai.alsl.xyz/api/oauth/grok`。服务同时保留 `/grok` 别名；发现文档和 JWT issuer 跟随实际使用的入口。

| 路径（相对于服务根） | 行为 |
| --- | --- |
| `/.well-known/openid-configuration`、`/.well-known/jwks.json` | 本服务 OIDC 发现和独立签名公钥 |
| `/oauth2/authorize`、`/oauth2/device/code` | 浏览器 PKCE 或设备授权；身份验证与确认在用户端完成 |
| `/oauth2/token`、`/oauth2/revoke`、`/oauth2/userinfo` | 本地令牌交换、刷新轮换、撤销和本人资料 |
| `/v1/models` | 已配置、启用且属于本人套餐的模型；返回 `data` 数组 |
| `/v1/responses`、`/v1/chat/completions`、`/v1/messages` | Grok 原生推理、流式工具事件及实际用量记录 |
| `/v1/user`、`/v1/settings`、`/v1/billing` | 本地虚拟身份、套餐服务策略和本地额度，不读取供应隐私 |

通用 `/v1/responses`、`/v1/models` 根据平台凭据分发；Grok 凭据不能访问 ChatGPT 专属路由。Grok Build 配置使用 `endpoints.cli_chat_proxy_base_url` 指向服务根后的 `/v1`，OAuth issuer 指向服务根本身。不要用 `models_base_url` 代替，因为客户端会把它解释为 API-key 认证的独立模型服务。

普通 429 保留结构化错误及重试信息，不改变供应状态；明确授权失效和额度耗尽才切换同平台号池。开始输出后不重放请求。客户端停止后仍收集已报告用量并结算一次。Grok 原生请求携带完整会话输入；供应端保存的 `previous_response_id` 不能跨虚拟账户读取。

远程沙箱、relay、工作区云同步及官方充值等服务没有被本地渠道伪造实现；未实现路由返回明确错误，不返回假空记录或假成功。真实上游推理是否可用取决于所绑定供应账户的官方权限。
