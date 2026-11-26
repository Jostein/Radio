using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;

namespace Radio;

/// <summary>
/// The panel that drops from the tray icon: what is playing, the stations, the
/// volume and Add Station. Same content as the macOS menu, drawn with WinUI 3
/// controls so it follows the system theme and accent colour by itself.
/// </summary>
internal sealed class FlyoutWindow
{
    private const int Width = 340;
    private const int TrayInset = 12;
    private const int VisibleRows = 7;
    private const int RowHeight = 44;
    /// <summary>Header, volume row, button and the padding around them.</summary>
    private const int Chrome = 176;

    private static readonly string PlayGlyph = char.ConvertFromUtf32(0xE768);
    private static readonly string PauseGlyph = char.ConvertFromUtf32(0xE769);
    private static readonly string SpeakerGlyph = char.ConvertFromUtf32(0xE767);
    private static readonly string VolumeGlyph = char.ConvertFromUtf32(0xE767);

    private readonly Window window = new() { Title = "Radio" };
    private readonly Grid root = new() { Padding = new Thickness(16, 14, 16, 14), RowSpacing = 12 };
    private readonly TextBlock stationName = new() { FontSize = 17, TextTrimming = TextTrimming.CharacterEllipsis };
    private readonly TextBlock status = new()
    {
        FontSize = 12,
        Opacity = 0.65,
        TextTrimming = TextTrimming.CharacterEllipsis,
    };
    private readonly FontIcon playIcon = new() { FontSize = 14 };
    private readonly ListView list = new();
    private readonly Dictionary<string, FontIcon> markers = new();
    private readonly Stations stations;
    private readonly Player player;
    private DateTime hiddenAt = DateTime.MinValue;
    private DateTime shownAt = DateTime.MinValue;
    /// <summary>Set by the demo hook, where nothing has focus to keep the panel up.</summary>
    private bool pinned;

    /// <summary>Where the tray icon is, so the panel can drop from it.</summary>
    public Func<RectInt32?>? Anchor { get; set; }

    public FlyoutWindow(Stations stations, Player player)
    {
        this.stations = stations;
        this.player = player;
        player.Changed += (_, _) => Refresh();

        ConfigureWindow();
        window.Content = BuildContent();
        window.Activated += (_, args) =>
        {
            // A window that is activated in the same instant it is shown can be
            // told it was deactivated before it ever drew; only a real click
            // elsewhere should close the panel.
            if (args.WindowActivationState == WindowActivationState.Deactivated
                && !pinned
                && (DateTime.UtcNow - shownAt).TotalMilliseconds > 400)
            {
                Hide();
            }
        };

        Refresh();
    }

    public void Toggle()
    {
        // Clicking the icon while the panel is up deactivates it first, so that
        // click has already closed it; showing it again would make it flicker.
        if ((DateTime.UtcNow - hiddenAt).TotalMilliseconds < 250)
        {
            return;
        }

        if (window.AppWindow.IsVisible)
        {
            Hide();
        }
        else
        {
            Show();
        }
    }

    internal void OpenForTesting()
    {
        pinned = true;
        Show();
    }

    private void Show()
    {
        Place();
        shownAt = DateTime.UtcNow;
        window.AppWindow.Show();
        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(window));
        window.Activate();
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr window);

    private void Hide()
    {
        if (!window.AppWindow.IsVisible)
        {
            return;
        }

        hiddenAt = DateTime.UtcNow;
        stations.SaveVolume();
        window.AppWindow.Hide();
    }

    private void Place()
    {
        // The click that opened the panel came from the tray, so the cursor is
        // on the monitor whose tray icon this is — which is where the panel
        // belongs, even when the taskbar is not on the primary display.
        var area = WorkAreaAtCursor();
        var rows = Math.Max(1, Math.Min(stations.Items.Count, VisibleRows));

        // MoveAndResize speaks pixels and the layout speaks points, so the window
        // is scaled by its own DPI or a 340-point panel is 170 points on a 200%
        // display.
        var scale = Dpi() / 96.0;
        var width = (int)Math.Round(Width * scale);
        var height = (int)Math.Round((Chrome + rows * RowHeight) * scale);
        var inset = (int)Math.Round(TrayInset * scale);

        var x = area.X + area.Width - width - inset;
        var y = area.Y + area.Height - height - inset;

        // Sit over the icon itself, the way the volume and battery flyouts do,
        // rather than in whichever corner the tray happens to share.
        if (Anchor?.Invoke() is { } icon)
        {
            x = Math.Min(icon.X + icon.Width - width, area.X + area.Width - width);
            y = Math.Max(icon.Y - height - (int)Math.Round(8 * scale), area.Y);
        }

        window.AppWindow.MoveAndResize(new RectInt32(x, y, width, height));
    }

    private static RectInt32 WorkAreaAtCursor()
    {
        GetCursorPos(out var point);
        var display = DisplayArea.GetFromPoint(
            new PointInt32(point.X, point.Y),
            DisplayAreaFallback.Primary);
        return display.WorkArea;
    }

    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT point);

    private double Dpi()
    {
        var dpi = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(window));
        return dpi == 0 ? 96 : dpi;
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(IntPtr window);

    private void ConfigureWindow()
    {
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
            presenter.IsAlwaysOnTop = true;
        }
        window.AppWindow.IsShownInSwitchers = false;
        window.SystemBackdrop = new MicaBackdrop();

        // A borderless window gets square corners from DWM unless asked.
        var round = 2;
        DwmSetWindowAttribute(
            WinRT.Interop.WindowNative.GetWindowHandle(window),
            DwmWindowCornerPreference,
            ref round,
            4);
    }

    private const int DwmWindowCornerPreference = 33;

    [System.Runtime.InteropServices.DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr window, int attribute, ref int value, int size);

    private UIElement BuildContent()
    {
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var header = new Grid { ColumnSpacing = 8 };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        var headings = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        headings.Children.Add(stationName);
        headings.Children.Add(status);
        Grid.SetColumn(headings, 0);
        header.Children.Add(headings);

        var play = new Button
        {
            Content = playIcon,
            Width = 40,
            Height = 40,
            CornerRadius = new CornerRadius(20),
            Style = Application.Current.Resources["AccentButtonStyle"] as Style,
            VerticalAlignment = VerticalAlignment.Center,
        };
        play.Click += (_, _) => player.Toggle();
        Grid.SetColumn(play, 1);
        header.Children.Add(play);
        Grid.SetRow(header, 0);
        root.Children.Add(header);

        list.IsItemClickEnabled = true;
        list.SelectionMode = ListViewSelectionMode.None;
        list.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        list.ItemClick += (_, args) =>
        {
            // The container is the ListViewItem, but the click reports the row
            // inside it, so the station rides on the row.
            if (args.ClickedItem is FrameworkElement { Tag: Station station })
            {
                _ = player.PlayAsync(station);
            }
        };
        foreach (var station in stations.Items)
        {
            list.Items.Add(Row(station));
        }
        Grid.SetRow(list, 1);
        root.Children.Add(list);

        var volume = new Grid { ColumnSpacing = 10 };
        volume.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        volume.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var volumeIcon = new FontIcon { Glyph = VolumeGlyph, FontSize = 14, Opacity = 0.8 };
        Grid.SetColumn(volumeIcon, 0);
        volume.Children.Add(volumeIcon);

        var slider = new Slider
        {
            Minimum = 0,
            Maximum = 1,
            StepFrequency = 0.01,
            Value = stations.Volume,
            VerticalAlignment = VerticalAlignment.Center,
        };
        slider.ValueChanged += (_, args) =>
        {
            player.Volume = args.NewValue;
            stations.Volume = args.NewValue;
        };
        Grid.SetColumn(slider, 1);
        volume.Children.Add(slider);
        Grid.SetRow(volume, 2);
        root.Children.Add(volume);

        var add = new Button
        {
            Content = "Add Station",
            HorizontalAlignment = HorizontalAlignment.Stretch,
            Style = Application.Current.Resources["AccentButtonStyle"] as Style,
        };
        add.Click += (_, _) => new AddStationDialog(Add).Show();
        Grid.SetRow(add, 3);
        root.Children.Add(add);

        return root;
    }

    private ListViewItem Row(Station station)
    {
        var marker = new FontIcon
        {
            FontSize = 12,
            Width = 16,
            Opacity = 0.8,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var name = new TextBlock
        {
            Text = station.Title,
            FontSize = 14,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var row = new Grid { ColumnSpacing = 2, Tag = station };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(marker, 0);
        Grid.SetColumn(name, 1);
        row.Children.Add(marker);
        row.Children.Add(name);

        markers[station.Id] = marker;
        return new ListViewItem { Content = row, Tag = station, Padding = new Thickness(8, 6, 8, 6) };
    }

    private void Add(Station station)
    {
        list.Items.Add(Row(station));
        _ = player.PlayAsync(station);
        Refresh();
    }

    private void Refresh()
    {
        stationName.Text = player.Current?.Title ?? "Radio";
        status.Text = player.Status;
        playIcon.Glyph = player.IsPlaying ? PauseGlyph : PlayGlyph;

        foreach (var (id, marker) in markers)
        {
            marker.Glyph = player.IsPlaying && player.Current?.Id == id ? SpeakerGlyph : string.Empty;
        }
    }
}
