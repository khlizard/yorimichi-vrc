using Microsoft.Win32;

namespace VrcLogger.App.Services;

/// <summary>Windows サインイン時の自動起動 (HKCU の Run キー)。例外は呼び出し側で処理する。</summary>
internal static class StartupRegistration
{
    public const string TrayArg = "--tray";

    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "VrcLogger";

    private static string? ExpectedCommand() =>
        Environment.ProcessPath is { } path ? $"\"{path}\" {TrayArg}" : null;

    public static bool IsEnabled()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
        return key?.GetValue(ValueName) is not null;
    }

    public static void SetEnabled(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
        if (enabled)
        {
            var command = ExpectedCommand()
                ?? throw new InvalidOperationException("実行ファイルのパスを取得できません。");
            key.SetValue(ValueName, command, RegistryValueKind.String);
        }
        else
        {
            key.DeleteValue(ValueName, throwOnMissingValue: false);
        }
    }

    /// <summary>登録済みで、パスが現在の実行ファイルと違う場合だけ書き直す (移動・更新対策)。</summary>
    public static void RefreshPathIfEnabled()
    {
        var expected = ExpectedCommand();
        if (expected is null)
            return;
        using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
        if (key?.GetValue(ValueName) is string current && current != expected)
            key.SetValue(ValueName, expected, RegistryValueKind.String);
    }
}
