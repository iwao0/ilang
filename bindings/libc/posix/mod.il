/// POSIX bindings — umbrella.
///
/// Re-exports each POSIX-header file so callers can write
/// `use posix { open, pthread_create, getpwuid, ... }` without
/// having to remember which file each symbol lives in.
///
/// Every entry point inside these files is annotated `@optional`
/// because the matching symbol is missing on Windows ucrt — on
/// such hosts the AOT linker substitutes an abort-stub and the
/// JIT raises an "unresolved symbol" panic only when the fn is
/// actually called.

pub use unistd.*
pub use time.*
pub use string.*
pub use stdlib.*
pub use stdio.*
pub use dirent.*
pub use stat.*
pub use fcntl.*
pub use mman.*
pub use pthread.*
pub use semaphore.*
pub use signal.*
pub use select.*
pub use socket.*
pub use pwd.*
