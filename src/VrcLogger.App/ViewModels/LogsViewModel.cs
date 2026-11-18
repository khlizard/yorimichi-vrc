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

    private const int PageSize = 200;

    /// <summary>「続きを読み込む」で 1 ページ目を超えて表示しているか。</summary>
    public bool HasExtraPages => Results.Count > PageSize;
    private string _activeTerm = "";
    private string? _activeLevel;
    private bool _isLoading;

    [ObservableProperty]
    private bool _hasMore;

    [RelayCommand]
    public void Search()
    {
        _activeTerm = SearchTerm.Trim();
        _activeLevel = SelectedLevel == "(全て)" ? null : SelectedLevel;
        Results.Clear();
        HasMore = false;
        LoadPage();
    }

    [RelayCommand]
    private void LoadMore()
    {
        if (!HasMore || _isLoading)
            return;
        LoadPage();
    }

    private void LoadPage()
    {
        _isLoading = true;
        try
        {
            (string Time, long Id)? before = Results.Count > 0 ? (Results[^1].EventTime, Results[^1].Id) : null;
            var page = main.Query.SearchRawEvents(_activeTerm, _activeLevel, main.DateFrom, main.DateTo, before, PageSize);
            foreach (var r in page)
                Results.Add(r);
            HasMore = page.Count == PageSize;
        }
        finally
        {
            _isLoading = false;
        }
    }

    [RelayCommand]
    private void OpenVisit(RawEventHit? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh() => Search();
}
