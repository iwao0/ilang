/// AppKit bindings — umbrella module. Flattens the sibling
/// category files into the `appkit` namespace so callers can
/// `use appkit { NSWindow, NSButton, ... }` (or reach through
/// the umbrella `cocoa.X`) without remembering which file each
/// type lives in.

pub use core.*
pub use controls.*
pub use layout.*
pub use text.*
pub use image.*
pub use menu.*
pub use panels.*
pub use workspace.*
pub use events.*
pub use table.*
pub use toolbar.*
pub use drawing.*
pub use gestures.*
