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
pub use checkbox.*
pub use radio.*
pub use slider.*
pub use progress.*
pub use combo.*
pub use list.*
pub use groupbox.*
pub use wnd_proc.*
pub use window.*
