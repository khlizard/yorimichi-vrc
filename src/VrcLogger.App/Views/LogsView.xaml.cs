using System.Windows.Controls;
using System.Windows.Input;
using VrcLogger.App.ViewModels;
using VrcLogger.Core.Data;

namespace VrcLogger.App.Views;

public partial class LogsView : UserControl
{
    public LogsView() => InitializeComponent();

    private void ResultGrid_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (DataContext is LogsViewModel vm && ResultGrid.SelectedItem is RawEventHit row)
            vm.OpenVisitCommand.Execute(row);
    }
}
