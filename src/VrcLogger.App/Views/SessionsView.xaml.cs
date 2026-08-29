using System.Diagnostics;
using System.Windows.Controls;
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
}
