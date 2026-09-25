# 临时启动测试

本次已构建 `target/debug/codex2api.exe`，其中包含管理前端。运行时不需要启动 Node.js。

在仓库根目录的 PowerShell 执行：

```powershell
Set-Location D:\github\Codex2Api-rs
$env:CODEX2API_BIND = '127.0.0.1:8080'
$env:CODEX2API_DB = 'data/codex2api-test.sqlite'
.\target\debug\codex2api.exe
```

这是独立测试库；首次管理员为 `admin` / `admin`，从 `http://127.0.0.1:8080/admin/` 登录。授权供应账户、配置模型/套餐、创建消费账户并绑定执行供应后再测试客户端。若8080已被占用，换成8081并同步修改访问地址。

另开 PowerShell 验证运行的版本：

```powershell
Invoke-RestMethod http://127.0.0.1:8080/healthz
Invoke-RestMethod http://127.0.0.1:8080/version
```

应看到 `ok: true`、`package_version: 0.157.0` 和 commit `00c972ed5d6ff6499317fd41b7f23605b8e6850d`。在启动窗口按 **Ctrl+C** 停止；再次运行同一命令保留测试库。要使用已有账号数据，显式把 `CODEX2API_DB` 改成对应数据库路径，先停止使用该库的现有正式实例并保留备份。

SQLx 会按迁移文件的原始字节校验历史。迁移 SQL 在 `.gitattributes` 中固定使用 LF，避免 Windows 的 `core.autocrlf` 将其转换为 CRLF 后，出现 `migration 1 was previously applied but has been modified`。本次已将工作区迁移文件恢复为仓库原始 LF；重新 `cargo run --locked -p codex2api` 即会编译正确校验值。无需删除已有数据库或更改 `_sqlx_migrations` 记录；非换行导致的真实 SQL 修改仍会报校验错误。

本次修复验证：现有库的39个校验值均匹配LF原文；8项迁移回归通过。用原库副本启动新二进制后，healthz正常、全部迁移记录和各表记录数保持不变，原库文件SHA256未变；验证副本已清理。

## 原生 Codex / Desktop 测试地址

官方原生工作区发现要求 **HTTPS**。上述HTTP地址可用于管理页及HTTP接口检查；原生客户端使用已经配置TLS的反向代理域名，转发到本地绑定端口，且须支持WebSocket。

例如现有反向代理地址为 `https://proxy.example.com`，在启动服务前设置：

```powershell
$env:CODEX2API_PUBLIC_BASE_URL = 'https://proxy.example.com'
```

Desktop启动器填 `https://proxy.example.com/api/oauth/chatgpt`。这个环境变量只声明公开地址，**不会自动启用TLS**。没有现成HTTPS入口时，先完成管理页测试；不要将HTTP URL的协议文字直接改成HTTPS。

依次检查：浏览器登录→账户/额度/模型读取→新建对话→连续两轮→取消生成→重新打开会话。供应详情应能把`NO_CONSTRAINT`显示为实际官方bootstrap origin，不能再出现`workspace backend origin is invalid`。记录查询可查看请求失败原因及通话创建记录；文件上传、云插件/自动化等已知未实现项见[逐请求审计](CODEX_REQUEST_AUDIT.md)。完整语音仍需实际供应支持，有限额且不能计价的请求继续明确拒绝。

## 修改源码后的重新构建

使用新开的 PowerShell，使已安装的Rust PATH生效：

```powershell
.\scripts\build.ps1
```

该脚本执行前端安装、检查、静态导出及Rust构建。仅修改Rust且前端导出仍有效时可运行 `cargo build --locked -p codex2api`。发布构建使用 `scripts/build.ps1 -Release`，产物在 `target/release/`；本次临时测试使用已验证的debug产物。
