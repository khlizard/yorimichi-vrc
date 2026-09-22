using System.Windows.Controls;
using System.Windows.Input;
using VrcLogger.App.ViewModels;
using VrcLogger.Core.Data;

namespace VrcLogger.App.Views;

public partial class AvatarsView : UserControl
{
    public AvatarsView() => InitializeComponent();

    private void ResultGrid_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (DataContext is AvatarsViewModel vm && ResultGrid.SelectedItem is AvatarRow row)
            vm.OpenVisitCommand.Execute(row);
    }
}
