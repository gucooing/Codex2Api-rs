# 临时服务验证

使用云端 CI 或 Release 生成的二进制，在独立测试目录运行。管理前端已嵌入二进制，运行时不需要 Node.js。

在放置二进制的目录执行：

```powershell
$env:CODEX2API_BIND = '127.0.0.1:8080'
$env:CODEX2API_DB = 'data/codex2api-test.sqlite'
.\codex2api.exe
```

测试库首次管理员为 admin/admin。通过管理页授权供应账户、配置模型和套餐、创建消费账户并绑定执行供应。
已有人工价格、停用和删除状态应保留；有完整预设的新支持模型应自动出现并启用。

```powershell
Invoke-RestMethod http://127.0.0.1:8080/healthz
Invoke-RestMethod http://127.0.0.1:8080/version
```

核对 package_version、release_commit 与仓库版本常量一致。Ctrl+C 停止服务；重新运行保留该测试库。
不要让测试实例与正式服务同时使用同一个 SQLite 文件。

ccodex 配置服务根 BASE_OAUTH_URL 后，可检查浏览器/设备码登录、模型和额度读取、连续两轮生成、
取消及客户端重连。使用实际反向代理入口时配置 CODEX2API_PUBLIC_BASE_URL 并启用 WebSocket；
该环境变量只声明地址，不自动提供 TLS。客户端重连由客户端负责，代理不重放生成。

检查用量记录中的实际模型、错误、上游 request ID 和费用快照。未知价格、未报告用量及
[未实现能力](ARCHITECTURE.md#client-routes-and-capability-boundaries)不能用假成功代替。

编译、Rust 测试、前端导出和六平台打包由云端工作流验证。本地允许源码、格式和不编译的脚本检查。
迁移 SQL 使用 LF，不能修改已应用的迁移或手工改写校验记录。
