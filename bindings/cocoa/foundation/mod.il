/// Foundation bindings — umbrella module. Flattens the sibling
/// category files into the `foundation` namespace so callers can
/// `use foundation { NSObject, NSString, NSSet, ... }` without
/// having to remember which file each type lives in.
///
/// `pub use measurement.*` is listed BEFORE `pub use formatters.*`
/// because formatters.il pulls measurement in non-publicly via
/// `use measurement { NSMeasurement, NSUnit }`. When the loader
/// processes that non-pub import first, the subsequent
/// `pub use measurement.*` short-circuits and never re-exports
/// measurement's classes — known loader quirk. Keep measurement
/// ahead of formatters until that's untangled.

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
pub use keycodes.*
