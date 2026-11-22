# Packaging and host notes

There is one Linux binary, `radio`. The desktop is a host for its
StatusNotifier/AppIndicator icon and its MPRIS service, not a separate build.

| Folder | What it is for |
|---|---|
| `arch/` | `radio-git` PKGBUILD for Arch and Omarchy |
| `gnome/` | the AppIndicator extension requirement |
| `kde/` | Plasma's native StatusNotifier support; no Qt build |
| `omarchy/` | AUR install, Hyprland autostart, Waybar tray and an optional module |

There is deliberately no Qt or GNOME fork. Plasma's Qt tray and GNOME's
AppIndicator extension both display the same icon, and both read the same MPRIS
service. A Qt-native Add Station dialog would be a second UI, not a port; it is
not part of this package.
