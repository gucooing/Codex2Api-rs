using System.Net;

namespace DesktopProxy;

internal sealed class MainForm : Form
{
    private readonly TextBox server = new() { Dock = DockStyle.Fill, PlaceholderText = "https://你的服务器/api/oauth/chatgpt" };
    private readonly CheckBox automatic = new() { Text = "自动检测已安装客户端（更新后自动使用新版本）", AutoSize = true, Checked = true };
    private readonly TextBox clientPath = new() { Dock = DockStyle.Fill, ReadOnly = true };
    private readonly Label version = new() { Text = "尚未检测客户端", AutoSize = true, ForeColor = Color.DimGray };
    private readonly Label status = new() { Text = "填写服务地址后，检查并启动。", AutoSize = true, Dock = DockStyle.Fill };
    private readonly TextBox log = new() { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical, Dock = DockStyle.Fill, BorderStyle = BorderStyle.FixedSingle };
    private readonly Button browse = new() { Text = "选择程序…", AutoSize = true, Enabled = false };
    private readonly Button detect = new() { Text = "重新检测", AutoSize = true };
    private readonly FlowLayoutPanel actions = new() { AutoSize = true, Dock = DockStyle.Fill, WrapContents = true };
    private readonly CancellationTokenSource lifetime = new();
    private readonly bool testing;

    public MainForm(bool testing = false)
    {
        this.testing = testing;
        Text = "Codex2API · Desktop 启动器";
        Font = new Font("Microsoft YaHei UI", 10F);
        AutoScaleMode = AutoScaleMode.Dpi; ClientSize = new Size(850, 590); MinimumSize = new Size(750, 530);
        StartPosition = FormStartPosition.CenterScreen; BackColor = Color.FromArgb(246, 247, 249);
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(24), ColumnCount = 1, RowCount = 12 };
        for (var i = 0; i < 11; i++) layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        Controls.Add(layout);
        layout.Controls.Add(new Label { Text = "原版客户端 · 自定义服务", Font = new Font(Font.FontFamily, 19F, FontStyle.Bold), AutoSize = true, Margin = new Padding(0, 0, 0, 8) });
        layout.Controls.Add(new Label { Text = "按服务地址保存独立配置与登录状态，使用已安装的原版客户端。", AutoSize = true, ForeColor = Color.DimGray, Margin = new Padding(0, 0, 0, 20) });
        layout.Controls.Add(new Label { Text = "服务器地址", AutoSize = true }); layout.Controls.Add(server);
        layout.Controls.Add(automatic);
        var paths = new TableLayoutPanel { ColumnCount = 3, Dock = DockStyle.Fill, AutoSize = true, Margin = new Padding(0, 4, 0, 4) };
        paths.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));paths.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));paths.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        paths.Controls.Add(clientPath);paths.Controls.Add(browse);paths.Controls.Add(detect);layout.Controls.Add(paths);
        layout.Controls.Add(version);
        var hint = new Label { Text = "登录、授权及令牌刷新均使用上述自定义服务。启动前请完全退出当前 Desktop。", AutoSize = true, MaximumSize = new Size(770, 0), Margin = new Padding(0, 12, 0, 12) }; layout.Controls.Add(hint);
        AddAction("保存配置", () => { var settings = ReadForm(); settings.Server = Launcher.NormalizeServer(settings.Server); ClientProfile.ForServer(settings.Server).Prepare(); settings.Save(Settings.FilePath); Report("配置已保存，独立登录将在客户端内完成。"); return Task.CompletedTask; });
        AddAction("检查连接", Check);
        AddAction("启动客户端", Launch, true);
        AddAction("配置目录", () => { var profile = ClientProfile.ForServer(server.Text); profile.Prepare(); System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(profile.CodexHome) { UseShellExecute = true }); return Task.CompletedTask; });
        AddAction("导出配置…", Export);
        AddAction("导入配置…", Import);
        layout.Controls.Add(actions); layout.Controls.Add(status);
        layout.Controls.Add(new Label { Text = "运行日志", AutoSize = true, Margin = new Padding(0, 12, 0, 5) });layout.Controls.Add(log);
        automatic.CheckedChanged += (_, _) => { browse.Enabled = !automatic.Checked; clientPath.ReadOnly = automatic.Checked; };
        browse.Click += (_, _) => { using var dialog = new OpenFileDialog { Filter = "应用程序 (*.exe)|*.exe", Title = "选择 Codex / ChatGPT 主程序" }; if (dialog.ShowDialog(this) == DialogResult.OK) { clientPath.Text = dialog.FileName; version.Text = "手动选择的客户端"; } };
        detect.Click += async (_, _) => await Run(async () => { var found = await ClientInstallation.Detect(lifetime.Token); Display(found); });
        if (!testing)
        {
            try { Display(Settings.Read(Settings.FilePath)); } catch (Exception e) { Report("读取配置失败：" + e.Message, true); }
            Shown += async (_, _) => { if (automatic.Checked) await Run(async () => Display(await ClientInstallation.Detect(lifetime.Token))); };
        }
        FormClosed += (_, _) => lifetime.Cancel();
    }
    private void AddAction(string text, Func<Task> action, bool primary = false)
    {
        var button = new Button { Text = text, AutoSize = true, Padding = new Padding(8, 4, 8, 4), Margin = new Padding(0, 0, 8, 8) };
        if (primary) { button.BackColor = Color.FromArgb(35, 79, 149); button.ForeColor = Color.White; button.FlatStyle = FlatStyle.Flat; }
        button.Click += async (_, _) => await Run(action); actions.Controls.Add(button);
    }
    private async Task Run(Func<Task> action)
    {
        actions.Enabled = detect.Enabled = false;
        try { await action(); }
        catch (OperationCanceledException) { if (!IsDisposed) Report("操作已取消或超时。", true); }
        catch (Exception e) { if (!IsDisposed) Report(e.Message, true); }
        finally { if (!IsDisposed) actions.Enabled = detect.Enabled = true; }
    }
    private Settings ReadForm() => new() { Server = server.Text.Trim(), AutoDetect = automatic.Checked, ClientPath = automatic.Checked ? "" : clientPath.Text.Trim() };
    private void Display(Settings value) { server.Text = value.Server; automatic.Checked = value.AutoDetect; clientPath.Text = value.ClientPath; }
    private void Display(ClientInstallation value) { clientPath.Text = value.Executable; version.Text = "客户端版本：" + value.Version; Report("已找到客户端。更新后会重新检测安装位置。"); }
    private async Task<ClientInstallation> Resolve(CancellationToken cancel)
    {
        var found = automatic.Checked ? await ClientInstallation.Detect(cancel) : await ClientInstallation.ResolvePath(clientPath.Text, cancel);
        Display(found); return found;
    }
    private async Task Check()
    {
        var root = Launcher.NormalizeServer(server.Text);server.Text = root;
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token); timeout.CancelAfter(TimeSpan.FromSeconds(20));
        var client = await Resolve(timeout.Token); await Task.Run(() => Launcher.CheckInstallation(client), timeout.Token);
        using var http = new HttpClient(new HttpClientHandler { AllowAutoRedirect = false });
        using var response = await http.GetAsync(root + "/backend-api/me", timeout.Token);
        if (response.StatusCode != HttpStatusCode.Unauthorized) throw new InvalidOperationException($"服务检查返回 HTTP {(int)response.StatusCode}，请核对代理基础地址。");
        Report("连接正常，可以启动客户端。");
    }
    private async Task Launch()
    {
        var root = Launcher.NormalizeServer(server.Text); server.Text = root;
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token); timeout.CancelAfter(TimeSpan.FromSeconds(60));
        var client = await Resolve(timeout.Token);
        var settings = ReadForm();settings.Server = root;settings.Save(Settings.FilePath);
        var pid = await Launcher.Start(client, root, new Progress<string>(s => Report(s)), timeout.Token);
        Report("客户端已启动，可以关闭本启动器。");
    }
    private Task Export()
    {
        var settings = ReadForm(); settings.Server = Launcher.NormalizeServer(settings.Server);
        using var dialog = new SaveFileDialog { Filter = "启动器配置 (*.json)|*.json", FileName = "desktop-proxy-settings.json" };
        if (dialog.ShowDialog(this) == DialogResult.OK) { settings.Save(dialog.FileName); Report("配置已导出，不包含登录凭证。"); }
        return Task.CompletedTask;
    }
    private Task Import()
    {
        using var dialog = new OpenFileDialog { Filter = "启动器配置 (*.json)|*.json" };
        if (dialog.ShowDialog(this) == DialogResult.OK) { var settings = Settings.Read(dialog.FileName);settings.Server = Launcher.NormalizeServer(settings.Server);Display(settings);Report("配置已导入，请检查后保存或启动。"); }
        return Task.CompletedTask;
    }
    private void Report(string message, bool error = false)
    {
        if (IsDisposed) return;
        status.Text = message;status.ForeColor = error ? Color.Firebrick : Color.FromArgb(30, 75, 60);
        log.AppendText($"[{DateTime.Now:HH:mm:ss}] {message}{Environment.NewLine}");
    }
}
