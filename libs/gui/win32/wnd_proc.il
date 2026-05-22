/// Shared window procedure for every `IlangGuiWindow`-class HWND.
///
/// On WM_* arrival this routes to the matching `dispatchXxx` /
/// `tryHandleXxx` entry point exposed by each per-widget module.
/// Keeping the dispatcher in its own file means no single module
/// has to import every other widget's state — `window.il` owns
/// the per-window callback bundle, `button.il` owns BN_CLICKED
/// fan-out, etc.
///
/// The Win32 → ilang routing convention:
/// - WindowMessage.close / size / keydown / ... — read the
///   slot index out of `GWLP_USERDATA`, forward to `window`.
/// - WindowMessage.command — try each notify-code dispatcher
///   in turn (button click / checkbox / radio / edit change /
///   combo / list); first match wins.
/// - WindowMessage.hScroll — trackbar position changes only;
///   route to `slider`.

use windows {
    HWND, HINSTANCE,
    DefWindowProcW, PostQuitMessage, DestroyWindow,
    WindowMessage, VirtualKey,
    GetKeyState,
    LoadCursorA, SetCursor, SystemCursor,
    HDC, RECT, GetClientRect, FillRect, GetSysColorBrush,
}
use core {
    dispatchClose, dispatchSize,
    dispatchKeyDown, dispatchKeyUp,
    dispatchMouseMove, dispatchMouseDown, dispatchMouseUp,
}
use button   { tryButtonClick }
use checkbox { tryCheckboxToggle }
use radio    { tryRadioToggle }
use edit     { tryEditChange }
use combo    { tryComboSelect }
use list     { tryListSelect }
use slider   { trySliderChange }

// ─── modifier helpers (GetKeyState high-bit set ⇒ pressed) ─────

fn keyDown(vk: i32): bool {
    (GetKeyState(vk) as i32 & 0x8000) != 0
}
fn modShift(): bool { keyDown(VirtualKey.shift as i32) }
fn modCtrl():  bool { keyDown(VirtualKey.control as i32) }
fn modAlt():   bool { keyDown(VirtualKey.alt as i32) }
fn modMeta():  bool { keyDown(VirtualKey.lWin as i32) || keyDown(VirtualKey.rWin as i32) }

// ─── VirtualKey → W3C KeyboardEvent.code ──────────────────────

fn vkToString(vk: u32): string {
    match vk {
        0x41 { "KeyA" }
        0x42 { "KeyB" }
        0x43 { "KeyC" }
        0x44 { "KeyD" }
        0x45 { "KeyE" }
        0x46 { "KeyF" }
        0x47 { "KeyG" }
        0x48 { "KeyH" }
        0x49 { "KeyI" }
        0x4A { "KeyJ" }
        0x4B { "KeyK" }
        0x4C { "KeyL" }
        0x4D { "KeyM" }
        0x4E { "KeyN" }
        0x4F { "KeyO" }
        0x50 { "KeyP" }
        0x51 { "KeyQ" }
        0x52 { "KeyR" }
        0x53 { "KeyS" }
        0x54 { "KeyT" }
        0x55 { "KeyU" }
        0x56 { "KeyV" }
        0x57 { "KeyW" }
        0x58 { "KeyX" }
        0x59 { "KeyY" }
        0x5A { "KeyZ" }
        0x30 { "Digit0" }
        0x31 { "Digit1" }
        0x32 { "Digit2" }
        0x33 { "Digit3" }
        0x34 { "Digit4" }
        0x35 { "Digit5" }
        0x36 { "Digit6" }
        0x37 { "Digit7" }
        0x38 { "Digit8" }
        0x39 { "Digit9" }
        0x0D { "Enter" }
        0x1B { "Escape" }
        0x08 { "Backspace" }
        0x09 { "Tab" }
        0x20 { "Space" }
        0x25 { "ArrowLeft" }
        0x26 { "ArrowUp" }
        0x27 { "ArrowRight" }
        0x28 { "ArrowDown" }
        _    { "" }
    }
}

@extern(C) {

/// Window procedure shared by every gui-win32 window. Per-instance
/// state is keyed by the window's HWND in `core.WINDOWS`; the
/// `dispatchXxx` helpers do the Map lookup themselves.
pub fn guiWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
    // lParam packs (x, y) for mouse messages and (width, height)
    // for WM_SIZE — same low/high u16 split, pulled here once.
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.close {
            // Fire the user's onClose, then DestroyWindow so
            // WM_DESTROY (below) can PostQuitMessage *after* the
            // window is gone. The old "PostQuitMessage from
            // WM_CLOSE without destroying" path left a live HWND
            // that the OS tore down forcibly at process exit.
            dispatchClose(hwnd)
            let _ = DestroyWindow(hwnd as HWND)
            return 0
        }
        WindowMessage.destroy {
            PostQuitMessage(0)
            return 0
        }
        WindowMessage.eraseBkgnd {
            // Paint the client area with COLOR_BTNFACE ourselves.
            // Relying on WNDCLASSEX.hbrBackground works for the
            // initial WM_PAINT but DWM sometimes skips it during
            // maximise / per-monitor DPI transitions, leaving
            // newly-uncovered regions as the compositor's
            // uninitialised surface (= black).
            let hdc = wParam as i64 as HDC
            let rc = RECT { left: 0, top: 0, right: 0, bottom: 0 }
            let _ = GetClientRect(hwnd as HWND, &rc)
            let brush = GetSysColorBrush(15 as i32)   // COLOR_BTNFACE
            let _ = FillRect(hdc, &rc, brush)
            return 1
        }
        WindowMessage.setCursor {
            // HTCLIENT (1) → force the standard arrow so the
            // resize-border cursor doesn't leak back in.
            let ht = (lParam & 0xFFFF) as i32
            if ht == 1 {
                let arrow = LoadCursorA(
                    0 as HINSTANCE,
                    SystemCursor.arrow as i64 as *const char
                )
                let _ = SetCursor(arrow)
                return 1
            }
        }
        WindowMessage.size {
            dispatchSize(hwnd, lo, hi)
            return 0
        }
        WindowMessage.keydown {
            dispatchKeyDown(hwnd, vkToString(wParam as u32),
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.keyup {
            dispatchKeyUp(hwnd, vkToString(wParam as u32),
                          modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mouseMove {
            dispatchMouseMove(hwnd, lo, hi, 0,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.lbuttondown {
            dispatchMouseDown(hwnd, lo, hi, 0,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.lbuttonup {
            dispatchMouseUp(hwnd, lo, hi, 0,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mButtonDown {
            dispatchMouseDown(hwnd, lo, hi, 1,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.mButtonUp {
            dispatchMouseUp(hwnd, lo, hi, 1,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.rButtonDown {
            dispatchMouseDown(hwnd, lo, hi, 2,
                              modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.rButtonUp {
            dispatchMouseUp(hwnd, lo, hi, 2,
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.command {
            // lParam = control HWND, HIWORD(wParam) = notify code.
            // Route by notify code, resolve the control by HWND
            // equality so each widget's id space stays private.
            let ctrlHwnd = lParam as i64
            let notify = ((wParam >> 16) & 0xFFFF) as i32
            if notify == 0 {
                // BN_CLICKED — button / checkbox / radio share
                // the same notify value; first match wins.
                if tryButtonClick(ctrlHwnd)   { return 0 }
                if tryCheckboxToggle(ctrlHwnd) { return 0 }
                if tryRadioToggle(ctrlHwnd)    { return 0 }
            } elif notify == 0x0300 {
                // EN_CHANGE — edit control contents changed.
                if tryEditChange(ctrlHwnd) { return 0 }
            } elif notify == 1 {
                // CBN_SELCHANGE / LBN_SELCHANGE share the value.
                if tryComboSelect(ctrlHwnd) { return 0 }
                if tryListSelect(ctrlHwnd)  { return 0 }
            }
            return 0
        }
        WindowMessage.hScroll {
            // Trackbars (msctls_trackbar32) post WM_HSCROLL to
            // their parent for every thumb move; lParam is the
            // slider HWND.
            if trySliderChange(lParam as i64) { return 0 }
            return 0
        }
        _ {}
    }
    DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
}

} // @extern(C)
