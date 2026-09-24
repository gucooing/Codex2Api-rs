# 虚拟账户接口与运营网页核查（2026-09-21）

## 结论

当前实现还不能认定为“虚拟账户接口完整、管理设置全部有效”。本次确认 **12 组问题：5 组 P1、7 组 P2**。主要问题是有效订阅权益不统一、部分执行路径没有完成账户归属和费用控制，以及网页允许保存客户端不能正确消费的配置。

按用户本轮明确的规则，本系统按 ChatGPT 订阅账户方式运营，区别是管理员手动发放订阅，替代官方购买流程。订阅发放、续期、套餐和有效期必须影响实际权益；管理网页、持久化、客户端读取、实际执行和运营记录必须形成完整链路。继续遵守已有数据归属约定：管理员维护订阅、身份、目录、计费和服务策略；客户端维护会话、偏好、审批、安装和操作状态，管理网页查看这些实际记录。

本轮是核查，未修改业务实现、客户端、启动器、数据库或 Codex 基线。临时复现全部使用测试 SQLite 和测试身份，不对供应账户执行任务或 MCP 操作。

用户新增的订阅运营规则已同步到 [AGENTS.md](D:/codex-proxy-rs/AGENTS.md) 和 [ARCHITECTURE.md](D:/codex-proxy-rs/docs/ARCHITECTURE.md)，作为后续实现约束。

## 依据与覆盖边界

- 客户端：用户指定的 `C:\Users\gucooing\Downloads\ChatGPT`，安装包 `26.915.4065.0`，内部版本 `26.915.31945`。使用格式化发布代码及原安装包内置代码，没有把它当作完整原始工程。
- 服务端：当前工作区，包括已有未提交文件；不是只检查 HEAD。
- 逐组检查了路由、handler、账户归属、SQLite、管理员表单/保存校验，以及对应客户端请求、读取函数和后续分支。
- 以下区分源码确认、测试 API 复现、执行真实客户端函数、原生 app-server 验证。本轮没有重新执行完整 Desktop GUI 会话或真实上游任务；已有文档中的历史 GUI 成功记录不作为本轮重新验证的证据。
- “字段不存在”只有在实际读取需要它时才列为问题；可选字段、明确不适用的月额度、没有付款事实的支付记录，不要求虚构填充。

客户端路径简写：下文 `APP` 指 [app-initial-6c4523b43a11.js](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js)，行号按 LF 计算。客户端业务路径默认位于代理 `/api/oauth/chatgpt/backend-api` 下。

## P1：账户隔离与执行控制

### F12. 手动发放的订阅仍被当作展示信息，缺少统一有效权益

- 管理网页字段仍称“展示套餐”，到期时间被直接保存，见 [oauth_accounts.rs:42](D:/codex-proxy-rs/crates/codex2api-admin/src/views/oauth_accounts.rs:42)。账户认证只校验令牌有效期和 `enabled`，见 [virtual_accounts.rs:163](D:/codex-proxy-rs/crates/codex2api-storage/src/virtual_accounts.rs:163)。
- 只有部分账户响应计算 `has_active_subscription`；OAuth 继续从存储的 `plan_type` 签发套餐声明，额度也继续返回原套餐并仅根据费用窗口计算 `allowed`，见 [virtual_identity.rs:13](D:/codex-proxy-rs/crates/codex2api-api/src/virtual_identity.rs:13)、[oauth.rs:224](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/oauth.rs:224)、[virtual_management.rs:559](D:/codex-proxy-rs/crates/codex2api-storage/src/virtual_management.rs:559)。
- 复现：将测试 Pro 订阅设置为昨天到期后登录，`accounts/optimized/check` 返回 `has_active_subscription:false`，新令牌仍声明 Pro，`wham/usage` 仍是 `plan_type:pro`、`allowed:true`。请求执行路径没有统一有效订阅权益判定。
- 影响：按本轮明确的“手动发放订阅”规则，发放和到期不能只改变部分展示；目前各读取路径与使用控制不一致。
- 网页配套：提供与有效订阅相连的发放、续期、套餐变更和有效期操作，并能追溯真实管理操作。客户端套餐、权益、额度和执行检查应共同读取有效订阅。到期不再享有原付费权益；是否仍有免费层访问由服务策略决定，不应把到期简单等同于注销登录。

### F1. 云任务续聊检查了错误层级的任务 ID

- 客户端实际构造 `POST /wham/tasks` 的续聊正文是 `follow_up: {task_id, turn_id, environment_mode}`，见 [APP:239425](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:239425)。
- 服务端仅检查顶层 `value["task_id"]`，随后将原始正文送往绑定供应账户，见 [backend.rs:173](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/backend.rs:173)。`follow_up.task_id`、所属轮次及原绑定来源没有经过对应检查。
- 影响：两个虚拟账户共用供应账户时，持有另一任务 ID 的请求可以越过本地任务隔离检查；换绑后的续聊同样没有按任务原供应账户阻止。
- 复现：测试账户故意不绑定供应账户。未知顶层任务 ID 返回 404；同一 ID 放到客户端真实的 `follow_up.task_id` 后进入供应账户认证阶段，返回 `503 upstream_unavailable`。此测试证明本地归属检查被跳过，没有向真实供应账户发送越权请求。
- 网页配套：任务记录需展示所属虚拟账户、来源供应账户及真实状态；管理员换绑不能把历史任务归属迁移给新供应账户。修复服务端检查，不开放手工修改任务归属或轮次历史。

### F2. MCP 直接转发供应账户的私有资源操作

- 客户端 `/ps/mcp` 不仅获取目录，还执行 `tools/call`；例如 `sites.get_environment_variables`、`sites.delete_site`，见 [APP:143134](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:143134)、[APP:143426](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:143426)、[APP:143575](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:143575)。
- [chatgpt.rs:486](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/chatgpt.rs:486) 对 MCP 正文、方法和资源 ID 不作虚拟账户授权检查，直接使用绑定账户请求上游，返回内容也不进入本地资源归属流程。插件响应删除几个身份字段的分支不适用于 MCP 业务授权。
- 影响：供应账户允许的私有资源读取或变更，可被虚拟身份直接使用；绑定负责执行不等于授权读取供应账户的全部私人资源。
- 证据边界：客户端工具调用与服务器直通路径由源码确认；本轮未读取、删除或改动任何真实站点及环境变量。
- 网页配套：连接器目录、当前虚拟账户已授权能力、归属资源和实际调用记录必须对应。只有目录展示、供应账户绑定和 HTTP 成功日志，不能表示已经实现独立账户的授权与资源管理。

### F3. 云任务创建和续聊绕过虚拟费用上限

- [backend.rs:173](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/backend.rs:173) 的 `CreateTask` 路径不调用 `check_virtual_quota`，也不建立 `UsageContext` 或费用结算记录。成功后只保存任务快照。
- 客户端新建和续聊都会调用该入口，见 [APP:239385](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:239385) 和 F1。
- 复现：5 小时费用上限设为 0 后，新建任务仍进入供应账户认证阶段，而非本地 429 拒绝。未绑定测试身份保证没有实际供应费用。
- 影响：网页配置的 5 小时/7 天上限并不覆盖这个执行入口，任务消耗也不会自动进入当前虚拟费用账本。
- 网页配套：用量页应能够追溯任务消耗及计价状态。无法取得真实使用量时须明确未计价；有限额账户不能借此绕开费用限制。不得把任务次数换算成虚构费用。

### F4. Realtime 路径未接费用账本，连接内额度检查也被跳过

- [realtime.rs:9](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/realtime.rs:9) 的 HTTP call 只检查已有额度，没有费用记录；WebSocket 使用 [realtime.rs:112](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/realtime.rs:112) 的 `bridge`。
- [websocket.rs:107](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/websocket.rs:107) 将 `ledger=None` 传入中继，而逐次 `response.create` 额度检查放在 `Some(ledger)` 分支内，见 [websocket.rs:179](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/websocket.rs:179)。因此该连接不做这一分支的逐次检查或完成结算。
- 影响：建连时未超额的实时连接，后续生成不受这条逐次检查约束；实时用量不出现在当前账本。普通 Responses 的 WS 修复不能覆盖这个入口。
- 证据边界：服务器控制流确认；客户端发布代码包含原生实时 RPC，但本轮没有进行真实语音/Realtime 推理或验证其每种网络传输。
- 网页配套：费用页应明确支持计价的执行类型和未计价记录。Realtime 若尚不能正确计价，有限额账户应有明确不可用结果，不能表现为正常收费能力。

## P2：网页保存与客户端协议不一致

### F5. 网页“系统提示”条目缺少实际需要的 `system_hint`

- 网页新增条目是 `{id,title,description}`，见 [virtual-management.js:51](D:/codex-proxy-rs/crates/codex2api-admin/static/virtual-management.js:51)。保存逻辑生成 `id`，未生成客户端读取的 `system_hint`。
- 客户端在插件/连接器提示分支直接执行 `t.system_hint.startsWith(...)`，见 [APP:194026](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:194026)。服务端对 `mode=basic/plugins/connectors` 返回同一配置。
- 复现：用当前管理员模板保存，经实际 API 读取，再执行客户端原读取代码，得到读取 `startsWith` 的异常。空数组时不会触发，因此空状态回归不能覆盖此问题。
- 网页配套：定义可选择的提示类型与实际业务内容，由代码生成协议标识，并按客户端请求模式输出对应条目。保存前应验证非空条目；不能要求运营人员自行补协议字段。

### F6. 网页“模型版本”模板不能形成客户端认可的版本

- 网页模板为 `versions:{name:'',models:[]}`，见 [virtual-management.js:45](D:/codex-proxy-rs/crates/codex2api-admin/static/virtual-management.js:45)。
- 客户端版本需要非空 `id`，并读取 `intelligence_presets`、`slugs` 和显示字段；无效版本通过 `.catch(null)` 被丢弃，见 [APP:135206](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:135206)。
- 服务端校验只检查模型条目的 `slug`，没有完整校验版本结构及引用关系，见 [virtual_management.rs:343](D:/codex-proxy-rs/crates/codex2api-storage/src/virtual_management.rs:343)。
- 复现：保存网页结构后 `/models` 返回 200；执行客户端自带 Zod 和真实模型 schema 后，该版本变成 `null`。
- 网页配套：提供版本名称、关联模型、档位/思考强度等有意义的控件，自动生成版本 ID，并检查默认模型、版本引用是否存在。模型能力如附件/工具/思考强度当前也缺少完整的具名配置链路，应与客户端实际需要的能力一起核对，而非增加任意字段编辑器。

### F7. 网页“首页公告”只能生成无法展示的对象

- 网页新增公告为 `{title,description}`，见 [virtual-management.js:40](D:/codex-proxy-rs/crates/codex2api-admin/static/virtual-management.js:40)。存储默认是 null，其通用形状校验允许这类不完整对象保存。
- 客户端要求 `beacon_id`、`beacon_name`、`type`、`action_items`、`ui_info` 等结构，见 [APP:181042](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:181042)。[公告读取函数:177](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/codex-app-home-beacon-announcement-state-42c9bb863dad.js:177) 对解析失败的公告返回 `beacon:null`。
- 复现：当前模板经过本地保存和 API 返回后，被实际客户端公告 schema 拒绝。
- 网页配套：保留运营填写标题、正文、展示形式及已支持按钮的方式，由服务端生成完整公告协议。无法执行的按钮动作不能作为可用操作提供。

## P2：客户端操作、记录和查询链路

### F8. 读取入口存在，但正常操作与后续读取尚未接通

以下 15 个请求已通过临时本地 Router 测试，均返回 `501 endpoint_not_implemented`。这是操作链路缺失，不应通过恢复管理员填写客户端状态来补偿。

| 功能 | 本次复现的请求 | 客户端依据与影响 |
| --- | --- | --- |
| 云偏好 | `PATCH /wham/settings/user`；`GET /wham/settings/configs/user-preferences` | [cloud-preferences:26](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/cloud-preferences-f6db6f007247.js:26)、[同文件:60](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/cloud-preferences-f6db6f007247.js:60)。页面所需配置和保存入口不完整；现 GET `/wham/settings/user` 只是读取另一套 `user_settings.settings`。 |
| 云置顶 | `POST/DELETE /pins/{item_type}/{item_id}` | [APP:210159](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:210159)。读取列表可用，但客户端云置顶操作无法保存；不涉及 Desktop 本地侧栏置顶。 |
| 云自动化 | `POST /automations/save`、`/set_status`、`/remove` | [APP:287092](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:287092)。新增、启停和移除未接入实际流程。 |
| ChatGPT 云会话 | `GET/PATCH /conversation/{conversation_id}` | 客户端有会话读取及更新流程；路由仅实现 init/prepare/send/resume/stop 和摘要列表。已列出的会话缺少完整重新打开链路。 |
| 云插件安装 | `POST /ps/plugins/{plugin_id}/install`、`/uninstall` | [安装组件:3059](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/plugin-installation-content-ddacd8c2c056.js:3059)、[APP:221559](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:221559)。已安装列表有结构和分页，不等于真实安装流程已完成；本地原生插件操作是另一条链路。 |
| 云任务 | `GET /wham/tasks/{id}/turns`、`GET /wham/tasks/{id}/turns/{turn}/logs`、`POST /wham/tasks/{id}/cancel`、`POST /wham/tasks/{id}/archive` | [APP:239126](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:239126)、[APP:239243](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:239243)。任务创建/详情之外的生命周期未接完。 |

路由依据：[lib.rs](D:/codex-proxy-rs/crates/codex2api-api/src/lib.rs)，缺失路径实际走 [missing.rs:8](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/missing.rs:8)。上表不是全部未实现 API 清单，也不把客户端索引中的全部 595 种路径都视为本项目必须立即实现的能力。

网页配套：每个账户的“客户端记录”应来自这些正常操作的真实落库；任务状态、安装结果和自动化执行结果不可由运营表单编造。现有只读归属方向符合约定，但数据接收和执行链路仍需补齐。

### F9. 列表未遵守筛选、分页及状态更新语义

- 自动化：客户端 [APP:286974](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:286974) 按 `filter/limit/cursor` 循环请求；实际路由进入 [desktop.rs:45](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/desktop.rs:45)，直接返回存储配置。[virtual_data.rs:170](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/virtual_data.rs:170) 虽写了自动化分页分支，但 `/automations` 没有使用它。
- 会话：[APP:209250](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:209250) 发送 `is_archived`、`is_starred`、来源及排序等参数；服务器仅解析 offset/limit。
- 云任务：[APP:163875](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:163875) 请求 `task_filter=current` 并依据任务状态继续轮询；[backend.rs:117](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/backend.rs:117) 没处理 `task_filter`，且只读本地快照，创建后的状态只有读取详情等路径才更新。
- 复现：`filter=scheduled&limit=1&cursor=1` 返回两条不同启停状态的自动化；`is_archived=false` 返回已归档会话；`task_filter=current` 返回归档任务。另执行原客户端自动化循环，确认服务器若返回历史存储中的非空固定 cursor，会重复请求；默认 null 不触发这个循环问题。
- 网页配套：网页和客户端应查询同一真实状态和筛选结果；页面显示更新时间，任务状态由实际查询/执行事件更新，不由管理员填写。

### F10. 语音设置写入成功，但用户设置读回缺字段

- `feature=voice_name` 写入单独的 `voice.selected`，见 [chatgpt.rs:122](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/chatgpt.rs:122)；`GET /settings/user` 直接返回 `user_settings`，未合入这一选择，见 [chatgpt.rs:52](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/chatgpt.rs:52)。
- 客户端读 `settings.voice_name`，见 [APP:179474](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:179474)。[语音设置组件:78](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/voice-personalization-dialog-db70fd14ae97.js:78) 保存后会失效并重读用户设置；[语音会话:3394](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/voice-session-5873ba3a7f75.js:3394) 有依赖 `voiceName` 的执行分支。
- 复现：PATCH 返回 200、SQLite 保存 `marin`，随后 `/settings/user` 没有 `settings.voice_name`。另一条 `/settings/voices.selected` 路径的存在不能保证全部消费者一致。
- 网页配套：语音保留客户端选择、管理端只读；服务器从同一份持久化选择生成两套实际需要的响应字段。

### F11. 会话事件可订阅，但没有对应实际事件生产链路

- 客户端订阅 `conversations` 与 `alder-conversations` 并处理创建、更新、完成事件，见 [APP:211875](C:/Users/gucooing/Downloads/ChatGPT/readable/webview/assets/app-initial-6c4523b43a11.js:211875)。
- 服务端允许这些订阅，但实际事件写入只覆盖配置更新和通知；[save_virtual_resource:673](D:/codex-proxy-rs/crates/codex2api-storage/src/virtual_management.rs:673) 只更新资源表，[record_conversations:199](D:/codex-proxy-rs/crates/codex2api-api/src/handlers/virtual_conversations.rs:199) 未生产这些订阅需要的事件。
- 影响：完成 WebSocket 连接/订阅与账户隔离测试，不表示会话新增、更新和完成能推动另一客户端刷新。
- 网页配套：实际会话变化应落库并产生对应账户事件；管理端显示同一记录和更新时间。无需提供人工制造事件的表单。

## 按独立账户运营检查网页操作

| 运营操作 | 当前情况 | 配套要求 |
| --- | --- | --- |
| 手动发放/续期订阅，管理有效期和权益 | 目前只是账户上的套餐/到期字段，尚未形成一致的有效权益 | 按 F12 连接真实运营操作、客户端权益和执行检查；不以购买/付款记录代替手动发放记录 |
| 创建账户、身份编辑、换绑、停用、修改密码、下线设备 | 已有表单、持久化、登录和 CSRF 校验；换绑保留历史的现有测试通过 | 保留账户身份、历史、设备与供应执行的边界；修复 F1 对历史任务的影响 |
| 配置 5 小时/7 天费用上限，查看用量 | 已有对应控件、账本及客户端窗口验证 | 补齐 F3/F4；不能仅凭额度页面正确就认定全部执行路径受控 |
| 设置 / 模型计费 | 价格表和请求价格快照已有实现；未知费用单独显示 | 沿用全局价格管理，不擅自增加账户独立价格、总额度或虚构消费 |
| 模型目录和默认选择 | 可编辑，但版本模板与能力字段不完整 | 按 F6 修复保存校验及客户端读回；区分 ChatGPT 目录与 Codex `/codex/models`，后者当前仍走上游，前者表单不控制它 |
| 系统提示、首页公告 | 能保存；有效非空配置分别导致异常或被丢弃 | 按 F5/F7 提供真正可发布的业务控件和生成逻辑 |
| 工作区功能策略、界面功能 flags | 已使用具名控件；与用户偏好编辑权限分开 | 检查启用后依赖的业务入口；例如“客户端原生额度申请”等选项不能仅因枚举有效就视为业务已接通 |
| 会话/云项目/置顶/插件/自动化记录 | 管理端已改只读，但多项客户端操作/采集未完成 | 按 F8/F9/F11 连接正常操作、查询和记录展示；不恢复管理员代写客户端状态 |
| 语音、浏览器审批、引导状态 | 已按客户端归属保存和展示；语音多读取路径不一致 | 修复 F10；继续使用现有版本冲突检查和客户端写入 |
| MCP/站点及连接器 | 有目录、部分策略和绑定执行，缺完整虚拟资源授权 | 修复 F2，并使运营可查看本账户实际授权与操作结果 |
| 日志和诊断 | 已有账户请求/活动日志、缺失接口历史和支持诊断 | HTTP 200 不能直接显示为任务/安装/外部操作成功；有真实业务结果后再展示完成状态 |

**只读空记录不等于已接完整数据源。** `code_review_metric`、`credit_event`、`plan_period`、`reset_credit` 目前有通用查询入口，未发现对应的完整生产写入流程。项目、支付/家庭、通知类别和安装记录也仍有采集或业务能力边界。例如通知默认类别为空，PATCH 只更新已有类别，不能据此宣称完整通知偏好流程已可用。无积分发放或付款事实时不制造记录；手动订阅发放记录应来自实际管理操作。

## 本轮验证结果

| 验证 | 结果与边界 |
| --- | --- |
| `codex2api-api / virtual_oauth` | 24 项通过，1 项原有完整登录集成测试仍按配置忽略。初次直接执行 WindowsApps 内二进制遇到系统拒绝访问的两项，改用 Desktop 自己已有的原生运行时后单独重跑通过 |
| 原生运行时核验 | 已有运行时与安装包 `resources/codex.exe` 的 SHA-256 相同：`BC45017E8239DC150258F69309CED9DF6BBCDF5B8E4F346DECF780AC0999E226`；本轮未复制或修改运行时 |
| 实际 Desktop 原生读回 | `account/rateLimits/read` 成功读取虚拟 5h/7d 窗口；`plugin/list` 成功读取本地安装列表分页。只证明这些读取，不证明插件安装或实时推理完成 |
| `codex2api-admin / virtual_accounts` | 4 项通过：会话/CSRF、管理保存、客户端状态禁止管理员代写、来源隔离和诊断页面等现有覆盖 |
| `codex2api-storage --lib` | 7 项通过，包括价格快照、缓存计费、重复结算保护、独立周期重置与历史归属 |
| 临时定点 API 复现 | 6 项复现检查完成，覆盖过期订阅权益不一致、错误任务 ID 层级、零额度任务入口、15 个缺失操作、列表筛选、语音读回、模型/提示/公告配置。检查断言的是当前问题行为，不能算作修复成功 |
| 临时运营表单复现 | 1 项检查通过真实管理员会话、CSRF、页面读取和 POST 保存模型/提示/公告，三者均返回 303、写入来源为 admin、版本递增；取这些实际保存值执行客户端原 schema/读取函数，三处问题全部复现 |
| 执行客户端原函数/schema | 模型版本被置 null、系统提示 `startsWith` 异常、公告被拒绝、固定非空 cursor 导致自动化重复请求，均已复现 |
| 网页验证方式 | 管理端 HTML/表单脚本、真实表单 HTTP 提交、存储校验、客户端读回和现有管理回归；本轮未做真实浏览器逐按钮点击或布局验收 |

复现材料保留在工作区 [target/virtual-account-audit-20260921](D:/codex-proxy-rs/target/virtual-account-audit-20260921)：`probe.rs`、`admin-probe.rs`、`client-probe.mjs` 及不含真实身份的 API/管理保存样本。临时 Cargo 测试入口已移除，未把诊断用“当前缺陷断言”加入正式测试套件。

## 修复顺序

1. 先处理 F12 的有效订阅，以及 F1–F4 的隔离与执行控制，保证手动发放的订阅和运营设置覆盖实际可执行入口。
2. 同时修正 F5–F7 的网页业务控件、服务端校验和协议生成，使用有效非空数据跑客户端真实读取。
3. 按已开放能力补 F8–F11 的正常客户端操作、分页、状态更新和事件；每项连同该账户管理页的查询展示一起验收。

验收应完整走一遍“网页操作或客户端操作 → SQLite → 客户端读回/实际执行 → 本账户费用和记录”，并验证共享供应账户的两个虚拟账户不互读、不互写。未接通的能力明确保留为未完成，不通过默认空值或成功标记隐藏。
