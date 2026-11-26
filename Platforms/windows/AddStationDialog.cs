using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using Windows.System;

namespace Radio;

/// <summary>
/// Add Station in its own window. A panel 340 points wide cannot hold a
/// ContentDialog, and the macOS app asks for the same two fields in a sheet, so
/// this is the closest thing Windows has: a small, fixed-size window with the
/// system caption.
/// </summary>
internal sealed class AddStationDialog
{
    private const int Width = 460;
    private const int Height = 320;

    private readonly Window window = new() { Title = "Add Station" };
    private readonly TextBox url = new() { Header = "Stream URL", PlaceholderText = "https://…" };
    private readonly TextBox title = new() { Header = "Title" };
    private readonly TextBlock note = new()
    {
        Text = "The title fills in when the station announces one.",
        FontSize = 12,
        Opacity = 0.7,
        TextWrapping = TextWrapping.Wrap,
    };
    private readonly Action<Station> added;

    public AddStationDialog(Action<Station> added)
    {
        this.added = added;
        window.Content = Build();
    }

    public void Show()
    {
        var area = DisplayArea.Primary.WorkArea;
        window.AppWindow.MoveAndResize(new RectInt32(
            area.X + (area.Width - Width) / 2,
            area.Y + (area.Height - Height) / 2,
            Width,
            Height));
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
        }
        window.Activate();
        url.Focus(FocusState.Programmatic);
    }

    private UIElement Build()
    {
        var root = new Grid { Padding = new Thickness(24, 20, 24, 20), RowSpacing = 12 };
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        root.Children.Add(url);
        Grid.SetRow(title, 1);
        root.Children.Add(title);
        Grid.SetRow(note, 2);
        root.Children.Add(note);

        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        var cancel = new Button { Content = "Cancel" };
        cancel.Click += (_, _) => window.Close();
        var add = new Button
        {
            Content = "Add",
            Style = Application.Current.Resources["AccentButtonStyle"] as Style,
        };
        add.Click += (_, _) => Commit();
        buttons.Children.Add(cancel);
        buttons.Children.Add(add);
        Grid.SetRow(buttons, 4);
        root.Children.Add(buttons);

        url.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                Commit();
            }
        };
        title.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                Commit();
            }
        };

        return root;
    }

    private void Commit()
    {
        var address = url.Text.Trim();
        var name = title.Text.Trim();

        if (name.Length == 0)
        {
            note.Text = "A station needs a title.";
            return;
        }
        if (!Uri.TryCreate(address, UriKind.Absolute, out var uri)
            || (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps))
        {
            note.Text = "That is not an http(s) stream URL.";
            return;
        }

        added(Station.Create(name, address));
        window.Close();
    }
}
