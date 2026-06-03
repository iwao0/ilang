/// Windows implementation of the `gui-core` bridge API.
///
/// Multi-file package: each widget owns a sibling file under
/// `libs/gui/win32/`. This umbrella re-exports the public surface
/// so gui-core stays as a single `use gui_impl { ... }`.

// Widget modules first — they need to land in the umbrella's
// effective prefix before `wnd_proc.il` (which has selective
// imports of their `tryXxx` helpers) gets a chance to merge them
// under the bare module name. The loader's "first-apply wins"
// dedup means the prefix chosen here sticks.

pub use util.*
pub use core.*
pub use button.*
pub use label.*
pub use edit.*
pub use textarea.*
pub use checkbox.*
pub use radio.*
pub use slider.*
pub use progress.*
pub use combo.*
pub use list.*
pub use groupbox.*
pub use menu.*
pub use canvas.*
pub use imageview.*
pub use tabview.*
pub use dialogs.*
pub use clipboard.*
// `gpuview` is intentionally NOT re-exported here. Its `wgpu29`
// dependency pulls in `wgpu_native.dll` at JIT-link time, which
// fails unless the dll is on `PATH` (or next to the binary).
// Keeping the export out of the umbrella means `use gui_impl
// { ... }` stays wgpu-free; the public `libs/gui/gpuview.il`
// imports straight from `gui_impl.gpuview { ... }` so callers
// who do `use gui.gpuview` opt into the dll requirement
// explicitly.
pub use wnd_proc.*
pub use window.*
