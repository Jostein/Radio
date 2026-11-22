# Enlightenment

There is no Enlightenment-specific binary. Enlightenment's Systray speaks the
StatusNotifierItem protocol, which is the same protocol the Linux `radio` binary
uses through libayatana-appindicator. The same `radio` package therefore puts
its icon in an Enlightenment shelf; MPRIS is available on the session bus for
Enlightenment's media controls.

## Run it

Install the package as usual, then start `radio`. For login:

1. Open **Settings → Apps → Startup Applications**.
2. Add a new entry with the command `radio`, or use the installed
   `radio.desktop` launcher.

If the icon does not appear, check that the shelf has a **Systray** gadget and
that Enlightenment was started after the indicator library was installed.

This host has not been exercised in the test VM yet; KDE Plasma, GNOME (with the
AppIndicator extension), Waybar and the headless Waybar demo are the hosts that
have. The protocol is the same, so a failure here is a bug worth reporting, not
a reason to build a second app.
