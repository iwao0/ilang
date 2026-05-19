// Umbrella module for Win32 bindings.
//
// Re-exports every DLL-per-file binding so callers can write
// `use windows { CreateWindowExA, MSG, ... }` instead of naming
// each module. Add new DLLs (gdi32, comctl32, ...) as separate
// files and re-export them here.

pub use kernel32.*
pub use user32.*
