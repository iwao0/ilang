/// Umbrella for the C standard library bindings.
///
/// `use libc { memcpy, malloc, ... }` brings symbols from any of
/// the bundled headers into scope. Bigger projects can target the
/// individual modules directly (`use libc.string { memcpy }`).
///
/// All symbols are ANSI/ISO C — present on Linux, macOS, and
/// Windows (msvcrt / ucrtbase) without an extra package. POSIX-only
/// or platform-specific routines (posix_memalign, clock_gettime,
/// strdup, ...) belong in a sibling `posix.il` that's `@optional`-
/// gated; not added yet.

pub use string.*
pub use stdlib.*
