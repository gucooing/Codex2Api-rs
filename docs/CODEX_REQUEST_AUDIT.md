# Codex 0.157.0 请求与动作核查结果

2026-09-25；按用户已批准的更新方案执行。这里区分源码核对、本地服务/传输回归、实际客户端代码执行与在线业务。**没有把所有请求都返回 200 或静态字符串命中当作与官方完全一致。**

固定源码：[`00c972ed5d6ff6499317fd41b7f23605b8e6850d`](https://github.com/openai/codex/tree/00c972ed5d6ff6499317fd41b7f23605b8e6850d)，release `0.157.0`。当前安装 Desktop 的 Appx 为 `26.917.9434.0`，package.json 为 `26.917.71314`，内置 native 为 `0.155.0-alpha.16.4`。它与新 CLI 分开验证。

## 已修复的差异

1. 供应 `workspace_backend_origin=NO_CONSTRAINT` 现在按官方语义采用固定 bootstrap origin；覆盖 us/us_cr、显式 HTTPS origin、非法值、旧持久快照和凭据变化。无需清空快照或重新登录。
2. 虚拟 WHAM accounts 统一返回 NO_CONSTRAINT，去掉按 UA 返回 chatgpt.com 的分支；native 不会因服务器发现响应被重新路由到官方域名。**原生工作区发现要求配置 HTTPS 地址；PUBLIC_BASE_URL 不是 TLS 开关。**
3. WS 握手错误保留状态、完整正文及允许传播的错误/重试/请求 ID 响应头，401 恢复失败保留原错误；日志摘要与协议正文分开，长中文/emoji JSON 不再被字节截断破坏。
4. 模型描述同步目标 release，移除旧 GPT-5.6 Sol 的 ultrafast 宣传等 6 个对象差异；SQLite 本地模型权限/价格/历史结算独立保留。
5. 增加 POST WHAM realtime/calls，按真实 Desktop 构造验证 SDP 与 Location，成功响应正文读完后保存 call 归属；管理“客户端记录”增加只读通话创建记录。
6. 当前安装包的 launcher 合约提取及模型 picker 测试更新；生产 launcher 不修改客户端业务/权限/默认 profile。

## 已确认的剩余差异

- 文件 create/upload/finalize/download 完整链路尚未实现（方案 A06）；新文件重试/300s/512MiB 行为也不能标为已对齐。
- 供应健康策略仍对瞬时失败记录持久异常，直到管理员恢复；与官方客户端重试存在产品差异（A07，本次未批准改变）。
- 虚拟插件/连接器授权执行器、云插件安装/卸载、云自动化调度、付款/邀请发送等外部业务没有完整实现；明确返回未实现或只读取自身记录。
- 官方新 application network policy 可撤销在途 HTTP/WS；本项目没有完整等价机制。WS 在请求/消息边界检查，HTTP/SSE 已开始响应没有相同即时撤销机制。保留为未对齐项，不声称全行为一致。
- 部分公共 Desktop 服务失败仍被转成 provider_request_failed/502；历史采集/自然月 cap/线程金额估算能力不完整。它们是既有边界，已列在每条记录中。
- 当前 Desktop 旧 Statsig/家庭/Quota 等若干专用提取脚本仍绑定旧压缩符号；本次修改的 workspace/model/realtime/launcher readers 已更新并执行。其它 GUI 全动作由用户临时启动手测，不能把未运行脚本计为通过。

## 证据组

| 组 | 核对的契约与动作 | 实现及本次运行证据 |
| --- | --- | --- |
| F01 | authorize/token/revoke、供应设备码、PKCE/state/redirect、授权码消费与刷新/注销；客户端浏览器登录与供应设备码是不同能力 | `codex2api-auth/tests/http_protocol.rs`、`manual_login.rs`、`proxy_routing.rs`；API `browser_login_identity_quota_refresh_devices_and_diagnostics`、`rejects_csrf_bad_password_redirects_and_pkce_replay`、`authorization_codes_expire_bind_client_and_callback_and_redeem_atomically` 均通过。虚拟设备码服务仍未实现。 |
| F02 | WHAM accounts array / ChatGPT accounts map、default ID、NO_CONSTRAINT、bootstrap 身份/配置；供应路由快照/revision/重新授权 | `routing.rs` 的矩阵和持久化测试、Admin `workspace_details_are_local_readonly_and_show_credential_invalidation` 通过。`updated_workspace_quota_and_catalog_are_accepted_by_the_actual_native_client` 的服务实际响应分别交两版 native；当前 Desktop `$Te` 也直接执行。Statsig 旧 bundle 专用脚本未全面移植。 |
| F03 | Codex capability descriptors、ChatGPT 模型选项、套餐过滤、empty/nonempty、默认模型、reasoning/service tiers | 精确复制 TARGET models.json；6 个对象更新。`global_catalog_and_desktop_picker_agree_when_legacy_groups_omit_new_models`、模型准入/管理契约通过；当前 renderer `yVt` 和 `YVt` 直接执行，不用手写宽松 schema。官方目录不发放本地权益或价格。 |
| F04 | POST/WS Responses、reviewer/classifier、compact、session/thread/turn/request IDs、zstd、WS 压缩、增量、warmup、错误/fallback/cancel、单次结算 | 上游 40 项与 API websocket/usage/额度测试通过。新版本 native 实际发出 generate=false 和 generation；响应来自本地 fixture。WS 错误保留 code/headers、正确 close。两版 native 不是完整 Desktop GUI，也不是在线官方模型推理。 |
| F05 | input_tokens、memory、允许的 responses 子路径、请求方法、身份替换及模型准入 | `endpoint.rs`、`compat.rs::responses_subpath_url` 与 API 模型准入检查；无实际在线计数/记忆压缩结果。拒绝非法子路径/查询；不把其他 API 的鉴权行为臆测成已实测。 |
| F06 | alpha/search 与 image JSON、image turn ID、imagegen/x-request-id、非成功状态/响应流和图片实际数量 | 出站 HTTP mock 和 API usage/image 结算回归通过；request-start 价格不重算历史。显式供应代理不继承客户端系统代理；search 重定向/PAC 的所有在线条件未实测。 |
| F07 | WHAM voice create → SDP/Location rtc ID → sideband；JSON/multipart/SDP、OpenAI-Alpha、模型/转录准入、call 归属、取消 | 新 `RealtimeKind::Wham` 保留官方 WHAM 路径，使用供应独立传输；当前 renderer `SFc` 的 v1/v3 请求进入本地 TLS mock，真实 SDP/Location 交回实际 reader。管理端增加通话创建记录，只显示真实创建状态；完整音频/跨线程续话留给用户手测。 |
| F08 | primary/secondary 整数百分比和 reset 时间、7d/30d/5h、本地账本与真实活动、日期筛选、headers/SSE/WS 同源 | quota、usage、subscription、迁移隔离测试通过，两版 native 读取实际服务响应。代码审查、credit、plan history 仅有记录读取/不完整采集；thread estimates、月 cap 显式不可用。 |
| F09 | 个人/账号/家庭/通知、邀请筛选游标、资料写后读回、服务订阅有效期；client-owned 值管理只读 | `desktop_profile_has_virtual_identity_and_persistent_local_statistics`、家庭/资料/订阅/跨账号测试通过；私有配置来自虚拟账户。支付资料/资格显示不等于实现外部业务。 |
| F10 | preferences/browser rules/revision、pins、onboarding、公告/提示等字段/枚举/持久化 | `repairs_client_writes_nonempty_pages_and_events_stay_owned_and_survive_restart`、`admin_json_operations_persist_valid_nonempty_desktop_configuration`、`controls_policy_and_family_reads_match_actual_desktop_and_admin_ownership`、Ultra 设置本地回归通过。未执行其所有旧 Desktop reader 脚本，GUI 全动作未验收。 |
| F11 | task 与 conversation 列表/参数、空/非空、多页、owner 与执行 supplier、turn/log 归属、resume/cancel/archive、事件保存 | `repairs_nested_task_and_turn_ownership_mcp_and_rebinding_are_checked_before_execution` 等 API 回归通过；实际云任务/云会话由供应执行，有限额且不可计价时拒绝；没有在线云任务成功证据。 |
| F12 | 插件/连接器目录与安装、SKU/includeExtensions/installed pagination、MCP、自动化 | 本地安装记录分页与未实现错误/隔离测试通过。官方新增 SKU 配置/插件 extensions 不意味着本项目已有对应授权/执行器；featured/list/suggested/detail/apps batch/connector directory/MCP 或安装调度缺口明确列出。 |
| F13 | 设备票据、Celsius 持久事件、远程 server enroll/refresh/socket、撤销及主机归属 | 现有 API 测试通过；remote client pairing/list/MFA 不在现有实现内。在途 Responses HTTP/SSE 没有官方新 network-policy 的统一撤销；Responses WS 仍在消息/生成边界检查，此差异单列。 |
| F14 | health/version、资源加载、遥测/心跳、未知路由记录、launcher 工作线程/地址路由 | 编译后二进制用临时 SQLite 启动并读取 health/version 成功，随后 Ctrl+C 停止。DesktopRequestRouting/WorkerStartup/ProxyHook 通过；没有部署、GUI 全按钮或在线资源全部可达声明。 |

## 每条已注册请求

共 **152 条方法/路由登记，兼容挂载展开为 300 个显式方法/路径组合**（不计隐式 HEAD 和参数取值）。新增 WHAM realtime/calls 在 W 组。每条记录均已对应处理路径、数据来源及下述结论；“本地回归”是本仓库测试证据，不等于实际官方服务成功。

C：`/v1`、`/backend-api/codex`、`/api/oauth/chatgpt/backend-api/codex`；W：`/backend-api/wham`、`/wham`、`/api/codex`、`/v1/api/codex`、`/v1/wham`、`/api/oauth/chatgpt/backend-api/wham`；D：统一加 `/api/oauth/chatgpt`；R：根路径。

| 编号 | 挂载 | 方法 | 路径 | 证据组 | 核查结论 |
| --- | --- | --- | --- | --- | --- |
| R001 | C | POST | [`/alpha/search`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L446) | F06 | 固定 alpha/search 和正文透传通过；在线搜索/重定向待手测 |
| R002 | C | GET | [`/guardian`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L429) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R003 | C | POST | [`/guardian`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L429) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R004 | C | GET | [`/guardian-classifier`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L435) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R005 | C | POST | [`/guardian-classifier`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L435) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R006 | C | POST | [`/images/edits`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L454) | F06 | 出站请求/图像头/实际结果计费回归通过；在线出图待手测 |
| R007 | C | POST | [`/images/generations`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L450) | F06 | 出站请求/图像头/实际结果计费回归通过；在线出图待手测 |
| R008 | C | GET | [`/live`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L471) | F07 | 调用归属/参数/限额拒绝回归通过；完整在线语音待手测 |
| R009 | C | POST | [`/live`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L471) | F07 | 调用归属/参数/限额拒绝回归通过；完整在线语音待手测 |
| R010 | C | GET | [`/live/{call_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L477) | F07 | 调用归属/参数/限额拒绝回归通过；完整在线语音待手测 |
| R011 | C | POST | [`/memories/trace_summarize`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L458) | F05 | 端点/身份透传已查；真实摘要执行待手测 |
| R012 | C | GET | [`/models`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L442) | F03 | 已同步 0.157.0 描述；本地权益过滤；native 读取通过 |
| R013 | C | GET | [`/realtime`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L463) | F07 | 调用归属/参数/限额拒绝回归通过；完整在线语音待手测 |
| R014 | C | POST | [`/realtime/calls`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L467) | F07 | 调用归属/参数/限额拒绝回归通过；完整在线语音待手测 |
| R015 | C | GET | [`/responses`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L423) | F04 | HTTP/SSE/WS、本地计费与额度回归通过；在线生成待手测 |
| R016 | C | POST | [`/responses`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L423) | F04 | HTTP/SSE/WS、本地计费与额度回归通过；在线生成待手测 |
| R017 | C | POST | [`/responses/compact`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L410) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R018 | C | POST | [`/responses/input_tokens`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L414) | F05 | 模型准入回归通过；实际计数上游响应待手测 |
| R019 | C | POST | [`/responses/{*subpath}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L418) | F04 | 路由/头/正文回归通过；实际审查/压缩结果待手测 |
| R020 | D | GET | [`/assets/{file}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L361) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R021 | D | GET | [`/backend-api/accounts/check/v4-2023-04-27`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L212) | F02 | 本地账户身份/服务策略；API 隔离回归通过；GUI 分支待手测 |
| R022 | D | GET | [`/backend-api/accounts/optimized/check`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L216) | F02 | 本地账户身份/服务策略；API 隔离回归通过；GUI 分支待手测 |
| R023 | D | GET | [`/backend-api/accounts/verified_access`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L153) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R024 | D | GET | [`/backend-api/accounts/{account_id}/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L44) | F02 | 本地账户身份/服务策略；API 隔离回归通过；GUI 分支待手测 |
| R025 | D | GET | [`/backend-api/accounts/{account_id}/spend-controls/current-user/monthly-usage`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L119) | F08 | 产品差异：无自然月 spending cap；返回显式不可用 |
| R026 | D | GET | [`/backend-api/aip/first-party/eligibility`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L47) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R027 | D | GET | [`/backend-api/amphora`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L41) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R028 | D | GET | [`/backend-api/amphora/notifications`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L38) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R029 | D | POST | [`/backend-api/amphora/notifications/{notification_id}/reacted`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L115) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R030 | D | GET | [`/backend-api/amphora/u18_graduation_unlink_setting_notices`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L59) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R031 | D | POST | [`/backend-api/amphora/u18_graduation_unlink_setting_notices/dismiss`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L63) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R032 | D | GET | [`/backend-api/amphora/{amphora_id}/members`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L300) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R033 | D | GET | [`/backend-api/aura/site_status`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L99) | F07 | 供应固定公共服务 + 本地记录；上游错误被归一化为 502，保留差异 |
| R034 | D | GET | [`/backend-api/automations`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L150) | F12 | 记录筛选与游标回归通过；调度执行器未实现 |
| R035 | D | POST | [`/backend-api/automations/remove`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L276) | F12 | 未实现执行器；501 和实际失败记录；禁止虚构成功 |
| R036 | D | POST | [`/backend-api/automations/save`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L266) | F12 | 未实现执行器；501 和实际失败记录；禁止虚构成功 |
| R037 | D | POST | [`/backend-api/automations/set_status`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L271) | F12 | 未实现执行器；501 和实际失败记录；禁止虚构成功 |
| R038 | D | GET | [`/backend-api/beacons/home`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L152) | F10 | 管理员具名内容、旧值校验与非空响应回归通过；GUI 效果待手测 |
| R039 | D | GET | [`/backend-api/celsius/ws/user`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L107) | F13 | 持久事件、ticket 及账号/设备撤销回归通过；GUI 事件效果待手测 |
| R040 | D | GET | [`/backend-api/celsius/ws/user/socket`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L326) | F13 | 持久事件、ticket 及账号/设备撤销回归通过；GUI 事件效果待手测 |
| R041 | D | GET | [`/backend-api/checkout_pricing_config/configs/{country_code}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L42) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R042 | D | POST | [`/backend-api/codex/analytics-events/events`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L314) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R043 | D | GET | [`/backend-api/codex/{call_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L206) | F07 | WHAM/SDP/Location 已修复；Desktop 构造/读取与本地传输通过 |
| R044 | D | GET | [`/backend-api/connectors/directory/list`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L95) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R045 | D | POST | [`/backend-api/conversation/init`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L123) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R046 | D | GET | [`/backend-api/conversation/{conversation_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L291) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R047 | D | PATCH | [`/backend-api/conversation/{conversation_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L291) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R048 | D | GET | [`/backend-api/conversations`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L151) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R049 | D | POST | [`/backend-api/f/conversation`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L132) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R050 | D | POST | [`/backend-api/f/conversation/prepare`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L127) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R051 | D | POST | [`/backend-api/f/conversation/resume`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L137) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R052 | D | GET | [`/backend-api/gift-credits/senders/eligibility`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L43) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R053 | D | GET | [`/backend-api/gizmos/snorlax/sidebar`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L48) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R054 | D | GET | [`/backend-api/me`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L220) | F02 | 本地账户身份/服务策略；API 隔离回归通过；GUI 分支待手测 |
| R055 | D | GET | [`/backend-api/models`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L50) | F03 | 本地模型准入/目录回归通过；当前 Desktop picker 已验证 |
| R056 | D | GET | [`/backend-api/notifications/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L45) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R057 | D | PATCH | [`/backend-api/notifications/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L111) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R058 | D | POST | [`/backend-api/o11y/v1/traces`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L196) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R059 | D | GET | [`/backend-api/payments/payment_methods`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L39) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R060 | D | GET | [`/backend-api/pins`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L46) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R061 | D | DELETE | [`/backend-api/pins/{item_type}/{item_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L296) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R062 | D | POST | [`/backend-api/pins/{item_type}/{item_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L296) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R063 | D | GET | [`/backend-api/plugins/featured`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L246) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R064 | D | PATCH | [`/backend-api/profiles/me`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L83) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R065 | D | GET | [`/backend-api/profiles/me/page`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L75) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R066 | D | PATCH | [`/backend-api/profiles/me/page`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L75) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R067 | D | GET | [`/backend-api/profiles/{username}/page`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L79) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R068 | D | POST | [`/backend-api/ps/apps/batch`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L305) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R069 | D | GET | [`/backend-api/ps/mcp`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L304) | F12 | 未实现虚拟 MCP 执行；501 JSON-RPC 错误并记录 |
| R070 | D | POST | [`/backend-api/ps/mcp`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L304) | F12 | 未实现虚拟 MCP 执行；501 JSON-RPC 错误并记录 |
| R071 | D | GET | [`/backend-api/ps/mcp/.well-known/oauth-protected-resource`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L371) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R072 | D | GET | [`/backend-api/ps/plugins/installed`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L254) | F12 | 已有客户端安装记录分页；不代表安装器已实现；API 回归通过 |
| R073 | D | GET | [`/backend-api/ps/plugins/list`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L250) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R074 | D | GET | [`/backend-api/ps/plugins/suggested/codex`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L258) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R075 | D | GET | [`/backend-api/ps/plugins/{plugin_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L262) | F12 | 未实现虚拟目录授权；501；不使用供应私有目录 |
| R076 | D | POST | [`/backend-api/ps/plugins/{plugin_id}/install`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L281) | F12 | 未实现执行器；501 和实际失败记录；禁止虚构成功 |
| R077 | D | POST | [`/backend-api/ps/plugins/{plugin_id}/uninstall`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L286) | F12 | 未实现执行器；501 和实际失败记录；禁止虚构成功 |
| R078 | D | GET | [`/backend-api/referrals/invite/eligibility`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L37) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R079 | D | GET | [`/backend-api/referrals/invite/tracking`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L91) | F09 | 自身邀请记录的筛选/分页回归通过；不提供邮件发送 |
| R080 | D | POST | [`/backend-api/sentinel/heartbeat`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L103) | F14 | 本地设备存活更新，204；无需供应调用；回归通过 |
| R081 | D | PATCH | [`/backend-api/settings/account_user_setting`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L242) | F02 | 本地账户身份/服务策略；API 隔离回归通过；GUI 分支待手测 |
| R082 | D | GET | [`/backend-api/settings/is_adult`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L51) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R083 | D | GET | [`/backend-api/settings/user`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L221) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R084 | D | GET | [`/backend-api/settings/voices`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L200) | F07 | 供应固定公共服务 + 本地记录；上游错误被归一化为 502，保留差异 |
| R085 | D | POST | [`/backend-api/stop_conversation`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L142) | F11 | 记录/归属/续聊/配置准入已查；真实云会话完整动作待手测 |
| R086 | D | GET | [`/backend-api/subscriptions`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L238) | F09 | 手动订阅/有效期/本地权益；不含官方 checkout 或付款 |
| R087 | D | GET | [`/backend-api/subscriptions/auto_top_up/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L230) | F09 | 手动订阅/有效期/本地权益；不含官方 checkout 或付款 |
| R088 | D | GET | [`/backend-api/subscriptions/credits/discount-offer`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L234) | F09 | 手动订阅/有效期/本地权益；不含官方 checkout 或付款 |
| R089 | D | GET | [`/backend-api/system_hints`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L49) | F10 | 管理员具名内容、旧值校验与非空响应回归通过；GUI 效果待手测 |
| R090 | D | GET | [`/backend-api/tpp/models`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L226) | F03 | 本地模型准入/目录回归通过；当前 Desktop picker 已验证 |
| R091 | D | GET | [`/backend-api/tpp/models/`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L222) | F03 | 本地模型准入/目录回归通过；当前 Desktop picker 已验证 |
| R092 | D | GET | [`/backend-api/trusted_contact/enabled`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L40) | F09 | 虚拟资格/资料读取；不代表付款、邀请发送或外部资格已实现 |
| R093 | D | POST | [`/backend-api/wham/analytics-events/events`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L310) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R094 | D | GET | [`/backend-api/wham/analytics/daily-code-review-metrics`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L155) | F08 | 查询本账号持久记录；采集来源不完整，不能据空列表称能力完成 |
| R095 | D | GET | [`/backend-api/wham/analytics/daily-plugin-usage-metrics`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L173) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R096 | D | GET | [`/backend-api/wham/analytics/daily-skill-usage-metrics`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L178) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R097 | D | GET | [`/backend-api/wham/analytics/daily-workspace-usage-counts`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L164) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R098 | D | GET | [`/backend-api/wham/browser/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L154) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R099 | D | PATCH | [`/backend-api/wham/browser/settings`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L188) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R100 | D | GET | [`/backend-api/wham/onboarding/context`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L149) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R101 | D | POST | [`/backend-api/wham/onboarding/desktop/complete`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L192) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R102 | D | PATCH | [`/backend-api/wham/profiles/me`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L87) | F09 | 自身资料/家庭/通知记录；账号隔离及写后读回回归通过 |
| R103 | D | GET | [`/backend-api/wham/remote/control/server`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L330) | F13 | 注册/续期/socket/撤销隔离回归通过；远程配对业务未完整实现 |
| R104 | D | POST | [`/backend-api/wham/remote/control/server/enroll`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L67) | F13 | 注册/续期/socket/撤销隔离回归通过；远程配对业务未完整实现 |
| R105 | D | POST | [`/backend-api/wham/remote/control/server/refresh`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L71) | F13 | 注册/续期/socket/撤销隔离回归通过；远程配对业务未完整实现 |
| R106 | D | GET | [`/backend-api/wham/sites/access`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L148) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R107 | D | POST | [`/backend-api/wham/statsig/bootstrap`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L306) | F02 | 虚拟身份和配置持久化回归通过；旧 SDK 提取器需另行跟进 |
| R108 | D | GET | [`/backend-api/wham/usage/credit-usage-events`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L172) | F08 | 查询本账号持久记录；采集来源不完整，不能据空列表称能力完成 |
| R109 | D | GET | [`/backend-api/wham/usage/daily-token-usage-breakdown`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L168) | F08 | 实际账本/活动聚合和账号隔离回归通过；不虚构历史 |
| R110 | D | GET | [`/backend-api/wham/usage/plan_limit_history`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L177) | F08 | 查询本账号持久记录；采集来源不完整，不能据空列表称能力完成 |
| R111 | D | OPTIONS | [`/ces/v1/rgstr`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L340) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R112 | D | POST | [`/ces/v1/rgstr`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L340) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R113 | D | OPTIONS | [`/ces/v1/telemetry/intake`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L334) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R114 | D | POST | [`/ces/v1/telemetry/intake`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L334) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R115 | D | GET | [`/codex-app-prod/windows-store-update.json`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L365) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R116 | D | GET | [`/mcp-app.html`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L357) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R117 | D | GET | [`/oauth/authorize`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L379) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R118 | D | GET | [`/oauth/authorize/bootstrap`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L383) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R119 | D | POST | [`/oauth/authorize/submit`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L387) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R120 | D | POST | [`/oauth/revoke`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L391) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R121 | D | POST | [`/oauth/token`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L375) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R122 | D | OPTIONS | [`/v1/initialize`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L352) | F02 | 虚拟身份和配置持久化回归通过；旧 SDK 提取器需另行跟进 |
| R123 | D | POST | [`/v1/initialize`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L352) | F02 | 虚拟身份和配置持久化回归通过；旧 SDK 提取器需另行跟进 |
| R124 | D | POST | [`/v1/responses/input_tokens`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L318) | F10 | 客户端状态与管理员策略分离；本地读写/归属回归；GUI 待手测 |
| R125 | D | OPTIONS | [`/v1/sdk_exception`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L346) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R126 | D | POST | [`/v1/sdk_exception`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L346) | F14 | 支持资源/采集边界已查；本地回归通过；远端资源实时可用性待手测 |
| R127 | R | GET | [`/codex/desktop-auth`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L399) | F01 | 本地 OAuth/发现契约回归通过；真实浏览器登录待手测 |
| R128 | R | GET | [`/healthz`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L397) | F14 | 编译后二进制实测通过 |
| R129 | R | GET | [`/v1/usage`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L14) | F08 | 同一虚拟额度来源；API/native 回归通过 |
| R130 | R | GET | [`/version`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L398) | F14 | 编译后二进制实测通过 |
| R131 | W | GET | [`/accounts/check`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L495) | F02 | 已修复 NO_CONSTRAINT；Desktop schema 和两版 native 通过 |
| R132 | W | GET | [`/config/bundle`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L503) | F02 | SQLite 服务配置已查；API 回归通过；GUI 全启动待手测 |
| R133 | W | GET | [`/profiles/me`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L499) | F09 | 身份和自身实际用量回归通过；当前 GUI 全动作待手测 |
| R134 | W | GET | [`/rate-limit-reset-credits`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L530) | F08 | 自身额度或重置记录；本地回归通过；不复制官方积分 |
| R135 | W | POST | [`/rate-limit-reset-credits/consume`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L534) | F08 | 自身额度或重置记录；本地回归通过；不复制官方积分 |
| R136 | W | POST | [`/realtime/calls`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L486) | F07 | WHAM/SDP/Location 已修复；Desktop 构造/读取与本地传输通过 |
| R137 | W | GET | [`/settings/configs/user-preferences`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L490) | F10 | 客户端偏好/schema 写后读回/持久化回归通过 |
| R138 | W | GET | [`/settings/user`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L507) | F10 | 客户端偏好/schema 写后读回/持久化回归通过 |
| R139 | W | PATCH | [`/settings/user`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L507) | F10 | 客户端偏好/schema 写后读回/持久化回归通过 |
| R140 | W | POST | [`/tasks`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L543) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R141 | W | GET | [`/tasks/list`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L539) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R142 | W | GET | [`/tasks/{task_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L547) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R143 | W | POST | [`/tasks/{task_id}/archive`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L567) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R144 | W | POST | [`/tasks/{task_id}/cancel`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L563) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R145 | W | GET | [`/tasks/{task_id}/turns`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L551) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R146 | W | GET | [`/tasks/{task_id}/turns/{turn_id}`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L555) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R147 | W | GET | [`/tasks/{task_id}/turns/{turn_id}/logs`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L559) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R148 | W | GET | [`/tasks/{task_id}/turns/{turn_id}/sibling_turns`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L571) | F11 | 自身记录/分页/归属/换绑/限额回归通过；真实云执行待手测 |
| R149 | W | GET | [`/usage`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L518) | F08 | 自身额度或重置记录；本地回归通过；不复制官方积分 |
| R150 | W | POST | [`/usage/thread-estimates/query`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L526) | F08 | 未实现线程金额估算；409 usage_estimate_unavailable |
| R151 | W | POST | [`/usage/thread_usage/query`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L522) | F08 | 未实现线程金额估算；409 usage_estimate_unavailable |
| R152 | W | GET | [`/workspace-messages`](../crates/codex2api-api/src/providers/chatgpt/routes.rs#L513) | F02 | SQLite 服务配置已查；API 回归通过；GUI 全启动待手测 |

## 对外供应请求补充

上表是客户端入口；供应侧另核对 `auth/src/oauth.rs` 的授权 URL GET、授权码/token-exchange form POST、refresh JSON POST、revoke JSON POST，以及 `manual.rs` 的 deviceauth/usercode 与 deviceauth/token POST。它们继续使用固定 client_id/scopes、账号独立传输和实际 OAuth 协议；不能把供应登录接口当虚拟 OAuth 服务的入口。供应 quota/accounts/profile/settings 只用于管理员快照或执行发现；虚拟端各项不借用供应私有数据。

## 客户端反向查漏

对当前 main/renderer 字面请求调用共扫描到 317 次、216 个不同路径，初筛 153 个未匹配候选，增加 WHAM voice 后减少其中 1 项。下面逐条保留候选，不把 bundle 中其他产品、动态模板或 feature 分支都判为 Codex 可达 bug。直接已确认的 files/voice 见上文；其余仍需按实际启用功能手测。该扫描不涵盖所有运行时拼接 URL，不能证明没有未知请求；缺失入口日志继续记录实际请求。

| 方法提示 | 候选路径 | 当前判断 |
| --- | --- | --- |
| safeGet | `/accounts/check/{version}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/accounts/logout-impact` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/accounts/mfa_info` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/accounts/send_add_credits_nudge_email` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/accounts/{account_id}/groups` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/accounts/{account_id}/invites/request` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/accounts/{account_id}/remaining_balance` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/accounts/{account_id}/users` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet/safePost | `/accounts/{account_id}/workspace_admin_requests` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePatch | `/accounts/{account_id}/workspace_admin_requests/{request_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| fetch | `/aip/connectors/${encodeURIComponent(e)}/logo?theme=${t}` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/github/has_installations` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/list_accessible` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/noauth` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/oauth` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/oauth/callback` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/oauth/complete` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/links/oauth/reauth` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/service_accounts/links/noauth` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/aip/connectors/service_accounts/links/oauth` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/aip/connectors/{connector_id}` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/aip/connectors/{connector_id}/link` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/aip/ledger/chats` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/credit/connection` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/files` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/files/{file_id}/download_link` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/aip/ledger/financial_memories` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/aip/ledger/financial_memories/{ledger_financial_memory_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/links` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/aip/ledger/links/{ledger_link_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/aip/ledger/links/{ledger_link_id}/accounts/{ledger_account_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/aip/ledger/links/{ledger_link_id}/sync` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/aip/ledger/oauth/start` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet/safePatch | `/aip/ledger/oauth/state/{oauth_state_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/aip/ledger/oauth/submit-public-token` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet/safePatch | `/aip/ledger/widget_preferences` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/widgets` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/aip/ledger/widgets/metadata` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/apps/availability` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/automation/{automation_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/automation/{automation_id}/run` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/beacons/event` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/chat/frontend/v1/saved-entities` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/chat/frontend/v1/saved-entities/remove` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/chat/frontend/v1/saved-entities/status` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/conversation/id/{conversation_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/conversation/id/{conversation_id}/rename` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/conversation/message/language-learning-blocks/feedback` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/conversation/{conversation_id}/interpreter/download` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete/safePost | `/conversation/{conversation_id}/messages/{message_id}/reactions` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/conversation/{conversation_id}/rating` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/conversation/{conversation_id}/stream_status` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/conversation/{id}/attachment/{file_id}/download` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/conversations/batch` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/conversations/search` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/call_mcp` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/launch_widget` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/launcher/auto_install` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/launcher/bootstrap` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/url_safe` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/ecosystem/widget` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/ecosystem/widget_state` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/f/steer_turn` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/files` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/files/download/{file_id}` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/directories` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/files/library/directories/path` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet/safePost | `/files/library/favorites` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/favorites/status` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeDelete | `/files/library/favorites/{favorite_id}` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePatch | `/files/library/files/{library_file_id}/move` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/files/{library_file_id}/restore` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/image-favorites` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/image-favorites/status` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/library/mounted/materialize` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/files/library/nodes` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/files/{file_id}/simple` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safePost | `/files/{file_id}/uploaded` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/gizmos/bootstrap` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/gizmos/firstparty/sidebar` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/gizmos/{gizmo_id_or_short_url}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/gizmos/{gizmo_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/gizmos/{gizmo_id}/conversations` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/global/search` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/hermes/agent/{agent_id}/pin` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/hermes/agent/{agent_id}/system-hint` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/hermes/agents/pinned` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/image-gen/markup-preview` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/ios/attestation_challenge` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/learning/flashcards/pronunciation` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/legalapi/enabled` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/local/entity-shares` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/notifications/subscription/deregister` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/paragen_submission` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/payments/checkout` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/pets` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| fetch | `/pets/create` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/pets/share/{shared_pet_id}/adopt` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/profiles/me/sticker-layout` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/profiles/{username}/sticker-layout` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/projects` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePatch | `/projects/{project_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/projects/{project_id}/connector_scopes` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/projects/{project_id}/files` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeDelete | `/projects/{project_id}/files/{file_id}` | A06：完整文件业务未实现；动态/资源路径仍需区分 |
| safeGet | `/projects/{project_id}/saves` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| fetch | `/pronunciation/synthesize?format=mp3` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/ps/plugins/workspace/template-instances` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/ps/plugins/{plugin_id}/skills/{skill_name}` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/ps/v2/links/credentials` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/quorum/refresh_enrollment_status` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/referrals/invite` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/referrals/invite/{referral_id}/send-email` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/sentinel/chat-requirements/prepare` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/settings/announcement_viewed` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePatch | `/settings/user_tpp_last_used_model_config` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/share/create` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/share/post` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/share/post/resolve/{post_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet/safePatch | `/share/{shared_conversation_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/share/{shared_conversation_id}/file/{file_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/share/{shared_conversation_id}/file_from_message/{message_id}` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/sidebar/conversation_context_citation_feedback` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/spaces/people` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/subscriptions/auto_top_up/disable` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/subscriptions/auto_top_up/enable` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/subscriptions/auto_top_up/update` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/subscriptions/update` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/subscriptions/update/cancel_pending` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/subscriptions/update/preview` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| fetch | `/transcribe` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/unified_user_signals` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safeGet | `/wham/environments` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/wham/environments/by-repo/{provider}/{repo_owner}/{repo_name}` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/wham/github/branches/{repo_id}/search` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/wham/github/installations/v2` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/wham/onboarding/entrypoints/{entrypoint}/complete` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| fetch | `/wham/profiles/me/photo` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/wham/remote/control/client/pair` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/wham/remote/control/clients` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeDelete | `/wham/remote/control/clients/{client_id}` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safeGet | `/wham/remote/control/mfa_requirement` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/wham/shared_threads` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/wham/shared_threads/upload_urls` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/wham/tasks/{task_id}/mark_read` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/wham/tasks/{task_id}/recover` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/wham/tasks/{task_id}/turns/{task_turn_id}/pr` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/wham/worktree_snapshots/finish_upload` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/wham/worktree_snapshots/upload_url` | A09：无匹配完整业务入口；静态分支可达性待实际动作确认 |
| safePost | `/widget_server_action` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
| safePost | `/widget_server_action/public` | 当前无匹配；含其它产品/资源或动态分支；不纳入已通过范围 |
