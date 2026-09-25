# 虚拟账户重置卡

用户于 2026-09-25 确认：重置卡使用后清零用量，刷新周期也重新计时，但订阅到期时间不变；发送给客户端的响应必须遵守官方协议，不能夹带本地管理字段。

虚拟账户详情的“重置卡”标签提供发放、使用和记录查询。每次发放 1–100 张，默认立即启用、有效期 30 天；管理员也可指定启用时间和 1–3650 天有效时长，时长从启用时间起计算。启用前或过期后客户端不会获取、也不会看到卡片，管理端仍可看到待启用／过期记录。管理备注只在管理 API 中返回。每张卡使用一次，不附带订阅、付款或供应账户权益。已到期订阅不能通过用卡恢复，未消耗的卡保留。

虚拟账户列表提供逐行 Checkbox、当前页全选和“当前筛选条件下的全部账户”选择。批量删除、直接重置用量、发放重置卡共用 `/admin/api/consumers/batch`；全选筛选结果只提交筛选条件，由 SQLite 查询账户 ID，不将所有匹配账户加载到浏览器。行的“更多”菜单提供直接重置和发卡，使用同一业务校验。直接重置创建管理员内部重置代次，不产生客户端可见卡片。

成功使用会清空当前外层和内层用量，将 7 天／30 天外层起点设为使用时间，清除原 5 小时内层起点；内层仍按套餐规则在下次实际使用时开始。订阅期限、套餐、原订阅生效时间、历史 Token 和已结算费用均不改动。没有当前用量或订阅已到期时返回 `nothing_to_reset`，不扣卡。

迁移 `0040_virtual_reset_credits.sql` 新建本账户的发卡、卡片与使用请求记录，给账户和请求账本增加重置代次。消费卡片、更新代次和保存防重复请求结果在同一个 SQLite `BEGIN IMMEDIATE` 事务中完成。请求入账时原子记录代次；用卡前已开始的请求即使稍后结算，也只增加历史费用，不重新占用新周期。用卡后同一毫秒开始的新请求仍计入新周期。

发卡请求带 `request_id`，同一请求重试不重复发卡；更改该请求的数量或备注会拒绝。用卡使用官方 `redeem_request_id`，账号之间隔离；成功请求重试返回 `already_redeemed`，不会再扣卡或重开周期。不同请求并发选择同一卡，也只有一次成功。管理页在传输失败后复用原请求标识。

固定参考仍为 `00c972ed5d6ff6499317fd41b7f23605b8e6850d`。官方客户端合同为：GET `/wham/usage` 的 `rate_limit_reset_credits.available_count`；GET `/wham/rate-limit-reset-credits` 的 `credits/available_count/total_earned_count` 和官方卡片字段；POST `/wham/rate-limit-reset-credits/consume` 的 `redeem_request_id`、可选 `credit_id`，以及 `reset/nothing_to_reset/no_credit/already_redeemed`、整数 `windows_reset` 和可选 `credit`。客户端响应不返回管理备注、美元计费、供应账户信息或内部窗口数组。证据来自 pinned `backend-client/src/types.rs`、`client/rate_limit_resets.rs` 及 `rate_limit_resets_tests.rs`。

已安装 Desktop 的实际 renderer 函数 `Eai/Oai/Tai/Yii/aIo/Dai` 已通过 `scripts/windows/Test-DesktopResetCredits.cjs` 验证：列表、选卡请求、缓存扣减、传输失败复用同一 UUID 和成功后的额度刷新均遵循原始逻辑。`Test-NativeUpdateContract.py` 同时对官方 0.157.0 与已安装 Desktop 内置 app-server 读取实际服务响应并发送用卡请求。

验证入口：`cargo test -p codex2api-storage --lib --test migrations`、`cargo test -p codex2api-admin --test reset_credits --test contracts --test virtual_accounts`、`cargo test -p codex2api-api --test virtual_oauth reset_credits_match_official_clients -- --nocapture`、`frontend/tests/browser/reset-credits.spec.ts`。原生验证仅使用进程内测试 token 和临时 TLS fixture，不代表部署或真实官方供应用卡。
