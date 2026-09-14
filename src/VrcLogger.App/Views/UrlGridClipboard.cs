using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.Views;

/// <summary>
/// Clipboard helpers for DataGrids bound to <see cref="UrlHit"/> rows. WPF's default
/// Ctrl+C copies every column of the row (and nothing for the Hyperlink column), which
/// is useless when you just want the URL.
/// </summary>
internal static class UrlGridClipboard
{
    /// <summary>Ctrl+C copies only the URL(s) of the selected rows, one per line.</summary>
    public static void OnPreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.C && Keyboard.Modifiers == ModifierKeys.Control && sender is DataGrid grid)
        {
            CopyUrls(grid);
            e.Handled = true;
        }
    }

    public static void CopyUrls(DataGrid grid)
    {
        var urls = grid.SelectedItems.OfType<UrlHit>().Select(u => u.Url).ToList();
        if (urls.Count > 0)
            SetText(string.Join(Environment.NewLine, urls));
    }

    /// <summary>Copies the selected rows as tab-separated text (time, kind, URL).</summary>
    public static void CopyRows(DataGrid grid)
    {
        var rows = grid.SelectedItems.OfType<UrlHit>()
            .Select(u => string.Join("\t", u.EventTime, u.Kind, u.WorldName, u.Url)).ToList();
        if (rows.Count > 0)
            SetText(string.Join(Environment.NewLine, rows));
    }

    /// <summary>
    /// Clipboard monitors (ATOK's clipboard watcher in particular) open the clipboard to read
    /// every new entry, so a second Ctrl+C right after the first can hit CLIPBRD_E_CANT_OPEN.
    /// WinForms' SetDataObject has a built-in retry loop for exactly this; it also writes the
    /// standard OLE format set (Unicode/ANSI text + locale) that other tools expect.
    /// </summary>
    private static void SetText(string text)
    {
        try
        {
            System.Windows.Forms.Clipboard.SetDataObject(text, copy: true, retryTimes: 20, retryDelay: 100);
        }
        catch (ExternalException)
        {
            MessageBox.Show("クリップボードが他のアプリ (IME のクリップボード監視など) に使用中のため、コピーできませんでした。もう一度お試しください。",
                "VrcLogger", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }

    /// <summary>Finds the DataGrid that owns a context-menu item.</summary>
    public static DataGrid? GridOf(object menuItem) =>
        (menuItem as MenuItem)?.Parent is ContextMenu cm ? cm.PlacementTarget as DataGrid : null;
}
