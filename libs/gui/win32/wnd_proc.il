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
use menu     { tryMenuClick }
use super.event

// ─── VirtualKey → W3C KeyboardEvent.code ──────────────────────

fn vkToString(vk: VirtualKey): string {
    vkToEnum(vk) as string
}

/// Win32 virtual key codes (`windows.VirtualKey`) → the parent
/// package's `event.KeyCode` (W3C `KeyboardEvent.code`). Unmapped
/// keys fall through to `event.KeyCode.unknown`; the caller then
/// serialises that into the discriminant string for the
/// cross-package callback ABI.
fn vkToEnum(vk: VirtualKey): event.KeyCode {
    match vk {
        VirtualKey.a       { event.KeyCode.keyA }
        VirtualKey.b       { event.KeyCode.keyB }
        VirtualKey.c       { event.KeyCode.keyC }
        VirtualKey.d       { event.KeyCode.keyD }
        VirtualKey.e       { event.KeyCode.keyE }
        VirtualKey.f       { event.KeyCode.keyF }
        VirtualKey.g       { event.KeyCode.keyG }
        VirtualKey.h       { event.KeyCode.keyH }
        VirtualKey.i       { event.KeyCode.keyI }
        VirtualKey.j       { event.KeyCode.keyJ }
        VirtualKey.k       { event.KeyCode.keyK }
        VirtualKey.l       { event.KeyCode.keyL }
        VirtualKey.m       { event.KeyCode.keyM }
        VirtualKey.n       { event.KeyCode.keyN }
        VirtualKey.o       { event.KeyCode.keyO }
        VirtualKey.p       { event.KeyCode.keyP }
        VirtualKey.q       { event.KeyCode.keyQ }
        VirtualKey.r       { event.KeyCode.keyR }
        VirtualKey.s       { event.KeyCode.keyS }
        VirtualKey.t       { event.KeyCode.keyT }
        VirtualKey.u       { event.KeyCode.keyU }
        VirtualKey.v       { event.KeyCode.keyV }
        VirtualKey.w       { event.KeyCode.keyW }
        VirtualKey.x       { event.KeyCode.keyX }
        VirtualKey.y       { event.KeyCode.keyY }
        VirtualKey.z       { event.KeyCode.keyZ }
        VirtualKey.d0      { event.KeyCode.digit0 }
        VirtualKey.d1      { event.KeyCode.digit1 }
        VirtualKey.d2      { event.KeyCode.digit2 }
        VirtualKey.d3      { event.KeyCode.digit3 }
        VirtualKey.d4      { event.KeyCode.digit4 }
        VirtualKey.d5      { event.KeyCode.digit5 }
        VirtualKey.d6      { event.KeyCode.digit6 }
        VirtualKey.d7      { event.KeyCode.digit7 }
        VirtualKey.d8      { event.KeyCode.digit8 }
        VirtualKey.d9      { event.KeyCode.digit9 }
        VirtualKey.enter   { event.KeyCode.enter }
        VirtualKey.escape  { event.KeyCode.escape }
        VirtualKey.back    { event.KeyCode.backspace }
        VirtualKey.tab     { event.KeyCode.tab }
        VirtualKey.space   { event.KeyCode.space }
        VirtualKey.left    { event.KeyCode.arrowLeft }
        VirtualKey.right   { event.KeyCode.arrowRight }
        VirtualKey.down    { event.KeyCode.arrowDown }
        VirtualKey.up      { event.KeyCode.arrowUp }
        _                  { event.KeyCode.unknown }
    }
}

@extern(C) {

// ─── modifier helpers (GetKeyState high-bit set ⇒ pressed) ─────

fn keyDown(vk: i32): bool {
    (GetKeyState(vk) as i32 & 0x8000) != 0
}
fn modShift(): bool { keyDown(VirtualKey.shift as i32) }
fn modCtrl():  bool { keyDown(VirtualKey.control as i32) }
fn modAlt():   bool { keyDown(VirtualKey.alt as i32) }
fn modMeta():  bool { keyDown(VirtualKey.lWin as i32) || keyDown(VirtualKey.rWin as i32) }

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
            DestroyWindow(hwnd as HWND)
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
            GetClientRect(hwnd as HWND, &rc)
            let brush = GetSysColorBrush(15 as i32)   // COLOR_BTNFACE
            FillRect(hdc, &rc, brush)
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
                SetCursor(arrow)
                return 1
            }
        }
        WindowMessage.size {
            dispatchSize(hwnd, lo, hi)
            return 0
        }
        WindowMessage.keydown {
            dispatchKeyDown(hwnd, vkToString(wParam as u32 as VirtualKey),
                            modCtrl(), modShift(), modAlt(), modMeta())
            return 0
        }
        WindowMessage.keyup {
            dispatchKeyUp(hwnd, vkToString(wParam as u32 as VirtualKey),
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
            if notify == 0 && ctrlHwnd == 0 {
                // Menu item: lParam is 0, LOWORD(wParam) is the item id.
                let id = (wParam & 0xFFFF) as i32
                if tryMenuClick(id) { return 0 }
            }
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
