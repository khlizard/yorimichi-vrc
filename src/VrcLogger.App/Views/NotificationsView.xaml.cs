using System.Windows.Controls;
using System.Windows.Input;
using VrcLogger.App.ViewModels;
using VrcLogger.Core.Data;

namespace VrcLogger.App.Views;

public partial class NotificationsView : UserControl
{
    public NotificationsView() => InitializeComponent();

    private void ModerationGrid_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (DataContext is NotificationsViewModel vm && ModerationGrid.SelectedItem is ModerationRow row)
            vm.OpenVisitCommand.Execute(row);
    }
}
