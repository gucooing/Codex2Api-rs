# Desktop 启动器

发布为一个自包含的 `Codex2API.DesktopProxy.exe`。填写服务器地址，保存后启动已安装的客户端。界面的“配置目录”打开该服务的独立配置；首次登录后由客户端写入 `auth.json`，以后正常刷新令牌。再次启动保留现有配置和登录状态。

每个规范化服务地址对应 `%LOCALAPPDATA%/Codex2API/DesktopLauncher/profiles/<SHA-256>` 下的 `codex` 和 `desktop` 目录，分别保存配置、凭据、SQLite 和桌面应用数据。首次初始化 `config.toml` 设置文件凭据存储及服务地址。不同服务不混用凭据，也不复制默认 `.codex` 数据。导入、导出功能只处理启动器设置。

## 实现

界面、加载及地址处理均由 C# 实现。`NativeHook` 编译为 NativeAOT 组件并嵌入 EXE；运行时按内容校验提取。没有 JavaScript/CJS 脚本、字符串求值、调试端口、断点、调试协议或运行时开关修改。

商店安装从当前用户的清单发现路径、版本和应用标识。Windows 以应用包身份启动同一个 C# 程序，再创建原客户端的挂起进程，加载启动组件后恢复运行，避免直接启动 `WindowsApps` 文件时的访问拒绝。组件在进程内接入原有加载入口及 Node-API，由 C# 回调处理 Electron、Fetch、HTTP、HTTP2、WebSocket 和 Worker 的请求。Worker 直接加载编译后的组件，不生成脚本。原安装文件和原生运行时不复制、不改写。

原生 app-server 继续由客户端正常创建。启动时读取其有效配置并传入地址覆盖；登录消息只关闭官方托管成功页跳转，保留 PKCE、state、回调及其他登录选项。后台、前台和恢复会话采用同一地址规则。已有配置、provider、模型、权限和账号归属不由启动器替换。

用户于 2026-09-26 明确要求直接支持 HTTP。仅在配置 HTTP 服务时，组件会在新建的原生进程运行前调整工作区发现及推理路由的协议判断，使其接受 `http` 和 `https`；保留主机、用户名/密码、路径、账号归属、要求和会话一致性检查。匹配客户端实际选择的原生程序，包括其自动更新缓存，不假定它一定使用安装包目录中的版本。处理位置从该程序的指令特征发现并校验唯一性，不使用固定地址、安装路径或版本号。未知结构会终止本次新进程。

Desktop 和 Worker 的工作区读取还包含单独的 HTTPS origin 验证。C# 回调仅为当前配置的 HTTP origin 适配这一验证使用的解析结果；实际 origin、URL 和请求仍为 HTTP。工作区路由省略服务路径前缀时，由同一地址规则恢复前缀。其他地址、凭据和路径约束保持原检查。不建立本地 HTTPS 转发、不安装证书。

当前原生加载与 HTTP 指令匹配支持 Windows x64。其他架构尚未验证。

## 编译

Windows 上需要 .NET 10 SDK 和 Visual C++ NativeAOT 工具链：

```powershell
dotnet publish tools/desktop-proxy/DesktopProxy.csproj -c Release -r win-x64 --self-contained true -o target/desktop-proxy/publish
```

构建会先编译并嵌入 NativeAOT 组件。分发只需输出的 EXE，用户不需要安装开发工具。

## 验证

`--self-test <报告路径>` 检查地址规则、配置隔离与保留、清单发现、参数转义及界面布局。实际启动测试使用 `--test-launch <测试配置路径> <结果路径>`，成功结果包含原客户端 PID 和 `nativeHookInstalled`；它使用单独的测试目录，不更改日常登录。

`tools/desktop-proxy-tests` 是开发测试工具，调用相同的 C# 地址规则及 HTTP 兼容代码。配合 `scripts/windows/Test-DesktopOAuth.py` 和 Rust 隔离服务测试实际登录、工作区发现、请求、重启及刷新。测试工具不随启动器发布。启动、登录和推理证据分别记录，不能把编译或组件加载成功当作后续请求成功。
