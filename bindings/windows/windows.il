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
// Media Foundation / SSPI / WLAN — a handful of declared exports
// aren't actually surfaced by every host's import library; import
// directly with `use mfplat { ... }` / `use secur32 { ... }` / etc.
//pub use mfplat.*
//pub use mfreadwrite.*
//pub use secur32.*
//pub use wlanapi.*
pub use wintrust.*
pub use wevtapi.*
pub use cabinet.*
pub use wer.*
// fwpuclnt.dll (Windows Filtering Platform) — not on the default
// AOT LIB path; import directly via `use fwpuclnt { ... }`.
//pub use fwpuclnt.*
pub use windowscodecs.*
pub use winspool.*
// mswsock / powrprof / winsta / rasapi32 — some declared symbols
// resolve only via WSAIoctl / runtime lookup, not the static .lib.
// Import the specific surface you need directly.
//pub use mswsock.*
pub use virtdisk.*
pub use taskschd.*
pub use httpapi.*
pub use rstrtmgr.*
//pub use powrprof.*
pub use dxva2.*
pub use sensapi.*
//pub use winsta.*
pub use wbemcli.*
//pub use rasapi32.*
pub use mscms.*
// ntdsapi.dll / bluetoothapis (bthprops.cpl) — some entries
// (DsFreeNameResult / BluetoothAuthenticateDeviceEx /
// BluetoothSelectDevices) aren't exported on every host; import
// directly when needed instead of through the umbrella.
//pub use ntdsapi.*
//pub use bluetoothapis.*
// Specialty DLLs whose import libraries aren't always on the
// default AOT LIB path (some require the Windows Driver Kit /
// Active Directory SDK / etc.). They're parsed and reachable
// through their own module names — e.g. `use mscoree { ... }` —
// but excluded from the umbrella so a plain `use windows { ... }`
// AOT build doesn't fail on hosts without those libs.
//
//   fltlib.lib            (Filter Manager — WDK)
//   wldap32.lib           (LDAP — usually on PATH, but uneven)
//   vssapi.lib            (Volume Shadow Copy)
//   uiautomationcore.lib  (Win10 SDK)
//   cryptui.lib           (cert UI)
//   clfsw32.lib           (Common Log File System)
//   mscoree.lib           (CLR — .NET SDK)
//   directcomposition.lib (DComp)
//   slc.lib               (Software Licensing)
//   mfsensorgroup.lib     (MF sensor)
//   icu.lib               (ICU — Win10 1903+)
//   dbgeng.lib             (Debug engine — Win10 SDK)
//   activeds.lib          (ADSI)
//   t2embed.lib           (font subsetting)
//   eapphost.lib          (EAP)
//   lzexpand.lib          (legacy LZ)
//   wuapi / certenroll    (header-only — no flat exports)
pub use oleacc.*
