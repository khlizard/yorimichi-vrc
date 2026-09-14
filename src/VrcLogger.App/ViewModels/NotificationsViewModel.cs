using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using VrcLogger.Core.Data;

namespace VrcLogger.App.ViewModels;

public sealed partial class NotificationsViewModel(MainViewModel main) : ObservableObject
{
    private const string AllKinds = "(全て)";
    private static readonly string[] KnownKinds = ["invite", "requestInvite", "friendRequest", "boop", "message"];

    public ObservableCollection<NotificationRow> Results { get; } = [];
    public ObservableCollection<ModerationRow> Moderation { get; } = [];
    public ObservableCollection<string> Kinds { get; } = [AllKinds, .. KnownKinds];

    [ObservableProperty]
    private string _searchTerm = "";

    [ObservableProperty]
    private string _selectedKind = AllKinds;

    partial void OnSelectedKindChanged(string value) => Search();

    [RelayCommand]
    public void Search()
    {
        // Keep the kind list in sync with whatever has actually been logged.
        foreach (var k in main.Query.GetNotificationKinds())
            if (!Kinds.Contains(k))
                Kinds.Add(k);

        var term = SearchTerm.Trim();
        var kind = SelectedKind == AllKinds ? null : SelectedKind;
        Results.Clear();
        foreach (var n in main.Query.SearchNotifications(term, kind, main.DateFrom, main.DateTo))
            Results.Add(n);
        Moderation.Clear();
        foreach (var m in main.Query.SearchModerationEvents(term, main.DateFrom, main.DateTo))
            Moderation.Add(m);
    }

    [RelayCommand]
    private void OpenVisit(ModerationRow? row) => main.NavigateToVisit(row?.VisitId);

    public void Refresh() => Search();
}
