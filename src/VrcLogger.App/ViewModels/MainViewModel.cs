using System.IO;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;
using VrcLogger.Core.Export;
using VrcLogger.App.Services;

namespace VrcLogger.App.ViewModels;

public sealed partial class MainViewModel : ObservableObject
{
    private readonly SettingsService _settings;
    private readonly DispatcherTimer _autoImportTimer = new();
    private SqliteConnection _queryConn;

    public QueryService Query { get; private set; }
    public PlayersViewModel Players { get; }
    public SessionsViewModel Sessions { get; }
    public UrlsViewModel Urls { get; }
    public LogsViewModel Logs { get; }
    public AvatarsViewModel Avatars { get; }
    public NotificationsViewModel Notifications { get; }
    public SettingsViewModel SettingsTab { get; }

    [ObservableProperty]
    private string _status = "";

    [ObservableProperty]
    private bool _isImporting;

    [ObservableProperty]
    private int _selectedTabIndex;

    [ObservableProperty]
    private DateTime? _dateFrom;

    [ObservableProperty]
    private DateTime? _dateTo;

    public string WindowTitle => $"VrcLogger — {_settings.ResolvedDbPath()}";

    public MainViewModel(SettingsService settings)
    {
        _settings = settings;
        _queryConn = Db.Open(settings.ResolvedDbPath());
        Db.EnsureCreated(_queryConn);
        // Cheap and idempotent: recovers avatar history from raw log lines kept in the DB.
        new ImportService(_queryConn).BackfillAvatarEventsFromRawEvents();
        Query = new QueryService(_queryConn);

        Players = new PlayersViewModel(this);
        Sessions = new SessionsViewModel(this);
        Urls = new UrlsViewModel(this);
        Logs = new LogsViewModel(this);
        Avatars = new AvatarsViewModel(this);
        Notifications = new NotificationsViewModel(this);
        SettingsTab = new SettingsViewModel(settings, ReopenDatabase, RestartAutoImportTimer, VacuumAsync, ReimportAllAsync, ExportViewerAsync);

        _autoImportTimer.Tick += async (_, _) => await ImportAsync();
        RestartAutoImportTimer();
    }

    public void RestartAutoImportTimer()
    {
        _autoImportTimer.Stop();
        var minutes = _settings.Settings.AutoImportIntervalMinutes;
        if (minutes > 0)
        {
            _autoImportTimer.Interval = TimeSpan.FromMinutes(minutes);
            _autoImportTimer.Start();
        }
    }

    partial void OnDateFromChanged(DateTime? value) => RefreshAll();
    partial void OnDateToChanged(DateTime? value) => RefreshAll();

    [RelayCommand]
    private async Task ImportAsync()
    {
        if (IsImporting)
            return;
        IsImporting = true;
        var folders = _settings.ResolvedLogFolders();
        var dbPath = _settings.ResolvedDbPath();
        var parseOptions = _settings.Settings.ToParseOptions();
        var retentionDays = _settings.Settings.RawLogRetentionDays;
        var noisePatterns = _settings.Settings.RawLogNoisePatterns.ToList();
        var viewerPath = _settings.ResolvedViewerExportPath();
        IProgress<string> progress = new Progress<string>(msg => Status = msg);
        var changed = true;
        try
        {
            changed = await Task.Run(() =>
            {
                // Import uses its own connection so UI-thread queries never collide with it.
                using var conn = Db.Open(dbPath);
                Db.EnsureCreated(conn);
                var svc = new ImportService(conn, parseOptions);
                var imported = svc.ImportFolders(folders, progress).Imported > 0;

                var pruned = svc.Prune(retentionDays, noisePatterns);
                if (pruned.Total > 0)
                    progress.Report($"生ログ整理: {pruned.ExpiredDeleted} 期限切れ / {pruned.NoiseDeleted} ノイズ を削除");
                if (pruned.Total >= VacuumThresholdRows)
                {
                    progress.Report("DB 最適化中 (VACUUM)…");
                    var before = Db.FileSizeBytes(conn);
                    Db.Vacuum(conn);
                    progress.Report($"DB 最適化完了: {before / 1024 / 1024} MB → {Db.FileSizeBytes(conn) / 1024 / 1024} MB");
                }
                var changedNow = imported || pruned.Total > 0;

                // 別PC閲覧用 HTML: 変更があったとき、またはまだ無いときだけ書き出す。失敗しても取り込みは成功扱い。
                if (viewerPath is not null && (changedNow || !File.Exists(viewerPath)))
                {
                    try
                    {
                        ViewerExporter.Export(conn, viewerPath, DateTime.Now);
                        progress.Report($"閲覧用 HTML を書き出しました: {Path.GetFileName(viewerPath)}");
                    }
                    catch (Exception ex)
                    {
                        progress.Report($"閲覧用 HTML の書き出しエラー: {ex.Message}");
                    }
                }
                return changedNow;
            });
        }
        catch (Exception ex)
        {
            Status = $"取り込みエラー: {ex.Message}";
        }
        finally
        {
            IsImporting = false;
        }
        // 自動取り込みで変更がないたびに一覧を再読込すると選択状態が飛ぶので、変更時のみ更新。
        if (changed)
            RefreshAll();
    }

    public void RefreshAll()
    {
        Players.Refresh();
        Sessions.Refresh();
        Urls.Refresh();
        Avatars.Refresh();
        Notifications.Refresh();
        Logs.Refresh();
    }

    /// <summary>Rows pruned in one import run above which the DB is compacted automatically.</summary>
    private const int VacuumThresholdRows = 10_000;

    private async Task ReimportAllAsync()
    {
        if (IsImporting)
            return;
        var dbPath = _settings.ResolvedDbPath();
        await Task.Run(() =>
        {
            using var conn = Db.Open(dbPath);
            var svc = new ImportService(conn);
            svc.BackfillAvatarEventsFromRawEvents();
            svc.MarkAllForReimport();
        });
        await ImportAsync();
    }

    private async Task ExportViewerAsync()
    {
        var viewerPath = _settings.ResolvedViewerExportPath();
        if (viewerPath is null)
        {
            Status = "閲覧用 HTML の書き出し先が空のため、書き出しは無効です";
            return;
        }
        var dbPath = _settings.ResolvedDbPath();
        try
        {
            await Task.Run(() =>
            {
                using var conn = Db.Open(dbPath);
                ViewerExporter.Export(conn, viewerPath, DateTime.Now);
            });
            Status = $"閲覧用 HTML を書き出しました: {viewerPath}";
        }
        catch (Exception ex)
        {
            Status = $"閲覧用 HTML の書き出しエラー: {ex.Message}";
        }
    }

    private async Task VacuumAsync()
    {
        if (IsImporting)
            return;
        IsImporting = true;
        var dbPath = _settings.ResolvedDbPath();
        try
        {
            Status = "DB 最適化中 (VACUUM)…";
            var (before, after) = await Task.Run(() =>
            {
                using var conn = Db.Open(dbPath);
                var b = Db.FileSizeBytes(conn);
                Db.Vacuum(conn);
                return (b, Db.FileSizeBytes(conn));
            });
            Status = $"DB 最適化完了: {before / 1024 / 1024} MB → {after / 1024 / 1024} MB";
        }
        catch (Exception ex)
        {
            Status = $"DB 最適化エラー: {ex.Message}";
        }
        finally
        {
            IsImporting = false;
        }
    }

    public void NavigateToVisit(long? visitId)
    {
        if (visitId is not { } id)
            return;
        SelectedTabIndex = 1; // Sessions tab
        Sessions.SelectVisitById(id);
    }

    private void ReopenDatabase()
    {
        _queryConn.Dispose();
        _queryConn = Db.Open(_settings.ResolvedDbPath());
        Db.EnsureCreated(_queryConn);
        Query = new QueryService(_queryConn);
        OnPropertyChanged(nameof(WindowTitle));
        RefreshAll();
    }
}
