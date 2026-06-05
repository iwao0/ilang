/// Cross-platform GUI library — umbrella module.
///
/// Per-control classes live in sibling files; this file re-
/// exports them so consumer code stays as `use gui { Window,
/// Button, … }`.
///
///   use gui
///   let w = new gui.Window("hello", 480.0, 320.0)
///   w.onClose.add(fn(e: gui.CloseEvent) { console.log("bye") })
///   gui.run()
///
/// Files:
///   event.il       — `EventTarget` + every `*Event` payload type
///   window.il      — `Window` + `Control` base class
///   button.il      — `Button`
///   label.il       — `Label`
///   linklabel.il   — `LinkLabel`
///   textfield.il   — `TextField`
///   searchfield.il — `SearchField`
///   checkbox.il    — `Checkbox`
///   switch.il      — `Switch`
///   segmentedcontrol.il — `SegmentedControl`
///   radio.il       — `RadioButton`
///   slider.il      — `Slider`
///   progressbar.il — `ProgressBar`
///   activityindicator.il — `ActivityIndicator`
///   colorwell.il   — `ColorWell`
///   webview.il — `WebView`
///   combobox.il    — `ComboBox`
///   listbox.il     — `ListBox`
///   groupbox.il    — `GroupBox`
///
/// ilang's `[deps] @target` filter swaps `gui_impl` between
/// `cocoa/` and `win32/` so the public surface above stays
/// platform-independent. User code never imports `gui_impl`.

use gui_impl {
    runEventLoop,
    isOSInDarkMode, installAppearanceWatcher,
}

pub use events.*
pub use font.*
pub use menubar.*
pub use statusbar.*
pub use toolbar.*
pub use layout.*
pub use window.*
pub use box.*
pub use grid.*
pub use button.*
pub use label.*
pub use linklabel.*
pub use textfield.*
pub use searchfield.*
pub use textarea.*
pub use checkbox.*
pub use switch.*
pub use segmentedcontrol.*
pub use radio.*
pub use slider.*
pub use progressbar.*
pub use activityindicator.*
pub use colorwell.*
pub use webview.*
pub use combobox.*
pub use listbox.*
pub use treeview.*
pub use table.*
pub use numericupdown.*
pub use datepicker.*
pub use timepicker.*
pub use groupbox.*
pub use accordion.*
pub use panel.*
pub use canvas.*
pub use imageview.*
pub use scrollview.*
pub use splitview.*
pub use notification.*
pub use trayicon.*
pub use tabview.*
pub use clipboard.*
// `gpuview` is intentionally NOT re-exported. Its `wgpu29`
// dependency pulls in `libwgpu_native.dylib` / `wgpu_native.dll`,
// which fails the JIT's symbol-resolution step at startup unless
// the loader path is configured (DYLD_LIBRARY_PATH / LD_LIBRARY_
// PATH / PATH). Apps that don't need GPU rendering can
// `use gui` alone and stay wgpu-free; apps that do need it
// import `use gui.gpuview { GpuView, RenderEvent }` explicitly
// and accept the dylib requirement.

/// `true` when the OS is currently set to dark mode. Re-read on
/// every call (cheap) — for "did this just flip?" checks register
/// a listener with `onAppearanceChange`.
pub fn isDarkMode(): bool {
    isOSInDarkMode()
}

/// Register `cb` to fire whenever the OS appearance flips
/// between light and dark while the app is running. The callback
/// gets the new dark-mode boolean. AppKit and GTK ネイティブ
/// widgets re-paint themselves on their own; the Win32
/// custom-drawn widgets (Switch, SegmentedControl, …) re-paint
/// through the same mechanism. User code typically wires this up
/// to redraw a `Canvas` or recolour any constants it owns.
///
///     gui.onAppearanceChange(fn(isDark: bool) {
///         console.log("dark =", isDark)
///     })
pub fn onAppearanceChange(cb: fn(bool)) {
    installAppearanceWatcher(cb)
}

/// Run the platform event loop until every window has closed
/// (macOS: `NSApp.run()` until `terminate:`; Windows:
/// `GetMessage` loop until `WM_QUIT`). Blocks the calling thread.
pub fn run() {
    runEventLoop()
}
