# Codex 0.157.0 更新方案与实施结果

2026-09-25。用户已回复“实施更新方案”，批准建议范围。本文件是同一方案的实施记录；逐请求结论见 [请求与动作核查](CODEX_REQUEST_AUDIT.md)，临时启动见 [本地测试](LOCAL_TEST.md)。

## 基线与批准范围

| 项目 | 值 |
| --- | --- |
| 原 BASE_COMMIT / release | `b412ff32c417f855c2b2d1581b77058eed87c84b` / `0.156.1` |
| TARGET_TAG / UA | `rust-v0.157.0` / `0.157.0` |
| TARGET_COMMIT | `00c972ed5d6ff6499317fd41b7f23605b8e6850d`，annotated tag 解引用为 commit |
| Commit 时间 | `2026-09-25T01:33:35Z`（原时区 `2026-09-24T18:33:35-07:00`） |
| 发布时间 / 查询时间 | `2026-09-25T02:31:06Z` / `2026-09-25T07:05:24Z` |
| 官方来源 | [0.157.0 release](https://github.com/openai/codex/releases/tag/rust-v0.157.0)，GitHub API latest 当时返回该稳定版 |
| 原本地 main | `d7b07d45517a793acfba4cbf8de697d723cceb46`，保留在 `reference/codex` 的 `main` 分支 |
| 当前参考快照 | `reference/codex` detached 到 TARGET；已补 `reference/SOURCE.md` |
| 实际 Desktop | Appx `26.917.9434.0`、内包 `26.917.71314`、native `0.155.0-alpha.16.4`，与新 release 分别验证 |
| Desktop archive SHA256 | `d4234b03eb532fe0f3e9a7d90caad51edb68af45f771cc786d966377e7446f5a` |

批准的是稳定版更新、A01–A05/A08 兼容修复和请求审计。A06 完整文件业务、A07 供应健康重试策略扩展未获单独批准；A09 其他外部产品缺口保留未完成，不伪造成功。用户后来要求完成后提供临时启动方式；未做部署、提交/推送或运行中服务重启。

## 官方源码比较

比较两个完整树 BASE→TARGET，共 1,086 个文件、42,832 行新增、10,984 行删除；154 新增、928 修改、3 删除、1 重命名。目标独有 128 提交，旧基线独有 2 提交（发布提交及 GPT-6 Sol/Luna 回移 `f1b21bb293`）。因此 GPT-6 Sol/Luna 已存在，不是本次项目新增模型。

已沿请求/认证/网络/模型/上传/Guardian/插件/Realtime 调用链核对；TUI/沙箱/本地执行器变更作适用性分类。原 main 比目标树还多 1,074 个文件差异、169 个 main 侧额外可达提交；未把 main 当成 release。本地分析材料在忽略目录 `target/codex-audit-20260925/`（files/commits/diff、精确发布源码树和 Desktop 请求字面扫描）。可按上面的 commit 重新取得源码。

## 适配矩阵及结果

| ID | 官方行为、实际差异 | 结果、实施位置与验证 |
| --- | --- | --- |
| A01 | 旧基线与目标都支持 origin 的 `NO_CONSTRAINT`；本项目把它当 URL 解析，产生用户提供的 `workspace backend origin is invalid` | **已修复**。upstream/routing.rs 按 bootstrap origin 解释哨兵；保持区域约束、账户唯一性、HTTPS/凭据/路径/空白检查。覆盖 HTTP/compact/Guardian/WS URL、旧 SQLite 快照与凭据变更；Admin 工作区记录显示实际解析结果。未读取真实供应快照，根因是源码定位，非该账户抓包。 |
| A02 | 虚拟账户对 native 返回 chatgpt.com，renderer 则靠 UA 改哨兵，会改变 native 目的地 | **已修复**。identity.rs 统一返回 NO_CONSTRAINT，backend.rs 去掉 UA 分支。当前 Desktop `$Te` schema 和两版 native 使用服务实际响应通过。原生要求 HTTPS base，HTTP 管理页可用不等于 HTTP 原生工作区受支持。 |
| A03 | 官方 WS 错误保留 headers，旧 Status 只留被截断 body 且 401 被折叠 | **已修复**。upstream/error.rs/websocket.rs、api/error.rs/usage.rs 保留状态、完整 JSON 及白名单的 error/retry/request ID 头；剥离连接头、供应 cookie/凭据/额度。400/401/403/426/429/503 与握手本地测试通过。 |
| A04 | String.truncate(4096) 可能切断中文 UTF-8，还会破坏 JSON | **已修复**。协议正文不截断；Display 无正文，账本仍用已有有界脱敏逻辑。中文/emoji 长错误与分类测试通过。 |
| A05 | 当前 Desktop `SFc` POST `/wham/realtime/calls?intent=quicksilver&architecture=avas`，旧路由缺失 | **已修复入口及已有数据链路**。单独 Wham kind 发到真实 WHAM 上游路径，保留 v1/v3 请求；检查成功 SDP/Location 后保存 call 归属。管理增加只读“语音通话创建记录”。实际 renderer 构造→本地 TLS 出站捕获→renderer reader 通过；完整在线音频未测试，有限额且未计价的调用仍明确拒绝。 |
| A06/U09 | file blob timeout 60→300s、最多5次 retry，create/PUT/finalize；本项目尚无完整入口/存储 | **未实施**，独立业务范围。不能把文件引用透传或新重试常量视为上传支持；需要账户归属、大小/状态/来源、换绑策略和管理记录后才能完成。 |
| A07 | 官方可重试瞬时 5xx/传输失败；本项目持久供应异常需管理员恢复 | **保留产品差异**，未批准改运营策略；不叠加代理与客户端重试造成重复执行。 |
| A08 | 旧 Test-DesktopProxyHook 的压缩符号已过期 | **已修复本次受影响验证**。当前 bundle 的 launcher/workspace/model/realtime 实际函数执行通过；不改安装包或业务/权限逻辑。其他旧 bundle 专用脚本未全部迁移，报告中保留边界。 |
| A09 | MCP 私有执行、连接器/插件授权目录、云安装/自动化、付款及部分历史采集不完整 | **未实施完整外部产品**，逐条记录真实501/不可用/仅查询行为，不能凭空集合算完成。 |
| U01 | 0.156.1→0.157.0 package/UA/ref commit | **已同步**。version crate、版本断言、README/AGENTS/ARCHITECTURE、Admin fixtures、SOURCE 和准确快照。 |
| U02 | 无 slug 增删；6 个模型对象改变，GPT-5.6 Sol 移除 ultrafast、priority 6→4，若干 metadata/messages/shell/plan/effort flags 改变 | **已同步**精确发布 models.json。当前 Desktop picker 和两版 native 可读取；管理模型元数据引用新 commit。本地启用、套餐、价格不从官方 available_in_plans 自动发放；历史费用不重算。 |
| U03 | 本次 OAuth issuer/client_id/scopes/code exchange/refresh/revoke wire 不变；新增网络策略绑定 | **协议已兼容**。Auth 单元、PKCE/设备码/refresh/revoke/独立代理回归通过。网络策略即时撤销的整体等价性见 U15。 |
| U04 | 新增 body-only `client_metadata.mcp_attribution`，只有官方目的地带内部元数据 | **透传已兼容**。JSON 归一化只重建安装身份，保留其它 metadata；官方客户端对自定义地址主动省略的字段不伪造。 |
| U05 | Guardian classifier 每次租用前解析 workspace，连接 key/凭据代际、约束 HTTP 禁止重定向 | **已有代理路由适用，A01/A02已修复**。两角色均走供应 Responses 与连接校验；路由/禁止重定向/代际回归通过，实际审批结果留给客户端。 |
| U06 | guardianv2.thread_context 默认启用，跨 compaction hash 让后端验证，额外政策/保留授权上下文改变 | **透传已兼容**，属于客户端审查上下文；代理不制造授权或改审批流程，在线 Guardian 全动作未验。 |
| U07 | Realtime 支持配置代理/网络策略，跨线程保持语音，可选 v3 reasoning status | **配置代理及帧透传已有，A05补入口**；本地传输/参数/归属/限额测试通过，真实音频/线程切换未在线验。 |
| U08 | standalone search 的每请求及重定向重新选路，路径/body不变 | **显式供应代理已兼容**；目标是服务端各账号固定代理，不继承桌面系统/PAC。所有在线重定向条件未测。 |
| U10 | image failures 保留 x-codex-imagegen-request-id | **HTTP响应透传已兼容**，保持请求ID与图像ID区别；图像调用和真实成功数量计费测试通过，在线出图未测。 |
| U11 | invalid_prompt 独立分类，network policy denied不可当瞬时失败 | **错误正文透传及A03修复通过**；不把策略拒绝改成功。 |
| U12 | 远程插件 OAI-Product-Sku 可配置、默认codex；extensions返回 | **本地安装记录仍可读，完整目录/授权执行未实现**；不能借供应私有数据实现；见F12。 |
| U13/U14 | daemon默认启用、fork/import/时间戳/MCP目标/技能缓存；Bedrock/Gateway/TUI/沙箱内部变化 | **客户端实现不适用代理移植**；产生的服务请求仍列入审计。未扩展其它提供商，launcher保持原生runtime/默认profile。 |
| U15 | 官方即时撤销在途HTTP/WS网络策略 | **保留未对齐项**。本项目HTTP/SSE没有统一即时撤销，WS按消息/生成边界检查；此项已核查并报告，不作全部行为一致声明。 |
| U16 | SQLx宏/AWS/crossterm/release workflow变化；WS fork revision不变 | **不需移植依赖升级**。保留已有Cargo.lock及crate边界，不依赖官方crates；Windows验证通过，其它发布目标未在本机运行。 |

主要官方证据（均锁定 TARGET）：

- [workspace resolve_routing](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/app-server/src/request_processors/account_processor/workspace_routing.rs#L427)，[旧基线同分支](https://github.com/openai/codex/blob/b412ff32c417f855c2b2d1581b77058eed87c84b/codex-rs/app-server/src/request_processors/account_processor/workspace_routing.rs#L427)。
- [模型目录](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/models-manager/models.json)、[Responses metadata](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/core/src/responses_metadata.rs#L325)。
- [Guardian connection pool](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/ext/guardian-v2/src/async_scorer/sampler/connection_pool.rs)、[WS error](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/codex-api/src/endpoint/responses_websocket.rs#L561)。
- [file upload](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/codex-api/src/files.rs)、[Realtime](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/codex-api/src/endpoint/realtime_websocket/methods.rs)、[插件SKU](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/core-plugins/src/remote.rs#L2371)。

## 验证证据与限制

| 验证层 | 实际结果 |
| --- | --- |
| 编译 | 受影响8个crate及 `cargo check --locked --workspace` 通过；前端静态导出后 `cargo build --locked -p codex2api` 通过，产物 `target/debug/codex2api.exe` |
| Rust 回归 | API/Admin/Auth/Accounts/Storage/Service/Version 共165项通过、2项原有忽略；上游40项通过。后续增加的UA存量/幂等和只读通话记录2项单独通过，共207个不同测试通过；针对当前Desktop的重复执行不重复计数 |
| 前端 | lint、Prettier、TypeScript、17项contract tests及Next静态build通过 |
| 本次改动的Desktop reader | 当前安装包workspace `$Te`、模型`YVt/yVt`、voice `SFc`及其真实call ID schema、main/native launcher边界实际执行；输入包含真实本地API响应或传输捕获 |
| 原生客户端 | 官方0.157.0 standalone app-server（release archive SHA256核对）和现有Desktop自行管理的runtime分别读取实际API fixtures；thread/start、turn/start、WS预热/生成、turn/completed成功；只使用进程内fixture外部token、默认profile及临时测试工作目录 |
| 启动器 | 地址路由/Worker断点/原生启动合约通过；生产hook未新增业务修改，GUI全流程未运行 |
| 二进制启动 | 独立测试SQLite与127.0.0.1:18080启动，healthz为true、version准确；Ctrl+C停止；未操作生产库 |
| 在线/跨平台 | 未调用真实上游模型、没有全GUI点击验收，Linux/macOS发布目标未在本机验证。2个忽略项为真实proxy网络检查及旧完整Desktop登录脚本；原生fixture成功不替代它们 |

逐条请求包括方法/path/query、数据归属、分页/空非空、错误与下游动作。当前登记152条，兼容挂载展开300个组合；反向扫描当前main/renderer还有未注册的动态/其它产品候选，全部留在 [审计表](CODEX_REQUEST_AUDIT.md)，不把字符串命中当执行证据。

## 存量数据、使用与恢复

没有新数据库迁移、依赖或锁文件更新。保留installation_id、OS/arch/terminal、时区、代理和凭据；现有启动 `align_user_agents` 幂等同步UA版本派生值，新增测试验证额外fingerprint字段、重启、二次启动均不丢失。供应工作区原始NO_CONSTRAINT快照可直接重读，无需删除账户/快照。UA变更后创建新的进程内HTTP pools；不重算价格快照/历史费用，不发放官方权益。

本地main分支仍保留用户拉取的提交，参考HEAD detached到目标release，没有reset main。`reference/`被git忽略，已提交源码记录足以重建快照。

临时启动使用已构建单进程EXE及独立测试库，见 [LOCAL_TEST.md](LOCAL_TEST.md)。管理页可用HTTP；原生Codex需指向实际HTTPS反向代理入口，配置PUBLIC_BASE_URL不会自行提供TLS。

本次不部署。无需schema回退；如用户切换到真实库启动，先按既有运维流程备份，并在一次只运行一个正式实例的方式下测试。发生兼容问题可回退二进制与代码；不以删除账户、令牌或用量记录恢复。最终基线为0.157.0支持范围的协议参考，剩余功能与行为差异仍明确存在。
