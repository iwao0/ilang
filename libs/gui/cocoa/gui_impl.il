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

pub use core.*
pub use window.*
pub use button.*
pub use label.*
pub use edit.*
pub use textarea.*
pub use checkbox.*
pub use slider.*
pub use progress.*
pub use combo.*
pub use radio.*
pub use list.*
pub use tree.*
pub use table.*
pub use groupbox.*
pub use menu.*
pub use canvas.*
pub use imageview.*
pub use notification.*
pub use scrollview.*
pub use splitview.*
pub use statusbar.*
pub use toolbar.*
pub use trayicon.*
pub use tabview.*
pub use dialogs.*
pub use clipboard.*
// `gpuview` is intentionally NOT re-exported here. Its `wgpu29`
// dependency pulls in `libwgpu_native.dylib` at JIT-link time,
// which fails unless `DYLD_LIBRARY_PATH` is configured. Keeping
// the export out of the umbrella means `use gui_impl { ... }`
// stays wgpu-free; the public `libs/gui/gpuview.il` imports
// straight from `gui_impl.gpuview { ... }` so callers who do
// `use gui.gpuview` opt into the dylib requirement explicitly.
