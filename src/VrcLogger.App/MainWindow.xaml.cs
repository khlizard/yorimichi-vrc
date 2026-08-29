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
        Loaded += async (_, _) =>
        {
            _vm.RefreshAll(); // show whatever is already in the DB immediately
            await _vm.ImportCommand.ExecuteAsync(null);
        };
    }

    private void SetupTrayIcon()
    {
        var menu = new System.Windows.Forms.ContextMenuStrip();
        menu.Items.Add("開く", null, (_, _) => ShowFromTray());
        menu.Items.Add("今すぐ取り込み", null, async (_, _) => await _vm.ImportCommand.ExecuteAsync(null));
        menu.Items.Add(new System.Windows.Forms.ToolStripSeparator());
        menu.Items.Add("終了", null, (_, _) =>
        {
            _exitRequested = true;
            Close();
        });
        _tray = new System.Windows.Forms.NotifyIcon
        {
            Icon = System.Drawing.SystemIcons.Application,
            Text = "VrcLogger",
            Visible = true,
            ContextMenuStrip = menu,
        };
        _tray.DoubleClick += (_, _) => ShowFromTray();
    }

    public void ShowFromTray()
    {
        Show();
        WindowState = WindowState.Normal;
        Activate();
    }

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
