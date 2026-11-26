using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Radio;

public static class Program
{
    [STAThread]
    private static void Main()
    {
        // A second copy would put a second icon in the tray and fight over the
        // same stations file, so the first one wins.
        using var single = new Mutex(true, @"Local\RadioPlayer", out var first);
        if (!first)
        {
            return;
        }

        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(parameters =>
        {
            var queue = DispatcherQueue.GetForCurrentThread();
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(queue));
            _ = new App();
        });
    }
}
