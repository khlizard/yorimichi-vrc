using System.Windows.Controls;
using System.Windows.Input;
using VrcLogger.App.ViewModels;
using VrcLogger.Core.Data;

namespace VrcLogger.App.Views;

public partial class PlayersView : UserControl
{
    public PlayersView() => InitializeComponent();

    private void EncounterGrid_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (DataContext is PlayersViewModel vm && EncounterGrid.SelectedItem is EncounterRow row)
            vm.OpenVisitCommand.Execute(row);
    }

    private void List_ScrollChanged(object sender, ScrollChangedEventArgs e)
    {
        if (DataContext is PlayersViewModel vm)
            InfiniteScroll.OnScrollChanged(e, () => vm.HasMore, () => vm.LoadMoreCommand.Execute(null));
    }
}
