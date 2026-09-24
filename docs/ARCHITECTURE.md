# Architecture

Implementation is aligned with official Codex:

- repo: https://github.com/openai/codex
- commit: `b412ff32c417f855c2b2d1581b77058eed87c84b`
- date: 2026-09-23T00:45:44Z
- snapshot: `reference/codex`
- constants: `crates/codex2api-version`

Official source is reference only. Do not depend on `codex-rs` crates.

## Process

One Rust process:

- public Codex-compatible Responses API
- admin web UI (one admin user)
- SQLite persistence
- per-account isolated upstream Codex environments

## Crates

| crate | responsibility |
| --- | --- |
| `codex2api-version` | pinned Codex constants |
| `codex2api-storage` | SQLite schema, migrations, admin user, account rows, tokens, sessions |
| `codex2api-accounts` | per-account SQLite identity, installation_id, frozen HTTP fingerprint |
| `codex2api-auth` | ChatGPT OAuth PKCE login, token refresh/revoke, SQLite token persistence |
| `codex2api-upstream` | talk to official Codex servers with official headers/body/stream behavior |
| `codex2api-api` | all incoming Codex-client routes and HTTP/WebSocket handlers |
| `codex2api-admin` | JSON administrator API, cookie sessions, CSRF and safe response DTOs |
| `codex2api-web` | compiled Next.js static export and source freshness validation |
| `codex2api-core` | provider and domain boundaries |
| `codex2api-service` | shared execution authorization and entitlement policy |
| `codex2api` | `lib.rs` composes the app; `main.rs` owns startup, listening and shutdown |

## Module organization

The HTTP entry points delegate to `codex2api-api/src/providers/chatgpt/routes.rs`
and `codex2api-admin/src/rest/mod.rs`, where each method, path and handler is explicit.
Provider routing does not derive incoming paths from upstream endpoint enums.

| Location | Responsibility |
| --- | --- |
| `codex2api-api/src/lib.rs`, `providers/mod.rs` | select the implemented provider adapter |
| `codex2api-api/src/providers/chatgpt/routes.rs` | ChatGPT route inventory, compatibility mounts and body limit |
| `codex2api-api/src/providers/chatgpt/` | ChatGPT protocol adapter, OAuth and client HTTP/WebSocket handlers |
| `codex2api-api/src/providers/chatgpt/access.rs` | ChatGPT consumer OAuth authentication and scope checks |
| `codex2api-api/src/state.rs`, `execution.rs`, `response.rs`, `error.rs` | dependencies, shared execution authorization, response forwarding and API errors |
| `codex2api-admin/src/rest/mod.rs` | public/protected JSON routes and session middleware attachment |
| `codex2api-admin/src/rest/` | typed JSON contracts, suppliers, consumers, subscriptions, model billing, proxy and settings operations |
| `codex2api-admin/src/services.rs` | official account data shared by account details and usage pages |
| `codex2api-admin/src/state.rs`, `session.rs` | shared dependencies and administrator session authentication |
| `frontend/src/` | Next.js administrator and consumer authorization pages, labelled business controls and read-only client records |
| `codex2api-web/build.rs` | verifies frontend source manifest and generates include_bytes asset table |
| `codex2api-upstream/src/lib.rs` | outbound client/pool/protocol exports; no incoming routes |
| `codex2api-auth/src/lib.rs` | OAuth orchestration, persistence and account transport exports |
| `codex2api-accounts/src/lib.rs` | persistent account identity and account-store exports |
| `codex2api-storage/src/lib.rs` | SQLite records, repository and credential-helper exports |
| `codex2api/src/lib.rs` | construct shared dependencies and merge client API, administrator API and embedded web assets |

Handlers adapt incoming JSON requests to services. Next.js reads these APIs and renders typed business controls without server-generated HTML. Official URL and wire-protocol rules remain in upstream/auth;
SQLite persistence remains in storage/accounts. Supplier-specific Rust exports use
the `SupplierAccount` names; removed API Key and HTML-handler exports are not retained.

## Web build and authentication boundaries

The administrator frontend is a Next.js static export with `/admin` base path and trailing slashes. `scripts/build.ps1` / `scripts/build.sh` build and check it before Cargo. The web crate embeds all exported files into the Rust executable; Node.js and source files are not runtime dependencies. Its build script hashes frontend source, configuration and build scripts against `out/.source-manifest.json`, refusing a missing or stale bundle. CI follows the same frontend-before-Rust sequence.

`/admin/api` accepts administrator Cookie sessions only. All mutations require the session-derived `X-CSRF-Token`; consumer OAuth tokens are never administrator credentials. Supplier tokens and password hashes have no response DTO fields. The session, overview, consumer, plan, model, routing and usage DTOs are tested against real REST response fixtures in `crates/codex2api-admin/tests/contracts.json`, which the frontend also reads in its contract tests. Consumer OAuth authorization uses a separate public Next.js page and the ChatGPT adapter's flow-scoped JSON endpoints, preserving PKCE, state and callback validation.

Supply and consumption have separate SQLite entities. A consumer's provider is fixed at creation; execution routes select only same-provider supply. A cleared route retains its revision to prevent stale edits. The global provider model catalog and subscriptions are the authorization source; per-account historical model metadata cannot override access. Client-owned preferences and activity remain read-only in administration. Old API Key credentials are removed by forward migration while historical usage remains attributable.
## Storage

SQLite file default: `./data/codex2api.sqlite`

Startup applies SQLx migrations automatically. Applied migration files remain immutable:
feature removal is a new forward migration, not deletion of migration history. Migration
`0005_remove_usage_errors.sql` removes the reverted error-detail column, including databases
where the column was already manually removed, while retaining all usage rows and indexes.
If the usage table itself was removed while migration history remains, that migration
recreates the empty table and its indexes without changing account or credential tables.

Account identity (`installation_id`, originator, User-Agent, OS/arch), official CLI HTTP fingerprint, and tokens are stored on the account SQLite row / `supplier_tokens`. There is no per-account `$CODEX_HOME` directory. Isolation is a database row, not a filesystem tree.

HTTP fingerprint is application-layer only (same headers Codex CLI sends): `originator`, `User-Agent`, `x-codex-installation-id`, plus this account's cookie jar. It is captured once at account creation and reused; it is not a forged TLS/JA3 device fingerprint.

Admin:

- single user
- username/password
- default `admin` / `admin` created on first boot if no admin exists
- password stored as argon2id hash

## Isolation

Each upstream ChatGPT/Codex account has:

- own SQLite row
- own `installation_id` (column on that row)
- own frozen HTTP fingerprint (`http_fingerprint_json`)
- own tokens (`supplier_tokens`)
- own HTTP client / cookie jar
- own session/thread/turn state

Do not share those across accounts.

## Virtual-account data and management

### 模型配置和图像计费（2026-09-22）

顶部“模型配置”入口为 `/admin/models`，每行一个模型，添加和编辑均使用弹窗。文本模型在弹窗内管理服务档位、上下文阶梯和四类 Token 单价；图像模型管理分辨率档位及美元/张单价。弹窗使用分组表单，正文独立纵向滚动，标题和保存按钮保持可见。整张表单通过模型版本号校验后事务保存。旧 `/admin/settings/pricing` 页面重定向至该入口。

图像价格按用户要求改为屏幕分辨率方式划档。实际宽高取长边，按上限归入 0.5K（512px）、1K（1024px）、2K（2560px）、4K（4096px）、8K（8192px）；横竖图和同档动态宽高使用同一单价。2560×1440 属于 2K，3840×2160 属于 4K。上限在表单选项中明确显示，不按固定宽高组合配置价格。旧尺寸规则在管理员保存后转为档位规则，同档不同价需要管理员确认；旧请求的固定尺寸快照仍按原价结算。

迁移 `0028_model_catalog.sql` 将已有模型价格和历史中已出现的模型归入独立目录，不为未定价模型编造价格。模型支持启用、停用和删除；删除保留禁用标记以拒绝旧客户端的新请求，不删除历史用量或价格快照。套餐模型选择来自已启用目录；客户端模型列表同时应用模型状态和套餐范围，HTTP/Responses WebSocket 的每次生成检查模型状态。

用户确认图像按“实际成功生成的张数 × 对应分辨率单价”收费。`/images/generations` 和 `/images/edits` 读取上游返回的 `data` 图片项，跳过空项和错误项，不以请求的 `n` 作为生成数量。分辨率优先取图片项尺寸、响应尺寸，再取明确的请求尺寸；无法确认尺寸或价格时保留明确的未计价状态。记录只保存分辨率和张数汇总，不保存图片、提示词或 URL。失败且未返回图片的请求不收费。

新请求快照同时保存 Token 和图像价格，旧数组形式的 Token 快照仍可结算。图片按分辨率分别乘以成功张数后合计，仅结算一次；管理员改价、停用、删除均不重算既有请求。实际图像费用进入虚拟账户套餐配置的最多两层额度窗口，用量列表展示图片张数、分辨率和结算状态。服务端测试覆盖管理操作、状态检查、快照和额度；未将解析或测试上游响应作为真实 Desktop 出图证据。

### 统一套餐管理（2026-09-22）

OAuth 管理分为“账户管理”和“套餐管理”。套餐按业务自定义名称、可访问模型、最多两层嵌套额度和原有的免费层访问策略；外层只允许 7 天或 30 天，内层可选且固定为 5 小时。内层用量受外层剩余额度限制，外层从订阅生效开始，内层从首次使用开始，外层重置时清除内层。管理表单不再将 free/plus/pro 等客户端协议标识作为本地套餐分类。`virtual_plans.plan_type` 只保留客户端兼容用途：已有套餐保留原值，新建订阅沿用此前的 plus 默认映射，实际模型权限和额度由本地套餐配置决定。虚拟账户通过 `plan_id` 选择套餐，仅在账户内维护订阅有效期；不再逐账户编辑套餐权益、额度或免费层策略。

可访问模型提供“全部模型”与“仅允许勾选的模型”。候选项来自已启用的模型配置，按模型名称去重；编辑时保留已保存的其他模型，避免静默丢失旧配置。指定模式必须至少勾选一项，服务端校验提交的模型，空选择不能隐式开放全部模型。套餐列表展示具体模型名称；已有客户端目录过滤和推理请求检查读取同一份套餐模型范围。

套餐修改立即适用于使用它的账户，用量仍按各虚拟账户独立统计。停止新分配不撤销已有订阅；删除仍被账户使用的套餐会被拒绝。账户的发放、续期和换套餐继续记录订阅操作，同类型套餐之间切换也会留痕。到期后的免费层访问由所选套餐的免费层策略决定，不影响登录。

迁移 `0027_virtual_plans.sql` 将旧账户实际生效的模型范围和两个额度窗口（原账号额度与套餐额度取较低者）合并为可复用套餐，相同配置共用一条套餐。原免费层规则、账户有效期、身份和消费历史保持不变。旧配置行作为历史保留，运行时和管理端均从套餐表读取，旧账户配置写入入口拒绝写入这些字段。

### 订阅账户运营规则（用户于 2026-09-21 明确）

本系统按 ChatGPT 订阅账户的方式运营每个虚拟账户，区别是由管理员手动发放订阅，替代官方的购买订阅流程。订阅发放、续期、套餐变更、有效期和权益属于服务端业务数据，不能仅作为客户端展示资料。网页操作、SQLite 中的有效订阅、客户端身份/权益响应、请求执行限制必须一致；到期后不再享有已到期订阅的权益。到期后的免费层访问按服务策略处理，与停用账户登录分开。

接口核查与修复必须包含相应运营网页操作和实际记录展示。客户端的会话、偏好、审批和安装状态继续遵守下节的数据归属，不由管理员代写。手动发放本系统订阅不产生官方购买、付款或供应账户订阅变更记录。既有“展示套餐/到期时间”的实现属于待核查旧实现，不能作为完成订阅运营的依据。

### 2026-09-22 Desktop 设置修复

本轮电脑操控、家庭读取、SQLite 写锁和推理强度设置的源码依据与验证见 [Desktop 设置修复](DESKTOP_SETTINGS_FIXES_2026-09-22.md)。`computer_use_policy` 和 `desktop_model_policy` 是管理员负责的可用性策略；客户端仍维护安装、审批及已选推理档位。已有配置读取不再执行写入，连接器目录按 64 项批量保存；会话数据与事件继续事务写入。家庭不存在时返回带 nullable id/role 的成功对象，不能再用 404 表示正常无家庭状态。

### 2026-09-21 审计后的修复实施

逐项状态、验证和未完成能力见 [修复实施记录](VIRTUAL_ACCOUNT_REPAIRS_2026-09-21.md)。当前有效套餐在读取时按有效期计算；账户表单保存产生订阅操作记录。`subscription_entitlements` 管理套餐模型范围和最多两层费用窗口，`subscription_policy` 管理独立免费层访问与同样的窗口规则。到期停止原付费权益，保留登录和历史；免费执行默认关闭，可由运营启用。外层为 7 天或 30 天，内层为可选的 5 小时，客户端按官方 primary/secondary 窗口返回。

云任务和 Realtime 的有限额账户在尚不能可靠计价时明确拒绝执行；不限额执行的实际请求标注未计价。虚拟 MCP 私有操作不再直通供应账户。独立连接器、云自动化和云插件执行器仍未实现，具名错误和操作日志不代表这些能力已完成。客户端正常偏好/置顶操作、模型/提示/公告协议和会话事件的本轮修复，不改变客户端与管理员的数据归属。

### 当前数据归属（用户于 2026-09-21 修正）

以下修正优先于此前“客户端数据全部在网页可配置”的表述：网页可编辑的是服务端负责的身份、额度、目录、公告和功能策略。正在进行的会话、客户端侧栏项目和置顶、用户偏好、浏览器审批规则、引导完成状态、语音选择、安装状态、通知状态及实际业务记录不由管理员填写生成；管理端最多查询已同步的数据。Desktop 本地 `local-projects`、`pinned-thread-ids`、`pinned-project-ids`、`sidebar-project-thread-orders` 和工作区根目录由客户端本地状态服务管理，与账号云项目读取接口不同。

当前管理页把这些入口移入“客户端记录”。旧链接仍能查看，但不能提交修改；保存接口和存储管理入口都检查归属。用户设置响应中的 `settings` 属于客户端偏好，`flags` 是服务可用性，网页只修改经过客户端读取代码确认的 flags。账号策略只开放已确认的工作区开关、测试版策略、第三方 GPT 策略及额度申请入口；既有未知字段保留但不让用户随意新增。功能配置使用已定义的中文业务控件，代码生成 Statsig 哈希、名称及规则标识。协议要求的翻译加载和已实现个人页能力自动提供，移除管理员覆盖客户端语言选择的入口。

同类入口一并修正：个人页展示选择、付款资料、充值偏好、邀请操作记录、家庭关系、授权记录、自动化、通知设置/状态、已安装插件、语音、引导、浏览器审批和旧 TOML 配置包均不再提供管理端编辑。身份资料、额度、模型目录、文件上传限制和公告等服务端配置保留。联系人功能可用性仍可管理，联系人本身不可在此填写生成。会话配置中的实时限制进度不可编辑，保留默认模型及附件限制控件。客户端已有写入接口仍可保存，管理端政策更新不覆盖用户偏好。

迁移 `0026_client_state_origin.sql` 为后续写入记录来源和更新时间。历史记录的来源标为未知并原样保留，不能把旧管理表单写入的数据标成刚采集的真实客户端状态。混合记录显示最近一次写入来源；该标签不推断每个历史字段的创建者。管理端不自动清空本地或云端历史。

### 用户已确认的要求（2026-09-20）

Desktop 接口以已安装桌面端的请求构造、响应读取和后续分支为依据，每个新增或修改请求都要执行实际 Desktop 逻辑验证。CLI 源码、CLI 登录测试或代理端自行编写的响应断言，不能替代 Desktop 的验证。网页管理必须同步提供已定义的业务控件，协议字段及标识由系统生成，不能要求用户填写字段名或 JSON。

虚拟账号是独立的数据与配置主体。绑定的真实账户负责上游消费和认证，不能把真实账户的用量、任务、日志或私人资料当成虚拟账号的数据。仅替换返回 JSON 中的账号 ID、名称或邮箱，不能完成数据隔离。

用户明确要求修正三类问题：客户端收到的数据不是虚拟账号自身的数据；客户端需要的数据或配置在网页上无法查看、配置；新增接口用空响应占位，没有接入实际管理。下述内容是后续实现约束与待修复清单，**不是已完成声明**。

| 数据类别 | 来源与管理要求 |
| --- | --- |
| 私人资料、账号设置、偏好及功能配置 | 在本系统按虚拟账号自行配置，持久化到 SQLite；网页提供对应查看、编辑入口，客户端读取同一份数据。不能因为是虚拟账号就固定清空或关闭功能。 |
| 用量、任务、活动及日志 | 来自虚拟账号自身实际发生的请求、任务和事件；网页可以查看、筛选并追溯。不得用绑定账户的整体统计替代，也不得手工编造实际使用历史。 |
| 会话、项目、置顶、通知等账号资源 | 资源和操作均归属虚拟账号；接通保存、查询及相应管理后，客户端返回实际记录。任务创建不能只返回一个成功标记而没有任务记录和状态。 |
| 模型目录及上游执行能力 | 核对真实协议和可用能力；客户端可见配置在本系统可查看、管理。目录转发不等于已实现虚拟账号配置或会话执行。 |

私人数据可以自行虚拟配置，并不表示已经获得官方资格、建立官方家庭关系、完成付款或创建外部资源。展示资料和外部操作结果必须区分；不得用本地配置伪造外部操作成功。这里只规定已涉及接口的数据来源与管理方式，不额外扩展未要求的业务。

持久化数据以虚拟账号 ID 为归属。更换绑定不重写历史或丢失配置；共用同一真实消费账户的多个虚拟账号不能互读数据。路径、查询参数及请求体中的账号和资源 ID 必须核对归属，不能只校验请求头。

管理网页与客户端接口使用同一服务和持久化数据源。对可配置项提供查看、修改、校验及保存结果；对统计、任务和日志提供实际记录的查看入口。不能只在 handler 中写死 JSON，也不能仅在数据库中存储、却让网页无法查看或管理。重启后仍需保留数据。

空响应只有在**已实现的数据源查询结果确实为空**时才是正常空状态。尚无数据模型、事件采集、资源操作或管理入口的接口属于未完成，不能靠返回 `{}`、`[]`、`false` 或无实际操作的 `success: true` 标为完成。把 501 改成 200/404、让页面不再报错、通过解析器校验，都不能单独作为完成依据。

### 历史排查：修正前的代码缺口

以下为 2026-09-20 修正前的静态核对，保留用于说明问题来源；当前实现以下方“本轮实现”一节为准。

| 位置 / 功能 | 已有行为与缺口 | 后续修复要求 |
| --- | --- | --- |
| `codex2api-api/src/handlers/backend.rs`：个人资料统计 | 已调用 `virtual_usage_summary`；不能据此认为所有客户端统计已隔离 | 保留已有按虚拟账号统计，检查其他统计接口的归属和网页展示一致性 |
| `codex2api-api/src/handlers/desktop_usage.rs`：每日模型 Token | 已调用 `virtual_daily_model_tokens` | 用真实请求记录核对 HTTP/WS、模型及日期口径，与管理页使用同一数据源 |
| 同文件：轮次、插件、技能、积分、额度历史 | 多个分支固定返回空数组或空周期 | 补实际采集、持久化、查询和网页查看；不把请求数冒充轮次、Token 冒充金额 |
| `codex2api-api/src/handlers/backend.rs`：任务、设置、工作区消息、轮次用量等 | 未进入本地分支的端点继续转发，并对成功 JSON 调用 `virtual_identity::mask` | 逐项建立虚拟账号资源归属及数据来源；不能以身份替换代替任务、日志隔离 |
| `backend.rs`：配置包 | 当前 `config/bundle` 返回固定空配置 | 接入虚拟账号配置存储和网页管理，并核对客户端配置协议 |
| `codex2api-api/src/handlers/desktop.rs` | 会话、自动化、公告、代码审查等多处固定空值；浏览器设置和引导状态已持久化 | 保留已有存储能力，补相应管理入口；其他资源按实际数据源实现 |
| `codex2api-api/src/handlers/chatgpt.rs` | 用户设置、自动充值等存在固定返回；追踪事件转发但不建立本地活动记录；已安装插件等需核查账号归属 | 配置由虚拟账号自行管理；客户端活动日志需建立本地归属，不能直接拿供应账户内容替代 |
| `codex2api-api/src/virtual_identity.rs` 与虚拟账号表单 | 存在同步供应账户额度和固定百分比展示两种旧行为 | 不得将其称为虚拟账号实际使用统计；后续厘清额度配置与实际消耗，并同步客户端及网页 |
| `codex2api-admin/src/views/oauth_accounts.rs` | 详情主要管理身份、绑定、展示额度和登录设备 | 补虚拟账号自身统计、任务、日志、私人资料与客户端配置的查看或管理；不能停留在设备列表 |

### 本次客户端缺失接口清单

统一前缀为 `/api/oauth/chatgpt/backend-api`。用户提供的记录覆盖 2026-09-18 至 2026-09-20。已查看本机客户端 `OpenAI.Codex 26.915.4065.0` 的 2026-09-20 日志和安装包读取代码，日志确认相关请求进入 501 未实现分支；安装包证据仅用于当前客户端接口，不更改固定 Codex 基线。

下列 **19 个接口是本次修正的原始清单**。表中记录的是排查时确认的客户端结构；实现状态见下方“本轮实现”。

| 方法 | 相对路径 | 修复重点 / 已核对的客户端结构 |
| --- | --- | --- |
| GET | `/referrals/invite/eligibility` | 虚拟账号资格配置，客户端读取 `should_show` |
| GET | `/celsius/ws/user` | 返回可用的 `websocket_url`；需真实账号隔离的连接、订阅和事件，不能返回无效 URL 或空值 |
| GET | `/amphora/notifications` | 虚拟账号通知记录；客户端读取 `items`、`cursor` |
| GET | `/payments/payment_methods` | 虚拟账号自行配置的资料与管理；客户端读取 `payment_methods`，不得复制供应账户支付信息 |
| GET | `/trusted_contact/enabled` | 虚拟账号联系人功能配置，客户端读取 `enabled` |
| GET | `/amphora` | 虚拟账号自行管理的关系资料，不能固定“无数据”代替管理 |
| GET | `/checkout_pricing_config/configs/{country_code}` | 定价/地区配置来源与网页管理；客户端读取 `country_code`、`currency_config`，不得编造官方定价 |
| GET | `/gift-credits/senders/eligibility` | 虚拟账号资格配置，客户端读取 `eligible` |
| GET | `/accounts/{account_id}/settings` | 必须核对虚拟账号归属；客户端结构要求 `beta_settings`，接入对应配置管理 |
| GET | `/accounts/{account_id}/spend-controls/current-user/monthly-usage` | 虚拟账号的实际统计及已定义的额度规则；不能转发供应账户月消费，也不能以固定零值占位 |
| GET | `/notifications/settings` | 虚拟账号通知配置，客户端读取 `settings` |
| GET | `/pins` | 虚拟账号置顶资源；客户端直接读取数组并调用 `filter` |
| GET | `/aip/first-party/eligibility` | 虚拟账号功能配置；客户端读取 `finances`、`health_eligibility.sidebar_visible` |
| GET | `/gizmos/snorlax/sidebar` | 虚拟账号项目资源；客户端读取 `items`、`cursor` |
| GET | `/system_hints` | 虚拟账号可管理的提示配置；客户端读取 `system_hints` |
| POST | `/conversation/init` | 核对会话归属、模型、上传限制等实际状态；客户端解析会话元数据，不能用空元数据假装已接通会话功能 |
| POST | `/f/conversation/prepare` | 接入该虚拟账号会话准备及后续执行；客户端读取 `conduit_token`，空 token 可被解析不等于功能完成 |
| GET | `/models` | ChatGPT 目录协议与 Codex `/codex/models` 不同；核对实际目录及虚拟账号可见配置，不能直接混用 |
| GET | `/settings/is_adult` | 虚拟账号自己的年龄相关资料配置；客户端读取 `is_adult`、`has_verified_age_or_dob`，本地资料不等于官方核验结果 |

### 后续实施与验收

1. 先核对虚拟账号数据归属，修正供应账户统计、任务和日志被直接透传的路径；保留已经正确的本地统计。
2. 补齐数据模型、采集或配置保存，再接管理网页与客户端读取。已有 `virtual_client_state` 可用于合适的配置，任务和事件需要可追溯的实际记录。
3. 按上表逐项修复，检查客户端实际读取结构及后续调用；未支持的外部执行能力明确记录，不提供虚假的成功结果。
4. 每项至少验证：两个虚拟账号共用同一上游时的数据隔离；换绑和重启后保留历史/配置；网页修改能在客户端读回；实际活动在网页与客户端统计一致；空数据来源真实；跨账号资源访问被拒绝。
5. 修改 Rust 后执行对应 crate 的 `cargo check` 和有意义的回归测试。报告区分编译、接口测试与客户端运行验证；只完成路由注册、空响应或结构测试时不得宣称功能修复完成。

### 本轮实现（2026-09-20）

- 已删除“同步绑定账户额度”和固定百分比展示：迁移 `0019` 删除三个旧字段，保留账号、设备、身份及历史。不得恢复这项功能。虚拟额度只从 `quota` 配置及本账号实际 Token 账本计算，HTTP、SSE、WebSocket 使用同一来源。真实账户只承担上游执行；其额度不混入客户端数据。
- `virtual_management` 存储模块提供配置清单、默认值持久化、结构校验、版本检查及事件写入；`/admin/oauth/accounts/{id}/manage` 提供对应查看与编辑，并展示实际统计、任务、会话、活动和请求日志。配置不会只停留在 handler 或数据库中不可见。
- 管理 UI 已改为账号详情顶部六个页签。资料、偏好、开关和集合用普通表单编辑，统计复用已有指标卡与每日 Token 图，任务、会话、日志和设备用表格；日期按浏览器本地时间显示。`/manage` 保留配置提交兼容入口，不再展示 JSON 编辑框或数据转储。必须保留版本冲突检测及未修改字段，不能只改变外观却破坏保存逻辑。
- 私人资料、资格、家庭、通知、支付资料、地区定价、项目、置顶、提示、年龄、ChatGPT 模型、功能开关、配置包、用户偏好等接口读取虚拟账号自己的配置。未配置地区价格明确提示管理页配置；未设置家庭资料返回客户端支持的“无家庭”状态。默认关闭的功能可以在网页配置，不是不可修改的常量。
- WHAM 任务列表不再向供应账户查询全部任务；创建结果建立归属，详情和兄弟轮次必须命中本账号记录，换绑后旧任务读取已保存快照。客户端请求日志按虚拟账号和设备保存，仅包含方法、路由、状态、耗时和时间，不含查询参数、凭据或正文。
- 统计事件在本地保存允许的元数据并去重，不转发成供应账户活动。每日 Token 继续读取真实推理账本；轮次、插件、技能读取实际客户端事件，网页可查看原始活动元数据。Trace 正文不保存，接收请求本身有本地日志。
- `celsius/ws/user` 使用本地、账号隔离的持久化事件通道；签名凭据仅作用于该 WebSocket 路径并关联有效设备凭据。通知配置改变会写入客户端结构的通知事件；已处理状态也保存在同一配置中。断线后可以按 offset 继续读取，设备撤销后关闭连接。
- ChatGPT 会话初始化使用本地模型和限制配置；原生 prepare/发送/恢复/停止独立于 Responses 转发。上游 conduit token 只存在独立的 `virtual_conduits` 表，客户端持有本地替代凭据，按虚拟账号、执行账户和有效期校验。会话归属仅在真实上游响应返回 ID 时记录，不能用自造 ID 冒充已创建会话。
- 迁移 `0018` 增加资源、事件和请求日志表，`0020` 增加 conduit 凭据表。没有清除旧的“未实现接口”诊断历史，也没有在本轮直接迁移或重启正在运行的服务。

### 实际能力与验证边界

#### 2026-09-21 支持接口、配置表单和客户端记录审计

本轮逐项读取安装包 `26.915.4065.0` 的请求、读取及失败分支后实现：

| 请求 | 安装包证据 | 服务行为 |
| --- | --- | --- |
| `POST /ces/v1/telemetry/intake` | `src-C3YaUE83.js` 的 `F8.send/I8` 发送 `text/plain` NDJSON、`x-request-id` 和 `ddforward`；2xx 忽略正文，5xx 重试 | 校验批次后保存诊断类别元数据；不保存正文、栈、用户提示或任意 metadata。重复批次只增加接收次数；成功返回 204。未认证上报不靠正文中的 user ID 归属账号。 |
| `POST /ces/v1/rgstr` | 内置 Statsig logger 批次 `events`，支持 `gz=1`；renderer `h6c/g6c` 仅为该 CES 入口请求附加认证 | 原格式及受大小限制的 gzip 均可读取，认证存在时严格验证并保存归属；匿名请求明确保留匿名来源。持久化成功后返回 SDK 所需成功结构。 |
| `POST /v1/sdk_exception` | 内置 `ErrorBoundary._onError` 发送 tag、exception、info 和 SDK 元数据，忽略响应正文 | 保存允许的诊断类型和 SDK 版本，不保留错误栈/任意配置；返回 204。 |
| `POST /v1/initialize` | 内置 NetworkCore 支持 `se=1` 反序 Base64；普通刷新读取完整评估，live overlay 使用 `previousDerivedFields`；第一次 live cursor 不保留 `full_checksum` | 从认证 bootstrap 的 `full_checksum` 和 `derived_fields` 生成签名快照，校验账号/设备/过期和撤销后查询当前 SQLite 配置。SDK 实际回传这两个协议字段，不能仅信任 body user ID。返回完整及 live overlay 结构，补齐 SDK 实际要求的 `live_entity_names.experiments`；真实 SDK 刷新验证通过。 |
| `GET /mcp-app.html` 与必要 `/assets/...` | `Y0e/H$/B$` 读取真实 HTML、提取启动脚本，再由 `codex-sandbox` 处理器保留 CSP 和 Permissions-Policy | 固定公开资源源站，转发所需请求头但不传认证/Cookie，真实下载后持久化缓存，保持 HTML、脚本和安全头。查询字符串不能选择任意上游地址，不返回假 shell。 |
| `GET /codex-app-prod/windows-store-update.json` | bootstrap `MT/NT/jT/PT` 检查 HTTPS、schemaVersion、buildVersion、Store 产品和包身份 | 从官方公开源取得真实清单，不编造版本、安装结果或“已是最新版”。当前 HTTP 代理地址仍会触发原版客户端 HTTPS 校验，完整更新检查须使用 HTTPS 服务地址；这项限制没有通过修改客户端检查绕过。 |
| `POST /wham/remote/control/server/enroll` | 上轮实际原生 app-server 验证，数据库已有 200 | 用户列表中是此前 03:46 的 501 历史，本轮保留历史并在诊断页展示后续实际成功响应时间；不把主机注册宣称为完整控制端配对。 |

“设置 / 客户端支持”提供出站代理选择、公开资源缓存时间和诊断元数据保留开关，并用表格展示匿名/认证诊断及资源缓存。没有提供客户端更新状态的人工设置。迁移 `0025_desktop_support.sql` 保存诊断批次和资源内容；配置刷新、private data 和供应账户信息均不透传到官方分析服务。

验证：24 项 API/OAuth 回归、4 项管理回归、5 项迁移回归通过；安装包 SDK 请求、编码压缩、持久化去重、签名刷新、跨账号和撤销检查已执行。原版 Desktop GUI 通过自身 renderer 传输请求上述支持接口，同时复验个人页和语言设置。管理页浏览器检查覆盖服务策略与只读记录，未发现任意字段编辑器或水平溢出。实际 Rust 服务成功下载公开 HTML、更新 JSON 和所需脚本并缓存；未把已下载 JSON 视为 HTTP 地址下通过原生更新检查。

本地服务已备份后更新到迁移 26，既有账号、设备、用量、配置及 501 历史保留。原版 Desktop 的旧 bootstrap 缓存需重新加载才能获取新的刷新签名；没有清空用户登录或客户端数据。

#### 2026-09-21 Desktop 请求出口统一路由

04:31 的实际日志中，普通轮次和后台标题生成均直连 `https://chatgpt.com/backend-api/codex/responses` 并收到 `unauthorized_unknown`。用户当时选择 `model_provider="custom"`，该 provider 没有 `base_url`；原生客户端会按认证模式回退官方地址，原启动参数的 `openai_base_url` 只覆盖内置 provider。之前只验证登录、个人页和额度，没有验证这一推理分支，不能据此认定全部请求已接管。

地址规则现集中在 `tools/desktop-proxy/AddressHook.cjs` 的 `proxyPolicy`，由 Electron 会话、net Fetch/request、Node Fetch/HTTP/HTTPS/HTTP2、WebSocket 和 Worker 传输出口共用，重定向同样重新经过映射。保留方法、正文、查询及认证，不枚举业务接口；域名范围来自安装包实际服务地址。其他站点和本地 OAuth 回调不变。旧登录响应、浏览器业务函数和 bootstrap 工作区路由补丁已移除，只保留自定义服务的认证地址检查适配和原生传输边界适配。

Rust app-server 不经过 JavaScript 网络层。启动前用客户端自己发现的原版运行时短暂读取有效配置，向正常原生进程传入地址覆盖；共享 JSON-RPC 发送边界处理运行时追加的配置。未设置端点的自定义 provider 也会映射到本服务，保留 provider ID、模型、凭证、权限和其他设置，不写用户配置、不使用 shim。普通会话、恢复和后台生成使用相同规则。当前 Windows 本地原生传输已验证；WSL 包装启动明确报不兼容，不静默遗漏地址覆盖。

`Test-DesktopProxyHook.cjs` 执行安装包原生启动方法并检查共享发送边界；`Test-DesktopRequestRouting.cjs` 用实际本地 HTTP/WebSocket 服务验证方法、正文、认证头、307/303 重定向和 Worker 出口，检查原有取消规则及 Worker 显式参数保留。Worker 使用临时地址预加载文件，正常退出时删除；预加载仍排除启动器的 inspector 断点。新版自包含 EXE 已重新发布至 `target/desktop-proxy/publish`。

原生集成测试使用 `custom` 且缺少 `base_url` 的配置，实际推理到达本地测试服务并收到其未绑定账户的明确错误，登录、刷新、主机注册和原版 GUI 资料/语言回归同时通过。另用默认凭证和已绑定账号执行两次实际原生请求：`gpt-5.6-sol` 返回 `OK`，`gpt-5.6-luna` 标题请求成功，SQLite 中两条 HTTP Responses 记录均为 `completed`，provider 仍为 `custom`。这些是实际推理证据；GUI 回归验证的是启动和页面，不把直接原生请求描述成 GUI 已发送任务。随后新版 GUI 启动默认资料的原版 Desktop，05:05 日志显示原生进程启动、账号读取成功。

#### 2026-09-21 主机注册、个人资料与界面语言

本机 `OpenAI.Codex 26.915.4065.0` 的 03:46 日志确认 `remoteControl/enable` 在 `/wham/remote/control/server/enroll` 得到 501。现在该请求按虚拟账号和登录设备注册本地主机；迁移 `0024_remote_servers.sql` 保存安装标识、主机信息、稳定服务器/环境标识及有期限的凭据哈希。刷新轮换凭据；专用 WebSocket 校验主机凭据和安装标识，维护连接租约，设备撤销或令牌失效时发送关闭帧。管理页“设备”展示实际注册及连接状态，通过下线所属登录设备撤销主机。已用安装包实际 app-server 的 `remoteControl/enable`、`remoteControl/status/read` 验证注册、响应解析及连接。**控制端授权、配对及远程消息转发仍未接入**；主机注册不是完成远程控制的声明，不为不存在的控制端确认消息已送达。

个人资料有两套实际读取路径：设置页的 `I$s/B$s` 受 `wKs` 的可见性开关控制，读取 `/wham/profiles/me`；独立新版页的 `T$s/O$s` 读取 `/profiles/me/page` 或自己的 `/profiles/{username}/page`。现两者都读取本虚拟账号资料、用量和已采集活动；新版页输出客户端需要的身份、私有可见性、分栏设置、统计和日/周/累计活动结构。名称、简介、头像形状和栏目设置通过既有管理控件维护；客户端文字/栏目修改回写同源配置，跨资料记录的修改在一个事务内校验版本。头像上传、展示作品编辑等未涉及的操作仍不宣称完成，不用伪造成功替代实际保存。

语言设置的根因是 renderer `Wal` 即使读到 `localeOverride`，仍要求 Statsig layer `72216192` 中 `enable_i18n=true` 才加载翻译。新增持久化“桌面显示设置”，为管理员提供多语言界面和个人页开关；bootstrap 用安装包实际 SDK 的哈希格式生成语言 layer、个人页 gate 和菜单 layer。语言选择本身继续走原版客户端设置流程，不修改 renderer、app-server 业务逻辑或生产启动配置。

`Test-DesktopProfileContract.cjs` 执行安装包请求构造及新版资料读取函数；`Test-DesktopBootstrapContract.cjs` 使用内置 Statsig SDK 验证真实 layer/gate 读取。`Test-DesktopProfileUI.cjs` 在临时测试账号中通过原版 GUI 控件打开个人资料，核对名称、用户名、数据库用量与活动图，并验证英文切换到简体中文、重新加载后仍选中简体中文。测试专用 renderer 调试参数仅存在于 `--test-launch` 路径，测试退出时终止该实例；生产启动器保持原有默认凭证、数据目录和运行时策略。

验证：22 项虚拟 OAuth/API 回归、3 项管理回归、5 项迁移回归、1 项独立原生 Desktop 集成测试通过；`cargo check -p codex2api` 通过。本地运行服务已更新至迁移 24，更新前完成 SQLite 一致性备份并确认没有进行中的推理；更新前后账号、设备、用量记录数量一致。随后用现有生产 GUI 启动默认资料的原版 Desktop：04:24 日志账号读取成功，实际 enroll 返回 200、主机在线，个人资料请求返回 200。

用户配置中的 `mcp_servers.cua_repl.type` 和 `mcp_servers.node_repl.type` 已备份后定点移除，修改时按 TOML 解析结果确认没有变更其他字段。检查期间另一轮文件整体重写曾把两项带回，已在保留其余新内容的前提下再次清理；不能据此确定历史写入者。默认资料的原版 Desktop 再次启动后，两项仍不存在。未添加自动清洗用户配置的启动器行为。

#### 2026-09-21 Desktop 启动器简化为地址 hook

用户已明确选择不隔离客户端凭证，并要求可编译的 GUI：使用 `tools/desktop-proxy` 中的 C# / WinForms 程序，单 EXE 内嵌地址 hook。原 `Start-ChatGPTProxy.ps1`、独立 Node 启动器、`CodexProxyShim` 和复制/profile/shim 测试均已删除。GUI 可更换服务器、保存/导入/导出设置，自动读取当前安装包清单定位主程序，也支持手动选路径；不绑定本机用户名、固定版本目录或服务器地址。原版 Desktop 的默认凭证、界面数据与 Windows 运行时定位机制继续工作；本项目不复制和接管它。历史隔离目录中的登录资料不自动复制进默认目录。

仅对启动进程指定 Desktop 已提供的后端、推理、授权发行方、刷新和撤销地址覆盖入口。以自定义域名的 `/codex/desktop-auth?authorize_url=...` 返回授权链接，实际 renderer 在所有包装选项下保留该域名。实际登录验证发现 native 回调服务的官方托管成功页仍会跳回 ChatGPT，因此仅关闭 `useHostedLoginSuccessPage` 这个官方跳转选项；保留 `codexStreamlinedLogin`、PKCE、state、回调和凭证存储。刷新和撤销使用同一自定义服务。临时调试端口在 hook 安装后关闭，并清理启动器自己添加的断点继承。未知版本若不匹配 hook 结构，GUI 报告兼容性错误并结束本次新进程，不静默跳过。

删除了客户端账户数据改写 hook。实际 native OAuth 登录要求 `/wham/accounts/check` 返回后端 URL，Desktop 的账户页解析器接受 `NO_CONSTRAINT`；由服务端根据实际客户端 UA 格式输出对应结构，不在客户端替换数据规避校验。这不改变服务端虚拟账户与供应账户的数据隔离。

实际验证已用新的 C# EXE 启动安装包原版 Desktop：原生登录完成，窗口账户读取成功，账户/设置/额度请求到达本地测试代理，临时调试端口关闭，进程重启后身份保留且令牌刷新成功。测试使用临时账号和测试目录，没有切换或注销用户的日常登录。编译输出 0 警告、0 错误；图形界面实际渲染及设置保存、更换地址、安装发现测试通过。正式发布产物为自包含单 EXE，运行不依赖 Node.js 或独立脚本。源码合约测试仍可由 Node/Python 开发工具执行。

补充登录地址出口修复：不能仅依赖某一路登录 RPC 改写 `authUrl`。hook 同时在实际打开链接的 Desktop browser bridge 和 Electron `shell.openExternal` 处检查授权 URL，把 renderer 再次产生的 ChatGPT 包装地址改为当前配置服务器。合约测试直接执行安装包原始登录包装和 browser bridge，在内置、外部浏览器两条分支验证最终目标；更换域名、端口和代理路径后仍使用新配置，并保留 state/PKCE/回调参数。原生登录集成测试禁止跟随自定义服务及本地回调以外的重定向。

#### 2026-09-21 模型计费、周期额度与断流修正

2026-09-21 用户明确要求：移除总费用限制，只保留 5 小时和每周额度。每个虚拟账号仅配置这两个周期的费用上限，以模型价格结算的 USD 计，任一达到即限制新的推理请求。账号的“客户端配置 → 虚拟账号额度”分别设置两项金额，“用量统计”显示两个周期的使用比例、剩余额度、重置时间；已结算累计费用只是统计，不能参与限额判断。设置页的“模型计费配置”管理共同使用的模型价格。禁止再次加入累计总上限。

周期沿用此前本地 UTC 固定窗口边界：Unix epoch 按 18,000／604,800 秒分段，以请求开始时间归入窗口。这是本地虚拟额度政策，不声称真实账户也使用同一重置锚点。迁移 `0022_spending_windows.sql` 补齐两个周期金额字段（留空表示不设限）；`0023_remove_total_spending_limit.sql` 删除旧总额上限，保留两个周期配置、价格和全部费用记录，并增加 revision。旧表单不能恢复已移除的限制。旧 Token 配置仍保留，不把 Token 数值猜测换算成美元。

默认价格核对于 2026-09-20 的 [OpenAI API 定价](https://developers.openai.com/api/docs/pricing)，覆盖当前 GPT-6 Astra、GPT-5.6 Sol（及 `gpt-5.6` 别名）、Terra、Luna 的 Standard、Fast、Flex 及长上下文价格。`priority` 与 `fast` 对应快速档；默认/auto 对应标准档。输入超过 272,000 Token 时整条请求采用长上下文价格，阈值依据各模型官方页面，例如 [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra)。这些是可编辑的本地价格快照，运行时不偷偷联网覆盖管理员设置，也不更新项目固定 Codex 基线。

计费公式为 `(普通输入 × 输入价 + 缓存读取 × 缓存价 + 缓存写入 × 写入价 + 输出 × 输出价) / 1,000,000`。普通输入是总输入减去两类缓存 Token，依据 [官方缓存计费规则](https://developers.openai.com/api/docs/guides/prompt-caching)；思考 Token 包含在输出中，不再加收一次。价格使用整数微美元/百万 Token，账本保存整数纳美元，整条请求最后舍入一次。优先采用上游实际返回的模型和服务档位，在请求开始时保存的价格快照中选价。

迁移 `0021_model_billing.sql` 新增价格表、请求价格快照、费用及计费状态。旧 Token 设置保留为 `legacy_token_quota`，不再执行，不能擅自把 100 Token 换成 100 美元；新金额默认留空、不设限，需要管理员设置。升级前记录标明“升级前未计费”，不以当前价格追溯扣费。之后更换绑定、修改价格或进程重启不会重算已结算费用；重复完成回调不会重复扣费。

仅对有实际 Token 上报的 Responses/compact 模型请求结算。未上报完整用量、未知实际模型、异常缓存计数及尚无计价支持的请求明确标为费用未确定，不写成零费用。有限额账号在请求模型/档位缺价或请求类型尚不能计价时返回 `model_pricing_unavailable`，管理员可在价格页补充模型。未完成的并发请求没有预扣，已开始请求可能使周期费用超过限额；超过后拒绝新的推理请求，不能宣称严格的预付费硬封顶。

`/wham/usage` 的 `allowed` / `limit_reached` 仅由两个周期决定。配置了周期额度时返回 `primary_window` / `secondary_window`，窗口仅包含整数 `used_percent`、`limit_window_seconds`、`reset_at`、`reset_after_seconds`；留空的周期返回 null。身份字段 `account_id` / `user_id` 来自虚拟账号。客户端响应不混入本地管理用的 `billing` 或窗口美元字段。HTTP 配额头及 SSE/WS `codex.rate_limits` 事件读同一数据源，继续剥离供应账户额度。月上限接口仍走不可用分支，7 天窗口不是自然月上限。

安装包 renderer 的 `NFa/JFa/IFa` 读取周期和重置时间，`SFa/jFa` 判断阻止状态，`wFa` 校验当前账户身份。`Test-DesktopQuotaContract.cjs` 执行这些真实函数；`Test-DesktopRateLimits.py` 执行安装包内置 app-server 的 `account/rateLimits/read`，输入实际本地 API 响应。此次测试实际复现了浮点 `used_percent: 100.0` 导致 `expected i32` 解码错误；已改为整数百分比，金额限额判断仍用整数纳美元，不靠显示比例判断。不得再以 renderer 测试通过替代 native 读取验证，或删掉窗口规避解析错误。

客户端验证要求：外层或内层达到上限时，实际 API 的 HTTP 与 WS 握手均返回 429；实际 Desktop renderer 和内置 app-server 必须成功读取相同官方 primary/secondary 响应。回归覆盖 7 天／30 天外层、可选 5 小时内层、内层受外层剩余额度限制、外层重置清除内层、跨账号隔离、重启保留及管理表单。这些是本地验证，不能声称全部官方接口或整套 GUI 已完成端到端验收。

移除总限制后的实际验证：上述相关回归 78 项通过，2 项按原配置忽略；安装包路径和内置 app-server 路径已启用，两个周期都经过真实 Desktop 读取验证。迁移测试确认旧总上限为 0 时不再误拦请求，两个周期金额及已有费用记录完整保留；管理接口拒绝旧总额字段，界面不再显示总限额控件或进度。运行中的服务仍需重新构建并重启以应用代码与迁移。

此次用户断流日志对应 06:23–06:24：当时虚拟账号两个 Token 限额都为 100，已有 20,728 Token 用量；HTTP 返回 429，WebSocket 升级后额度检查失败直接丢弃连接，导致客户端显示 `Connection reset without closing handshake`。现握手前检查额度，已连接情况下的每次生成也检查额度；保留结构化错误和状态，发送关闭帧。正常完成的 WS 用量先持久化再交付完成事件，避免下一轮早于上一轮结算。达到限额不会以正常完成冒充成功。

`POST /wham/analytics-events/events` 已接入本地虚拟账号活动采集，与 `/codex/analytics-events/events` 共用存储。Desktop renderer `o0c → c0c/l0c → u0c` 发送 `{events:[{event_type,event_params}]}` 并忽略成功响应正文；其应用查看、评价、操作、变更事件保存允许的标识、状态、评价和时间，去重且账号隔离，不转发供应账户。diff、任意 metadata 和仓库内容不留存。网页活动页以中文事件名称、详情、会话和轮次列展示。

验证脚本 `Test-DesktopAnalyticsContract.cjs` 执行安装包的实际上报构造函数；`Test-DesktopQuotaContract.cjs` 执行实际额度/阻止/重置判断函数；`Test-DesktopStreamError.py` 启动安装包自带 app-server，在本地真实 WS relay 上接收限额错误。该版本 Desktop 将 429 显示为限流重试耗尽，而不保留自定义错误文案；验证确认不再产生关闭握手缺失错误。以上是本地合约与客户端运行测试，不是已部署声明，也没有修改 Desktop 安装包或业务逻辑。

本轮相关单元、接口、管理与迁移回归共 75 项通过，2 项按原配置忽略；已设置安装包和 bundled app-server 测试路径执行上述新合约检查。主程序及 API、Admin、Storage 的 `cargo check` 通过。生产数据库和运行中的代理尚未迁移或重启，新价格、管理页和错误处理需在重新构建并重启代理后生效。

#### 2026-09-20 “Starting your task”完整启动链路复查

之前仅凭启动身份校验通过，不能断言整个创建流程已恢复。后续实际 Desktop 复现确认身份和执行配置已经就绪，阻塞发生在本地 `worktree-shell-environment` 请求等待 Git 工作线程返回。页面的“Waiting for worktree setup”也是无会话 ID 时的占位文字，不能单凭文字认定服务端会话接口出错。

根因在本项目 `DesktopProxyHook.cjs` 的启动环境：`--inspect-brk` 被工作线程从 Node 原生参数继承；关闭主进程 inspector、仅修改 JS 的 `process.execArgv` 都不能解除这种继承，且 Electron 的该数组可能不显示调试开关。启动器现在为默认 Worker 参数显式排除自己添加的断点，保留客户端显式选项，不修改 Desktop 的业务函数、不绕过工作区准备、不修改安装包。`Test-DesktopWorkerStartup.cjs` 使用真实 Worker 复现旧启动断点和修正后的消息返回。

原版 Desktop 运行验证：独立测试实例日志 `2026-09-20T06:19:02.138Z` 的 `thread/start` 成功（368 ms），`06:19:02.753Z` 的 `turn/start` 成功（15 ms）；服务端 `06:19:04` 实际记录 WebSocket Responses 调用，其中一条已完成。测试使用临时 Desktop 数据目录，未结束用户正在使用的实例。这才是“不再卡在创建前且实际发出请求”的证据。

同时修正已安装插件接口：实际 Desktop 日志报告 `missing field pagination`。服务端为虚拟账号记录生成 `pagination.limit` 和 `pagination.next_page_token`，支持 `pageToken`，兼容已保存的旧配置；分页标识不开放为网页配置项。`Test-DesktopInstalledPlugins.py` 将实际接口响应交给安装包原始 app-server 的 `plugin/list`，复现旧响应错误并验证新响应解析完成，没有修改该程序或以其源码代替运行验证。

Desktop 挂载后的 beta 身份更新逻辑还会比较 `custom.desktop_app_beta_enabled`；服务端按启动请求生成该布尔值（未启用时为 false），避免缺字段触发不必要的二次身份拉取。bootstrap 测试已改用 Desktop 自带 Statsig SDK 初始化响应，并执行实际 beta 更新判断；这项验证与工作线程修复分别报告，不能混为同一根因。

2026-09-20 补充修正：

- `GET /referrals/invite/tracking`：安装包 renderer 的 `shs` 和 `chs` 使用 `program_id`、`period`、`limit`、`cursor`，读取 `items` / `cursor` 并按非空邮箱计数；支持 `this_month`、`past_90_days`。接口查询本虚拟账号持久化的邀请记录，按活动和 UTC 日期筛选、排序、分页，最后一页返回空游标。网页“客户端配置 → 邀请记录”提供邮箱、活动、状态、时间及可选链接，邀请标识由服务端生成。记录维护不发送邮件，不返回可重新发送的虚假能力。
- 创建会话卡在 `execution-config-loading`：本机 Desktop `main-LM8MUIFp.js` 的 `cT`/`lT` 会将启动配置身份与当前登录主体匹配，renderer 的 `r6c` 发布该身份，`ydn` 在执行配置未就绪时停止创建。旧 bootstrap 只返回 `userID` 和邮箱，缺少 `customIDs.account_id`、`custom.auth_method`；HTTP 200 不代表这条客户端链路成功。现由 OAuth 虚拟身份生成账户和认证字段，将请求的 `stable_id` 保留为本设备的 `customIDs.stableID`，同步构造 `evaluated_keys`；`has_updates`、时间及身份字段自动生成，不让管理员手填或关闭。
- `Test-DesktopBootstrapContract.cjs` 与 `Test-DesktopReferralContract.cjs` 直接读取本机 `app.asar`，分别执行 Desktop 的身份校验/执行设置读取以及邀请列表/翻页/计数函数；输入是集成测试得到的实际代理响应。旧启动响应可以复现未就绪，修正后通过。运行相关接口测试时通过 `CODEX2API_TEST_DESKTOP_ASAR` 指定安装包；没有执行这些脚本时不能声称已完成 Desktop 合约验证。
- 启动配置在桌面进程中缓存。更新代理后须重新打开 Desktop 获取修正后的配置；这项修复不要求用户清空身份或重新创建虚拟账号。本轮未发起真实上游模型会话，不将上述组件验证表述为完整会话端到端验证。

新增的三个客户端接口已按逻辑核对：

| 接口 | 客户端证据与实现 |
| --- | --- |
| `GET /connectors/directory/list` | 固定参考的 `connectors/src/lib.rs` 明确此接口为分页公开目录，与 `list_workspace` 分开。保留 `token`、`external_logos`，响应读取 `apps` 和 `next_token`（兼容 `nextToken`）；通过现有独立执行账户传输读取公开目录，不混入工作区私有目录。目录读取结果按虚拟账号保存并在资源页显示。 |
| `GET /aura/site_status` | 本机桌面 `main-LM8MUIFp.js` 的 `NBe`/`LBe` 及 renderer 的 `ifn` 均读取 `feature_status`；`agent=true`、`page_content=true` 表示阻止对应功能。固定请求官方接口并保留实际布尔策略，不用默认 false 掩盖失败；仅传站点 URL 和请求来源，不传本地 thread/turn 身份。仅保存域名、策略和查询时间，在日志页查看，不保存访问 URL 的查询内容。 |
| `POST /sentinel/heartbeat` | 本机 renderer 的 `X_l` 无请求正文、忽略响应正文，每分钟调用。接口要求本地 OAuth，更新对应虚拟设备最近使用时间，返回 204；请求日志可查看心跳，不转发真实账户，也不伪造 Sentinel 认证凭据。 |

三项相对路径均位于 `/api/oauth/chatgpt/backend-api` 下；失败保持真实错误，诊断历史不清空。

配置与数据隔离不等于实现所有外部产品功能。付款、购买积分、站点发布、自动化执行、GitHub 代码审查采集和完整官方额度历史没有本地业务执行器；不能通过配置开关、空集合或从供应账户复制数据宣称这些业务已完成。相关实际历史只读取本账号记录，缺少采集源的历史仍是不完整的。轮次金额估算明确不可用；月金额上限使用客户端已有的不可用分支，因为本系统当前设置的是 Token 限额。

额度仅使用 UTC 固定 5 小时／7 天窗口，计量单位为模型价格结算后的费用。禁止因变更计费方式删除客户端所需窗口，也禁止恢复总费用限制。

验证包括迁移兼容、管理页会话/CSRF/校验/版本冲突、账号隔离、换绑与重启保留、实际活动去重、WebSocket 事件及设备撤销、分片 SSE 额度替换，以及原生会话响应的归属记录。相关 crate 的 `cargo check` 通过，相关单元与集成回归共 93 项通过。

另外使用安装包 `OpenAI.Codex 26.915.4065.0` 中的 CLI（`0.155.0-alpha.9.2`）完成独立测试：登录、配置要求读取、进程重启后身份保留、令牌刷新均通过，共计 94 项。Windows 不允许直接执行安装目录程序，因此按项目现有启动方式复制 CLI 到 `target/desktop-client-check`，校验 SHA256 一致后运行；测试使用临时账号和数据库，没有启动 GUI、修改安装包或升级代理固定基线。测试的功能配置读取使用实际本地 handler，已移除旧的上游 bootstrap 模拟覆盖。

真实账号的原生 ChatGPT 上游执行、第三方客户端全部界面和外部业务尚未做在线端到端验证。任何后续报告都须保留这些区别。

## Upstream identity (must match official Codex CLI)

From `reference/codex` at the pinned commit:

- originator: `codex_cli_rs` (constant)
- User-Agent formula (official `get_codex_user_agent`): `{originator}/{CARGO_PKG_VERSION} ({os_type} {os_version}; {arch}) {terminal_token}`
- UA version token is the packaged release `0.156.1` (constant)
- OS / arch / version / terminal are rolled once per account from official `os_info` + terminal-detection value sets, then frozen on that account row. Same account always sends the same UA. Do not read the proxy host.
- ChatGPT Codex base: `https://chatgpt.com/backend-api/codex`
- Responses path: `/responses`
- OAuth issuer: `https://auth.openai.com`
- client_id: `app_EMoamEEZ73f0CkXaXp7hrann`
- token URL: `https://auth.openai.com/oauth/token`
- headers include `originator`, `User-Agent`, `Authorization: Bearer`, `ChatGPT-Account-ID`, `x-codex-installation-id`
- dynamic ids (`session-id`, `thread-id`, `x-client-request-id`, turn metadata) are per-request, generated inside the account context

## Public API

Codex clients should be able to point `base_url` at this proxy with `wire_api = "responses"`.

`codex2api-api/src/lib.rs` is the authoritative route inventory:

- `/v1`: Responses/Guardian HTTP and WebSocket, models, search, images, memory summaries and realtime.
- `/backend-api/codex`: compatibility mount of the same Codex routes.
- `/backend-api/wham`: account checks, profile/config/settings, messages, usage details, reset credits and cloud tasks.
- `/wham`, `/api/codex`, `/v1/api/codex`, `/v1/wham`: compatibility mounts of the WHAM routes.
- `GET /v1/usage`: short alias for WHAM usage.
- `GET /healthz` and `GET /version`: unauthenticated process metadata.

Public clients log in as a virtual consumer account through OAuth with PKCE. The
access token is scoped to that consumer and its fixed provider; the execution
service resolves its separately configured supplier route. Client API Key issuance
has been removed.

## Admin

- `GET /admin/login/` static sign-in page
- `POST /admin/api/login` username/password JSON request
- HttpOnly administrator cookie session; mutations require the session CSRF token
- pages to authorize suppliers, manage virtual consumers and subscriptions,
  configure models and execution routes, and inspect account records and usage
- client-owned preferences, sessions, approvals and history remain read-only in admin
- default credentials: `admin` / `admin`

## Client usage ledger

- `codex2api-api/src/usage.rs` observes incoming billable Codex HTTP requests and each
  Responses WebSocket `response.create`. WebSocket `generate=false` warmups are excluded.
- `codex2api-storage/migrations/0003_usage_records.sql` stores account/key label snapshots,
  endpoint, model, requested reasoning effort, official token counters, image size,
  first-body-byte latency, total forwarding duration, request timestamp and completion state.
- SSE/JSON bytes are forwarded unchanged. Counters are read from upstream completion events;
  missing values remain NULL. Input includes cached input; output includes reasoning tokens.
  Search usage displays a dash; image requests display their size.
- First-byte timing begins when the handler receives an HTTP request (or a WebSocket generation)
  and ends at the first upstream body bytes/event. Total time ends at HTTP stream completion or
  a WebSocket terminal event. Internal auth retries do not create extra client request records.
- The record is inserted before forwarding; completion is persisted asynchronously. Client
  disconnects finalize interrupted records, and process startup marks previously unfinished rows
  interrupted. No historical usage is inferred from the official aggregate profile.
- `/admin/usage` provides session-protected filtering by consumer or historical source, supplier name, model and UTC
  time range, with browser-local time display and pagination defaulting to 20 rows (10/20/30/50 selectable). Existing official quota
  and daily-profile charts remain separate from this locally collected request ledger.
- Only client traffic is recorded; admin reads, key/token secrets, prompts, response text and
  image content are not stored in the ledger.

### Supplier communication health and cached quota

Supplier enable/disable intent remains in `supplier_accounts.status`. Migration 0034 stores sticky communication failures separately in `supplier_health`; admin DTOs expose active, disabled, or error. Network failures, terminal authentication failures, upstream 5xx, HTTP timeouts and broken upstream streams record a credential-free reason. Invalid consumer requests and quota 429 responses do not mark supplier health. Execution checks reject a supplier with a recorded failure. Administrator recovery performs an actual official quota request and clears only the observed failure revision, preserving concurrent newer failures and manual disable state.

The supplier list reads SQLite quota snapshots without upstream calls. A quota read reuses the existing ten-minute cache and account lock; a missing or expired cache is fetched on demand. Failure retains the last successful snapshot. The admin summary returns an ordered `windows` array containing only the actual official primary/secondary window objects and their reported durations, including 2,592,000-second windows. It derives relative reset times from the snapshot observation time and preserves unknown values. List, card and official quota detail views use this same summary without synthesizing missing windows. Consumer quotas remain separate local billing windows.

### Usage model and failure metadata

The ledger reads the pinned Codex server-model evidence: HTTP `openai-model` / `x-openai-model`, then per-response `response.headers`, with top-level event `headers` as the next choice. Once reported headers provide a model, a payload's `model` field cannot overwrite it. Payload model remains a fallback when no header evidence exists. WebSocket upgrade headers apply to the first generation only; subsequent generations use their own event evidence. Requested and actual models remain separate, and prices still use the request-start snapshot.

Migration 0035 adds nullable `error_code` and `error_message`. HTTP JSON errors and nested SSE/WebSocket `response.error` are recorded as bounded metadata; bearer/key-like credentials are redacted. Response bytes and client protocol behavior are unchanged. Existing rows without recorded model/error evidence remain unknown; do not invent or retroactively infer that evidence. The admin list displays the response status code, with detailed reason in a request-detail dialog. Failed usage/cost dashes are presentation only and never rewrite historical billing.

Migration 0036 stores `upstream_request_id` from the official `x-request-id` HTTP response header or the corresponding per-response SSE/WebSocket headers. Header names are matched case-insensitively; absent IDs remain NULL. Never substitute the local ledger UUID or a response ID. A WebSocket upgrade request ID is connection-scoped and is not assigned to generation records. Administration shows the recorded ID next to the timestamp in request details.
