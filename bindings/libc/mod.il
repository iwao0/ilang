/// Umbrella for the C standard library bindings.
///
/// `use libc { memcpy, malloc, printf, ... }` is the common entry;
/// reach for the per-header modules directly
/// (`use libc.string { memcpy }`) when an importer only wants a
/// narrow slice.
///
/// All symbols below are ANSI/ISO C — present on Linux, macOS, and
/// Windows (msvcrt / ucrtbase) without an extra package. POSIX-only
/// or platform-specific routines live in the sibling `posix`
/// module (`use libc.posix { strdup, clock_gettime, ... }`) and
/// carry `@optional` so absent-symbol AOT links don't break.

pub use string.*
pub use stdlib.*
pub use stdio.*
pub use cmath.*
pub use ctype.*
pub use ctime.*
pub use errno.*
pub use locale.*
pub use signal.*
pub use setjmp.*
pub use wchar.*
pub use wctype.*
pub use assert.*
pub use fenv.*
pub use inttypes.*
pub use threads.*
pub use uchar.*
pub use posix.*
