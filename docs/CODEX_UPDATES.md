# 官方 Codex 更新维护流程

更新顺序为“官方最新稳定 tag → `gucooing/codex` 的 `ccodex` 分支 → 本仓库”。客户端在独立仓库维护，典型本地路径为 `../codex`；官方 tag commit 与 fork commit 分别记录。客户端的服务地址和安装名称是定制差异，不能当作供应端官方协议常量。服务端仍执行下述源码分析、方案裁决和验证流程。

## ccodex 客户端重建

ccodex 每次都从官方最新稳定发布 tag 指向的完整 commit 重新建立，只使用该 tag 的代码，不使用官方 `main` 或其他未发布的仓库代码。annotated tag 必须解析到 commit；alpha 等预发布 tag 只在用户明确指定时使用。

在该 tag 之上只保留以下差异，其余官方代码保持不变，不加入服务路由层或可配置的服务根：

| 差异 | 内容 |
| --- | --- |
| 发布 | `.github/workflows/ccodex.yml`、`scripts/ccodex/*` 和 `codex-cli` npm 打包；推送 `ccodex-v<版本>` tag 后发布 npm `@gucooing/ccodex` 和 GitHub Release |
| 服务器地址 | 客户端连接的官方服务地址（`auth.openai.com`、`chatgpt.com`、`api.openai.com` 等）替换为 `https://oauth-ai.alsl.xyz/v1/chatgpt`，原路径保留；JWT 声明名、issuer 等协议标识不改 |
| 与官方隔离 | 命令为 `ccodex`；默认目录为 `~/.ccodex`，可用 `CCODEX_HOME` 或 `CODEX_HOME` 覆盖；更新检查和安装脚本使用 gucooing 渠道；daemon 使用包内入口 `bin/ccodex` |

重建步骤：

1. 查询官方 `openai/codex` 的最新稳定 release，记录 tag、完整 commit 和发布时间。
2. 在 `../codex` 从该 commit 新建 `ccodex` 分支，重新应用上表的现有 ccodex 提交并解决冲突。
3. 在该 tag 的代码中搜索官方服务地址，替换新增的地址；把 `scripts/ccodex/upstream.json` 更新为该 tag 及其官方包的 checksum。
4. 只做源码审查、格式检查和不编译的打包检查（`node --check codex-cli/bin/ccodex.js`、`scripts/ccodex` 单元测试）；Rust 编译由发布 CI 完成。
5. 打 `ccodex-v<版本>` tag，同一版本的打包修订使用 `+N` 后缀。用户审核后，强制推送 `ccodex` 分支并推送 tag。
6. 在 AGENTS.md 和[架构说明](ARCHITECTURE.md)中更新官方 tag commit 与 fork commit。服务端是否跟进该版本，按下文流程另行提出方案。

代理的编译检查与 Rust 回归测试在本地执行。ccodex 按用户要求不在本地编译，只执行源码审查、格式和不触发编译的打包检查；Rust 测试、clippy、schema 生成及原生构建均保留为未验证，除非用户另行授权相应验证方式。不要为了获得云端 CI 验证而推送改动或新建分支。检查未完成或失败时如实记录，不能以已推送代替验证通过。

本文件供维护本项目的 AI 在收到“更新 Codex”“升级到 0.155.0”“跟进官方最新版本”等命令时执行。目标是让本项目与目标发布版中**已登录 ChatGPT 账户的官方 Codex CLI**保持一致的应用层请求行为，包括身份、认证、请求内容、传输与会话语义。

更新分为：**查明版本与差异 → 提交更新方案 → 用户裁决 → 按批准范围实施 → 验证并更新基线**。普通更新命令只启动调查和方案编写；必须先把具体方案交给用户，获得裁决后才能实施。

## 1. 确认当前基线与目标

开始前读取以下文件，并检查本地未提交改动，保留与本次更新无关的工作：

- [AGENTS.md](../AGENTS.md)：项目约束及当前固定版本。
- [版本常量](../crates/codex2api-common/src/providers/chatgpt/version.rs)：实际使用的版本、commit、协议常量。
- [README.md](../README.md)、[架构说明](ARCHITECTURE.md)：产品范围和实现边界。
- `reference/SOURCE.md`、`reference/codex`：本地官方源码来源与快照。

分别记录以下值，不能混为一个“当前版本”：

| 名称 | 来源与用途 |
| --- | --- |
| `BASE_COMMIT` | 当前 `CODEX_REF_COMMIT`，本项目实际参考的官方源码基线，是主要差异比较的起点 |
| 当前发布版及 commit | `CODEX_RELEASE_VERSION`、`CODEX_RELEASE_TAG`、`CODEX_RELEASE_COMMIT`，用于补充说明官方发布版之间的变化 |
| 当前 UA 版本 | `CODEX_PACKAGE_VERSION`，核对实际发出的 User-Agent |
| `TARGET_TAG` | 用户指定版本对应的官方发布 tag；例如 `0.155.0` 对应 `rust-v0.155.0`，需实际核实 |
| `TARGET_COMMIT` | `TARGET_TAG` 最终指向的完整源码 commit；annotated tag 必须解析到 commit，不能使用 tag 对象的 SHA |

若用户要求“最新版本”，查询官方 `openai/codex` 发布记录，确定当时最新的稳定发布版；只有用户明确指定时才选择预发布版。核实版本、tag、完整 commit、发布日期、发布说明和查询时间，把结果写入方案。`0.155.0` 在本文中只是示例，不代表已查询到该发布版。

以本地代码与官方仓库证据为依据。文档、版本常量或快照不一致时，先报告差异并查明实际基线；无法确认的值标记为待确认。不能用官方 `main` 最新 commit 代替目标发布版 commit，也不能凭版本号猜测 commit。

`reference/` 当前被 `.gitignore` 排除，源码快照也可能没有 `.git`。允许将官方远程仓库克隆或拉取到实例文件夹中的独立目录，也可使用新的临时目录，用于获取 commit、tag 并分析源码差异。保留现有快照，不修改本项目的 `origin`。缺少本地参考文件时，可以从版本常量中记录的官方 commit 恢复分析所需源码。无法取得任一端源码时，说明受阻原因，不据此宣称完成比较。

## 2. 比较源码，确认官方实际改了什么

主要比较必须是 **`BASE_COMMIT` 与 `TARGET_COMMIT` 两个完整源码树之间的差异**。先检查全部变更文件、提交记录、重命名和删除，再沿调用链阅读相关实现、测试、默认配置与构建配置。发布说明和 PR 描述用于帮助定位，不能替代源码比较。

以下为 PowerShell 分析命令示例；执行时必须重新读取当前基线、核实目标 tag，不能长期照抄示例版本：

```powershell
$baseCommit = "a8964cb1bad67bc26a826fb07d1bef99c6a3f008"
$targetTag = "rust-v0.155.0"
$analysisDir = Join-Path ([System.IO.Path]::GetTempPath()) ("codex-review-" + [guid]::NewGuid().ToString("N"))
git clone --filter=blob:none --no-checkout https://github.com/openai/codex $analysisDir
if ($LASTEXITCODE -ne 0) { throw "无法取得官方仓库" }
git -C $analysisDir fetch origin $baseCommit "refs/tags/${targetTag}:refs/tags/${targetTag}"
if ($LASTEXITCODE -ne 0) { throw "无法取得基线或目标 tag" }
$targetCommit = git -C $analysisDir rev-parse "${targetTag}^{commit}"
if ($LASTEXITCODE -ne 0) { throw "无法解析目标 commit" }
git -C $analysisDir show --no-patch --format=fuller $targetCommit
git -C $analysisDir log --left-right --oneline "${baseCommit}...${targetCommit}"
git -C $analysisDir diff --stat $baseCommit $targetCommit
git -C $analysisDir diff --name-status --find-renames $baseCommit $targetCommit
git -C $analysisDir diff --find-renames $baseCommit $targetCommit
```

`git diff BASE TARGET` 比较两个端点；不要用 `git diff BASE...TARGET` 替代，它从共同祖先开始比较，可能遗漏当前参考快照已有的行为。上例 `git log --left-right BASE...TARGET` 仅用于识别两端各自独有的提交。

当前基线与目标发布版可能不在同一条连续历史上。即使目标版本号较新，也要识别参考快照中未进入目标发布版的修改，以及目标中的回退、删除和替代实现。必要时补充“当前发布版 commit → 目标发布版 commit”的比较，分别说明官方发布变化和本项目真正需要适配的变化。

至少覆盖以下影响面，并结合实际变更补充；官方文件路径发生变化时按符号与调用链查找，不依赖固定目录列表：

| 影响面 | 要确认的官方行为 | 本项目重点位置 |
| --- | --- | --- |
| 请求身份 | originator、UA 公式与发布版版本、安装标识、账户头、客户端元数据、头部添加条件 | `codex2api-common::version`、`codex2api-suppliers`、`codex2api-upstream/src/providers/chatgpt/headers.rs`、`codex2api-suppliers/src/providers/chatgpt/transport.rs` |
| 认证 | OAuth issuer/client_id/scopes、PKCE、回调、设备码、刷新令牌、撤销、错误码与重试 | `codex2api-suppliers`、管理端授权入口 |
| 请求协议 | endpoint、方法、字段、默认值、模型目录、推理强度、service tier、路由提示、工具与多模态内容 | `codex2api-upstream`、`codex2api-oauth` |
| 传输与会话 | HTTP/WS 握手、压缩、SSE 事件、增量输入、预热、重连、回退、取消、动态 ID 和连接状态生命周期 | `codex2api-upstream`、`codex2api-oauth/src/handlers` |
| 账户与用量 | 官方账户接口、额度窗口及重置时间、套餐、usage 字段和流结束条件 | `codex2api-admin`、`codex2api-oauth/src/usage.rs`、`codex2api-storage` |
| 构建与依赖 | 发布时注入的版本、默认 feature、影响上述行为的依赖变化和跨平台要求 | 官方发布流程、相关依赖；本项目 `Cargo.toml`、`Cargo.lock`、发布工作流 |

对每项变更确认它在目标发布版是否启用，以及适用的认证方式、模型、feature flag 和平台。源码中存在某个分支不等于发布版默认采用；其他供应商或 API Key 模式的行为不能直接套到 ChatGPT 订阅模式。

官方 CLI 的界面、编辑器、沙箱或工具执行变化也应列入变更概览，再说明是否影响本项目的对外协议、请求内容或元数据；没有影响的项目标为“不适用”，无需移植。

## 3. 先提交可裁决的更新方案

分析阶段可以读取代码、查询官方仓库、运行只读检查，以及编写方案；**不得提前修改业务实现、协议常量、依赖、PostgreSQL 数据、当前参考快照或已对齐版本声明**。

把方案写入临时工作文档 `docs/CODEX_UPDATE_PLAN.md`，在对话中提供该文件入口和需要用户决定的内容。保持同一份方案随用户裁决更新，记录日期与目标 commit，避免多个方案版本相互矛盾；不复制为历史版本归档。方案至少包含：

1. 当前参考 commit、当前发布版及 commit、目标发布版/tag/commit、官方证据链接与查询时间。
2. 官方客户端更新概览，以及从当前参考快照到目标发布版的实际行为差异。
3. 逐项适配表：官方旧行为、新行为、证据、本项目当前行为、受影响文件、建议修改、验证方法与风险。
4. 每项结论的状态：**需要同步、已兼容、不适用、待确认**。已透传或已兼容的变化也写明依据，不能只列准备改的部分。
5. 既有账户、持久化指纹、数据库、HTTP 客户端和会话状态的处理方式，以及是否需要重新构建或重启。
6. 实施顺序、验证范围、失败时的恢复方式，以及尚未取得证据的部分。
7. 明确的用户裁决项：批准整个方案、仅批准指定项目、要求调整或暂不更新。

证据优先引用官方仓库中固定 commit 的源码路径、符号或行号，辅以对应提交、测试和发布说明。不能用可变的 `main` 链接代替目标 commit 证据，不能把猜测写成已确认的协议规则。

**提交方案后等待用户裁决。** 用户已经明确批准某个具体方案时，按批准范围继续，不重复询问。若后续发现需要更换目标 commit、扩大实现范围或增加未批准的数据处理，先更新方案并取得对新增部分的裁决。

## 4. 按批准方案实施

只实施获准项目，遵守现有 crate 边界。官方源码仅供参考，不引入官方 `codex-rs` crates 作为项目依赖。

应用层身份对齐继续遵守账户隔离规则：

- 保留每个已有账户的 `installation_id`、OS、架构、终端等持久化身份；不因升级或重新授权而重新随机生成，不读取代理宿主机作为账户环境。
- 需要更新 UA 时，按目标发布版的真实版本和公式生成。不能只改版本字符串而遗漏协议变化，也不能取官方源码树的占位版本 `0.0.0`，或本项目自身的发布 tag 作为官方 UA 版本。
- 固定值按渠道集中定义在 `codex2api-common/src/providers/<provider>/`，包括发布版/UA 版本、originator、官方地址、OAuth client_id/scopes、头名和协议路径；身份生成、认证和传输统一引用，不在各调用点重复字面量。管理员配置的本地服务地址仍属于配置，不能替代官方供应端常量。
- 数据库只保存账户独有的安装 ID、OS、系统版本、架构、终端、Cookie 等冻结身份或状态；完整 UA、UA 包版本和固定 originator 不作为账户配置持久化。UA 由共享常量与冻结输入在运行时生成，新旧账户使用同一个生成函数；普通版本升级不执行 UA 数据迁移。
- 清理现存的固定值/派生值存储时，方案须单独列出一次性 SQL 迁移：先从现有指纹提取并保留账户原有的终端等必要输入，再删除冗余字段，同时更新当前 schema。无效指纹应明确报错，不重新随机生成、不增加旧格式运行时读取。不得通过改写历史迁移代替新迁移，也不得修改请求账本中记录的客户端 UA。
- 核对新账户、已有账户和重新授权三条路径，以及刷新令牌、额度探测和推理所用 HTTP 客户端。客户端创建及缓存必须使用当前共享常量和原有账户输入；重新构建并重启进程后生效，不从持久化旧 UA 回填。
- auth、cookie、HTTP 客户端和会话状态继续逐账户隔离；session/thread/turn/request ID 按官方生命周期保持动态。
- 保留现有代理、时区及凭据配置。需要改变其行为时，必须已经在获准方案中说明。
- 继续采用官方应用层头部与协议规则，不扩展为 TLS/JA3 伪造。

数据库结构需要调整时维护当前 schema、种子数据和按版本自动执行的 SQL 迁移，并在隔离数据库验证。迁移 SQL 必须保留；应用处理器只读取当前 schema。

目标源码应先单独准备并核对 commit。在实现和验证完成后，再把目标作为新的 `reference/codex` 快照，并更新 `reference/SOURCE.md`。若只实施了部分方案且仍有必要的兼容工作未完成，不得将整个项目标记为已完整对齐新版本。

## 5. 验证并更新维护基线

针对实际变化验证官方请求语义，包含请求路径、头部、请求体、压缩、认证交换和流事件等受影响部分。优先使用本地 mock 服务检查发出的请求，并覆盖相关旧数据和账户隔离回归。测试数据不得包含真实 token、API Key 或用户内容。

至少运行受影响 crate 的 `cargo check -p <crate>` 和对应测试；涉及公共依赖或跨 crate 接口时检查工作区。涉及依赖、原生编译或平台代码时，同时验证受影响的发布目标。区分通过的检查、既有失败和未验证项；不能把仅编译通过表述为真实上游行为已验证。

获准更新完成并通过相应检查后，统一更新以下基线记录：

- `crates/codex2api-common/src/providers/chatgpt/version.rs`：参考 commit、来源、时间、提交说明、官方发布版/tag/commit、UA 版本及有证据变化的协议常量；同步相关版本断言。
- `AGENTS.md`、`README.md`、`docs/ARCHITECTURE.md`：当前对齐版本与相应行为说明。
- `reference/SOURCE.md` 和本地 `reference/codex`：实际采用的目标源码快照。该目录被忽略，其他工作区必须能根据已提交的 commit 信息重新取得源码。
- `docs/CODEX_UPDATE_PLAN.md`：实施期间保存批准范围、验证状态和未完成项。任务完成后删除；长期有效的行为及限制并入 README、架构或相应当前规范。

文档只维护当前有效状态。更新完成后删除历史版本方案、排查记录、审计日志和实施日记，并清理所有引用；不要新增归档 Markdown 或“前次实施记录已归档”之类的叙述。Git 历史及云端 CI 保留提交、验证和发布的追溯信息。

下一次更新从这个已核实、已完成适配的 `CODEX_REF_COMMIT` 继续比较。交付时说明官方改了什么、本项目同步了什么、哪些无需改、验证结果和使用新构建所需步骤。

提交、推送、本项目打 tag/发 Release、重启运行中的服务及操作真实数据库，按用户对这些动作的实际授权执行；获准适配官方新版本不自动包含发布或部署。已有授权应沿用，不重复请求确认。

## 使用示例

用户下达“把官方 Codex 更新到 0.155.0”时，AI 应先核实官方 `rust-v0.155.0` 对应的完整 commit，以当前 `CODEX_REF_COMMIT` 为起点比较源码，形成方案并等待裁决。用户批准后实施获准变化，通过验证，再将目标 commit 记为新基线。不能在提出方案之前先修改 UA 或宣布已经升级。
