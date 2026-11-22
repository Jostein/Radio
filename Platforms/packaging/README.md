# Packaging and host notes

There is one Linux binary, `radio`. The desktop is a host for its
StatusNotifier/AppIndicator icon and its MPRIS service, not a separate build.

| Folder | What it is for |
|---|---|
| `arch/` | `radio-git` PKGBUILD for Arch and Omarchy |
| `gnome/` | the AppIndicator extension requirement |
| `kde/` | Plasma's native StatusNotifier support; no Qt build |
| `enlightenment/` | Enlightenment's StatusNotifier systray; no separate build |
| `omarchy/` | AUR install, Hyprland autostart, Waybar tray and an optional module |

There is deliberately no Qt, GNOME or Enlightenment fork. Plasma's Qt tray,
GNOME's AppIndicator extension and Enlightenment's Systray all display the same
icon, and all read the same MPRIS service. A native Add Station dialog for each
desktop would be a second UI, not a port; it is not part of this package.

IRIX is not a supported target. It is a different, legacy Unix without the
modern Rust, GTK 3, ALSA, D-Bus and ffmpeg runtime this binary needs; supporting
it would be a separate legacy port, not another build of this app.
