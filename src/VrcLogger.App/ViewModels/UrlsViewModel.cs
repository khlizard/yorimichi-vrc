using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class UrlsViewModel(MainViewModel main) : ObservableObject
{
    public ObservableCollection<UrlHit> Results { get; } = [];

    public string[] Kinds { get; } = ["(全て)", "video", "string", "image"];

    [ObservableProperty]
    private string _searchTerm = "";

    [ObservableProperty]
    private string _selectedKind = "(全て)";

    partial void OnSelectedKindChanged(string value) => Search();

    [RelayCommand]
    public void Search()
    {
        Results.Clear();
        var kind = SelectedKind == "(全て)" ? null : SelectedKind;
        foreach (var u in main.Query.SearchUrls(SearchTerm.Trim(), kind, main.DateFrom, main.DateTo))
            Results.Add(u);
    }

    [RelayCommand]
    private void OpenVisit(UrlHit? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh() => Search();
}
