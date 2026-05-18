// Foundation bindings — umbrella module. Flattens the sibling
// category files into the `foundation` namespace so callers can
// `use foundation { NSObject, NSString, NSSet, ... }` without
// having to remember which file each type lives in.

pub use core.*
pub use collections.*
pub use datetime.*
pub use network.*
pub use serial.*
pub use system.*
pub use io.*
pub use timers.*
pub use numbers.*
pub use uuid_sort.*
pub use concurrency.*
pub use archive.*
pub use xml.*
pub use strings.*
pub use predicate.*
pub use formatters.*
