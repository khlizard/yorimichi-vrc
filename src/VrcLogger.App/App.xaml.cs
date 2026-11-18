using System.Windows;
using VrcLogger.App.Services;

namespace VrcLogger.App;

public partial class App : Application
{
    // Local\ scope: one instance per user session.
    private const string MutexName = @"Local\VrcLogger.SingleInstance";
    private const string ActivateEventName = @"Local\VrcLogger.Activate";

    private Mutex? _mutex;
    private EventWaitHandle? _activateEvent;
    private RegisteredWaitHandle? _activateWait;

    protected override void OnStartup(StartupEventArgs e)
    {
        _mutex = new Mutex(initiallyOwned: true, MutexName, out var createdNew);
        _activateEvent = new EventWaitHandle(false, EventResetMode.AutoReset, ActivateEventName);

        if (!createdNew)
        {
            // Another instance is running (possibly hidden in the tray): ask it to show itself and quit.
            _activateEvent.Set();
            Shutdown();
            return;
        }

        base.OnStartup(e);

        var startInTray = e.Args.Contains(StartupRegistration.TrayArg, StringComparer.OrdinalIgnoreCase);
        try { StartupRegistration.RefreshPathIfEnabled(); } catch { }

        // Keep the app alive on unexpected UI-thread exceptions; show what happened instead of vanishing.
        DispatcherUnhandledException += (_, args) =>
        {
            MessageBox.Show(args.Exception.ToString(), "VrcLogger - エラー", MessageBoxButton.OK, MessageBoxImage.Error);
            args.Handled = true;
        };

        var window = new MainWindow();
        MainWindow = window;

        // Wake up the existing window whenever a second instance signals us.
        _activateWait = ThreadPool.RegisterWaitForSingleObject(
            _activateEvent,
            (_, _) => Dispatcher.BeginInvoke(window.ShowFromTray),
            null, Timeout.Infinite, executeOnlyOnce: false);

        if (!startInTray)
            window.Show();
        _ = window.StartAsync();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _activateWait?.Unregister(null);
        _activateEvent?.Dispose();
        if (_mutex is not null)
        {
            try { _mutex.ReleaseMutex(); } catch (ApplicationException) { /* not owned (second instance) */ }
            _mutex.Dispose();
        }
        base.OnExit(e);
    }
}
