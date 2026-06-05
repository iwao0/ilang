/// Light / dark appearance detection.
///
/// Windows exposes the current setting through the registry
/// value `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\
/// Personalize\AppsUseLightTheme` (DWORD; 0 = dark, 1 = light).
/// The shell broadcasts `WM_SETTINGCHANGE` with `lParam` pointing
/// at the wide string `"ImmersiveColorSet"` when the user flips
/// Settings → Personalization → Colors. Each top-level window
/// gets the message; `wnd_proc.il` routes it here so the global
/// callback fires at most once per real edge.

use windows {
    HKEY,
    RegOpenKeyExA, RegQueryValueExA, RegCloseKey,
}
use std.ffi { cstrFromString }

let APPEARANCE_INSTALLED: bool = false
let APPEARANCE_CB: fn(bool) = appearanceNoOp
let LAST_DARK: bool = false
/// Internal repaint listeners — every custom-painted widget
/// (Switch / Segmented / LinkLabel / SearchField / ColorWell)
/// adds an entry here at first-use so a system-wide light/dark
/// flip retones every visible instance in one pass.
pub let APPEARANCE_REPAINT_LISTENERS: fn()[] = []

fn appearanceNoOp(isDark: bool) {}

pub fn registerAppearanceRepaintListener(cb: fn()) {
    APPEARANCE_REPAINT_LISTENERS.push(cb)
}

/// `light` when the OS is in light mode, `dark` otherwise. The
/// custom-painted widgets call this from inside their WM_PAINT
/// handlers, so a single re-poll per repaint is enough — no
/// caching needed on top of the registry call inside
/// `isOSInDarkMode`.
pub fn pickColor(light: u32, dark: u32): u32 {
    if isOSInDarkMode() { dark } else { light }
}

/// HKEY_CURRENT_USER. The numeric value is part of the registry
/// API contract — bindings/windows doesn't surface it as a named
/// constant, so the literal lives here.
const HKCU_HANDLE: u64 = 0x80000001 as u64
const KEY_QUERY_VALUE: u32 = 0x00000001 as u32

@extern(C) {
pub fn isOSInDarkMode(): bool {
    let subKey  = cstrFromString("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
    let valName = cstrFromString("AppsUseLightTheme")
    let hKey: HKEY = 0 as HKEY
    let r = RegOpenKeyExA(
        HKCU_HANDLE as HKEY,
        subKey,
        0 as u32,
        KEY_QUERY_VALUE,
        &hKey
    )
    if r != 0 as i32 { return false }
    let v: u32 = 1 as u32   // default = light
    let size: u32 = 4 as u32
    let _ = RegQueryValueExA(
        hKey,
        valName,
        0 as *u32,
        0 as *u32,
        &v as *void,
        &size
    )
    RegCloseKey(hKey)
    // AppsUseLightTheme = 0 means dark mode.
    v == 0 as u32
}
}

pub fn installAppearanceWatcher(cb: fn(bool)) {
    APPEARANCE_CB = cb
    if APPEARANCE_INSTALLED { return }
    APPEARANCE_INSTALLED = true
    LAST_DARK = isOSInDarkMode()
}

/// Called from `wnd_proc.il` whenever a window receives
/// `WM_SETTINGCHANGE`. Re-reads the registry; if the dark-mode
/// bit moved, dispatches the user callback so the global
/// `onAppearanceChange` signal fires once per real flip.
pub fn notifyAppearanceMaybeChanged() {
    let now = isOSInDarkMode()
    if now != LAST_DARK {
        LAST_DARK = now
        for r in APPEARANCE_REPAINT_LISTENERS {
            r()
        }
        let cb = APPEARANCE_CB
        cb(now)
    }
}
