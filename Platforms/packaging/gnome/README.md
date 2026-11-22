# GNOME

There is no GNOME-specific binary. GNOME Shell does not show tray icons by
default, so it needs the AppIndicator extension to host the app's
StatusNotifier icon.

## Install

On Arch:

```sh
sudo pacman -S gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

On Debian and Ubuntu the package is `gnome-shell-extension-appindicator`; Ubuntu
normally enables its equivalent already. Log out and back in after installing a
new extension, then check that it is enabled in Extensions.

Launch `radio` from the app grid. The package installs `radio.desktop`, so the
entry is already there. The icon appears in the top bar; without the extension
there is no tray host and therefore no menu.

The Add Station dialog is GTK, so it follows the GNOME theme. Media controls
come from MPRIS and appear in GNOME's own media widget.
