using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class PlayersViewModel(MainViewModel main) : ObservableObject
{
    public ObservableCollection<PlayerHit> Players { get; } = [];
    public ObservableCollection<EncounterRow> Encounters { get; } = [];

    [ObservableProperty]
    private string _searchTerm = "";

    [ObservableProperty]
    private PlayerHit? _selectedPlayer;

    private const int PageSize = 100;

    /// <summary>「続きを読み込む」で 1 ページ目を超えて表示しているか。</summary>
    public bool HasExtraPages => Players.Count > PageSize;
    private string _activeTerm = "";

    [ObservableProperty]
    private bool _hasMore;

    private bool _isLoading;

    [RelayCommand]
    public void Search()
    {
        var previousId = SelectedPlayer?.Id;
        _activeTerm = SearchTerm.Trim();
        Players.Clear();
        Encounters.Clear();
        SelectedPlayer = null;
        HasMore = false;
        LoadPage();
        SelectedPlayer = Players.FirstOrDefault(p => p.Id == previousId) ?? Players.FirstOrDefault();
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
            var page = _activeTerm.Length == 0
                ? main.Query.GetRecentPlayers(main.DateFrom, main.DateTo, Players.Count, PageSize)
                : main.Query.SearchPlayers(_activeTerm, Players.Count, PageSize);
            foreach (var p in page)
                Players.Add(p);
            HasMore = page.Count == PageSize;
        }
        finally
        {
            _isLoading = false;
        }
    }

    partial void OnSelectedPlayerChanged(PlayerHit? value) => LoadEncounters();

    private void LoadEncounters()
    {
        Encounters.Clear();
        if (SelectedPlayer is null)
            return;
        foreach (var e in main.Query.GetEncounters(SelectedPlayer.Id, main.DateFrom, main.DateTo))
            Encounters.Add(e);
    }

    [RelayCommand]
    private void OpenVisit(EncounterRow? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh() => Search();
}
