# WebSocket 上下文压缩顺序修复

## 原因与修复

供应账户配置时区时，`codex2api-upstream/src/timezone.rs` 会更新客户端的环境片段，或插入一个日期／时区消息。旧代码在输入中没有用户消息时直接追加到末尾。Responses WebSocket 的增量压缩请求可以只有 `compaction_trigger`，因此旧代码把有效的 `[compaction_trigger]` 变成 `[compaction_trigger, environment_context]`，触发 `The 'compaction_trigger' item must be the final input item.`。

现在找不到用户消息时，新增环境片段插在已有压缩标记之前。原有输入项之间的顺序、工具结果、加密压缩内容和 `previous_response_id` 都保留；已有环境片段仍原位更新，重复处理不追加第二份。HTTP JSON、HTTP zstd 和 WS 文本／UTF-8 二进制帧共用这段修复。不删除压缩标记、不关闭压缩、不修改客户端。

管理入口仍是供应账户详情“指纹与网络 / 时区”，补充了对话与上下文压缩使用该配置的说明。管理测试验证保存后详情读回同一时区。请求记录继续使用现有“用量记录”详情；转发回归检查压缩输出交付、完成状态和实际报告用量的结算，不添加提示词或响应内容存储。

## 协议与客户端证据

- 固定基线 `00c972ed5d6ff6499317fd41b7f23605b8e6850d`：`reference/codex/codex-rs/core/src/compact_remote_v2_attempt.rs` 在最后追加 `ResponseItem::CompactionTrigger {}`，收到压缩输出后从保存的输入中移除此标记。`core/src/client.rs` 的增量输入逻辑在历史前缀一致时发送差量，并携带前一响应 ID。
- 通过安装包清单发现本机 Desktop `OpenAI.Codex 26.924.2738.0`，其 `app/resources/codex.exe --version` 为 `0.158.0-alpha.2.1`。这是额外客户端兼容证据，没有改变本仓库固定基线或协议常量。
- 实际安装包 `app.asar` 的 `.vite/build/main-DAwJoFgo.js` 与 `webview/assets/app-shared-c568b0b98683.js` 以 `thread/compact/start`、`{threadId}` 构造压缩操作，保留会话 owner 检查；`webview/assets/execution-8517b007c046.js` 读取并映射 `contextCompaction` 的类型与 ID。未修改这些文件。
- 使用原版已安装 app-server、全新临时配置目录和仅监听 localhost 的 WS 测试上游，执行 `initialize → thread/start → turn/start → thread/compact/start → turn/start`。实际捕获的压缩请求为 `type=response.create`、`previous_response_id=response-2`、`input=[{"type":"compaction_trigger"}]`。原版客户端读取测试 `compaction` 输出，发出 `item/started`、`item/completed`（均为 `contextCompaction`），并完成后续普通对话。
- 本次本机证据保存在忽略目录 `target/review-ws-compaction/desktop-readers.json` 与 `native-evidence.json`；这些是隔离测试记录，不是远端用户会话或官方推理结果。

## 验证范围

新增顺序回归先在旧代码下失败，实际末项是错误追加的环境消息；修复后通过。上游 43 项单元／传输测试通过，包括纯标记、工具输出加标记、已有压缩项、完整用户输入、重复处理及 HTTP JSON/zstd。WS 10 项测试通过，其中实际 socket 桥接验证文本帧与二进制帧、会话关联、压缩输出、完成事件与持久化计费，并保留额度／鉴权拒绝和关闭帧的既有测试。

原版 app-server 验证的是请求构造、响应读取和下一轮执行；代理 socket 测试验证修复后的服务端转发与结算。此次没有启动或修改 GUI launcher，也未访问、部署或验证用户截图中的远端实例，不把测试上游当作真实官方压缩成功。
