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

    [RelayCommand]
    public void Search()
    {
        Players.Clear();
        Encounters.Clear();
        if (string.IsNullOrWhiteSpace(SearchTerm))
            return;
        foreach (var p in main.Query.SearchPlayers(SearchTerm.Trim()))
            Players.Add(p);
        SelectedPlayer = Players.FirstOrDefault();
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

    public void Refresh()
    {
        if (!string.IsNullOrWhiteSpace(SearchTerm))
            LoadEncounters();
    }
}
