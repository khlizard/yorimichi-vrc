using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Navigation;

namespace VrcLogger.App.Views;

public partial class SessionsView : UserControl
{
    public SessionsView() => InitializeComponent();

    private void Hyperlink_RequestNavigate(object sender, RequestNavigateEventArgs e)
    {
        Process.Start(new ProcessStartInfo(e.Uri.AbsoluteUri) { UseShellExecute = true });
        e.Handled = true;
    }

    private void UrlGrid_PreviewKeyDown(object sender, KeyEventArgs e) => UrlGridClipboard.OnPreviewKeyDown(sender, e);

    private void CopyUrl_Click(object sender, RoutedEventArgs e)
    {
        if (UrlGridClipboard.GridOf(sender) is { } grid)
            UrlGridClipboard.CopyUrls(grid);
    }

    private void CopyRow_Click(object sender, RoutedEventArgs e)
    {
        if (UrlGridClipboard.GridOf(sender) is { } grid)
            UrlGridClipboard.CopyRows(grid);
    }
}
