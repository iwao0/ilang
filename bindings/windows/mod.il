/// Umbrella module mirroring what default `windows.h` transitively
/// pulls in. Re-exports the DLL-per-file modules that correspond to
/// the headers windows.h includes (windef, winbase/kernel32, wingdi,
/// winuser, winreg/advapi32, ole2 → ole32 / oleaut32, commdlg,
/// prsht → comctl32, imm, mmsystem, winnetwk, winspool, rpc,
/// shellapi, winsock).
///
/// Modules outside that scope (shlwapi, winhttp, gdiplus, d3d11,
/// dwrite, d2d1, xaudio2, propsys, ...) ship as their own files
/// under `bindings/windows/` and must be imported directly, e.g.
/// `use winhttp { ... }` / `use bcrypt { ... }`.

pub use winnull.*
pub use winnt.*
pub use windef.*
pub use kernel32.*
pub use user32.*
pub use gdi32.*
pub use msimg32.*
pub use shell32.*
pub use comdlg32.*
pub use comctl32.*
pub use winmm.*
pub use advapi32.*
pub use ws2_32.*
pub use ole32.*
pub use oleaut32.*
pub use version.*
pub use imm32.*
pub use rpcrt4.*
pub use mpr.*
pub use winspool.*
