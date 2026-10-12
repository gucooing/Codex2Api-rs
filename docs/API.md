# 按量 API

按量 API 在同一 Rust 进程的独立端口提供 ChatGPT 和 Grok 模型调用。默认地址为
`http://127.0.0.1:8083/v1`。`CODEX2API_METERED_API_BIND` 控制监听地址，
`CODEX2API_PUBLIC_METERED_API_URL` 提供公开 origin 的启动默认值；管理员在
**设置 / 访问地址 / 按量 API** 保存的 origin 优先。用户 API Key 页面读取这一
地址，不从浏览器地址或转发头推测端口。反向代理可使用独立域名指向 8083，
以保留 OAuth 入口 8080 的兼容路径。

## 调用与平台

所有模型调用与模型发现使用 `Authorization: Bearer <API Key>`。Key 绑定一个
API 组，组的平台决定上游提供商；调用者不能用请求头切换供应账户或平台。
浏览器会话、OAuth access/refresh token 和 API Key 各自独立。

| 路径 | 行为 |
| --- | --- |
| `GET /healthz` | 无凭据的进程健康检查 |
| `GET /v1/models` | OpenAI `object: list` / `data` 模型列表，按 Key 所属组的真实模型范围返回 |
| `POST /v1/responses` | Responses JSON 或 SSE |
| `POST /v1/chat/completions` | Chat Completions JSON 或 SSE，包括函数工具调用和用量 |
| `POST /v1/images/generations` | 图像生成，JSON 请求和响应 |
| `POST /v1/images/edits` | 图像编辑，JSON 图像引用或 multipart 图片上传，JSON 响应 |

ChatGPT 文本请求使用锁定版本的 Codex Responses 上游；Chat Completions 的
消息、函数定义、工具结果与流式增量在 ChatGPT 适配器中转换。Grok 文本调用
使用自身原生接口。流式 Chat Completions 仅在客户端请求
`stream_options.include_usage=true` 时发布最终用量 chunk；其 `choices` 为空。
失败、截断与正常结束保持可区分，完成事件在账本结算后发送。

供应端复用各平台登录后的官方 transport。ChatGPT 的 originator、UA 版本、
OAuth 凭据和冻结的账户指纹来自固定 Codex 基线；Grok 使用独立的官方头和
凭据。请求级标识由适配器生成，用户的 API Key、Cookie、UA、账户选择头与
API 组信息不转发给供应商。公开 API 的兼容转换不能替换供应端登录协议。

ChatGPT 适配采用固定 Codex 请求结构，不支持 `temperature`、`top_p`、
`max_tokens`、`max_completion_tokens`、`max_output_tokens`、`stop`、
`logprobs` 或多候选 `n`；显式指定不支持的参数返回 400，不静默忽略。
Chat Completions 的 `n=1`、函数工具、文本／图像输入、结构化文本格式与
`reasoning_effort` 可转换；当前适配层不接受 Responses 的 `stream_options`。
Grok 使用自己的原生契约，不套用 ChatGPT 的参数和恢复规则。

ChatGPT 图像请求遵循仓库 `reference/codex/codex-rs/codex-api/src/images.rs`
的固定契约：`model`、`prompt`、可选 `n`、`background`、`quality`、`size`，
编辑时发送 `images: [{image_url: ...}]`，结果为 base64。Grok 图像发送独立的
`image: {url, type: "image_url"}` 或多图 `images`，支持 `resolution`、
`aspect_ratio`、平台允许的 `quality` 和 `response_format`。仍使用原有
Grok 订阅 transport 与官方 CLI proxy 地址，账户是否获准使用图像模型取决于
实际供应权限；不切换到开发者付费 API 来掩盖订阅拒绝。

multipart 编辑支持 `image` / `image[]` / `images` / `images[]` 的 PNG、JPEG、
WebP 图片，在内存中转成 data URL；单文件不超过 15 MiB，请求总量不超过
64 MiB。ChatGPT 最多 16 张输入图，Grok 最多 5 张。不下载用户提供的远程图像
URL；URL 由对应供应商处理。图片生成只接受 JSON。

图像流式、异步作业、供应商文件 ID、蒙版，以及当前平台适配器不支持的参数
返回明确的请求错误。不会静默丢弃这些参数或伪造完成响应。Grok 的 `size`
可转换为支持的分辨率和宽高比；省略时显式发送官方默认的 `resolution: "1k"`。
模型名来自管理员目录，代码不建立官方模型白名单。

例如，先获取该 Key 可用的模型，再将返回的模型名放入标准请求。以下命令中的
`API_KEY` 与 `MODEL` 由调用者设置：

```bash
curl http://127.0.0.1:8083/v1/models \
  -H "Authorization: Bearer $API_KEY"

curl http://127.0.0.1:8083/v1/chat/completions \
  -H "Authorization: Bearer $API_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"model\":\"$MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"你好\"}]}"
```

公开协议参考：[OpenAI Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)、
[xAI 图像接口](https://docs.x.ai/developers/rest-api-reference/inference/images)、
[xAI 图像分辨率](https://docs.x.ai/developers/model-capabilities/images/generation)。
公开 API 文档不能替代 ChatGPT 固定供应端版本，接口适配不升级 Codex 基线。

## API 组、倍率和限流

管理员在 **API 组** 维护名称、介绍、平台、供应标签、模型范围与计费倍率。
一个组可关联同平台多个标签；供应商按 ID 去重后复用共享调度器。
组的平台创建后不可更换。删除组为软删除，既有 Key 与账单保留，调用立即失效。
标签范围、模型范围和价格调整分别使用持久化配置，不修改 OAuth 套餐。

组模型范围为：该平台已启用的计费目录 ∩ 关联标签供应账户的可用模型 ∩
组显式模型列表。组模型列表为空时，最后一项不限制。供应账户模型列表的
官方发现与自定义模型均参与这一计算；`/v1/models`、模型广场与执行准入读取
同一模型来源。临时供应商繁忙或额度状态由调度处理，不把它当作永久目录删除。

组倍率默认作用于组内所有模型，模型单独倍率覆盖组倍率，不再次相乘。
倍率使用最多六位小数的十进制定点数，`0` 表示明确免收；缺少基价仍不可
按免费模型执行。已开始请求保留当时的模型价格、服务档位与倍率快照。

并发和 RPM 分别设置在组与模型层，两层同时约束 **用户**。同一用户在同组
创建多个 Key 不能增加额度，不同用户分别计数；`0` 表示该项无限。
RPM 是滚动 60 秒窗口，超过限额返回 429、具体限制代码和 `Retry-After`。
流式任务保留并发占用至实际执行结束，客户端断开后继续收集已产生的用量
并完成结算。当前并发来自进程运行态，进程重启不恢复已结束的执行占用。

## 钱包与记录

API 仅供普通用户创建 Key，虚拟用户不能通过用户网站或 UserStore 获取这一
能力。Key 使用随机秘密，数据库只保存哈希与可展示前缀；完整 Key 仅在
创建成功时返回一次。停用、撤销、用户停用、组停用或删除均阻止后续准入。
请求在等待供应商或重试时重新核对 Key 与组版本。

API 用量独立保存在 `api_usage_records`，不消耗 OAuth 订阅的 5 小时、7 天、
30 天额度。使用量只保存账单元数据，不持久化请求提示、工具输入或图片内容。
模型基价与倍率在开始时快照；完成时以实际普通输入、缓存读取、缓存写入与
输出结算，推理 Token 已包含在输出中，不重复收费。图像根据成功返回数量
和实际分辨率结算，复用模型计费中匹配档位或最近已配置档位的规则；整个模型
未定价、缺失数值或分辨率保持未知。

费用以整数 nano USD 计算，用户钱包仍为整数美分。小于一美分的费用保存于
`api_wallet_state`，累计达到一美分才原子扣减余额并写不可变 `api_usage`
钱包流水，不对每个微小请求单独向上取整。已执行的并发请求可能超过剩余
余额，超出部分保留待扣，后续收费调用受余额检查约束，不丢失真实已消费费用。
管理员增加余额或下一次 API 准入时，先以原账单来源清偿已知的整美分待扣款。
订单支付和管理员减少余额需保留 API 已发生的待扣金额。进程重启将未完成
请求恢复为中断，缺失的用量保持未知，不能按零费用补出成功记录。
结算通过请求 ID 幂等，修改价格或倍率不会重算历史。

管理员 **API 密钥 / API 用量** 查看普通用户的 Key、真实请求、费用与当前
并发。用户 **API 密钥 / API 用量** 只读取本人记录；查询参数无法扩大所有权。
用户 DTO 显式排除供应身份、路由标签、秘密、指纹与供应商额度。
**模型广场** 默认展示管理员模型计费目录，横向切换 API 组后展示模型有效倍率、
倍率后的价格、服务档位、输入范围、图像分辨率以及组和模型的两层限额。
