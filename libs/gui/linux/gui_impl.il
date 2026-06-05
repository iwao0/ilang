/// Linux (GTK 4) implementation of the gui-core bridge API.
///
/// Same surface as libs/gui/cocoa/gui_impl.il and
/// libs/gui/win32/gui_impl.il. Each widget owns a sibling file.

pub use core.*
pub use dark.*
pub use window.*
pub use button.*
pub use label.*
pub use linklabel.*
pub use edit.*
pub use searchfield.*
pub use textarea.*
pub use checkbox.*
pub use switch.*
pub use segmentedcontrol.*
pub use radio.*
pub use slider.*
pub use progress.*
pub use activityindicator.*
pub use colorwell.*
pub use webview.*
pub use combo.*
pub use list.*
pub use tree.*
pub use table.*
pub use numericupdown.*
pub use datepicker.*
pub use timepicker.*
pub use groupbox.*
pub use accordion.*
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
// dependency pulls in `libwgpu_native.so` at JIT-link time,
// which fails unless `LD_LIBRARY_PATH` is configured. Keeping
// the export out of the umbrella means `use gui_impl { ... }`
// stays wgpu-free; the public `libs/gui/gpuview.il` imports
// straight from `gui_impl.gpuview { ... }` so callers who do
// `use gui.gpuview` opt into the dylib requirement explicitly.
