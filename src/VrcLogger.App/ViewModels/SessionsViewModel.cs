using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class SessionsViewModel(MainViewModel main) : ObservableObject
{
    public ObservableCollection<VisitRow> Visits { get; } = [];
    public ObservableCollection<VisitPlayerRow> VisitPlayers { get; } = [];
    public ObservableCollection<UrlHit> VisitUrls { get; } = [];

    [ObservableProperty]
    private string _worldFilter = "";

    [ObservableProperty]
    private VisitRow? _selectedVisit;

    [ObservableProperty]
    private bool _hideLocalPlayer;

    [RelayCommand]
    public void Refresh()
    {
        var keep = SelectedVisit?.Id;
        Visits.Clear();
        foreach (var v in main.Query.GetVisits(WorldFilter, main.DateFrom, main.DateTo))
            Visits.Add(v);
        SelectedVisit = Visits.FirstOrDefault(v => v.Id == keep) ?? Visits.FirstOrDefault();
    }

    private bool _suppressAutoRefresh;

    partial void OnWorldFilterChanged(string value)
    {
        if (!_suppressAutoRefresh)
            Refresh();
    }

    partial void OnSelectedVisitChanged(VisitRow? value) => LoadDetail();

    partial void OnHideLocalPlayerChanged(bool value) => LoadDetail();

    private void LoadDetail()
    {
        VisitPlayers.Clear();
        VisitUrls.Clear();
        if (SelectedVisit is null)
            return;
        foreach (var p in main.Query.GetVisitPlayers(SelectedVisit.Id))
            if (!HideLocalPlayer || !p.IsLocal)
                VisitPlayers.Add(p);
        foreach (var u in main.Query.GetVisitUrls(SelectedVisit.Id))
            VisitUrls.Add(u);
    }

    public void SelectVisitById(long visitId)
    {
        var hit = Visits.FirstOrDefault(v => v.Id == visitId);
        if (hit is null)
        {
            // Not in the current filtered list — clear filters and reload.
            _suppressAutoRefresh = true;
            WorldFilter = "";
            _suppressAutoRefresh = false;
            Visits.Clear();
            foreach (var v in main.Query.GetVisits(null, null, null))
                Visits.Add(v);
            hit = Visits.FirstOrDefault(v => v.Id == visitId);
        }
        if (hit is not null)
            SelectedVisit = hit;
    }
}
