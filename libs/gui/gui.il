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
///   textfield.il   — `TextField`
///   checkbox.il    — `Checkbox`
///   radio.il       — `RadioButton`
///   slider.il      — `Slider`
///   progressbar.il — `ProgressBar`
///   combobox.il    — `ComboBox`
///   listbox.il     — `ListBox`
///   groupbox.il    — `GroupBox`
///
/// ilang's `[deps] @target` filter swaps `gui_impl` between
/// `cocoa/` and `win32/` so the public surface above stays
/// platform-independent. User code never imports `gui_impl`.

use gui_impl { runEventLoop }

pub use event as _ { * }
pub use window as _ { * }
pub use button as _ { * }
pub use label as _ { * }
pub use textfield as _ { * }
pub use checkbox as _ { * }
pub use radio as _ { * }
pub use slider as _ { * }
pub use progressbar as _ { * }
pub use combobox as _ { * }
pub use listbox as _ { * }
pub use groupbox as _ { * }

/// Run the platform event loop until every window has closed
/// (macOS: `NSApp.run()` until `terminate:`; Windows:
/// `GetMessage` loop until `WM_QUIT`). Blocks the calling thread.
pub fn run() {
    runEventLoop()
}
