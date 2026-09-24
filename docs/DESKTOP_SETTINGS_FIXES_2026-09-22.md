# Desktop 设置与 SQLite 锁冲突修复（2026-09-22）

## 后续修正：成年解绑提示接口

补齐 `GET /amphora/u18_graduation_unlink_setting_notices` 和 `POST /amphora/u18_graduation_unlink_setting_notices/dismiss`。实际 Desktop `settings-8d6c2878e79a.js` 的 `$e/et` 读取 `notices`，按 `recipient_type=teen/parent` 分组，展示 `teen_display_name` 和可选的 `learn_more_url`；关闭时发送当时捕获的 `notice_ids`，成功后失效重读。

接口查询本虚拟账户在 SQLite `virtual_resources` 中的 `family_graduation_notice` 记录。没有记录时返回 `{"notices":[]}`，不会访问供应账户的家庭数据，也不会因读取接口而生成成年/解绑事实。存储提供按来源事件去重的记录入口；**实际年龄核验及自动解绑事件生产仍未接入，不能把此读取修复当成完整家庭业务已经实现。**

关闭操作在同一事务中先检查整批 ID 的账户归属，再记录关闭时间和实际登录设备；重复关闭保留原时间，未知/跨账户混合批次不改变任何记录，新出现的提示不会被误关闭。管理页“客户端记录 → 家庭关联提示”只读展示所有记录及关闭状态，禁止通过管理表单制造提示。

`Test-DesktopFamilyNotices.mjs` 执行原客户端查询、角色分组、关闭回调和刷新，通过实际测试 HTTP 服务验证非空提示及捕获之后新增的提示。接口回归还覆盖未登录、跨账户、重复关闭、来源事件去重、只读管理记录和进程重启后的保留。完整年龄核验和自动解绑不在本次修复完成范围内。

## 后续修正：Ultra 滑块开关不可操作

之前的语音读回修复在未选择语音时输出 `settings.voice_name: null`，但实际 Desktop `kfr` schema 定义的是 `J().optional()`。因此即使 `/settings/user` 返回 200，整份设置仍解析失败；`agent-settings` 的 `pr` 组件在用户设置不存在时令 Ultra 开关 `disabled=true`。这不是开启入口 gate 就能修复的问题，也不是用户没有选择 Ultra 档位。

现未选择语音时省略 `voice_name`，已选语音仍返回真实字符串。Ultra 保存后的原始 `tjr` 会失效并重读用户设置和 `/tpp/models/`，因此同时将该模型路径接到本虚拟账户的模型数据源，支持带/不带尾斜杠。管理端“客户端记录”按原界面名称显示“模型选择器滑块中的 Ultra”，继续只读。

新增 `Test-DesktopUltraSlider.mjs`，执行发布代码的完整 `kfr` schema、`xfr` 读取、`pr` 控件属性和 `tjr` 保存函数，连接实际测试 Router 完成开启→关闭→开启的 HTTP 往返。验证两条失效读取均成功、SQLite 保存、重启保留、其他偏好/服务 flags 不被覆盖，以及跨账户隔离。相关虚拟 OAuth/API 回归 30 项通过、1 项按原配置忽略；编译检查与程序构建通过。

此后续修复构建位于 `target/desktop-ultra-slider-20260922/codex2api-fixed.exe`，需要再次用新代码启动代理。未改写用户的实际 Ultra 选择，也未通过桌面 UI 自动操作；验证使用原客户端代码及真实测试 HTTP 服务。

## 电脑操控

用户截图是 Desktop 设置中的“电脑操控”，Chrome 显示“已被你的组织停用，或在你所在地区不可用”。实际发布代码 `computer-use-settings-77945f81b828.js` 的 `Ir/Kr` 会在浏览器可用性为 false 时显示该文案；`app-initial-6c4523b43a11.js` 的 `AWn/jWn` 还把缺少 Statsig gate `410065390` 判为不可用。整机操控的 `uHn/fHn` 使用 `1506311413`。

修复前实际 bootstrap 不包含这两个 gate。独立调用当前原生运行时的 `experimentalFeature/list` 确认 `browser_use_external`、`computer_use` 均已启用，`configRequirements/read` 没有禁止浏览器/电脑使用。因此没有修改 Desktop 的权限检查、审批、本地配置或安装状态。

新增本账户 `computer_use_policy`，管理网页“客户端配置 → 浏览器与电脑操控”提供两个具名开关，默认允许。bootstrap 从 SQLite 生成相应 gate；Windows 工作区响应保留显式已有拒绝，同时遵守电脑操控策略。

## 推理强度

Desktop 配置页 `agent-settings-e300d87c4341.js` 的 `br` 由 gate `3693343337` 决定是否渲染“模型功能 / 可用推理强度”；`PLa/O9n` 使用 `1186680773` 决定是否展示实际模型支持的 Ultra。这两个 gate 原来也缺失。

通过当前原生 app-server 的 `model/list` 实际读取到 Max/Ultra 支持，包括 gpt-6-astra 和 gpt-5.6-sol/terra；没有向模型目录添加虚构档位。新增 `desktop_model_policy`，管理网页“客户端配置 → Desktop 推理强度设置”分别管理设置入口和 Ultra 可见性。具体档位选择继续使用原客户端 `TLa → enabledReasoningEfforts` 写入流程；没有代写用户偏好或权限。

## 家庭信息

`presentation-7092c5138f20.js` 的当前家庭查询把 404 当成错误并清空相关查询缓存。设置页 `Zt` 的正常无家庭分支要求成功返回对象且 `id == null`，不是 404 或 JSON null。

本地家庭记录为 null 时，现返回 `200 {"id":null,"role":null}`。已有家庭继续读取本账户持久化资料，新增成员读取入口并校验家庭归属、支持分页；尚无成员采集数据时明确返回不可用，不伪造空成员。管理页显示真实无家庭状态，家庭成员保持只读。此次修复读取，不表示家庭邀请、加入和家长控制写入已经全部实现。

## SQLite 锁冲突

确认的两个问题：

1. `virtual_config`、`oauth_signing_key`、`desktop_support_settings` 即使已有记录也先执行 `INSERT OR IGNORE`，只读请求因此竞争写锁。新增测试在另一个连接持有写事务时复现旧实现阻塞；改为已有数据直接读取后通过。
2. 连接器目录有约 4,200 条。之前每条保存都开启事务、更新父账户主键为自身、读取旧值并写入；目录刷新产生大量串行写事务。非会话资源现在单条 upsert，目录每 64 项执行一次批量 upsert。会话仍使用 `BEGIN IMMEDIATE` 事务保证资源和事件原子写入，没有移除事件或放弃记录。

并发回归：4,300 条目录刷新两轮，同时执行 320 次额度读取和 640 次记录写入，约 1.4 秒完成，全部记录保留、无锁错误。没有通过延长 busy timeout 或吞掉错误掩盖问题。

## 验证与运行状态

- Storage/API/Admin 相关测试共 107 项通过、2 项按原配置忽略；推理强度补充后重跑组合测试通过。主程序 `cargo check` 和修复程序构建通过。
- `Test-DesktopControlsAndFamily.cjs` 使用安装包原 Statsig SDK、电脑/浏览器判断函数、配置页组件、模型筛选及偏好保存函数验证；也执行原家庭组件的错误、无家庭和有成员分支。
- 当前原生运行时 `0.155.0-alpha.9.2` 使用默认客户端凭证与配置完成只读 `experimentalFeature/list`、`configRequirements/read`、`model/list`。模型读取样本：`target/desktop-controls-20260922/native-models.json`。
- 本轮未修改固定 Codex 基线、安装包、生产启动器、客户端审批或本地安装状态。未执行实际鼠标/键盘操控；原函数验证不等于一次实际电脑任务执行。
- 用户于 `2026-09-22T02:33:33Z` 重启本地代理后，实际 `/amphora` 返回 200（当前无家庭），bootstrap 的四个 gate 均为 true，`/wham/usage` 返回 200；连接器目录实读 4,242 条，返回 200。
- 查询到的最近四条目录 500 均在 `02:33:00Z–02:33:16Z`，早于新进程启动。复验时没有新进程启动后的 500 记录；不把这一观察当成永久不存在数据库错误的承诺。
- SQLite 备份：`target/desktop-controls-20260922/before-update-20260922T023053Z.sqlite`。账号、虚拟账号、设备数量保持 2/1/1，用量记录从 469 增至 473。没有删除历史错误或用量。
