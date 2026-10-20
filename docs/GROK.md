# Grok 渠道

Grok 与 ChatGPT 是独立渠道。客户端参考 [gucooing/grok-build](https://github.com/gucooing/grok-build)，参考提交为 `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`，`SOURCE_REV` 为 `559751fdcec02d413e4c57c8832ab275e4f44980`，协议版本为 `1.0.45`。客户端源码仅供参考，不作为 Rust 依赖。维护客户端的当前路由集成提交为 `b5cf836c76ebc927dee84e74dfe7d2729f55e1f0`；它只修改 fork 接入地址，不替换上述供应协议来源。

## 目录与账户隔离

各 crate 的 `src/providers/grok/` 保存 Grok 身份、认证、请求、响应和管理实现；`src/providers/chatgpt/` 保存 ChatGPT 实现。入口模块仅负责渠道分发。数据库、供应池、请求账本、模型计价、用户钱包和订阅事务是公共基础能力。`scripts/check-module-boundaries.py` 检查跨渠道导入。

Grok 供应凭据使用独立的 `GrokCredentials`，不会通过 Codex `auth.json` 解析。每个供应账户在 PostgreSQL 内保存独立身份、安装 UUID、冻结的设备资料、凭据版本和代理；HTTP 连接与刷新锁按账户隔离。Grok 供应身份由带个人／团队前缀的主体及官方用户编号标识，不跨提供商合并。

## 供应授权和资料

供应授权提供代码／回调、设备码和 RT 三种方式。代码授权同时接受官方页面的纯授权代码和完整回调 URL。URL 必须符合本次请求的地址和 state；两种输入都使用本次 PKCE verifier，并验证 ID token 的签名、issuer、audience、有效期与 nonce。设备码遵循轮询间隔和 `slow_down`。RT 通过 xAI `/oauth2/token` 交换，使用官方 `/v1/user` 验证主体后才保存。

RT 创建一行一个，每批最多 50 条；页面依次提交并显示每行的真实结果。批量新账户独立生成设备指纹和安装 UUID，沿用选定代理和时区。同一提供商、用户和主体的重新授权复用原账户与指纹。失败项可重试，已完成项不重复提交；页面结果只含行号和账户链接，不显示凭据。

官方供应地址是 `https://cli-chat-proxy.grok.com/v1`，issuer 是 `https://auth.x.ai`。请求使用 Grok Build UA、`X-XAI-Token-Auth: xai-grok-cli`、`x-grok-client-version` 和供应身份，不携带 Codex 头或客户端账户凭据。刷新按凭据版本比较更新，迟到结果不能覆盖新授权。

供应列表和详情以邮箱作为主标识。订阅名称优先采用 `/settings` 的 `subscription_tier_display`；缺失时才将 `/user?include=subscription` 的内部档位转换为官方名称，不会把缺失值猜成付费订阅。资料保存在账户和官方快照中，旧账户可以通过“刷新官方资料”更新。额度从 `/billing?format=credits` 的实际周期与百分比生成紧凑摘要，继续使用既有 PostgreSQL 缓存。

## 模型和计价

管理端计费模型目录决定客户端发布和套餐授权，供应账户的“可用模型”决定该账号能否承接请求。调度只选择同平台、同号池且已添加请求模型的供应账号；列表为空不接受生成。供应账户详情的“可用模型”管理该账户的模型。同步调用该账号的 `/v1/models`，只更新该账号的上游模型及安全描述；手动模型只能从已存在的计费配置选择，允许未启用或未定价。同步、添加和移除供应模型都不修改计费模型或价格；同步保留手动添加项。客户端仍能使用管理员添加的自定义模型，没有内置模型白名单。

“模型配置”的价格预设来自 [xAI 官方计价](https://docs.x.ai/developers/pricing)。已核实的文本预设覆盖 Grok 4.7、4.6、4.5、4.3、grok-build-0.1 以及列出的 4.20 模型；200,000 输入 Token 起使用官方长上下文价格。预设不推断未公布档位、不改已有人工价格，管理员可以添加任意有效模型名并设置价格。输入、缓存读取、缓存写入、输出按请求开始时的价格快照结算，推理不重复计费。未知费用保持未知，有额度限制时不能把缺价当免费。

Grok 套餐独立选择 Grok 订阅档位、模型范围、售价、供应池及 7 天／30 天外层和可选 5 小时内层额度，RPM 与并发限制默认分别为 40 和 10。用户默认无订阅，同一用户可持有多个 Grok 订阅，OAuth 确认时选择具体订阅。管理员发放与钱包购买新增独立订阅；续费、升级、用量和设备撤销按订阅 ID 管理。到期保留记录并停止执行权益，不创建或回退到 Free，不能跨平台绑定供应或模型。

本地套餐的订阅选项为 X Basic、X Premium、X Premium+、SuperGrok Lite、SuperGrok、SuperGrok Plus、SuperGrok Heavy；供应账户仍可显示官方返回的 Free 档位。客户端 `/settings` 使用官方展示名，不使用管理员自定义的套餐名称；`/user.subscriptionTier` 和访问令牌的数字 `tier` 分别遵循客户端枚举。例如 `GrokPro` 对应 SuperGrok，`SuperGrokPro` 对应 SuperGrok Heavy。本地配置只接受上述档位的规范 ID。

## cgrok 接入

服务根为 OAuth origin 加 `/v1/grok`，例如 `https://oauth-ai.alsl.xyz/v1/grok`。发现文档和 JWT issuer 使用这一根地址。cgrok 应显式配置服务根。

服务端在管理端“设置 → 访问地址”保存 OAuth 客户端和用户端的完整基础地址；协议和端口均使用管理员填写的值。OAuth 基础地址用于发现文档中的所有端点及令牌 issuer，用户端基础地址用于浏览器授权和设备码确认页。保存立即生效并优先于启动配置，不受反向代理内部监听地址或转发头影响。客户端从发现文档读取登录地址，因此修正服务端地址后重新运行 `cgrok login` 即可，已配置新服务根的客户端无需重新编译。

| 路径（相对于服务根） | 行为 |
| --- | --- |
| `/.well-known/openid-configuration`、`/.well-known/jwks.json` | 本服务 OIDC 发现和独立签名公钥 |
| `/oauth2/authorize`、`/oauth2/device/code` | 浏览器 PKCE 或设备授权；身份验证与确认在用户端完成 |
| `/oauth2/token`、`/oauth2/revoke`、`/oauth2/userinfo` | 本地令牌交换、刷新轮换、撤销和本人资料 |
| `/v1/models` | 已配置、启用且属于本人套餐的模型；返回 `data` 数组 |
| `/v1/responses`、`/v1/chat/completions`、`/v1/messages` | Grok 原生推理、流式工具事件及实际用量记录 |
| `/v1/user`、`/v1/settings`、`/v1/billing` | 本地虚拟身份、套餐服务策略和本地额度，不读取供应隐私 |

平台由路径明确选择，Grok 客户端使用 `/v1/grok/v1/responses`、`/v1/grok/v1/models`；Grok 凭据不能访问 ChatGPT 专属路由。Grok Build 配置使用 `endpoints.cli_chat_proxy_base_url` 指向服务根后的 `/v1`，OAuth issuer 指向服务根本身。不要用 `models_base_url` 代替，因为客户端会把它解释为 API-key 认证的独立模型服务。

cgrok 的 API-key 请求使用独立 API 网关 `{api_url}/v1`，通过 `endpoints.xai_api_base_url` 或 `CGROK_XAI_API_BASE_URL` 配置，不指向 OAuth 服务根。维护客户端的默认值由 `xai-grok-env` 统一提供。供应 UA 使用本渠道公共版本与账户冻结输入运行时生成，不在供应账户中保存完整 UA。

Grok 原生请求携带完整会话输入；供应端保存的 `previous_response_id` 不能跨虚拟账户读取。

## 错误处理和供应调度

Grok 的错误解析和调度策略位于 `codex2api-upstream/src/providers/grok/`，OAuth 与标准 API 使用同一分类和流式前导判断。协议依据是固定参考提交中的 [provider_error.rs](https://github.com/gucooing/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-sampling-types/src/provider_error.rs)、[sampling-types/error.rs](https://github.com/gucooing/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-sampling-types/src/error.rs)、[sampler/retry.rs](https://github.com/gucooing/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-sampler/src/retry.rs) 和 [shell/sampling/error.rs](https://github.com/gucooing/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-shell/src/sampling/error.rs)。客户端的单账号退避与代理的供应池切换分开处理。

| 上游情况 | 处理 |
| --- | --- |
| HTTP 或流内 401 | 当前账号最多刷新一次并重试；刷新成功后仍被拒绝、缺少 RT，或刷新返回 `invalid_grant` / `invalid_client` 时永久回避（授权失效）并换号 |
| 刷新服务限流、5xx、未知 OAuth 错误 | 不据此判定凭据永久失效；本次请求回避并换号 |
| `subscription:free-usage-exhausted` | 额度回避到 reset 时间并换号；支持平面、嵌套、WKE 和 JSON 字符串包装 |
| 429 中明确的 “You've used all the included free usage for model … for now. Usage resets over a rolling …” | 无语义错误码时也识别为免费额度耗尽；仅出现 quota、upgrade 或普通限流字样不足以判定额度耗尽 |
| `personal-team-blocked:spending-limit`、`insufficient_quota`、`insufficient_credits`、`billing_error` | 额度回避到 reset 时间并换号 |
| HTTP 402 | 永久回避（账单受限）并换号 |
| 普通 429、`rate_limit_exceeded`、`slow_down`、未分类的 429 | 限流回避到 `Retry-After`／重试秒数或毫秒数（缺省 60 秒）并换号，不转发给客户端 |
| 403 内容策略或安全拒绝 | 原样返回调用者，不改账户状态 |
| 403 账号被停用、封禁或终止 | 永久回避（授权失效）并换号 |
| 其他 403 | 回避 10 分钟并换号 |
| 404 模型不存在 | 回避该模型 1 小时并换号 |
| 400/413/415/422 请求、图片或上下文错误 | 原样返回调用者，不刷新身份、不改请求内容、不换号 |
| 529 过载 | 回避 10 分钟并换号 |
| 5xx、Cloudflare 52x/530 | 仅本次请求回避并换号；HTML 页面不作为额度耗尽证据 |
| 连接失败 | 回避 10 分钟并换号；超时仅本次请求回避；流在首个输出前结束同号重试 1 次 |

错误处理保留官方重试指示，包括 `x-should-retry: false`，但调用者从不收到供应方的限流或授权错误：候选耗尽时收到 503 `no_available_supplier`。Grok 的裸 HTTP 402 按账单受限永久回避，与 ChatGPT 的分类彼此独立实现。供应隐私和任意诊断文本只进管理端账本。

额度回避使用实际返回的 reset 时间、Retry-After、重试秒数／毫秒数或 “rolling N hour” 说明中的时长；没有明确时间时回避 60 秒后允许下一次真实请求尝试。不将 `/billing` 的月度余额当成特定模型免费额度已经恢复的证据。回避写入 `supplier_blocks` 并使候选池缓存失效，管理端供应状态显示“配额耗尽”“限流中”等状态。管理端重置和重新授权沿用现有操作；迟到失败不得覆盖新授权。

仅在没有生成内容或计费用量时换号。Responses 输出、Chat 的文本／推理／工具调用、Messages 的内容或用量一旦出现，就不重放；HTTP 错误体中的部分输出和用量同样生效。流中途失败只更新账号的回避状态，当前生成以结构化错误结束。
