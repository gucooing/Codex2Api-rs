# Codex2API Rust

## 项目定位

一个用于账户池挂载 Rust 版本的 `codex2api` 程序，并提供管理端网页。

目标是把 Codex 订阅转换成标准、可供 Codex 客户端使用的 API。Proxy 对外提供该 API；对内管理多个上游 OpenAI/ChatGPT 账户，并把请求发到官方 Codex 服务。

一个上游 OpenAI/ChatGPT 登录凭据对应一个账户。

官方 Codex 源码（https://github.com/openai/codex）只作为协议和行为参考，不作为必须依赖或必须照抄的实现。本仓库将官方源码快照放在 `reference/codex`，版本信息见 `reference/SOURCE.md`。实现可以自行编写，但与官方 Codex 服务器通信时的表现必须和在 Codex 中直接登录完全一致。

当前实现对齐的官方版本：

- 发布版: `0.154.0`（tag `rust-v0.154.0`，commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`）
- 源码快照: `a8964cb1bad67bc26a826fb07d1bef99c6a3f008`（2026-09-15T05:44:41Z）
- User-Agent 版本写死为 `0.154.0`，不用源码树的 `0.0.0`
- 常量 crate: `crates/codex2api-version`
- 架构说明: `docs/ARCHITECTURE.md`

数据用 SQLite（默认 `data/codex2api.sqlite`）。管理端只有一个管理员，账户密码登录，首次启动默认 `admin` / `admin`。

## 启动与配置

程序目前不解析命令行参数，运行配置通过环境变量传入。

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `CODEX2API_BIND` | `127.0.0.1:8080` | HTTP 服务监听地址，格式为 `IP:端口`。 |
| `CODEX2API_DB` | `data/codex2api.sqlite` | SQLite 数据库文件路径；相对路径以程序启动目录为基准。 |
| `CODEX_CA_CERTIFICATE` | 未设置 | 可选的 PEM 格式自定义 CA 证书文件路径。 |
| `SSL_CERT_FILE` | 未设置 | 未配置 `CODEX_CA_CERTIFICATE` 时使用的自定义 CA 证书文件路径。 |
| `RUST_LOG` | `codex2api=info` | 日志过滤规则。 |

直接使用默认配置启动：

```bash
./codex2api
```

Linux 或 macOS 自定义监听地址和数据库路径：

```bash
CODEX2API_BIND=0.0.0.0:8080 \
CODEX2API_DB=/var/lib/codex2api/codex2api.sqlite \
./codex2api
```

Windows PowerShell：

```powershell
$env:CODEX2API_BIND = "0.0.0.0:8080"
$env:CODEX2API_DB = "D:\data\codex2api.sqlite"
.\codex2api.exe
```

## 第三方客户端 OAuth 接入

在管理端顶栏 **OAuth** 中添加 RT，选择一个已启用且完成官方授权的账户，再复制 RT 到第三方客户端。首页按账户展示 RT 数量、登录设备数和最近使用时间；点击 **详情** 管理该账户的多个 RT，并查看各 RT 的登录设备、首次/最近登录和最近使用时间。这里签发的是代理自己的 RT，真实官方 RT 始终留在服务端。

账户行的 **删除** 会清除该账户全部代理 RT、Access Token 和设备记录，保留原始账户、官方授权、API Key 和历史用量。

客户端需要支持修改服务基础地址并通过 RT 刷新登录。统一基础地址为 `http://服务地址/api/oauth/chatgpt`，接口如下：

| 功能 | 路径 |
| --- | --- |
| 令牌刷新 | `POST /api/oauth/chatgpt/oauth/token` |
| 撤销代理令牌 | `POST /api/oauth/chatgpt/oauth/revoke` |
| Responses | `POST /api/oauth/chatgpt/backend-api/codex/responses` |
| 上下文压缩 | `POST /api/oauth/chatgpt/backend-api/codex/responses/compact` |
| 其他 Responses 子接口 | `POST /api/oauth/chatgpt/backend-api/codex/responses/{子路径}`（校验路径片段） |
| Responses WebSocket | `ws://服务地址/api/oauth/chatgpt/backend-api/codex/responses` |
| 模型列表 | `GET /api/oauth/chatgpt/backend-api/codex/models` |
| 额度查询 | `GET /api/oauth/chatgpt/backend-api/wham/usage` |
| 账户查询 | `GET /api/oauth/chatgpt/backend-api/wham/accounts/check` |
| RT 导入后的账户信息 | `GET /api/oauth/chatgpt/backend-api/accounts/check/v4-2023-04-27` |
| 订阅到期时间 | `GET /api/oauth/chatgpt/backend-api/subscriptions?account_id=绑定账户ID` |
| 关闭训练数据共享 | `PATCH /api/oauth/chatgpt/backend-api/settings/account_user_setting?feature=training_allowed&value=false` |
| 输入 Token 计数 | `POST /api/oauth/chatgpt/v1/responses/input_tokens` |
| 实时会话创建 | `POST /api/oauth/chatgpt/backend-api/codex/realtime/calls?intent=quicksilver&architecture=avas` |
| 实时会话事件通道 | `ws://服务地址/api/oauth/chatgpt/backend-api/codex/{call_id}` |

现有 Codex 和 WHAM 路由均可通过此前缀访问，包括图片、搜索和账户资料。HTTP/WS 由应用提供，证书和外部 HTTPS/WSS 由反向代理处理；反向代理需保留请求路径并支持 WebSocket Upgrade。

Sub2API 的地址字段可按以下示例配置（客户端与代理在同一台机器）：

```json
{
  "responses_url": "http://127.0.0.1:8080/api/oauth/chatgpt/backend-api/codex/responses",
  "chatgpt_base_url": "http://127.0.0.1:8080/api/oauth/chatgpt",
  "auth_base_url": "http://127.0.0.1:8080/api/oauth/chatgpt",
  "platform_base_url": "http://127.0.0.1:8080/api/oauth/chatgpt"
}
```

Responses 子路径和同级模型、图片、搜索、实时接口按该客户端的地址重写规则保留。输入 Token 计数单独转发到官方 `https://api.openai.com/v1/responses/input_tokens`，不计作已消耗推理用量；如果上游不接受该账户的凭据，保留真实错误，由客户端决定是否采用本地估算。

令牌接口接受 JSON 或 `application/x-www-form-urlencoded`，例如：

```json
{
  "grant_type": "refresh_token",
  "refresh_token": "从管理端复制的代理 RT",
  "client_id": "app_EMoamEEZ73f0CkXaXp7hrann"
}
```

成功返回 `access_token`、`refresh_token`、`id_token`、`token_type: "Bearer"`、`expires_in: 3600` 和 `scope`。`client_id` 可以省略；填写时必须为上例的 Codex client ID。代理签发的 JWT 使用自身签发方 `codex2api`，账户声明反映绑定账户，不是 OpenAI 签发的令牌。

RT 固定绑定一个账户，刷新时保持不变，直到暂停、删除或撤销；每次刷新签发独立的一小时 Access Token。业务接口使用 `Authorization: Bearer <access_token>`。如果发送 `ChatGPT-Account-ID`，必须与绑定账户一致。暂停 RT 会作废已签发的 Access Token；重新启用后需刷新获取新令牌。删除 RT 或账户会清除关联令牌。撤销 Access Token 只影响该令牌，撤销 RT 会删除该凭据及其全部 Access Token，不会撤销上游官方授权。

登录设备按刷新请求中的 `x-codex-installation-id` 识别，未提供时按 User-Agent 归类；同 UA 的多个设备可能合并，这些标识由客户端提供，不代表设备已被验证。刷新令牌更新最近登录时间，HTTP 鉴权和 WebSocket 业务消息更新最近使用时间。设备记录不代表当前在线状态。

此前缀仅接受代理 OAuth Access Token，原 API Key 接口仍使用 API Key。计费请求继续执行 UA 黑白名单，并接入用量管理。用量列表和筛选项统一称为“来源”：API Key 调用显示 Key 名称，OAuth 调用显示 RT 设置的名称。WebSocket 在转发每条业务消息前重新检查有效期与凭据状态。此模块实现代理 RT 登录流程，不包含浏览器授权码/设备码登录、PAT 验证或 Agent Identity 任务注册。

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
- 账户专属身份与 token（SQLite，不落官方 `$CODEX_HOME` 文件）。

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

## 管理端网页

项目需要提供管理端网页，用于管理 Proxy 中的上游账户及其运行状态。

管理端的具体页面、字段和操作范围尚未在本文中预先确定，后续单独讨论后再补充。

## 明确不采用的方案

- 不为每个账户启动一个独立的 Codex CLI 操作系统进程；
- 不把官方源码当作必须依赖或必须照抄的实现；
- 不用账号 ID、邮箱或自定义哈希替代官方身份生成规则；
- 不把每次请求重新生成的随机值当作账户固定身份；
- 不通过伪造 TLS/JA3 或网络设备特征实现所谓的官方指纹。

## 当前状态

工程骨架已建立：workspace crates、SQLite schema、默认管理员、版本钉死常量和进程入口。具体登录、上游转发和管理端功能按 crate 继续实现。
