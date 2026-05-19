// Umbrella module for Win32 bindings.
//
// Re-exports every DLL-per-file binding so callers can write
// `use windows { CreateWindowExA, MSG, ... }` instead of naming
// each module.  Add new DLLs as separate files and re-export here.

pub use kernel32.*
pub use user32.*
pub use gdi32.*
pub use shell32.*
pub use comdlg32.*
pub use dwmapi.*
pub use winmm.*
