/// Umbrella module for Win32 bindings.
///
/// Re-exports every DLL-per-file binding so callers can write
/// `use windows { CreateWindowExA, MSG, ... }` instead of naming
/// each module.  Add new DLLs as separate files and re-export here.

pub use winnull.*
pub use kernel32.*
pub use user32.*
pub use gdi32.*
pub use shell32.*
pub use comdlg32.*
pub use dwmapi.*
pub use winmm.*
pub use wasapi.*
pub use wasapi_keys.*
pub use xaudio2.*
pub use advapi32.*
pub use ws2_32.*
pub use shlwapi.*
pub use ole32.*
pub use msimg32.*
pub use oleaut32.*
pub use wininet.*
pub use psapi.*
pub use version.*
// xinput is not auto-re-exported: linking against `xinput.lib` requires
// the Windows 10 SDK component, which is not present on every machine.
// Import it explicitly with `use xinput1_4 { ... }` if needed.
pub use iphlpapi.*
pub use crypt32.*
pub use dbghelp.*
pub use winhttp.*
pub use imm32.*
pub use shcore.*
pub use uxtheme.*
pub use gdiplus.*
pub use opengl32.*
pub use bcrypt.*
pub use netapi32.*
pub use userenv.*
pub use dinput8.*
pub use setupapi.*
pub use rpcrt4.*
pub use wtsapi32.*
pub use pdh.*
pub use avrt.*
pub use mpr.*
pub use comctl32.*
pub use d3d11.*
pub use dwrite.*
pub use d2d1.*
pub use msi.*
pub use dnsapi.*
pub use credui.*
pub use urlmon.*
pub use propsys.*
pub use mfplat.*
pub use mfreadwrite.*
pub use secur32.*
pub use wlanapi.*
pub use wintrust.*
pub use wevtapi.*
pub use cabinet.*
pub use wer.*
pub use fwpuclnt.*
pub use windowscodecs.*
pub use winspool.*
pub use mswsock.*
pub use virtdisk.*
pub use taskschd.*
