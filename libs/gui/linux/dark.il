/// Light / dark appearance detection.
///
/// Stubbed: properly reading GTK's `gtk-application-prefer-dark-theme`
/// or libadwaita's `AdwStyleManager.color-scheme` requires the
/// `GValue` accessor surface (`g_value_init`, `g_value_get_*`)
/// — neither lives in bindings/gtk4 yet, and libadwaita has no
/// binding at all in this tree. GTK already retones every native
/// widget when the OS theme flips, so the visual appearance
/// follows automatically; only `Canvas` and user-owned colour
/// constants need this signal to react. Wire it up properly in a
/// follow-up.

pub fn isOSInDarkMode(): bool { false }

pub fn installAppearanceWatcher(cb: fn(bool)) {}
