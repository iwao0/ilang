// Foundation bindings — umbrella module. Flattens the sibling
// category files into the `foundation` namespace so callers can
// `use foundation { NSObject, NSString, NSSet, ... }` without
// having to remember which file each type lives in.

pub use core.*
pub use collections.*
