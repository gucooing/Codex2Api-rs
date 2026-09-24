# Desktop 图形启动器

C# / WinForms 编写，交付为单个 `Codex2API.DesktopProxy.exe`，内嵌地址 hook。发布包包含 .NET 运行时，使用时不需要 Node.js、PowerShell 启动脚本或另行安装 .NET。

打开程序，填写代理基础地址，点击“检查连接”，完全退出已有 Desktop 后点击“启动客户端”。原版客户端自行管理默认凭证、会话、应用数据和工具宿主。本工具不复制客户端、不创建隔离 profile、不写客户端配置，也不使用 app-server shim。

默认通过当前用户的安装包清单检测客户端及其主程序；每次启动重新发现，因此不保存固定安装版本路径。便携安装可取消自动检测后手动选择 EXE。地址、自动检测选项和手动路径保存在当前用户的 `Codex2API/DesktopLauncher/settings.json`，只包含启动器设置；可通过界面导出、导入，在另一台设备重新检测安装位置。

地址映射集中在 `AddressHook.cjs` 的 `proxyPolicy`。Electron 会话、`net.fetch/request`、Node Fetch/HTTP/HTTPS/HTTP2、WebSocket 和 Worker 的请求出口共用该规则，把安装包使用的 OpenAI/ChatGPT 服务、静态资源、Statsig、内部分析和更新服务地址改到界面配置的服务，保留路径、查询、方法和正文；重定向也经过同一规则。域名规则集中维护，无需为新接口逐一增加业务 hook。未实现的服务接口仍返回其实际错误，不回退直连官方。

原生 app-server 不经过 JavaScript 网络栈。启动时由已安装的原生程序短暂读取有效配置，再将地址覆盖作为命令行参数传给正常运行的原版程序；不会写配置文件、替换 provider 或复制运行时。省略 `base_url` 的自定义 provider 也会被识别，避免回退官方地址。共享 JSON-RPC 发送边界处理会话中额外指定的地址配置，普通生成、后台生成和恢复会话使用相同规则。当前不支持通过 WSL 包装命令启动的原生传输，检测到时明确报错。

hook 在启动时检查安装包的认证地址检查和原生传输边界；模块文件名从安装包读取，不绑定 hash 文件名或固定版本。若未来版本改变结构，界面报告不兼容并结束本次新进程。Worker 使用从 EXE 内嵌代码生成的临时地址预加载文件，保留原有入口、显式参数和断点清理，主进程正常退出时删除该临时文件。

浏览器桥接等本地通信保持原始参数：非代理服务路径的回环 Fetch、内部协议及 HTTP 命名管道不构造新的 Request、不丢失原生选项或改变重定向模式。配置的代理服务请求和官方地址仍执行原有路由规则。该修复避免 hook 干扰内部通信；浏览器工具的通用连接错误仍须通过实际客户端复测确认根因。

登录入口始终使用自定义域名的 `/codex/desktop-auth`，其 `authorize_url` 指向自定义服务的 `/oauth/authorize`；刷新与撤销分别使用 `/oauth/token`、`/oauth/revoke`。原版回调服务的官方托管成功页会跳回 ChatGPT，因此只将这个地址选项 `useHostedLoginSuccessPage` 设为 false，使用原版本地成功页；保留简化登录、PKCE、state、回调和认证过程。不会修改模型、工作区准备或权限逻辑。

登录地址在统一请求出口及 `shell.openExternal` 出口使用同一规则处理；保留嵌套授权链接的 state、PKCE 和回调参数。原先改写登录响应、浏览器业务函数和 bootstrap 工作区路由的 hook 已移除。非官方站点链接及本地回调不变。

临时回环调试端口用于安装 hook，随后关闭；工作线程排除启动器自身添加的暂停参数。其他客户端业务和运行时选择均保持原版行为。

## 编译

在 Windows 上安装 .NET 10 SDK，仓库根目录执行：

```powershell
dotnet publish tools/desktop-proxy/DesktopProxy.csproj -c Release -r win-x64 --self-contained true -o target/desktop-proxy/publish
```

产物为 `target/desktop-proxy/publish/Codex2API.DesktopProxy.exe`。ARM64 可把运行时参数改为 `win-arm64`。PDB 是调试信息，分发时只需要 EXE。

## 验证

开发检查 `--self-test <报告路径>` 验证设置保存、更换地址、安装发现、默认凭证/运行时选择，并生成实际 GUI 图像。`scripts/windows/Test-DesktopProxyHook.cjs` 直接读取当前安装包，执行地址判断和登录包装函数。OAuth 集成测试可通过 `CODEX2API_TEST_GUI` 指向该 EXE，用测试账号和临时数据启动原版 Desktop；临时目录仅用于测试，不是产品隔离功能。
