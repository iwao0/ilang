/// Foundation bindings — umbrella module. Flattens the sibling
/// category files into the `foundation` namespace so callers can
/// `use foundation { NSObject, NSString, NSSet, ... }` without
/// having to remember which file each type lives in.
///
/// The loader topologically sorts these `pub use M.*` entries so
/// declaration order here doesn't matter for cross-module
/// non-pub `use` dependencies — keep them roughly grouped for
/// readability.

pub use core.*
pub use measurement.*
pub use collections.*
pub use datetime.*
pub use network.*
pub use serial.*
pub use system.*
pub use io.*
pub use timers.*
pub use numbers.*
pub use concurrency.*
pub use archive.*
pub use xml.*
pub use strings.*
pub use predicate.*
pub use formatters.*
pub use metadata.*
pub use activity.*
pub use extension.*
pub use xpc.*
pub use applescript.*
pub use notifications.*
pub use keycodes.*
