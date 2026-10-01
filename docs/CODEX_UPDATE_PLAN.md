# Codex 0.159.3 与 ccodex 接入方案

2026-10-01（Asia/Shanghai）。状态：**用户已批准，正在实施**。用户已选择以官方 0.159.3 创建 `ccodex` 分支，npm 包为 `@gucooing/ccodex`。[前次实施记录](CODEX_UPDATE_0.157.0.md)已保留。用户随后要求不做本地编译验证，推送后由云端 CI 编译。

## 基线与证据

| 项目 | 核实值 |
| --- | --- |
| 当前 BASE / release / UA | `00c972ed5d6ff6499317fd41b7f23605b8e6850d` / `rust-v0.157.0` / `0.157.0` |
| 目标 release / tag | `0.159.3` / `rust-v0.159.3` |
| TARGET_COMMIT | `01fc69f4026735edfdf6789820549727a4867b11`，官方 tag 解引用结果 |
| commit 时间 / 发布时间 | `2026-09-30T21:46:21Z` / `2026-09-30T22:57:34Z`（发布为北京时间 2026-10-01 06:57:34） |
| 官方证据 / 查询日期 | [release](https://github.com/openai/codex/releases/tag/rust-v0.159.3)、[latest API](https://api.github.com/repos/openai/codex/releases/latest)，2026-10-01，非预发布版 |
| 客户端 fork | `https://github.com/gucooing/codex`，本地 `D:\Github\codex` |
| 原 main | `a933dd77dbe101d7bd746ea3c7d1f8174eca4a05`，保留；新 `ccodex` 分支从 TARGET 开始 |
| 快照缺口 | 当前工作区没有 `reference/codex`/`SOURCE.md`，已从官方取回 BASE 对象完成源码比较；未写入新的已对齐快照 |

两个完整树 `git diff BASE TARGET`：1,603 文件，72,575 行新增、19,198 行删除；244 新增、1,323 修改、29 删除、7 重命名。补齐历史后目标独有 283、旧基线独有 2 个提交。完整 files/stat/diff/commits 保存在忽略目录 `target/codex-audit-20261001/`，可由上述 commit 重建。没有用 main 或源码占位版本代替 release。

变化主要涉及 TUI、工具/exec-server、Guardian、OAuth 回调、Retry-After deadline、WS continuation/interrupt、模型目录、图片文件引用及套餐类型。客户端本地执行/沙箱变化不移植到代理；源码分支的存在不等于发布默认启用。

## 适配矩阵

官方路径相对 [TARGET 源码](https://github.com/openai/codex/tree/01fc69f4026735edfdf6789820549727a4867b11/codex-rs)。

| ID / 状态 | 官方行为与证据 | 当前实现、修改及验证 |
| --- | --- | --- |
| S01 需要同步 | 版本 0.157.0 → 0.159.3；originator、UA公式、安装/账户头无变化；`login/src/auth/default_client.rs` | 最后统一更新 version crate、文档、快照、断言；复用现有 `align_user_agents` 幂等更新派生UA，测试旧账户身份保留和多账户隔离。 |
| S02 需要同步 | OAuth callback/success 从 localhost 改为127.0.0.1；`login/src/server.rs`、`success_page.rs` | `auth/src/oauth.rs::redirect_uri` 仍用localhost，需修改；验证PKCE授权URL、code exchange、fallback port。旧pending授权仍用已保存redirect_uri。 |
| S03 已兼容，补回归 | issuer/client_id/scopes、refresh/revoke wire不变 | 供应OAuth保持官方地址，虚拟OAuth保持服务前缀；虚拟redirect校验已接受新loopback。验证ccodex授权/刷新/撤销与原设备管理一致。 |
| S04 需要同步 | `models-manager/models.json` 新增gpt-6.1-sol、移除内置gpt-5.4，排序/描述/messages变化 | 同步upstream的官方描述符。保留管理员目录、授权、价格及历史；不自动授予新模型或编造价格，不删除已有自定义模型。验证管理模型数据与客户端reader。 |
| S05 已兼容，补回归 | WS `response.interrupt`携带response_id和discard_partial_items，保留连接以继续生成；`codex-api/src/endpoint/responses_websocket.rs`、`12de0e395d` | websocket handler仅归一化create，其他帧透传；ws_start仅为create建账。测试interrupt不开新账、continuation/完成usage顺序，发现缺口则修复bridge/ledger。 |
| S06 已兼容，补回归 | HTTP遵守Retry-After deadline，错误限额上下文增强；`codex-client/src/retry.rs`、`http-client/src/retry_after.rs`、`codex-api/src/sse/responses_error.rs` | api/error已有retry-after/retry-after-ms及结构化错误透传；验证HTTP/SSE/WS。保留客户端重试职责，不叠加可能重复收费的代理重放。 |
| S07 已兼容，补回归 | OpenAI provider显式endpoint override也保留内部metadata；`model-provider-info/src/lib.rs`、`model-provider/src/provider.rs`、`c9e2520707` | 当前JSON归一化仅替换供应安装身份，测试不丢工具/client_metadata。 |
| S08 待确认 | 账户协议新增`promax`；`codex-backend-openapi-models/.../rate_limit_status_payload.rs`、`backend-client/src/client.rs` | 检查供应快照解析和管理展示；必要时补类型兼容。虚拟订阅只由管理员实际发放，不自动升级任何权益。 |
| S09 本次排除 | CLI的MCP文件传递使用`core/src/mcp_openai_file.rs`→`codex-api/src/files.rs`，不是普通上传命令 | 用户最新要求撤回文件上传扩展。新增文件路由、迁移、管理页及引用校验已全部移除，保留此前未实现边界。 |
| S10 已有差异 | 官方managed network policy绑定账户并影响在途请求；`login/src/auth/default_client.rs`、`chatgpt/src/chatgpt_client.rs` | 保留供应HTTP client/proxy/cookie隔离和连接代际检查，不移植桌面系统/PAC策略；既有HTTP在途即时撤销差异继续明确记录。 |
| S11 不适用代理移植 | TUI安全提示、Guardian上下文、工具/沙箱/exec-server、Bedrock及第三方MCP认证变化 | 客户端继承稳定版；涉及服务请求继续登记，不伪造官方安全设置、购买、外部授权或执行成功。 |
| S12 需要同步 | 新维护链：官方 → gucooing/codex → Codex2API | 更新AGENTS、CODEX_UPDATES、README，分别记录官方release commit及fork集成差异。供应wire依据仍是官方，fork地址不能写进供应端常量。保留用户README修改意图。 |

## 客户端与服务接口

客户端改造已获用户指示，可独立推进：

- 命令`ccodex`，包`@gucooing/ccodex`及同scope平台包；安装、更新检查、更新提示、归档和发布流程指向用户仓库。内部crate/协议字段/originator保留官方定义。
- `config.toml`顶层`BASE_OAUTH_URL`，默认`https://oauth-ai.alsl.xyz/api/oauth/chatgpt`。校验URL并规范化尾斜线；OAuth、backend-api、v1、HTTP/WS从同一服务根派生。错误不能静默回退官方地址。
- 逐调用链核查浏览器授权、交换、刷新、撤销、device code、models、usage、responses、compact、Guardian、图片、搜索、Realtime、工作区/云配置、插件/MCP/任务/遥测。JWT claim命名空间、OAuth client_id、用户第三方MCP、文档链接、npm/GitHub发布源不是可全局替换的服务endpoint。
- ccodex配置/凭据默认独立于官方codex。用户明确要求仅与官方codex隔离，不按服务地址拆分凭据；统一使用ccodex自己的配置和登录数据。其他工具、权限、沙箱、登录及交互行为保持官方实现。
- 服务端统一前缀`/api/oauth/chatgpt`；对每条请求登记method/path/query/body/response/error/ownership/admin数据源。真实已有实现复用，不能以fallback 200或空集合冒充缺失能力。
- 设备码若进入登录序列，补usercode/token/验证页、短期状态及速率/到期处理，最终授权与撤销复用登录设备管理。保留PKCE/state/callback。
- 本次不扩展文件上传、云插件执行、远程控制等既有产品缺口，不建立购买/付款系统。客户端仅增加第三方服务地址支持，其他行为沿用官方。

## 实施、数据与验证

1. 批准后先做S02/S04和必要兼容修复，再连接ccodex与临时服务；最后更新基线声明。
2. 保留installation_id、冻结OS/arch/terminal、代理、时区、token、订阅与账本；仅幂等更新派生UA并重建进程HTTP池，不重算费用。
3. 基线更新本身无需数据库迁移。设备码/文件需要新状态时只新增迁移，用临时SQLite测试；必须复用/补齐设备和资源管理记录。真实库不操作。
4. 按用户最新要求，编译、Rust测试、schema生成及三平台构建交由云端CI，不再在本地运行。保留本地格式、静态检查和不需编译的打包检查。云端结果与未执行项分别记录。
5. 改虚拟接口前核查实际安装Desktop的请求/reader/下游分支，server mock、ccodex和Desktop证据分开；不修改Desktop。
6. 本地mock使用假token和临时SQLite，捕获方法、地址、头部、PKCE、JSON、压缩、SSE/WS。用户域名尚未部署，不能声称在线登录/真实推理通过。
7. 全部获准基线兼容项通过后更新version、AGENTS/README/ARCHITECTURE、reference/SOURCE与快照。客户端发布工作流准备好后，实际发布所需远程权限另按实际授权执行。

恢复方式：保留旧基线记录、原main分支；代码可逐文件恢复，不重启现有服务、不改真实库。未来部署需要备份与新构建验证。未取得的证据：修改后的编译/运行、Desktop执行、线上服务及跨平台发布，均尚未验证。

## 裁决

最终范围为S01–S08、S12和ccodex对现有服务的地址接入、必要设备码登录。用户后来明确移除S09文件扩展，已撤回全部相关代码。S10/S11保留已说明边界。

用户于2026-10-01明确回复“批准方案，继续实施”，批准本节建议范围。协议一致不能保证服务端无法识别代理。

## 当前实施记录

- ccodex已推送到`gucooing/codex`的`ccodex`分支，提交`efcc6089a51a66397937faf9f09638162571d811`。它从官方0.159.3建立，不移动原main。六平台构建由[GitHub Actions](https://github.com/gucooing/codex/actions/runs/36877373461)执行，尚不能视为全部通过。
- 客户端没有增加按地址存储凭据的逻辑，`login/src/auth/storage.rs`与官方基线无差异；默认`.ccodex`仅用于与官方安装分开。单个`BASE_OAUTH_URL`负责服务路由。
- 服务端更新OAuth loopback回调、模型描述符、版本/UA目标和相应断言；补设备码usercode/token/浏览器授权，兑换仍走原PKCE和登录设备记录。后台现有登录设备列表及撤销操作复用，不引入手工token配置。
- 新增迁移仅0048设备授权。文件上传、下载、文件记录页、文件票据、文件引用重写及0049迁移已按用户最新要求撤回；它们没有推送或作用于真实数据库。
- npm平台别名/命令入口静态测试、Node语法、安装器Shell/PowerShell语法、Rust格式与前端新增页格式检查已执行；SQLite迁移在内存库检查，无真实库修改。
- 用户要求停止本地编译后，没有继续本地build/test/clippy。此前尝试曾遇到缺少MSVC链接器及上游release锁文件workspace版本戳问题，不计为验证通过。后续编译、Rust回归、schema生成、跨平台产物检查均由云端执行；没有实际登录未部署的服务或调用真实上游模型。
