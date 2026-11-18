using System.IO;
using System.Text.Json;
using VrcLogger.Core.Parsing;

namespace VrcLogger.App.Services;

public sealed class AppSettings
{
    public string DbPath { get; set; } = "vrclog.db";
    public List<string> LogFolders { get; set; } = [];

    /// <summary>0 で自動取り込み無効。</summary>
    public int AutoImportIntervalMinutes { get; set; } = 1;

    /// <summary>閉じるボタンで終了せずタスクトレイに常駐する。</summary>
    public bool MinimizeToTray { get; set; } = true;

    /// <summary>生ログ (raw_events) の保持日数。0 で無制限。</summary>
    public int RawLogRetentionDays { get; set; } = 30;

    /// <summary>生ログから除外する行 (前方一致。先頭 '*' で部分一致)。</summary>
    public List<string> RawLogNoisePatterns { get; set; } = RawLogNoise.Default.ToList();

    /// <summary>閲覧用 HTML を前回書き出したフォルダ (保存ダイアログの初期位置)。</summary>
    public string? LastViewerExportFolder { get; set; }

    public ParseOptions ToParseOptions() => new(RawLogNoisePatterns);
}

public sealed class SettingsService
{
    private readonly string _settingsPath;

    /// <summary>Directory the settings file lives in; relative paths resolve against it.</summary>
    public string BaseDir { get; }

    public AppSettings Settings { get; private set; } = new();

    public SettingsService()
    {
        BaseDir = FindRepoRoot() ?? AppContext.BaseDirectory;
        _settingsPath = Path.Combine(BaseDir, "vrclogger.settings.json");
        Load();
    }

    private static string? FindRepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null && !File.Exists(Path.Combine(dir.FullName, "VrcLogger.slnx")))
            dir = dir.Parent;
        return dir?.FullName;
    }

    private void Load()
    {
        if (File.Exists(_settingsPath))
        {
            Settings = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(_settingsPath)) ?? new AppSettings();
        }
        else
        {
            Settings = new AppSettings
            {
                DbPath = "vrclog.db",
                LogFolders =
                [
                    Path.Combine(BaseDir, "logs"),
                    @"%USERPROFILE%\AppData\LocalLow\VRChat\VRChat",
                ],
            };
            Save();
        }
    }

    public void Save() =>
        File.WriteAllText(_settingsPath, JsonSerializer.Serialize(Settings, new JsonSerializerOptions { WriteIndented = true }));

    public string ResolvedDbPath()
    {
        var p = Environment.ExpandEnvironmentVariables(Settings.DbPath);
        return Path.IsPathRooted(p) ? p : Path.Combine(BaseDir, p);
    }

    public List<string> ResolvedLogFolders() =>
        Settings.LogFolders.Select(Environment.ExpandEnvironmentVariables)
            .Select(p => Path.IsPathRooted(p) ? p : Path.Combine(BaseDir, p))
            .ToList();
}
