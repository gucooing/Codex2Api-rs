# Responses 状态处理重写

## 协议依据

固定基线不变：`00c972ed5d6ff6499317fd41b7f23605b8e6850d`。本轮从网络获取了该提交的官方 [SSE 实现](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/codex-api/src/sse/responses.rs)，并核对本地固定快照的 [WebSocket 实现](https://github.com/openai/codex/blob/00c972ed5d6ff6499317fd41b7f23605b8e6850d/codex-rs/codex-api/src/endpoint/responses_websocket.rs)。官方文档站和联网搜索工具在本环境不可用，未把未能读取的文档作为证据。

- `response.completed` 才完成正常生成；`response.failed` 提供生成错误；`response.incomplete` 保留未完成原因。
- SSE 建立后的 HTTP 200 和 WS 的 101 属于连接状态；生成结果由事件决定。保留事件正文及其中的身份、状态、错误、重试/重置字段，不把事件失败改造成 HTTP 错误。
- WS `error` 的 `status` / `status_code` 是事件内的数值状态；错误的 `code` / `type` 用于进一步分类。
- 官方把 `rate_limit_exceeded` / `slow_down` 归为临时限流，把 `insufficient_quota`、积分余额或支出额度耗尽归为额度错误。未知 429 不猜原因。

## 实现边界

删除旧的通信故障封停逻辑与散落的事件状态判定，统一使用 `outcome.rs` 的每轮状态机。HTTP 和 WS 共用，终态不可被后续断线覆盖；请求用量按实际报告结算一次。HTTP 初始拒绝仍保留真实状态、错误正文和允许转发的重试响应头。

账户状态只因明确的上游 401 拒绝对应凭据版本而变化；网络错误、超时、403、限流、额度耗尽及服务端错误不修改凭据状态。刷新端点的状态和错误码保留为结构化值。旧凭据的迟到拒绝不影响新凭据。迁移 0046 保留历史诊断与账本，旧通信错误不再阻止执行，不重写历史计费和未知状态。

管理端提供“授权失效 / 检查授权”，用量详情区分连接 HTTP 状态、生成错误状态、失败类别和原始错误码。未知历史错误仍显示未知，不从旧的 200 推断成功或具体失败码。

## 验证

- 服务端：HTTP/WS 事件分类、原始事件转发、分片/CRLF SSE、额度与限流、未完成、错误终态、连接中断、已有完成记录、凭据版本并发与账本结算；账户查询、管理记录与原有隔离回归。
- 本机原版 Desktop：从安装清单发现 `OpenAI.Codex 26.924.2738.0`，原生 `codex-cli 0.158.0-alpha.2.1`。使用临时隔离 profile、原安装目录运行时和本地 Rust fixture。SSE 验证实际执行 `thread/start` / `turn/start` 并发送推理请求；错误分别被原生运行时识别为 `usageLimitExceeded`、`rateLimitExceeded`、`contextWindowExceeded`，回合状态为 failed。
- 原生 WS：当前 Desktop 的 `generate=false` 预热正常完成，后续生成收到代理的结构化 429 事件和关闭帧；原生回合失败信息保留 `httpStatusCode: 429`，没有裸连接重置，受拒绝的生成没有转发到上游。
- launcher：本轮没有修改 launcher；上面的原生运行时验证不代表 GUI/launcher 的端到端验证，也不代表真实官方账户在线推理。
- 前端：TypeScript、ESLint、业务单元测试和静态导出；浏览器验收使用独立临时数据库与本地服务。
