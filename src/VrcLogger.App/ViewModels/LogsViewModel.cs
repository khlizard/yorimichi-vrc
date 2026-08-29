using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class LogsViewModel(MainViewModel main) : ObservableObject
{
    public ObservableCollection<RawEventHit> Results { get; } = [];

    public string[] Levels { get; } = ["(全て)", "Debug", "Warning"];

    [ObservableProperty]
    private string _searchTerm = "";

    [ObservableProperty]
    private string _selectedLevel = "(全て)";

    partial void OnSelectedLevelChanged(string value) => Search();

    [RelayCommand]
    public void Search()
    {
        Results.Clear();
        if (string.IsNullOrWhiteSpace(SearchTerm))
            return; // full-table dumps are not useful; require a term
        var level = SelectedLevel == "(全て)" ? null : SelectedLevel;
        foreach (var r in main.Query.SearchRawEvents(SearchTerm.Trim(), level, main.DateFrom, main.DateTo))
            Results.Add(r);
    }

    [RelayCommand]
    private void OpenVisit(RawEventHit? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh()
    {
        if (!string.IsNullOrWhiteSpace(SearchTerm))
            Search();
    }
}
