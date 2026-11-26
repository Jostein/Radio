using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Radio;

public partial class App : Application
{
    private Tray? tray;
    private FlyoutWindow? flyout;
    private Player? player;

    public App()
    {
        UnhandledException += (_, args) => Log(args.Exception?.ToString() ?? args.Message);
    }

    internal static void Log(string message)
    {
        try
        {
            File.AppendAllText(Path.Combine(Path.GetTempPath(), "radio-error.txt"), message + "\n");
        }
        catch
        {
        }
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        try
        {
            Launch(args);
        }
        catch (Exception error)
        {
            Log(error.ToString());
            throw;
        }
    }

    private void Launch(LaunchActivatedEventArgs args)
    {
        var queue = DispatcherQueue.GetForCurrentThread();
        var stations = Stations.Load();
        player = new Player(queue);

        flyout = new FlyoutWindow(stations, player);

        tray = new Tray();
        tray.LeftClick = () => flyout.Toggle();
        tray.Install(Quit);
        flyout.Anchor = tray.Bounds;

        if (stations.AskTrayHint())
        {
            // Give the shell a moment to settle before it has to draw something.
            _ = Task.Delay(2000).ContinueWith(_ => tray.Hint(
                "Radio is in the tray overflow",
                "Click here, then turn on Radio in Taskbar settings to keep it next to the clock."));
        }

        // The same hook the Rust port uses on the build worker: open the panel
        // without a hand, so a session with no tray in reach can be looked at.
        if (Environment.GetEnvironmentVariable("RADIO_FLYOUT_DEMO") is { Length: > 0 })
        {
            flyout.OpenForTesting();
        }
    }

    private void Quit()
    {
        tray?.Dispose();
        player?.Stop();
        Exit();
    }
}
