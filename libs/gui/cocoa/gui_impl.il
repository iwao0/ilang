/// macOS implementation of the `gui-core` bridge API.
///
/// Multi-file package: each widget owns a sibling file under
/// `libs/gui/cocoa/`. This umbrella re-exports the public surface
/// so gui-core stays as a single `use gui_impl { ... }`.
///
/// Public API contract (must stay in sync with `libs/gui/win32`):
///
///   createNativeWindow / setWindowTitle / setWindowSize /
///   showNativeWindow / hideNativeWindow / runEventLoop
///   setOnClose / setOnResize / setOnKeyDown / setOnKeyUp /
///     setOnMouseMove / setOnMouseDown / setOnMouseUp
///   createNative{Button,Label,Edit,Checkbox,Radio,Slider,
///                Progress,Combo,List,GroupBox}
///   + their setters / readers
///
/// The `handle` returned to gui-core is the parallel-array
/// index inside the matching widget file — we can't cast an
/// `NSObject` value to `i64` (the type system refuses it for
/// non-`@handle` classes), so an array slot is the simplest
/// stable per-instance key.

pub use core as _ { * }
pub use window as _ { * }
pub use button as _ { * }
pub use label as _ { * }
pub use edit as _ { * }
pub use checkbox as _ { * }
pub use slider as _ { * }
pub use progress as _ { * }
pub use combo as _ { * }
pub use radio as _ { * }
pub use list as _ { * }
pub use groupbox as _ { * }
