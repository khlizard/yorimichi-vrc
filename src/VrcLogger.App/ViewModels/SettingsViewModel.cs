using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.App.Services;

namespace VrcLogger.App.ViewModels;

public sealed partial class SettingsViewModel : ObservableObject
{
    private readonly SettingsService _settings;
    private readonly Action _reopenDatabase;
    private readonly Action _restartAutoImport;
    private readonly Func<Task> _vacuum;
    private readonly Func<Task> _reimportAll;

    public ObservableCollection<string> LogFolders { get; }

    [ObservableProperty]
    private string? _selectedFolder;

    [ObservableProperty]
    private string _dbPath;

    [ObservableProperty]
    private int _autoImportIntervalMinutes;

    [ObservableProperty]
    private bool _minimizeToTray;

    [ObservableProperty]
    private int _rawLogRetentionDays;

    /// <summary>1 行 1 パターン。</summary>
    [ObservableProperty]
    private string _rawLogNoisePatterns;

    [ObservableProperty]
    private string _saveStatus = "";

    public SettingsViewModel(SettingsService settings, Action reopenDatabase, Action restartAutoImport, Func<Task> vacuum, Func<Task> reimportAll)
    {
        _reimportAll = reimportAll;
        _settings = settings;
        _reopenDatabase = reopenDatabase;
        _restartAutoImport = restartAutoImport;
        _vacuum = vacuum;
        _rawLogRetentionDays = settings.Settings.RawLogRetentionDays;
        _rawLogNoisePatterns = string.Join(Environment.NewLine, settings.Settings.RawLogNoisePatterns);
        LogFolders = new ObservableCollection<string>(settings.Settings.LogFolders);
        _dbPath = settings.Settings.DbPath;
        _autoImportIntervalMinutes = settings.Settings.AutoImportIntervalMinutes;
        _minimizeToTray = settings.Settings.MinimizeToTray;
    }

    [RelayCommand]
    private void AddFolder()
    {
        var dialog = new Microsoft.Win32.OpenFolderDialog { Title = "ログフォルダを選択" };
        if (dialog.ShowDialog() == true)
            LogFolders.Add(dialog.FolderName);
    }

    [RelayCommand]
    private void RemoveFolder()
    {
        if (SelectedFolder is not null)
            LogFolders.Remove(SelectedFolder);
    }

    [RelayCommand]
    private void Save()
    {
        var dbChanged = _settings.Settings.DbPath != DbPath;
        _settings.Settings.LogFolders = LogFolders.ToList();
        _settings.Settings.DbPath = DbPath;
        _settings.Settings.AutoImportIntervalMinutes = Math.Max(0, AutoImportIntervalMinutes);
        _settings.Settings.MinimizeToTray = MinimizeToTray;
        _settings.Settings.RawLogRetentionDays = Math.Max(0, RawLogRetentionDays);
        _settings.Settings.RawLogNoisePatterns = RawLogNoisePatterns
            .Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .ToList();
        _settings.Save();
        if (dbChanged)
            _reopenDatabase();
        _restartAutoImport();
        SaveStatus = $"保存しました ({DateTime.Now:HH:mm:ss})";
    }

    [RelayCommand]
    private void ResetNoisePatterns() =>
        RawLogNoisePatterns = string.Join(Environment.NewLine, VrcLogger.Core.Parsing.RawLogNoise.Default);

    [RelayCommand]
    private Task Vacuum() => _vacuum();

    [RelayCommand]
    private Task ReimportAll() => _reimportAll();
}
