using Microsoft.UI.Dispatching;
using Windows.Media;
using Windows.Media.Core;
using Windows.Media.Playback;

namespace Radio;

/// <summary>
/// Playback through Windows' own media stack: Media Foundation decodes MP3, AAC,
/// Ogg and HLS, so this app needs no decoder beside it, and the command manager
/// puts the station in the Windows 11 media flyout and on the keyboard's media
/// keys.
/// </summary>
internal sealed class Player
{
    private readonly MediaPlayer media = new() { AudioCategory = MediaPlayerAudioCategory.Media };
    private readonly DispatcherQueue queue;

    public Player(DispatcherQueue queue)
    {
        this.queue = queue;

        media.CommandManager.IsEnabled = true;
        media.PlaybackSession.PlaybackStateChanged += (_, _) => Post();
        media.MediaFailed += (_, args) =>
        {
            Error = string.IsNullOrWhiteSpace(args.ErrorMessage)
                ? "The stream stopped unexpectedly."
                : args.ErrorMessage;
            queue.TryEnqueue(Post);
        };
    }

    public event EventHandler? Changed;

    public Station? Current { get; private set; }

    public string? Error { get; private set; }

    public double Volume
    {
        get => media.Volume;
        set => media.Volume = Math.Clamp(value, 0.0, 1.0);
    }

    public bool IsPlaying => media.PlaybackSession.PlaybackState == MediaPlaybackState.Playing;

    public string Status
    {
        get
        {
            if (Error is { } message)
            {
                return message;
            }
            return media.PlaybackSession.PlaybackState switch
            {
                MediaPlaybackState.Opening => "Connecting…",
                MediaPlaybackState.Buffering => "Buffering…",
                MediaPlaybackState.Playing => "Live",
                MediaPlaybackState.Paused => "Paused",
                _ => Current is null ? "Nothing playing" : Current.Title,
            };
        }
    }

    public async Task PlayAsync(Station station)
    {
        Current = station;
        Error = null;
        Post();

        var url = station.PlayableUrl();
        if (url is null)
        {
            Error = "That stream URL is not valid.";
            Post();
            return;
        }

        if (Playlist.IsPlaylist(url))
        {
            var resolved = await Playlist.ResolveAsync(url);
            if (resolved is null)
            {
                Error = "That playlist could not be read.";
                Post();
                return;
            }
            url = resolved;
        }

        media.Source = MediaSource.CreateFromUri(url);

        // The Windows 11 media flyout names the station, not the socket.
        var updater = media.SystemMediaTransportControls.DisplayUpdater;
        updater.Type = MediaPlaybackType.Music;
        updater.MusicProperties.Title = station.Title;
        updater.Update();

        media.Play();
        Post();
    }

    public void Toggle()
    {
        if (IsPlaying)
        {
            media.Pause();
        }
        else if (media.Source is not null)
        {
            media.Play();
        }
        else if (Current is not null)
        {
            _ = PlayAsync(Current);
        }
        Post();
    }

    public void Stop()
    {
        media.Pause();
        media.Source = null;
        Current = null;
        Post();
    }

    private void Post() => queue.TryEnqueue(() => Changed?.Invoke(this, EventArgs.Empty));
}
