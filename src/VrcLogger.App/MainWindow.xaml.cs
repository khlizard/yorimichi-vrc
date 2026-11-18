using System.ComponentModel;
using System.Windows;
using VrcLogger.App.Services;
using VrcLogger.App.ViewModels;

namespace VrcLogger.App;

public partial class MainWindow : Window
{
    private readonly MainViewModel _vm;
    private readonly SettingsService _settings;
    private System.Windows.Forms.NotifyIcon? _tray;
    private bool _exitRequested;

    public MainWindow()
    {
        InitializeComponent();
        _settings = new SettingsService();
        _vm = new MainViewModel(_settings);
        DataContext = _vm;
        SetupTrayIcon();
    }

    /// <summary>起動時の初期表示と初回取り込み。ウィンドウを表示するかどうかとは独立に呼ぶ。</summary>
    public async Task StartAsync()
    {
        _vm.RefreshAll(); // show whatever is already in the DB immediately
        await _vm.ImportCommand.ExecuteAsync(null);
    }

    private void SetupTrayIcon()
    {
        var menu = new System.Windows.Forms.ContextMenuStrip();
        menu.Items.Add("開く", null, (_, _) => ShowFromTray());
        menu.Items.Add("今すぐ取り込み", null, async (_, _) => await _vm.ImportCommand.ExecuteAsync(null));
        menu.Items.Add("閲覧用 HTML を書き出す…", null, async (_, _) => await _vm.ExportViewerCommand.ExecuteAsync(null));
        menu.Items.Add(new System.Windows.Forms.ToolStripSeparator());
        menu.Items.Add("終了", null, (_, _) => ExitApp());
        _tray = new System.Windows.Forms.NotifyIcon
        {
            Icon = CreateTrayIcon(),
            Text = "VrcLogger",
            Visible = true,
            ContextMenuStrip = menu,
        };
        _tray.DoubleClick += (_, _) => ShowFromTray();
    }

    private static System.Drawing.Icon CreateTrayIcon()
    {
        var resource = Application.GetResourceStream(new Uri("pack://application:,,,/Assets/icon.ico"));
        if (resource is null)
            return System.Drawing.SystemIcons.Application;

        using var stream = resource.Stream;
        var size = System.Windows.Forms.SystemInformation.SmallIconSize;
        return new System.Drawing.Icon(stream, size.Width, size.Height);
    }

    public void ShowFromTray()
    {
        Show();
        WindowState = WindowState.Normal;
        Activate();
    }

    private void ExitApp()
    {
        _exitRequested = true;
        Close();
    }

    private void Exit_Click(object sender, RoutedEventArgs e) => ExitApp();

    protected override void OnClosing(CancelEventArgs e)
    {
        if (!_exitRequested && _settings.Settings.MinimizeToTray)
        {
            // 常駐モード: 閉じる = 隠すだけ。自動取り込みタイマーは動き続ける。
            e.Cancel = true;
            Hide();
            return;
        }
        _tray?.Dispose();
        base.OnClosing(e);
    }

    private void ClearDates_Click(object sender, RoutedEventArgs e)
    {
        _vm.DateFrom = null;
        _vm.DateTo = null;
    }
}
