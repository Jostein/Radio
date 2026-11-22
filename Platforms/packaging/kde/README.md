# KDE Plasma / Qt

There is no Qt build to install. Plasma hosts StatusNotifierItem natively, and
the same `radio` binary appears in the system tray. Qt is the host toolkit here,
not an application dependency.

Install `radio` from the AUR or build the checkout, launch it, and enable the
Radio entry in Plasma's tray settings if it is hidden.

The only GTK surface is the Add Station dialog. `gtk3` is already a dependency;
if it looks out of place, set a GTK theme in System Settings → Colors & Themes →
Application Style → Configure GNOME/GTK Application Style.

Media controls come from MPRIS, so Plasma's media widget and the media keys work
without any Qt-specific code.
