using System.Text.Json;
using System.Text.Json.Serialization;

namespace Radio;

/// <summary>A station the user saved, in the same JSON shape the macOS app writes.</summary>
internal sealed class Station
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("title")]
    public string Title { get; set; } = string.Empty;

    [JsonPropertyName("urlString")]
    public string UrlString { get; set; } = string.Empty;

    public static Station Create(string title, string url) => new()
    {
        Id = Guid.NewGuid().ToString("N"),
        Title = title,
        UrlString = url,
    };

    /// <summary>The stream to hand the player, or null when the text is not a URL.</summary>
    public Uri? PlayableUrl()
    {
        var cleaned = new string(UrlString.Where(c => !char.IsWhiteSpace(c)).ToArray());
        if (!Uri.TryCreate(cleaned, UriKind.Absolute, out var uri))
        {
            return null;
        }
        return uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps ? uri : null;
    }
}

/// <summary>
/// The station list and the volume, in <c>%APPDATA%\Radio</c>: the folder the
/// macOS app and the Rust port both use, so a list copies over unchanged.
/// </summary>
internal sealed class Stations
{
    private static readonly JsonSerializerOptions Json = new()
    {
        WriteIndented = true,
        Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private readonly string directory;

    public List<Station> Items { get; private set; } = new();

    public double Volume { get; set; } = 1.0;

    private Stations(string directory) => this.directory = directory;

    public static Stations Load()
    {
        var directory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "Radio");
        var stations = new Stations(directory);
        stations.Reload();
        stations.LoadSettings();
        return stations;
    }

    public string StationsFile => Path.Combine(directory, "stations.json");

    private string SettingsFile => Path.Combine(directory, "settings.json");

    private void Reload()
    {
        try
        {
            var items = JsonSerializer.Deserialize<List<Station>>(File.ReadAllText(StationsFile), Json);
            Items = items ?? Starter();
        }
        catch
        {
            // No file yet, or one this version cannot read: start from the sample
            // station rather than an empty panel.
            Items = Starter();
        }
    }

    public void Add(Station station)
    {
        Items.Add(station);
        Save();
    }

    public void Remove(Station station)
    {
        Items.Remove(station);
        Save();
    }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(directory);
            File.WriteAllText(StationsFile, JsonSerializer.Serialize(Items, Json));
        }
        catch
        {
            // Nothing useful to do: the list is still correct in memory.
        }
    }

    public void SaveVolume()
    {
        try
        {
            Directory.CreateDirectory(directory);
            File.WriteAllText(SettingsFile, JsonSerializer.Serialize(new Settings { Volume = Volume }, Json));
        }
        catch
        {
        }
    }

    private void LoadSettings()
    {
        try
        {
            var settings = JsonSerializer.Deserialize<Settings>(File.ReadAllText(SettingsFile), Json);
            if (settings is not null)
            {
                Volume = Math.Clamp(settings.Volume, 0.0, 1.0);
            }
        }
        catch
        {
            Volume = 1.0;
        }
    }

    private static List<Station> Starter() => new()
    {
        Station.Create("FIP Reggae", "https://stream.radiofrance.fr/fipreggae/fipreggae.m3u8"),
    };

    private sealed class Settings
    {
        [JsonPropertyName("volume")]
        public double Volume { get; set; } = 1.0;
    }
}
