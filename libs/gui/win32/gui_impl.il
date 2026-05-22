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

pub use util as _ { * }
pub use core as _ { * }
pub use button as _ { * }
pub use label as _ { * }
pub use edit as _ { * }
pub use checkbox as _ { * }
pub use radio as _ { * }
pub use slider as _ { * }
pub use progress as _ { * }
pub use combo as _ { * }
pub use list as _ { * }
pub use groupbox as _ { * }
pub use wnd_proc as _ { * }
pub use window as _ { * }
