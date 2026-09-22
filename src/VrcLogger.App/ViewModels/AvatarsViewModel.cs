using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class AvatarsViewModel(MainViewModel main) : ObservableObject
{
    public ObservableCollection<AvatarRow> Results { get; } = [];

    [ObservableProperty]
    private string _searchTerm = "";

    [RelayCommand]
    public void Search()
    {
        Results.Clear();
        foreach (var a in main.Query.SearchAvatars(SearchTerm.Trim(), main.DateFrom, main.DateTo))
            Results.Add(a);
    }

    [RelayCommand]
    private void OpenVisit(AvatarRow? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh() => Search();
}
