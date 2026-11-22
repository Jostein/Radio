# Omarchy

Omarchy is Arch + Hyprland + Waybar. It uses the same Linux binary as GNOME and
KDE; the only Omarchy-specific parts are installation, autostart and the tray.

## Install

Build the recipe from this checkout:

```sh
cd Platforms/packaging/arch
makepkg -si
```

Once the `radio-git` AUR package is published, the one-liner is:

```sh
omarchy pkg add radio-git
```

## Start it with the session

Omarchy 4 reads `~/.config/hypr/autostart.lua`. Add:

```lua
o.launch_on_start("radio")
```

The binary is installed as `/usr/bin/radio`; the package also installs the
launcher entry and icon.

## Tray

Waybar hosts the icon. Omarchy hides tray icons behind the tray expander by
default; change the `group/tray-expander` entry to `tray` in
`~/.config/waybar/config.jsonc` (Setup → Config → Waybar opens it).

## Optional Waybar module

Copy `waybar/radio.sh` to `~/.config/waybar/scripts/radio.sh`, make it
executable, and add a module like this to `~/.config/waybar/config.jsonc`:

```jsonc
"custom/radio": {
  "exec": "~/.config/waybar/scripts/radio.sh",
  "interval": 2,
  "tooltip": false,
  "on-click": "playerctl -p radio play-pause",
  "on-click-right": "radio"
}
```

Add `"custom/radio"` to `modules-right` (or wherever the bar keeps media
controls). The module needs `playerctl`:

```sh
sudo pacman -S playerctl
```

It reads the app's MPRIS service, so the station is the artist line and the
track is the title. With no player running it prints nothing and Waybar hides
the module.
