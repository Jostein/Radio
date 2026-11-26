namespace Radio;

/// <summary>
/// The two playlist files a radio directory hands out. HLS (<c>.m3u8</c>) is not
/// one of them: Media Foundation plays a master playlist itself and picks the
/// best rendition, which is exactly what the macOS app wants from it.
/// </summary>
internal static class Playlist
{
    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(10) };

    public static bool IsPlaylist(Uri uri)
    {
        var path = uri.AbsolutePath.ToLowerInvariant();
        return path.EndsWith(".pls") || path.EndsWith(".m3u");
    }

    public static async Task<Uri?> ResolveAsync(Uri playlist)
    {
        try
        {
            var text = await Http.GetStringAsync(playlist);
            return FirstEntry(text, playlist);
        }
        catch
        {
            return null;
        }
    }

    /// <summary>PLS writes <c>File1=…</c>; M3U is a list of addresses.</summary>
    private static Uri? FirstEntry(string text, Uri playlist)
    {
        foreach (var raw in text.Split('\n'))
        {
            var line = raw.Trim();
            if (line.Length == 0 || line.StartsWith('#'))
            {
                continue;
            }

            var entry = line.StartsWith("File", StringComparison.OrdinalIgnoreCase)
                ? line[(line.IndexOf('=') + 1)..].Trim()
                : line;

            if (!Uri.TryCreate(playlist, entry, out var resolved))
            {
                continue;
            }
            if (resolved.Scheme == Uri.UriSchemeHttp || resolved.Scheme == Uri.UriSchemeHttps)
            {
                return resolved;
            }
        }
        return null;
    }
}
