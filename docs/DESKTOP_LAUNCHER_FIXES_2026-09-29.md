# Desktop 界面、通知及共享目录修复

## 本机证据

当前会话的主进程来自已注册的 `OpenAI.Codex 26.924.6891.0`，安装包 `app.asar/package.json` 与应用更新检查均报告 Desktop `26.924.51851`、build `12111`、prod。应用检查返回 `up_to_date`，不是启动器拷贝了旧界面。这里的 Desktop 版本不修改仓库固定的 Codex 协议基线。

安装包 `app-shared-1b7135e7e526.js` 的统一标签栏 selector `NIr` 读取 gate `3528415127`，缺失时为 false。当前服务实际 bootstrap 不含其哈希 `2494272518`。新增持久化 `desktop_ui_policy.unified_tabs_enabled`，默认 true；账号详情的功能开关页提供具名开关。bootstrap、管理员修改和重启后读取使用同一记录，不修改客户端 selector，也不覆盖客户端侧栏或标签偏好。

## 通知

原版 Desktop 使用 Electron Notification。商店包的 Windows 通知按包身份创建，点击会激活包清单中的原客户端，独立用户数据目录并不会改变这个激活目标。

启动器使用同一个编译后的 C# EXE 承接 Windows 通知。通知进程通过 Windows Desktop App Policy 脱离商店包身份，按服务注册 AUMID/COM 激活类，保留标题、正文、静音、提醒、按钮和回复；当前用户管道把事件送回原 Notification 对象。原 Desktop 自己处理打开会话和执行操作。关闭客户端后清理通知并结束承接进程。没有改写商店安装、全局 `codex://` 关联或客户端业务脚本。

## 共享目录

用户 2026-09-29 明确要求只隔离 `auth.json`、`config.toml`。启动器保留它们已有的服务专属路径，客户端的 `CODEX_HOME` 改回原目录，桌面数据使用客户端默认路径。C# 原生文件访问适配同时用于 Electron 和它实际选择的原生 app-server，处理这两个文件的读取、属性查询、原子替换、登录写入和退出删除。其他文件不重定向，不复制原客户端凭据。

旧独立目录保留；本轮不合并运行中的历史 SQLite。共享模式直接使用原客户端已有会话，新会话也写入同一来源。旧独立历史仍在原路径，需要退出后另行迁移。

## 补充接口

- `POST /ces/statsc/flush`：安装包 `WSr` 发送 counters、histograms、client_type，`KSr` 要求 JSON 布尔 `success`。新增受大小限制的解析、数值校验及诊断持久化。客户端原请求不带凭据，记录保持匿名，正文中的账号标签不能冒充账号身份。设置／诊断记录显示指标名称、数值或样本数；不存正文标签里的账号声明。
- `GET /backend-api/wham/environments`：实际 `spn` 与侧栏查询读取数组，`pen/men` 使用 label、repos、repo_map.clone_url。列表从本虚拟账号已经持久化的任务环境元数据中读取、按 ID 去重，管理端“任务关联的云环境”读取相同来源。不读取供应账号环境目录，不从远程主机记录伪造云环境。无任务环境记录时才返回空数组；独立云环境创建、供应端目录发现及没有环境元数据的旧任务补全仍未实现。

## 验证边界

Rust 回归覆盖默认布局、管理端修改、持久化、环境归属、管理记录、匿名指标及重复批次。`Test-DesktopLayoutMetrics.cjs` 执行安装包实际布局 selector、指标请求构造和响应 schema、环境读取函数。

原生配置验证和 Windows 通知激活验证分别执行。隔离 Desktop 测试使用真实安装路径及临时数据目录；通知探针只创建测试通知，不创建用户任务或推理请求。当前用户会话不重启，生产服务需要部署后才会发布新的布局配置和接口。

实际测试结果：原生 app-server 完成独立配置的读取／原子写入、登录／退出及不回退原凭据；在临时共享目录创建的测试会话随后由未加载 hook 的原生客户端读取成功。真实 Desktop 创建的测试通知通过 Windows COM 激活后，其 click 回调报告的 PID 与测试 Desktop 启动结果相同；用户也确认点击成功。测试不代表生产服务已经部署，也不代表当前旧 hook 进程已经替换。
