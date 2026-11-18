using System.Windows.Controls;
using System.Windows.Threading;

namespace VrcLogger.App.Views;

/// <summary>
/// "Load the next page when the user reaches the bottom" for virtualized lists.
/// ScrollChanged fires many times while a page of new rows is being measured, often with
/// transient offset/extent values that still look like "at the bottom". Reacting to those
/// directly chain-loads every page. Instead, one check is queued per burst and runs after
/// layout has settled, against the ScrollViewer's real state.
/// </summary>
internal static class InfiniteScroll
{
    private const double BottomThreshold = 5;
    private static readonly HashSet<ScrollViewer> Pending = [];

    public static void OnScrollChanged(ScrollChangedEventArgs e, Func<bool> hasMore, Action loadMore)
    {
        if (e.OriginalSource is not ScrollViewer sv || !Pending.Add(sv))
            return;
        sv.Dispatcher.BeginInvoke(DispatcherPriority.ContextIdle, () =>
        {
            Pending.Remove(sv);
            if (sv.ExtentHeight > 0
                && sv.VerticalOffset + sv.ViewportHeight >= sv.ExtentHeight - BottomThreshold
                && hasMore())
                loadMore();
        });
    }
}
