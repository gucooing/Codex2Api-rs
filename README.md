# Codex2API Rust

## 项目定位

单个 Rust 进程提供 OAuth 客户端、AI API 网关、管理端和用户端四个独立监听入口；两个前端分别构建并嵌入同一个可执行文件。

OAuth 模块专用于已配置本服务地址的 ccodex、cgrok 和 hook Desktop，提供各平台的登录、推理、账户、额度和客户端状态协议，统一服务根为 `/v1/chatgpt`、`/v1/grok`。API 模块专用于标准 AI API 网关，提供 `/v1/responses`、`/v1/chat/completions` 等接口。

供应账户负责上游执行。普通用户可以持有同平台的多个独立订阅，OAuth 客户端身份、设备、额度和历史绑定明确选择的订阅。API Key 则按 API 组调用文本和图像模型，从用户 USD 钱包结算，不消耗订阅额度。ChatGPT、Grok 与 Antigravity 使用各自独立的协议、凭据和上游传输。接入见 [OAuth 客户端](docs/OAUTH.md)及 [AI API 网关](docs/API.md)。

客户端单独维护在 [gucooing/codex 的 ccodex 分支](https://github.com/gucooing/codex/tree/ccodex)，本地可与本仓库并列放在 `../codex`。更新顺序为官方稳定版 → ccodex → 本仓库。官方源码只作为协议和行为参考，不引入官方 Rust crates；纯净参考快照位置为 `reference/codex`，精确来源记录在版本常量中。

官方协议目标：

- 发布版: `0.163.0`（tag `rust-v0.163.0`，commit `83086188ae0614bd882818ff1aa23f3194962012`）
- 源码快照: `83086188ae0614bd882818ff1aa23f3194962012`（2026-10-15T18:57:07Z）
- User-Agent 使用公共版本常量 `0.163.0` 与账户冻结指纹在运行时生成，不将派生 UA 存入账户记录
- 协议常量: `crates/codex2api-common/src/version.rs`
- 架构说明: `docs/ARCHITECTURE.md`

支持范围与剩余能力见[架构说明](docs/ARCHITECTURE.md)及[客户端接口](docs/CCODEX_ENDPOINTS.md)。协议基线不表示文件、云插件等全部官方产品能力已实现。[临时启动测试](docs/LOCAL_TEST.md)使用独立测试库。

计费配置提供经过核实的价格预设，包括 GPT-5.5。输入已支持名称即可自动填价；升级后的首次启动自动登记新支持模型，价格完整时默认启用，按现有套餐规则使用。已有人工定价、停用和删除状态保持原样，历史请求仍按原快照结算。GPT-5.5 Fast 的短上下文上限随价格保存，不把未知长上下文价格当作免费。GPT-6 Astra 和 GPT-6.1 Sol 支持独立的 Ultrafast 档位；管理员配置 Ultrafast 价格后，代理同时在 Codex 模型目录和 Desktop 版本预设中发布该选项，按请求快照计费，缺少价格时保持未定价。Responses 在生成输出前遇到供应授权或额度拒绝时，会在标签号池内自动换号；已输出后的中断不重放生成。

数据仅使用 PostgreSQL，通过必填的 `CODEX2API_DATABASE_URL` 连接。管理端只有一个管理员，账户密码登录，首次启动默认 `admin` / `admin`。

Grok 作为独立渠道提供代码／回调、设备码、RT 和逐行批量授权，支持本地套餐、钱包购买、模型计价和用量记录。`cgrok` 服务根使用 OAuth origin 加 `/v1/grok`。模型可用性由管理端配置及套餐决定，自定义模型无需内置名单认证；官方数据仅作预设。接入方式及能力边界见 [Grok 渠道](docs/GROK.md)。

Antigravity 作为只服务标准 AI API 网关的供应渠道接入 Google Cloud Code Assist：供应账户通过 Google 授权代码／回调或 RT 创建，请求伪装为官方 Antigravity 桌面客户端，Responses、Chat Completions 和 Messages 在渠道内部转换为 Gemini 线协议。它没有 `/v1/antigravity` OAuth 客户端服务根，也没有套餐或订阅；额度耗尽按模型冷却，Gemini 与 Claude 模型按官方公开价格提供默认计费预设。详见 [Antigravity 渠道](docs/ANTIGRAVITY.md)。

## Docker Compose 部署

需要 Docker Engine、Docker Compose（`docker compose`）和容器可访问的 PostgreSQL、Redis。Compose 不创建数据库服务，必须在 `.env` 中填写 `CODEX2API_DATABASE_URL` 和 `CODEX2API_REDIS_URL`。镜像地址为 `ghcr.io/gucooing/codex2api-rs`，支持 `linux/amd64` 和 `linux/arm64`；两个前端已经嵌入镜像内的 Rust 可执行文件，服务器不需要安装 Node.js 或 Rust。

下载仓库中的 `docker-compose.yml` 和 `.env.example` 到同一目录，或在仓库根目录执行：

```bash
cp .env.example .env
# 填写 PostgreSQL、Redis 地址，并按部署环境编辑端口和对外访问地址。
docker compose pull
docker compose up -d --wait
docker compose logs -f --tail=100
```

PowerShell 中复制配置使用 `Copy-Item .env.example .env`。默认镜像标签为 `latest`，指向最近成功发布的正式版本；首个正式版本镜像发布前，可将 `.env` 中的 `CODEX2API_IMAGE` 改为 `ghcr.io/gucooing/codex2api-rs:edge`，使用通过 CI 的 `main` 构建。生产部署也可以指定已发布的完整版本标签或 `@sha256:…` 摘要固定镜像。

默认只在宿主机回环地址开放四个端口：

| 入口 | 地址 | Compose 配置 |
| --- | --- | --- |
| OAuth 客户端 | `http://127.0.0.1:8080` | `CODEX2API_OAUTH_HOST` / `CODEX2API_OAUTH_PORT` |
| 管理端 | `http://127.0.0.1:8081/admin/` | `CODEX2API_ADMIN_HOST` / `CODEX2API_ADMIN_PORT` |
| 用户端 | `http://127.0.0.1:8082/user/` | `CODEX2API_USER_HOST` / `CODEX2API_USER_PORT` |
| AI API 网关 | `http://127.0.0.1:8083/v1` | `CODEX2API_API_HOST` / `CODEX2API_API_PORT` |

直接从其他机器访问时，将所需入口的 `*_HOST` 改为 `0.0.0.0`，并将对应的 `CODEX2API_PUBLIC_*_URL` 改为客户端实际访问的 origin。首次管理登录为 `admin` / `admin`，登录后修改密码。使用宿主机上的反向代理时，可保留回环绑定，并将四个公开地址设置为实际 HTTPS origin；这些变量不启用容器内 TLS。反向代理应把 `/admin` 及其子路径转发到 8081，把 `/user` 及其子路径转发到 8082，`/v1/chatgpt`、`/v1/grok` 及其子路径原样转发到 8080，其余标准 `/v1/*` 转发到 8083，并支持 WebSocket 和不缓冲的 SSE。两类入口也可以使用独立域名。容器内监听端口固定为 8080、8081、8082、8083，只通过端口映射更改宿主机端口。

管理端 **设置 → 访问地址** 中保存的地址优先于 `.env` 中的启动默认值；已有部署改域名时同步更新该设置，并重新发起客户端登录。

账户、配置和账本保存在外部 PostgreSQL。首次启动在已创建的空数据库内原子初始化当前 schema 和预设；已有数据库在启动时按版本自动执行嵌入的 SQL 迁移。迁移和版本记录在同一事务提交，失败时回滚并停止启动。应用只读取当前 schema 和配置契约。数据库备份使用 PostgreSQL 自身的备份工具。

升级时执行：

```bash
docker compose pull
docker compose up -d --wait
```

也可使用仓库中的多阶段 `Dockerfile` 自行构建，然后在 `.env` 中设置 `CODEX2API_IMAGE=codex2api:local`：

```bash
docker build --build-arg CODEX2API_BUILD_TAG=local -t codex2api:local .
docker compose up -d --wait --pull never
```

构建会分别检查、导出两个前端，再使用仓库锁定的 Rust 工具链及 `Cargo.lock` 编译。运行容器为非 root 用户，根文件系统只读，持久化写入 PostgreSQL 和 Redis，临时文件使用临时目录；健康检查覆盖四个监听入口，停止时向 Rust 进程直接发送 SIGTERM，并预留 60 秒关闭时间。自定义 CA 证书可通过 Compose override 只读挂载到容器，再设置 `CODEX_CA_CERTIFICATE` 为容器内路径。

### CI 镜像发布

现有 `Check` 和 `Release` 工作流调用 `.github/workflows/docker.yml`：

- PR 先通过现有检查，再在原生 amd64、arm64 runner 上构建镜像，不登录 GHCR、不发布。
- `main` 推送在 `Check` 通过后发布 `edge` 和 `sha-<完整提交哈希>`。
- Git 标签推送在现有多平台二进制构建通过后发布同名镜像标签及提交哈希标签；正式 `vX.Y.Z` / `X.Y.Z` 标签同时更新 `latest`，预发布标签不更新 `latest`。GitHub Release 在镜像成功发布后创建。
- `Container` 支持手动运行：`main` 和已合入 `main` 的标签允许发布，其他分支仅验证。标签必须满足 Docker 标签格式。

两个平台构建均通过后才发布多架构标签。镜像名从当前 GitHub 仓库名自动转为小写，Fork 仓库部署时同步修改 `.env` 中的镜像地址。

CI 使用自动提供的 `GITHUB_TOKEN` 及 `packages: write` 权限推送，无需另设 GHCR 密码。首次发布后，如需匿名拉取，在 GitHub Packages 中将该包的可见性设为 Public；私有包须先使用有 `read:packages` 权限的凭据执行 `docker login ghcr.io`。详见 [GitHub Container Registry 文档](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)。

## 从源码构建

管理前端 `frontend/` 和用户前端 `frontend-user/` 分别使用 Next.js 静态导出，OAuth 确认页面属于用户前端。先安装 Node.js 24，再执行：

```powershell
./scripts/build.ps1 -Release
```

Linux/macOS：

```bash
bash scripts/build.sh --release
```

脚本依次执行 `npm ci`、ESLint、Prettier 格式检查、TypeScript 检查、前端契约测试、Next.js 导出及 Rust 编译。`codex2api-admin` 和 `codex2api-user` 分别通过公共工具校验前端源文件 SHA-256 清单，把各自的 `frontend/out` 和 `frontend-user/out` 编译进二进制；产物缺失或过期会明确拒绝编译。Cargo 不会隐式联网安装 Node 依赖。发布仍为单个可执行文件，运行不依赖 Node.js 或外部网页目录。

管理端入口为 `http://127.0.0.1:8081/admin/`，接口为 `/admin/api/`。用户端入口为 `http://127.0.0.1:8082/user/`，接口为 `/user/api/`。OAuth 在 `127.0.0.1:8080`，仅注册客户端协议接口。AI API 网关在 `127.0.0.1:8083`，只接受 API Key。四端不注册其他端的页面或接口；静态页面不能代替登录验证，所有数据操作仍经过对应账户类型的 JWT 与 CSRF 边界。HTML 不缓存，Next.js 哈希资源长期缓存；未知 API 路径不会返回网页壳。

反向代理部署后，在管理端 **设置 → 访问地址** 填写 OAuth 客户端、AI API 网关、用户端和管理端的实际基础地址，包含明确的 `http://` 或 `https://` 及可选端口，不带 `/v1/grok`、`/user`、`/admin` 等路径。四个入口可使用独立域名，也可由反向代理按平台前缀和网站路径区分。保存到 PostgreSQL 后立即生效，优先于启动环境变量；尚未保存时显示并沿用启动配置。OAuth 发现、令牌 issuer、授权跳转、设备码页面和客户端服务链接使用保存的地址，Cookie 安全属性使用对应网站地址的协议，不从反向代理的内部 HTTP 连接或转发请求头推断。修改后重新发起客户端登录，无需重新编译 cgrok。

开发时可使用 `cargo run -p codex2api --features dev-frontend`，再分别运行两个前端的 `pnpm dev`，无需预先导出静态页面。该特性仅用于本地开发，release 构建不允许启用；完整配置见 [本地验证](docs/LOCAL_TEST.md#前后端开发启动)。

管理界面的 shadcn/ui 组件、操作方式和隔离浏览器验证见 [前端说明](docs/FRONTEND_UI.md)。

## 启动与配置

程序目前不解析命令行参数，运行配置通过环境变量传入。

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `CODEX2API_OAUTH_BIND` | `127.0.0.1:8080` | OAuth 客户端监听地址。 |
| `CODEX2API_ADMIN_BIND` | `127.0.0.1:8081` | 管理端监听地址。 |
| `CODEX2API_USER_BIND` | `127.0.0.1:8082` | 用户端监听地址。 |
| `CODEX2API_API_BIND` | `127.0.0.1:8083` | AI API 网关监听地址。 |
| `CODEX2API_PUBLIC_OAUTH_URL` | `http://127.0.0.1:8080` | 对外 OAuth origin，用于 OAuth 资源发现和客户端事件地址。 |
| `CODEX2API_PUBLIC_USER_URL` | `http://127.0.0.1:8082` | 对外用户端 origin，用于浏览器授权跳转、同源校验和安全 Cookie。 |
| `CODEX2API_PUBLIC_ADMIN_URL` | `http://127.0.0.1:8081` | 对外管理端 origin；HTTPS 时登录、退出和改密 Cookie 都带 `Secure`，不采信请求中的转发协议头。 |
| `CODEX2API_PUBLIC_API_URL` | `http://127.0.0.1:8083` | AI API 网关公开 origin；用户 API Key 页显示此地址加 `/v1`。 |
| `CODEX2API_DATABASE_URL` | 必填 | PostgreSQL URL，例如 `postgresql://codex2api:password@127.0.0.1:5432/codex2api`。 |
| `CODEX2API_DB_MAX_CONNECTIONS` | `32` | 单进程 PostgreSQL 连接上限，范围 1–1024。 |
| `CODEX2API_DB_MIN_CONNECTIONS` | `2` | 保持的最小连接数，不得超过上限。 |
| `CODEX2API_DB_ACQUIRE_TIMEOUT_SECONDS` | `10` | 等待池连接的超时秒数。 |
| `CODEX2API_DB_IDLE_TIMEOUT_SECONDS` | `300` | 空闲连接回收时间。 |
| `CODEX2API_DB_MAX_LIFETIME_SECONDS` | `1800` | 连接最长寿命。 |
| `CODEX2API_REDIS_URL` | 必填 | Redis 连接地址，例如 `redis://127.0.0.1:6379/0`；未配置或连接失败时启动报错。 |
| `CODEX2API_REDIS_PREFIX` | PostgreSQL 主机、端口、库名及 schema 的哈希 | 共享 Redis 的独立部署使用不同且固定的键前缀。 |
| `CODEX_CA_CERTIFICATE` | 未设置 | 可选的 PEM 格式自定义 CA 证书文件路径。 |
| `SSL_CERT_FILE` | 未设置 | 未配置 `CODEX_CA_CERTIFICATE` 时使用的自定义 CA 证书文件路径。 |
| `RUST_LOG` | `codex2api=info` | 日志过滤规则；`codex2api=debug` 启用完整 AI 请求输入输出日志。 |

PostgreSQL 连接数应按实例总数和数据库 `max_connections` 分配，并预留运维连接。连接池不随 HTTP 请求创建；池满时有界等待，超时返回存储错误。Redis 使用一个可复用的异步连接管理器。请求结算按账户加行锁、独立事务提交；不同账户可并行执行。分页和多段统计使用一致的只读快照。

使用 `codex2api --debug` 启动也可启用 debug 模式。AI 请求与响应正文按请求 ID、方向和序号完整打印到控制台，覆盖 HTTP、SSE 和 WebSocket，不截断流式内容；二进制或跨块 UTF-8 字节以 Base64 保留。默认启动不打印正文，正文不写入 PostgreSQL。

通过 HTTPS 反向代理提供管理端时，将 `CODEX2API_PUBLIC_ADMIN_URL` 设置为浏览器访问的 HTTPS origin。该配置不启动 TLS 监听器。

先设置必填的 `CODEX2API_REDIS_URL`，再启动程序：

```bash
CODEX2API_REDIS_URL=redis://127.0.0.1:6379/0 ./codex2api
```

Linux 或 macOS 自定义监听地址和数据库路径：

```bash
CODEX2API_OAUTH_BIND=0.0.0.0:8080 \
CODEX2API_REDIS_URL=redis://127.0.0.1:6379/0 \
CODEX2API_DATABASE_URL=postgresql://codex2api:password@127.0.0.1:5432/codex2api \
./codex2api
```

Windows PowerShell：

```powershell
$env:CODEX2API_OAUTH_BIND = "0.0.0.0:8080"
$env:CODEX2API_REDIS_URL = "redis://127.0.0.1:6379/0"
$env:CODEX2API_DATABASE_URL = "postgresql://codex2api:password@127.0.0.1:5432/codex2api"
.\codex2api.exe
```

## 第三方客户端 OAuth 接入

配套客户端安装与更新：`npm install -g @gucooing/ccodex@latest`。命令为 `ccodex`，默认配置与登录数据放在 `~/.ccodex`，不覆盖官方 `codex`。客户端基于官方最新稳定版，服务地址已内置为 `https://oauth-ai.alsl.xyz/v1/chatgpt`，无需配置；客户端保留官方工具、权限、沙箱和交互逻辑。

该域名需部署本服务后才能使用；部署时设置 `CODEX2API_PUBLIC_OAUTH_URL=https://oauth-ai.alsl.xyz`，反向代理保留 `/v1/chatgpt` 路径并支持 WebSocket。`ccodex login` 使用浏览器授权；`ccodex login --device-auth` 使用设备码，浏览器验证页为 `/v1/chatgpt/codex/device`。授权入口跳转到 `CODEX2API_PUBLIC_USER_URL` 上的确认页；反向代理应分别路由四个入口，并在用户端保留 `/user` 路径。设备登录可以在用户中心或管理端撤销。

管理员在 **用户管理** 创建用户，在 **订阅管理** 新增、续期、调整或终止用户订阅。列表以用户、套餐和独立订阅编号区分记录，“管理”打开设置、用量、重置卡、记录和登录设备。

用户中心采用侧栏、顶部导航和紧凑内容区，风格与管理端一致。首页总览显示本人钱包、本人持有的全部独立订阅、实际请求与 Token / 计费用量趋势和最近订单；钱包位于“账户”，模型广场位于“工作台”；侧栏还可进入使用信息、订阅、套餐、订单、设备和用户信息。使用信息支持时间、平台、模型和结果筛选，展示本人请求明细；未知用量或费用不当作零。用户信息页为 `/user/profile/`，展示基本资料并提供修改密码。刷新先恢复会话，站内切换及刷新保留当前页面。新用户默认无订阅，同平台可同时持有多个订阅。没有 Free 默认套餐或到期兜底，过期记录保留且停止执行权益。钱包与套餐售价使用 USD，余额按整数美分保存，初始为零；管理员可在用户管理中增加或减少余额，增加或减少按事务内当前余额执行，不因其他钱包变动发生身份版本冲突；每次调整记录为系统操作，保留操作人及变动前后余额，原因可选填。管理端“流水管理”集中查看全部用户的来源、前后余额和时间，并支持时间范围与用户名筛选；用户端没有充值或调整余额接口，支付方式仅支持钱包余额。套餐只有已配置售价且开启“允许购买”时才显示在用户目录并允许购买、续订；预览、确认和支付均校验。未定价或停售套餐仍可由管理员分配，供应号池配置不影响展示。套餐可设置售价、购买有效天数、RPM 与并发限制（默认 40 和 10）、手动授权的模型列表（含模型倍率与模型限速）以及仅管理端可见的默认供应号池标签（可多选）；用户端展示套餐及订阅的生效限流值。点击下单先打开不落单的预览，核对套餐、有效期、升级抵扣和优惠券后选择支付方式；点击“确认订单”才创建待支付订单，再单独确认钱包支付，成功后显示订单完成。金额和权益在 PostgreSQL 写事务中一起提交，重复确认或支付不重复落单、扣款。供应路由只用于执行，不作为钱包支付条件。

同套餐可以续订；当期换套餐按日均售价判断，只能升级，降级或同价换套餐在到期后办理。升级按确认的预览时刻剩余时长计算新套餐费用，再抵扣原订阅的剩余价值，保持原到期时间和用量周期起点。购买、续订和管理员发放均保存各期售价快照，后续改价不重算原抵扣价值。预览有效期最多 5 分钟，确认订单后沿用该截止时间，最终应付金额在确认订单前显示。优惠券可缩短预览有效期；修改优惠码后必须重新计算。管理端“订单管理”支持按用户、订单号、套餐与状态查询明细和取消待支付订单；用户只能查看、支付或取消自己的订单。管理员在“优惠券管理”创建或停用固定 USD 金额优惠券，设置适用套餐、有效期、最低应付金额及总量 / 每人次数。预览不占用次数；确认订单时预留，取消或超时释放，支付成功后核销。已确认订单保留优惠快照，之后升级按优惠后的剩余价值抵扣；日均售价比较仍使用原标价。

浏览器已有用户会话时显示身份，未登录时仅验证当前授权链接的用户名和密码。随后从当前平台的可用订阅中下拉选择目标并确认，才发放授权码。保留 PKCE S256、state 和本机回调地址；每份订阅独立持有 OAuth 设备与凭据。

登录身份统一保存在 `accounts`，管理员和普通用户资料分开。`platform_accounts` 只保存用户拥有的订阅及客户端身份，不另存登录密码。同平台订阅通过独立 ID 绑定权限和数据。

供应账户的身份、凭据、指纹、额度快照及执行路由只在管理端可见。用户接口不复用管理 DTO；用户 Cookie、AI access/refresh/ID token 均不能授权管理接口。改密和停用用户撤销其网页会话、平台设备及未确认授权；订阅过期后保留记录并停止执行权益。

虚拟身份、登录设备和本系统用量独立于真实账户。订阅默认继承套餐的标签号池，可在管理中配置独立的标签集合。每次请求从号池中选择同平台、已配置该模型且状态可用的供应账号，结合会话粘连、优先级、负载、等待数、错误率和首字延迟调度；可在设置中选择传统负载模式。粘连记录是会话到账号的映射，Grok 按模型隔离。删除或失效账号会退出候选池；无可用模型或账号时拒绝执行。号池变更保留身份、设备与历史用量。

套餐设置 RPM 和并发限制，默认 40 和 10；订阅可覆盖或留空继承，0 表示无限。系统设置中没有这两项全局默认值。供应账户调度仍有独立并发上限、优先级和负载容量。HTTP 与 WebSocket 共用每份订阅的限制，内部换号不重复计算 RPM 或并发。

运行态必须通过 `CODEX2API_REDIS_URL` 接入 Redis，存放会话粘连、响应归属、RPM 和并发租约。`CODEX2API_REDIS_PREFIX` 用于隔离共享 Redis 的不同部署；留空时按 PostgreSQL 主机、端口、库名及 schema 生成。独立部署可显式设置不同前缀。未配置 Redis 或初始连接失败时启动报错；运行中 Redis 不可用时拒绝新的调度请求，不使用本地替代。响应归属写入 Redis 后才交付完成响应。PostgreSQL 保留身份、配置和真实计费账本；业务快照统一使用本地热缓存、Redis 和合并回源，按业务域或对象失效。API 组/模型与订阅的限流统一使用 Redis；价格快照在 PostgreSQL 事务中校验版本，最近使用时间合并写入。

语音通话不提供服务。Realtime、Live、通话创建、侧边 WebSocket、语音目录及语音选择 API 返回 `501 voice_calls_unavailable`，不会调用供应账号。客户端设置明确关闭语音入口，独立听写功能保留。

订阅详情顶部提供 **账户设置、用量统计、重置卡、记录查询、登录设备** 页签。配置使用输入框、开关、下拉框和可增删列表，保存校验结构与版本；用量以指标、额度进度和每日图表展示，资源与日志使用表格。网页和客户端使用同一份 PostgreSQL 数据。实现范围与验证边界见[订阅客户端数据与管理](docs/ARCHITECTURE.md#subscription-client-data-and-management)。

邀请记录、个人偏好、会话、侧栏项目、审批和安装状态由客户端或实际业务流程维护，管理端仅供查询；身份、功能政策、目录和订阅权益使用具名业务控件管理。Desktop 启动所需的账号、认证方式及设备标识由系统构造，用户无需填写协议字段。升级涉及启动配置的修复后，请重新打开 Desktop，避免继续使用进程内缓存的旧配置。

**订阅额度独立**：使用 7 天或 30 天外层与可选 5 小时内层两个美元费用窗口，各订阅额度由其套餐决定，无订阅或订阅到期后不提供兜底权益。模型价格在请求开始时快照，按实际普通输入、缓存输入、缓存写入与输出结算，推理 Token 不重复收费。修改价格不重算历史；未知价格或用量明确标注。客户端响应、HTTP 头、SSE 与 WebSocket 读取同一 PostgreSQL 账本。未完成的并发请求尚未结算，可能造成短暂超额。

本系统真实用量单独记录，每条记录保留请求时的实际供应账户和来源名称，号池变化不会重写历史。供应账户详情提供“可用模型”和“调度”。同步上游模型只更新当前账号，手动添加项来自计费配置（可未启用或未定价）；两者都不改变计费配置，空模型列表不接受请求。

客户端个人资料 `GET /v1/chatgpt/backend-api/wham/profiles/me` 返回虚拟账号显示名称、用户名，以及按虚拟账号累计的本系统 Token 和每日用量统计。变更号池和重启不会丢失统计，不继承真实账户的头像或个人资料。未记录的轮次持续时间保持为空。

客户端需支持自定义服务地址及浏览器授权码登录。统一基础地址为 `http://127.0.0.1:8080/v1/chatgpt`，接口如下：

| 功能 | 路径 |
| --- | --- |
| 浏览器授权入口 | `GET /v1/chatgpt/oauth/authorize`，跳转至用户端 `/user/authorize/` |
| 授权码兑换／令牌刷新 | `POST /v1/chatgpt/oauth/token` |
| 撤销代理令牌 | `POST /v1/chatgpt/oauth/revoke` |
| 申请设备码／轮询授权结果 | `POST /v1/chatgpt/api/accounts/deviceauth/usercode`、`POST /v1/chatgpt/api/accounts/deviceauth/token` |
| 设备码验证页 | `GET /v1/chatgpt/codex/device` |
| Responses | `POST /v1/chatgpt/backend-api/codex/responses` |
| 上下文压缩 | `POST /v1/chatgpt/backend-api/codex/responses/compact` |
| 其他 Responses 子接口 | `POST /v1/chatgpt/backend-api/codex/responses/{子路径}`（校验路径片段） |
| Responses WebSocket | `ws://服务地址/v1/chatgpt/backend-api/codex/responses` |
| 模型列表 | `GET /v1/chatgpt/backend-api/codex/models` |
| 额度查询 | `GET /v1/chatgpt/backend-api/wham/usage` |
| 账户查询 | `GET /v1/chatgpt/backend-api/wham/accounts/check` |
| 虚拟账户信息 | `GET /v1/chatgpt/backend-api/accounts/check/v4-2023-04-27` |
| 订阅到期时间 | `GET /v1/chatgpt/backend-api/subscriptions?account_id=虚拟账号ID` |
| 关闭训练数据共享 | `PATCH /v1/chatgpt/backend-api/settings/account_user_setting?feature=training_allowed&value=false` |
| 输入 Token 计数 | `POST /v1/chatgpt/v1/responses/input_tokens` |
| 应用批量查询 | `POST /v1/chatgpt/backend-api/ps/apps/batch` |
| 客户端统计事件 | `POST /v1/chatgpt/backend-api/codex/analytics-events/events`（按虚拟账号保存允许的活动元数据，重试去重，不计推理 Token） |
| MCP | `GET /v1/chatgpt/backend-api/ps/mcp`、`POST /v1/chatgpt/backend-api/ps/mcp` |
| MCP 资源发现（无需登录） | `GET /v1/chatgpt/backend-api/ps/mcp/.well-known/oauth-protected-resource` |
| 客户端追踪事件 | `POST /v1/chatgpt/backend-api/o11y/v1/traces`（记录本地请求状态，不保存正文、不转发供应账户） |
| 浏览器设置 | `GET/PATCH /v1/chatgpt/backend-api/wham/browser/settings`（按虚拟账号持久化，PATCH 校验版本） |
| 引导状态 | `GET /v1/chatgpt/backend-api/wham/onboarding/context`、`POST /v1/chatgpt/backend-api/wham/onboarding/desktop/complete` |
| 精选插件 | `GET /v1/chatgpt/backend-api/plugins/featured` |
| 插件目录 | `GET /v1/chatgpt/backend-api/ps/plugins/list` |
| 已安装插件 | `GET /v1/chatgpt/backend-api/ps/plugins/installed` |
| Codex 推荐插件 | `GET /v1/chatgpt/backend-api/ps/plugins/suggested/codex` |

现有 Codex 和 WHAM 路由均可通过此前缀访问，包括图片、搜索和账户资料。HTTP/WS 由应用提供，证书和外部 HTTPS/WSS 由反向代理处理；反向代理需保留请求路径并支持 WebSocket Upgrade。

客户端配置保存在 PostgreSQL 的 `virtual_client_state`，任务、会话归属和活动元数据保存在 `virtual_resources`，通知事件保存在 `virtual_events`，请求日志保存在 `virtual_request_logs`。数据跨重启、变更号池保留。默认配置首次读取时持久化，网页可修改；集合没有本账号记录时才返回空。私人配置不继承供应账户，也不等于完成外部付款、自动化执行或站点发布。

云任务只列出本账号实际创建并记录的任务；详情和兄弟轮次请求校验归属。任务与已创建会话保留原供应来源，只有原账号仍属于当前号池且可用时才能继续执行；不能用新账号读取旧供应资源。ChatGPT conduit 凭据在服务端保存，以有期限且绑定虚拟账号及执行来源的本地凭据替换。只有上游实际返回会话 ID 后才建立会话归属。

`celsius/ws/user` 返回本地事件连接地址，通知更新从持久化事件读取。连接凭据仅用于事件通道，设备下线后失效；没有通知时保持连接，不返回假的上游 URL。反向代理部署须设置 `CODEX2API_PUBLIC_OAUTH_URL` 为外部 HTTPS origin，以生成正确的 WSS 地址。

桌面用量页支持 `GET /v1/chatgpt/backend-api/wham/usage/daily-token-usage-breakdown`：按虚拟账号、UTC 日期及实际模型汇总本系统已记录的输入和输出 Token（缓存输入已包含在输入中，不重复累加）。支持 `start_date`、`end_date`（包含当天）及 `group_by=day`，默认最近七天；没有上报 Token 的请求不伪造成已知用量。数据跨变更号池保留。

轮次、插件和技能统计读取本账号实际提交的 `codex_turn_event`、`codex_plugin_used`、`skill_invocation`，按服务端接收日期汇总；轮次按 thread/turn 去重，客户端未提交的活动无法还原。这些是客户端活动统计，不替代从上游完成事件读取的 Token 账本。金额消费、积分购买和官方历史周期尚无本地业务数据源，不能用 Token 冒充；月金额上限返回客户端支持的不可用状态，轮次金额估算明确返回不可用，历史覆盖不会标为完整。

Sub2API 的地址字段可按以下示例配置（客户端与代理在同一台机器）：

```json
{
  "responses_url": "http://127.0.0.1:8080/v1/chatgpt/backend-api/codex/responses",
  "chatgpt_base_url": "http://127.0.0.1:8080/v1/chatgpt",
  "auth_base_url": "http://127.0.0.1:8080/v1/chatgpt",
  "platform_base_url": "http://127.0.0.1:8080/v1/chatgpt"
}
```

Responses 子路径和同级模型、图片、搜索、实时接口按该客户端的地址重写规则保留。输入 Token 计数单独转发到官方 `https://api.openai.com/v1/responses/input_tokens`，不计作已消耗推理用量；如果上游不接受该账户的凭据，保留真实错误，由客户端决定是否采用本地估算。

授权请求使用 `response_type=code`、`client_id=app_EMoamEEZ73f0CkXaXp7hrann`、`state`、`code_challenge_method=S256` 和 `code_challenge`。目前允许 HTTP 回环地址的 `/auth/callback`，必须包含端口，不接受外部回调、查询参数、用户信息或片段。授权码有效期两分钟，只能兑换一次，并绑定客户端、完整回调地址和 PKCE 证明。

令牌接口接受 JSON 或表单。兑换需要 `grant_type=authorization_code`、`client_id`、`redirect_uri`、`code`、`code_verifier`；后续由客户端使用 `grant_type=refresh_token` 和设备 Refresh Token 续期。Access Token 与 ID Token 的字段及有效期遵循固定官方协议，Refresh Token 保持客户端 OAuth 原有契约；不复用网页登录 JWT。返回身份均为本地平台账户。业务请求使用 `Authorization: Bearer <access_token>`；若发送 `ChatGPT-Account-ID`，值必须是虚拟账号 ID。签名密钥、设备凭据哈希和 Access Token 哈希持久化到 PostgreSQL，服务重启后可以继续续期。

每次授权创建独立设备会话。后台显示客户端／UA、安装标识、首次登录、最近续期和最近使用，支持下线单个设备；下线使它的 Refresh Token 和全部 Access Token 失效。安装标识和 UA 是客户端上报信息，不能可靠识别物理设备，也不表示当前在线。

客户端仅接受虚拟消费账户的 OAuth Access Token。授权 scope 随设备保存，刷新仅可保持或缩小；客户端令牌不能管理账户、套餐、价格、路由或读取供应凭据。管理端和用户端均使用 JWT，网页通过各自 HttpOnly Cookie 携带；接口也可使用对应用途的 Bearer JWT，写入仍要求 X-CSRF-Token。管理员与用户使用两套独立 RSA 密钥，保存在 PostgreSQL 的 `jwt_signing_keys`。验签只允许服务端固定的 RS256，拒绝 none、空算法、其他算法、未知密钥和非 JWT 凭据。签名之外还校验 account_type、token_use、aud、iss、有效期及服务端会话/设备撤销记录；错误 Bearer 不会回退使用 Cookie。Codex OAuth 的签名密钥、设备授权和令牌保持原有契约。

未实现的路径和 HTTP 方法返回 `501 / endpoint_not_implemented`，在 **系统设置 → 端点诊断** 中记录方法、路径、次数、首次和最近时间，不保存查询参数、请求正文、密码或令牌。诊断只覆盖到达代理此前缀的请求，无法捕获应用绕过代理的直连流量。

Windows **ChatGPT** 桌面应用使用 [C# 图形启动器](tools/desktop-proxy/README.md)：界面配置服务器地址、自动发现客户端或手动选路径，支持设置导入导出。单个 EXE 内嵌编译后的地址处理组件，按服务地址保存独立的 `auth.json`、`config.toml` 和客户端数据；继续使用客户端实际选择的原生运行时，不复制客户端或包裹 app-server。启动器直接支持 HTTP 服务，不使用脚本或调试通道加载。登录使用自定义域名入口 `GET /codex/desktop-auth`，反向代理也需转发该路径；登录成功使用本地回调页，不跳转到 ChatGPT。当前服务端不包含 PAT 或 Agent Identity 登录。

## 跟进官方 Codex 更新

向 AI 下达“更新 Codex”或“升级官方 Codex 到 0.155.0”等命令后，按
[官方 Codex 更新维护流程](docs/CODEX_UPDATES.md) 执行：先核实目标发布版的 commit，
比较当前参考 commit 与目标 commit 的源码和实际客户端行为，提交带证据的更新方案，
**由用户裁决后再实施**。验证完成后统一更新代码、参考快照和版本记录，作为下一次更新的基线。

`0.155.0` 仅为示例版本。更新不能仅修改 User-Agent 版本号；参考源码 commit、官方发布版 commit
和本项目自身的发布版本需要分别处理。AI 的执行入口同时记录在 [AGENTS.md](AGENTS.md)。

## 核心原则

### 与官方服务器行为一致

与官方 Codex 服务通信时，认证、请求构造、默认参数、请求头、元数据、流式响应、会话/线程/回合状态处理，都必须与官方 Codex 客户端直接登录后的行为一致。

以实际通信表现为准，不以是否复用官方代码为准。官方源码用于核对协议、字段、常量和生命周期，不要求把 `codex-rs` 嵌进本项目，也不要求照抄官方模块。

### 单一 Proxy 进程

整个项目运行一个 Rust Proxy 进程。

不为每个账户启动一个 Codex CLI 进程，也不把 Proxy 做成多个外部 Codex 进程的编排器。

Proxy 进程内部为每个账户维护独立的上游通信上下文。账户之间只共享不可变的程序代码，不共享账户运行状态。

## 账户级隔离与持久化

每个账户必须拥有独立且持久化的环境，包括：

- 官方请求身份上下文（如 `originator`、`User-Agent`、构建版本、操作系统和架构信息）；
- 认证状态；
- `ChatGPT-Account-ID`；
- `installation_id`；
- HTTP/WebSocket 客户端状态；
- 会话、线程和回合相关状态；
- 账户专属身份与 token（PostgreSQL，不落官方 `$CODEX_HOME` 文件）。

以下内容不能在账户之间共享：

- 账户身份行和 token 行；
- 认证缓存和登录状态；
- 账户级请求上下文；
- 账户级 HTTP/WebSocket 连接状态；
- 会话、线程和回合状态。

这里所说的请求身份是官方 Codex 的应用层请求上下文，不是额外伪造的 TLS、JA3 或其他网络设备指纹。

## 请求身份规则

### 固定的账户身份

每个账户从授权登录开始初始化自己的官方 Codex 请求身份上下文。

授权成功后，该上下文与上游账户绑定并持久化。该账户后续的所有请求都必须使用自己的上下文，不得回退到共享的全局账户状态。

固定并持久化的是账户级身份和官方运行上下文。固定的含义是同一账户在其生命周期内保持稳定，不是人为制造官方客户端没有的差异。

如果多个账户使用相同的官方构建版本和相同的运行环境，官方客户端自然相同的进程级字段可以相同。不能为了制造账户差异而手动改写构建版本、操作系统信息、`User-Agent` 或 `originator` 的生成规则。

### 动态请求内容

以下内容不能被强行固定为账户级常量，必须按官方客户端的生命周期生成：

- 请求 ID；
- 会话 ID；
- 线程 ID；
- 回合 ID；
- 窗口或客户端上下文 ID；
- 回合元数据和回合状态；
- 请求时间和其他请求级动态字段。

账户级隔离的目标是让这些动态内容始终在正确的账户环境中生成，而不是复用其他账户的值。

## 授权和持久化流程

### 授权开始

1. 创建待完成的账户授权上下文。
2. 初始化该账户专属的官方状态空间。
3. 按官方客户端规则生成或读取账户的 `installation_id` 及其他官方身份状态。
4. 在授权完成前，将该上下文标记为待绑定状态。

### 授权成功

1. 从官方授权结果中确定上游账户身份。
2. 将上游账户身份与账户上下文原子绑定。
3. 持久化认证状态、`ChatGPT-Account-ID`、`installation_id` 以及对应的官方状态空间。
4. 后续请求根据账户记录路由到同一个账户上下文。

### 已存在账户重新授权

如果上游账户已经存在，重新授权不得重新生成或替换该账户已经持久化的固定身份上下文。

### 授权失败或取消

授权失败或取消时，待绑定上下文不能关联到任何已确认的上游账户。

## 逻辑结构

```text
Rust Proxy 进程
    |
    +-- 对外标准 API
    |
    +-- 账户路由
          |
          +-- 账户环境 A（独立持久化）
          +-- 账户环境 B（独立持久化）
          +-- 账户环境 C（独立持久化）
                    |
                    +-- 与官方 Codex 服务通信
                    +-- 行为与 Codex 直接登录一致
```

Proxy 层负责对外 API、账户选择、请求转发和管理端所需的账户管理能力。对官方服务的请求表现必须与官方 Codex 客户端一致。

## 明确不采用的方案

- 不为每个账户启动一个独立的 Codex CLI 操作系统进程；
- 不把官方源码当作必须依赖或必须照抄的实现；
- 不用账号 ID、邮箱或自定义哈希替代官方身份生成规则；
- 不把每次请求重新生成的随机值当作账户固定身份；
- 不通过伪造 TLS/JA3 或网络设备特征实现所谓的官方指纹。
